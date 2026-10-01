//! Plugins to tes3conv-schema JSON, in process, on `tes3::esp`.
//!
//! tes3conv is greatness7's converter built on this same crate, so serialising its
//! records with serde (zstd'd base64 for the packed arrays, as tes3conv does) gives
//! tes3conv's JSON without the subprocess. One difference on purpose: record types
//! the crate does not model (OpenMW's LUAL and the like) are skipped rather than
//! failing the whole file, as the toolkit's native session always has.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use tes3::esp::Plugin;

/// The record tags `tes3::esp` reads (its `TES3Object` variants).
pub const TAGS: [&[u8; 4]; 43] = [
    b"TES3", b"GMST", b"GLOB", b"CLAS", b"FACT", b"RACE", b"SOUN", b"SNDG", b"SKIL", b"MGEF", b"SCPT",
    b"REGN", b"BSGN", b"SSCR", b"LTEX", b"SPEL", b"STAT", b"DOOR", b"MISC", b"WEAP", b"CONT", b"CREA",
    b"BODY", b"LIGH", b"ENCH", b"NPC_", b"ARMO", b"CLOT", b"REPA", b"ACTI", b"APPA", b"LOCK", b"PROB",
    b"INGR", b"BOOK", b"ALCH", b"LEVI", b"LEVC", b"CELL", b"LAND", b"PGRD", b"DIAL", b"INFO",
];

fn known(tag: [u8; 4]) -> bool {
    TAGS.iter().any(|t| **t == tag)
}

/// Reads a plugin, keeping the records the crate models.
pub fn read(bytes: &[u8]) -> io::Result<Plugin> {
    let mut plugin = Plugin::new();
    crate::guarded(|| plugin.load_bytes_filtered(bytes, known))?;
    Ok(plugin)
}

/// Writes a plugin's records as a JSON array to `out`; returns how many.
pub fn to_json_file(src: &Path, out: &Path) -> io::Result<usize> {
    let plugin = read(&std::fs::read(src)?)?;
    let tmp = out.with_extension("json.tmp");
    {
        let mut w = BufWriter::new(File::create(&tmp)?);
        serde_json::to_writer(&mut w, &plugin.objects).map_err(io::Error::other)?;
        w.flush()?;
    }
    std::fs::rename(&tmp, out)?;
    Ok(plugin.objects.len())
}

/// A record the crate does not model, as the JSON bridge carries it: its tag, its
/// flags word and its body (the subrecords after the flags), base64'd - written back
/// byte for byte.
const RAW: &str = "__Raw";

/// Whether serialized JSON holds a `null` outside strings (a non-finite float).
fn contains_null(json: &[u8]) -> bool {
    let (mut in_str, mut esc) = (false, false);
    for (i, &c) in json.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'n' && json[i..].starts_with(b"null") {
            return true;
        }
    }
    false
}

/// A plugin's records as a JSON array, in file order: each modelled record in the
/// tes3conv schema, each other one as `{"type": "__Raw", "tag", "flags", "body"}`.
/// With `keep`, only the records whose tags are in it (unknown or not).
pub fn records_json(bytes: &[u8], keep: Option<&[[u8; 4]]>) -> io::Result<String> {
    use base64::Engine as _;
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
    let mut out = Vec::with_capacity(bytes.len() * 2);
    out.push(b'[');
    let mut first = true;
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes.len() - pos < 16 {
            return Err(bad(format!("a record header at {pos} runs past the end of the file")));
        }
        let tag: [u8; 4] = bytes[pos..pos + 4].try_into().unwrap();
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let end = pos.checked_add(16 + size).filter(|&e| e <= bytes.len()).ok_or_else(|| {
            bad(format!("the {} record at {pos} runs past the end of the file", String::from_utf8_lossy(&tag)))
        })?;
        if keep.is_none_or(|k| k.contains(&tag)) {
            if !first {
                out.push(b',');
            }
            first = false;
            if known(tag) {
                let obj: tes3::esp::TES3Object = crate::guarded(|| bytes_io::Reader::new(&bytes[pos..end]).load()).map_err(|e| {
                    bad(format!("the {} record at {pos}: {e}", String::from_utf8_lossy(&tag)))
                })?;
                let text = serde_json::to_vec(&obj).map_err(io::Error::other)?;
                if contains_null(&text) {
                    // A NaN or infinite float, which JSON has no number for (serde_json
                    // writes null, and cannot read it back - tes3conv shares this). The
                    // record's own bytes ride along, so an unchanged record is written
                    // back exactly as it was read.
                    let mut v: serde_json::Value = serde_json::from_slice(&text).map_err(io::Error::other)?;
                    v["__raw"] = serde_json::Value::from(base64::engine::general_purpose::STANDARD.encode(&bytes[pos + 16..end]));
                    v["__raw_flags"] = serde_json::Value::from(u32::from_le_bytes(bytes[pos + 12..pos + 16].try_into().unwrap()));
                    serde_json::to_writer(&mut out, &v).map_err(io::Error::other)?;
                } else {
                    out.extend_from_slice(&text);
                }
            } else {
                let flags = u32::from_le_bytes(bytes[pos + 12..pos + 16].try_into().unwrap());
                let raw = serde_json::json!({
                    "type": RAW,
                    "tag": tag.iter().map(|&b| b as char).collect::<String>(),
                    "flags": flags,
                    "body": base64::engine::general_purpose::STANDARD.encode(&bytes[pos + 16..end]),
                });
                serde_json::to_writer(&mut out, &raw).map_err(io::Error::other)?;
            }
        }
        pos = end;
    }
    out.push(b']');
    String::from_utf8(out).map_err(io::Error::other)
}

