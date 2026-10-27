//! The load-order lint (`lint_plugins` in `wraithguard_toolkit.py`), from `toolscore::lint`
//! (`viewer-shell/toolscore`, shared with the viewer's Tools page).
//!
//! - `lint_order(order, paths, subset, origins)`: the whole lint - the plugins read side by
//!   side, the load order folded, the warnings worded - and its counts.
//! - `PluginLinter`: one plugin's facts, as the parity tests read them.
//! - `backup_scan(roots, max_depth)`: the backups the tools leave (`toolscore::backups`).
//! - `master_remove` / `master_rename`: a plugin's master removed or renamed
//!   (`toolscore::masters`).
//! - `save_content_files(data)`: a save's content files (`toolscore::save`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use toolscore::lint::{Event, Linter};

/// Reads one plugin's records into the facts the lint checks consume.
#[pyclass(frozen)]
struct PluginLinter {
    inner: Linter,
}

#[pymethods]
impl PluginLinter {
    /// `evil_gmsts` maps a lower-cased setting name to `(value tag, raw value)`; the function
    /// lists are the Tribunal and Bloodmoon script functions; `skip_cells` are lower-cased
    /// cell names left out of the pathgrid check.
    #[new]
    fn new(
        evil_gmsts: HashMap<String, (String, Vec<u8>)>,
        tribunal_funcs: Vec<String>,
        bloodmoon_funcs: Vec<String>,
        skip_cells: BTreeSet<String>,
    ) -> Self {
        let evil = evil_gmsts.into_iter().map(|(k, (t, v))| (k, (t.into_bytes(), v))).collect();
        Self { inner: Linter::new(evil, &tribunal_funcs, &bloodmoon_funcs, skip_cells) }
    }

    /// One plugin's lint facts. `is_custom` turns on the header and expansion-call checks,
    /// which apply only to the user's own mods.
    ///
    /// Returns a dict: `events` (in record order: `("header", [missing])` or `("cell", name,
    /// cell_id, fog_bug)`), `evil_gmsts`, `pathgrids`, `masters`, `tribunal`, `bloodmoon`.
    fn scan<'py>(&self, py: Python<'py>, raw: &[u8], is_custom: bool) -> PyResult<Bound<'py, PyDict>> {
        let facts = py.detach(|| self.inner.facts(raw, is_custom));
        let out = PyDict::new(py);
        let events = PyList::empty(py);
        for e in facts.events {
            match e {
                Event::Header(missing) => events.append(("header", missing))?,
                Event::Cell { name, cell_id, fog_bug } => events.append(PyTuple::new(
                    py,
                    [
                        "cell".into_pyobject(py)?.into_any(),
                        name.into_pyobject(py)?.into_any(),
                        cell_id.into_pyobject(py)?.into_any(),
                        pyo3::types::PyBool::new(py, fog_bug).to_owned().into_any(),
                    ],
                )?)?,
            }
        }
        out.set_item("events", events)?;
        out.set_item("evil_gmsts", facts.evil_gmsts)?;
        out.set_item("pathgrids", facts.pathgrids)?;
        out.set_item("masters", facts.masters.into_iter().collect::<Vec<_>>())?;
        out.set_item("tribunal", facts.tribunal.into_iter().collect::<Vec<_>>())?;
        out.set_item("bloodmoon", facts.bloodmoon.into_iter().collect::<Vec<_>>())?;
        Ok(out)
    }
}

