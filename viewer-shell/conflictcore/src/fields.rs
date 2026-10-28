//! Field diff (port step 4.3, slice 3): one conflicting record's fields across the plugins
//! that define it (`wraithguard_toolkit.diff_record_fields`, `batch_record_fields` with the
//! native session), and what each field and plugin is doing to it
//! (`wraithguard.patch.status`, `conflict_all` / `conflict_this`, from yampt's
//! `record_conflict.cpp`, MIT, Rafał Wierzchoś; the names follow xEdit's).
//!
//! A record's fields are its tes3conv-schema form flattened to dotted keys, objects opened
//! and lists kept whole (`flatten_dict`, from TES3 Conflictsolver), in the order the record
//! writes them. Two values are the same when their JSON is (as the Python compares `repr`).

use std::collections::{HashMap, HashSet};
use std::io;

use serde_json::{Map, Value};

/// A record's key: `(type, id)` as the conflict scan keys it.
pub type Key = (String, String);

/// The records of a plugin's bytes whose keys are in `wanted`, in their tes3conv-schema form
/// (`record_subset`: the first record of each key).
///
/// Only the record types `wanted` names are decoded (a plugin's other records - the cells'
/// references, the landscape's vertices, the dialogue - are stepped over by their size, never
/// parsed or turned into JSON); with a type this reader does not know, every record is read as
/// before. A path grid is keyed by its cell's name, and whether that cell is an interior
/// decides the key, so for those the cells are read too, lightly (no references).
pub fn records_of(bytes: &[u8], wanted: &HashSet<Key>) -> io::Result<HashMap<Key, Value>> {
    let mut out = HashMap::new();
    if wanted.is_empty() {
        return Ok(out);
    }
    let mut tags: Vec<[u8; 4]> = Vec::new();
    let mut known = true;
    for (t, _) in wanted {
        match editcore::key::tag_of(t).map(|g| g.as_bytes()) {
            Some(g) if g.len() == 4 => {
                let g = [g[0], g[1], g[2], g[3]];
                if !tags.contains(&g) {
                    tags.push(g);
                }
            }
            _ => known = false,
        }
    }
    let to_values = |objs: Vec<tes3::esp::TES3Object>| -> io::Result<Vec<Value>> {
        objs.iter().map(|o| serde_json::to_value(o).map_err(io::Error::other)).collect()
    };
    let values: Vec<Value> = to_values(editcore::plugin::objects(bytes, if known { Some(tags.as_slice()) } else { None }, false)?)?;
    let mut interior = interior_cells(&values);
    if known && !tags.contains(b"CELL") && wanted.iter().any(|(t, _)| t == "PathGrid") {
        interior.extend(interior_cells(&to_values(editcore::plugin::objects(bytes, Some(&[*b"CELL"]), true)?)?));
    }
    for v in values {
        let Some(k) = editcore::plugin::conflict_key(&v, &interior) else { continue };
        if wanted.contains(&k) && !out.contains_key(&k) {
            out.insert(k, v);
        }
    }
    Ok(out)
}

/// The lower-cased names of the interior cells among `values` (`_interior_cell_names`).
fn interior_cells(values: &[Value]) -> HashSet<String> {
    let mut out = HashSet::new();
    for v in values {
        if !v.get("type").and_then(Value::as_str).is_some_and(|t| t.eq_ignore_ascii_case("cell")) {
            continue;
        }
        let inside = match v.get("data").and_then(|d| d.get("flags")) {
            Some(Value::Number(n)) => n.as_u64().is_some_and(|f| f & 1 != 0),
            Some(Value::Null) | None => false,
            Some(f) => f.to_string().to_uppercase().contains("INTERIOR"),
        };
        if inside {
            let name = v
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| v.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()));
            if let Some(n) = name {
                out.insert(n.to_lowercase());
            }
        }
    }
    out
}

/// A record flattened to dotted keys (`flatten_dict`): objects opened, everything else (lists
/// too) one value, in the record's order.
pub fn flatten(v: &Value) -> Vec<(String, Value)> {
    fn walk(m: &Map<String, Value>, parent: &str, out: &mut Vec<(String, Value)>) {
        for (k, v) in m {
            let key = if parent.is_empty() { k.clone() } else { format!("{parent}.{k}") };
            match v {
                Value::Object(inner) => walk(inner, &key, out),
                other => out.push((key, other.clone())),
            }
        }
    }
    let mut out = Vec::new();
    if let Value::Object(m) = v {
        walk(m, "", &mut out);
    }
    out
}

