//! The slope limiter (`land/slope.py`): makes every merged cell representable in `VHGT`.
//!
//! `VHGT` stores the step between neighbouring vertices in a signed byte, so a merge that
//! puts two vertices more than 127 stored units apart cannot be written as it is. Each too
//! steep step is shared out between its two ends (and every cell sharing that vertex), pass
//! after pass, until none is left; a vertex shared with terrain outside the merge, or with a
//! cell borrowed only to reconcile a border, is never moved.

use std::collections::{BTreeMap, BTreeSet};

use crate::curvature::curvature_at;
use crate::grid::{HEIGHT_SCALE, LAND_SIZE};
use crate::grid::Value;

pub type Coords = (i64, i64);

/// The largest step `VHGT` stores, in world units.
pub const MAX_STEP: Value = 127 * HEIGHT_SCALE;
/// Kept back from the limit, so rounding cannot push a fixed step over again.
const SAFETY: Value = HEIGHT_SCALE;
const STRUCTURE_WEIGHT: f64 = 12.0;
pub const MAX_PASSES: usize = 24;
const LAST: usize = LAND_SIZE - 1;

/// What the limiter did (`SlopeReport`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlopeReport {
    pub adjusted: usize,
    pub pinned: usize,
    pub worst_excess: Value,
    pub passes: usize,
    pub converged: bool,
    pub cells_touched: BTreeSet<Coords>,
}

impl Default for SlopeReport {
    fn default() -> Self {
        SlopeReport { adjusted: 0, pinned: 0, worst_excess: 0, passes: 0, converged: true, cells_touched: BTreeSet::new() }
    }
}

/// Every other cell sharing a vertex (`_twins`).
fn twins(coords: Coords, x: usize, y: usize) -> Vec<(Coords, usize, usize)> {
    let (cx, cy) = coords;
    let mut xs = vec![(cx, x)];
    let mut ys = vec![(cy, y)];
    if x == 0 {
        xs.push((cx - 1, LAST));
    } else if x == LAST {
        xs.push((cx + 1, 0));
    }
    if y == 0 {
        ys.push((cy - 1, LAST));
    } else if y == LAST {
        ys.push((cy + 1, 0));
    }
    let mut out = Vec::new();
    for &(gx, vx) in &xs {
        for &(gy, vy) in &ys {
            if (gx, gy) != coords {
                out.push(((gx, gy), vx, vy));
            }
        }
    }
    out
}

fn movable(cells: &BTreeMap<Coords, Vec<Value>>, coords: Coords, x: usize, y: usize, auth: &BTreeSet<Coords>) -> bool {
    if auth.contains(&coords) {
        return false;
    }
    twins(coords, x, y).iter().all(|(t, _, _)| cells.contains_key(t) && !auth.contains(t))
}

fn shift(cells: &mut BTreeMap<Coords, Vec<Value>>, coords: Coords, x: usize, y: usize, delta: Value, r: &mut SlopeReport) {
    if delta == 0 {
        return;
    }
    // As in the Python: `_shift` is called without the authoritative set, so an end found
    // movable by the caller is moved.
    if !movable(cells, coords, x, y, &BTreeSet::new()) {
        r.pinned += 1;
        return;
    }
    if let Some(g) = cells.get_mut(&coords) {
        g[y * LAND_SIZE + x] += delta;
    }
    r.adjusted += 1;
    r.cells_touched.insert(coords);
    for (t, tx, ty) in twins(coords, x, y) {
        if let Some(g) = cells.get_mut(&t) {
            g[ty * LAND_SIZE + tx] += delta;
            r.cells_touched.insert(t);
        }
    }
}

/// The constrained pairs `VHGT` stores a delta between (`_PAIRS`).
fn pairs() -> Vec<((usize, usize), (usize, usize))> {
    let mut p = Vec::with_capacity(LAND_SIZE * LAND_SIZE);
    for y in 0..LAND_SIZE {
        for x in 1..LAND_SIZE {
            p.push(((x - 1, y), (x, y)));
        }
    }
    for y in 1..LAND_SIZE {
        p.push(((0, y - 1), (0, y)));
    }
    p
}

fn structure_map(grid: &[Value]) -> Vec<f64> {
    let rows: Vec<f64> = grid.iter().map(|&v| v as f64).collect();
    let mut out = Vec::with_capacity(grid.len());
    for y in 0..LAND_SIZE {
        for x in 0..LAND_SIZE {
            out.push(curvature_at(&rows, LAND_SIZE, x, y));
        }
    }
    out
}

/// Python's `round`: half to even.
fn round_half_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 { 2.0 * (v / 2.0).round() } else { r }
}

