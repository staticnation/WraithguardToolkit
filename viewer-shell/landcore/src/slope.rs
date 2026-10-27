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

pub type Coords = (i64, i64);

/// The largest step `VHGT` stores, in world units.
pub const MAX_STEP: i64 = 127 * HEIGHT_SCALE;
/// Kept back from the limit, so rounding cannot push a fixed step over again.
const SAFETY: i64 = HEIGHT_SCALE;
const STRUCTURE_WEIGHT: f64 = 12.0;
pub const MAX_PASSES: usize = 24;
const LAST: usize = LAND_SIZE - 1;

/// What the limiter did (`SlopeReport`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlopeReport {
    pub adjusted: usize,
    pub pinned: usize,
    pub worst_excess: i64,
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

fn movable(cells: &BTreeMap<Coords, Vec<i64>>, coords: Coords, x: usize, y: usize, auth: &BTreeSet<Coords>) -> bool {
    if auth.contains(&coords) {
        return false;
    }
    twins(coords, x, y).iter().all(|(t, _, _)| cells.contains_key(t) && !auth.contains(t))
}

fn shift(cells: &mut BTreeMap<Coords, Vec<i64>>, coords: Coords, x: usize, y: usize, delta: i64, r: &mut SlopeReport) {
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

fn structure_map(grid: &[i64]) -> Vec<f64> {
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

fn split(a: f64, b: f64, excess: i64) -> (i64, i64) {
    let wa = 1.0 / (1.0 + a * STRUCTURE_WEIGHT);
    let wb = 1.0 / (1.0 + b * STRUCTURE_WEIGHT);
    let total = wa + wb;
    if total <= 0.0 {
        let share = excess.div_euclid(2);
        return (share, excess - share);
    }
    let share_a = (round_half_even(excess as f64 * wa / total) as i64).clamp(0, excess);
    (share_a, excess - share_a)
}

/// How many steps exceed what `VHGT` can store (`count_unencodable`).
pub fn count_unencodable<'a>(cells: impl IntoIterator<Item = &'a Vec<i64>>, limit: i64) -> usize {
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
pub fn limit_slopes(cells: &mut BTreeMap<Coords, Vec<i64>>, authoritative: &BTreeSet<Coords>) -> SlopeReport {
    limit_slopes_with(cells, MAX_STEP - SAFETY, false, authoritative)
}

/// `limit_slopes`: every too-steep step shared out until none is left, or `MAX_PASSES`.
pub fn limit_slopes_with(cells: &mut BTreeMap<Coords, Vec<i64>>, limit: i64, use_curvature: bool, authoritative: &BTreeSet<Coords>) -> SlopeReport {
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
pub fn limit_slopes_passes(cells: &mut BTreeMap<Coords, Vec<i64>>, limit: i64, max_passes: usize, use_curvature: bool, authoritative: &BTreeSet<Coords>) -> (SlopeReport, bool) {
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
}
