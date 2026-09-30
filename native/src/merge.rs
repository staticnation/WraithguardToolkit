//! Whole-plugin merging: greatness7's merge_to_master, called as a library.
//!
//! The crate is vendored under `vendor/merge_to_master` with our stable-Rust patch
//! (`tools/vendor_merge_to_master.py`); this module is the Python face of its two entry
//! points. Each returns the merged master as plugin bytes, which `wraithguard.merge`
//! reads back into its own model, so its Python API is unchanged.

use std::io;
use std::path::PathBuf;

use merge_to_master::{MergeOptions, PluginData};
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// An error out of the merge as Python sees it: OSError when a file could not be read
/// or found, ValueError for anything else (a malformed plugin, a missing master entry).
fn py_err(e: anyhow::Error) -> PyErr {
    let msg = format!("{e:#}");
    let os = e.chain().any(|c| {
        c.downcast_ref::<io::Error>()
            .is_some_and(|io| matches!(io.kind(), io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied))
    });
    if os { PyOSError::new_err(msg) } else { PyValueError::new_err(msg) }
}

/// The merged data as a plugin file's bytes.
fn to_bytes(merged: PluginData) -> anyhow::Result<Vec<u8>> {
    Ok(merged.into_plugin().save_bytes()?)
}

/// Runs a merge off the GIL, with the crate's panics turned into errors.
fn run(py: Python<'_>, f: impl FnOnce() -> anyhow::Result<PluginData> + Send) -> PyResult<Py<PyBytes>> {
    let bytes = py
        .detach(|| match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f().and_then(to_bytes))) {
            Ok(r) => r,
            Err(_) => Err(anyhow::anyhow!("the merge gave up on these files")),
        })
        .map_err(py_err)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

fn options(remove_deleted: bool, apply_moved_references: bool, preserve_duplicate_references: bool) -> MergeOptions {
    MergeOptions { remove_deleted, apply_moved_references, preserve_duplicate_references }
}

/// Merges the plugin at `plugin_path` into the master at `master_path` (its last master;
/// one it does not list is added as its last); returns the merged master as plugin bytes.
#[pyfunction]
#[pyo3(signature = (plugin_path, master_path, *, remove_deleted=false, apply_moved_references=false, preserve_duplicate_references=false))]
fn merge_plugins(
    py: Python<'_>,
    plugin_path: PathBuf,
    master_path: PathBuf,
    remove_deleted: bool,
    apply_moved_references: bool,
    preserve_duplicate_references: bool,
) -> PyResult<Py<PyBytes>> {
    let o = options(remove_deleted, apply_moved_references, preserve_duplicate_references);
    run(py, move || merge_to_master::merge_plugins(&plugin_path, &master_path, o))
}

/// Merges a whole load order (paths in load order) into one master, resolved as the
/// game would see it; returns it as plugin bytes. Plugins are read in parallel.
#[pyfunction]
#[pyo3(signature = (plugin_paths, *, remove_deleted=false, apply_moved_references=false, preserve_duplicate_references=false))]
fn merge_load_order(
    py: Python<'_>,
    plugin_paths: Vec<PathBuf>,
    remove_deleted: bool,
    apply_moved_references: bool,
    preserve_duplicate_references: bool,
) -> PyResult<Py<PyBytes>> {
    let o = options(remove_deleted, apply_moved_references, preserve_duplicate_references);
    run(py, move || merge_to_master::par_merge_load_order(&plugin_paths, o))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(merge_plugins, m)?)?;
    m.add_function(wrap_pyfunction!(merge_load_order, m)?)?;
    Ok(())
}