/// A JSON array of records (as `records_json` gives them) back to plugin bytes, in
/// order, each written as it stands - no record count is recomputed.
pub fn records_bytes(json: &str) -> io::Result<Vec<u8>> {
    use base64::Engine as _;
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
    let items: Vec<serde_json::Value> = serde_json::from_str(json).map_err(|e| bad(e.to_string()))?;
    let mut w = bytes_io::Writer::new(Vec::new());
    for (i, v) in items.into_iter().enumerate() {
        if v.get("type").and_then(|t| t.as_str()) == Some(RAW) {
            let tag = v["tag"].as_str().unwrap_or("");
            let tag: Vec<u8> = tag.chars().map(|c| c as u32 as u8).collect();
            if tag.len() != 4 {
                return Err(bad(format!("record {i}: a raw record's tag must be four bytes")));
            }
            let body = base64::engine::general_purpose::STANDARD
                .decode(v["body"].as_str().unwrap_or(""))
                .map_err(|e| bad(format!("record {i}: {e}")))?;
            let flags = v["flags"].as_u64().unwrap_or(0) as u32;
            let c = &mut w.cursor;
            use std::io::Write as _;
            c.write_all(&tag)?;
            c.write_all(&(body.len() as u32).to_le_bytes())?;
            c.write_all(&0u32.to_le_bytes())?;
            c.write_all(&flags.to_le_bytes())?;
            c.write_all(&body)?;
        } else {
            let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("?").to_string();
            let obj: tes3::esp::TES3Object =
                serde_json::from_value(v).map_err(|e| bad(format!("record {i} ({kind}): {e}")))?;
            crate::guarded(|| w.save(&obj))?;
        }
    }
    Ok(w.cursor.into_inner())
}

/// A plugin's records the crate models, in file order, each loaded on its own; `keep`
/// limits it to those tags. Record types the crate does not model are skipped, as
/// `read` skips them; a malformed record fails the whole file, as it does there.
///
/// `light` empties the bulky parts no record key or cell list needs - a cell's
/// references, a landscape's vertex data, a path grid's points and connections, a
/// script's text and bytecode - so a whole plugin can be keyed without holding them.
pub fn objects(bytes: &[u8], keep: Option<&[[u8; 4]]>, light: bool) -> io::Result<Vec<tes3::esp::TES3Object>> {
    Ok(items(bytes, keep, light, false)?
        .into_iter()
        .filter_map(|i| match i {
            Item::Obj(o) => Some(o),
            Item::Raw { .. } => None,
        })
        .collect())
}

/// One record as `items` gives it: modelled, or (with `raw`) any other tag as it is.
pub enum Item {
    Obj(tes3::esp::TES3Object),
    Raw { tag: [u8; 4], flags: u32, body: Vec<u8> },
}