/// One record's fields across its plugins (`diff_record_fields`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldDiff {
    /// Every field, in the order the plugins first name it (load order).
    pub ordered: Vec<String>,
    /// Each plugin's fields (empty for a plugin whose record was not read).
    pub per: Vec<(String, Vec<(String, Value)>)>,
    /// The fields whose values differ, or that some plugin lacks.
    pub differing: HashSet<String>,
}

impl FieldDiff {
    /// One field's value in each plugin, in load order (`None`: the plugin lacks it).
    pub fn values(&self, field: &str) -> Vec<Option<&Value>> {
        self.per.iter().map(|(_, fs)| fs.iter().find(|(k, _)| k == field).map(|(_, v)| v)).collect()
    }
}

/// The diff of one record's versions, each plugin's in load order (`None`: not read).
pub fn diff(versions: &[(String, Option<&Value>)]) -> FieldDiff {
    let per: Vec<(String, Vec<(String, Value)>)> =
        versions.iter().map(|(p, v)| (p.clone(), v.map(flatten).unwrap_or_default())).collect();
    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    for (_, fields) in &per {
        for (k, _) in fields {
            if seen.insert(k.clone()) {
                ordered.push(k.clone());
            }
        }
    }
    let mut out = FieldDiff { ordered, per, differing: HashSet::new() };
    for k in &out.ordered {
        let vals = out.values(k);
        let present: Vec<&Value> = vals.iter().flatten().copied().collect();
        if present.len() != vals.len() || present.iter().any(|v| *v != present[0]) {
            out.differing.insert(k.clone());
        }
    }
    out
}

/// What is happening to a record or field across every file that defines it
/// (`ConflictAll`), ordered by how much attention it deserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConflictAll {
    Unknown,
    OnlyOne,
    NoConflict,
    OverrideBenign,
    Conflict,
}

impl ConflictAll {
    /// The Python enum's value.
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// What one file is doing to a record the others also touch (`ConflictThis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConflictThis {
    Unknown,
    Ignored,
    Master,
    IdenticalToMaster,
    OverrideWins,
    ConflictWins,
    ConflictLoses,
    Deleted,
}

impl ConflictThis {
    /// The Python enum's value.
    pub fn name(self) -> &'static str {
        match self {
            ConflictThis::Unknown => "unknown",
            ConflictThis::Ignored => "ignored",
            ConflictThis::Master => "master",
            ConflictThis::IdenticalToMaster => "identical to master",
            ConflictThis::OverrideWins => "override, wins",
            ConflictThis::ConflictWins => "conflict, wins",
            ConflictThis::ConflictLoses => "conflict, loses",
            ConflictThis::Deleted => "deleted",
        }
    }

    /// How much it matters, for rolling up (`THIS_SEVERITY`).
    pub fn severity(self) -> u8 {
        match self {
            ConflictThis::Unknown | ConflictThis::Deleted | ConflictThis::Ignored => 0,
            ConflictThis::IdenticalToMaster => 1,
            ConflictThis::Master => 2,
            ConflictThis::OverrideWins => 3,
            ConflictThis::ConflictWins => 4,
            ConflictThis::ConflictLoses => 5,
        }
    }
}

fn present<T>(values: &[Option<T>], skip_absent: bool) -> Vec<usize> {
    (0..values.len()).filter(|&i| !skip_absent || values[i].is_some()).collect()
}

fn nothing_is_lost<T: PartialEq>(values: &[Option<T>], part: &[usize]) -> bool {
    let first = &values[part[0]];
    let winner = &values[part[part.len() - 1]];
    !part.iter().any(|&i| values[i].is_some() && values[i] != *first && values[i] != *winner)
}

/// A record or field across every file defining it, in load order (`conflict_all`); `None`
/// is a file without it, and `skip_absent` leaves such files out.
pub fn conflict_all<T: PartialEq>(values: &[Option<T>], skip_absent: bool) -> ConflictAll {
    let part = present(values, skip_absent);
    if part.len() <= 1 {
        return ConflictAll::OnlyOne;
    }
    let first = &values[part[0]];
    if part[1..].iter().all(|&i| values[i] == *first) {
        return ConflictAll::NoConflict;
    }
    if nothing_is_lost(values, &part) { ConflictAll::OverrideBenign } else { ConflictAll::Conflict }
}

