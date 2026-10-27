//! Conflicts for the Python tools (port step 4.3), from `conflictcore`: the record scan with
//! the native session, the singles, the loose-file scan, and their reports and CSVs. The scans
//! run with the interpreter released.
//!
//! - `conflict_scan(order, paths, subset, mode)`: `(records, stats)` as `detect_conflicts`
//!   (`mode="conflicts"`), `list_subset_singles` (`"mine"`) or `list_other_singles`
//!   (`"other"`) give them with the native session. `paths` is each plugin's file (`None`:
//!   not found), as the caller's `PluginFileIndex` resolves it.
//! - `conflict_report(conflicts, stats, subset_only, limit)`, `conflict_csv(conflicts)`.
//! - `resource_scan(data_dirs, subset_dirs, exclude_exts, compare)`: as
//!   `detect_resource_conflicts`.
//! - `resource_report(conflicts, stats, notes, subset_only, limit)`,
//!   `resource_csv(conflicts, columns)`: `notes` and `columns` are the mesh analysis's, by
//!   conflict.
//! - `mesh_analyse(conflicts, limit)`: as `analyse_mesh_conflicts`, each finding a dict of
//!   `MeshFinding`'s fields (`difference` a dict of `Difference`'s, or `None`).

use std::collections::HashMap;
use std::path::PathBuf;

use conflictcore::mesh;
use conflictcore::records::{self, Conflict, Stats};
use conflictcore::resources::{self, MeshColumns, Resource, ResourceStats};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

fn conflict_dict<'py>(py: Python<'py>, c: &Conflict) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("type", &c.rtype)?;
    d.set_item("id", &c.id)?;
    d.set_item("plugins", &c.plugins)?;
    d.set_item("winner", &c.winner)?;
    d.set_item("involves_subset", c.involves_subset)?;
    d.set_item("deleted_by", &c.deleted_by)?;
    Ok(d)
}

fn conflict_of(d: &Bound<'_, PyAny>) -> PyResult<Conflict> {
    let get = |k: &str| d.get_item(k);
    Ok(Conflict {
        rtype: get("type")?.str()?.to_string(),
        id: get("id")?.str()?.to_string(),
        plugins: get("plugins")?.extract()?,
        winner: get("winner")?.str()?.to_string(),
        involves_subset: get("involves_subset")?.is_truthy()?,
        deleted_by: get("deleted_by")?.extract()?,
    })
}

/// `conflict_scan(order, paths, subset, mode)`.
#[pyfunction]
fn conflict_scan<'py>(
    py: Python<'py>,
    order: Vec<String>,
    paths: Vec<Option<String>>,
    subset: Vec<String>,
    mode: &str,
) -> PyResult<(Vec<Bound<'py, PyDict>>, Bound<'py, PyDict>)> {
    if paths.len() != order.len() {
        return Err(PyValueError::new_err("conflict_scan: one path per plugin"));
    }
    let at: HashMap<String, PathBuf> =
        order.iter().zip(&paths).filter_map(|(n, p)| Some((n.clone(), PathBuf::from(p.as_ref()?)))).collect();
    let find = move |name: &str| at.get(name).cloned();
    let (found, stats) = py.detach(|| {
        let scan = records::scan_touch(&order, &find, &records::plugin_keys);
        match mode {
            "conflicts" => Ok(records::conflicts(&scan, &subset)),
            "mine" => Ok(records::singles(&scan, &subset, true)),
            "other" => Ok(records::singles(&scan, &subset, false)),
            _ => Err(()),
        }
    })
    .map_err(|()| PyValueError::new_err(format!("conflict_scan: no mode {mode:?}")))?;
    let list = found.iter().map(|c| conflict_dict(py, c)).collect::<PyResult<Vec<_>>>()?;
    let s = PyDict::new(py);
    s.set_item("scanned", stats.scanned)?;
    s.set_item("unreadable", &stats.unreadable)?;
    s.set_item("records", stats.records)?;
    s.set_item(if mode == "conflicts" { "conflicts" } else { "singles" }, stats.found)?;
    s.set_item("engine", "native")?;
    let p = PyDict::new(py);
    for (k, v) in &stats.paths {
        p.set_item(k, v)?;
    }
    s.set_item("paths", p)?;
    Ok((list, s))
}

