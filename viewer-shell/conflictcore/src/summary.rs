//! The plugin view (`wraithguard.patch.summary`, `wraithguard.patch.bulk`): a scan regrouped
//! as plugin -> record type -> records, each record judged for one plugin, and a plugin's
//! fields taken across every record it defines.
//!
//! The flat conflict list answers "what conflicts"; this answers "what does *this mod*
//! touch", the question asked while deciding what to do about a mod.

use std::collections::HashMap;

use serde_json::Value;

use crate::fields::{conflict_all, conflict_this, diff, worst_all, worst_this, ConflictAll, ConflictThis};
use crate::records::Conflict;

/// One plugin's part of a scan (`Branch`): record type to the `(type, id)` it defines, in
/// the order the scan reported them, types in first-seen order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Branch {
    pub plugin: String,
    pub groups: Vec<(String, Vec<(String, String)>)>,
}

impl Branch {
    /// How many records this plugin takes part in.
    pub fn records(&self) -> usize {
        self.groups.iter().map(|(_, m)| m.len()).sum()
    }
}

/// A flat conflict list as plugin -> type -> records (`group_by_plugin`), plugins in `order`
/// (case-insensitively); those not in it follow, by name, rather than being dropped - a scan
/// can outlive the order it was taken from.
pub fn group_by_plugin(conflicts: &[Conflict], order: &[String]) -> Vec<Branch> {
    let mut branches: Vec<Branch> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for c in conflicts {
        for plugin in &c.plugins {
            let i = *at.entry(plugin.clone()).or_insert_with(|| {
                branches.push(Branch { plugin: plugin.clone(), groups: Vec::new() });
                branches.len() - 1
            });
            let b = &mut branches[i];
            match b.groups.iter_mut().find(|(t, _)| *t == c.rtype) {
                Some((_, m)) => m.push((c.rtype.clone(), c.id.clone())),
                None => b.groups.push((c.rtype.clone(), vec![(c.rtype.clone(), c.id.clone())])),
            }
        }
    }
    let ranked: HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    let fallback = ranked.len();
    branches.sort_by_cached_key(|b| {
        let low = b.plugin.to_lowercase();
        (ranked.get(&low).copied().unwrap_or(fallback), low)
    });
    branches
}

/// One record judged across its plugins (`field_statuses` rolled up by `record_status` and
/// `record_plugin_statuses`): the record's worst field, and each plugin's worst status
/// anywhere in it. `versions` in load order, `None` a plugin whose record was not read.
pub fn judge(versions: &[(String, Option<&Value>)]) -> (ConflictAll, Vec<ConflictThis>) {
    let d = diff(versions);
    let mut all = Vec::with_capacity(d.ordered.len());
    let mut per: Vec<Vec<ConflictThis>> = vec![Vec::with_capacity(d.ordered.len()); versions.len()];
    for name in &d.ordered {
        let values = d.values(name);
        all.push(conflict_all(&values, false));
        for (i, s) in conflict_this(&values, false).into_iter().enumerate() {
            per[i].push(s);
        }
    }
    (worst_all(all), per.into_iter().map(worst_this).collect())
}

/// How many records gave a plugin each status (`PluginTally`), by status name.
pub fn tally(statuses: impl IntoIterator<Item = ConflictThis>, into: &mut Vec<(ConflictThis, usize)>) -> &mut Vec<(ConflictThis, usize)> {
    for s in statuses {
        match into.iter_mut().find(|(k, _)| *k == s) {
            Some((_, n)) => *n += 1,
            None => into.push((s, 1)),
        }
    }
    into
}

/// Fields that name *which record this is* (`IDENTITY_FIELDS`), never taken in bulk:
/// taking them would make a different record, not a patched one.
pub const IDENTITY_FIELDS: [&str; 4] = ["type", "id", "grid", "data.grid"];

/// One conflicting record with each plugin's flattened fields (`BulkRecord`): `plugins` in
/// load order, the winner last; `values` per plugin, absent for a plugin not read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BulkRecord {
    pub rtype: String,
    pub key: String,
    pub plugins: Vec<String>,
    pub values: HashMap<String, Vec<(String, Value)>>,
}

/// One take to queue (`BulkFieldDecision`): `path` of `(rtype, key)` from the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Take {
    pub rtype: String,
    pub key: String,
    pub path: String,
}

fn field<'a>(fields: &'a [(String, Value)], path: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == path).map(|(_, v)| v)
}

/// The takes for a source plugin across `records` (`bulk_field_choices`): a record only when
/// the source defines it and is not already the winner, a field only when it is not an
/// identity field, the source has it, and its value differs from the winner's. In records
/// order, then `paths` order.
pub fn bulk_takes(records: &[BulkRecord], source: &str, paths: &[String]) -> Vec<Take> {
    let wanted: Vec<&String> = paths.iter().filter(|p| !IDENTITY_FIELDS.contains(&p.as_str())).collect();
    let mut out = Vec::new();
    for r in records {
        let Some(winner) = r.plugins.last() else { continue };
        if winner == source {
            continue;
        }
        let Some(src) = r.values.get(source) else { continue };
        let win = r.values.get(winner).map(Vec::as_slice).unwrap_or(&[]);
        for path in &wanted {
            let Some(v) = field(src, path) else { continue };
            if field(win, path) == Some(v) {
                continue;
            }
            out.push(Take { rtype: r.rtype.clone(), key: r.key.clone(), path: (*path).clone() });
        }
    }
    out
}

