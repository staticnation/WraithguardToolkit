//! Plugins to tes3conv-schema JSON, in process, on `tes3::esp`: the Python side. The
//! reading and writing themselves are editcore's (`viewer-shell/editcore/src/plugin.rs`),
//! shared with the viewer's Editor; this module turns them into Python objects.

use std::io;
use std::path::PathBuf;

use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;

pub use editcore::plugin::{
    CellRef, Item, RAW, TAGS, blob, conflict_key, items, keys_and_cells, known, objects, read, record_deleted, records_bytes,
    records_json, take_blobs, to_json_file,
};

fn py_err(what: &str, e: io::Error) -> PyErr {
    let msg = format!("{what}: {e}");
    match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => PyOSError::new_err(msg),
        _ => PyValueError::new_err(msg),
    }
}

/// Converts the plugin at `src` to tes3conv-schema JSON at `out`; returns the
/// record count. ValueError for a malformed plugin, OSError when unreadable.
#[pyfunction]
fn plugin_json_file(py: Python<'_>, src: PathBuf, out: PathBuf) -> PyResult<usize> {
    py.detach(|| to_json_file(&src, &out)).map_err(|e| py_err(&src.display().to_string(), e))
}

/// A plugin's records as a tes3conv-schema JSON string.
#[pyfunction]
fn plugin_json(py: Python<'_>, data: &[u8]) -> PyResult<String> {
    py.detach(|| {
        let plugin = read(data)?;
        serde_json::to_string(&plugin.objects).map_err(io::Error::other)
    })
    .map_err(|e| py_err("plugin", e))
}

/// A plugin's records as a JSON array (see `records_json`); `keep` limits it to those
/// four-byte tags. ValueError for a malformed plugin.
#[pyfunction]
#[pyo3(signature = (data, keep=None))]
fn plugin_records_json(py: Python<'_>, data: &[u8], keep: Option<Vec<Vec<u8>>>) -> PyResult<String> {
    let keep: Option<Vec<[u8; 4]>> =
        keep.map(|k| k.into_iter().filter_map(|t| <[u8; 4]>::try_from(t.as_slice()).ok()).collect());
    py.detach(|| records_json(data, keep.as_deref())).map_err(|e| py_err("plugin", e))
}

/// `v` as `json.load` would have given it, recursively: every tuple a list (pythonize
/// gives serde tuples - a grid, a colour, a position - as Python tuples), and every
/// float that came from an `f32` the shortest decimal for that `f32` (1.3, not
/// 1.2999999523162842), by ryu as serde_json writes it, so it is the number the JSON
/// wrote. The toolkit compares and
/// shows these values as the JSON had them. A NaN stays a NaN, where JSON had null.
fn as_json_shapes(v: Bound<'_, PyAny>) -> PyResult<Bound<'_, PyAny>> {
    use pyo3::types::{PyDict, PyFloat, PyList, PyTuple};
    let nested = |x: &Bound<'_, PyAny>| {
        x.is_instance_of::<PyDict>() || x.is_instance_of::<PyList>() || x.is_instance_of::<PyTuple>() || x.is_exact_instance_of::<PyFloat>()
    };
    if let Ok(f) = v.cast::<PyFloat>() {
        let x = f.value();
        let n = x as f32;
        if x.is_finite()
            && n as f64 == x
            && let Ok(short) = ryu::Buffer::new().format_finite(n).parse::<f64>()
        {
            return Ok(PyFloat::new(v.py(), short).into_any());
        }
        return Ok(v);
    }
    if let Ok(d) = v.cast::<PyDict>() {
        for (k, x) in d.iter() {
            if nested(&x) {
                d.set_item(k, as_json_shapes(x)?)?;
            }
        }
        return Ok(v);
    }
    if let Ok(l) = v.cast::<PyList>() {
        for i in 0..l.len() {
            let x = l.get_item(i)?;
            if nested(&x) {
                l.set_item(i, as_json_shapes(x)?)?;
            }
        }
        return Ok(v);
    }
    if let Ok(t) = v.cast::<PyTuple>() {
        let items: Vec<Bound<'_, PyAny>> = t.iter().map(as_json_shapes).collect::<PyResult<_>>()?;
        return Ok(PyList::new(v.py(), items)?.into_any());
    }
    Ok(v)
}



