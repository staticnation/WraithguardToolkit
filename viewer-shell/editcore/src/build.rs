//! Building a patch: the queue's decisions turned into one new plugin that loads last
//! (`wraithguard/patch/service.py`'s `build_record_patch`, with what it uses of
//! `records.py`, `merge.py` and `refedit.py`, and the GUI's `prepare_patch`, `run_patch`
//! and `finish_patch`). Port step 4.1, slice 6. Cases: `tests/fixtures/build_cases.json`,
//! run here and by `tests/test_build_cases.py`.
//!
//! Everything is additive: no source plugin is written; the output is one new file, and
//! deleting it restores the load order as it was. A patch can be built over several
//! sessions: with `append`, an earlier build of the same file rides along into the new
//! one (`carry_forward`), this session's decisions taking precedence.
//!
//! The order of the master list is meaning: `mast_index` values are places in it, so
//! every record taken from a plugin has its references renumbered for the patch's list
//! (`index_map`, `remap_references`), and a merge renumbers each field it takes against
//! its own plugin's list.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::dialogue::{responses_by_topic, topic_order, Placed, Response};
use crate::edit::{set_at, value_at, Editor};
use crate::key::record_key;
use crate::pyval::{repr, str_of, str_repr};
use crate::queue::{Choice, NewRecord, NewRef, RefEdit, Selection};
use crate::session::truthy;

/// What a patch is called when the caller does not say.
pub const DEFAULT_NAME: &str = "Wraithguard Patch.esp";
/// What the header says it is.
pub const DESCRIPTION: &str = "Record patch built by Wraithguard Toolkit. Load last.";
/// Fields that say which record this is (a merge may not take or type them).
pub const IDENTITY: [&str; 4] = ["type", "id", "grid", "data.grid"];
/// A reference's fields (what a change or a new reference may set), `id` aside.
pub const REF_FIELDS: [&str; 19] = [
    "translation", "rotation", "scale", "deleted", "moved_cell", "owner", "owner_global", "owner_faction", "owner_faction_rank", "lock_level", "key", "trap",
    "soul", "charge_left", "health_left", "object_count", "blocked", "temporary", "destination",
];
const LUAL: &str = "ScriptConfigList";

/// A build refused: the reason to show.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildError(pub String);

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BuildError {}

type Res<T> = Result<T, BuildError>;

fn fail<T>(m: impl Into<String>) -> Res<T> {
    Err(BuildError(m.into()))
}

/// Each source plugin's records (header included, file order), by load-order name.
pub type Sources = IndexMap<String, Vec<Value>>;

/// A record built from several plugins field by field.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    pub record_type: String,
    pub key: String,
    /// The plugin supplying every field not chosen.
    pub base_plugin: String,
    pub choices: Vec<Choice>,
}

impl Merge {
    /// Every plugin it reads from, base included (a typed value belongs to none).
    pub fn plugins(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self.choices.iter().filter_map(|c| if let Choice::Plugin { plugin, .. } = c { Some(plugin.clone()) } else { None }).collect();
        out.insert(self.base_plugin.clone());
        out
    }
}

/// What a build produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatchResult {
    /// The plugin written (`None` for a dry run).
    pub output: Option<PathBuf>,
    pub records: usize,
    /// Records carried from an earlier build of this patch.
    pub carried: usize,
    pub masters: Vec<String>,
    /// Records whose references were renumbered.
    pub remapped: usize,
    /// The build's report.
    pub lines: Vec<String>,
}

fn rtype(r: &Value) -> &str {
    r.get("type").and_then(Value::as_str).unwrap_or("")
}

// -- records (records.py) ----------------------------------------------------------------

/// Every key a record answers to: its key, and a cell's name.
pub fn keys_of(record: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    out.insert(record_key(record));
    if rtype(record) == "Cell"
        && let Some(n) = record.get("name").filter(|n| truthy(n))
    {
        out.insert(str_of(n));
    }
    out.remove("");
    out
}

/// The one record of a type a key names in a plugin; refused when the key names more
/// than one (a cell name several exterior cells share).
pub fn find_record<'a>(records: &'a [Value], record_type: &str, key: &str, plugin: &str) -> Res<Option<&'a Value>> {
    let hits: Vec<&Value> = records.iter().filter(|r| rtype(r) == record_type && keys_of(r).contains(key)).collect();
    if hits.len() > 1 {
        let wher: Vec<String> = hits.iter().map(|r| record_key(r)).collect();
        return fail(format!("{} names {} {record_type} records in {plugin} ({}). Choose the one meant by its grid.", str_repr(key), hits.len(), wher.join(", ")));
    }
    Ok(hits.first().copied())
}

/// A plugin's declared masters, in order.
pub fn master_names(records: &[Value]) -> Vec<String> {
    for r in records {
        if rtype(r) != "Header" {
            continue;
        }
        let Some(m) = r.get("masters").and_then(Value::as_array) else { return Vec::new() };
        return m.iter().filter_map(|e| e.as_array().and_then(|e| e.first()).map(str_of)).collect();
    }
    Vec::new()
}

/// How one plugin's `mast_index` values read in the patch: 0 (the plugin itself) and
/// each of its masters to their places in the patch's list.
pub fn index_map(source: &str, source_masters: &[String], patch_masters: &[String]) -> Res<HashMap<i64, i64>> {
    let lowered: HashMap<String, i64> = patch_masters.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i as i64 + 1)).collect();
    let position = |name: &str| -> Res<i64> {
        match lowered.get(&name.to_lowercase()) {
            Some(p) => Ok(*p),
            None => fail(format!(
                "{name} is referenced by records taken from {source} but is not in the patch's master list. Every reference to it would point somewhere else, so the record is refused rather than written wrong."
            )),
        }
    };
    let mut mapping = HashMap::new();
    mapping.insert(0, position(source)?);
    for (i, name) in source_masters.iter().enumerate() {
        mapping.insert(i as i64 + 1, position(name)?);
    }
    Ok(mapping)
}

/// A bare list of references renumbered.
pub fn remap_reference_list(references: &[Value], mapping: &HashMap<i64, i64>) -> Res<Vec<Value>> {
    let mut out = references.to_vec();
    for r in out.iter_mut() {
        let Some(m) = r.as_object_mut() else { continue };
        let Some(old) = m.get("mast_index").filter(|v| v.is_i64() || v.is_u64()).and_then(Value::as_i64) else { continue };
        let Some(new) = mapping.get(&old) else {
            return fail(format!(
                "a reference uses mast_index {old}, which its plugin's master list does not reach. The plugin is inconsistent; patching it would guess at what that index meant."
            ));
        };
        m.insert("mast_index".into(), json!(new));
    }
    Ok(out)
}

