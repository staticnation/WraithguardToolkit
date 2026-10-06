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

// The plain-Rust part lives in viewer-shell/luacore, shared with the viewer's engine.
pub use luacore::{analysis, findings, ldt, lexer, mermaid, omwscripts, parser, report, scan, teal};
pub use luacore::{lual_error_is_os, read_lual_scripts};

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

/// `lua_check(paths, include, globals=())`: each file checked by Teal. `include` are module
/// folders, lowest priority first (the data folders in load order, then the declarations);
/// `globals` modules whose globals every file sees (a Cyan project's `global_env_def`). One
/// entry per path: a list of `{"line", "col", "severity", "kind", "rule", "message"}`, or
/// a string saying why the file could not be checked.
#[pyfunction]
#[pyo3(signature = (paths, include, globals=Vec::new()))]
fn lua_check<'py>(
    py: Python<'py>,
    paths: Vec<PathBuf>,
    include: Vec<PathBuf>,
    globals: Vec<String>,
) -> PyResult<Bound<'py, PyList>> {
    let res = py
        .detach(|| {
            crate::guarded(|| {
                check::check_with(&paths, &include, &globals).map_err(|e| std::io::Error::other(format!("{e:#}")))
            })
        })
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
    py.detach(|| read_lual_scripts(&path)).map_err(|e| {
        if lual_error_is_os(&e) {
            PyOSError::new_err(e.to_string())
        } else {
            pyo3::exceptions::PyValueError::new_err(e.to_string())
        }
    })
}

/// An empty Lua data field written as `""` (the editor's, and a hand-made record's) in
/// the form the crate's serde reads: its own encoding of no bytes. Without this the
/// decompressor is handed an empty string and the whole record "does not read".
fn blank_lua_data(v: &mut serde_json::Value) {
    let empty = serde_json::to_value(tes3::esp::PerInstanceConfig::default())
        .ok()
        .and_then(|d| d.get("data").cloned());
    let Some(empty) = empty else { return };
    let fix = |o: &mut serde_json::Value, key: &str| {
        if o.get(key).and_then(|x| x.as_str()) == Some("") {
            o[key] = empty.clone();
        }
    };
    let Some(scripts) = v.get_mut("scripts").and_then(|s| s.as_array_mut()) else { return };
    for script in scripts {
        fix(script, "init_data");
        for list in ["records", "instances"] {
            if let Some(items) = script.get_mut(list).and_then(|x| x.as_array_mut()) {
                for item in items {
                    fix(item, "data");
                }
            }
        }
    }
}

/// `lual_remap(record_json, mapping)`: a LUAL record (`ScriptConfigList`, the editor's JSON)
/// for a new master list - its per-reference entries (LUAI) and the references serialized
/// in its Lua data (LUAD, merge_to_master's `remap_refnums`) renumbered by `mapping` (old
/// content-file index -> new; 0 is the file itself, 1.. its masters, as FRMR's). The
/// record's JSON back. ValueError for a record that is not a LUAL, an index the mapping
/// does not cover, or data that does not read (it would be left meaning other objects).
#[pyfunction]
fn lual_remap(record_json: &str, mapping: std::collections::HashMap<i32, i32>) -> PyResult<String> {
    use pyo3::exceptions::PyValueError;
    use tes3::esp::TES3Object;
    let mut value: serde_json::Value =
        serde_json::from_str(record_json).map_err(|e| PyValueError::new_err(format!("not a record: {e}")))?;
    blank_lua_data(&mut value);
    let mut obj: TES3Object =
        serde_json::from_value(value).map_err(|e| PyValueError::new_err(format!("not a record: {e}")))?;
    let TES3Object::ScriptConfigList(list) = &mut obj else {
        return Err(PyValueError::new_err("not a LUAL record"));
    };
    let mut missing: Option<i32> = None;
    for script in &mut list.scripts {
        for instance in &mut script.instances {
            match mapping.get(&instance.mast_idx) {
                Some(&m) => instance.mast_idx = m,
                None => missing = Some(instance.mast_idx),
            }
        }
        let datas = std::iter::once(&mut script.init_data)
            .chain(script.records.iter_mut().map(|r| &mut r.data))
            .chain(script.instances.iter_mut().map(|i| &mut i.data));
        for data in datas {
            merge_to_master::remap_refnums(data, &mut |file: &mut i32, _index: &mut u32| match mapping.get(file) {
                Some(&m) => *file = m,
                None => missing = Some(*file),
            })
            .map_err(|e| PyValueError::new_err(format!("{}: its Lua data does not read ({e})", script.path)))?;
        }
    }
    if let Some(m) = missing {
        return Err(PyValueError::new_err(format!(
            "a script entry names content file {m}, which the plugin's master list does not reach"
        )));
    }
    serde_json::to_string(&obj).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// `lua_tokenize(src, comments=False, teal=False)`: Lua source as tokens,
/// `[(kind, text, start, line, col, value)]` ending with `eof` (`lexer`, the Python
/// `wraithguard.lua.lexer.tokenize`'s). ValueError `(message, line, col)` for text that is
/// not Lua.
#[pyfunction]
#[pyo3(signature = (src, comments=false, teal=false))]
fn lua_tokenize(py: Python<'_>, src: &str, comments: bool, teal: bool) -> PyResult<Vec<lexer::Token>> {
    py.detach(|| lexer::tokenize(src, comments, teal)).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// A syntax tree node as Python reads it: `(kind, line, col, value, [children])`.
fn node_to_py<'py>(py: Python<'py>, n: &parser::Node) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
    let kids = PyList::empty(py);
    for c in &n.children {
        kids.append(node_to_py(py, c)?)?;
    }
    pyo3::types::PyTuple::new(py, [
        n.kind.into_pyobject(py)?.into_any(),
        n.line.into_pyobject(py)?.into_any(),
        n.col.into_pyobject(py)?.into_any(),
        n.value.as_deref().into_pyobject(py)?.into_any(),
        kids.into_any(),
    ])
}