/// The blobs `take_blobs` took, put into the record's dict as `bytes`.
fn put_blobs(v: &Bound<'_, PyAny>, blobs: Vec<(&'static str, Option<&'static str>, Vec<u8>)>) -> PyResult<()> {
    use pyo3::types::{PyBytes, PyDict};
    let Ok(d) = v.cast::<PyDict>() else { return Ok(()) };
    for (key, inner, bytes) in blobs {
        let b = PyBytes::new(v.py(), &bytes);
        match inner {
            None => d.set_item(key, b)?,
            Some(k) => match d.get_item(key)? {
                Some(x) => match x.cast::<PyDict>() {
                    Ok(sub) => sub.set_item(k, b)?,
                    Err(_) => d.set_item(key, b)?,
                },
                None => {
                    let sub = PyDict::new(v.py());
                    sub.set_item(k, b)?;
                    d.set_item(key, sub)?;
                }
            },
        }
    }
    Ok(())
}

/// A plugin's records as Python dicts in the tes3conv schema - the same values
/// `json.load` gives for `plugin_json_file`'s output, with no JSON in between - except
/// the packed number arrays (see `take_blobs`), which come as `bytes`: what the JSON's
/// base64 string held once decoded and decompressed. `keep`
/// limits it to those four-byte tags; `light` empties the bulky parts (see `objects`).
/// ValueError for a malformed plugin.
#[pyfunction]
/// With `raw`, a record the crate does not model comes as `{"type": "__Raw", "tag",
/// "flags", "body"}` (`body` as bytes) in its place, instead of being skipped.
#[pyo3(signature = (data, keep=None, light=false, raw=false))]
fn plugin_records<'py>(
    py: Python<'py>,
    data: &[u8],
    keep: Option<Vec<Vec<u8>>>,
    light: bool,
    raw: bool,
) -> PyResult<Bound<'py, pyo3::types::PyList>> {
    let keep: Option<Vec<[u8; 4]>> =
        keep.map(|k| k.into_iter().filter_map(|t| <[u8; 4]>::try_from(t.as_slice()).ok()).collect());
    let got = py.detach(|| items(data, keep.as_deref(), light, raw)).map_err(|e| py_err("plugin", e))?;
    let list = pyo3::types::PyList::empty(py);
    for it in got {
        match it {
            Item::Obj(mut o) => {
                let blobs = take_blobs(&mut o);
                let v = pythonize::pythonize(py, &o).map_err(|e| PyValueError::new_err(e.to_string()))?;
                let v = as_json_shapes(v)?;
                put_blobs(&v, blobs)?;
                list.append(v)?;
            }
            Item::Raw { tag, flags, body } => {
                let d = pyo3::types::PyDict::new(py);
                d.set_item("type", RAW)?;
                d.set_item("tag", tag.iter().map(|&b| b as char).collect::<String>())?;
                d.set_item("flags", flags)?;
                d.set_item("body", pyo3::types::PyBytes::new(py, &body))?;
                list.append(d)?;
            }
        }
    }
    Ok(list)
}

