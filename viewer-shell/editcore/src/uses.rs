//! The Construction Set's Use Report over a load order, and Search & Replace over it:
//! `wraithguard/patch/uses.py`, `usesindex.py` and `EditorSession.uses`, `replace_plan`
//! and `replace_uses` (port step 4.1, slice 5b). Cases: `tests/fixtures/use_cases.json`,
//! run here and by `tests/test_use_cases.py`.
//!
//! A record uses an id when a string in it equals the id (any case: a leveled list's
//! entry, an inventory's item, a spell list, a reference placed in a cell...), or when
//! its script source names the id as a word (a script's text, a dialogue response's
//! result). Each use says which plugin's version of the using record it is in and
//! whether that version is the one the load order uses (the last plugin with the
//! record); a cell is not last-wins - each plugin's CELL record adds references to it -
//! so a reference placed by any plugin counts.
//!
//! Three ways to the same answer:
//! - [`report`], the files scanned (was `native/src/uses.rs`): each plugin mapped, and
//!   parsed only when its bytes hold the id at all, on threads;
//! - [`report_with`], over records already read (the tests' seam, as the Python's
//!   `use_report`);
//! - [`from_index`], one query of the use index in the record cache ([`index_rows`] are
//!   its rows: every string a record holds, and the words of its script source).

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

use indexmap::{IndexMap, IndexSet};
use ordercore::refs::origin;
use serde_json::{json, Value};
use tes3::esp::TypeInfo;

use crate::cache::{Cache, UseHit, UseRow};
use crate::frames::{records, subrecords, zstr};
use crate::edit::{err, set_at, value_at, EditError, Editor, SCRIPT_TEXT};
use crate::key::{record_key, tag_of, type_name};
use crate::queue::{Choice, NewRecord, NewRef, RefEdit};
use crate::scripts::Compile;
use crate::search::{word_char, Finder};
use crate::session::{truthy, Found};

/// One record that uses the id.
#[derive(Clone, Debug, PartialEq)]
pub struct Use {
    /// The plugin (its place in the load order) whose version of the record it is.
    pub plugin: usize,
    /// The using record's type and tag.
    pub record_type: String,
    pub tag: String,
    /// Its key.
    pub key: String,
    /// Where in it the id is: dotted paths, a cell's references as one (`references`),
    /// the source field for words of script source.
    pub paths: Vec<String>,
    /// How many times (a cell: how many references place it).
    pub count: usize,
    /// Whether this version is the one the load order uses.
    pub wins: bool,
}

/// The field of a record type that holds script source, naming ids as words: a
/// script's text, a dialogue response's result.
pub fn source_field(record_type: &str) -> Option<&'static str> {
    match record_type {
        "Script" => Some("text"),
        "DialogueInfo" => Some("script_text"),
        _ => None,
    }
}

/// The dotted paths in a value at which a string equal to `want` (lower case) sits.
fn walk(v: &Value, want: &str, path: &mut String, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if s.to_lowercase() == want {
                out.push(path.clone());
            }
        }
        Value::Object(m) => {
            for (k, x) in m {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(k);
                walk(x, want, path, out);
                path.truncate(len);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                let len = path.len();
                path.push('.');
                path.push_str(&i.to_string());
                walk(x, want, path, out);
                path.truncate(len);
            }
        }
        _ => {}
    }
}