/// Whether a LUAL record names placed objects (per-reference entries, or Lua data).
fn lual_names_objects(record: &Value) -> bool {
    record.get("scripts").and_then(Value::as_array).into_iter().flatten().filter(|s| s.is_object()).any(|s| {
        s.get("instances").is_some_and(truthy)
            || s.get("init_data").is_some_and(truthy)
            || s.get("records").and_then(Value::as_array).into_iter().flatten().any(|r| r.is_object() && r.get("data").is_some_and(truthy))
    })
}

/// An empty Lua data field (`""`) in the form the crate's serde reads.
fn blank_lua_data(v: &mut Value) {
    let empty = serde_json::to_value(tes3::esp::PerInstanceConfig::default()).ok().and_then(|d| d.get("data").cloned());
    let Some(empty) = empty else { return };
    let fix = |o: &mut Value, key: &str| {
        if o.get(key).and_then(Value::as_str) == Some("") {
            o[key] = empty.clone();
        }
    };
    let Some(scripts) = v.get_mut("scripts").and_then(Value::as_array_mut) else { return };
    for script in scripts {
        fix(script, "init_data");
        for list in ["records", "instances"] {
            if let Some(items) = script.get_mut(list).and_then(Value::as_array_mut) {
                for item in items {
                    fix(item, "data");
                }
            }
        }
    }
}

/// A LUAL record for a new master list: its per-reference entries (LUAI) and the
/// references serialized in its Lua data (merge_to_master's `remap_refnums`) renumbered
/// (was `native/src/lua`'s `lual_remap`, which now calls this).
pub fn lual_remap(record: &Value, mapping: &HashMap<i32, i32>) -> Result<Value, String> {
    use tes3::esp::TES3Object;
    let mut value = record.clone();
    blank_lua_data(&mut value);
    let mut obj: TES3Object = serde_json::from_value(value).map_err(|e| format!("not a record: {e}"))?;
    let TES3Object::ScriptConfigList(list) = &mut obj else {
        return Err("not a LUAL record".into());
    };
    let mut missing: Option<i32> = None;
    for script in &mut list.scripts {
        for instance in &mut script.instances {
            match mapping.get(&instance.mast_idx) {
                Some(&m) => instance.mast_idx = m,
                None => missing = Some(instance.mast_idx),
            }
        }
        let datas = std::iter::once(&mut script.init_data).chain(script.records.iter_mut().map(|r| &mut r.data)).chain(script.instances.iter_mut().map(|i| &mut i.data));
        for data in datas {
            merge_to_master::remap_refnums(data, &mut |file: &mut i32, _index: &mut u32| match mapping.get(file) {
                Some(&m) => *file = m,
                None => missing = Some(*file),
            })
            .map_err(|e| format!("{}: its Lua data does not read ({e})", script.path))?;
        }
    }
    if let Some(m) = missing {
        return Err(format!("a script entry names content file {m}, which the plugin's master list does not reach"));
    }
    serde_json::to_value(&obj).map_err(|e| e.to_string())
}

/// A record renumbered for its new home (a copy).
pub fn remap_references(record: &Value, mapping: &HashMap<i64, i64>) -> Res<Value> {
    if rtype(record) == LUAL {
        if !lual_names_objects(record) {
            return Ok(record.clone());
        }
        let m: HashMap<i32, i32> = mapping.iter().map(|(k, v)| (*k as i32, *v as i32)).collect();
        return lual_remap(record, &m).map_err(|e| BuildError(format!("the Lua script list cannot be carried: {e}")));
    }
    let mut copied = record.clone();
    if let Some(refs) = copied.get("references").and_then(Value::as_array).cloned() {
        copied["references"] = Value::Array(remap_reference_list(&refs, mapping)?);
    }
    Ok(copied)
}

/// Whether a record's meaning depends on the master list.
pub fn needs_remapping(record: &Value) -> bool {
    if rtype(record) == LUAL {
        return lual_names_objects(record);
    }
    rtype(record) == "Cell" && record.get("references").is_some_and(truthy)
}

/// The topic a dialogue response belongs to: the topic before it in its plugin.
pub fn owning_dialogue<'a>(records: &'a [Value], record_type: &str, key: &str) -> Option<&'a Value> {
    if record_type != "DialogueInfo" {
        return None;
    }
    let mut topic = None;
    for r in records {
        match rtype(r) {
            "Dialogue" => topic = Some(r),
            "DialogueInfo" if record_key(r) == key => return topic,
            _ => {}
        }
    }
    None
}

/// What kind of topic a `Dialogue` record is.
pub fn topic_kind(topic: Option<&Value>) -> String {
    let Some(t) = topic else { return String::new() };
    let flat = match t.get("dialogue_type") {
        Some(v) if !v.is_null() => Some(v),
        _ => t.get("data").filter(|d| d.is_object()).and_then(|d| d.get("dialogue_type")),
    };
    match flat {
        Some(v) if truthy(v) => str_of(v),
        _ => String::new(),
    }
}

/// Which plugins define each dialogue response, in the order the sources are given.
pub fn defining_plugins(sources: &Sources) -> HashMap<String, Vec<String>> {
    let mut holders: HashMap<String, Vec<String>> = HashMap::new();
    for (plugin, records) in sources {
        for r in records {
            if rtype(r) == "DialogueInfo" {
                holders.entry(record_key(r)).or_default().push(plugin.clone());
            }
        }
    }
    holders
}

/// The neighbouring responses that hold a carried line in place and are not carried
/// themselves: `(response, anchor, plugin)`.
pub fn position_anchors(carried: &[Value], sources: &Sources) -> Vec<(String, String, String)> {
    let carried_ids: HashSet<String> = carried.iter().filter(|r| rtype(r) == "DialogueInfo").map(record_key).collect();
    let holders = defining_plugins(sources);
    let mut found = Vec::new();
    for r in carried.iter().filter(|r| rtype(r) == "DialogueInfo") {
        for field in ["prev_id", "next_id"] {
            let n = text(r.get(field));
            if n.is_empty() || carried_ids.contains(&n) {
                continue;
            }
            for p in holders.get(&n).into_iter().flatten() {
                found.push((record_key(r), n.clone(), p.clone()));
            }
        }
    }
    found
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => str_of(v),
        _ => String::new(),
    }
}

fn first12(s: &str) -> String {
    s.chars().take(12).collect()
}

