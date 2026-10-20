//! The load order, read by `ordercore` (viewer-shell/ordercore) - the reader the
//! viewer's engine uses - so the Editor and the viewer read a setup the same way.

use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::PyDict;

fn text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `openmw_cfg(path, userdata=None, userconfig=None)`: an openmw.cfg and every cfg it
/// chains, by OpenMW's documented rules (`ordercore::cfg`).
///
/// `userdata` / `userconfig` stand in for those tokens' folders (tests); left out, they
/// are this machine's (a Flatpak OpenMW's when the cfg is in its sandbox).
///
/// Returns a dict: `files`, `data` (lowest priority first, `data-local` last),
/// `missing_data`, `data_local`, `content`, `groundcover`, `archives`, `user_data`,
/// `resources`, `warnings`, `plugins` (the load order) and `located` (each plugin with
/// its file, or None).
#[pyfunction]
#[pyo3(signature = (path, userdata=None, userconfig=None))]
fn openmw_cfg(
    py: Python<'_>,
    path: PathBuf,
    userdata: Option<PathBuf>,
    userconfig: Option<PathBuf>,
) -> PyResult<Py<PyDict>> {
    let tokens = if userdata.is_some() || userconfig.is_some() {
        ordercore::cfg::Tokens { userdata, userconfig, local: None, global: None }
    } else {
        ordercore::cfg::Tokens::for_cfg(&path)
    };
    let (c, plugins, located) = py.detach(|| -> std::io::Result<_> {
        let c = ordercore::cfg::read(&path, &tokens)?;
        let plugins = c.plugins();
        let located = c.locate(&plugins);
        Ok((c, plugins, located))
    })?;
    let paths = |ps: &[PathBuf]| ps.iter().map(|p| text(p)).collect::<Vec<_>>();
    let d = PyDict::new(py);
    d.set_item("files", paths(&c.files))?;
    d.set_item("data", paths(&c.data))?;
    d.set_item("missing_data", paths(&c.missing_data))?;
    d.set_item("data_local", c.data_local.as_deref().map(text))?;
    d.set_item("content", &c.content)?;
    d.set_item("groundcover", &c.groundcover)?;
    d.set_item("archives", &c.archives)?;
    d.set_item("user_data", c.user_data.as_deref().map(text))?;
    d.set_item("resources", c.resources.as_deref().map(text))?;
    d.set_item("warnings", &c.warnings)?;
    d.set_item("plugins", plugins)?;
    let located: Vec<(String, Option<String>)> = located.into_iter().map(|(n, p)| (n, p.as_deref().map(text))).collect();
    d.set_item("located", located)?;
    Ok(d.unbind())
}

/// A cell named the one way (`ordercore::cells`): any spelling - `int:Name`, `x,y`,
/// `(x, y)`, a bare name - to a dict of `fold` (the key every spelling of the cell
/// shares), `spec` (the viewer's spelling), `pool` (the Editor pool's) and `grid` (an
/// exterior's square, else None); None for an empty name.
#[pyfunction]
fn cell_key(py: Python<'_>, spec: &str) -> PyResult<Option<Py<PyDict>>> {
    let Some(k) = ordercore::cells::CellKey::parse(spec) else { return Ok(None) };
    let d = PyDict::new(py);
    d.set_item("fold", k.fold())?;
    d.set_item("spec", k.spec())?;
    d.set_item("pool", k.pool_key())?;
    d.set_item("grid", k.grid())?;
    Ok(Some(d.unbind()))
}

/// Which of a reference's versions stands (`ordercore::refs::winner`): given whether each
/// version, in load order, deletes it, the standing version's position, whether it is
/// deleted, and the positions the game ignores (versions after a deletion that do not
/// delete it again); None for no versions.
#[pyfunction]
fn ref_winner(deleted: Vec<bool>) -> Option<(usize, bool, Vec<usize>)> {
    let versions: Vec<ordercore::refs::Version> =
        deleted.into_iter().enumerate().map(|(plugin, deleted)| ordercore::refs::Version { plugin, deleted }).collect();
    ordercore::refs::winner(&versions).map(|w| (w.at, w.deleted, w.ignored))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(openmw_cfg, m)?)?;
    m.add_function(wrap_pyfunction!(cell_key, m)?)?;
    m.add_function(wrap_pyfunction!(ref_winner, m)?)?;
    Ok(())
}