/// One record's use of the id (`want`, lower case), or `None` when it has none: the
/// record itself (`record_type` and the id) is not a use of itself.
fn use_of(plugin: usize, rec: &Value, tag: &str, record_type: &str, want: &str, words: &Finder) -> Option<Use> {
    let kind = rec.get("type").and_then(Value::as_str)?;
    if kind == "Header" {
        return None;
    }
    let key = record_key(rec);
    if kind == record_type && key.to_lowercase() == want {
        return None;
    }
    let mut found = Vec::new();
    walk(rec, want, &mut String::new(), &mut found);
    let mut paths: Vec<String> = Vec::new();
    let mut count = 0;
    for p in found {
        if p.starts_with("references.") {
            count += 1;
            if !paths.iter().any(|x| x == "references") {
                paths.push("references".into());
            }
        } else if p != "id" {
            count += 1;
            paths.push(p);
        }
    }
    if let Some(field) = source_field(kind)
        && let Some(text) = rec.get(field).and_then(Value::as_str)
    {
        let hits = words.count(text);
        if hits > 0 {
            count += hits;
            paths.push(field.into());
        }
    }
    (count > 0).then(|| Use { plugin, record_type: kind.into(), tag: tag.into(), key, paths, count, wins: true })
}

/// The uses of an id in one plugin's records (`uses.uses_in`).
pub fn uses_in(plugin: usize, records: &[Value], record_type: &str, rid: &str) -> Vec<Use> {
    let want = rid.trim().to_lowercase();
    let words = Finder::new(&want, true);
    records
        .iter()
        .filter_map(|r| use_of(plugin, r, tag_of(type_name(r)).unwrap_or(""), record_type, &want, &words))
        .collect()
}

/// Every use of an id over a load order of `n` plugins, each read whole by `read` (in
/// load order; one that cannot be read has none): `uses.use_report`.
pub fn report_with(n: usize, read: impl Fn(usize) -> io::Result<Vec<Value>>, record_type: &str, rid: &str) -> Vec<Use> {
    let mut found = Vec::new();
    let mut last: HashMap<(String, String), usize> = HashMap::new();
    for i in 0..n {
        let Ok(records) = read(i) else { continue };
        for r in &records {
            last.insert((type_name(r).to_string(), record_key(r).to_lowercase()), i);
        }
        found.push(uses_in(i, &records, record_type, rid));
    }
    let mut out = Vec::new();
    for mut u in found.into_iter().flatten() {
        u.wins = u.record_type == "Cell" || last.get(&(u.record_type.clone(), u.key.to_lowercase())) == Some(&u.plugin);
        out.push(u);
    }
    out
}

// -- the files scanned -------------------------------------------------------------------

/// Whether `hay` holds `needle` (already lower case), ASCII case aside.
fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    let Some((&first, rest)) = needle.split_first() else { return true };
    let n = needle.len();
    if hay.len() < n {
        return false;
    }
    (0..=hay.len() - n).any(|i| hay[i].to_ascii_lowercase() == first && hay[i + 1..i + n].iter().zip(rest).all(|(a, b)| a.to_ascii_lowercase() == *b))
}

/// Every record of a plugin keyed the cheap way, from the file's framing (no parse):
/// `(tag, NAME or a response's INAM, lower case)`.
fn keys(data: &[u8]) -> HashSet<([u8; 4], String)> {
    let mut out = HashSet::new();
    for (tag, body) in records(data) {
        let Ok(tag) = <[u8; 4]>::try_from(tag) else { continue };
        let want: &[u8] = if &tag == b"INFO" { b"INAM" } else { b"NAME" };
        if let Some((_, d)) = subrecords(body).find(|(t, _)| *t == want) {
            out.insert((tag, zstr(d).to_lowercase()));
        }
    }
    out
}

/// One plugin's uses of the id (`wins` not yet known), and its keys.
fn scan(data: &[u8], plugin: usize, record_type: &str, want: &str, words: &Finder) -> io::Result<(Vec<Use>, HashSet<([u8; 4], String)>)> {
    let index = keys(data);
    let mut uses = Vec::new();
    if !contains_ci(data, want.as_bytes()) {
        return Ok((uses, index));
    }
    let parsed = crate::plugin::read(data)?;
    for obj in &parsed.objects {
        let tag = *obj.tag();
        if &tag == b"TES3" {
            continue;
        }
        let Ok(v) = serde_json::to_value(obj) else { continue };
        if let Some(u) = use_of(plugin, &v, &String::from_utf8_lossy(&tag), record_type, want, words) {
            uses.push(u);
        }
    }
    Ok((uses, index))
}

