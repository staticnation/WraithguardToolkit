//! The Editor's record cache (`editcore::cache`, port step 4.1 slice 2) for Python:
//! `wraithguard/patch/recordcache.py` keeps its API and calls this. What it stores was
//! pickled; it is now tagged JSON (`cache_dumps` / `cache_loads`), which keeps every
//! Python value the cache holds as it was - tuples stay tuples, bytes stay bytes - and
//! runs no code when read back.
//!
//! The tags, each an object with one key: `{"$t": [...]}` a tuple, `{"$b": "base64"}`
//! bytes, `{"$i": "digits"}` an int past 64 bits, `{"$f": "nan"}` a float JSON has no
//! number for, `{"$p": "path"}` a path, `{"$m": [[k, v], ...]}` a dict with keys that are
//! not all strings, and `{"$d": {...}}` a dict that would itself read as a tag.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use base64::Engine as _;
use editcore::cache::{Cache, UseHit, UseRow};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Value};

const TAGS: [&str; 7] = ["$t", "$b", "$i", "$f", "$p", "$m", "$d"];

fn tagged(tag: &str, v: Value) -> Value {
    let mut m = Map::new();
    m.insert(tag.to_string(), v);
    Value::Object(m)
}

/// A Python value as tagged JSON. TypeError for a value the cache does not keep.
fn to_value(v: &Bound<'_, PyAny>) -> PyResult<Value> {
    if v.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(b) = v.cast::<PyBool>() {
        return Ok(Value::Bool(b.is_true()));
    }
    if v.is_instance_of::<PyInt>() {
        if let Ok(i) = v.extract::<i64>() {
            return Ok(Value::from(i));
        }
        return Ok(tagged("$i", Value::from(v.str()?.to_string())));
    }
    if let Ok(f) = v.cast::<PyFloat>() {
        let x = f.value();
        return Ok(match serde_json::Number::from_f64(x) {
            Some(n) => Value::Number(n),
            None => tagged("$f", Value::from(v.repr()?.to_string())),
        });
    }
    if let Ok(s) = v.cast::<PyString>() {
        return Ok(Value::from(s.to_str()?));
    }
    if let Ok(b) = v.cast::<PyBytes>() {
        return Ok(tagged("$b", Value::from(base64::engine::general_purpose::STANDARD.encode(b.as_bytes()))));
    }
    if let Ok(l) = v.cast::<PyList>() {
        return Ok(Value::Array(l.iter().map(|x| to_value(&x)).collect::<PyResult<_>>()?));
    }
    if let Ok(t) = v.cast::<PyTuple>() {
        return Ok(tagged("$t", Value::Array(t.iter().map(|x| to_value(&x)).collect::<PyResult<_>>()?)));
    }
    if let Ok(d) = v.cast::<PyDict>() {
        if d.keys().iter().all(|k| k.is_instance_of::<PyString>()) {
            let mut m = Map::new();
            for (k, x) in d.iter() {
                m.insert(k.cast::<PyString>()?.to_str()?.to_string(), to_value(&x)?);
            }
            let reads_as_tag = m.len() == 1 && m.keys().next().is_some_and(|k| TAGS.contains(&k.as_str()));
            return Ok(if reads_as_tag { tagged("$d", Value::Object(m)) } else { Value::Object(m) });
        }
        let pairs = d.iter().map(|(k, x)| Ok(Value::Array(vec![to_value(&k)?, to_value(&x)?]))).collect::<PyResult<_>>()?;
        return Ok(tagged("$m", Value::Array(pairs)));
    }
    if let Ok(b) = v.cast::<pyo3::types::PyByteArray>() {
        return Ok(tagged("$b", Value::from(base64::engine::general_purpose::STANDARD.encode(b.to_vec()))));
    }
    let os = v.py().import("os")?;
    if v.is_instance(&os.getattr("PathLike")?)? {
        let s: String = os.call_method1("fspath", (v,))?.extract()?;
        return Ok(tagged("$p", Value::from(s)));
    }
    Err(PyTypeError::new_err(format!("the record cache does not keep a {}", v.get_type().name()?)))
}