/// `lua_parse(src, teal=False)`: a chunk's syntax tree (`parser`, the Python
/// `wraithguard.lua.parser.parse`'s), as nested `(kind, line, col, value, [children])`.
/// ValueError `(message, line, col)` for source that is not Lua (or Teal).
#[pyfunction]
#[pyo3(signature = (src, teal=false))]
fn lua_parse<'py>(py: Python<'py>, src: &str, teal: bool) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
    let tree = py.detach(|| parser::parse(src, teal)).map_err(pyo3::exceptions::PyValueError::new_err)?;
    node_to_py(py, &tree)
}

/// `lua_analyze(src, contexts, handlers, per_frame, packages)`: one script read as the
/// Python `wraithguard.lua.analysis.analyze` reads it (`analysis`), against one release's
/// rules (`handlers` and `packages`: name -> contexts). Returns `{interface_name,
/// interface_line, interface_members, engine_handlers, event_handlers, requires,
/// sent_events, findings, tree}` - the maps as `[(name, line)]` in their order, findings
/// as `(severity, code, line, message)`, `tree` as `lua_parse` gives it or None.
#[pyfunction]
fn lua_analyze<'py>(
    py: Python<'py>,
    src: &str,
    contexts: Vec<String>,
    handlers: std::collections::HashMap<String, Vec<String>>,
    per_frame: Vec<String>,
    packages: std::collections::HashMap<String, Vec<String>>,
) -> PyResult<Bound<'py, PyDict>> {
    let rules = analysis::Rules { handlers, per_frame: per_frame.into_iter().collect(), packages };
    let (info, tree) = py.detach(|| analysis::analyze(src, &contexts, &rules));
    info_to_py(py, info, tree)
}

/// `lua_analyze`'s dict for one script's analysis.
fn info_to_py<'py>(py: Python<'py>, info: analysis::Info, tree: Option<parser::Node>) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    out.set_item("interface_name", info.interface_name)?;
    out.set_item("interface_line", info.interface_line)?;
    out.set_item("interface_members", info.interface_members)?;
    out.set_item("engine_handlers", info.engine_handlers.items)?;
    out.set_item("event_handlers", info.event_handlers.items)?;
    out.set_item("requires", info.requires.items)?;
    out.set_item("sent_events", info.sent_events.items)?;
    out.set_item("findings", info.findings)?;
    match tree {
        Some(t) => out.set_item("tree", node_to_py(py, &t)?)?,
        None => out.set_item("tree", py.None())?,
    }
    Ok(out)
}

