//! Morrowind scripts (MWScript) for the Python Editor: the compiler and Script Edit's
//! checks are `editcore::mwscript` (moved there in port step 4.1, slice 5d, so the viewer
//! runs the same ones); this wraps them.
//!
//! `wraithguard/mwscript/compiler.py` is the Python twin: `compile_script` there calls
//! `ScriptCompiler` here when the native module is built, and runs itself when it is not.

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use editcore::mwscript::check;

/// A record the editor's pool adds or changes: `(tag, id, script, dial_type, shorts,
/// longs, floats)` (`script` empty and `dial_type` -1 when they do not apply).
type Extra = editcore::mwscript::Extra;

/// `ScriptCompiler(plugins, extra=(), custom=None)`: the load order's records, read once,
/// to compile any number of scripts against. `extra` (the patch pool's records) is laid
/// over them; `custom` is a `customfunctions.dat`'s text (the extended MWSE / MW-Enhanced
/// functions), None for the set MWEdit ships.
#[pyclass(frozen, module = "wraithguard_native")]
pub struct ScriptCompiler {
    inner: editcore::mwscript::ScriptCompiler,
}

#[pymethods]
impl ScriptCompiler {
    #[new]
    #[pyo3(signature = (plugins, extra=Vec::new(), custom=None))]
    fn new(py: Python<'_>, plugins: Vec<PathBuf>, extra: Vec<Extra>, custom: Option<Vec<u8>>) -> Self {
        let inner = py.detach(|| editcore::mwscript::ScriptCompiler::new(&plugins, &extra, custom.as_deref()));
        ScriptCompiler { inner }
    }

    /// Distinct IDs indexed.
    fn __len__(&self) -> usize {
        self.inner.index.len()
    }

    /// `script_locals(id)`: the locals of a script, or of the script an object carries
    /// (a dialogue result's speaker), as `(shorts, longs, floats)`; None when it has none.
    fn script_locals<'py>(&self, py: Python<'py>, id: &str) -> Option<[Vec<Bound<'py, PyBytes>>; 3]> {
        let locals = self.inner.script_locals(id)?;
        Some(locals.map(|names| names.iter().map(|n| PyBytes::new(py, n)).collect()))
    }

    /// `compile(source, levels="default", tribunal=True, bloodmoon=True, extended=True)`:
    /// the source as the plugin's bytes (Windows-1252). Returns `{ok, name, data,
    /// shorts, longs, floats, var_data, header, messages}`, `messages` as `(line,
    /// column, level, text)`.
    #[pyo3(signature = (source, levels="default", tribunal=true, bloodmoon=true, extended=true))]
    fn compile<'py>(
        &self,
        py: Python<'py>,
        source: &[u8],
        levels: &str,
        tribunal: bool,
        bloodmoon: bool,
        extended: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let result = py.detach(|| self.inner.run(source, levels, tribunal, bloodmoon, extended));
        let out = PyDict::new(py);
        let names = |k: usize| -> Vec<Bound<'py, PyBytes>> {
            result.locals[k].iter().map(|n| PyBytes::new(py, n)).collect()
        };
        out.set_item("ok", result.ok)?;
        out.set_item("name", PyBytes::new(py, &result.name))?;
        out.set_item("data", PyBytes::new(py, &result.data))?;
        out.set_item("shorts", names(0))?;
        out.set_item("longs", names(1))?;
        out.set_item("floats", names(2))?;
        out.set_item("var_data", PyBytes::new(py, &result.var_data()))?;
        out.set_item("header", PyBytes::new(py, &result.header()))?;
        let messages: Vec<(usize, usize, &str, String)> =
            result.messages.iter().map(|m| (m.line, m.column, m.level, m.text.clone())).collect();
        out.set_item("messages", messages)?;
        Ok(out)
    }

    /// `compile_record(id, text, levels="default", tribunal=True, bloodmoon=True)`: the
    /// `SCPT` record a save writes, as the tes3 crate's JSON (the editor's record form:
    /// `header`, `variables`, `bytecode` as the patch stores them), or None when the
    /// source does not compile. `text` is kept as given.
    #[pyo3(signature = (id, text, levels="default", tribunal=true, bloodmoon=true, extended=true))]
    #[allow(clippy::too_many_arguments)]
    fn compile_record(
        &self,
        py: Python<'_>,
        id: &str,
        text: &str,
        levels: &str,
        tribunal: bool,
        bloodmoon: bool,
        extended: bool,
    ) -> PyResult<Option<String>> {
        let made = py.detach(|| self.inner.compile_record(id, text, levels, tribunal, bloodmoon, extended));
        Ok(made.map(|v| v.to_string()))
    }
}

/// `mwscript_check(text, script_id="", globals=())`: the Script Edit window's checks as
/// it is typed (`check`), `[(line, level, message)]`.
#[pyfunction]
#[pyo3(signature = (text, script_id="", globals=Vec::new()))]
fn mwscript_check(py: Python<'_>, text: &str, script_id: &str, globals: Vec<String>) -> Vec<check::Finding> {
    py.detach(|| check::check_script(text, script_id, &globals))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ScriptCompiler>()?;
    m.add_function(wrap_pyfunction!(mwscript_check, m)?)?;
    Ok(())
}
