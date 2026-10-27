//! The numeric core of Merged Lands (`wraithguard/land/`), for Python.
//!
//! `wraithguard/land/` is our port of Merged Lands and its OpenMW fork, with our own
//! fixes. Its per-vertex arithmetic - run millions of times over a large landmass - lives
//! in `viewer-shell/landcore`, which the viewer's Merge Lands runs whole; this module hands
//! the same functions to the Python, so both merge to the unit:
//!
//! - [`RelativeGrid`]: a reference grid plus one plugin's deltas (`land/diff.py`),
//!   with the same methods, so every caller keeps working (`landcore::grid`);
//! - [`merge_grids`]: the per-vertex loop of `land/merge.py`'s `merge_layer`
//!   (`landcore::merge`);
//! - [`limit_slopes`]: `land/slope.py`'s limiter (`landcore::slope`);
//! - [`vertex_normals_from_heights`], [`resolve_cell_normals`]: `land/heights.py`'s normals
//!   and `land/pipeline.py`'s `resolve_normals` (`landcore::heights`);
//! - [`decode_heights`]: `tes3fields/landscape.py`'s `VHGT` decoder.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use landcore::grid::{self, LAND_SIZE};
use landcore::merge::ConflictParams;
use landcore::meta::Strategy;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

// ---------------------------------------------------------------------------
// RelativeGrid

/// A reference grid plus the deltas one plugin applied to it (`land/diff.py`).
///
/// Values are flat and interleaved: a 65x65 grid of three-component normals is one
/// list of 12,675 integers. One changed flag per *vertex*.
#[pyclass(module = "wraithguard_native", skip_from_py_object)]
#[derive(Clone)]
pub struct RelativeGrid {
    inner: grid::RelativeGrid,
}

impl RelativeGrid {
    fn make(reference: Vec<i64>, side: usize, components: usize) -> PyResult<Self> {
        Ok(Self { inner: grid::RelativeGrid::new(reference, side, components).map_err(PyValueError::new_err)? })
    }

    fn vertex(&self, x: i64, y: i64) -> PyResult<usize> {
        let (s, n) = (self.inner.side as i64, (self.inner.side * self.inner.side) as i64);
        let i = y * s + x;
        // Python indexes a list from the end with a negative index; kept.
        let i = if i < 0 { i + n } else { i };
        if i < 0 || i >= n {
            return Err(PyIndexError::new_err("list index out of range"));
        }
        Ok(i as usize)
    }

    fn index(&self, x: i64, y: i64, component: i64) -> PyResult<usize> {
        let n = self.inner.reference.len() as i64;
        let i = (y * self.inner.side as i64 + x) * self.inner.components as i64 + component;
        let i = if i < 0 { i + n } else { i };
        if i < 0 || i >= n {
            return Err(PyIndexError::new_err("list index out of range"));
        }
        Ok(i as usize)
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
        Ok(Self { inner: grid::RelativeGrid::from_difference(reference, &plugin, side, components).map_err(PyValueError::new_err)? })
    }

    #[getter]
    fn side(&self) -> usize {
        self.inner.side
    }

    #[getter]
    fn components(&self) -> usize {
        self.inner.components
    }

    #[pyo3(signature = (x, y, component=0))]
    fn offset_of(&self, x: i64, y: i64, component: i64) -> i64 {
        (y * self.inner.side as i64 + x) * self.inner.components as i64 + component
    }

    #[pyo3(signature = (x, y, component=0))]
    fn value_at(&self, x: i64, y: i64, component: i64) -> PyResult<i64> {
        let i = self.index(x, y, component)?;
        Ok(self.inner.reference[i] + self.inner.delta[i])
    }

    #[pyo3(signature = (x, y, component=0))]
    fn delta_at(&self, x: i64, y: i64, component: i64) -> PyResult<i64> {
        Ok(self.inner.delta[self.index(x, y, component)?])
    }

    fn has_difference(&self, x: i64, y: i64) -> PyResult<bool> {
        Ok(self.inner.changed[self.vertex(x, y)?])
    }

    fn set_value(&mut self, x: i64, y: i64, values: Vec<i64>) -> PyResult<()> {
        if values.len() != self.inner.components {
            return Err(PyValueError::new_err(format!(
                "expected {} component(s), got {}",
                self.inner.components,
                values.len()
            )));
        }
        let mut changed = false;
        for (c, v) in values.iter().enumerate() {
            let i = self.index(x, y, c as i64)?;
            let d = v - self.inner.reference[i];
            self.inner.delta[i] = d;
            changed = changed || d != 0;
        }
        let v = self.vertex(x, y)?;
        self.inner.changed[v] = changed;
        Ok(())
    }