/// The responses whose place in their topic the patch cannot promise: one note each.
pub fn dialogue_position_risk(carried: &[Value], sources: &Sources) -> Vec<String> {
    let carried_ids: HashSet<String> = carried.iter().filter(|r| rtype(r) == "DialogueInfo").map(record_key).collect();
    let holders = if sources.is_empty() { HashMap::new() } else { defining_plugins(sources) };
    let mut kinds: HashMap<String, String> = HashMap::new();
    let mut current = String::new();
    for r in carried {
        match rtype(r) {
            "Dialogue" => current = topic_kind(Some(r)),
            "DialogueInfo" if !current.is_empty() => {
                kinds.insert(record_key(r), current.clone());
            }
            _ => {}
        }
    }
    let mut notes = Vec::new();
    for r in carried.iter().filter(|r| rtype(r) == "DialogueInfo") {
        let key = record_key(r);
        let loose: Vec<String> = [("previous", "prev_id"), ("next", "next_id")]
            .iter()
            .filter(|(_, f)| r.get(*f).is_some_and(truthy) && !carried_ids.contains(&str_of(&r[*f])))
            .map(|(side, f)| {
                let n = str_of(&r[*f]);
                let wher = holders.get(&n).cloned().unwrap_or_default();
                if wher.len() > 1 {
                    format!(
                        "{side} response {}... is defined by {} files ({}), so which of them supplies it decides where this line sits",
                        first12(&n),
                        wher.len(),
                        wher.join(", ")
                    )
                } else if let Some(w) = wher.first() {
                    format!("{side} response {}... is held in place by {w}", first12(&n))
                } else {
                    format!("{side} response {}... comes from another file", first12(&n))
                }
            })
            .collect();
        if loose.is_empty() {
            continue;
        }
        let tail = if kinds.get(&key).map(String::as_str) == Some("Greeting") {
            "and this is a greeting, where position is the whole of it: greetings are numbered buckets read top down and matched on filters alone, so a line that moves down is a line that stops being said"
        } else {
            "so where this line falls in its topic depends on those staying put"
        };
        notes.push(format!("{key}: its {}, {tail}", loose.join(" and its ")));
    }
    notes
}

/// The chosen records, in the order chosen, references renumbered; a dialogue response
/// preceded by its topic (once).
pub fn collect(selections: &[Selection], sources: &Sources, patch_masters: &[String]) -> Res<Vec<Value>> {
    let mut maps: HashMap<String, HashMap<i64, i64>> = HashMap::new();
    let mut seen_topics: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for sel in selections {
        let Some(records) = sources.get(&sel.plugin) else {
            return fail(format!("no records were read for {}", sel.plugin));
        };
        let Some(found) = find_record(records, &sel.record_type, &sel.key, &sel.plugin)? else {
            return fail(format!("{} has no {} record {}. It may have been updated since the scan.", sel.plugin, sel.record_type, str_repr(&sel.key)));
        };
        if let Some(topic) = owning_dialogue(records, &sel.record_type, &sel.key)
            && seen_topics.insert(record_key(topic))
        {
            out.push(topic.clone());
        }
        if !needs_remapping(found) {
            out.push(found.clone());
            continue;
        }
        if !maps.contains_key(&sel.plugin) {
            maps.insert(sel.plugin.clone(), index_map(&sel.plugin, &master_names(records), patch_masters)?);
        }
        out.push(remap_references(found, &maps[&sel.plugin])?);
    }
    Ok(out)
}

/// An earlier build of this same patch re-emitted into a new one, its own index 0
/// unchanged (the file keeps its identity), the rest renumbered; records this session
/// decides again (`skip`) left out.
pub fn carry_forward(records: &[Value], patch_masters: &[String], skip: &HashSet<(String, String)>) -> Res<Vec<Value>> {
    let old_masters = master_names(records);
    let lowered: HashMap<String, i64> = patch_masters.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i as i64 + 1)).collect();
    let mut mapping: Option<HashMap<i64, i64>> = None;
    let mut out = Vec::new();
    for r in records {
        if rtype(r) == "Header" {
            continue;
        }
        if keys_of(r).iter().any(|k| skip.contains(&(rtype(r).to_string(), k.clone()))) {
            continue;
        }
        if !needs_remapping(r) {
            out.push(r.clone());
            continue;
        }
        if mapping.is_none() {
            let mut built = HashMap::new();
            built.insert(0, 0);
            for (i, name) in old_masters.iter().enumerate() {
                let Some(found) = lowered.get(&name.to_lowercase()) else {
                    return fail(format!(
                        "the existing patch references {name}, which the new build does not declare. Rescan with it present, or replace the patch instead of appending to it."
                    ));
                };
                built.insert(i as i64 + 1, *found);
            }
            mapping = Some(built);
        }
        out.push(remap_references(r, mapping.as_ref().unwrap())?);
    }
    Ok(out)
}

/// What a patch of these records must declare: every plugin a record is taken from and
/// its masters (and `extra`), in load order, the rest after by name.
pub fn required_masters(selections: &[Selection], sources: &Sources, load_order: &[String], extra: &[String]) -> Vec<String> {
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for sel in selections {
        needed.insert(sel.plugin.clone());
        needed.extend(master_names(sources.get(&sel.plugin).map(Vec::as_slice).unwrap_or(&[])));
    }
    needed.extend(extra.iter().cloned());
    let lowered: IndexMap<String, String> = needed.into_iter().map(|n| (n.to_lowercase(), n)).collect();
    let mut ordered: Vec<String> = load_order.iter().filter(|n| lowered.contains_key(&n.to_lowercase())).cloned().collect();
    let seen: HashSet<String> = ordered.iter().map(|n| n.to_lowercase()).collect();
    let mut rest: Vec<String> = lowered.iter().filter(|(k, _)| !seen.contains(*k)).map(|(_, n)| n.clone()).collect();
    rest.sort();
    ordered.extend(rest);
    ordered
}

// -- merges (merge.py) -------------------------------------------------------------------

fn find_in<'a>(plugin: &str, record_type: &str, key: &str, sources: &'a Sources) -> Res<&'a Value> {
    let Some(records) = sources.get(plugin) else {
        return fail(format!("no records were read for {plugin}"));
    };
    match find_record(records, record_type, key, plugin)? {
        Some(r) => Ok(r),
        None => fail(format!("{plugin} has no {record_type} record {}", str_repr(key))),
    }
}