/// A syntax tree from Python's nested `(kind, line, col, value, [children])` (as
/// `lua_parse` gives it, or `wraithguard.lua.parser._to_native` makes it).
fn node_from_py(obj: &Bound<'_, PyAny>) -> PyResult<parser::Node> {
    let (kind, line, col, value, kids): (String, usize, usize, Option<String>, Vec<Bound<'_, PyAny>>) = obj.extract()?;
    let children = kids.iter().map(node_from_py).collect::<PyResult<Vec<_>>>()?;
    Ok(parser::Node { kind: intern_kind(&kind), line, col, value, children })
}

/// A node kind as the parser's `&'static str`.
fn intern_kind(kind: &str) -> &'static str {
    const KINDS: [&str; 41] = [
        "Assign", "Binop", "Block", "Break", "Call", "CallStat", "Cast", "Clause", "Do", "Else", "Exprs", "False",
        "Field", "ForIn", "ForNum", "Function", "FunctionStat", "Goto", "If", "Index", "Item", "Label", "Local",
        "LocalFunction", "Method", "Name", "Names", "Nil", "Number", "Params", "Paren", "Repeat", "Return", "String",
        "Table", "Targets", "True", "TypeDecl", "Unop", "Vararg", "While",
    ];
    if let Some(&k) = KINDS.iter().find(|k| **k == kind) {
        return k;
    }
    let leaked: &'static str = Box::leak(kind.to_string().into_boxed_str());
    leaked
}

/// `lua_omwscripts(text, name)`: an `.omwscripts` file parsed (`omwscripts`, the Python
/// `parse_omwscripts`'s): `(name, [(path, flags, source, line)], [(line, problem)])`.
#[pyfunction]
fn lua_omwscripts(
    text: &str,
    name: &str,
) -> (String, Vec<(String, Vec<String>, String, usize)>, Vec<(usize, String)>) {
    let p = omwscripts::parse_omwscripts(text, name);
    (p.name, p.entries.into_iter().map(|e| (e.path, e.flags, e.source, e.line)).collect(), p.problems)
}

/// `lua_scan(data_dirs, content, handlers, per_frame, packages, builtin_interfaces,
/// builtin_events)`: a load order's scripts found, read and checked (`scan`, the Python
/// `scan_load_order`'s, archives already unpacked into `data_dirs`). Returns `{omwscripts,
/// omwaddons, lual_files, scripts, findings}`: `omwscripts` as `lua_omwscripts` gives
/// each; `scripts` as dicts `{path, registrations, flags, providers, info, read_error}`
/// (`info` as `lua_analyze` gives it, or None); `findings` as `(path, severity, code,
/// line, message)`. OSError for an `.omwscripts` file that is there but cannot be read.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn lua_scan<'py>(
    py: Python<'py>,
    data_dirs: Vec<PathBuf>,
    content: Vec<String>,
    handlers: std::collections::HashMap<String, Vec<String>>,
    per_frame: Vec<String>,
    packages: std::collections::HashMap<String, Vec<String>>,
    builtin_interfaces: Vec<String>,
    builtin_events: Vec<String>,
) -> PyResult<Bound<'py, PyDict>> {
    let rules = scan::ScanRules {
        rules: analysis::Rules { handlers, per_frame: per_frame.into_iter().collect(), packages },
        builtin_interfaces: builtin_interfaces.into_iter().collect(),
        builtin_events: builtin_events.into_iter().collect(),
    };
    let got = py
        .detach(|| scan::scan_load_order(&data_dirs, &content, &rules))
        .map_err(|e| PyOSError::new_err(e.to_string()))?;
    let out = PyDict::new(py);
    let files = PyList::empty(py);
    for p in got.omwscripts {
        let entries: Vec<(String, Vec<String>, String, usize)> =
            p.entries.into_iter().map(|e| (e.path, e.flags, e.source, e.line)).collect();
        files.append((p.name, entries, p.problems))?;
    }
    out.set_item("omwscripts", files)?;
    out.set_item("omwaddons", got.omwaddons)?;
    out.set_item("lual_files", got.lual_files)?;
    let scripts = PyList::empty(py);
    for rec in got.scripts {
        let d = PyDict::new(py);
        d.set_item("path", rec.path)?;
        let regs: Vec<(String, Vec<String>, String, usize)> =
            rec.registrations.into_iter().map(|e| (e.path, e.flags, e.source, e.line)).collect();
        d.set_item("registrations", regs)?;
        d.set_item("flags", rec.flags)?;
        let providers: Vec<String> = rec.providers.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        d.set_item("providers", providers)?;
        match rec.info {
            Some((info, tree)) => d.set_item("info", info_to_py(py, info, tree)?)?,
            None => d.set_item("info", py.None())?,
        }
        d.set_item("read_error", rec.read_error)?;
        scripts.append(d)?;
    }
    out.set_item("scripts", scripts)?;
    out.set_item("findings", got.findings)?;
    Ok(out)
}