fn split(a: f64, b: f64, excess: Value) -> (Value, Value) {
    let wa = 1.0 / (1.0 + a * STRUCTURE_WEIGHT);
    let wb = 1.0 / (1.0 + b * STRUCTURE_WEIGHT);
    let total = wa + wb;
    if total <= 0.0 {
        let share = excess.div_euclid(2);
        return (share, excess - share);
    }
    let share_a = (round_half_even(excess as f64 * wa / total) as Value).clamp(0, excess);
    (share_a, excess - share_a)
}

/// How many steps exceed what `VHGT` can store (`count_unencodable`).
pub fn count_unencodable<'a>(cells: impl IntoIterator<Item = &'a Vec<Value>>, limit: Value) -> usize {
    let all = pairs();
    cells
        .into_iter()
        .map(|g| {
            all.iter()
                .filter(|&&((ax, ay), (bx, by))| {
                    let step = g[bx + by * LAND_SIZE] - g[ax + ay * LAND_SIZE];
                    step > limit || step < -limit
                })
                .count()
        })
        .sum()
}

/// `limit_slopes`, in place, with the Python's defaults (`limit` 1008, no curvature).
pub fn limit_slopes(cells: &mut BTreeMap<Coords, Vec<Value>>, authoritative: &BTreeSet<Coords>) -> SlopeReport {
    limit_slopes_with(cells, MAX_STEP - SAFETY, false, authoritative)
}

/// `limit_slopes`: every too-steep step shared out until none is left, or `MAX_PASSES`.
pub fn limit_slopes_with(cells: &mut BTreeMap<Coords, Vec<Value>>, limit: Value, use_curvature: bool, authoritative: &BTreeSet<Coords>) -> SlopeReport {
    if cells.is_empty() {
        return SlopeReport::default();
    }
    let (mut r, settled) = limit_slopes_passes(cells, limit, MAX_PASSES, use_curvature, authoritative);
    if !settled {
        r.converged = count_unencodable(cells.values(), MAX_STEP) == 0;
    }
    r
}

