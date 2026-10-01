//! The numeric core of Merged Lands (`wraithguard/land/`), in Rust.
//!
//! `wraithguard/land/` is our port of Merged Lands and its OpenMW fork, with our own
//! fixes. Everything that decides *what* happens - settings, strategies, which cells
//! are merged, reports - stays in Python there. What moves here is the per-vertex
//! arithmetic it runs millions of times over a large landmass:
//!
//! - [`RelativeGrid`]: a reference grid plus one plugin's deltas (`land/diff.py`),
//!   with the same methods, so every caller keeps working;
//! - [`merge_grids`]: the per-vertex loop of `land/merge.py`'s `merge_layer`;
//! - [`limit_slopes`]: `land/slope.py`'s limiter;
//! - [`normals_for`]: `land/heights.py`'s normals and `land/pipeline.py`'s
//!   `resolve_normals`;
//! - [`decode_heights`]: `tes3fields/landscape.py`'s `VHGT` decoder.
//!
//! Each is the Python it replaces, step for step - including Python's rounding
//! (`round` is half-to-even; `int()` truncates toward zero) and its floating-point
//! calls (`x ** 0.5` is `pow`, not `sqrt`) - so the merged terrain is the same to the
//! unit. The Python tests of `land/` run against these.

use std::collections::{HashMap, HashSet};

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// Vertices per edge of a `LAND` grid.
const LAND_SIZE: usize = 65;
/// Stored heights are divided by this.
const HEIGHT_SCALE: f64 = 8.0;

// ---------------------------------------------------------------------------
// RelativeGrid

/// A reference grid plus the deltas one plugin applied to it (`land/diff.py`).
///
/// Values are flat and interleaved: a 65x65 grid of three-component normals is one
/// list of 12,675 integers. One changed flag per *vertex*.
#[pyclass(module = "wraithguard_native")]
#[derive(Clone)]
pub struct RelativeGrid {
    reference: Vec<i64>,
    delta: Vec<i64>,
    changed: Vec<bool>,
    #[pyo3(get)]
    side: usize,
    #[pyo3(get)]
    components: usize,
}

impl RelativeGrid {
    fn make(reference: Vec<i64>, side: usize, components: usize) -> PyResult<Self> {
        let expected = side * side * components;
        if reference.len() != expected {
            return Err(PyValueError::new_err(format!(
                "expected {expected} values for a {side}x{side} grid of {components}-component values, got {}",
                reference.len()
            )));
        }
        Ok(Self { delta: vec![0; expected], changed: vec![false; side * side], reference, side, components })
    }

    fn vertex(&self, x: i64, y: i64) -> PyResult<usize> {
        let (s, n) = (self.side as i64, (self.side * self.side) as i64);
        let i = y * s + x;
        // Python indexes a list from the end with a negative index; kept.
        let i = if i < 0 { i + n } else { i };
        if i < 0 || i >= n {
            return Err(PyIndexError::new_err("list index out of range"));
        }
        Ok(i as usize)
    }

    fn index(&self, x: i64, y: i64, component: i64) -> PyResult<usize> {
        let n = self.reference.len() as i64;
        let i = (y * self.side as i64 + x) * self.components as i64 + component;
        let i = if i < 0 { i + n } else { i };
        if i < 0 || i >= n {
            return Err(PyIndexError::new_err("list index out of range"));
        }
        Ok(i as usize)
    }

    fn flat(&self) -> Vec<i64> {
        self.reference.iter().zip(&self.delta).map(|(a, b)| a + b).collect()
    }
}

#[pymethods]
impl RelativeGrid {
    /// Wrap a reference grid with an all-zero set of deltas.
    #[new]
    #[pyo3(signature = (reference, side, components=1))]
    fn new(reference: Vec<i64>, side: usize, components: usize) -> PyResult<Self> {
        Self::make(reference, side, components)
    }

    /// Build the difference of a plugin's grid against a reference.
    #[classmethod]
    #[pyo3(signature = (reference, plugin, side, components=1))]
    fn from_difference(
        _cls: &Bound<'_, pyo3::types::PyType>,
        reference: Vec<i64>,
        plugin: Vec<i64>,
        side: usize,
        components: usize,
    ) -> PyResult<Self> {
        difference(reference, &plugin, side, components)
    }

    #[pyo3(signature = (x, y, component=0))]
    fn offset_of(&self, x: i64, y: i64, component: i64) -> i64 {
        (y * self.side as i64 + x) * self.components as i64 + component
    }

