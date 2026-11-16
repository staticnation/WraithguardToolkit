//! Heights a record can hold: one eight-unit phase per connected piece of terrain, and every
//! stored step inside a signed byte (the OpenMW fork's `repair/height_encoding.rs`).
//!
//! `VHGT` is an `f32` offset and signed-byte steps of eight units, so every height of a cell is
//! the offset's remainder (its *phase*) plus a whole number of steps. A record whose offset is
//! not a whole number (400.5) holds heights like 3,204; floored to 3,200 they no longer meet a
//! neighbor that stays at 3,204. So cells that touch keep one phase between them, the one most
//! of their vertices already have.
//!
//! Only the steps along a row, and down the first column, are stored. One that does not fit is
//! repaired by lowering its higher end to the bound, every copy of that vertex with it (a border
//! vertex is in two cells, a corner in four), and following on from there until none is left.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::grid::{Value, HEIGHT_SCALE, LAND_SIZE};
use crate::heights::{MAX_GRADIENT, MIN_GRADIENT};
use crate::landmass::Coords;

const LAST: usize = LAND_SIZE - 1;

/// `v` on the eight-unit grid of `phase`: the grid height toward zero from it, as the OpenMW
/// fork rounds (`(height - phase) / 8 * 8 + phase`, an integer division that truncates), so a
/// height under the phase goes up to the grid and one over it goes down.
pub fn quantize_to_phase(v: Value, phase: Value) -> Value {
    (v - phase) / HEIGHT_SCALE * HEIGHT_SCALE + phase
}

/// Every cell put on the phase most vertices of its connected piece have (cells that share a
/// border or only a corner are connected; `quantize_height_components`). A tie goes to the phase
/// of the piece's first cell's first vertex, then to the lowest. Answers each cell's phase.
pub fn quantize_components(cells: &mut BTreeMap<Coords, Vec<Value>>) -> BTreeMap<Coords, Value> {
    let mut phases: BTreeMap<Coords, Value> = BTreeMap::new();
    let starts: Vec<Coords> = cells.keys().copied().collect();
    for start in starts {
        if phases.contains_key(&start) {
            continue;
        }
        let mut seen: BTreeSet<Coords> = BTreeSet::from([start]);
        let mut pending = VecDeque::from([start]);
        let mut counts = [0usize; HEIGHT_SCALE as usize];
        while let Some(c) = pending.pop_front() {
            for &v in &cells[&c] {
                counts[v.rem_euclid(HEIGHT_SCALE) as usize] += 1;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let n = (c.0 + dx, c.1 + dy);
                    if cells.contains_key(&n) && seen.insert(n) {
                        pending.push_back(n);
                    }
                }
            }
        }
        let mut phase = cells[&start][0].rem_euclid(HEIGHT_SCALE) as usize;
        for candidate in 0..counts.len() {
            if counts[candidate] > counts[phase] {
                phase = candidate;
            }
        }
        for c in seen {
            for v in cells.get_mut(&c).expect("a cell of the piece") {
                *v = quantize_to_phase(*v, phase as Value);
            }
            phases.insert(c, phase as Value);
        }
    }
    phases
}

/// What [`fit_steps`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepReport {
    /// Vertices lowered (a vertex lowered twice counts twice).
    pub lowered: usize,
    /// Cells brought in because a vertex they share was lowered.
    pub brought_in: Vec<Coords>,
    /// False when the work was given up (it does not end where copies of a vertex are kept
    /// apart on purpose and keep undoing each other): the encoder's clamp is then what is left.
    pub finished: bool,
}