/// `conflict_report(conflicts, stats, subset_only, limit)`: `format_conflict_report`.
#[pyfunction]
fn conflict_report(conflicts: Vec<Bound<'_, PyAny>>, stats: Bound<'_, PyAny>, subset_only: bool, limit: usize) -> PyResult<String> {
    let list = conflicts.iter().map(conflict_of).collect::<PyResult<Vec<_>>>()?;
    let st = Stats {
        scanned: stats.get_item("scanned")?.extract()?,
        unreadable: stats.get_item("unreadable")?.extract()?,
        records: stats.get_item("records")?.extract()?,
        found: stats.get_item("conflicts")?.extract()?,
        paths: Vec::new(),
    };
    Ok(records::report(&list, &st, subset_only, limit))
}

/// `conflict_csv(conflicts)`: the text `write_conflict_csv` writes.
#[pyfunction]
fn conflict_csv(conflicts: Vec<Bound<'_, PyAny>>) -> PyResult<String> {
    let list = conflicts.iter().map(conflict_of).collect::<PyResult<Vec<_>>>()?;
    Ok(records::conflict_csv(&list))
}

fn resource_of(d: &Bound<'_, PyAny>) -> PyResult<Resource> {
    let identical = match d.get_item("identical") {
        Ok(v) if !v.is_none() => Some(v.is_truthy()?),
        _ => None,
    };
    Ok(Resource {
        path: d.get_item("path")?.extract()?,
        providers: d.get_item("providers")?.extract()?,
        winner: d.get_item("winner")?.extract()?,
        involves_subset: d.get_item("involves_subset")?.is_truthy()?,
        identical,
    })
}

/// `resource_scan(data_dirs, subset_dirs, exclude_exts, compare)`.
#[pyfunction]
fn resource_scan<'py>(
    py: Python<'py>,
    data_dirs: Vec<String>,
    subset_dirs: Vec<String>,
    exclude_exts: Vec<String>,
    compare: bool,
) -> PyResult<(Vec<Bound<'py, PyDict>>, Bound<'py, PyDict>)> {
    let (found, stats) = py.detach(|| resources::detect(&data_dirs, &subset_dirs, &exclude_exts, compare));
    let list = found
        .iter()
        .map(|c| {
            let d = PyDict::new(py);
            d.set_item("path", &c.path)?;
            d.set_item("providers", PyList::new(py, &c.providers)?)?;
            d.set_item("winner", &c.winner)?;
            d.set_item("involves_subset", c.involves_subset)?;
            if let Some(same) = c.identical {
                d.set_item("identical", same)?;
            }
            Ok(d)
        })
        .collect::<PyResult<Vec<_>>>()?;
    let s = PyDict::new(py);
    s.set_item("dirs", stats.dirs)?;
    s.set_item("files", stats.files)?;
    s.set_item("conflicts", stats.conflicts)?;
    s.set_item("identical", stats.identical)?;
    s.set_item("differing", stats.differing)?;
    Ok((list, s))
}