/// Tagged JSON back to the Python value.
fn from_value<'py>(py: Python<'py>, v: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        Value::Null => py.None().into_bound(py),
        Value::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
        Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => i.into_pyobject(py)?.into_any(),
            (None, Some(u)) => u.into_pyobject(py)?.into_any(),
            _ => PyFloat::new(py, n.as_f64().unwrap_or(f64::NAN)).into_any(),
        },
        Value::String(s) => PyString::new(py, s).into_any(),
        Value::Array(a) => PyList::new(py, a.iter().map(|x| from_value(py, x)).collect::<PyResult<Vec<_>>>()?)?.into_any(),
        Value::Object(m) => {
            if m.len() == 1 {
                let (k, x) = m.iter().next().unwrap();
                let text = || x.as_str().ok_or_else(|| PyValueError::new_err(format!("a bad {k} in the record cache")));
                match k.as_str() {
                    "$t" => {
                        let items = x.as_array().map(|a| a.iter().map(|y| from_value(py, y)).collect::<PyResult<Vec<_>>>()).transpose()?.unwrap_or_default();
                        return Ok(PyTuple::new(py, items)?.into_any());
                    }
                    "$b" => {
                        let bytes = base64::engine::general_purpose::STANDARD.decode(text()?).map_err(|e| PyValueError::new_err(e.to_string()))?;
                        return Ok(PyBytes::new(py, &bytes).into_any());
                    }
                    "$i" => return py.import("builtins")?.getattr("int")?.call1((text()?,)),
                    "$f" => return py.import("builtins")?.getattr("float")?.call1((text()?,)),
                    "$p" => return py.import("pathlib")?.getattr("Path")?.call1((text()?,)),
                    "$m" => {
                        let d = PyDict::new(py);
                        for pair in x.as_array().into_iter().flatten() {
                            if let Some([a, b]) = pair.as_array().map(|p| p.as_slice()) {
                                d.set_item(from_value(py, a)?, from_value(py, b)?)?;
                            }
                        }
                        return Ok(d.into_any());
                    }
                    "$d" => {
                        let d = PyDict::new(py);
                        for (k2, y) in x.as_object().into_iter().flatten() {
                            d.set_item(k2, from_value(py, y)?)?;
                        }
                        return Ok(d.into_any());
                    }
                    _ => {}
                }
            }
            let d = PyDict::new(py);
            for (k, x) in m {
                d.set_item(k, from_value(py, x)?)?;
            }
            d.into_any()
        }
    })
}

/// A value the record cache keeps, as the bytes it stores (tagged JSON). TypeError for a
/// value it does not keep (anything but None, bool, int, float, str, bytes, list, tuple,
/// dict and paths).
#[pyfunction]
fn cache_dumps<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let v = to_value(value)?;
    let text = serde_json::to_vec(&v).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, &text))
}

/// The value `cache_dumps` stored. ValueError for bytes it did not write.
#[pyfunction]
fn cache_loads<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let v: Value = py.detach(|| serde_json::from_slice(data)).map_err(|e| PyValueError::new_err(e.to_string()))?;
    from_value(py, &v)
}

/// The record cache's database (`editcore::cache::Cache`). Plugins are paths; lists of
/// plugins are answered by position. Warnings go to the `wraithguard.patch.recordcache`
/// log.
#[pyclass(name = "RecordCacheDb", module = "wraithguard_native", frozen)]
pub struct RecordCacheDb {
    inner: Cache,
}

fn warn_to_log(msg: &str) {
    Python::attach(|py| {
        if let Ok(log) = py.import("logging").and_then(|m| m.call_method1("getLogger", ("wraithguard.patch.recordcache",))) {
            let _ = log.call_method1("warning", (msg,));
        }
    });
}

#[pymethods]
impl RecordCacheDb {
    #[new]
    #[pyo3(signature = (path, parser="", max_bytes=editcore::cache::MAX_BYTES))]
    fn new(path: PathBuf, parser: &str, max_bytes: u64) -> Self {
        RecordCacheDb { inner: Cache::with_warn(path, parser, max_bytes, Box::new(warn_to_log)) }
    }