/// `lua_findings_for(diags, teal, bound, members, stubs)`: one file's Teal diagnostics
/// `[(kind, severity, line, message)]` as findings `[(severity, code, line, message)]`
/// (`findings`, the Python `openmw_api.findings_for`'s). `bound`: the names the script
/// binds; `members`: each declared type's member names; `stubs`: no install was read.
#[pyfunction]
fn lua_findings_for(
    diags: Vec<(String, String, usize, String)>,
    teal: bool,
    bound: Vec<String>,
    members: std::collections::HashMap<String, Vec<String>>,
    stubs: bool,
) -> Vec<analysis::Finding> {
    let refs: Vec<findings::DiagRef<'_>> = diags
        .iter()
        .map(|(k, s, l, m)| findings::DiagRef { kind: k, severity: s, line: *l, message: m })
        .collect();
    findings::findings_for(&refs, teal, &bound.into_iter().collect(), &members, stubs)
}

/// `lua_check_findings(jobs, include, globals, members, stubs)`: `lua_check` and
/// `lua_findings_for` in one pass. `jobs`: `(file to check, is Teal, the .lua whose names
/// count as bound)`. One entry per job: its findings, or a string saying why the file
/// could not be checked.
#[pyfunction]
fn lua_check_findings<'py>(
    py: Python<'py>,
    jobs: Vec<(PathBuf, bool, PathBuf)>,
    include: Vec<PathBuf>,
    globals: Vec<String>,
    members: std::collections::HashMap<String, Vec<String>>,
    stubs: bool,
) -> PyResult<Bound<'py, PyList>> {
    let paths: Vec<PathBuf> = jobs.iter().map(|j| j.0.clone()).collect();
    let res = py
        .detach(|| {
            crate::guarded(|| {
                check::check_with(&paths, &include, &globals).map_err(|e| std::io::Error::other(format!("{e:#}")))
            })
            .map(|results| {
                results
                    .into_iter()
                    .zip(&jobs)
                    .map(|(r, (_, teal, src))| {
                        r.map(|diags| {
                            let bound = scan::read_text(src)
                                .ok()
                                .and_then(|text| parser::parse(&text, false).ok())
                                .map(|tree| analysis::bound_names(&tree))
                                .unwrap_or_default();
                            let refs: Vec<findings::DiagRef<'_>> = diags
                                .iter()
                                .map(|d| findings::DiagRef {
                                    kind: d.kind,
                                    severity: d.severity,
                                    line: d.line,
                                    message: &d.message,
                                })
                                .collect();
                            findings::findings_for(&refs, *teal, &bound, &members, stubs)
                        })
                    })
                    .collect::<Vec<_>>()
            })
        })
        .map_err(|e| PyOSError::new_err(format!("the Teal checker did not start: {e}")))?;
    let out = PyList::empty(py);
    for r in res {
        match r {
            Ok(found) => out.append(found)?,
            Err(e) => out.append(e)?,
        }
    }
    Ok(out)
}