    #[pyo3(signature = (x, y, component=0))]
    fn value_at(&self, x: i64, y: i64, component: i64) -> PyResult<i64> {
        let i = self.index(x, y, component)?;
        Ok(self.reference[i] + self.delta[i])
    }

    #[pyo3(signature = (x, y, component=0))]
    fn delta_at(&self, x: i64, y: i64, component: i64) -> PyResult<i64> {
        Ok(self.delta[self.index(x, y, component)?])
    }

    fn has_difference(&self, x: i64, y: i64) -> PyResult<bool> {
        Ok(self.changed[self.vertex(x, y)?])
    }

    fn set_value(&mut self, x: i64, y: i64, values: Vec<i64>) -> PyResult<()> {
        if values.len() != self.components {
            return Err(PyValueError::new_err(format!(
                "expected {} component(s), got {}",
                self.components,
                values.len()
            )));
        }
        let mut changed = false;
        for (c, v) in values.iter().enumerate() {
            let i = self.index(x, y, c as i64)?;
            let d = v - self.reference[i];
            self.delta[i] = d;
            changed = changed || d != 0;
        }
        let v = self.vertex(x, y)?;
        self.changed[v] = changed;
        Ok(())
    }

    fn deltas_at<'py>(&self, py: Python<'py>, x: i64, y: i64) -> PyResult<Bound<'py, PyTuple>> {
        let start = ((y * self.side as i64 + x) * self.components as i64).max(0) as usize;
        let end = (start + self.components).min(self.delta.len());
        PyTuple::new(py, &self.delta[start.min(end)..end])
    }

    fn set_deltas(&mut self, x: i64, y: i64, deltas: Vec<i64>) -> PyResult<()> {
        if deltas.len() != self.components {
            return Err(PyValueError::new_err(format!(
                "expected {} delta(s), got {}",
                self.components,
                deltas.len()
            )));
        }
        let start = self.index(x, y, 0)?;
        let mut changed = false;
        for (o, d) in deltas.iter().enumerate() {
            self.delta[start + o] = *d;
            changed = changed || *d != 0;
        }
        let v = self.vertex(x, y)?;
        self.changed[v] = changed;
        Ok(())
    }

    /// A copy of the reference values, with no deltas applied.
    fn to_flat_reference(&self) -> Vec<i64> {
        self.reference.clone()
    }

    /// Discard a vertex's delta, returning it to the reference.
    fn clear(&mut self, x: i64, y: i64) -> PyResult<()> {
        for c in 0..self.components {
            let i = self.index(x, y, c as i64)?;
            self.delta[i] = 0;
        }
        let v = self.vertex(x, y)?;
        self.changed[v] = false;
        Ok(())
    }

    #[getter]
    fn is_modified(&self) -> bool {
        self.changed.iter().any(|&c| c)
    }

    #[getter]
    fn num_differences(&self) -> usize {
        self.changed.iter().filter(|&&c| c).count()
    }

    fn changed_vertices(&self) -> Vec<(usize, usize)> {
        let s = self.side;
        self.changed.iter().enumerate().filter(|(_, c)| **c).map(|(i, _)| (i % s, i / s)).collect()
    }

    /// The plugin's grid as flat values: reference plus delta.
    fn to_flat(&self) -> Vec<i64> {
        self.flat()
    }

    fn to_rows(&self) -> PyResult<Vec<Vec<i64>>> {
        if self.components != 1 {
            return Err(PyValueError::new_err("to_rows is only meaningful for single-component grids"));
        }
        Ok(self.flat().chunks(self.side).map(|r| r.to_vec()).collect())
    }

    fn __copy__(&self) -> Self {
        self.clone()
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        self.clone()
    }
}

fn difference(reference: Vec<i64>, plugin: &[i64], side: usize, components: usize) -> PyResult<RelativeGrid> {
    let mut grid = RelativeGrid::make(reference, side, components)?;
    if plugin.len() != grid.reference.len() {
        return Err(PyValueError::new_err(format!(
            "plugin grid has {} values, reference has {}",
            plugin.len(),
            grid.reference.len()
        )));
    }
    for (i, (was, now)) in grid.reference.iter().zip(plugin).enumerate() {
        if was != now {
            grid.delta[i] = now - was;
            grid.changed[i / components] = true;
        }
    }
    Ok(grid)
}

// ---------------------------------------------------------------------------
// Merging one layer (land/merge.py)

