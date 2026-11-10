//! A plugin edited in place: the queue's decisions written into one of the load order's own
//! plugins, the file otherwise as it was (`TODO.md`, Next, 1; Static, 2026-11-09). The
//! Construction Set's active file, where a patch build makes a new one.
//!
//! The file is not read into records and written out again. The crate that reads plugins does
//! not write every record back as the same bytes (padding the game left in an `AIDT`, a
//! script's missing terminator, an empty `SCVR` - `tests/roundtrip.rs` lists them over the
//! game's masters), so a record the build does not touch is copied as its bytes, and only
//! the ones it decides are encoded:
//!
//! - a record the queue decides (taken whole, merged, made) takes the place of the file's own
//!   record of that key, or is added: a response into its topic's block, where its links
//!   place it (as merge_to_master does), anything else at the end of the file;
//! - a cell whose references the queue changes or adds to is the file's own record with
//!   those changes (or a new record for a cell the file has none of);
//! - the header keeps its author, description and masters; the record count is set, and a
//!   master the build needs besides is added after its own (so the file's references keep
//!   their numbers).

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::build::{build_records, keys_of, master_names, next_new_index, patch_bytes, BuildError, Kept, PatchResult, Plan, Sources};
use crate::key::{record_key, type_name};

type Res<T> = Result<T, BuildError>;

fn fail<T>(m: impl Into<String>) -> Res<T> {
    Err(BuildError(m.into()))
}

/// One record of the file as it lies there.
struct Raw<'a> {
    bytes: &'a [u8],
    /// Its type and key (lower case), when the crate reads it; a response's key is
    /// `topic\0id`.
    key: Option<(String, String)>,
    /// The topic it is, or is a response of (lower case).
    topic: Option<String>,
    is_info: bool,
}

/// The file's records as they lie, header first.
fn frames(original: &[u8]) -> Res<Vec<&[u8]>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < original.len() {
        if at + 16 > original.len() {
            return fail("the plugin ends inside a record's header: it is cut short, and is not written over");
        }
        let size = u32::from_le_bytes(original[at + 4..at + 8].try_into().expect("4 bytes")) as usize;
        let end = at + 16 + size;
        if end > original.len() {
            return fail("the plugin ends inside a record: it is cut short, and is not written over");
        }
        out.push(&original[at..end]);
        at = end;
    }
    if out.first().is_none_or(|h| &h[..4] != b"TES3") {
        return fail("the file does not begin with a plugin's header");
    }
    Ok(out)
}

fn info_key(topic: &str, id: &str) -> String {
    format!("{}\0{}", topic.to_lowercase(), id.to_lowercase())
}

/// The header with its record count set and `more` masters added after its own.
fn header_bytes(header: &[u8], count: usize, more: &[(String, u64)]) -> Res<Vec<u8>> {
    let mut out = header.to_vec();
    let mut at = 16;
    let mut set = false;
    while at + 8 <= out.len() {
        let size = u32::from_le_bytes(out[at + 4..at + 8].try_into().expect("4 bytes")) as usize;
        if &out[at..at + 4] == b"HEDR" {
            if size < 300 {
                return fail("the plugin's header is not the size a Morrowind plugin's is");
            }
            let n = u32::try_from(count).map_err(|_| BuildError("too many records".into()))?;
            out[at + 8 + 296..at + 8 + 300].copy_from_slice(&n.to_le_bytes());
            set = true;
        }
        at += 8 + size;
    }
    if !set {
        return fail("the plugin's header has no HEDR");
    }
    for (name, size) in more {
        if name.is_empty() || *size == 0 {
            return fail(format!("master {name:?} has no usable size ({size})"));
        }
        let mut text: Vec<u8> = name.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect();
        text.push(0);
        out.extend_from_slice(b"MAST");
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(b"DATA");
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
    }
    let body = u32::try_from(out.len() - 16).map_err(|_| BuildError("the header is too big".into()))?;
    out[4..8].copy_from_slice(&body.to_le_bytes());
    Ok(out)
}

/// One record as plugin bytes.
fn encode(record: &Value) -> Res<Vec<u8>> {
    patch_bytes(std::slice::from_ref(record))
}

