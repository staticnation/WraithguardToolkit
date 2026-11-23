//! Merged Objects: the records two or more plugins change, merged into one plugin loaded last,
//! so each plugin's changes are kept, not only the last one's. A port of NullCascade's
//! TES3Merge (MIT, © 2019 NullCascade: `RecordMerger.cs`, `Merger/*.cs`, `MergeCommand.cs`)
//! onto greatness7's tes3 crate and its records as values (tes3conv's schema), Static
//! 2026-11-18 ("this will help vanilla and mo2 so much").
//!
//! The rule (`MergeAndPatchRecords`): a record's first appearance in the load order is the base,
//! and the merged record starts as its last. The versions between are taken from the latest back
//! to the earliest; each fills in what the merged record still has as the base and that version
//! changed (`MergePropertyBase`: the merged value unchanged from the base, the version's changed,
//! so the version's is taken). A record two plugins define only once each is not merged - the
//! later one already wins whole - but the patches apply to every record.
//!
//! Values are merged field by field, objects opened (a subrecord's fields, as TES3Lib's
//! subrecords), a list taken whole when it changed; fixed-size number arrays inside a record's
//! data (an NPC's attributes and skills, a creature's attacks, a color) number by number, as
//! TES3Lib's named fields are. As TES3Merge has it, besides:
//!
//! - leveled lists (`LEVI.ITEM`, `LEVC.CRIT`): every entry any of the three has, each as many
//!   times as the most any has it, sorted by level then id - set when that differs from the base;
//! - spell, enchantment and potion effects (`Shared.EffectList`): slot by slot, the first eight;
//!   an effect changed whole when its magic effect changed, else field by field, and one the
//!   base lacks added;
//! - AI packages (`NPC_.AIPackage`, `CREA.AIPackage`): only the first Wander package, field by
//!   field; the rest are the last plugin's;
//! - an NPC's or creature's inventory: whole, or with `inclusive` every item any version has
//!   (`Shared.ItemsList`, `--inclusive`);
//! - a class: only its specialization and playable flag (`CLAS.CLDT`); a faction: only its
//!   hidden flag (`FACT.FADT`);
//! - a cell (`CELL.Merge`): only its record flags, and an exterior's name where a later plugin
//!   renamed it and the last did not (`cellnames`), written without its references;
//! - `summons`: the known summoned creatures flagged persistent; `fogbug`: an interior that is
//!   not like an exterior given fog density 0.01 where it has 0.
//!
//! A field a version leaves out that the base has (an optional subrecord it dropped, a list it
//! emptied) is not taken, as TES3Merge keeps the merged record's there.
//!
//! Different from TES3Merge, on purpose (each seen comparing the two on real plugins, 2026-11-18):
//!
//! - the patches apply to every record. TES3Merge's `isAnythingChanged || Patch(...)` never calls
//!   `Patch` once a merge changed something, so a merged summoned creature was not made
//!   persistent, and a merged fogbugged cell not fixed;
//! - a record the last plugin deletes is left deleted (TES3Merge merged the others' fields into
//!   the deleted record and wrote that), and a version that deletes it gives nothing;
//! - a record is written only when its values differ from the last plugin's (TES3Merge also wrote
//!   records its library merely encoded differently);
//! - an NPC's flags are merged as one value with its blood type beside them (TES3Merge's merge of
//!   the packed flags gave Igna a blood type no plugin has);
//! - ids are compared without case, as the engines compare them (TES3Merge keys them as written,
//!   so "Wares" and "wares" were two records);
//! - a plugin that cannot be read is left out and said, where TES3Merge stopped.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The record types TES3Merge merges by default (`Util.GetMergeTags`; FACT, DIAL, INFO, LAND,
/// LTEX, PGRD, REGN, SCPT and GLOB are not).
pub const DEFAULT_TYPES: [&str; 32] = [
    "ACTI", "ALCH", "APPA", "ARMO", "BODY", "BOOK", "BSGN", "CELL", "CLAS", "CLOT", "CONT", "CREA", "DOOR", "ENCH", "GMST", "INGR", "LEVC", "LEVI",
    "LIGH", "LOCK", "MGEF", "MISC", "NPC_", "PROB", "RACE", "REPA", "SKIL", "SNDG", "SOUN", "SPEL", "STAT", "WEAP",
];