/// `lua_tlconfig_dirs(data_dirs)`: the folders the mods' `tlconfig.lua` files name
/// (`source_dir`, `include_dir`), joined to their file's folder, in the order found; not
/// resolved or checked (the Python `tlconfig_dirs` does that).
#[pyfunction]
fn lua_tlconfig_dirs(py: Python<'_>, data_dirs: Vec<PathBuf>) -> Vec<String> {
    py.detach(|| findings::tlconfig_dirs(&data_dirs)).iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// `lua_tlconfig_globals(data_dirs)`: the modules the mods' `global_env_def` name.
#[pyfunction]
fn lua_tlconfig_globals(py: Python<'_>, data_dirs: Vec<PathBuf>) -> Vec<String> {
    py.detach(|| findings::tlconfig_globals(&data_dirs))
}

/// `lua_find_teal_declarations(near)`: a `teal_declarations` folder in or beside one of
/// these, or None.
#[pyfunction]
fn lua_find_teal_declarations(py: Python<'_>, near: Vec<PathBuf>) -> Option<String> {
    py.detach(|| findings::find_teal_declarations(&near)).map(|p| p.to_string_lossy().into_owned())
}

/// `lua_find_resources(cfg=None, explicit=None)`: the OpenMW install's `resources` folder
/// (the Python `openmw_api.find_resources`'s order), or None.
#[pyfunction]
#[pyo3(signature = (cfg=None, explicit=None))]
fn lua_find_resources(py: Python<'_>, cfg: Option<PathBuf>, explicit: Option<PathBuf>) -> Option<String> {
    py.detach(|| findings::find_resources(cfg.as_deref(), explicit.as_deref())).map(|p| p.to_string_lossy().into_owned())
}

/// `lua_flowchart(node)`: a function's (or a chunk's) control flow as Mermaid.
#[pyfunction]
fn lua_flowchart(node: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(mermaid::flowchart(&node_from_py(node)?))
}

/// `lua_call_graph_chart(chunk)`: a script's call graph as Mermaid.
#[pyfunction]
fn lua_call_graph_chart(chunk: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(mermaid::call_graph_chart(&node_from_py(chunk)?))
}

/// `lua_mermaid(nodes, edges)`: `[(id, shape, label)]` and `[(from, to, label or None)]`
/// as a Mermaid flowchart.
#[pyfunction]
fn lua_mermaid(nodes: Vec<(String, String, String)>, edges: Vec<(String, String, Option<String>)>) -> String {
    mermaid::render(&nodes, &edges)
}

/// `lua_report(header, findings, scripts, info=True)`: a scan as text (`report`).
/// `header`: `(openmw, revision, omwscripts, scripts, data folders, api_source, omwaddons,
/// lual files)`; `findings`: `(path, severity, code, line, message)`, any order;
/// `scripts`: `(path, flags, interface or None, engine handlers, missing)`.
#[pyfunction]
#[pyo3(signature = (header, found, scripts, info=true))]
fn lua_report(
    header: (String, i64, usize, usize, usize, String, usize, usize),
    found: Vec<report::Row>,
    scripts: Vec<report::ScriptLine>,
    info: bool,
) -> String {
    let (openmw, revision, omwscripts, n_scripts, data_dirs, api_source, omwaddons, lual_files) = header;
    let h = report::Header { openmw, revision, omwscripts, scripts: n_scripts, data_dirs, api_source, omwaddons, lual_files };
    report::render(&h, found, &scripts, info)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(lua_api, m)?)?;
    m.add_function(wrap_pyfunction!(lua_write_declarations, m)?)?;
    m.add_function(wrap_pyfunction!(lua_check, m)?)?;
    m.add_function(wrap_pyfunction!(lual_scripts, m)?)?;
    m.add_function(wrap_pyfunction!(lual_remap, m)?)?;
    m.add_function(wrap_pyfunction!(lua_tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(lua_parse, m)?)?;
    m.add_function(wrap_pyfunction!(lua_analyze, m)?)?;
    m.add_function(wrap_pyfunction!(lua_omwscripts, m)?)?;
    m.add_function(wrap_pyfunction!(lua_scan, m)?)?;
    m.add_function(wrap_pyfunction!(lua_findings_for, m)?)?;
    m.add_function(wrap_pyfunction!(lua_check_findings, m)?)?;
    m.add_function(wrap_pyfunction!(lua_tlconfig_dirs, m)?)?;
    m.add_function(wrap_pyfunction!(lua_tlconfig_globals, m)?)?;
    m.add_function(wrap_pyfunction!(lua_find_teal_declarations, m)?)?;
    m.add_function(wrap_pyfunction!(lua_find_resources, m)?)?;
    m.add_function(wrap_pyfunction!(lua_flowchart, m)?)?;
    m.add_function(wrap_pyfunction!(lua_call_graph_chart, m)?)?;
    m.add_function(wrap_pyfunction!(lua_mermaid, m)?)?;
    m.add_function(wrap_pyfunction!(lua_report, m)?)?;
    m.add("TEAL_VERSION", "0.24.8")?;
    Ok(())
}

#[cfg(test)]
mod lual_tests {
    use super::*;

    #[test]
    fn empty_lua_data_reads() {
        // The editor's (and the Python tests') record: Lua data written as "".
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{"type":"ScriptConfigList","flags":"","scripts":[{"path":"scripts/a.lua","init_data":"",
            "flags":"","types":["NPC_"],"records":[],
            "instances":[{"attach":true,"mast_idx":1,"ref_idx":7,"data":""}]}]}"#,
        )
        .unwrap();
        blank_lua_data(&mut v);
        let obj: tes3::esp::TES3Object = serde_json::from_value(v).expect("reads once blanked");
        let tes3::esp::TES3Object::ScriptConfigList(list) = obj else { panic!("not a LUAL") };
        assert!(list.scripts[0].init_data.is_empty() && list.scripts[0].instances[0].data.is_empty());
    }
}