    fn deltas_at<'py>(&self, py: Python<'py>, x: i64, y: i64) -> PyResult<Bound<'py, PyTuple>> {
        let start = ((y * self.inner.side as i64 + x) * self.inner.components as i64).max(0) as usize;
        let end = (start + self.inner.components).min(self.inner.delta.len());
        PyTuple::new(py, &self.inner.delta[start.min(end)..end])
    }

    fn set_deltas(&mut self, x: i64, y: i64, deltas: Vec<i64>) -> PyResult<()> {
        if deltas.len() != self.inner.components {
            return Err(PyValueError::new_err(format!(
                "expected {} delta(s), got {}",
                self.inner.components,
                deltas.len()
            )));
        }
        let start = self.index(x, y, 0)?;
        let mut changed = false;
        for (o, d) in deltas.iter().enumerate() {
            self.inner.delta[start + o] = *d;
            changed = changed || *d != 0;
        }
        let v = self.vertex(x, y)?;
        self.inner.changed[v] = changed;
        Ok(())
    }

    /// A copy of the reference values, with no deltas applied.
    fn to_flat_reference(&self) -> Vec<i64> {
        self.inner.reference.clone()
    }

    /// Discard a vertex's delta, returning it to the reference.
    fn clear(&mut self, x: i64, y: i64) -> PyResult<()> {
        for c in 0..self.inner.components {
            let i = self.index(x, y, c as i64)?;
            self.inner.delta[i] = 0;
        }
        let v = self.vertex(x, y)?;
        self.inner.changed[v] = false;
        Ok(())
    }

    #[getter]
    fn is_modified(&self) -> bool {
        self.inner.is_modified()
    }

    #[getter]
    fn num_differences(&self) -> usize {
        self.inner.num_differences()
    }

    fn changed_vertices(&self) -> Vec<(usize, usize)> {
        self.inner.changed_vertices()
    }

    /// The plugin's grid as flat values: reference plus delta.
    fn to_flat(&self) -> Vec<i64> {
        self.inner.flat()
    }

    fn to_rows(&self) -> PyResult<Vec<Vec<i64>>> {
        if self.inner.components != 1 {
            return Err(PyValueError::new_err("to_rows is only meaningful for single-component grids"));
        }
        Ok(self.inner.flat().chunks(self.inner.side).map(|r| r.to_vec()).collect())
    }

    fn __copy__(&self) -> Self {
        self.clone()
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        self.clone()
    }
}

// ---------------------------------------------------------------------------
// Merging one layer (land/merge.py)

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
    let chosen = match strategy {
        "overwrite" => Strategy::Overwrite,
        "ignore" => Strategy::Ignore,
        "curvature" => Strategy::Curvature,
        _ => Strategy::Resolve,
    };
    let p = ConflictParams { minor_threshold_pct, minor_threshold_min, minor_threshold_max };
    let (merged, r) = landcore::merge::merge_grids(&first.inner, &second.inner, chosen, p).map_err(PyValueError::new_err)?;
    Ok((RelativeGrid { inner: merged }, r.taken_from_one, r.taken_from_two, r.contested, r.minor, r.major, r.major_vertices))
}

// ---------------------------------------------------------------------------
// The slope limiter (land/slope.py)

type Coords = (i64, i64);

/// `limit_slopes`: make every merged cell representable in `VHGT`.
///
/// `cells` maps coordinates to flat 65x65 height lists; the adjusted lists are
/// returned in the same mapping. Returns `(cells, adjusted, pinned, worst_excess,
/// passes, converged_pass, cells_touched)`; `converged_pass` is true when a pass found
/// nothing to do.
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
    for (c, g) in &cells {
        if g.len() != LAND_SIZE * LAND_SIZE {
            return Err(PyValueError::new_err(format!("cell {c:?} has {} heights, expected 4225", g.len())));
        }
    }
    let mut cells: BTreeMap<Coords, Vec<i64>> = cells.into_iter().collect();
    let auth: BTreeSet<Coords> = authoritative.into_iter().collect();
    let (r, settled) = landcore::slope::limit_slopes_passes(&mut cells, limit, max_passes, use_curvature, &auth);
    Ok((cells.into_iter().collect(), r.adjusted, r.pinned, r.worst_excess, r.passes, settled, r.cells_touched.into_iter().collect()))
}

/// `count_unencodable`: how many steps exceed what `VHGT` can store.
#[pyfunction]
fn count_unencodable(cells: Vec<Vec<i64>>, limit: i64) -> usize {
    landcore::slope::count_unencodable(cells.iter(), limit)
}

// ---------------------------------------------------------------------------
// Normals (land/heights.py, land/pipeline.py)

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
    let n = landcore::heights::normals_from_heights(&flat);
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
    Ok(landcore::heights::resolve_cell_normals(&heights, original.as_deref(), base.as_deref()))
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
    landcore::heights::decode_heights(raw, offset).map_err(PyValueError::new_err)
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