/// One record built from a base and the fields chosen from other plugins (or typed),
/// each field taken from a plugin renumbered against that plugin's own master list.
pub fn merge_record(m: &Merge, sources: &Sources, patch_masters: &[String]) -> Res<Value> {
    let base = find_in(&m.base_plugin, &m.record_type, &m.key, sources)?;
    let mut maps: HashMap<String, HashMap<i64, i64>> = HashMap::new();
    let mut mapping_for = |plugin: &str| -> Res<HashMap<i64, i64>> {
        if let Some(found) = maps.get(plugin) {
            return Ok(found.clone());
        }
        let built = index_map(plugin, &master_names(sources.get(plugin).map(Vec::as_slice).unwrap_or(&[])), patch_masters)?;
        maps.insert(plugin.to_string(), built.clone());
        Ok(built)
    };
    let mut merged = base.clone();
    if let Some(refs) = merged.get("references").and_then(Value::as_array).cloned() {
        merged["references"] = Value::Array(remap_reference_list(&refs, &mapping_for(&m.base_plugin)?)?);
    }
    let lual = rtype(&merged) == LUAL;
    if lual {
        merged = remap_references(&merged, &mapping_for(&m.base_plugin)?)?;
    }
    let set = |merged: &mut Value, path: &str, value: Value| set_at(merged, path, value).map_err(|e| BuildError(e.0));
    for choice in &m.choices {
        match choice {
            Choice::Value { path, value } => {
                if IDENTITY.contains(&path.as_str()) {
                    return fail(format!("{path} says which record this is. Giving it a typed value would not merge this record, it would make a different one."));
                }
                set(&mut merged, path, value.clone())?;
            }
            Choice::Plugin { path, plugin } => {
                if *plugin == m.base_plugin {
                    continue;
                }
                if IDENTITY.contains(&path.as_str()) {
                    return fail(format!("{path} says which record this is. Taking it from {plugin} would not merge this record, it would make a different one."));
                }
                let source = find_in(plugin, &m.record_type, &m.key, sources)?;
                let Some(value) = value_at(source, path) else {
                    return fail(format!(
                        "{plugin} has no {path} in this record, so there is nothing to take. Choose a plugin that has the field, or leave it as the base has it."
                    ));
                };
                let mut value = value.clone();
                if path == "references"
                    && let Some(list) = value.as_array().cloned()
                {
                    value = Value::Array(remap_reference_list(&list, &mapping_for(plugin)?)?);
                } else if lual
                    && path == "scripts"
                    && value.is_array()
                {
                    value = remap_references(&json!({"type": LUAL, "scripts": value}), &mapping_for(plugin)?)?["scripts"].clone();
                }
                set(&mut merged, path, value)?;
            }
        }
    }
    Ok(merged)
}

/// A merge in words, a line per field changed.
pub fn describe(choices: &[Choice], base_plugin: &str) -> Vec<String> {
    let lines: Vec<String> = choices
        .iter()
        .filter_map(|c| match c {
            Choice::Value { path, value } => Some(format!("{path}: set to {}", repr(value))),
            Choice::Plugin { path, plugin } if plugin != base_plugin => Some(format!("{path}: from {plugin}")),
            _ => None,
        })
        .collect();
    if lines.is_empty() { vec![format!("nothing taken from elsewhere; this is {base_plugin}'s record whole")] } else { lines }
}

// -- references (refedit.py) -------------------------------------------------------------

/// Every plugin's CELL record for one cell, in load order.
pub fn cell_versions(cell: &str, sources: &Sources, load_order: &[String]) -> Vec<(String, Value)> {
    let want = cell.to_lowercase();
    load_order
        .iter()
        .filter_map(|p| sources.get(p).and_then(|recs| recs.iter().find(|r| rtype(r) == "Cell" && record_key(r).to_lowercase() == want)).map(|r| (p.clone(), r.clone())))
        .collect()
}

/// The reference `(origin, refr_index)` as the load order resolves it over a cell's
/// versions (`ordercore::refs`): `(plugin, reference)`.
pub fn winning_reference(versions: &[(String, Value)], masters_of: &HashMap<String, Vec<String>>, origin_name: &str, refr_index: i64) -> Option<(String, Value)> {
    let mut mine: Vec<(String, Value)> = Vec::new();
    for (plugin, rec) in versions {
        let masters = masters_of.get(plugin).cloned().unwrap_or_default();
        for r in rec.get("references").and_then(Value::as_array).into_iter().flatten() {
            if !r.is_object() || r.get("refr_index").and_then(Value::as_i64) != Some(refr_index) {
                continue;
            }
            let mast = r.get("mast_index").and_then(Value::as_i64).unwrap_or(-1);
            if ordercore::refs::origin(plugin, &masters, mast).is_some_and(|o| o.to_lowercase() == origin_name.to_lowercase()) {
                mine.push((plugin.clone(), r.clone()));
            }
        }
    }
    let versions: Vec<ordercore::refs::Version> = mine.iter().enumerate().map(|(i, (_, r))| ordercore::refs::Version { plugin: i, deleted: r.get("deleted").is_some_and(truthy) }).collect();
    let w = ordercore::refs::winner(&versions)?;
    Some(mine.swap_remove(w.at))
}

/// One above every `(0, n)` reference the patch's cells carry; 1 for none.
pub fn next_new_index(records: &[Value]) -> i64 {
    let mut top = 0;
    for rec in records.iter().filter(|r| rtype(r) == "Cell") {
        for r in rec.get("references").and_then(Value::as_array).into_iter().flatten() {
            if r.is_object() && r.get("mast_index").and_then(Value::as_i64) == Some(0) {
                top = top.max(r.get("refr_index").and_then(Value::as_i64).unwrap_or(0));
            }
        }
    }
    top + 1
}

/// The CELL record carrying one cell's changed references, and only those.
pub fn cell_patch_record(edits: &[&RefEdit], sources: &Sources, load_order: &[String], patch_masters: &[String], own: &str) -> Res<Value> {
    let Some(first) = edits.first() else { return fail("no reference changes to write") };
    let cells: HashSet<String> = edits.iter().map(|e| e.cell.to_lowercase()).collect();
    if cells.len() != 1 {
        return fail("one CELL record carries one cell's references");
    }
    let versions = cell_versions(&first.cell, sources, load_order);
    let Some((_, last)) = versions.last() else {
        return fail(format!("no plugin of the load order has the cell {}", first.cell));
    };
    let masters_of: HashMap<String, Vec<String>> = versions.iter().map(|(p, _)| (p.clone(), master_names(sources.get(p).map(Vec::as_slice).unwrap_or(&[])))).collect();
    let position: HashMap<String, i64> = patch_masters.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i as i64 + 1)).collect();
    let mut record = last.clone();
    if let Some(m) = record.as_object_mut() {
        m.shift_remove("references");
    }
    let mut refs = Vec::new();
    let mut seen: HashSet<(String, i64)> = HashSet::new();
    for e in edits {
        if !seen.insert((e.origin.to_lowercase(), e.refr_index)) {
            return fail(format!("two sets of changes for reference {}:{}", e.origin, e.refr_index));
        }
        let mut bad: Vec<&str> = e.changes.keys().map(String::as_str).filter(|k| !REF_FIELDS.contains(k) && *k != "id").collect();
        if !bad.is_empty() {
            bad.sort();
            return fail(format!("{} cannot be changed on a reference", bad.join(", ")));
        }
        let Some((_, won)) = winning_reference(&versions, &masters_of, &e.origin, e.refr_index) else {
            return fail(format!("the cell {} has no reference {}:{}", e.cell, e.origin, e.refr_index));
        };
        let at = if !own.is_empty() && e.origin.to_lowercase() == own.to_lowercase() { Some(0) } else { position.get(&e.origin.to_lowercase()).copied() };
        let Some(at) = at else {
            return fail(format!("{} created this reference but is not among the patch's masters, so the reference cannot be named", e.origin));
        };
        let mut r = won;
        for (k, v) in e.changes.iter() {
            r[k.as_str()] = v.clone();
        }
        r["mast_index"] = json!(at);
        r["refr_index"] = json!(e.refr_index);
        refs.push(r);
    }
    record["references"] = Value::Array(refs);
    Ok(record)
}