/// The summoned creatures `summons` flags persistent (`CREA.SummonedCreatures`).
pub const SUMMONED: [&str; 22] = [
    "centurion_fire_dead",
    "wraith_sul_senipul",
    "ancestor_ghost_summon",
    "atronach_flame_summon",
    "atronach_frost_summon",
    "atronach_storm_summon",
    "bonelord_summon",
    "bonewalker_summon",
    "bonewalker_greater_summ",
    "centurion_sphere_summon",
    "clannfear_summon",
    "daedroth_summon",
    "dremora_summon",
    "golden saint_summon",
    "hunger_summon",
    "scamp_summon",
    "skeleton_summon",
    "ancestor_ghost_variner",
    "fabricant_summon",
    "bm_bear_black_summon",
    "bm_wolf_grey_summon",
    "bm_wolf_bone_summon",
];

/// What to merge and how (TES3Merge's options and `TES3Merge.ini`).
#[derive(Clone, Debug)]
pub struct Options {
    /// The record types (tags) to merge.
    pub types: Vec<String>,
    /// An NPC's or creature's inventory merged inclusively (`--inclusive`).
    pub inclusive: bool,
    pub fogbug: bool,
    pub summons: bool,
    pub cellnames: bool,
    /// Plugins left out, by name, case ignored (`[FileFilters]`); the output's own is always.
    pub ignore: Vec<String>,
    /// Ids allowed or not, each a regular expression over the id in lower case, the last match
    /// deciding (`[ObjectFilters]`).
    pub filters: Vec<(String, bool)>,
    /// Only the leveled lists merged, and the patches (TES3Merge's `multipatch`): nothing else of
    /// a record is.
    pub lists_only: bool,
    /// The output's file name.
    pub file_name: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            types: DEFAULT_TYPES.iter().map(|s| (*s).to_string()).collect(),
            inclusive: false,
            fogbug: true,
            summons: true,
            cellnames: true,
            ignore: Vec::new(),
            filters: Vec::new(),
            lists_only: false,
            file_name: "Merged Objects.esp".into(),
        }
    }
}

/// A merge's result: the merged records in TES3Merge's order (types as first met, ids as first
/// met), the plugins they need as masters in load order, and what was done.
#[derive(Debug, Default)]
pub struct Outcome {
    pub records: Vec<Value>,
    pub masters: Vec<String>,
    pub lines: Vec<String>,
    /// How many records of each type were merged, in the order of `records`.
    pub by_type: Vec<(String, usize)>,
    /// Plugins that could not be read, and why: left out of the merge.
    pub unreadable: Vec<(String, String)>,
    /// Records of a type the crate can read but a merge failed for (the record left out).
    pub failed: Vec<String>,
}

/// A value's record tag, from its `type`.
fn tag_of_type(t: &str) -> Option<&'static str> {
    Some(match t {
        "Activator" => "ACTI",
        "Alchemy" => "ALCH",
        "Apparatus" => "APPA",
        "Armor" => "ARMO",
        "Bodypart" | "BodyPart" => "BODY",
        "Book" => "BOOK",
        "Birthsign" => "BSGN",
        "Cell" => "CELL",
        "Class" => "CLAS",
        "Clothing" => "CLOT",
        "Container" => "CONT",
        "Creature" => "CREA",
        "Door" => "DOOR",
        "Enchanting" => "ENCH",
        "Faction" => "FACT",
        "GameSetting" => "GMST",
        "GlobalVariable" => "GLOB",
        "Ingredient" => "INGR",
        "LeveledCreature" => "LEVC",
        "LeveledItem" => "LEVI",
        "Light" => "LIGH",
        "Lockpick" => "LOCK",
        "MagicEffect" => "MGEF",
        "MiscItem" => "MISC",
        "Npc" => "NPC_",
        "Probe" => "PROB",
        "Race" => "RACE",
        "RepairItem" => "REPA",
        "Skill" => "SKIL",
        "SoundGen" => "SNDG",
        "Sound" => "SOUN",
        "Spell" => "SPEL",
        "Static" => "STAT",
        "Weapon" => "WEAP",
        "Region" => "REGN",
        "Script" => "SCPT",
        _ => return None,
    })
}

fn has_flag(v: Option<&Value>, flag: &str) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| s.split('|').any(|f| f.trim() == flag))
}

fn is_interior(cell: &Value) -> bool {
    has_flag(cell.pointer("/data/flags"), "IS_INTERIOR")
}