/// A vertex of the world: every cell that has it, and where.
fn copies(cell: Coords, x: usize, y: usize) -> Vec<(Coords, usize)> {
    let xs: Vec<(i64, usize)> = if x == 0 { vec![(cell.0, 0), (cell.0 - 1, LAST)] } else if x == LAST { vec![(cell.0, LAST), (cell.0 + 1, 0)] } else { vec![(cell.0, x)] };
    let ys: Vec<(i64, usize)> = if y == 0 { vec![(cell.1, 0), (cell.1 - 1, LAST)] } else if y == LAST { vec![(cell.1, LAST), (cell.1 + 1, 0)] } else { vec![(cell.1, y)] };
    let mut out = Vec::with_capacity(4);
    for &(cy, vy) in &ys {
        for &(cx, vx) in &xs {
            out.push(((cx, cy), vy * LAND_SIZE + vx));
        }
    }
    out
}

/// Every stored step made to fit (`repair_height_encoding`'s second half). `outside` gives a
/// cell the merge does not hold, as the game loads it: the fork holds every cell of the world,
/// so a vertex it lowers moves in all the cells that share it, and here a cell that shares one
/// and is not in `cells` is brought in for that (or the border would tear).
pub fn fit_steps(cells: &mut BTreeMap<Coords, Vec<Value>>, outside: &dyn Fn(Coords) -> Option<Vec<Value>>) -> StepReport {
    let (up, down) = (MAX_GRADIENT * HEIGHT_SCALE, -MIN_GRADIENT * HEIGHT_SCALE);
    let mut report = StepReport { finished: true, ..Default::default() };
    let mut pending: VecDeque<(Coords, usize, usize)> = VecDeque::new();
    // The stored edges that end at a vertex of a cell: along its row, and down the first column.
    let edges_at = |x: usize, y: usize| {
        let mut e: Vec<((usize, usize), (usize, usize))> = Vec::with_capacity(4);
        if x > 0 {
            e.push(((x - 1, y), (x, y)));
        }
        if x < LAST {
            e.push(((x, y), (x + 1, y)));
        }
        if x == 0 {
            if y > 0 {
                e.push(((0, y - 1), (0, y)));
            }
            if y < LAST {
                e.push(((0, y), (0, y + 1)));
            }
        }
        e
    };
    // One edge held to its bounds: the higher end lowered, in every cell that has that vertex.
    #[allow(clippy::too_many_arguments)] // the state of the walk, and one edge
    fn constrain(
        cells: &mut BTreeMap<Coords, Vec<Value>>,
        outside: &dyn Fn(Coords) -> Option<Vec<Value>>,
        report: &mut StepReport,
        pending: &mut VecDeque<(Coords, usize, usize)>,
        cell: Coords,
        from: (usize, usize),
        to: (usize, usize),
        up: Value,
        down: Value,
    ) {
        let g = &cells[&cell];
        let (a, b) = (g[from.1 * LAND_SIZE + from.0], g[to.1 * LAND_SIZE + to.0]);
        let ((x, y), height) = if b - a > up {
            (to, a + up)
        } else if a - b > down {
            (from, b + down)
        } else {
            return;
        };
        for (c, i) in copies(cell, x, y) {
            if let std::collections::btree_map::Entry::Vacant(e) = cells.entry(c) {
                let Some(theirs) = outside(c) else { continue };
                e.insert(theirs);
                report.brought_in.push(c);
            }
            cells.get_mut(&c).expect("the cell")[i] = height;
        }
        report.lowered += 1;
        pending.push_back((cell, x, y));
    }
    let start: Vec<Coords> = cells.keys().copied().collect();
    for cell in start {
        for y in 0..LAND_SIZE {
            if y > 0 {
                constrain(cells, outside, &mut report, &mut pending, cell, (0, y - 1), (0, y), up, down);
            }
            for x in 1..LAND_SIZE {
                constrain(cells, outside, &mut report, &mut pending, cell, (x - 1, y), (x, y), up, down);
            }
        }
    }
    // Far more than any terrain needs; reached only where the work cannot end.
    let mut budget = 64 * LAND_SIZE * LAND_SIZE * cells.len().max(1);
    while let Some((cell, x, y)) = pending.pop_front() {
        if budget == 0 {
            report.finished = false;
            break;
        }
        budget -= 1;
        for (c, i) in copies(cell, x, y) {
            if !cells.contains_key(&c) {
                continue;
            }
            for (from, to) in edges_at(i % LAND_SIZE, i / LAND_SIZE) {
                constrain(cells, outside, &mut report, &mut pending, c, from, to, up, down);
            }
        }
    }
    report.brought_in.sort();
    report.brought_in.dedup();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(h: Value) -> Vec<Value> {
        vec![h; LAND_SIZE * LAND_SIZE]
    }

    /// A fractional offset's heights keep their phase, and the cells that touch share one.
    #[test]
    fn touching_cells_share_the_phase_most_vertices_have() {
        let mut cells = BTreeMap::from([((0, 0), flat(3204)), ((1, 1), flat(516)), ((5, 5), flat(801))]);
        cells.get_mut(&(1, 1)).unwrap()[7] = 513;
        let phases = quantize_components(&mut cells);
        assert_eq!((phases[&(0, 0)], phases[&(1, 1)], phases[&(5, 5)]), (4, 4, 1));
        assert!(cells[&(0, 0)].iter().all(|&v| v == 3204));
        assert_eq!(cells[&(1, 1)][7], 508, "on the piece's phase, toward it");
        assert!(cells[&(5, 5)].iter().all(|&v| v == 801));
        // Toward zero, as the fork: -3 is 0, and 3 is 0.
        assert_eq!((quantize_to_phase(-3, 0), quantize_to_phase(3, 0), quantize_to_phase(-13, 0)), (0, 0, -8));
    }

    /// A cliff no record can hold comes down in steps, the border vertex in both cells, and a
    /// legal step (the full +1,016 and -1,024) is left alone.
    #[test]
    fn a_step_too_high_is_lowered_in_every_cell_that_has_the_vertex() {
        let mut left = flat(0);
        let mut right = flat(0);
        for y in 0..LAND_SIZE {
            left[y * LAND_SIZE + LAST] = 4000;
            right[y * LAND_SIZE] = 4000;
            right[y * LAND_SIZE + 1] = 4000 - 1024;
        }
        let mut cells = BTreeMap::from([((0, 0), left), ((1, 0), right)]);
        let r = fit_steps(&mut cells, &|_| None);
        assert!(r.finished && r.lowered > 0 && r.brought_in.is_empty());
        for g in cells.values() {
            for y in 0..LAND_SIZE {
                for x in 1..LAND_SIZE {
                    let d = g[y * LAND_SIZE + x] - g[y * LAND_SIZE + x - 1];
                    assert!((-1024..=1016).contains(&d), "{d}");
                }
            }
        }
        assert_eq!(cells[&(0, 0)][LAST], 1016);
        assert_eq!(cells[&(1, 0)][0], 1016, "the same vertex");
        let mut legal = flat(0);
        legal[1] = 1016;
        legal[2] = 1016;
        for v in &mut legal[3..LAND_SIZE] {
            *v = -8;
        }
        let mut cells = BTreeMap::from([((0, 0), legal.clone())]);
        assert_eq!(fit_steps(&mut cells, &|_| None).lowered, 0);
        assert_eq!(cells[&(0, 0)], legal);
    }

    /// A vertex lowered on the border of the merge takes the cell beyond it along.
    #[test]
    fn a_cell_outside_is_brought_in_when_its_vertex_is_lowered() {
        let mut inside = flat(0);
        for y in 0..LAND_SIZE {
            inside[y * LAND_SIZE + LAST] = 2000;
        }
        let mut cells = BTreeMap::from([((0, 0), inside)]);
        let r = fit_steps(&mut cells, &|c| (c == (1, 0)).then(|| flat(2000)));
        assert_eq!(r.brought_in, vec![(1, 0)]);
        assert_eq!(cells[&(1, 0)][0], 1016);
        assert_eq!(cells[&(1, 0)][1], 2000, "the next step, 984, fits");
    }
}