/// The references the patch adds put into its records, each in the patch's record for
/// its cell (or a new one, the winning version's own fields and no other reference), as
/// `(0, n)` numbered on from the highest the patch carries: uid -> index given.
pub fn place_new_refs(records: &mut Vec<Value>, new_refs: &[NewRef], sources: &Sources, load_order: &[String]) -> Res<IndexMap<String, i64>> {
    let mut given = IndexMap::new();
    for (n, new) in (next_new_index(records)..).zip(new_refs) {
        let mut bad: Vec<&str> = new.fields.keys().map(String::as_str).filter(|k| !REF_FIELDS.contains(k) && *k != "id").collect();
        if !bad.is_empty() {
            bad.sort();
            return fail(format!("{} is not a field of a reference", bad.join(", ")));
        }
        if !new.fields.get(&"id".to_string()).is_some_and(truthy) || new.fields.get(&"translation".to_string()).is_none() {
            return fail(format!("the new reference {} has no object or no position", new.uid));
        }
        let want = new.cell.to_lowercase();
        let at = records.iter().position(|r| rtype(r) == "Cell" && keys_of(r).iter().any(|k| k.to_lowercase() == want));
        let at = match at {
            Some(i) => i,
            None => {
                let versions = cell_versions(&new.cell, sources, load_order);
                let Some((_, last)) = versions.last() else {
                    return fail(format!("no plugin of the load order has the cell {}", new.cell));
                };
                let mut cell = last.clone();
                if let Some(m) = cell.as_object_mut() {
                    m.shift_remove("references");
                }
                cell["references"] = json!([]);
                records.push(cell);
                records.len() - 1
            }
        };
        let mut r = Map::new();
        r.insert("mast_index".into(), json!(0));
        r.insert("refr_index".into(), json!(n));
        r.insert("id".into(), new.fields.get(&"id".to_string()).cloned().unwrap_or(Value::Null));
        r.insert("temporary".into(), json!(true));
        r.insert("rotation".into(), json!([0.0, 0.0, 0.0]));
        for (k, v) in new.fields.iter() {
            if !v.is_null() {
                r.insert(k.clone(), v.clone());
            }
        }
        r.insert("mast_index".into(), json!(0));
        r.insert("refr_index".into(), json!(n));
        let cell = &mut records[at];
        if !cell.get("references").is_some_and(Value::is_array) {
            cell["references"] = json!([]);
        }
        cell["references"].as_array_mut().unwrap().push(Value::Object(r));
        given.insert(new.uid.clone(), n);
    }
    Ok(given)
}

// -- dialogue moves (dialogue.py) --------------------------------------------------------

/// Which responses a change moves, and how far: `(id, was, now)` by the new place.
pub fn moved(before: &[Placed], after: &[Placed]) -> Vec<(String, usize, usize)> {
    let was: HashMap<&str, usize> = before.iter().enumerate().map(|(i, p)| (p.key.as_str(), i + 1)).collect();
    let mut now: Vec<(&str, usize)> = after.iter().enumerate().map(|(i, p)| (p.key.as_str(), i + 1)).collect();
    // Python's dict: a key placed twice keeps its last place.
    let mut last: IndexMap<&str, usize> = IndexMap::new();
    for (k, i) in now.drain(..) {
        last.insert(k, i);
    }
    let mut v: Vec<(&str, usize)> = last.into_iter().collect();
    v.sort_by_key(|(_, i)| *i);
    v.into_iter().filter(|(k, i)| was.get(k).is_some_and(|w| w != i)).map(|(k, i)| (k.to_string(), was[k], i)).collect()
}

/// The responses a patch moves, and to where: each topic it touches resolved with and
/// without it (`(topic, id, was, now)`, by topic in load order).
pub fn shifts(sources: &Sources, load_order: &[String], carried: &[Value]) -> Vec<(String, String, usize, usize)> {
    let mut before: IndexMap<String, Vec<Response>> = IndexMap::new();
    for plugin in load_order {
        let Some(records) = sources.get(plugin) else { continue };
        for (topic, found) in responses_by_topic(records, plugin) {
            before.entry(topic).or_default().extend(found);
        }
    }
    let mut out = Vec::new();
    for (topic, added) in responses_by_topic(carried, "") {
        let Some(was) = before.get(&topic).filter(|w| !w.is_empty()) else { continue };
        let mut all = was.clone();
        all.extend(added);
        for (key, old, new) in moved(&topic_order(was), &topic_order(&all)) {
            out.push((topic.clone(), key, old, new));
        }
    }
    out
}

// -- the build (service.py) --------------------------------------------------------------

/// The header of a patch: its masters with their sizes, and how many records follow.
pub fn build_header(masters: &[(String, u64)], num_objects: usize, description: &str) -> Res<Value> {
    if masters.is_empty() {
        return fail("a merged landscape plugin must declare its masters. Without them the engine cannot resolve the terrain this plugin overrides, and the result loads but is wrong.");
    }
    let mut entries = Vec::new();
    for (name, size) in masters {
        if name.is_empty() || *size == 0 {
            return fail(format!("master {} has no usable size ({size})", str_repr(name)));
        }
        entries.push(json!([name, size]));
    }
    Ok(json!({
        "type": "Header", "flags": "", "version": 1.3, "file_type": "Esp", "author": "Wraithguard Toolkit",
        "description": description, "num_objects": num_objects, "masters": entries,
    }))
}

/// What a patch must declare (`_masters_for`): the plugins records are taken from
/// (whole, merged, a new record's source), the plugins that created the references it
/// changes and define what it places, and an earlier build's masters - never itself.
fn masters_for(selections: &[Selection], plan: &Plan, sources: &Sources, load_order: &[String], own: &str) -> Vec<String> {
    let Plan { merges, carried, ref_edits, new_refs, .. } = plan;
    let mut stand: Vec<Selection> = selections.to_vec();
    for m in merges {
        stand.extend(m.plugins().into_iter().map(|p| Selection { plugin: p, record_type: m.record_type.clone(), key: m.key.clone() }));
    }
    stand.extend(ref_edits.iter().map(|e| Selection { plugin: e.origin.clone(), record_type: "Cell".into(), key: e.cell.clone() }));
    stand.extend(new_refs.iter().filter(|n| !n.base_plugin.is_empty()).map(|n| Selection { plugin: n.base_plugin.clone(), record_type: "Cell".into(), key: n.cell.clone() }));
    if !own.is_empty() {
        stand.retain(|s| s.plugin.to_lowercase() != own.to_lowercase());
    }
    let found = required_masters(&stand, sources, load_order, &master_names(carried));
    found.into_iter().filter(|n| own.is_empty() || n.to_lowercase() != own.to_lowercase()).collect()
}