/// What names a record (`GetEditorId`): its id; a cell's interior name, or an exterior's grid
/// as `(x,y)`; a skill's or magic effect's own id.
pub fn record_id(v: &Value) -> Option<String> {
    if v.get("type").and_then(Value::as_str) == Some("Cell") {
        if is_interior(v) {
            return v.get("name").and_then(Value::as_str).map(String::from);
        }
        let g = v.pointer("/data/grid")?;
        return Some(format!("({},{})", g.get(0)?.as_i64()?, g.get(1)?.as_i64()?));
    }
    ["id", "skill_id", "effect_id"].iter().find_map(|k| match v.get(*k)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

// ---- the merge rules ------------------------------------------------------------------------

/// `MergePropertyBase`: `current` takes `next` where it is still `first` and `next` is not.
fn take_if_changed(current: &mut Value, first: &Value, next: &Value) -> bool {
    if current == first && next != first {
        *current = next.clone();
        return true;
    }
    false
}

fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap_or(&Value::Null)
}

/// Field by field: objects opened, fixed number arrays inside a record (`depth` > 0) number by
/// number, anything else taken whole (`MergeAllProperties`).
fn merge_value(current: &mut Value, first: &Value, next: &Value, depth: usize) -> bool {
    match (&mut *current, first, next) {
        (Value::Object(c), Value::Object(f), Value::Object(n)) => {
            let mut keys: Vec<String> = n.keys().cloned().collect();
            for k in f.keys().chain(c.keys()) {
                if !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
            let mut changed = false;
            for k in keys {
                let (fv, nv) = (f.get(&k).cloned().unwrap_or(Value::Null), n.get(&k).cloned().unwrap_or(Value::Null));
                let cv = c.entry(k).or_insert(Value::Null);
                changed |= merge_value(cv, &fv, &nv, depth + 1);
            }
            changed
        }
        (Value::Array(c), Value::Array(f), Value::Array(n))
            if depth > 1 && c.len() == f.len() && f.len() == n.len() && f.iter().chain(n.iter()).chain(c.iter()).all(Value::is_number) =>
        {
            let mut changed = false;
            for i in 0..f.len() {
                changed |= take_if_changed(&mut c[i], &f[i], &n[i]);
            }
            changed
        }
        _ => take_if_changed(current, first, next),
    }
}

/// `Shared.EffectList`: the first eight effects slot by slot.
fn merge_effects(current: &mut Value, first: &Value, next: &Value) -> bool {
    let (Some(f), Some(n)) = (first.as_array(), next.as_array()) else { return take_if_changed(current, first, next) };
    if !current.is_array() {
        *current = Value::Array(f.clone());
    }
    let c = current.as_array_mut().expect("an array");
    let mut changed = false;
    for i in 0..8 {
        match (i < c.len(), f.get(i), n.get(i)) {
            (true, Some(fe), Some(ne)) => {
                if fe.get("magic_effect") != ne.get("magic_effect") {
                    if c[i] != *ne {
                        c[i] = ne.clone();
                        changed = true;
                    }
                } else {
                    changed |= merge_value(&mut c[i], fe, ne, 1);
                }
            }
            (_, None, Some(ne)) => {
                c.push(ne.clone());
                changed = true;
            }
            _ => {}
        }
    }
    changed
}

/// `NPC_.AIPackage`: only the first Wander package, field by field.
fn merge_ai_packages(current: &mut Value, first: &Value, next: &Value) -> bool {
    let wander = |v: &Value| v.as_array().and_then(|a| a.iter().position(|p| p.get("type").and_then(Value::as_str) == Some("Wander")));
    let (Some(ci), Some(fi), Some(ni)) = (wander(current), wander(first), wander(next)) else { return false };
    let (f, n) = (first[fi].clone(), next[ni].clone());
    merge_value(&mut current.as_array_mut().expect("an array")[ci], &f, &n, 1)
}

/// The order TES3Merge's `OrderBy(level).ThenBy(id)` gives ids: .NET's culture comparison, which
/// puts punctuation before digits before letters and ignores case before it compares it.
fn culture_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn class(c: char) -> u8 {
        if c.is_alphabetic() {
            2
        } else if c.is_numeric() {
            1
        } else {
            0
        }
    }
    let key = |s: &str| s.chars().map(|c| (class(c), c.to_lowercase().next().unwrap_or(c))).collect::<Vec<_>>();
    key(a).cmp(&key(b)).then_with(|| {
        // Lower case first where only case differs, as ICU's root collation has it.
        let flip = |s: &str| s.chars().map(|c| if c.is_lowercase() { 0 } else { 1 }).collect::<Vec<u8>>();
        flip(a).cmp(&flip(b))
    })
}

/// `LEVI.ITEM` / `LEVC.CRIT`: the minimal inclusive union of the three lists' `[id, level]`
/// entries, sorted by level then id; set when it differs from the base.
fn merge_leveled(current: &mut Value, first: &Value, next: &Value) -> bool {
    type Entry = (String, i64);
    let entries = |v: &Value| -> Vec<Entry> {
        v.as_array()
            .map(|a| a.iter().filter_map(|e| Some((e.get(0)?.as_str()?.to_string(), e.get(1)?.as_i64()?))).collect())
            .unwrap_or_default()
    };
    let (f, c, n) = (entries(first), entries(current), entries(next));
    let mut order: Vec<Entry> = Vec::new();
    let mut most: HashMap<Entry, usize> = HashMap::new();
    for list in [&f, &c, &n] {
        let mut count: HashMap<&Entry, usize> = HashMap::new();
        for e in list.iter() {
            *count.entry(e).or_insert(0) += 1;
        }
        for e in list.iter() {
            if !most.contains_key(e) {
                order.push(e.clone());
            }
            let m = most.entry(e.clone()).or_insert(0);
            *m = (*m).max(count[e]);
        }
    }
    let mut union: Vec<Entry> = order.iter().flat_map(|e| std::iter::repeat_n(e.clone(), most[e])).collect();
    union.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| culture_cmp(&a.0, &b.0)));
    if union == f {
        return false;
    }
    let v = Value::Array(union.into_iter().map(|(id, lvl)| serde_json::json!([id, lvl])).collect());
    if *current == v {
        return false;
    }
    *current = v;
    true
}

