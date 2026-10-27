//! Seams between cells (`land/seams.py`).
//!
//! Neighbouring cells repeat their shared edge: the last column of one is the first of the
//! next, and a corner is held by four cells. A merge that took one cell from one mod and its
//! neighbour from another can leave those copies disagreeing, which is a wall or a chasm in
//! the game. Corners are settled first (to an authoritative cell's value, the mean of all
//! four, or the terrain outside the merge), then edges, then the correction is feathered a
//! couple of vertices inward so it does not leave a ridge.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use crate::grid::LAND_SIZE;

pub type Coords = (i64, i64);
const LAST: usize = LAND_SIZE - 1;
pub const DEFAULT_FEATHER: i64 = 2;
const SIDES: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, 1), (0, -1)];

/// One corner: the vertex and the offset of each cell holding it.
type Corner = [((usize, usize), (i64, i64)); 4];

/// Each corner of a cell (`_CORNERS`).
const CORNERS: [Corner; 4] = [
    [((0, 0), (0, 0)), ((0, LAST), (0, -1)), ((LAST, 0), (-1, 0)), ((LAST, LAST), (-1, -1))],
    [((LAST, 0), (0, 0)), ((LAST, LAST), (0, -1)), ((0, 0), (1, 0)), ((0, LAST), (1, -1))],
    [((LAST, LAST), (0, 0)), ((LAST, 0), (0, 1)), ((0, LAST), (1, 0)), ((0, 0), (1, 1))],
    [((0, LAST), (0, 0)), ((0, 0), (0, 1)), ((LAST, LAST), (-1, 0)), ((LAST, 0), (-1, 1))],
];

/// A border still disagreeing after repair (`Tear`); `right` `None` for terrain outside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tear {
    pub left: Coords,
    pub right: Option<Coords>,
    pub vertices: usize,
    pub worst: i64,
}

/// What seam repair did (`SeamReport`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeamReport {
    pub corner_vertices: usize,
    pub edge_vertices: usize,
    pub tears: Vec<Tear>,
    pub pinned_corners: usize,
    pub anchored_corners: usize,
    pub largest_gap: i64,
    pub worst_borders: Vec<(Coords, Coords, usize)>,
    pub feathered_vertices: usize,
}

impl SeamReport {
    pub fn total(&self) -> usize {
        self.corner_vertices + self.edge_vertices
    }
}

fn index(x: usize, y: usize) -> usize {
    y * LAND_SIZE + x
}

/// Python's `int // int` for a positive divisor rounds down; `mean` rounds toward zero.
pub fn mean(values: &[i64]) -> i64 {
    let total: i64 = values.iter().sum();
    let count = values.len() as i64;
    if total < 0 { -((-total).div_euclid(count)) } else { total.div_euclid(count) }
}

/// Neighbouring pairs of cells, each once, lower first (`_border_pairs`).
fn border_pairs(cells: &BTreeMap<Coords, Vec<i64>>) -> Vec<(Coords, Coords)> {
    let mut seen: HashSet<(Coords, Coords)> = HashSet::new();
    let mut queue: VecDeque<(Coords, Coords)> = VecDeque::new();
    for &coords in cells.keys() {
        for (dx, dy) in SIDES {
            let n = (coords.0 + dx, coords.1 + dy);
            let pair = if coords < n { (coords, n) } else { (n, coords) };
            if seen.insert(pair) {
                queue.push_back(pair);
            }
        }
    }
    queue.into_iter().filter(|(l, r)| cells.contains_key(l) && cells.contains_key(r)).collect()
}

fn compare_border(low: &[i64], high: &[i64], vertical: bool) -> Option<(usize, i64)> {
    let (mut count, mut worst) = (0, 0);
    for step in 0..LAND_SIZE {
        let (a, b) = if vertical { (index(step, LAST), index(step, 0)) } else { (index(LAST, step), index(0, step)) };
        let gap = (low[a] - high[b]).abs();
        if gap != 0 {
            count += 1;
            worst = worst.max(gap);
        }
    }
    (count > 0).then_some((count, worst))
}

/// The borders that still disagree (`find_tears`), worst first.
pub fn find_tears(cells: &BTreeMap<Coords, Vec<i64>>, reference: Option<&BTreeMap<Coords, Vec<i64>>>, untouched: &BTreeSet<Coords>) -> Vec<Tear> {
    let mut tears = Vec::new();
    for (left, right) in border_pairs(cells) {
        if untouched.contains(&left) && untouched.contains(&right) {
            continue;
        }
        if let Some((count, worst)) = compare_border(&cells[&left], &cells[&right], left.0 == right.0) {
            tears.push(Tear { left, right: Some(right), vertices: count, worst });
        }
    }
    if let Some(reference) = reference {
        for (&coords, grid) in cells {
            if untouched.contains(&coords) {
                continue;
            }
            for (dx, dy) in SIDES {
                let other = (coords.0 + dx, coords.1 + dy);
                if cells.contains_key(&other) {
                    continue;
                }
                let Some(outside) = reference.get(&other) else { continue };
                let forward = matches!((dx, dy), (1, 0) | (0, 1));
                let (low, high) = if forward { (grid, outside) } else { (outside, grid) };
                if let Some((count, worst)) = compare_border(low, high, dx == 0) {
                    tears.push(Tear { left: coords, right: None, vertices: count, worst });
                }
            }
        }
    }
    tears.sort_by_key(|t| -t.worst);
    tears
}