/// The Use Report from the plugin files: every record of the load order that names `id`
/// (other than the record itself, of `record_type`), in load order. Most plugins do not
/// hold the id's bytes and are passed over at the speed of a byte scan; a plugin that
/// cannot be read is passed over too.
pub fn report(paths: &[PathBuf], record_type: &str, id: &str) -> Vec<Use> {
    let want = id.trim().to_lowercase();
    let words = Finder::new(&want, true);
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16).min(paths.len().max(1));
    let mut results: Vec<Option<(Vec<Use>, HashSet<([u8; 4], String)>)>> = (0..paths.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|t| {
                let (want, words) = (&want, &words);
                s.spawn(move || {
                    let mut got = Vec::new();
                    for (i, p) in paths.iter().enumerate().skip(t).step_by(n) {
                        let one = std::fs::File::open(p)
                            // SAFETY: the plugin is only read, and only while mapped here.
                            .and_then(|f| unsafe { memmap2::Mmap::map(&f) })
                            .and_then(|m| scan(&m, i, record_type, want, words));
                        if let Ok(r) = one {
                            got.push((i, r));
                        }
                    }
                    got
                })
            })
            .collect();
        for h in handles {
            if let Ok(got) = h.join() {
                for (i, r) in got {
                    results[i] = Some(r);
                }
            }
        }
    });
    let mut last: HashMap<([u8; 4], String), usize> = HashMap::new();
    for (i, r) in results.iter().enumerate() {
        if let Some((_, index)) = r {
            for k in index {
                last.insert(k.clone(), i);
            }
        }
    }
    let mut out = Vec::new();
    for r in results.into_iter().flatten() {
        for mut u in r.0 {
            let tag: [u8; 4] = u.tag.as_bytes().try_into().unwrap_or(*b"    ");
            u.wins = u.record_type == "Cell" || last.get(&(tag, u.key.to_lowercase())).is_none_or(|&at| at == u.plugin);
            out.push(u);
        }
    }
    out
}

// -- the use index -----------------------------------------------------------------------

/// Strings longer than this are not ids (a book's text, a description): not indexed.
pub const MAX_VALUE: usize = 64;

/// Whether the index answers for an id: a run of word characters, and short. The word
/// rule (no word character either side) finds an id with a space or a dash where it is
/// not a word of the text; for those the script source is searched beside the index.
pub fn indexable(rid: &str) -> bool {
    let want = rid.trim();
    !want.is_empty() && want.chars().count() <= MAX_VALUE && want.chars().all(word_char)
}

/// Every string in a value with its dotted path.
fn strings<'a>(v: &'a Value, path: &mut String, out: &mut Vec<(String, &'a str)>) {
    match v {
        Value::String(s) => out.push((path.clone(), s)),
        Value::Object(m) => {
            for (k, x) in m {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(k);
                strings(x, path, out);
                path.truncate(len);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                let len = path.len();
                path.push('.');
                path.push_str(&i.to_string());
                strings(x, path, out);
                path.truncate(len);
            }
        }
        _ => {}
    }
}

/// A list of paths as Python's `json.dumps` writes it (the index stores that text).
fn dumps(paths: &[String]) -> String {
    let items: Vec<String> = paths.iter().map(|p| Value::from(p.as_str()).to_string()).collect();
    format!("[{}]", items.join(", "))
}