/// Python's `round`: half to even.
fn round_half_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 { 2.0 * (v / 2.0).round() } else { r }
}

/// `average_delta`: the two edits blended toward the larger, and whether the
/// compromise is major.
fn average_delta(first: i64, second: i64, p: (f64, f64, f64)) -> (i64, bool) {
    let (s1, s2) = (first.abs() as f64, second.abs() as f64);
    let total = s1 + s2;
    if total == 0.0 {
        return (0, false);
    }
    let weight = s1 / total;
    let biased = weight.powf(1.5);
    let other = (1.0 - weight).powf(1.5);
    let weight = biased / (biased + other);
    let blended = weight * first as f64 + (1.0 - weight) * second as f64;
    (blended.trunc() as i64, is_major(first, second, blended, p))
}

fn is_major(first: i64, second: i64, blended: f64, (pct, lo, hi): (f64, f64, f64)) -> bool {
    let smaller = first.min(second) as f64;
    let threshold = (pct * smaller).max(lo).min(hi);
    (smaller - blended).abs() >= threshold
}

/// `weighted_delta`: the two edits blended with the caller's weights.
fn weighted_delta(first: i64, second: i64, w1: f64, w2: f64, p: (f64, f64, f64)) -> (i64, bool) {
    let total = w1 + w2;
    if total <= 0.0 {
        return average_delta(first, second, p);
    }
    let share = w1 / total;
    let blended = share * first as f64 + (1.0 - share) * second as f64;
    (blended.trunc() as i64, is_major(first, second, blended, p))
}

/// `curvature._normal_at`.
fn normal_at(rows: &[f64], side: usize, x: usize, y: usize) -> (f64, f64, f64) {
    let limit = side - 1;
    let fx = if x == limit { x - 1 } else { x };
    let fy = if y == limit { y - 1 } else { y };
    let step = 128.0 / HEIGHT_SCALE;
    let here = rows[fy * side + fx] / HEIGHT_SCALE;
    let east = rows[fy * side + fx + 1] / HEIGHT_SCALE;
    let north = rows[(fy + 1) * side + fx] / HEIGHT_SCALE;
    let nx = -(east - here) * step;
    let ny = -(north - here) * step;
    let nz = step * step;
    let length = (nx * nx + ny * ny + nz * nz).sqrt();
    if length == 0.0 {
        return (0.0, 0.0, 1.0);
    }
    (nx / length, ny / length, nz / length)
}

/// `curvature.curvature_at`: the mean angle between a vertex's normal and its
/// neighbours'.
fn curvature_at(rows: &[f64], side: usize, x: usize, y: usize) -> f64 {
    let here = normal_at(rows, side, x, y);
    let (mut total, mut counted) = (0.0, 0);
    for (dx, dy) in [(1i64, 0i64), (0, 1), (-1, 0), (0, -1)] {
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        if nx < 0 || ny < 0 || nx >= side as i64 || ny >= side as i64 {
            continue;
        }
        let o = normal_at(rows, side, nx as usize, ny as usize);
        let dot = here.0 * o.0 + here.1 * o.1 + here.2 * o.2;
        total += dot.clamp(-1.0, 1.0).acos();
        counted += 1;
    }
    if counted > 0 { total / counted as f64 } else { 0.0 }
}