fn anchor_value(corner: &Corner, coords: Coords, cells: &BTreeMap<Coords, Vec<i64>>, anchor: Option<&BTreeMap<Coords, Vec<i64>>>) -> Option<i64> {
    let anchor = anchor?;
    for &((x, y), (dx, dy)) in corner {
        let other = (coords.0 + dx, coords.1 + dy);
        if cells.contains_key(&other) {
            continue;
        }
        if let Some(grid) = anchor.get(&other) {
            return Some(grid[index(x, y)]);
        }
    }
    None
}

fn shared_cells(coords: Coords, x: usize, y: usize) -> Vec<Coords> {
    let (cx, cy) = coords;
    let mut xs = vec![cx];
    if x == 0 {
        xs.push(cx - 1);
    } else if x == LAST {
        xs.push(cx + 1);
    }
    let mut ys = vec![cy];
    if y == 0 {
        ys.push(cy - 1);
    } else if y == LAST {
        ys.push(cy + 1);
    }
    xs.iter().flat_map(|&gx| ys.iter().map(move |&gy| (gx, gy))).collect()
}

fn is_pinned(cells: &BTreeMap<Coords, Vec<i64>>, coords: Coords, x: usize, y: usize) -> bool {
    shared_cells(coords, x, y).iter().any(|c| !cells.contains_key(c))
}

fn repair_corners(cells: &mut BTreeMap<Coords, Vec<i64>>, report: &mut SeamReport, anchor: Option<&BTreeMap<Coords, Vec<i64>>>, authoritative: &BTreeSet<Coords>) {
    let order: Vec<Coords> = cells.keys().copied().collect();
    for coords in order {
        for corner in &CORNERS {
            let present: Vec<(Coords, usize)> = corner
                .iter()
                .filter_map(|&((x, y), (dx, dy))| {
                    let other = (coords.0 + dx, coords.1 + dy);
                    cells.contains_key(&other).then_some((other, index(x, y)))
                })
                .collect();
            if present.len() < 2 {
                continue;
            }
            let values: Vec<i64> = present.iter().map(|(c, o)| cells[c][*o]).collect();
            let spread = values.iter().max().unwrap() - values.iter().min().unwrap();
            if spread != 0 {
                report.largest_gap = report.largest_gap.max(spread);
            }
            let fixed: Vec<i64> = present.iter().filter(|(c, _)| authoritative.contains(c)).map(|(c, o)| cells[c][*o]).collect();
            let average = if let Some(&first) = fixed.first() {
                report.anchored_corners += 1;
                first
            } else if present.len() == corner.len() {
                mean(&values)
            } else if let Some(outside) = anchor_value(corner, coords, cells, anchor) {
                report.anchored_corners += 1;
                outside
            } else {
                report.pinned_corners += 1;
                mean(&values)
            };
            for (c, o) in &present {
                let g = cells.get_mut(c).unwrap();
                if g[*o] != average {
                    g[*o] = average;
                    report.corner_vertices += 1;
                }
            }
        }
    }
}

fn repair_edges(cells: &mut BTreeMap<Coords, Vec<i64>>, report: &mut SeamReport, authoritative: &BTreeSet<Coords>) -> Result<(), String> {
    let mut borders = Vec::new();
    for (left, right) in border_pairs(cells) {
        let vertical = left.0 == right.0;
        let mut moved = 0;
        for step in 0..LAND_SIZE {
            let (a, b) = if vertical { (index(step, LAST), index(step, 0)) } else { (index(LAST, step), index(0, step)) };
            let (first, second) = (cells[&left][a], cells[&right][b]);
            if first == second {
                continue;
            }
            if step == 0 || step == LAST {
                let (lx, ly) = if vertical { (step, LAST) } else { (LAST, step) };
                if is_pinned(cells, left, lx, ly) {
                    report.pinned_corners += 1;
                    continue;
                }
                return Err(format!(
                    "corner at step {step} of the border ({}, {})-({}, {}) still differs ({first} vs {second}). Corners must be repaired before edges, or their endpoints are averaged twice.",
                    left.0, left.1, right.0, right.1
                ));
            }
            report.largest_gap = report.largest_gap.max((first - second).abs());
            let (lf, rf) = (authoritative.contains(&left), authoritative.contains(&right));
            if lf && rf {
                continue;
            }
            let average = if lf {
                first
            } else if rf {
                second
            } else {
                mean(&[first, second])
            };
            cells.get_mut(&left).unwrap()[a] = average;
            cells.get_mut(&right).unwrap()[b] = average;
            moved += 1;
        }
        if moved > 0 {
            report.edge_vertices += moved;
            borders.push((left, right, moved));
        }
    }
    borders.sort_by_key(|b| std::cmp::Reverse(b.2));
    borders.truncate(20);
    report.worst_borders = borders;
    Ok(())
}