/// The use index's rows of one plugin's records (`usesindex.index_rows`): one per
/// `(string, using record)` - where in the record (`references` as one path) and how
/// many times; one per `(word, record)` of its script source; and one definition row per
/// record (its own id, count 0), which tells who defines what.
pub fn index_rows(records: &[Value]) -> Vec<UseRow> {
    let mut rows = Vec::new();
    for rec in records {
        let Some(kind) = rec.get("type").and_then(Value::as_str) else { continue };
        if kind == "Header" {
            continue;
        }
        let key = record_key(rec);
        let mut found = Vec::new();
        strings(rec, &mut String::new(), &mut found);
        let mut per: IndexMap<String, (Vec<String>, i64)> = IndexMap::new();
        for (path, text) in found {
            let low = text.to_lowercase();
            if low.is_empty() || low.chars().count() > MAX_VALUE || path == "id" {
                continue;
            }
            let e = per.entry(low).or_default();
            if path.starts_with("references.") {
                if !e.0.iter().any(|p| p == "references") {
                    e.0.push("references".into());
                }
            } else {
                e.0.push(path);
            }
            e.1 += 1;
        }
        rows.extend(per.into_iter().map(|(low, (p, n))| (low, kind.to_string(), key.clone(), dumps(&p), n)));
        if let Some(field) = source_field(kind)
            && let Some(body) = rec.get(field).and_then(Value::as_str)
        {
            let mut words: IndexMap<String, i64> = IndexMap::new();
            for w in body.split(|c: char| !word_char(c)).filter(|w| !w.is_empty()) {
                *words.entry(w.to_lowercase()).or_default() += 1;
            }
            let paths = dumps(&[field.to_string()]);
            rows.extend(words.into_iter().filter(|(w, _)| w.chars().count() <= MAX_VALUE).map(|(w, n)| (w, kind.to_string(), key.clone(), paths.clone(), n)));
        }
        rows.push((key.to_lowercase(), kind.to_string(), key, dumps(&["id".to_string()]), 0));
    }
    rows
}

/// The Use Report from the use index (`EditorSession._uses_from_index`), when it has every
/// plugin as it now is; `None` when it cannot answer (a plugin not indexed yet: the caller
/// scans, and indexes in the background). For an id that is not [`indexable`], the
/// script source of each plugin (`sources`: its scripts and responses, in that order) is
/// searched for it beside the index.
pub fn from_index(cache: &Cache, plugins: &[PathBuf], record_type: &str, rid: &str, sources: impl Fn(usize) -> Vec<Value>) -> Option<Vec<Use>> {
    let want = rid.trim().to_lowercase();
    if want.is_empty() {
        return None;
    }
    let mut rows: Vec<UseHit> = cache.uses_for(plugins, &want)?;
    if !indexable(rid) {
        let words = Finder::new(&want, true);
        for i in 0..plugins.len() {
            for rec in sources(i) {
                let kind = type_name(&rec);
                let Some(field) = source_field(kind) else { continue };
                let hits = rec.get(field).and_then(Value::as_str).map_or(0, |t| words.count(t));
                if hits > 0 {
                    rows.push((i, kind.to_string(), record_key(&rec), vec![field.to_string()], hits as i64));
                }
            }
        }
    }
    rows.retain(|r| !(r.1 == record_type && r.2.to_lowercase() == want));
    let wanted: IndexSet<(String, String)> = rows.iter().filter(|r| r.1 != "Cell").map(|r| (r.1.clone(), r.2.to_lowercase())).collect();
    let owners = cache.definers(plugins, &wanted.into_iter().collect::<Vec<_>>())?;
    let mut merged: IndexMap<(usize, String, String), Use> = IndexMap::new();
    for (i, rtype, key, paths, n) in rows {
        let u = merged.entry((i, rtype.clone(), key.clone())).or_insert_with(|| Use {
            plugin: i,
            tag: tag_of(&rtype).unwrap_or("").to_string(),
            record_type: rtype,
            key,
            paths: Vec::new(),
            count: 0,
            wins: true,
        });
        u.paths.extend(paths);
        u.count += n.max(0) as usize;
    }
    for u in merged.values_mut() {
        let last = owners.get(&(u.record_type.clone(), u.key.to_lowercase())).and_then(|v| v.last().copied()).unwrap_or(u.plugin);
        u.wins = u.record_type == "Cell" || last == u.plugin;
    }
    Some(merged.into_values().collect())
}