/// `objects`, and with `raw` every record the crate does not model as its tag, flags
/// and body, in its place in the file.
pub fn items(bytes: &[u8], keep: Option<&[[u8; 4]]>, light: bool, raw: bool) -> io::Result<Vec<Item>> {
    use tes3::esp::TES3Object as O;
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes.len() - pos < 16 {
            return Err(bad(format!("a record header at {pos} runs past the end of the file")));
        }
        let tag: [u8; 4] = bytes[pos..pos + 4].try_into().unwrap();
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let end = pos.checked_add(16 + size).filter(|&e| e <= bytes.len()).ok_or_else(|| {
            bad(format!("the {} record at {pos} runs past the end of the file", String::from_utf8_lossy(&tag)))
        })?;
        if !known(tag) && raw && keep.is_none_or(|k| k.contains(&tag)) {
            let flags = u32::from_le_bytes(bytes[pos + 12..pos + 16].try_into().unwrap());
            out.push(Item::Raw { tag, flags, body: bytes[pos + 16..end].to_vec() });
        } else if known(tag) && keep.is_none_or(|k| k.contains(&tag)) {
            let mut obj: O = crate::guarded(|| bytes_io::Reader::new(&bytes[pos..end]).load())
                .map_err(|e| bad(format!("the {} record at {pos}: {e}", String::from_utf8_lossy(&tag))))?;
            if light {
                match &mut obj {
                    O::Cell(c) => c.references.clear(),
                    O::Landscape(l) => {
                        let (flags, grid) = (l.flags, l.grid);
                        *l = Default::default();
                        (l.flags, l.grid) = (flags, grid);
                    }
                    O::PathGrid(g) => {
                        g.points.clear();
                        g.connections.clear();
                    }
                    O::Script(s) => {
                        s.text.clear();
                        s.bytecode.clear();
                    }
                    _ => {}
                }
            }
            out.push(Item::Obj(obj));
        }
        pos = end;
    }
    Ok(out)
}

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

/// The bytes the crate's serde support would zstd-compress and base64 into a string:
/// what `Save` writes for the field (a `Box`'s array as it is; a `Vec` as a `u32`
/// count, then its elements).
fn blob<T: bytes_io::Save>(v: &T) -> Vec<u8> {
    let mut w = bytes_io::Writer::new(Vec::new());
    let _ = w.save(v);
    w.cursor.into_inner()
}