/// `Shared.ItemsList` (`--inclusive`): every inventory entry any of the three has, in the order
/// met; set when that differs from the base.
fn merge_inventory_inclusive(current: &mut Value, first: &Value, next: &Value) -> bool {
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    let mut union: Vec<Value> = Vec::new();
    for e in list(first).into_iter().chain(list(current)).chain(list(next)) {
        if !union.contains(&e) {
            union.push(e);
        }
    }
    if union == list(first) || *current == Value::Array(union.clone()) {
        return false;
    }
    *current = Value::Array(union);
    true
}

/// Only the named fields of an object, each taken where changed (`MergeNamedProperties`).
fn merge_named(current: &mut Value, first: &Value, next: &Value, names: &[&str]) -> bool {
    let mut changed = false;
    if let Some(c) = current.as_object_mut() {
        for k in names {
            let cv = c.entry((*k).to_string()).or_insert(Value::Null);
            changed |= take_if_changed(cv, get(first, k), get(next, k));
        }
    }
    changed
}

/// `RecordMerger.Merge`: `next` merged into `current` against the base `first`.
fn merge_record(tag: &str, current: &mut Value, first: &Value, next: &Value, opts: &Options) -> bool {
    let Some(c) = current.as_object_mut() else { return false };
    let mut changed = false;
    // The record flags (`MergeTypeRecord`, `CELL.Merge`).
    changed |= take_if_changed(c.entry("flags").or_insert(Value::Null), get(first, "flags"), get(next, "flags"));
    if tag == "CELL" {
        // `cellnames`: an exterior keeps a later plugin's rename the last one reverted.
        if opts.cellnames && !is_interior(current) {
            let (f, n) = (get(first, "name"), get(next, "name"));
            let cur = current.get("name").cloned().unwrap_or(Value::Null);
            if cur == *f && n != f && n.as_str().is_some_and(|s| !s.trim_end_matches('\0').is_empty()) {
                current["name"] = n.clone();
                changed = true;
            }
        }
        return changed;
    }
    let mut keys: Vec<String> = next.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    for k in first.as_object().into_iter().flat_map(|o| o.keys()) {
        if !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    for k in keys {
        if matches!(k.as_str(), "type" | "flags" | "id" | "skill_id" | "effect_id") {
            continue;
        }
        let (f, n) = (get(first, &k).clone(), get(next, &k).clone());
        let c = current.as_object_mut().expect("an object").entry(k.clone()).or_insert(Value::Null);
        let list = matches!((tag, k.as_str()), ("LEVI", "items") | ("LEVC", "creatures"));
        if opts.lists_only {
            if list {
                changed |= merge_leveled(c, &f, &n);
            }
            continue;
        }
        changed |= match (tag, k.as_str()) {
            _ if list => merge_leveled(c, &f, &n),
            ("SPEL" | "ENCH" | "ALCH", "effects") => merge_effects(c, &f, &n),
            ("NPC_" | "CREA", "ai_packages") => merge_ai_packages(c, &f, &n),
            ("NPC_" | "CREA", "inventory") if opts.inclusive => merge_inventory_inclusive(c, &f, &n),
            ("CLAS", "data") => merge_named(c, &f, &n, &["specialization", "flags"]),
            ("FACT", "data") => merge_named(c, &f, &n, &["flags"]),
            // A field a version leaves out that the base has - an optional subrecord it
            // dropped (an enchantment, a script) or a list it emptied - is not taken: the merged
            // record keeps what it has (`MergeAllProperties`: "retain the latest value").
            _ if absent(&n) && !absent(&f) => false,
            _ => merge_value(c, &f, &n, 1),
        };
    }
    changed
}

/// A record field that stands for a subrecord the record does not have: nothing, an empty
/// string, an empty list.
fn absent(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

fn deleted(v: &Value) -> bool {
    has_flag(v.get("flags"), "DELETED")
}

/// The patches (`MergeAction.Patch`): `summons` and `fogbug`.
fn patch_record(tag: &str, v: &mut Value, id: &str, opts: &Options) -> bool {
    if opts.summons && tag == "CREA" && SUMMONED.contains(&id.to_lowercase().as_str()) && !has_flag(v.get("flags"), "PERSISTENT") {
        let flags = v.get("flags").and_then(Value::as_str).unwrap_or("").trim().to_string();
        v["flags"] = Value::String(if flags.is_empty() { "PERSISTENT".into() } else { format!("{flags} | PERSISTENT") });
        return true;
    }
    if opts.fogbug
        && tag == "CELL"
        && is_interior(v)
        && !has_flag(v.pointer("/data/flags"), "BEHAVES_LIKE_EXTERIOR")
        && let Some(d) = v.pointer_mut("/atmosphere_data/fog_density")
        && d.as_f64() == Some(0.0)
    {
        *d = serde_json::json!(0.01f32);
        return true;
    }
    false
}

// ---- the run --------------------------------------------------------------------------------

/// One plugin's records of the merged types, as values (cells without their references), a
/// record read leniently marked (`plugin::LOSSY`).
fn read_plugin(path: &Path, tags: &[[u8; 4]]) -> Result<Vec<Value>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    crate::plugin::values(&bytes, Some(tags), true).map_err(|e| e.to_string())
}

/// Merges `plugins` (`(name, path)` in load order) as TES3Merge does (see the module).
pub fn merge(plugins: &[(String, PathBuf)], opts: &Options) -> Outcome {
    let mut out = Outcome::default();
    let tags: Vec<[u8; 4]> = opts
        .types
        .iter()
        .filter_map(|t| {
            let b = t.as_bytes();
            (b.len() == 4).then(|| [b[0], b[1], b[2], b[3]])
        })
        .collect();
    let wanted: HashSet<&str> = opts.types.iter().map(String::as_str).collect();
    let ignore: HashSet<String> = opts.ignore.iter().map(|s| s.to_lowercase()).chain(std::iter::once(opts.file_name.to_lowercase())).collect();
    let filters: Vec<(regex::Regex, bool)> = opts
        .filters
        .iter()
        .filter_map(|(re, allow)| match regex::Regex::new(re) {
            Ok(r) => Some((r, *allow)),
            Err(e) => {
                out.lines.push(format!("WARNING: filter {re:?} could not be read: {e}"));
                None
            }
        })
        .collect();
    let read: Vec<(usize, &(String, PathBuf))> = plugins.iter().enumerate().filter(|(_, (n, _))| !ignore.contains(&n.to_lowercase())).collect();
    for (_, (n, _)) in plugins.iter().enumerate().filter(|(_, (n, _))| ignore.contains(&n.to_lowercase())) {
        if !n.eq_ignore_ascii_case(&opts.file_name) {
            out.lines.push(format!("Ignoring file {n}"));
        }
    }
    // Read across the cores, kept in load order.
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let chunk = read.len().div_ceil(threads).max(1);
    let loaded: Vec<(usize, Result<Vec<Value>, String>)> = std::thread::scope(|s| {
        let jobs: Vec<_> = read.chunks(chunk).map(|part| s.spawn(|| part.iter().map(|(i, (_, p))| (*i, read_plugin(p, &tags))).collect::<Vec<_>>())).collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    });
    // Each record's versions in load order, keyed by type and id (case ignored).
    let mut order: Vec<(String, String)> = Vec::new();
    let mut versions: HashMap<(String, String), Vec<(usize, Value)>> = HashMap::new();
    for (i, got) in loaded {
        let name = &plugins[i].0;
        match got {
            Err(e) => {
                out.lines.push(format!("WARNING: {name} could not be read ({e}); it is left out of the merge."));
                out.unreadable.push((name.clone(), e));
            }
            Ok(values) => {
                for v in values {
                    let Some(tag) = v.get("type").and_then(Value::as_str).and_then(tag_of_type) else { continue };
                    if !wanted.contains(tag) {
                        continue;
                    }
                    let Some(id) = record_id(&v).map(|s| s.replace('\0', "")).filter(|s| !s.is_empty()) else { continue };
                    let low = id.to_lowercase();
                    let mut allow = true;
                    for (re, a) in &filters {
                        if re.is_match(&low) {
                            allow = *a;
                        }
                    }
                    if !allow {
                        continue;
                    }
                    let key = (tag.to_string(), low);
                    let e = versions.entry(key.clone()).or_default();
                    if e.is_empty() {
                        order.push(key);
                    }
                    e.push((i, v));
                }
            }
        }
    }
    // TES3Merge's order: the types as first met, then each type's ids as first met.
    let mut type_order: Vec<String> = Vec::new();
    for (t, _) in &order {
        if !type_order.contains(t) {
            type_order.push(t.clone());
        }
    }
    let mut used: HashSet<usize> = HashSet::new();
    for t in &type_order {
        for key in order.iter().filter(|k| &k.0 == t) {
            let marked = &versions[key];
            // A record the last plugin deletes stays deleted: nothing is merged into it.
            if deleted(&marked[marked.len() - 1].1) {
                continue;
            }
            // The merge starts from the last version: one read past a subrecord its type does
            // not know (lenient reading) that could not be kept would lose it, so the record is
            // left out. The last version's kept subrecords stay with the merged record; an
            // earlier version's are not merged (TES3Merge does not know them either).
            if let Some(why) = crate::plugin::lossy(&marked[marked.len() - 1].1) {
                let id = record_id(&marked[marked.len() - 1].1).unwrap_or_default();
                out.failed.push(format!("{t} {id}"));
                out.lines.push(format!(
                    "Left out {t} record '{id}': {} was read past a subrecord its type does not know ({why}), and merging it would drop that subrecord.",
                    plugins[marked[marked.len() - 1].0].0
                ));
                continue;
            }
            let recs: Vec<(usize, Value)> = marked
                .iter()
                .enumerate()
                .map(|(n, (i, v))| {
                    let mut v = v.clone();
                    if let Value::Object(m) = &mut v {
                        m.remove(crate::plugin::LOSSY);
                        if n + 1 < marked.len() {
                            m.remove(crate::plugin::UNKNOWN);
                        }
                    }
                    (*i, v)
                })
                .collect();
            let recs = &recs;
            let (first, last) = (&recs[0].1, &recs[recs.len() - 1].1);
            let id = record_id(last).unwrap_or_default();
            let mut current = last.clone();
            let mut from: Vec<usize> = vec![recs[0].0, recs[recs.len() - 1].0];
            if recs.len() > 2 && !deleted(first) {
                for (pi, v) in recs[1..recs.len() - 1].iter().rev().filter(|(_, v)| !deleted(v)) {
                    if merge_record(t, &mut current, first, v, opts) {
                        from.push(*pi);
                    }
                }
            }
            patch_record(t, &mut current, &id, opts);
            if current == *last {
                continue;
            }
            // The value must still be a record the crate writes.
            if serde_json::from_value::<tes3::esp::TES3Object>(current.clone()).is_err() {
                out.failed.push(format!("{t} {id}"));
                out.lines.push(format!("Could not resolve conflicts for {t} record '{id}': the merged record does not encode."));
                continue;
            }
            let mut names: Vec<usize> = from.clone();
            names.sort_unstable();
            names.dedup();
            out.lines.push(format!(
                "Merged {t} record: {id} (from {})",
                names.iter().map(|&i| plugins[i].0.as_str()).collect::<Vec<_>>().join(", ")
            ));
            used.extend(names);
            match out.by_type.iter_mut().find(|(k, _)| k == t) {
                Some((_, n)) => *n += 1,
                None => out.by_type.push((t.clone(), 1)),
            }
            out.records.push(current);
        }
    }
    let mut masters: Vec<usize> = used.into_iter().collect();
    masters.sort_unstable();
    out.masters = masters.into_iter().map(|i| plugins[i].0.clone()).collect();
    out
}

/// The merged plugin's bytes: a header naming `masters` with their sizes, then the records.
pub fn plugin_bytes(outcome: &Outcome, sizes: &HashMap<String, u64>, description: &str) -> Result<Vec<u8>, String> {
    let masters: Vec<(String, u64)> = outcome.masters.iter().map(|m| (m.clone(), sizes.get(m).copied().unwrap_or(0))).collect();
    let header = crate::build::build_header(&masters, outcome.records.len(), description).map_err(|e| e.to_string())?;
    let doc: Vec<Value> = std::iter::once(header).chain(outcome.records.iter().cloned()).collect();
    crate::plugin::records_bytes(&Value::Array(doc).to_string()).map_err(|e| e.to_string())
}

/// `bytes` written to `path`; a file already there keeps its date (its place in a load order
/// sorted by date, as TES3Merge keeps it).
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let kept = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::plugin_io::write_whole(path, bytes)?;
    if let Some(t) = kept {
        std::fs::File::options().write(true).open(path)?.set_modified(t)?;
    }
    Ok(())
}