/// `lint_order(order, paths, subset, origins)`: `lint_plugins`, whole. `paths` is each
/// plugin's file (`None`: not found); `origins` maps a lower-cased plugin name to its tag.
/// Returns `(warnings, stats)`.
#[pyfunction]
fn lint_order<'py>(
    py: Python<'py>,
    order: Vec<String>,
    paths: Vec<Option<String>>,
    subset: Vec<String>,
    origins: BTreeMap<String, String>,
) -> PyResult<(Vec<String>, Bound<'py, PyDict>)> {
    if paths.len() != order.len() {
        return Err(PyValueError::new_err("lint_order: one path per plugin"));
    }
    let at: HashMap<String, PathBuf> = order.iter().zip(&paths).filter_map(|(n, p)| Some((n.clone(), PathBuf::from(p.as_ref()?)))).collect();
    let find = move |name: &str| at.get(name).cloned();
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
    let (warnings, stats) = py.detach(|| toolscore::lint::lint_order(&order, &find, &subset, &origins, threads));
    let s = PyDict::new(py);
    s.set_item("scanned", stats.scanned)?;
    s.set_item("unreadable", stats.unreadable)?;
    s.set_item("warnings", stats.warnings)?;
    s.set_item("interior_cells", stats.interior_cells)?;
    s.set_item("pathgrids", stats.pathgrids)?;
    Ok((warnings, s))
}

/// `backup_scan(roots, max_depth)`: `scan_backups` (`toolscore::backups`): `[(backup,
/// original, kind)]`, by path without case.
#[pyfunction]
fn backup_scan(py: Python<'_>, roots: Vec<String>, max_depth: usize) -> Vec<(String, String, String)> {
    let roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    py.detach(|| toolscore::backups::scan(&roots, max_depth))
        .into_iter()
        .map(|b| (b.path.to_string_lossy().into_owned(), b.original.to_string_lossy().into_owned(), b.kind))
        .collect()
}

/// `master_remove(data, master)`: `remove_master_from_bytes` (`toolscore::masters`):
/// `(bytes, {master, removed, references_dropped, dropped_by_cell: [(cell, n)],
/// references_remapped, remaining_masters})`.
#[pyfunction]
fn master_remove<'py>(py: Python<'py>, data: &[u8], master: &str) -> PyResult<(Bound<'py, pyo3::types::PyBytes>, Bound<'py, PyDict>)> {
    let (out, r) = py.detach(|| toolscore::masters::remove_master(data, master)).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let d = PyDict::new(py);
    d.set_item("master", r.master)?;
    d.set_item("removed", r.removed)?;
    d.set_item("references_dropped", r.references_dropped)?;
    d.set_item("dropped_by_cell", r.dropped_by_cell)?;
    d.set_item("references_remapped", r.references_remapped)?;
    d.set_item("remaining_masters", r.remaining_masters)?;
    Ok((pyo3::types::PyBytes::new(py, &out), d))
}

/// `master_rename(data, old, new, new_size)`: `rename_master_in_bytes`: `(bytes, {renamed,
/// remaining_masters})`; a `ValueError` when `new` is already another master.
#[pyfunction]
fn master_rename<'py>(py: Python<'py>, data: &[u8], old: &str, new: &str, new_size: u64) -> PyResult<(Bound<'py, pyo3::types::PyBytes>, Bound<'py, PyDict>)> {
    let (out, r) = py.detach(|| toolscore::masters::rename_master(data, old, new, new_size)).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let d = PyDict::new(py);
    d.set_item("renamed", r.renamed)?;
    d.set_item("remaining_masters", r.remaining_masters)?;
    Ok((pyo3::types::PyBytes::new(py, &out), d))
}

/// `save_content_files(data)`: the content files a save names (`read_savegame_content_files`);
/// a `ValueError` saying why when they cannot be read.
#[pyfunction]
fn save_content_files(data: &[u8]) -> PyResult<Vec<String>> {
    toolscore::save::content_files(data).map_err(PyValueError::new_err)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(save_content_files, m)?)?;
    m.add_function(wrap_pyfunction!(master_remove, m)?)?;
    m.add_function(wrap_pyfunction!(master_rename, m)?)?;
    m.add_class::<PluginLinter>()?;
    m.add_function(wrap_pyfunction!(lint_order, m)?)?;
    m.add_function(wrap_pyfunction!(backup_scan, m)?)?;
    Ok(())
}