// -- the Use Report and Search & Replace in the Editor -----------------------------------

/// One change Search & Replace makes (`replace_plan`).
#[derive(Clone, Debug)]
pub enum Repoint {
    /// A field of a using record, with its new value (a script's source with the id
    /// renamed in it, [`crate::rename::replace_id`]).
    Field { user: Found, field: String, value: Value },
    /// A placed reference made to place the other record (its key kept: the engine
    /// merges it so).
    Ref(RefEdit),
    /// A reference the patch itself places, made to place the other record.
    New(NewRef),
}

/// The dialog's field that holds a dotted path: up to the first list (a list is one
/// field, as the conflict viewer keeps it).
fn field_of(record: &Value, path: &str) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = record;
    for (i, part) in parts.iter().enumerate() {
        if cur.is_array() {
            return parts[..i].join(".");
        }
        match cur.as_object().and_then(|m| m.get(*part)) {
            Some(v) => cur = v,
            None => return parts[..=i].join("."),
        }
    }
    path.to_string()
}

/// A copy of a value with every string equal to `old` (any case) made `new`.
fn swap(v: &Value, old: &str, new: &str) -> Value {
    match v {
        Value::String(s) if s.to_lowercase() == old.to_lowercase() => json!(new),
        Value::Array(a) => Value::Array(a.iter().map(|x| swap(x, old, new)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), swap(x, old, new))).collect()),
        _ => v.clone(),
    }
}

impl Editor {
    /// The record's Use Report: `{tag, id, uses: [{plugin, type, tag, key, paths, count,
    /// wins}], live, cells}` - `live` the uses in the versions the load order uses,
    /// `cells` the references placed. The plugins as they are; the patch's own records
    /// are not looked in.
    pub fn uses(&self, found: &Found) -> Value {
        let found_uses = self.session.use_report(&found.record_type, &found.key);
        let order = self.session.order();
        let rows: Vec<Value> = found_uses
            .iter()
            .map(|u| {
                json!({
                    "plugin": order.get(u.plugin).cloned().unwrap_or_default(),
                    "type": u.record_type,
                    "tag": tag_of(&u.record_type).unwrap_or(""),
                    "key": u.key,
                    "paths": u.paths,
                    "count": u.count,
                    "wins": u.wins,
                })
            })
            .collect();
        json!({
            "tag": found.tag,
            "id": found.key,
            "uses": rows,
            "live": found_uses.iter().filter(|u| u.wins).map(|u| u.count).sum::<usize>(),
            "cells": found_uses.iter().filter(|u| u.record_type == "Cell").map(|u| u.count).sum::<usize>(),
        })
    }

    /// The references of a cell (its versions, load order) that place an object, as the
    /// load order resolves them (`refedit.refs_naming`): `(creating plugin, refr_index)`
    /// of each whose winning version places it and is not deleted.
    fn refs_naming(&self, versions: &[(String, Value)], rid: &str) -> Vec<(String, i64)> {
        let want = rid.to_lowercase();
        let mut keys: IndexMap<(String, i64), String> = IndexMap::new();
        for (plugin, rec) in versions {
            let masters = self.session.masters_of(plugin);
            for r in rec.get("references").and_then(Value::as_array).into_iter().flatten() {
                if !r.is_object() {
                    continue;
                }
                let mast = r.get("mast_index").and_then(Value::as_i64).unwrap_or(-1);
                if let Some(o) = origin(plugin, &masters, mast) {
                    let refr = r.get("refr_index").and_then(Value::as_i64).unwrap_or(0);
                    keys.insert((o.to_lowercase(), refr), o.to_string());
                }
            }
        }
        let mut out = Vec::new();
        for ((_, refr), o) in keys {
            if let Some((_, won)) = self.session.winning_reference(versions, &o, refr)
                && won.get("id").and_then(Value::as_str).unwrap_or("").to_lowercase() == want
                && !won.get("deleted").is_some_and(truthy)
            {
                out.push((o, refr));
            }
        }
        out
    }