/// A record's packed number arrays, taken out before the record goes to Python - the
/// only fields the crate's serde support packs (the five landscape grids, a script's
/// variables and bytecode, a path grid's connections). Each is replaced by an empty or
/// zeroed value, so nothing is compressed only to be decompressed again.
/// `(path, bytes)`: the dict key, and the key inside it for a landscape grid.
fn take_blobs(o: &mut tes3::esp::TES3Object) -> Vec<(&'static str, Option<&'static str>, Vec<u8>)> {
    use tes3::esp::TES3Object as O;
    let mut out = Vec::new();
    match o {
        O::Landscape(l) => {
            out.push(("vertex_normals", Some("data"), blob(&l.vertex_normals.data)));
            out.push(("vertex_heights", Some("data"), blob(&l.vertex_heights.data)));
            out.push(("world_map_data", Some("data"), blob(&l.world_map_data.data)));
            out.push(("vertex_colors", Some("data"), blob(&l.vertex_colors.data)));
            out.push(("texture_indices", Some("data"), blob(&l.texture_indices.data)));
            l.vertex_normals = Default::default();
            l.vertex_heights = tes3::esp::VertexHeights { offset: l.vertex_heights.offset, ..Default::default() };
            l.world_map_data = Default::default();
            l.vertex_colors = Default::default();
            l.texture_indices = Default::default();
        }
        O::Script(sc) => {
            out.push(("variables", None, blob(&sc.variables)));
            out.push(("bytecode", None, blob(&sc.bytecode)));
            sc.variables.clear();
            sc.bytecode.clear();
        }
        O::PathGrid(g) => {
            out.push(("connections", None, blob(&g.connections)));
            g.connections.clear();
        }
        _ => {}
    }
    out
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

/// One record's conflict key, as `wraithguard_toolkit._tes3conv_record_key` reads it
/// from the record's tes3conv-schema form: `(type, id)`, with id-less records keyed by
/// their grid (`"(x, y)"`) and cell-scoped ones (path grids) by their cell - the name
/// alone for an interior, name and grid for a named exterior. `None` for the header
/// and anything with no usable id. The logic is the Python's, line for line, on the
/// same fields.
fn record_key(v: &serde_json::Value, interior: &std::collections::HashSet<String>) -> Option<(String, String)> {
    use serde_json::Value;
    let rtype = v.get("type")?.as_str()?;
    if rtype.is_empty() || rtype.eq_ignore_ascii_case("header") || rtype.eq_ignore_ascii_case("tes3") {
        return None;
    }
    let text = |x: Option<&Value>| -> Option<String> {
        match x? {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::String(_) | Value::Null | Value::Bool(false) => None,
            Value::Number(n) if n.as_f64() == Some(0.0) => None,
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    };
    let data = v.get("data").filter(|d| d.is_object());
    if let Some(rid) = text(v.get("id")).or_else(|| text(v.get("name"))) {
        return Some((rtype.to_string(), rid));
    }
    let grid = v.get("grid").filter(|g| !g.is_null()).or_else(|| data.and_then(|d| d.get("grid")));
    let (gx, gy) = match grid.and_then(|g| g.as_array()) {
        Some(a) if a.len() >= 2 => (a[0].as_i64(), a[1].as_i64()),
        _ => (None, None),
    };
    let cell = text(v.get("cell"))
        .or_else(|| text(v.get("cell_name")))
        .or_else(|| data.and_then(|d| text(d.get("cell")).or_else(|| text(d.get("cell_name")))));
    let rid = if let Some(cell) = cell {
        if interior.contains(&cell.to_lowercase()) {
            cell
        } else {
            let f = |x: Option<i64>| x.map(|n| n.to_string()).unwrap_or_else(|| "None".into());
            format!("{cell} ({}, {})", f(gx), f(gy))
        }
    } else if let (Some(x), Some(y)) = (gx, gy) {
        format!("({x}, {y})")
    } else {
        return None;
    };
    Some((rtype.to_string(), rid))
}

/// Whether a record's flags say deleted (`_rec_deleted`: "DELETED" in the flag text).
fn record_deleted(v: &serde_json::Value) -> bool {
    match v.get("flags") {
        Some(serde_json::Value::String(s)) => s.to_lowercase().contains("delet"),
        Some(serde_json::Value::Number(n)) => n.as_u64().is_some_and(|f| f & 0x20 != 0),
        Some(serde_json::Value::Array(a)) => a.iter().any(|f| f.to_string().to_lowercase().contains("delet")),
        _ => v.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false),
    }
}

/// A cell of `keys_and_cells`: an interior by name, or an exterior by grid.
pub enum CellRef {
    Int(String),
    Ext(i64, i64),
}

/// A plugin's conflict keys and the cells it touches - what
/// `wraithguard_toolkit._keys_and_cells` computes from its records, computed here
/// without building those records in Python: `(type, id, deleted)` per record, first
/// wins, and each CELL as an interior name or an exterior grid.
pub fn keys_and_cells(bytes: &[u8]) -> io::Result<(Vec<(String, String, bool)>, Vec<CellRef>)> {
    let values: Vec<serde_json::Value> = objects(bytes, None, true)?
        .iter()
        .map(|o| serde_json::to_value(o).map_err(io::Error::other))
        .collect::<io::Result<_>>()?;
    let is_cell = |v: &serde_json::Value| v.get("type").and_then(|t| t.as_str()).is_some_and(|t| t.eq_ignore_ascii_case("cell"));
    let mut interior = std::collections::HashSet::new();
    for v in values.iter().filter(|v| is_cell(v)) {
        let flags = v.get("data").and_then(|d| d.get("flags"));
        let inside = match flags {
            Some(serde_json::Value::Number(n)) => n.as_u64().is_some_and(|f| f & 1 != 0),
            Some(serde_json::Value::Null) | None => false,
            Some(f) => f.to_string().to_uppercase().contains("INTERIOR"),
        };
        if inside {
            let name = v.get("id").and_then(|x| x.as_str()).filter(|s| !s.is_empty())
                .or_else(|| v.get("name").and_then(|x| x.as_str()).filter(|s| !s.is_empty()));
            if let Some(n) = name {
                interior.insert(n.to_lowercase());
            }
        }
    }
    let mut keys = Vec::new();
    let mut cells = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for v in &values {
        let Some((rtype, rid)) = record_key(v, &interior) else { continue };
        if !seen.insert((rtype.clone(), rid.clone())) {
            continue;
        }
        let deleted = record_deleted(v);
        if rtype.eq_ignore_ascii_case("cell") {
            let name = v.get("id").and_then(|x| x.as_str()).filter(|s| !s.is_empty())
                .or_else(|| v.get("name").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
                .map(str::to_string)
                .unwrap_or_else(|| rid.clone());
            if interior.contains(&name.to_lowercase()) {
                cells.push(CellRef::Int(name));
            } else {
                let grid = v.get("data").and_then(|d| d.get("grid")).filter(|g| !g.is_null()).or_else(|| v.get("grid"));
                match grid.and_then(|g| g.as_array()).filter(|a| a.len() >= 2) {
                    Some(a) => {
                        if let (Some(x), Some(y)) = (a[0].as_i64(), a[1].as_i64()) {
                            cells.push(CellRef::Ext(x, y));
                        }
                    }
                    None => {
                        // A key of the form "(x, y)".
                        let t = rid.trim();
                        if let Some(inner) = t.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
                            let mut it = inner.split(", ");
                            if let (Some(x), Some(y), None) = (it.next(), it.next(), it.next())
                                && let (Ok(x), Ok(y)) = (x.parse(), y.parse())
                            {
                                cells.push(CellRef::Ext(x, y));
                            }
                        }
                    }
                }
            }
        }
        keys.push((rtype, rid, deleted));
    }
    Ok((keys, cells))
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

#[cfg(test)]
mod tests {
    use super::*;

    const LAMP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../viewer-shell/ui/tests/fixture/Data Files/Lamp.esm");

    #[test]
    fn converts_a_plugin_to_tes3conv_json() {
        let out = std::env::temp_dir().join(format!("wgesp{}.json", std::process::id()));
        let n = to_json_file(Path::new(LAMP), &out).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        std::fs::remove_file(&out).ok();
        let recs = v.as_array().unwrap();
        assert_eq!(recs.len(), n);
        assert_eq!(recs[0]["type"], "Header");
        assert!(recs.iter().any(|r| r["type"] == "Cell"));
    }

    #[test]
    fn records_round_trip_through_json_byte_for_byte() {
        let mut bytes = std::fs::read(LAMP).unwrap();
        // An OpenMW-only record in the middle: carried raw, and written back as it was.
        bytes.extend_from_slice(b"LUAL");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0x20u32.to_le_bytes());
        bytes.extend_from_slice(b"junk");
        let json = records_json(&bytes, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let last = v.as_array().unwrap().last().unwrap();
        assert_eq!(last["type"], RAW);
        assert_eq!(last["tag"], "LUAL");
        assert_eq!(last["flags"], 0x20);
        // Written back: the raw record exactly as it was, and a second round changes nothing
        // (the fixture itself is not in the crate's canonical layout, so the first write
        // may normalise it).
        let out = records_bytes(&json).unwrap();
        assert!(out.ends_with(&bytes[bytes.len() - 20..]), "the raw LUAL record came back changed");
        assert!(records_bytes(&records_json(&out, None).unwrap()).unwrap() == out, "not stable");
        // Filtered: only the tags asked for.
        let cells = records_json(&bytes, Some(&[*b"CELL"])).unwrap();
        let cv: serde_json::Value = serde_json::from_str(&cells).unwrap();
        assert!(cv.as_array().unwrap().iter().all(|r| r["type"] == "Cell"));
        // A record running past the end is an error, not a short read.
        assert!(records_json(&bytes[..bytes.len() - 1], None).is_err());
    }

    #[test]
    fn unknown_record_types_are_skipped_not_fatal() {
        let mut bytes = std::fs::read(LAMP).unwrap();
        // An OpenMW-only record appended: tag, size, header flags, then its body.
        bytes.extend_from_slice(b"LUAL");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        bytes.extend_from_slice(b"junk");
        let with = read(&bytes).unwrap().objects.len();
        let without = read(&std::fs::read(LAMP).unwrap()).unwrap().objects.len();
        assert_eq!(with, without);
    }
}