/// The per-vertex loop of `merge_layer`: combine two plugins' edits to one layer.
///
/// `strategy` is `"overwrite"`, `"ignore"`, `"resolve"` or `"curvature"` (already
/// resolved from `AUTO` and checked by the caller). Returns the merged grid and the
/// counts `(taken_from_one, taken_from_two, contested, minor, major, major_vertices)`.
#[pyfunction]
#[pyo3(signature = (first, second, strategy, minor_threshold_pct=0.3, minor_threshold_min=10.0, minor_threshold_max=64.0))]
#[allow(clippy::type_complexity)]
fn merge_grids(
    first: PyRef<'_, RelativeGrid>,
    second: PyRef<'_, RelativeGrid>,
    strategy: &str,
    minor_threshold_pct: f64,
    minor_threshold_min: f64,
    minor_threshold_max: f64,
) -> PyResult<(RelativeGrid, usize, usize, usize, usize, usize, Vec<(usize, usize)>)> {
    if first.side != second.side || first.components != second.components {
        return Err(PyValueError::new_err(format!(
            "cannot merge a {0}x{0}x{1} grid with a {2}x{2}x{3} one",
            first.side, first.components, second.side, second.components
        )));
    }
    let p = (minor_threshold_pct, minor_threshold_min, minor_threshold_max);
    let (side, comps) = (first.side, first.components);
    let mut merged = RelativeGrid::make(first.reference.clone(), side, comps)?;
    let (mut one, mut two, mut contested, mut minor, mut major) = (0, 0, 0, 0, 0);
    let mut major_vertices = Vec::new();

    // Curvature weighting reads each side's terrain: the reference, and each applied.
    let surfaces = (strategy == "curvature").then(|| {
        let f = |v: Vec<i64>| v.into_iter().map(|x| x as f64).collect::<Vec<f64>>();
        (f(first.reference.clone()), f(first.flat()), f(second.flat()))
    });

    let put = |merged: &mut RelativeGrid, v: usize, deltas: &[i64]| {
        let start = v * comps;
        let mut changed = false;
        for (o, d) in deltas.iter().enumerate() {
            merged.delta[start + o] = *d;
            changed = changed || *d != 0;
        }
        merged.changed[v] = changed;
    };

    for y in 0..side {
        for x in 0..side {
            let v = y * side + x;
            let (m1, m2) = (first.changed[v], second.changed[v]);
            let d1 = &first.delta[v * comps..(v + 1) * comps];
            let d2 = &second.delta[v * comps..(v + 1) * comps];
            match (m1, m2) {
                (false, false) => continue,
                (true, false) => {
                    one += 1;
                    put(&mut merged, v, d1);
                    continue;
                }
                (false, true) => {
                    two += 1;
                    put(&mut merged, v, d2);
                    continue;
                }
                (true, true) => {}
            }
            contested += 1;
            match strategy {
                "overwrite" => {
                    put(&mut merged, v, d2);
                    continue;
                }
                "ignore" => {
                    put(&mut merged, v, d1);
                    continue;
                }
                _ => {}
            }
            let mut blended = Vec::with_capacity(comps);
            let mut worst = false;
            if let Some((reference, t1, t2)) = &surfaces {
                let added1 = (curvature_at(t1, side, x, y) - curvature_at(reference, side, x, y)).max(0.0);
                let added2 = (curvature_at(t2, side, x, y) - curvature_at(reference, side, x, y)).max(0.0);
                let w1 = d1[0].abs() as f64 * (1.0 + added1 * 8.0);
                let w2 = d2[0].abs() as f64 * (1.0 + added2 * 8.0);
                let (value, sev) = weighted_delta(d1[0], d2[0], w1, w2, p);
                blended.push(value);
                worst = sev;
            } else {
                for (a, b) in d1.iter().zip(d2) {
                    let (value, sev) = average_delta(*a, *b, p);
                    blended.push(value);
                    worst = worst || sev;
                }
            }
            put(&mut merged, v, &blended);
            if worst {
                major += 1;
                major_vertices.push((x, y));
            } else {
                minor += 1;
            }
        }
    }
    Ok((merged, one, two, contested, minor, major, major_vertices))
}

// ---------------------------------------------------------------------------
// The slope limiter (land/slope.py)

type Coords = (i64, i64);
const LAST: usize = LAND_SIZE - 1;

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

fn movable(cells: &HashMap<Coords, Vec<i64>>, coords: Coords, x: usize, y: usize, auth: &HashSet<Coords>) -> bool {
    if auth.contains(&coords) {
        return false;
    }
    twins(coords, x, y).iter().all(|(t, _, _)| cells.contains_key(t) && !auth.contains(t))
}

#[derive(Default)]
struct SlopeTally {
    adjusted: usize,
    pinned: usize,
    worst_excess: i64,
    touched: HashSet<Coords>,
}