    /// What Search & Replace would change (`replace_plan`): each live use's field with
    /// `new_id` in place of the record's id, each placed reference made to place it, and
    /// the Use Report the plan was made from. With `cell` (its key), only the references
    /// placed in that cell ("Replace all in the current cell"). Script source changes
    /// where the id stands as a word; a script is compiled again when it is queued.
    pub fn replace_plan(&self, found: &Found, new_id: &str, cell: Option<&str>) -> Result<(Vec<Repoint>, Value), EditError> {
        let Some(target) = self.find(&found.tag, new_id, None) else {
            return err(format!("{new_id} is not a {} of this load order", found.record_type));
        };
        if target.key.to_lowercase() == found.key.to_lowercase() {
            return err("that is the same record");
        }
        let report = self.uses(found);
        let rows = report["uses"].as_array().cloned().unwrap_or_default();
        let only = cell.map(|c| c.trim().to_lowercase());
        let text = |v: &Value| v.as_str().unwrap_or("").to_string();
        let mut plan = Vec::new();
        for u in &rows {
            if u["type"] == "Cell" || only.is_some() || u["wins"] != true || text(&u["tag"]).is_empty() {
                continue;
            }
            let Some(user) = self.find(&text(&u["tag"]), &text(&u["key"]), None) else { continue };
            let record = user.record().clone();
            let mut fields: IndexSet<String> = IndexSet::new();
            for p in u["paths"].as_array().into_iter().flatten() {
                fields.insert(field_of(&record, &text(p)));
            }
            for field in fields {
                let Some(current) = value_at(&record, &field) else { continue };
                if SCRIPT_TEXT.contains(&(user.record_type.as_str(), field.as_str()))
                    && let Some(source) = current.as_str()
                {
                    let (renamed, n) = crate::rename::replace_id(source, &found.key, &target.key);
                    if n > 0 {
                        plan.push(Repoint::Field { user: user.clone(), field, value: json!(renamed) });
                    }
                    continue;
                }
                plan.push(Repoint::Field { user: user.clone(), field, value: swap(current, &found.key, &target.key) });
            }
        }
        let cells: IndexSet<String> = rows.iter().filter(|u| u["type"] == "Cell").map(|u| text(&u["key"])).collect();
        for key in cells {
            if only.as_ref().is_some_and(|o| key.to_lowercase() != *o) {
                continue;
            }
            let holders = self.cell_plugins(&key, None);
            let versions: Vec<(String, Value)> = holders
                .iter()
                .filter_map(|p| self.session.records(p, "CELL").get(&key.to_lowercase()).map(|r| (p.clone(), r.clone())))
                .collect();
            for (o, refr) in self.refs_naming(&versions, &found.key) {
                let mut changes = crate::queue::Fields::default();
                changes.insert("id".into(), json!(target.key));
                plan.push(Repoint::Ref(RefEdit { cell: key.clone(), origin: o, refr_index: refr, changes, plugins: holders.clone() }));
            }
        }
        for new in self.queue.new_refs() {
            if only.as_ref().is_some_and(|o| new.cell.to_lowercase() != *o) {
                continue;
            }
            if new.fields.get(&"id".to_string()).and_then(Value::as_str).unwrap_or("").to_lowercase() == found.key.to_lowercase() {
                let mut moved = new.clone();
                moved.fields.insert("id".into(), json!(target.key));
                plan.push(Repoint::New(moved));
            }
        }
        Ok((plan, report))
    }