/// What each file is doing (`conflict_this`), one status per value.
pub fn conflict_this<T: PartialEq>(values: &[Option<T>], skip_absent: bool) -> Vec<ConflictThis> {
    let mut out = vec![ConflictThis::Unknown; values.len()];
    let part = present(values, skip_absent);
    let Some(&first_at) = part.first() else { return out };
    let last_at = part[part.len() - 1];
    out[first_at] = if values[first_at].is_none() { ConflictThis::Unknown } else { ConflictThis::Master };
    if part.len() == 1 {
        return out;
    }
    let first = &values[first_at];
    let benign = nothing_is_lost(values, &part);
    for &i in &part[1..] {
        out[i] = match &values[i] {
            None => {
                if first.is_none() {
                    ConflictThis::IdenticalToMaster
                } else {
                    ConflictThis::Unknown
                }
            }
            v if v == first => ConflictThis::IdenticalToMaster,
            _ if benign => ConflictThis::OverrideWins,
            _ if i == last_at => ConflictThis::ConflictWins,
            _ => ConflictThis::ConflictLoses,
        };
    }
    out
}

/// The worst of a group's record statuses (`worst_all`).
pub fn worst_all(statuses: impl IntoIterator<Item = ConflictAll>) -> ConflictAll {
    statuses.into_iter().max().unwrap_or(ConflictAll::OnlyOne)
}

/// The worst of a group's version statuses (`worst_this`): the first of the most severe.
pub fn worst_this(statuses: impl IntoIterator<Item = ConflictThis>) -> ConflictThis {
    let mut best: Option<ConflictThis> = None;
    for s in statuses {
        if best.is_none_or(|b| s.severity() > b.severity()) {
            best = Some(s);
        }
    }
    best.unwrap_or(ConflictThis::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn statuses_follow_the_master_and_the_winner() {
        let v = |xs: &[Option<i32>]| xs.to_vec();
        assert_eq!(conflict_all(&v(&[Some(1)]), false), ConflictAll::OnlyOne);
        assert_eq!(conflict_all(&v(&[Some(1), Some(1)]), false), ConflictAll::NoConflict);
        assert_eq!(conflict_all(&v(&[Some(1), Some(2), Some(2)]), false), ConflictAll::OverrideBenign);
        assert_eq!(conflict_all(&v(&[Some(1), Some(2), Some(3)]), false), ConflictAll::Conflict);
        assert_eq!(
            conflict_this(&v(&[Some(1), Some(2), Some(3)]), false),
            vec![ConflictThis::Master, ConflictThis::ConflictLoses, ConflictThis::ConflictWins]
        );
        assert_eq!(
            conflict_this(&v(&[Some(1), Some(1), Some(2)]), false),
            vec![ConflictThis::Master, ConflictThis::IdenticalToMaster, ConflictThis::OverrideWins]
        );
        assert_eq!(
            conflict_this(&v(&[None, Some(1), None]), true),
            vec![ConflictThis::Unknown, ConflictThis::Master, ConflictThis::Unknown]
        );
        assert_eq!(
            conflict_this(&v(&[None, None, Some(2)]), false),
            vec![ConflictThis::Unknown, ConflictThis::IdenticalToMaster, ConflictThis::OverrideWins]
        );
        assert_eq!(worst_this([ConflictThis::Master, ConflictThis::ConflictLoses, ConflictThis::ConflictWins]), ConflictThis::ConflictLoses);
        assert_eq!(worst_all([]), ConflictAll::OnlyOne);
    }

    #[test]
    fn a_diff_names_every_field_and_what_differs() {
        let a = json!({"type": "Npc", "id": "x", "data": {"level": 1, "gold": 5}, "spells": ["a"]});
        let b = json!({"type": "Npc", "id": "x", "data": {"level": 2, "gold": 5}});
        let d = diff(&[("A.esp".into(), Some(&a)), ("B.esp".into(), Some(&b)), ("C.esp".into(), None)]);
        assert_eq!(d.ordered, ["type", "id", "data.level", "data.gold", "spells"]);
        // C.esp lacks every field, so every field is "differing", as in the Python.
        assert_eq!(d.differing.len(), 5);
        let d2 = diff(&[("A.esp".into(), Some(&a)), ("B.esp".into(), Some(&b))]);
        let mut got: Vec<&String> = d2.differing.iter().collect();
        got.sort();
        assert_eq!(got, ["data.level", "spells"]);
        assert_eq!(d2.values("data.gold"), [Some(&json!(5)), Some(&json!(5))]);
    }
}