/// `resource_report(conflicts, stats, notes, subset_only, limit)`: `format_resource_report`,
/// `notes` the mesh line of each conflict (`""`: none).
#[pyfunction]
fn resource_report(
    conflicts: Vec<Bound<'_, PyAny>>,
    stats: Bound<'_, PyAny>,
    notes: Vec<String>,
    subset_only: bool,
    limit: usize,
) -> PyResult<String> {
    let list = conflicts.iter().map(resource_of).collect::<PyResult<Vec<_>>>()?;
    let num = |k: &str| -> PyResult<usize> {
        match stats.get_item(k) {
            Ok(v) if !v.is_none() => v.extract(),
            _ => Ok(0),
        }
    };
    let st = ResourceStats {
        dirs: num("dirs")?,
        files: num("files")?,
        conflicts: num("conflicts")?,
        identical: num("identical")?,
        differing: num("differing")?,
    };
    let mesh: Vec<Option<MeshColumns>> =
        notes.into_iter().map(|n| Some(MeshColumns { report_note: n, ..MeshColumns::default() })).collect();
    Ok(resources::report(&list, &st, &mesh, subset_only, limit))
}

/// `resource_csv(conflicts, columns)`: the text `write_resource_csv` writes, `columns` each
/// conflict's `(note, lost_collision, lost_animation, triangle_ratio)`.
#[pyfunction]
fn resource_csv(conflicts: Vec<Bound<'_, PyAny>>, columns: Vec<(String, String, String, String)>) -> PyResult<String> {
    let list = conflicts.iter().map(resource_of).collect::<PyResult<Vec<_>>>()?;
    let mesh: Vec<Option<MeshColumns>> = columns
        .into_iter()
        .map(|(note, lost_collision, lost_animation, triangle_ratio)| {
            Some(MeshColumns { note, lost_collision, lost_animation, triangle_ratio, report_note: String::new() })
        })
        .collect();
    Ok(resources::csv(&list, &mesh))
}

fn finding_dict<'py>(py: Python<'py>, f: &mesh::Finding) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("path", &f.path)?;
    d.set_item("unreadable", &f.unreadable)?;
    d.set_item("loser_partial", f.loser_partial)?;
    d.set_item("winner_partial", f.winner_partial)?;
    match &f.difference {
        None => d.set_item("difference", py.None())?,
        Some(x) => {
            let dd = PyDict::new(py);
            dd.set_item("triangle_ratio", x.triangle_ratio)?;
            dd.set_item("lost_collision", x.lost_collision)?;
            dd.set_item("lost_animation", x.lost_animation)?;
            dd.set_item("added_textures", &x.added_textures)?;
            dd.set_item("dropped_textures", &x.dropped_textures)?;
            dd.set_item("unreliable", x.unreliable)?;
            d.set_item("difference", dd)?;
        }
    }
    Ok(d)
}

/// `mesh_analyse(conflicts, limit)`: `analyse_mesh_conflicts`'s findings, one per conflict
/// (`None`: not looked at), and its counts.
#[pyfunction]
fn mesh_analyse<'py>(
    py: Python<'py>,
    conflicts: Vec<Bound<'py, PyAny>>,
    limit: usize,
) -> PyResult<(Vec<Option<Bound<'py, PyDict>>>, Bound<'py, PyDict>)> {
    let list = conflicts.iter().map(resource_of).collect::<PyResult<Vec<_>>>()?;
    let (found, stats) = py.detach(|| mesh::analyse(&list, limit));
    let out = found.iter().map(|f| f.as_ref().map(|f| finding_dict(py, f)).transpose()).collect::<PyResult<Vec<_>>>()?;
    let s = PyDict::new(py);
    s.set_item("analysed", stats.analysed)?;
    s.set_item("findings", stats.findings)?;
    s.set_item("unreadable", stats.unreadable)?;
    Ok((out, s))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(conflict_scan, m)?)?;
    m.add_function(wrap_pyfunction!(conflict_report, m)?)?;
    m.add_function(wrap_pyfunction!(conflict_csv, m)?)?;
    m.add_function(wrap_pyfunction!(resource_scan, m)?)?;
    m.add_function(wrap_pyfunction!(resource_report, m)?)?;
    m.add_function(wrap_pyfunction!(resource_csv, m)?)?;
    m.add_function(wrap_pyfunction!(mesh_analyse, m)?)?;
    Ok(())
}