/// Records given as Python dicts - the tes3conv schema, or `{"type": "__Raw", "tag",
/// "flags", "body"}` with `body` as bytes or base64 - written to plugin bytes, in order,
/// each as it stands. No JSON in between: each dict goes straight into the crate's
/// record type. ValueError naming the record that could not be built.
#[pyfunction]
fn plugin_write<'py>(py: Python<'py>, items: &Bound<'py, pyo3::types::PyList>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    use base64::Engine as _;
    use pyo3::types::{PyAnyMethods, PyBytes, PyDict};
    let mut w = bytes_io::Writer::new(Vec::new());
    for (i, item) in items.iter().enumerate() {
        let d = item.cast::<PyDict>().map_err(|_| PyValueError::new_err(format!("record {i}: not a dict")))?;
        let kind: String = d.get_item("type")?.map(|t| t.extract()).transpose()?.unwrap_or_default();
        if kind == RAW {
            let tag: String = d.get_item("tag")?.map(|t| t.extract()).transpose()?.unwrap_or_default();
            let tag: Vec<u8> = tag.chars().map(|c| c as u32 as u8).collect();
            if tag.len() != 4 {
                return Err(PyValueError::new_err(format!("record {i}: a raw record's tag must be four bytes")));
            }
            let body: Vec<u8> = match d.get_item("body")? {
                Some(b) if b.is_instance_of::<PyBytes>() => b.extract()?,
                Some(b) => base64::engine::general_purpose::STANDARD
                    .decode(b.extract::<String>()?)
                    .map_err(|e| PyValueError::new_err(format!("record {i}: {e}")))?,
                None => Vec::new(),
            };
            let flags: u32 = d.get_item("flags")?.map(|f| f.extract()).transpose()?.unwrap_or(0);
            let c = &mut w.cursor;
            use std::io::Write as _;
            c.write_all(&tag)?;
            c.write_all(&(body.len() as u32).to_le_bytes())?;
            c.write_all(&0u32.to_le_bytes())?;
            c.write_all(&flags.to_le_bytes())?;
            c.write_all(&body)?;
        } else {
            let obj: tes3::esp::TES3Object = pythonize::depythonize(&item)
                .map_err(|e| PyValueError::new_err(format!("record {i} ({kind}): {e}")))?;
            crate::guarded(|| w.save(&obj)).map_err(|e| py_err(&format!("record {i} ({kind})"), e))?;
        }
    }
    Ok(PyBytes::new(py, &w.cursor.into_inner()))
}


/// A plugin's conflict keys and cells (see `keys_and_cells`): `([(type, id, deleted)],
/// [("int", name, None) | ("ext", x, y)])`. ValueError for a malformed plugin.
#[pyfunction]
fn plugin_keys<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
    use pyo3::types::{PyList, PyTuple};
    let (keys, cells) = py.detach(|| keys_and_cells(data)).map_err(|e| py_err("plugin", e))?;
    let k = PyList::empty(py);
    for (t, id, del) in keys {
        k.append(PyTuple::new(py, [t.into_pyobject(py)?.into_any(), id.into_pyobject(py)?.into_any(), pyo3::types::PyBool::new(py, del).to_owned().into_any()])?)?;
    }
    let c = PyList::empty(py);
    for cell in cells {
        let t = match cell {
            CellRef::Int(name) => PyTuple::new(py, ["int".into_pyobject(py)?.into_any(), name.into_pyobject(py)?.into_any(), py.None().into_bound(py)])?,
            CellRef::Ext(x, y) => PyTuple::new(py, ["ext".into_pyobject(py)?.into_any(), x.into_pyobject(py)?.into_any(), y.into_pyobject(py)?.into_any()])?,
        };
        c.append(t)?;
    }
    PyTuple::new(py, [k.into_any(), c.into_any()])
}

/// Records as a JSON array back to plugin bytes (see `records_bytes`).
#[pyfunction]
fn plugin_records_bytes<'py>(py: Python<'py>, json: &str) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    let b = py.detach(|| records_bytes(json)).map_err(|e| py_err("records", e))?;
    Ok(pyo3::types::PyBytes::new(py, &b))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(plugin_json_file, m)?)?;
    m.add_function(wrap_pyfunction!(plugin_json, m)?)?;
    m.add_function(wrap_pyfunction!(plugin_records_json, m)?)?;
    m.add_function(wrap_pyfunction!(plugin_records_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(plugin_records, m)?)?;
    m.add_function(wrap_pyfunction!(plugin_write, m)?)?;
    m.add_function(wrap_pyfunction!(plugin_keys, m)?)?;
    Ok(())
}