fn feather_corrections(cells: &mut BTreeMap<Coords, Vec<i64>>, moves: &BTreeMap<Coords, Vec<(usize, i64)>>, depth: i64, report: &mut SeamReport) {
    if depth < 2 {
        return;
    }
    for (coords, moved) in moves {
        let Some(grid) = cells.get_mut(coords) else { continue };
        for &(offset, delta) in moved {
            if delta == 0 {
                continue;
            }
            let (x, y) = ((offset % LAND_SIZE) as i64, (offset / LAND_SIZE) as i64);
            let last = LAST as i64;
            for (ax, ay, limit) in [(-1i64, 0i64, x), (1, 0, last - x), (0, -1, y), (0, 1, last - y)] {
                if limit != 0 && limit != last {
                    continue;
                }
                for step in 1..depth.min(LAND_SIZE as i64 - 1) {
                    let (nx, ny) = (x + ax * step, y + ay * step);
                    if !(0 < nx && nx < last && 0 < ny && ny < last) {
                        break;
                    }
                    let share = (delta * (depth - step)).div_euclid(depth);
                    if share == 0 {
                        break;
                    }
                    grid[ny as usize * LAND_SIZE + nx as usize] += share;
                    report.feathered_vertices += 1;
                }
            }
        }
    }
}

fn boundary_offsets() -> Vec<usize> {
    let mut seen = Vec::new();
    for step in 0..LAND_SIZE {
        for o in [index(step, 0), index(step, LAST), index(0, step), index(LAST, step)] {
            if !seen.contains(&o) {
                seen.push(o);
            }
        }
    }
    seen
}

/// `repair_seams`: corners, edges, then the feather, in place.
pub fn repair_seams(
    cells: &mut BTreeMap<Coords, Vec<i64>>,
    feather: i64,
    anchor: Option<&BTreeMap<Coords, Vec<i64>>>,
    authoritative: &BTreeSet<Coords>,
) -> Result<SeamReport, String> {
    for (c, g) in cells.iter() {
        if g.len() != LAND_SIZE * LAND_SIZE {
            return Err(format!("cell ({}, {}) has {} vertices, expected {}", c.0, c.1, g.len(), LAND_SIZE * LAND_SIZE));
        }
    }
    let mut report = SeamReport::default();
    let offsets = boundary_offsets();
    let before: BTreeMap<Coords, Vec<(usize, i64)>> = cells.iter().map(|(c, g)| (*c, offsets.iter().map(|&o| (o, g[o])).collect())).collect();
    repair_corners(cells, &mut report, anchor, authoritative);
    repair_edges(cells, &mut report, authoritative)?;
    if feather >= 2 {
        let moves: BTreeMap<Coords, Vec<(usize, i64)>> = before
            .iter()
            .map(|(c, vals)| (*c, vals.iter().filter(|(o, v)| cells[c][*o] != *v).map(|(o, v)| (*o, cells[c][*o] - v)).collect()))
            .collect();
        feather_corrections(cells, &moves, feather, &mut report);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn means_round_toward_zero() {
        assert_eq!(mean(&[-3, 0]), -1);
        assert_eq!(mean(&[3, 0]), 1);
        assert_eq!(mean(&[-4, 1, 0]), -1);
    }

    #[test]
    fn mends_an_edge() {
        let mut cells = BTreeMap::new();
        cells.insert((0, 0), vec![0; LAND_SIZE * LAND_SIZE]);
        let mut right = vec![0; LAND_SIZE * LAND_SIZE];
        right[index(0, 30)] = 100;
        cells.insert((1, 0), right);
        let r = repair_seams(&mut cells, DEFAULT_FEATHER, None, &BTreeSet::new()).unwrap();
        assert_eq!(r.edge_vertices, 1);
        assert_eq!(cells[&(0, 0)][index(LAST, 30)], 50);
        assert_eq!(cells[&(1, 0)][index(0, 30)], 50);
        assert!(find_tears(&cells, None, &BTreeSet::new()).is_empty());
        assert_eq!(r.feathered_vertices, 2);
        assert_eq!(cells[&(0, 0)][index(LAST - 1, 30)], 25);
        assert_eq!(cells[&(1, 0)][index(1, 30)], -25);
    }
}