/// The passes themselves (`native`'s `limit_slopes`): the report, and whether a pass found
/// nothing left to do.
///
/// Across the cores, with the same answer as one cell after another
/// ([`limit_slopes_passes_in_order`]), bit for bit. A cell reads only its own grid, and
/// its grid is changed only by itself and by the cells it shares a vertex with (a shift
/// moves the twins). So each cell is given a level: one more than the highest level of
/// its neighbours that come before it in the cells' order. The cells of one level share
/// no vertex, so they run at once, each on its own grid, and the moves they make in their
/// neighbours' grids are added when the level is done - by then every neighbour that came
/// earlier has done its work, and none that comes later has started, exactly as in order.
/// Additions of integers in any order give the same sum; the report's counts are sums,
/// its worst a maximum and its cells a set.
pub fn limit_slopes_passes(cells: &mut BTreeMap<Coords, Vec<Value>>, limit: Value, max_passes: usize, use_curvature: bool, authoritative: &BTreeSet<Coords>) -> (SlopeReport, bool) {
    use std::collections::HashMap;
    use std::sync::{Barrier, Mutex};
    let keys: Vec<Coords> = cells.keys().copied().collect();
    let index: HashMap<Coords, usize> = keys.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let structure: Vec<Vec<f64>> = if use_curvature { cells.values().map(|g| structure_map(g)).collect() } else { Vec::new() };
    // The levels, in the cells' order.
    let mut level = vec![0usize; keys.len()];
    let mut levels: Vec<Vec<usize>> = Vec::new();
    for (i, &(x, y)) in keys.iter().enumerate() {
        let mut l = 0;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(&j) = index.get(&(x + dx, y + dy))
                    && j < i
                {
                    l = l.max(level[j] + 1);
                }
            }
        }
        level[i] = l;
        if levels.len() <= l {
            levels.resize_with(l + 1, Vec::new);
        }
        levels[l].push(i);
    }
    // Each vertex's twins as (cell index, vertex index), and whether it may move.
    let present: BTreeSet<Coords> = keys.iter().copied().collect();
    let twins_of = |coords: Coords, x: usize, y: usize| -> Vec<(usize, usize)> {
        twins(coords, x, y).into_iter().filter_map(|(t, tx, ty)| index.get(&t).map(|&j| (j, ty * LAND_SIZE + tx))).collect()
    };
    let may_move = |coords: Coords, x: usize, y: usize, auth: &BTreeSet<Coords>| -> bool {
        if auth.contains(&coords) {
            return false;
        }
        twins(coords, x, y).iter().all(|(t, _, _)| present.contains(t) && !auth.contains(t))
    };
    let none = BTreeSet::new();
    let grids: Vec<Mutex<Vec<Value>>> = keys.iter().map(|c| Mutex::new(std::mem::take(cells.get_mut(c).expect("a key")))).collect();
    /* A cell whose grid has not changed since it was last gone over does what it did then:
       nothing moved (else its grid would have changed), the same steps too steep, the same
       refused. So it is gone over again only when changed (by itself, or by a neighbour's
       move into a shared vertex), and otherwise counts what it counted then - the same
       report, pass for pass (2026-11-10: on a real order every pass went over every cell,
       24 times, for the few steps that touch terrain outside the merge and never settle). */
    let dirty: Vec<std::sync::atomic::AtomicBool> = keys.iter().map(|_| std::sync::atomic::AtomicBool::new(true)).collect();
    let seen: Vec<Mutex<(usize, usize, Value)>> = keys.iter().map(|_| Mutex::new((0, 0, 0))).collect();
    let flat = vec![0.0; LAND_SIZE * LAND_SIZE];
    let all = pairs();
    let widest = levels.iter().map(Vec::len).max().unwrap_or(0);
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(widest.max(1));
    fn lock(m: &Mutex<Vec<Value>>) -> std::sync::MutexGuard<'_, Vec<Value>> {
        m.lock().unwrap_or_else(|p| p.into_inner())
    }
    // One worker's share of one pass: its report and how many steps it found too steep.
    let work = |w: usize, n: usize, barrier: &Barrier| -> (SlopeReport, usize) {
        let mut r = SlopeReport::default();
        let mut excessive = 0usize;
        let mut moves: Vec<(usize, usize, Value)> = Vec::new();
        for lvl in &levels {
            for &i in lvl.iter().skip(w).step_by(n) {
                use std::sync::atomic::Ordering::Relaxed;
                if !dirty[i].swap(false, Relaxed) {
                    let (p, e, worst) = *seen[i].lock().unwrap_or_else(|p| p.into_inner());
                    r.pinned += p;
                    excessive += e;
                    r.worst_excess = r.worst_excess.max(worst);
                    continue;
                }
                let (pinned0, excessive0, adjusted0) = (r.pinned, excessive, r.adjusted);
                let mut worst = 0;
                let coords = keys[i];
                let shape = structure.get(i).unwrap_or(&flat);
                let mut grid = lock(&grids[i]);
                let mut shift = |grid: &mut Vec<Value>, x: usize, y: usize, delta: Value, r: &mut SlopeReport| {
                    if delta == 0 {
                        return;
                    }
                    // As in the Python: `_shift` is called without the authoritative set.
                    if !may_move(coords, x, y, &none) {
                        r.pinned += 1;
                        return;
                    }
                    grid[y * LAND_SIZE + x] += delta;
                    r.adjusted += 1;
                    r.cells_touched.insert(coords);
                    for (j, v) in twins_of(coords, x, y) {
                        moves.push((j, v, delta));
                        r.cells_touched.insert(keys[j]);
                    }
                };
                for &((ax, ay), (bx, by)) in &all {
                    let first = ax + ay * LAND_SIZE;
                    let second = bx + by * LAND_SIZE;
                    let step = grid[second] - grid[first];
                    if -limit <= step && step <= limit {
                        continue;
                    }
                    excessive += 1;
                    let excess = step.abs() - limit;
                    r.worst_excess = r.worst_excess.max(excess);
                    worst = worst.max(excess);
                    let ma = may_move(coords, ax, ay, authoritative);
                    let mb = may_move(coords, bx, by, authoritative);
                    let (share_a, share_b) = if ma && mb {
                        split(shape[first], shape[second], excess)
                    } else if ma {
                        (excess, 0)
                    } else if mb {
                        (0, excess)
                    } else {
                        r.pinned += 1;
                        continue;
                    };
                    let direction = if step > 0 { 1 } else { -1 };
                    shift(&mut grid, ax, ay, direction * share_a, &mut r);
                    shift(&mut grid, bx, by, -direction * share_b, &mut r);
                }
                // What it counted, for a pass that finds it unchanged; moved: changed itself.
                *seen[i].lock().unwrap_or_else(|p| p.into_inner()) = (r.pinned - pinned0, excessive - excessive0, worst);
                if r.adjusted != adjusted0 {
                    dirty[i].store(true, Relaxed);
                }
            }
            barrier.wait();
            for (j, v, d) in moves.drain(..) {
                lock(&grids[j])[v] += d;
                dirty[j].store(true, std::sync::atomic::Ordering::Relaxed);
            }
            barrier.wait();
        }
        (r, excessive)
    };
    let mut r = SlopeReport::default();
    let mut settled = false;
    let mut last_pinned = 0;
    for attempt in 1..=max_passes {
        r.passes = attempt;
        let changed = dirty.iter().filter(|d| d.load(std::sync::atomic::Ordering::Relaxed)).count();
        /* Nothing changed since the pass before: this pass, and every one after it, is that
           one again (each cell counts what it counted, nothing moves) - its refusals counted
           for each pass left, as going over them would. On a real order a few steps that can
           never settle kept all 24 passes going, each one waiting at every level on every
           core (2026-11-10: slopes 2.2 s on Static's machine). */
        if attempt > 1 && changed == 0 {
            r.pinned += last_pinned * (max_passes - attempt + 1);
            r.passes = max_passes;
            break;
        }
        // Few cells to go over: one thread, in order, is the same answer without the waits.
        let n = if changed < 64 { 1 } else { workers };
        let barrier = Barrier::new(n);
        let parts: Vec<(SlopeReport, usize)> = if n <= 1 {
            vec![work(0, 1, &barrier)]
        } else {
            std::thread::scope(|s| {
                let jobs: Vec<_> = (0..n).map(|w| { let (work, barrier) = (&work, &barrier); s.spawn(move || work(w, n, barrier)) }).collect();
                jobs.into_iter().map(|j| j.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
            })
        };
        let mut excessive = 0;
        last_pinned = 0;
        for (p, e) in parts {
            excessive += e;
            r.adjusted += p.adjusted;
            r.pinned += p.pinned;
            last_pinned += p.pinned;
            r.worst_excess = r.worst_excess.max(p.worst_excess);
            r.cells_touched.extend(p.cells_touched);
        }
        if excessive == 0 {
            settled = true;
            break;
        }
    }
    for (c, g) in keys.iter().zip(grids) {
        *cells.get_mut(c).expect("a key") = g.into_inner().unwrap_or_else(|p| p.into_inner());
    }
    (r, settled)
}