/// A merge run over a setup: what was said, the file written (none on a dry run or with
/// nothing to merge), and the merge itself.
#[derive(Debug, Default)]
pub struct Run {
    pub status: String,
    pub written: Option<PathBuf>,
    pub lines: Vec<String>,
    pub outcome: Outcome,
}

/// The merge over a setup (`ordercore::install`'s name: an `openmw.cfg`, `mo2:<folder>`, a
/// `Data Files`), as the Tools page and the command line run it: the load order's plugins found
/// in its data folders, merged, and written into `folder` (none: the data folder holding most of
/// the load order, as Merge Lands picks) unless `dry_run`. `filters` are lines of
/// `regex = true|false` (TES3Merge.ini's `[ObjectFilters]`); a line that is not is said and skipped.
pub fn run(setup_name: &str, folder: Option<&Path>, mut opts: Options, filters: &str, dry_run: bool) -> Result<Run, String> {
    let setup = ordercore::install::read(setup_name)?;
    let order = setup.plugins();
    let mut r = Run::default();
    let mut plugins: Vec<(String, PathBuf)> = Vec::new();
    for (name, path) in setup.locate(&order) {
        match path {
            Some(p) => plugins.push((name, p)),
            None => r.lines.push(format!("WARNING: {name} is in the load order but no data folder has it; it is left out.")),
        }
    }
    for l in filters.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';') && !l.starts_with('#')) {
        let parsed = l.rsplit_once('=').and_then(|(re, allow)| match allow.trim().to_lowercase().as_str() {
            "true" => Some((re.trim().trim_matches('"').to_string(), true)),
            "false" => Some((re.trim().trim_matches('"').to_string(), false)),
            _ => None,
        });
        match parsed {
            Some(f) => opts.filters.push(f),
            None => r.lines.push(format!("WARNING: filter {l:?} is not `regex = true` or `regex = false`; it is skipped.")),
        }
    }
    r.lines.push(format!("{} plugin(s) in the load order; record types: {}", plugins.len(), opts.types.join(", ")));
    let t = std::time::Instant::now();
    let out = merge(&plugins, &opts);
    r.lines.extend(out.lines.iter().cloned());
    let dir = match folder {
        Some(f) => f.to_path_buf(),
        None => {
            let folders: Vec<PathBuf> = setup.data.iter().filter(|d| d.is_dir()).cloned().collect();
            landcore::service::home_folder(&folders, &order).ok_or("No data folder to write into: choose one.")?
        }
    };
    let path = dir.join(&opts.file_name);
    let n = out.records.len();
    let kinds = out.by_type.iter().map(|(t, n)| format!("{n} {t}")).collect::<Vec<_>>().join(", ");
    r.status = if n == 0 {
        "No merges were deemed necessary: every record two plugins change is already the last one's.".to_string()
    } else if dry_run {
        format!("Dry run: {n} records would be merged ({kinds}) into {}; nothing written.", path.display())
    } else {
        let sizes = plugins.iter().map(|(n, p)| (n.clone(), std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))).collect();
        let description = format!("Merged objects made by Wraithguard Toolkit (TES3Merge's rules) from {} plugins.", plugins.len());
        let bytes = plugin_bytes(&out, &sizes, &description)?;
        write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        r.written = Some(path.clone());
        let listed = order.iter().any(|p| p.eq_ignore_ascii_case(&opts.file_name));
        format!(
            "Wrote {n} merged records ({kinds}) to {} in {:.1} s.{}",
            path.display(),
            t.elapsed().as_secs_f64(),
            if listed { "" } else { " Add it to the load order, last." }
        )
    };
    if !out.unreadable.is_empty() {
        r.lines.push(format!(
            "{} plugin(s) could not be read and were left out: {}",
            out.unreadable.len(),
            out.unreadable.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    r.outcome = out;
    Ok(r)
}

/// TES3Merge's `multipatch`: only the leveled lists merged (and the patches on creatures and
/// cells), into `multipatch.esp`.
pub fn lists_only(mut opts: Options) -> Options {
    opts.types = ["LEVI", "LEVC", "CREA", "CELL"].iter().map(|s| (*s).to_string()).collect();
    opts.lists_only = true;
    opts.file_name = "multipatch.esp".into();
    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn opts() -> Options {
        Options::default()
    }

    #[test]
    fn later_changes_fill_in_what_the_last_left_as_the_base() {
        // Base, a weight change, a value change, a name change last: all three kept.
        let base = json!({"type":"MiscItem","flags":"","id":"cup","name":"Cup","data":{"weight":1.0,"value":5,"flags":""}});
        let a = json!({"type":"MiscItem","flags":"","id":"cup","name":"Cup","data":{"weight":2.0,"value":5,"flags":""}});
        let b = json!({"type":"MiscItem","flags":"","id":"cup","name":"Cup","data":{"weight":1.0,"value":9,"flags":""}});
        let mut cur = json!({"type":"MiscItem","flags":"","id":"cup","name":"Fine Cup","data":{"weight":1.0,"value":5,"flags":""}});
        for v in [&b, &a] {
            merge_record("MISC", &mut cur, &base, v, &opts());
        }
        assert_eq!(cur["name"], "Fine Cup");
        assert_eq!(cur["data"]["weight"], 2.0);
        assert_eq!(cur["data"]["value"], 9);
        // The later of two changes to one field wins (they are taken latest first).
        let c = json!({"type":"MiscItem","flags":"","id":"cup","name":"Cup","data":{"weight":3.0,"value":5,"flags":""}});
        let mut cur = base.clone();
        for v in [&c, &a] {
            merge_record("MISC", &mut cur, &base, v, &opts());
        }
        assert_eq!(cur["data"]["weight"], 3.0);
    }

    #[test]
    fn leveled_lists_are_the_union_sorted_by_level_then_id() {
        let l = |items: Value| json!({"type":"LeveledItem","flags":"","id":"l","leveled_item_flags":"","chance_none":0,"items":items});
        let base = l(json!([["b", 1], ["a", 5]]));
        let mid = l(json!([["b", 1], ["a", 5], ["c", 2], ["c", 2]]));
        let mut cur = l(json!([["b", 1], ["d", 3]]));
        merge_record("LEVI", &mut cur, &base, &mid, &opts());
        assert_eq!(cur["items"], json!([["b", 1], ["c", 2], ["c", 2], ["d", 3], ["a", 5]]));
    }

    #[test]
    fn effects_slot_by_slot_and_patches() {
        let e = |m: &str, mag: i64| json!({"magic_effect":m,"skill":"None","attribute":"None","range":"OnSelf","area":0,"duration":1,"min_magnitude":mag,"max_magnitude":mag});
        let s = |fx: Value| json!({"type":"Spell","flags":"","id":"s","name":"S","effects":fx,"data":{"spell_type":"Spell","cost":1,"flags":""}});
        let base = s(json!([e("FortifyHealth", 5)]));
        let mid = s(json!([e("FortifyHealth", 10), e("Light", 1)]));
        let mut cur = s(json!([e("FortifyHealth", 5)]));
        merge_record("SPEL", &mut cur, &base, &mid, &opts());
        assert_eq!(cur["effects"], json!([e("FortifyHealth", 10), e("Light", 1)]));
        let mut scamp = json!({"type":"Creature","flags":"","id":"Scamp_Summon"});
        assert!(patch_record("CREA", &mut scamp, "Scamp_Summon", &opts()));
        assert_eq!(scamp["flags"], "PERSISTENT");
        let mut cell = json!({"type":"Cell","flags":"","name":"Dark","data":{"flags":"IS_INTERIOR","grid":[0,0]},"atmosphere_data":{"fog_density":0.0}});
        assert!(patch_record("CELL", &mut cell, "Dark", &opts()));
        assert!(cell["atmosphere_data"]["fog_density"].as_f64().unwrap() > 0.0);
    }
}
