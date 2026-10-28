//! Plugins to and from tes3conv-schema JSON, on `tes3::esp` - moved here from
//! `native/src/esp.rs` (the Python module keeps its functions and calls these), so the
//! viewer's Editor and the Python one read a plugin the one way.
//!
//! tes3conv is greatness7's converter built on this same crate, so serialising its
//! records with serde (zstd'd base64 for the packed arrays, as tes3conv does) gives
//! tes3conv's JSON without the subprocess. One difference on purpose: record types
//! the crate does not model (OpenMW's LUAL and the like) are skipped rather than
//! failing the whole file, as the toolkit's native session always has.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use tes3::esp::Plugin;

use crate::guarded;

/// The record tags `tes3::esp` reads (its `TES3Object` variants).
pub const TAGS: [&[u8; 4]; 43] = [
    b"TES3", b"GMST", b"GLOB", b"CLAS", b"FACT", b"RACE", b"SOUN", b"SNDG", b"SKIL", b"MGEF", b"SCPT",
    b"REGN", b"BSGN", b"SSCR", b"LTEX", b"SPEL", b"STAT", b"DOOR", b"MISC", b"WEAP", b"CONT", b"CREA",
    b"BODY", b"LIGH", b"ENCH", b"NPC_", b"ARMO", b"CLOT", b"REPA", b"ACTI", b"APPA", b"LOCK", b"PROB",
    b"INGR", b"BOOK", b"ALCH", b"LEVI", b"LEVC", b"CELL", b"LAND", b"PGRD", b"DIAL", b"INFO",
];

pub fn known(tag: [u8; 4]) -> bool {
    TAGS.iter().any(|t| **t == tag)
}

/// Reads a plugin, keeping the records the crate models.
pub fn read(bytes: &[u8]) -> io::Result<Plugin> {
    let mut plugin = Plugin::new();
    guarded(|| plugin.load_bytes_filtered(bytes, known))?;
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
pub const RAW: &str = "__Raw";

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
                let obj: tes3::esp::TES3Object = guarded(|| bytes_io::Reader::new(&bytes[pos..end]).load()).map_err(|e| {
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
            guarded(|| w.save(&obj))?;
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
    Ok(items_pos(bytes, keep, light, raw)?.into_iter().map(|(_, _, i)| i).collect())
}

/// [`items`], each with its record's place in the file: `(start, end)`, header included.
fn items_pos(bytes: &[u8], keep: Option<&[[u8; 4]]>, light: bool, raw: bool) -> io::Result<Vec<(usize, usize, Item)>> {
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
            out.push((pos, end, Item::Raw { tag, flags, body: bytes[pos + 16..end].to_vec() }));
        } else if known(tag) && keep.is_none_or(|k| k.contains(&tag)) {
            let mut obj: O = guarded(|| bytes_io::Reader::new(&bytes[pos..end]).load())
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
            out.push((pos, end, Item::Obj(obj)));
        }
        pos = end;
    }
    Ok(out)
}

/// The bytes the crate's serde support would zstd-compress and base64 into a string:
/// what `Save` writes for the field (a `Box`'s array as it is; a `Vec` as a `u32`
/// count, then its elements).
pub fn blob<T: bytes_io::Save>(v: &T) -> Vec<u8> {
    let mut w = bytes_io::Writer::new(Vec::new());
    let _ = w.save(v);
    w.cursor.into_inner()
}

/// A record's packed number arrays, taken out before the record goes to Python - the
/// only fields the crate's serde support packs (the five landscape grids, a script's
/// variables and bytecode, a path grid's connections). Each is replaced by an empty or
/// zeroed value, so nothing is compressed only to be decompressed again.
/// `(path, bytes)`: the dict key, and the key inside it for a landscape grid.
pub fn take_blobs(o: &mut tes3::esp::TES3Object) -> Vec<(&'static str, Option<&'static str>, Vec<u8>)> {
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

/// One record's conflict key, as `wraithguard_toolkit._tes3conv_record_key` reads it
/// from the record's tes3conv-schema form: `(type, id)`, with id-less records keyed by
/// their grid (`"(x, y)"`) and cell-scoped ones (path grids) by their cell - the name
/// alone for an interior, name and grid for a named exterior. `None` for the header
/// and anything with no usable id. The logic is the Python's, line for line, on the
/// same fields.
pub fn conflict_key(v: &serde_json::Value, interior: &std::collections::HashSet<String>) -> Option<(String, String)> {
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
pub fn record_deleted(v: &serde_json::Value) -> bool {
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
    let (keys, cells, _) = analyse(&values, None);
    Ok((keys, cells))
}

/// A record's place in its file: `(type, id, offset, length)` - the whole record, header
/// included, so `bytes[offset..offset + length]` reads it back by itself ([`record_value`]).
pub type Span = (String, String, u64, u32);

/// [`keys_and_cells`], and where each key's record (the first of its key, as the keys count
/// it) lies in the file, from the one read: what lets a record be fetched by seeking to it
/// instead of reading the plugin through again.
pub fn keys_cells_spans(bytes: &[u8]) -> io::Result<(Vec<(String, String, bool)>, Vec<CellRef>, Vec<Span>)> {
    let mut values: Vec<serde_json::Value> = Vec::new();
    let mut places: Vec<(usize, usize)> = Vec::new();
    for (pos, end, item) in items_pos(bytes, None, true, false)? {
        if let Item::Obj(o) = item {
            values.push(serde_json::to_value(&o).map_err(io::Error::other)?);
            places.push((pos, end));
        }
    }
    Ok(analyse(&values, Some(&places)))
}

/// One record by itself - its bytes as [`keys_cells_spans`] places them - in its
/// tes3conv-schema form, whole (no references or vertices cleared).
pub fn record_value(span: &[u8]) -> io::Result<serde_json::Value> {
    let obj: tes3::esp::TES3Object = guarded(|| bytes_io::Reader::new(span).load())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("the record: {e}")))?;
    serde_json::to_value(&obj).map_err(io::Error::other)
}

/// The keys, cells and (given the records' places in the file) spans of records already in
/// their JSON form.
fn analyse(
    values: &[serde_json::Value],
    places: Option<&[(usize, usize)]>,
) -> (Vec<(String, String, bool)>, Vec<CellRef>, Vec<Span>) {
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
    let mut spans = Vec::new();
    for (vi, v) in values.iter().enumerate() {
        let Some((rtype, rid)) = conflict_key(v, &interior) else { continue };
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
        if let Some(&(pos, end)) = places.and_then(|p| p.get(vi)) {
            spans.push((rtype.clone(), rid.clone(), pos as u64, (end - pos) as u32));
        }
        keys.push((rtype, rid, deleted));
    }
    (keys, cells, spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAMP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/tests/fixture/Data Files/Lamp.esm");

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