/// A topic of the records by id (any case).
fn topic_named<'a>(records: &'a [Value], topic: &str) -> Option<&'a Value> {
    if topic.is_empty() {
        return None;
    }
    let want = topic.to_lowercase();
    records.iter().find(|r| rtype(r) == "Dialogue" && text(r.get("id")).to_lowercase() == want)
}

/// A response put after its topic's block in the patch, the topic first when absent.
fn place_in(records: &mut Vec<Value>, record: Value, topic: &Value) {
    let want = record_key(topic).to_lowercase();
    match records.iter().position(|r| rtype(r) == "Dialogue" && record_key(r).to_lowercase() == want) {
        None => {
            records.push(topic.clone());
            records.push(record);
        }
        Some(at) => {
            let mut end = at + 1;
            while end < records.len() && rtype(&records[end]) == "DialogueInfo" {
                end += 1;
            }
            records.insert(end, record);
        }
    }
}

/// A built record added to the patch; a response inside its own topic.
fn place(records: &mut Vec<Value>, record: Value, source: &[Value]) {
    match owning_dialogue(source, rtype(&record), &record_key(&record)).cloned() {
        None => records.push(record),
        Some(topic) => place_in(records, record, &topic),
    }
}

/// Everything a build is made of.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub selections: Vec<Selection>,
    pub merges: Vec<Merge>,
    pub carried: Vec<Value>,
    pub ref_edits: Vec<RefEdit>,
    pub new_refs: Vec<NewRef>,
    pub new_records: Vec<NewRecord>,
}

/// The patch's records (header first) and what was made of them; `say` hears each line
/// of the report (`build_record_patch`, less the writing: [`write_patch`]).
pub fn build_record_patch(plan: &Plan, sources: &Sources, load_order: &[String], sizes: &HashMap<String, u64>, output_name: &str, say: &mut dyn FnMut(&str)) -> Res<(Vec<Value>, PatchResult)> {
    let mut lines: Vec<String> = Vec::new();
    let mut say = |t: String, lines: &mut Vec<String>| {
        say(&t);
        lines.push(t);
    };
    let Plan { selections, merges, carried, ref_edits, new_refs, new_records } = plan;
    if selections.is_empty() && merges.is_empty() && carried.is_empty() && ref_edits.is_empty() && new_refs.is_empty() && new_records.is_empty() {
        return fail("nothing was selected, so there is no patch to build");
    }
    let whole: BTreeSet<(String, String)> = selections.iter().map(|s| (s.record_type.clone(), s.key.clone())).collect();
    let merged: BTreeSet<(String, String)> = merges.iter().map(|m| (m.record_type.clone(), m.key.clone())).collect();
    let clashes: Vec<String> = whole.intersection(&merged).map(|(k, key)| format!("{k} {}", str_repr(key))).collect();
    if !clashes.is_empty() {
        return fail(format!("{} is both taken whole and merged. Carrying it twice would leave the patch's own last-wins to decide which you get.", clashes.join(", ")));
    }
    say(
        format!(
            "records: {} whole, {} merged{}",
            selections.len(),
            merges.len(),
            if carried.is_empty() { String::new() } else { format!(", up to {} carried forward from the existing patch", carried.len()) }
        ),
        &mut lines,
    );
    let own = output_name;
    let mut stand = selections.clone();
    stand.extend(new_records.iter().filter(|m| !m.source.is_empty()).map(|m| Selection { plugin: m.source.clone(), record_type: m.record_type.clone(), key: m.key.clone() }));
    let masters = masters_for(&stand, plan, sources, load_order, own);
    say(format!("declaring {} master(s): {}", masters.len(), masters.join(", ")), &mut lines);
    let mut decided: HashSet<(String, String)> = whole.iter().chain(merged.iter()).cloned().collect();
    decided.extend(new_records.iter().map(|m| (m.record_type.clone(), m.key.clone())));
    let mut records = if carried.is_empty() { Vec::new() } else { carry_forward(carried, &masters, &decided)? };
    let carried_count = records.len();
    if !carried.is_empty() {
        say(format!("{carried_count} record(s) carried forward from the existing patch"), &mut lines);
    }
    records.extend(collect(selections, sources, &masters)?);
    for m in merges {
        for line in describe(&m.choices, &m.base_plugin) {
            say(format!("  {} {}: {line}", m.record_type, m.key), &mut lines);
        }
        let built = merge_record(m, sources, &masters)?;
        place(&mut records, built, sources.get(&m.base_plugin).map(Vec::as_slice).unwrap_or(&[]));
    }
    for m in new_records {
        say(format!("  {} {}: made by the patch", m.record_type, m.key), &mut lines);
        let topic = topic_named(&records, &m.topic).cloned().or_else(|| topic_named(sources.get(&m.source).map(Vec::as_slice).unwrap_or(&[]), &m.topic).cloned());
        match topic {
            Some(t) if m.record_type == "DialogueInfo" => place_in(&mut records, m.record.clone(), &t),
            _ => records.push(m.record.clone()),
        }
    }
    if !ref_edits.is_empty() {
        let mut by_cell: IndexMap<String, Vec<&RefEdit>> = IndexMap::new();
        for e in ref_edits {
            by_cell.entry(e.cell.to_lowercase()).or_default().push(e);
        }
        for edits in by_cell.values() {
            let built = cell_patch_record(edits, sources, load_order, &masters, own)?;
            let cell = edits[0].cell.clone();
            say(format!("  Cell {cell}: {} changed reference(s)", edits.len()), &mut lines);
            let want = cell.to_lowercase();
            let have = records.iter().position(|r| rtype(r) == "Cell" && keys_of(r).iter().any(|k| k.to_lowercase() == want));
            match have {
                None => records.push(built),
                Some(at) => {
                    let key = |r: &Value| (r.get("mast_index").cloned().unwrap_or(Value::Null).to_string(), r.get("refr_index").cloned().unwrap_or(Value::Null).to_string());
                    let mut changed: IndexMap<(String, String), Value> = IndexMap::new();
                    for r in built["references"].as_array().into_iter().flatten() {
                        changed.insert(key(r), r.clone());
                    }
                    let mut kept: Vec<Value> = records[at].get("references").and_then(Value::as_array).into_iter().flatten().filter(|r| !changed.contains_key(&key(r))).cloned().collect();
                    kept.extend(changed.into_values());
                    records[at]["references"] = Value::Array(kept);
                }
            }
        }
    }
    if !new_refs.is_empty() {
        let given = place_new_refs(&mut records, new_refs, sources, load_order)?;
        for n in new_refs {
            let id = n.fields.get(&"id".to_string()).map(str_of).unwrap_or_else(|| "None".into());
            say(format!("  Cell {}: new {id} as (0, {})", n.cell, given[&n.uid]), &mut lines);
        }
    }
    for note in dialogue_position_risk(&records, sources) {
        say(format!("  note: {note}"), &mut lines);
    }
    for (topic, key, was, now) in shifts(sources, load_order, &records) {
        say(
            format!("  moves: {key} in topic {} goes from position {was} to {now}. A topic is read top down and the first response whose filters match is the one spoken.", str_repr(&topic)),
            &mut lines,
        );
    }
    for (key, anchor, plugin) in position_anchors(&records, sources) {
        say(
            format!(
                "  anchor: {key} sits next to {}..., which {plugin} carries unchanged only to hold this line's place. The patch does not carry it, so that position stays whatever the load order makes it.",
                first12(&anchor)
            ),
            &mut lines,
        );
    }
    let remapped = records.iter().filter(|r| r.get("references").is_some_and(truthy)).count();
    if remapped > 0 {
        say(format!("{remapped} record(s) had their references renumbered for the new master list"), &mut lines);
    }
    let missing: Vec<&str> = masters.iter().filter(|n| !sizes.contains_key(*n)).map(String::as_str).collect();
    if !missing.is_empty() {
        return fail(format!("cannot measure {}. A master's size goes in the header, and a wrong one makes the plugin look corrupt.", missing.join(", ")));
    }
    let with_sizes: Vec<(String, u64)> = masters.iter().map(|n| (n.clone(), sizes[n])).collect();
    let header = build_header(&with_sizes, records.len(), DESCRIPTION)?;
    let mut document = vec![header];
    document.extend(records.iter().cloned());
    let result = PatchResult { output: None, records: records.len(), carried: carried_count, masters, remapped, lines };
    Ok((document, result))
}

