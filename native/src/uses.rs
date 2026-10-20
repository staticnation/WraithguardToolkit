//! The Construction Set's Use Report over a load order, for the Python Editor: the scan is
//! `editcore::uses::report` (moved there in port step 4.1, slice 5b, so the viewer runs
//! the same one): each plugin mapped, parsed only when its bytes hold the id, on threads.

use std::path::PathBuf;

use pyo3::prelude::*;

/// One record that uses the id: `(plugin index, type, tag, key, paths, count, wins)`.
pub type Use = (usize, String, String, String, Vec<String>, usize, bool);

/// `use_report(paths, record_type, id)`: every record of the load order that names `id`
/// (other than the record itself, `record_type`), in load order; Python released.
#[pyfunction]
fn use_report(py: Python<'_>, paths: Vec<PathBuf>, record_type: String, id: String) -> Vec<Use> {
    py.detach(|| editcore::uses::report(&paths, &record_type, &id))
        .into_iter()
        .map(|u| (u.plugin, u.record_type, u.tag, u.key, u.paths, u.count, u.wins))
        .collect()
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(use_report, m)?)?;
    Ok(())
}