    /// Queues a Search & Replace plan (`replace_uses`) as one change - one journal save,
    /// one undo step - and says how many records and placed references it changed. A
    /// script's new source is saved with its bytecode when it compiles
    /// ([`Editor::set_script_text`]).
    pub fn replace_uses(&mut self, plan: &[Repoint], compiler: &dyn Compile) -> Result<usize, EditError> {
        self.batch(|ed| {
            let mut changed: HashSet<(String, String)> = HashSet::new();
            let mut refs = 0;
            for item in plan {
                match item {
                    Repoint::Ref(edit) => {
                        ed.queue.add_ref_edit(edit.clone());
                        refs += 1;
                    }
                    Repoint::New(new) => {
                        ed.queue.add_new_ref(new.clone());
                        refs += 1;
                    }
                    Repoint::Field { user, field, value } => {
                        if user.record_type == "Script"
                            && field == "text"
                            && let Some(source) = value.as_str()
                        {
                            ed.set_script_text(user, source, compiler)?;
                        } else if user.new {
                            let Some(made) = ed.queue.new_record(&user.record_type, &user.key).cloned() else { continue };
                            let mut record = made.record.clone();
                            set_at(&mut record, field, value.clone())?;
                            ed.queue.add_new_record(NewRecord { record, ..made });
                        } else {
                            ed.queue.add_field(&user.record_type, &user.key, Choice::Value { path: field.clone(), value: value.clone() });
                            ed.queue.set_base(&user.record_type, &user.key, user.winner());
                        }
                        changed.insert((user.record_type.clone(), user.key.clone()));
                    }
                }
            }
            Ok(changed.len() + refs)
        })
    }
}

/// `editReplace`'s answer for a plan and what it changed: `{changed, refs, scripts}` -
/// the references among the changes, and the scripts and dialogue results changed.
pub fn replace_summary(plan: &[Repoint], changed: usize) -> Value {
    let refs = plan.iter().filter(|p| !matches!(p, Repoint::Field { .. })).count();
    let scripts = plan
        .iter()
        .filter(|p| match p {
            Repoint::Field { user, field, .. } => (field == "text" || field == "script_text") && (user.record_type == "Script" || user.record_type == "DialogueInfo"),
            _ => false,
        })
        .count();
    json!({"changed": changed, "refs": refs, "scripts": scripts})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_words_and_paths() {
        assert!(contains_ci(b"xx Gold_001 yy", b"gold_001"));
        assert!(!contains_ci(b"gold_00", b"gold_001"));
        let f = Finder::new("gold_001", true);
        assert_eq!(f.count("player->AddItem gold_001 5\nset x to gold_0012"), 1);
        assert_eq!(f.count("\"Gold_001\" gold_001"), 2);
        assert!(indexable("gold_001") && !indexable("gold 001") && !indexable("a-b") && !indexable(" "));
        let rec = json!({"a": {"b": [1, {"c": 2}]}, "d": 3});
        assert_eq!(field_of(&rec, "a.b.1.c"), "a.b");
        assert_eq!(field_of(&rec, "a.x.y"), "a.x");
        assert_eq!(field_of(&rec, "d"), "d");
        assert_eq!(swap(&json!([["GOLD_001", 1], "gold_0012"]), "gold_001", "x"), json!([["x", 1], "gold_0012"]));
        assert_eq!(dumps(&["a".into(), "b.0".into()]), "[\"a\", \"b.0\"]");
    }

    #[test]
    fn a_response_result_is_script_source_too() {
        let recs = vec![json!({"type": "DialogueInfo", "id": "1", "script_text": "player->additem gold_001 1"})];
        let u = uses_in(0, &recs, "MiscItem", "gold_001");
        assert_eq!((u[0].paths.clone(), u[0].count), (vec!["script_text".to_string()], 1));
        let rows = index_rows(&recs);
        assert!(rows.iter().any(|r| r.0 == "gold_001" && r.3 == "[\"script_text\"]" && r.4 == 1));
    }
}
