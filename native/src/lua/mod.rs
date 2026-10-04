//! OpenMW Lua scripts: the API read from the setup's own OpenMW install, Teal
//! declarations written from it, and scripts checked against them by the Teal compiler.
//!
//! - [`ldt`]: OpenMW's LDT doc comments (`resources/lua_api`, `resources/vfs`) -> a model.
//! - [`teal`]: that model -> `.d.tl` files in a cache folder (or stubs with no install).
//! - [`check`]: `.lua`/`.tl` files checked by tl, embedded through htl.
//!
//! OpenMW's API documentation is GPLv3 and is only ever read from the user's install;
//! nothing of it is compiled in or shipped, so this crate stays MIT.

pub mod check;
pub mod ldt;
pub mod teal;

use std::path::PathBuf;

use pyo3::exceptions::PyOSError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// A function's signature as text, for the API browser and messages: `look(from, options?)`.
fn signature(f: &ldt::Func) -> String {
    let ps: Vec<String> = f.params.iter().map(|p| format!("{}{}", p.name, if p.optional { "?" } else { "" })).collect();
    format!("{}{}({})", if f.method { ":" } else { "" }, f.name, ps.join(", "))
}

/// `lua_api(resources)`: what an install's documentation says - `{"version", "modules",
/// "members"}` (`members`: each declared type's member names by its Teal name) - or None
/// when the folder is not an OpenMW `resources` folder with a Lua API.
#[pyfunction]
fn lua_api<'py>(py: Python<'py>, resources: PathBuf) -> PyResult<Option<Bound<'py, PyDict>>> {
    let Some(api) = py.detach(|| ldt::read_resources(&resources)) else {
        return Ok(None);
    };
    let out = PyDict::new(py);
    out.set_item("version", api.version.clone())?;
    let mods = PyList::empty(py);
    for m in &api.modules {
        let d = PyDict::new(py);
        d.set_item("name", &m.name)?;
        d.set_item("require", &m.require)?;
        d.set_item("interface", m.interface.clone())?;
        d.set_item("contexts", m.contexts.clone())?;
        d.set_item("file", &m.file)?;
        let own = m.types.get(&m.name);
        let fields: Vec<String> = own.map(|t| t.fields.iter().map(|f| f.name.clone()).collect()).unwrap_or_default();
        let funcs: Vec<String> = own.map(|t| t.functions.iter().map(signature).collect()).unwrap_or_default();
        d.set_item("fields", fields)?;
        d.set_item("functions", funcs)?;
        d.set_item("types", m.types.keys().filter(|k| *k != &m.name).cloned().collect::<Vec<_>>())?;
        mods.append(d)?;
    }
    out.set_item("modules", mods)?;
    out.set_item("members", teal::members(&api))?;
    Ok(Some(out))
}

/// `lua_write_declarations(out, resources=None, packages=())`: writes Teal declarations
/// into `out` (emptied first) from the install's documentation, or, with no usable
/// `resources`, stubs for `packages`. Returns `{"source": "openmw"|"stubs", "version",
/// "packages"}`.
#[pyfunction]
#[pyo3(signature = (out, resources=None, packages=Vec::new()))]
fn lua_write_declarations<'py>(
    py: Python<'py>,
    out: PathBuf,
    resources: Option<PathBuf>,
    packages: Vec<String>,
) -> PyResult<Bound<'py, PyDict>> {
    let res = py.detach(|| -> std::io::Result<(&'static str, Option<String>, usize)> {
        if let Some(api) = resources.as_deref().and_then(ldt::read_resources) {
            let n = teal::write_declarations(&api, &out)?;
            return Ok(("openmw", api.version, n));
        }
        Ok(("stubs", None, teal::write_stubs(&packages, &out)?))
    });
    let (source, version, n) = res.map_err(|e| PyOSError::new_err(format!("{}: {e}", out.display())))?;
    let d = PyDict::new(py);
    d.set_item("source", source)?;
    d.set_item("version", version)?;
    d.set_item("packages", n)?;
    Ok(d)
}

/// `lua_check(paths, include)`: each file checked by Teal. `include` are module folders,
/// lowest priority first (the data folders in load order, then the declarations). One
/// entry per path: a list of `{"line", "col", "severity", "kind", "rule", "message"}`, or
/// a string saying why the file could not be checked.
#[pyfunction]
fn lua_check<'py>(py: Python<'py>, paths: Vec<PathBuf>, include: Vec<PathBuf>) -> PyResult<Bound<'py, PyList>> {
    let res = py
        .detach(|| crate::guarded(|| check::check(&paths, &include).map_err(|e| std::io::Error::other(format!("{e:#}")))))
        .map_err(|e| PyOSError::new_err(format!("the Teal checker did not start: {e}")))?;
    let out = PyList::empty(py);
    for r in res {
        match r {
            Ok(diags) => {
                let l = PyList::empty(py);
                for d in diags {
                    let x = PyDict::new(py);
                    x.set_item("line", d.line)?;
                    x.set_item("col", d.col)?;
                    x.set_item("severity", d.severity)?;
                    x.set_item("kind", d.kind)?;
                    x.set_item("rule", d.rule)?;
                    x.set_item("message", d.message)?;
                    l.append(x)?;
                }
                out.append(l)?;
            }
            Err(e) => out.append(e)?,
        }
    }
    Ok(out)
}

/// The scripts a content file registers in its LUAL records (OpenMW's
/// `LuaScriptsCfg`), read with the tes3 crate's `ScriptConfigList`:
/// `[(vfs path, flags word, record types, per object)]` in record order - `per object`
/// when it is attached to particular records or references (LUAR/LUAI). Only LUAL records
/// are read. ValueError for a file the crate cannot read, OSError when unreadable.
#[pyfunction]
fn lual_scripts(py: Python<'_>, path: PathBuf) -> PyResult<Vec<(String, u32, Vec<String>, bool)>> {
    use tes3::esp::{Plugin, TES3Object};
    let got = py.detach(|| -> std::io::Result<Vec<(String, u32, Vec<String>, bool)>> {
        let bytes = std::fs::read(&path)?;
        let mut plugin = Plugin::new();
        crate::guarded(|| plugin.load_bytes_filtered(&bytes, |t| &t == b"LUAL"))?;
        let mut out = Vec::new();
        for obj in &plugin.objects {
            if let TES3Object::ScriptConfigList(list) = obj {
                for sc in &list.scripts {
                    let per_object = !sc.records.is_empty() || !sc.instances.is_empty();
                    out.push((sc.path.clone(), sc.flags.bits(), sc.types.clone(), per_object));
                }
            }
        }
        Ok(out)
    });
    got.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => PyOSError::new_err(e.to_string()),
        _ => pyo3::exceptions::PyValueError::new_err(e.to_string()),
    })
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(lua_api, m)?)?;
    m.add_function(wrap_pyfunction!(lua_write_declarations, m)?)?;
    m.add_function(wrap_pyfunction!(lua_check, m)?)?;
    m.add_function(wrap_pyfunction!(lual_scripts, m)?)?;
    m.add("TEAL_VERSION", "0.24.8")?;
    Ok(())
}