/// The passes one cell after another, as the Python runs them: the reference
/// [`limit_slopes_passes`] is checked against (`across_cores_is_the_same_as_in_order`).
pub fn limit_slopes_passes_in_order(cells: &mut BTreeMap<Coords, Vec<Value>>, limit: Value, max_passes: usize, use_curvature: bool, authoritative: &BTreeSet<Coords>) -> (SlopeReport, bool) {
    let mut r = SlopeReport::default();
    let structure: BTreeMap<Coords, Vec<f64>> =
        if use_curvature { cells.iter().map(|(c, g)| (*c, structure_map(g))).collect() } else { BTreeMap::new() };
    let flat = vec![0.0; LAND_SIZE * LAND_SIZE];
    let in_order: Vec<Coords> = cells.keys().copied().collect();
    let all = pairs();
    let mut settled = false;
    for attempt in 1..=max_passes {
        r.passes = attempt;
        let mut excessive = 0;
        for &coords in &in_order {
            let shape = structure.get(&coords).unwrap_or(&flat);
            for &((ax, ay), (bx, by)) in &all {
                let first = ax + ay * LAND_SIZE;
                let second = bx + by * LAND_SIZE;
                let grid = &cells[&coords];
                let step = grid[second] - grid[first];
                if -limit <= step && step <= limit {
                    continue;
                }
                excessive += 1;
                let excess = step.abs() - limit;
                r.worst_excess = r.worst_excess.max(excess);
                let ma = movable(cells, coords, ax, ay, authoritative);
                let mb = movable(cells, coords, bx, by, authoritative);
                let (share_a, share_b) = if ma && mb {
                    split(shape[first], shape[second], excess)
                } else if ma {
                    (excess, 0)
                } else if mb {
                    (0, excess)
                } else {
                    r.pinned += 1;
                    continue;
                };
                let direction = if step > 0 { 1 } else { -1 };
                shift(cells, coords, ax, ay, direction * share_a, &mut r);
                shift(cells, coords, bx, by, -direction * share_b, &mut r);
            }
        }
        if excessive == 0 {
            settled = true;
            break;
        }
    }
    (r, settled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_a_cliff_out() {
        let mut cells = BTreeMap::new();
        let mut g = vec![0; LAND_SIZE * LAND_SIZE];
        g[32 * LAND_SIZE + 32] = 3000;
        cells.insert((0, 0), g);
        let r = limit_slopes(&mut cells, &BTreeSet::new());
        assert!(r.adjusted > 0 && r.converged);
        assert_eq!(count_unencodable(cells.values(), MAX_STEP - SAFETY), 0);
        assert_eq!(split(0.0, 0.0, 5), (2, 3));
        assert_eq!(split(0.0, 0.0, 3), (2, 1));
    }

    /// A rough field of cells, some authoritative: the levels' answer is the in-order one.
    #[test]
    fn across_cores_is_the_same_as_in_order() {
        let mut seed = 11u64;
        let mut rnd = |n: Value| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as Value).rem_euclid(n)
        };
        let mut cells = BTreeMap::new();
        for x in -6..7 {
            for y in -5..6 {
                if rnd(9) == 0 {
                    continue; // holes in the field
                }
                let g: Vec<Value> = (0..LAND_SIZE * LAND_SIZE).map(|_| rnd(6000) - 3000).collect();
                cells.insert((x, y), g);
            }
        }
        let auth: BTreeSet<Coords> = cells.keys().copied().filter(|_| rnd(7) == 0).collect();
        for curvature in [false, true] {
            let mut a = cells.clone();
            let mut b = cells.clone();
            let ra = limit_slopes_passes(&mut a, MAX_STEP - SAFETY, 6, curvature, &auth);
            let rb = limit_slopes_passes_in_order(&mut b, MAX_STEP - SAFETY, 6, curvature, &auth);
            assert_eq!(ra, rb);
            assert_eq!(a, b);
            assert!(ra.0.adjusted > 0);
        }
    }

    /// A field that settles but for steps no pass can fix (an end on a cell that may not
    /// move): the cells gone over again only when changed give the report and the terrain
    /// the passes over every cell give, all 24 passes counted.
    #[test]
    fn unchanged_cells_count_what_they_counted() {
        let mut cells = BTreeMap::new();
        for x in 0..6 {
            for y in 0..5 {
                let mut g = vec![0; LAND_SIZE * LAND_SIZE];
                if (x + y) % 3 == 0 {
                    // a cliff in the middle, which settles in a few passes
                    for i in 0..LAND_SIZE {
                        g[i * LAND_SIZE + 30] = 4000;
                    }
                }
                cells.insert((x, y), g);
            }
        }
        // A cliff along the edge a cell that may not move shares: never fixed.
        let mut pinned = vec![0; LAND_SIZE * LAND_SIZE];
        for i in 0..LAND_SIZE {
            pinned[i * LAND_SIZE + 1] = 9000;
        }
        cells.insert((6, 0), pinned);
        let auth: BTreeSet<Coords> = [(6, 0)].into();
        let (mut a, mut b) = (cells.clone(), cells.clone());
        let ra = limit_slopes_passes(&mut a, MAX_STEP - SAFETY, MAX_PASSES, false, &auth);
        let rb = limit_slopes_passes_in_order(&mut b, MAX_STEP - SAFETY, MAX_PASSES, false, &auth);
        assert_eq!(ra, rb);
        assert_eq!(a, b);
        assert!(!ra.1 && ra.0.passes == MAX_PASSES && ra.0.adjusted > 0 && ra.0.pinned > 0, "{:?}", ra.0);
    }

    /// The real order's shape (2026-11-10: 4,640 merged cells and 714 borrowed round them,
    /// slopes 2.2 s): a block of cells, steep spots here and there, and cliffs against the
    /// borrowed ring that never settle. `cargo test --release -p landcore slope_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn slope_bench() {
        let mut seed = 5u64;
        let mut rnd = |n: Value| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as Value).rem_euclid(n)
        };
        let (w, h) = (80i64, 67i64);
        let mut cells = BTreeMap::new();
        let mut auth = BTreeSet::new();
        for x in -1..=w {
            for y in -1..=h {
                let ring = x < 0 || y < 0 || x == w || y == h;
                let mut g = vec![0; LAND_SIZE * LAND_SIZE];
                for (i, v) in g.iter_mut().enumerate() {
                    *v = ((i % LAND_SIZE) as Value) * 8;
                }
                if ring {
                    // a cliff along the ring's inner edge: never fixed
                    if rnd(4) == 0 {
                        for i in 0..LAND_SIZE {
                            g[i * LAND_SIZE + 1] += 6000;
                        }
                    }
                    auth.insert((x, y));
                } else if rnd(20) == 0 {
                    let (cx, cy) = (rnd(60) as usize + 2, rnd(60) as usize + 2);
                    g[cy * LAND_SIZE + cx] += 3000;
                }
                cells.insert((x, y), g);
            }
        }
        let t = std::time::Instant::now();
        let r = limit_slopes(&mut cells, &auth);
        eprintln!("{} cells: {:?}, passes {}, adjusted {}, pinned {}", cells.len(), t.elapsed(), r.passes, r.adjusted, r.pinned);
    }
}