/// A reference's fields the crate cannot do without, given as the Python writer gives
/// them (its `Reference`'s defaults, the crate's): a reference the patch places names
/// only what it sets.
fn complete_references(document: &mut [Value]) {
    let Ok(Value::Object(blank)) = serde_json::to_value(tes3::esp::Reference::default()) else { return };
    for record in document.iter_mut().filter(|r| rtype(r) == "Cell") {
        let Some(refs) = record.get_mut("references").and_then(Value::as_array_mut) else { continue };
        for r in refs.iter_mut().filter_map(Value::as_object_mut) {
            for (k, v) in &blank {
                if !r.contains_key(k) {
                    r.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

/// A patch's document as plugin bytes (the crate's writer).
pub fn patch_bytes(document: &[Value]) -> Res<Vec<u8>> {
    let mut document = document.to_vec();
    complete_references(&mut document);
    crate::plugin::records_bytes(&Value::Array(document).to_string()).map_err(|e| BuildError(format!("could not encode the patch natively: {e}")))
}

/// The patch's records written to `target` (the crate's writer; the folder made).
pub fn write_patch(document: &[Value], target: &Path) -> Res<u64> {
    if let Some(dir) = target.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let bytes = patch_bytes(document)?;
    std::fs::write(target, &bytes).map_err(|e| BuildError(format!("could not encode the patch natively: {e}")))?;
    Ok(bytes.len() as u64)
}

/// What a build captured (`prepare_patch`'s job): the queue's decisions, the plugins to
/// read, and what finishing spends.
#[derive(Clone, Debug)]
pub struct Job {
    pub target: PathBuf,
    pub plan: Plan,
    /// The load order.
    pub order: Vec<String>,
    /// The plugins to read whole, sorted.
    pub names: Vec<String>,
    /// The load order's files `(name, path)`, read by [`run_patch_job`].
    pub paths: Vec<(String, PathBuf)>,
    /// The masters' sizes, by name.
    pub sizes: HashMap<String, u64>,
    /// What the queue held: finishing spends exactly this, not later edits.
    pub snapshot: crate::queue::Decisions,
}

impl Editor {
    /// The queued field choices as merges, each on the base the Editor queued for it.
    pub fn merges(&self) -> Vec<Merge> {
        self.queue
            .fields()
            .iter()
            .map(|((t, k), choices)| Merge { record_type: t.clone(), key: k.clone(), base_plugin: self.queue.base(t, k).to_string(), choices: choices.clone() })
            .collect()
    }

    /// What the queue holds, by kind: `{records, whole, merged, refs, new_refs,
    /// new_records}`.
    pub fn patch_summary(&self) -> Value {
        json!({
            "records": self.queue.len(),
            "whole": self.queue.selections().len(),
            "merged": self.queue.fields().len(),
            "refs": self.queue.ref_edits().count(),
            "new_refs": self.queue.new_refs().count(),
            "new_records": self.queue.new_records().count(),
        })
    }

    /// A build of the queue to `target` made ready (`append`: what the file holds
    /// already rides along). Refused with nothing queued, or a merge whose base the
    /// Editor no longer knows.
    pub fn prepare_patch(&self, target: &Path, append: bool) -> Res<Job> {
        if self.queue.is_empty() {
            return fail("Nothing is queued: there is no patch to write.");
        }
        let merges = self.merges();
        if let Some(m) = merges.iter().find(|m| m.base_plugin.is_empty()) {
            return fail(format!(
                "{} {} is no longer in the scan, so there is no way to tell which plugin its unchosen fields should come from. Rescan and queue it again.",
                m.record_type, m.key
            ));
        }
        let carried = if append && target.exists() {
            let got: Vec<Value> = std::fs::read(target).ok().and_then(|b| crate::plugin::records_json(&b, None).ok()).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
            if got.is_empty() {
                let name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                return fail(format!("{name} could not be decoded, so there is nothing to append to. Write again and replace it, or check the file opens in another tool first."));
            }
            got
        } else {
            Vec::new()
        };
        let plan = Plan {
            selections: self.queue.selections().to_vec(),
            merges,
            carried,
            ref_edits: self.queue.ref_edits().cloned().collect(),
            new_refs: self.queue.new_refs().cloned().collect(),
            new_records: self.queue.new_records().cloned().collect(),
        };
        let mut wanted: BTreeSet<String> = plan.selections.iter().map(|s| s.plugin.clone()).collect();
        for m in &plan.merges {
            wanted.extend(m.plugins());
        }
        for e in &plan.ref_edits {
            wanted.insert(e.origin.clone());
            wanted.extend(e.plugins.iter().cloned());
        }
        wanted.extend(plan.new_records.iter().filter(|m| !m.source.is_empty()).map(|m| m.source.clone()));
        for n in &plan.new_refs {
            wanted.extend(n.plugins.iter().cloned());
            if !n.base_plugin.is_empty() {
                wanted.insert(n.base_plugin.clone());
            }
        }
        let order = self.session.order().to_vec();
        let sizes = order.iter().zip(self.session.plugin_paths()).filter_map(|(n, p)| std::fs::metadata(&p).ok().map(|m| (n.clone(), m.len()))).collect();
        let paths = order.iter().cloned().zip(self.session.plugin_paths()).collect();
        Ok(Job { target: target.to_path_buf(), plan, order, names: wanted.into_iter().collect(), paths, sizes, snapshot: self.queue.snapshot() })
    }

    /// The finished build's decisions spent from the queue (an edit made since the job
    /// was prepared stays), the queue saved, and the patch written remembered.
    pub fn finish_patch(&mut self, job: &Job, result: &PatchResult) {
        self.queue.consume(&job.snapshot);
        self.save_journal();
        if let Some(out) = &result.output {
            self.last_patch = Some(out.clone());
        }
    }
}

/// A prepared build run with its plugins read from their files (a session of its own,
/// so no lock on the Editor's is held while it runs).
pub fn run_patch_job(job: &Job, dry_run: bool, say: &mut dyn FnMut(&str)) -> Res<PatchResult> {
    run_patch(&crate::session::Session::new(job.paths.clone()), job, dry_run, say)
}

/// A prepared build run (`run_patch`): its plugins read whole, the patch built and
/// (unless `dry_run`) written to the job's target. `say` hears each line as it is made.
pub fn run_patch(session: &crate::session::Session, job: &Job, dry_run: bool, say: &mut dyn FnMut(&str)) -> Res<PatchResult> {
    let sources: Sources = job.names.iter().map(|n| (n.clone(), session.read_whole(n).unwrap_or_default())).collect();
    let name = job.target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let (document, mut result) = build_record_patch(&job.plan, &sources, &job.order, &job.sizes, &name, say)?;
    if dry_run {
        let line = "dry run: nothing was written".to_string();
        say(&line);
        result.lines.push(line);
        return Ok(result);
    }
    let size = write_patch(&document, &job.target)?;
    let line = format!("wrote {} ({size} bytes, {} records)", job.target.display(), document.len());
    say(&line);
    result.lines.push(line);
    result.output = Some(job.target.clone());
    Ok(result)
}

/// What a written patch is, in a sentence or two (`patch_written_note`).
pub fn written_note(result: &PatchResult) -> String {
    let carried = if result.carried > 0 { format!(" ({} carried forward from before)", result.carried) } else { String::new() };
    format!(
        "{} record(s) written to:\n{}{carried}\n\nDeclares {} master(s). Add it to your load order LAST.",
        result.records,
        result.output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "None".into()),
        result.masters.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_lua_data_reads() {
        // The editor's (and the Python tests') record: Lua data written as "".
        let mut v: Value = serde_json::from_str(
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

    #[test]
    fn lual_entries_follow_the_master_list() {
        let v: Value = serde_json::from_str(
            r#"{"type":"ScriptConfigList","flags":"","scripts":[{"path":"scripts/a.lua","init_data":"",
            "flags":"","types":["NPC_"],"records":[],
            "instances":[{"attach":true,"mast_idx":1,"ref_idx":7,"data":""}]}]}"#,
        )
        .unwrap();
        let out = lual_remap(&v, &HashMap::from([(0, 3), (1, 2)])).unwrap();
        assert_eq!(out["scripts"][0]["instances"][0]["mast_idx"], json!(2));
        assert!(lual_remap(&v, &HashMap::from([(0, 3)])).unwrap_err().contains("content file 1"));
    }

    fn npc(id: &str, level: i64) -> Value {
        let mut r = crate::blank::blank("Npc").unwrap();
        r["id"] = json!(id);
        r["data"]["level"] = json!(level);
        r
    }

    fn plugin(path: &Path, masters: &[&str], records: Vec<Value>) {
        let mut doc = vec![build_header(&masters.iter().map(|m| (m.to_string(), 100)).collect::<Vec<_>>(), records.len(), "").unwrap_or_else(|_| json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 0, "masters": []}))];
        doc.extend(records);
        std::fs::write(path, patch_bytes(&doc).unwrap()).unwrap();
    }

    #[test]
    fn a_build_writes_the_queue_and_spends_it() {
        let dir = std::env::temp_dir().join(format!("wg-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (mw, md) = (dir.join("Morrowind.esm"), dir.join("Mod.esp"));
        plugin(&mw, &[], vec![npc("fargoth", 2), npc("caius", 3)]);
        plugin(&md, &["Morrowind.esm"], vec![npc("fargoth", 10)]);
        let session = crate::session::Session::new(vec![("Morrowind.esm".into(), mw), ("Mod.esp".into(), md)]);
        let mut ed = Editor::new(session, crate::queue::PatchQueue::new(), None);
        let target = dir.join("Patch.esp");
        assert_eq!(ed.prepare_patch(&target, false).unwrap_err().0, "Nothing is queued: there is no patch to write.");

        ed.queue.add_whole(Selection { plugin: "Mod.esp".into(), record_type: "Npc".into(), key: "fargoth".into() });
        let job = ed.prepare_patch(&target, false).unwrap();
        assert_eq!(job.names, ["Mod.esp"]);
        let mut heard = Vec::new();
        let dry = run_patch(&ed.session, &job, true, &mut |l| heard.push(l.to_string())).unwrap();
        assert_eq!((dry.output.as_ref(), heard.last().map(String::as_str)), (None, Some("dry run: nothing was written")));
        assert!(!target.exists());
        // An edit made while the build runs is not spent by it.
        ed.queue.add_whole(Selection { plugin: "Morrowind.esm".into(), record_type: "Npc".into(), key: "caius".into() });
        let result = run_patch(&ed.session, &job, false, &mut |_| {}).unwrap();
        ed.finish_patch(&job, &result);
        assert_eq!((result.records, result.masters.clone()), (1, vec!["Morrowind.esm".to_string(), "Mod.esp".into()]));
        assert!(result.lines.last().unwrap().starts_with("wrote "));
        assert_eq!(ed.queue.selections().len(), 1);
        assert!(written_note(&result).starts_with("1 record(s) written to:\n"));

        // Appended: the first build rides along.
        let job = ed.prepare_patch(&target, true).unwrap();
        assert_eq!(job.plan.carried.len(), 2);
        let result = run_patch(&ed.session, &job, false, &mut |_| {}).unwrap();
        ed.finish_patch(&job, &result);
        assert_eq!((result.records, result.carried), (2, 1));
        assert!(ed.queue.is_empty());
        let back: Vec<Value> = serde_json::from_str(&crate::plugin::records_json(&std::fs::read(&target).unwrap(), None).unwrap()).unwrap();
        let ids: Vec<&str> = back.iter().map(|r| r["id"].as_str().unwrap_or("")).collect();
        assert_eq!(ids, ["", "fargoth", "caius"]);
        assert_eq!(back[1]["data"]["level"], json!(10));
        let _ = std::fs::remove_dir_all(&dir);
    }

}