/// The plugin `name`, whose bytes are `original`, with the plan's decisions written into
/// it: the new file's bytes and what was made. `say` hears each line of the report.
pub fn build_in_place(
    plan: &Plan,
    sources: &Sources,
    load_order: &[String],
    sizes: &HashMap<String, u64>,
    name: &str,
    original: &[u8],
    say: &mut dyn FnMut(&str),
) -> Res<(Vec<u8>, PatchResult)> {
    if !plan.carried.is_empty() {
        return fail("a plugin edited in place carries nothing forward: it is the file");
    }
    let spans = frames(original)?;
    // Each record's key, read off its own bytes; one the crate does not read has none and is
    // only ever copied.
    let mut raws: Vec<Raw> = Vec::with_capacity(spans.len());
    let mut values: Vec<Option<Value>> = Vec::with_capacity(spans.len());
    let mut topic: Option<String> = None;
    for (i, span) in spans.iter().enumerate() {
        let value = if i == 0 { None } else { crate::plugin::record_value(span).ok() };
        let rtype = value.as_ref().map(type_name).unwrap_or("").to_string();
        let key = value.as_ref().map(record_key).unwrap_or_default();
        let is_info = rtype == "DialogueInfo";
        if rtype == "Dialogue" {
            topic = Some(key.to_lowercase());
        } else if !is_info {
            topic = None;
        }
        let key = match (rtype.is_empty() || key.is_empty(), is_info) {
            (true, _) => None,
            (false, true) => Some((rtype, info_key(topic.as_deref().unwrap_or(""), &key))),
            (false, false) => Some((rtype, key.to_lowercase())),
        };
        raws.push(Raw { bytes: span, key, topic: topic.clone(), is_info });
        values.push(value);
    }
    let header: Value = serde_json::from_str(&crate::plugin::records_json(spans[0], None).map_err(|e| BuildError(format!("the plugin's header does not read: {e}")))?)
        .ok()
        .and_then(|v: Value| v.as_array().and_then(|a| a.first().cloned()))
        .ok_or_else(|| BuildError("the plugin's header does not read".into()))?;
    let own_masters = master_names(std::slice::from_ref(&header));
    // The last record of a key is the one the game has, and the one replaced.
    let mut at_of: HashMap<(String, String), usize> = HashMap::new();
    for (i, r) in raws.iter().enumerate() {
        if let Some(k) = &r.key {
            at_of.insert(k.clone(), i);
        }
    }

    // The file's own records the build changes without deciding them whole: the cells whose
    // references it changes or adds to. They ride into the build as an earlier build's would.
    let decided: HashSet<(String, String)> = plan
        .selections
        .iter()
        .map(|s| (s.record_type.clone(), s.key.to_lowercase()))
        .chain(plan.merges.iter().map(|m| (m.record_type.clone(), m.key.to_lowercase())))
        .chain(plan.new_records.iter().map(|m| (m.record_type.clone(), m.key.to_lowercase())))
        .collect();
    let touched_cells: HashSet<String> = plan.ref_edits.iter().map(|e| e.cell.to_lowercase()).chain(plan.new_refs.iter().map(|n| n.cell.to_lowercase())).collect();
    let mut carried = vec![header.clone()];
    let mut all_cells: Vec<Value> = Vec::new();
    for v in values.iter().flatten().filter(|v| type_name(v) == "Cell") {
        if keys_of(v).iter().any(|k| touched_cells.contains(&k.to_lowercase())) && !keys_of(v).iter().any(|k| decided.contains(&("Cell".to_string(), k.to_lowercase()))) {
            carried.push(v.clone());
        }
        // Only the indices matter here: the references' numbers, not the records.
        all_cells.push(json!({"type": "Cell", "references": v.get("references").cloned().unwrap_or(Value::Null)}));
    }
    let next_ref = next_new_index(&all_cells);
    drop(all_cells);
    let plan = Plan { carried: if carried.len() > 1 { carried } else { Vec::new() }, ..plan.clone() };
    let kept = Kept { masters: &own_masters, next_ref };
    let (built, mut result) = build_records(&plan, sources, load_order, sizes, name, Some(&kept), say)?;

    // Where each built record goes: in place of the file's own record of its key, into its
    // topic's block (a response), or at the end.
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_lowercase();
    let mut replace: HashMap<usize, Vec<u8>> = HashMap::new();
    // A topic's responses the build decides: `(id, prev_id, bytes)`, in the build's order.
    let mut into_topic: HashMap<String, Vec<(String, String, Vec<u8>)>> = HashMap::new();
    let mut tail: Vec<Vec<u8>> = Vec::new();
    let (mut replaced, mut added) = (0usize, 0usize);
    let mut current: Option<(String, bool)> = None;
    for record in &built {
        let rtype = type_name(record).to_string();
        let key = record_key(record).to_lowercase();
        match rtype.as_str() {
            "Dialogue" => {
                let there = at_of.get(&(rtype.clone(), key.clone())).copied();
                current = Some((key.clone(), there.is_some()));
                match there {
                    // The topic a response rides behind: the file's own stands as it is.
                    Some(_) if !decided.contains(&(rtype.clone(), key.clone())) => {}
                    Some(i) => {
                        replace.insert(i, encode(record)?);
                        replaced += 1;
                    }
                    None => {
                        tail.push(encode(record)?);
                        added += 1;
                    }
                }
            }
            "DialogueInfo" => {
                let (topic, topic_there) = current.clone().unwrap_or_default();
                if topic_there {
                    if at_of.contains_key(&(rtype.clone(), info_key(&topic, &key))) {
                        replaced += 1;
                    } else {
                        added += 1;
                    }
                    into_topic.entry(topic).or_default().push((key, text(record, "prev_id"), encode(record)?));
                } else {
                    tail.push(encode(record)?);
                    added += 1;
                }
            }
            _ => {
                current = None;
                // A cell answers to its grid or its name.
                let there = keys_of(record).iter().find_map(|k| at_of.get(&(rtype.clone(), k.to_lowercase())).copied()).or_else(|| at_of.get(&(rtype.clone(), key.clone())).copied());
                match there {
                    Some(i) => {
                        replace.insert(i, encode(record)?);
                        replaced += 1;
                    }
                    None => {
                        tail.push(encode(record)?);
                        added += 1;
                    }
                }
            }
        }
    }

    // The file again: each record as it lay, or its replacement; the rest at the end. A
    // topic's block is put together anew when the build decides a response of it, each one
    // placed by its links as merge_to_master places them (greatness7's `insert_info`): one
    // already there whose `prev_id` is unchanged stays where it is; else it goes after the
    // response its `prev_id` names, first with none, last when that one is not in the block.
    let mut body: Vec<u8> = Vec::with_capacity(original.len() + tail.iter().map(Vec::len).sum::<usize>());
    let mut count = 0usize;
    let mut i = 1;
    while i < raws.len() {
        let raw = &raws[i];
        match replace.get(&i) {
            Some(bytes) => body.extend_from_slice(bytes),
            None => body.extend_from_slice(raw.bytes),
        }
        count += 1;
        i += 1;
        let decided_here = if raw.is_info { None } else { raw.topic.as_ref().and_then(|t| into_topic.remove(t)) };
        let Some(decided_here) = decided_here else { continue };
        // The block: `(id, prev_id, bytes)`.
        let mut block: Vec<(String, String, std::borrow::Cow<[u8]>)> = Vec::new();
        while i < raws.len() && raws[i].is_info {
            let v = values[i].as_ref();
            block.push((v.map(|v| text(v, "id")).unwrap_or_default(), v.map(|v| text(v, "prev_id")).unwrap_or_default(), std::borrow::Cow::Borrowed(raws[i].bytes)));
            i += 1;
        }
        for (id, prev, bytes) in decided_here {
            if let Some(at) = block.iter().position(|b| b.0 == id) {
                if block[at].1 == prev {
                    block[at].2 = std::borrow::Cow::Owned(bytes);
                    continue;
                }
                block.remove(at);
            }
            let entry = (id, prev.clone(), std::borrow::Cow::Owned(bytes));
            if prev.is_empty() {
                block.insert(0, entry);
            } else if let Some(at) = block.iter().position(|b| b.0 == prev) {
                block.insert(at + 1, entry);
            } else {
                block.push(entry);
            }
        }
        for (_, _, bytes) in &block {
            body.extend_from_slice(bytes);
            count += 1;
        }
    }
    for bytes in &tail {
        body.extend_from_slice(bytes);
        count += 1;
    }
    let have: HashSet<String> = own_masters.iter().map(|n| n.to_lowercase()).collect();
    let mut more: Vec<(String, u64)> = Vec::new();
    for n in result.masters.iter().filter(|n| !have.contains(&n.to_lowercase())) {
        let Some(size) = sizes.get(n) else { return fail(format!("cannot measure {n}. A master's size goes in the header, and a wrong one makes the plugin look corrupt.")) };
        more.push((n.clone(), *size));
    }
    let mut out = header_bytes(spans[0], count, &more)?;
    out.extend_from_slice(&body);

    let mut line = |t: String| {
        say(&t);
        result.lines.push(t);
    };
    line(format!("{name} edited in place: {replaced} record(s) changed, {added} added, {} left as they were", count - replaced - added));
    if !more.is_empty() {
        line(format!("master(s) added to its header: {}", more.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")));
    }
    result.records = count;
    result.carried = count - replaced - added;
    Ok((out, result))
}