/// The fields the picker offers for a source (`_bulk_read_done`): every field the source
/// defines (in first-seen order) that taking would change somewhere, with how many records
/// it would change and the source's value from the first of them (`None`: no preview),
/// sorted by path case-insensitively.
pub fn bulk_candidates(records: &[BulkRecord], source: &str) -> Vec<(String, usize, Option<Value>)> {
    let mut paths: Vec<String> = Vec::new();
    for r in records {
        for (k, _) in r.values.get(source).into_iter().flatten() {
            if !paths.contains(k) {
                paths.push(k.clone());
            }
        }
    }
    let takes = bulk_takes(records, source, &paths);
    let mut out: Vec<(String, usize, Option<Value>)> = Vec::new();
    for t in &takes {
        match out.iter_mut().find(|(p, _, _)| *p == t.path) {
            Some((_, n, _)) => *n += 1,
            None => {
                let preview = records
                    .iter()
                    .find(|r| r.rtype == t.rtype && r.key == t.key)
                    .and_then(|r| r.values.get(source))
                    .and_then(|f| field(f, &t.path))
                    .cloned();
                out.push((t.path.clone(), 1, preview));
            }
        }
    }
    out.sort_by_key(|(p, _, _)| p.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn c(t: &str, id: &str, plugins: &[&str]) -> Conflict {
        Conflict {
            rtype: t.into(),
            id: id.into(),
            plugins: plugins.iter().map(|s| s.to_string()).collect(),
            winner: plugins.last().unwrap().to_string(),
            involves_subset: false,
            deleted_by: vec![],
        }
    }

    #[test]
    fn groups_by_plugin_in_load_order() {
        let list = [c("Weapon", "a", &["M.esm", "Z.esp"]), c("Npc", "b", &["M.esm", "x.esp"]), c("Weapon", "c", &["M.esm", "Z.esp"])];
        let order: Vec<String> = ["m.esm", "z.esp"].iter().map(|s| s.to_string()).collect();
        let got = group_by_plugin(&list, &order);
        let names: Vec<&str> = got.iter().map(|b| b.plugin.as_str()).collect();
        assert_eq!(names, ["M.esm", "Z.esp", "x.esp"]);
        assert_eq!(got[0].groups.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["Weapon", "Npc"]);
        assert_eq!(got[0].records(), 3);
        assert_eq!(got[1].groups[0].1, vec![("Weapon".to_string(), "a".to_string()), ("Weapon".to_string(), "c".to_string())]);
    }

    #[test]
    fn judges_a_record_per_plugin() {
        let m = json!({"id": "a", "data": {"weight": 1, "value": 5}});
        let x = json!({"id": "a", "data": {"weight": 2, "value": 5}});
        let z = json!({"id": "a", "data": {"weight": 1, "value": 9}});
        let v = [("M".to_string(), Some(&m)), ("X".to_string(), Some(&x)), ("Z".to_string(), Some(&z))];
        let (all, this) = judge(&v);
        assert_eq!(all, ConflictAll::Conflict);
        // Z restates the weight and alone changes the value: nothing of its own is lost.
        assert_eq!(this, vec![ConflictThis::Master, ConflictThis::ConflictLoses, ConflictThis::OverrideWins]);
        let same = [("M".to_string(), Some(&m)), ("X".to_string(), Some(&m))];
        assert_eq!(judge(&same), (ConflictAll::NoConflict, vec![ConflictThis::Master, ConflictThis::IdenticalToMaster]));
    }

    #[test]
    fn tallies() {
        let mut t = Vec::new();
        tally([ConflictThis::ConflictLoses, ConflictThis::Master, ConflictThis::ConflictLoses], &mut t);
        assert_eq!(t, vec![(ConflictThis::ConflictLoses, 2), (ConflictThis::Master, 1)]);
    }

    fn rec(key: &str, per: &[(&str, Value)]) -> BulkRecord {
        BulkRecord {
            rtype: "Creature".into(),
            key: key.into(),
            plugins: per.iter().map(|(p, _)| p.to_string()).collect(),
            values: per.iter().map(|(p, v)| (p.to_string(), crate::fields::flatten(v))).collect(),
        }
    }

    #[test]
    fn bulk_takes_only_what_changes_something() {
        let records = [
            rec("rat", &[("M", json!({"id": "rat", "hp": 5, "lvl": 1})), ("Fix", json!({"id": "rat", "hp": 9, "lvl": 1})), ("W", json!({"id": "rat", "hp": 5, "lvl": 2}))]),
            rec("dog", &[("M", json!({"id": "dog", "hp": 7})), ("Fix", json!({"id": "dog", "hp": 7})), ("W", json!({"id": "dog", "hp": 7}))]),
            rec("cat", &[("M", json!({"id": "cat", "hp": 1})), ("Fix", json!({"id": "cat", "hp": 3}))]),
            rec("bat", &[("M", json!({"id": "bat", "hp": 1})), ("W", json!({"id": "bat", "hp": 2}))]),
        ];
        let paths: Vec<String> = ["id", "hp", "lvl"].iter().map(|s| s.to_string()).collect();
        let got = bulk_takes(&records, "Fix", &paths);
        assert_eq!(got, vec![Take { rtype: "Creature".into(), key: "rat".into(), path: "hp".into() }, Take { rtype: "Creature".into(), key: "rat".into(), path: "lvl".into() }]);
        let cand = bulk_candidates(&records, "Fix");
        assert_eq!(cand, vec![("hp".to_string(), 1, Some(json!(9))), ("lvl".to_string(), 1, Some(json!(1)))]);
        assert!(bulk_takes(&records, "W", &paths).is_empty());
    }
}