#[allow(clippy::too_many_arguments)]
fn shift(
    cells: &mut HashMap<Coords, Vec<i64>>,
    coords: Coords,
    x: usize,
    y: usize,
    delta: i64,
    r: &mut SlopeTally,
    auth: &HashSet<Coords>,
) {
    if delta == 0 {
        return;
    }
    if !movable(cells, coords, x, y, auth) {
        r.pinned += 1;
        return;
    }
    if let Some(g) = cells.get_mut(&coords) {
        g[y * LAND_SIZE + x] += delta;
    }
    r.adjusted += 1;
    r.touched.insert(coords);
    for (t, tx, ty) in twins(coords, x, y) {
        if let Some(g) = cells.get_mut(&t) {
            g[ty * LAND_SIZE + tx] += delta;
            r.touched.insert(t);
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

fn split(a: f64, b: f64, excess: i64) -> (i64, i64) {
    let wa = 1.0 / (1.0 + a * 12.0);
    let wb = 1.0 / (1.0 + b * 12.0);
    let total = wa + wb;
    if total <= 0.0 {
        let share = excess.div_euclid(2);
        return (share, excess - share);
    }
    let share_a = (round_half_even(excess as f64 * wa / total) as i64).clamp(0, excess);
    (share_a, excess - share_a)
}

/// `limit_slopes`: make every merged cell representable in `VHGT`, in place.
///
/// `cells` maps coordinates to flat 65x65 height lists; the adjusted lists are
/// returned in the same mapping. Returns `(cells, adjusted, pinned, worst_excess,
/// passes, excessive_at_end, cells_touched)`; `excessive_at_end` is zero when a pass
/// found nothing to do.
#[pyfunction]
#[pyo3(signature = (cells, limit, max_passes, authoritative, use_curvature=false))]
#[allow(clippy::type_complexity)]
fn limit_slopes(
    cells: HashMap<Coords, Vec<i64>>,
    limit: i64,
    max_passes: usize,
    authoritative: HashSet<Coords>,
    use_curvature: bool,
) -> PyResult<(HashMap<Coords, Vec<i64>>, usize, usize, i64, usize, bool, HashSet<Coords>)> {
    let mut cells = cells;
    for (c, g) in &cells {
        if g.len() != LAND_SIZE * LAND_SIZE {
            return Err(PyValueError::new_err(format!("cell {c:?} has {} heights, expected 4225", g.len())));
        }
    }
    let mut r = SlopeTally::default();
    let structure: HashMap<Coords, Vec<f64>> =
        if use_curvature { cells.iter().map(|(c, g)| (*c, structure_map(g))).collect() } else { HashMap::new() };
    let flat = vec![0.0; LAND_SIZE * LAND_SIZE];
    let mut in_order: Vec<Coords> = cells.keys().copied().collect();
    in_order.sort();
    let all = pairs();
    let mut passes = 0;
    let mut converged_pass = false;

    for attempt in 1..=max_passes {
        passes = attempt;
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
                let ma = movable(&cells, coords, ax, ay, &authoritative);
                let mb = movable(&cells, coords, bx, by, &authoritative);
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
                // As in the Python: `_shift` is called without the authoritative set,
                // so an end found movable above is moved.
                let none = HashSet::new();
                shift(&mut cells, coords, ax, ay, direction * share_a, &mut r, &none);
                shift(&mut cells, coords, bx, by, -direction * share_b, &mut r, &none);
            }
        }
        if excessive == 0 {
            converged_pass = true;
            break;
        }
    }
    Ok((cells, r.adjusted, r.pinned, r.worst_excess, passes, converged_pass, r.touched))
}

/// `count_unencodable`: how many steps exceed what `VHGT` can store.
#[pyfunction]
fn count_unencodable(cells: Vec<Vec<i64>>, limit: i64) -> usize {
    let all = pairs();
    cells
        .iter()
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

// ---------------------------------------------------------------------------
// Normals (land/heights.py, land/pipeline.py)

/// `_to_signed_byte`: saturate to a signed byte, NaN to zero, truncating.
fn to_signed_byte(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else if v >= 127.0 {
        127
    } else if v <= -128.0 {
        -128
    } else {
        v.trunc() as i64
    }
}

/// `vertex_normals_from_heights`, flat: 65x65 heights in, 65x65x3 normals out.
fn normals_from_heights(h: &[f64]) -> Vec<i64> {
    let step = 128.0 / HEIGHT_SCALE;
    let limit = LAND_SIZE - 1;
    let fin = |v: f64| if v.is_finite() { v } else { 0.0 };
    let mut out = Vec::with_capacity(h.len() * 3);
    for y in 0..LAND_SIZE {
        let fy = if y == limit { y - 1 } else { y };
        for x in 0..LAND_SIZE {
            let fx = if x == limit { x - 1 } else { x };
            let here = fin(h[fy * LAND_SIZE + fx]) / HEIGHT_SCALE;
            let east = fin(h[fy * LAND_SIZE + fx + 1]) / HEIGHT_SCALE;
            let north = fin(h[(fy + 1) * LAND_SIZE + fx]) / HEIGHT_SCALE;
            let nx = -(east - here) * step;
            let ny = -(north - here) * step;
            let nz = step * step;
            // `** 0.5` in the Python is `pow`, which is not always `sqrt` to the bit.
            let length = (nx * nx + ny * ny + nz * nz).powf(0.5);
            if length == 0.0 {
                out.extend_from_slice(&[0, 0, 127]);
                continue;
            }
            let unit = length / 127.0;
            out.push(to_signed_byte(nx / unit));
            out.push(to_signed_byte(ny / unit));
            out.push(to_signed_byte(nz / unit));
        }
    }
    out
}

/// Normals for a 65x65 height grid (`heights.vertex_normals_from_heights`), as rows
/// of `(x, y, z)`.
#[pyfunction]
fn vertex_normals_from_heights(rows: Vec<Vec<f64>>) -> PyResult<Vec<Vec<(i64, i64, i64)>>> {
    if rows.len() != LAND_SIZE {
        return Err(PyValueError::new_err(format!("expected {LAND_SIZE} rows, got {}", rows.len())));
    }
    for (y, r) in rows.iter().enumerate() {
        if r.len() != LAND_SIZE {
            return Err(PyValueError::new_err(format!("row {y} has {} values, expected {LAND_SIZE}", r.len())));
        }
    }
    let flat: Vec<f64> = rows.concat();
    let n = normals_from_heights(&flat);
    Ok(n.chunks(3 * LAND_SIZE).map(|row| row.chunks(3).map(|t| (t[0], t[1], t[2])).collect()).collect())
}

/// One merged cell's normals (`pipeline.resolve_normals`): recomputed from its final
/// heights, keeping the reference's own normal wherever the height did not move and
/// that normal is not `(0, 0, 0)`. Returns the flat normals and how many were kept.
#[pyfunction]
#[pyo3(signature = (heights, original=None, base=None))]
fn resolve_cell_normals(heights: Vec<i64>, original: Option<Vec<i64>>, base: Option<Vec<i64>>) -> PyResult<(Vec<i64>, usize)> {
    if heights.len() != LAND_SIZE * LAND_SIZE {
        return Err(PyValueError::new_err(format!("expected {LAND_SIZE} rows, got {}", heights.len() / LAND_SIZE)));
    }
    let hf: Vec<f64> = heights.iter().map(|&v| v as f64).collect();
    let mut normals = normals_from_heights(&hf);
    let mut preserved = 0;
    if let (Some(orig), Some(base)) = (original, base)
        && base.len() == heights.len()
        && orig.len() >= normals.len()
    {
        for i in 0..heights.len() {
            if heights[i] != base[i] {
                continue;
            }
            let t = &orig[i * 3..i * 3 + 3];
            if t == [0, 0, 0] {
                continue;
            }
            normals[i * 3..i * 3 + 3].copy_from_slice(t);
            preserved += 1;
        }
    }
    Ok((normals, preserved))
}

// ---------------------------------------------------------------------------
// Decoding (tes3fields/landscape.py)

/// `VHGT` to absolute heights, flat and truncated to ints as `LandscapeLayers`
/// holds them: `int(height * 8)` of the doubly cumulative deltas from `offset`.
#[pyfunction]
fn decode_heights(raw: &[u8], offset: f64) -> PyResult<Vec<i64>> {
    let n = LAND_SIZE * LAND_SIZE;
    if raw.len() < n {
        return Err(PyValueError::new_err(format!("VHGT height data is {} bytes, needs {n}", raw.len())));
    }
    let mut out = Vec::with_capacity(n);
    let mut row_height = offset;
    for y in 0..LAND_SIZE {
        let base = y * LAND_SIZE;
        row_height += raw[base] as i8 as f64;
        let mut height = row_height;
        for x in 0..LAND_SIZE {
            if x > 0 {
                height += raw[base + x] as i8 as f64;
            }
            let v = height * HEIGHT_SCALE;
            if !v.is_finite() {
                return Err(PyValueError::new_err("cannot convert float NaN or infinity to integer"));
            }
            out.push(v.trunc() as i64);
        }
    }
    Ok(out)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RelativeGrid>()?;
    m.add_function(wrap_pyfunction!(merge_grids, m)?)?;
    m.add_function(wrap_pyfunction!(limit_slopes, m)?)?;
    m.add_function(wrap_pyfunction!(count_unencodable, m)?)?;
    m.add_function(wrap_pyfunction!(vertex_normals_from_heights, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_cell_normals, m)?)?;
    m.add_function(wrap_pyfunction!(decode_heights, m)?)?;
    Ok(())
}