    fn get<'py>(&self, py: Python<'py>, plugin: PathBuf, tag: &str) -> Option<Bound<'py, PyBytes>> {
        py.detach(|| self.inner.get(&plugin, tag)).map(|b| PyBytes::new(py, &b))
    }

    #[pyo3(signature = (plugin, tag, data, keys=None))]
    fn put(&self, py: Python<'_>, plugin: PathBuf, tag: &str, data: Vec<u8>, keys: Option<Vec<String>>) {
        py.detach(|| self.inner.put(&plugin, tag, &data, keys.as_deref()));
    }

    fn put_keys(&self, py: Python<'_>, plugin: PathBuf, tag: &str, keys: Vec<String>) {
        py.detach(|| self.inner.put_keys(&plugin, tag, &keys));
    }

    fn stale_of(&self, py: Python<'_>, plugins: Vec<PathBuf>, tag: &str) -> Option<Vec<usize>> {
        py.detach(|| self.inner.stale_of(&plugins, tag))
    }

    fn keys_current(&self, py: Python<'_>, plugin: PathBuf, tag: &str) -> bool {
        py.detach(|| self.inner.keys_current(&plugin, tag))
    }

    fn missing_keys(&self, py: Python<'_>, plugins: Vec<PathBuf>, tag: &str) -> Vec<usize> {
        py.detach(|| self.inner.missing_keys(&plugins, tag))
    }

    fn holders(&self, py: Python<'_>, plugins: Vec<PathBuf>, tag: &str, key: &str) -> Option<Vec<usize>> {
        py.detach(|| self.inner.holders(&plugins, tag, key))
    }

    fn all_keys(&self, py: Python<'_>, plugins: Vec<PathBuf>, tag: &str) -> Option<HashSet<String>> {
        py.detach(|| self.inner.all_keys(&plugins, tag))
    }

    fn put_uses(&self, py: Python<'_>, plugin: PathBuf, rows: Vec<UseRow>) {
        py.detach(|| self.inner.put_uses(&plugin, &rows));
    }

    fn missing_uses(&self, py: Python<'_>, plugins: Vec<PathBuf>) -> Vec<usize> {
        py.detach(|| self.inner.missing_uses(&plugins))
    }

    fn uses_for(&self, py: Python<'_>, plugins: Vec<PathBuf>, value: &str) -> Option<Vec<UseHit>> {
        py.detach(|| self.inner.uses_for(&plugins, value))
    }

    fn definers(&self, py: Python<'_>, plugins: Vec<PathBuf>, records: Vec<(String, String)>) -> Option<HashMap<(String, String), Vec<usize>>> {
        py.detach(|| self.inner.definers(&plugins, &records))
    }

    #[staticmethod]
    fn load_order_signature(py: Python<'_>, plugins: Vec<PathBuf>) -> Option<String> {
        py.detach(|| Cache::load_order_signature(&plugins))
    }

    fn get_report<'py>(&self, py: Python<'py>, sig: &str, kind: &str, key: &str) -> Option<Bound<'py, PyBytes>> {
        py.detach(|| self.inner.get_report(sig, kind, key)).map(|b| PyBytes::new(py, &b))
    }

    fn put_report(&self, py: Python<'_>, sig: &str, kind: &str, key: &str, data: Vec<u8>) {
        py.detach(|| self.inner.put_report(sig, kind, key, &data));
    }

    fn clear(&self, py: Python<'_>) {
        py.detach(|| self.inner.clear());
    }

    fn stats(&self, py: Python<'_>) -> HashMap<String, i64> {
        py.detach(|| self.inner.stats().into_iter().collect())
    }

    fn compact(&self, py: Python<'_>) -> bool {
        py.detach(|| self.inner.compact())
    }

    fn size_bytes(&self, py: Python<'_>) -> i64 {
        py.detach(|| self.inner.size_bytes())
    }

    fn close(&self, py: Python<'_>) {
        py.detach(|| self.inner.close());
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(cache_dumps, m)?)?;
    m.add_function(wrap_pyfunction!(cache_loads, m)?)?;
    m.add_class::<RecordCacheDb>()?;
    Ok(())
}
