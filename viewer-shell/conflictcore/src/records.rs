//! Record conflicts (port step 4.3, slices 1, 2 and 4): every record the plugins of a load
//! order define, keyed `(type, id)` as the conflict viewer keys them
//! (`editcore::plugin::keys_and_cells`), and which plugins touch it, in load order - a plugin
//! that defines a record twice counted once. Two or more is a conflict (the last plugin's
//! version wins); exactly one is a single.
//!
//! As `wraithguard_toolkit.detect_conflicts` with the native session (the engine that ships):
//! a plugin whose file is not found is unreadable; one found but not read gives no keys. The
//! tes3conv session is the Python's alone.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use ordercore::inputs::PluginIndex;

use crate::vfs::par_map;

/// One record in the touch table: its key, and every plugin that defines it with whether
/// that plugin deletes it, in load order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Touched {
    pub rtype: String,
    pub id: String,
    pub hits: Vec<(String, bool)>,
}

/// What reading the load order gave.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    /// The records in the order first met.
    pub touched: Vec<Touched>,
    /// Plugins whose files were not found.
    pub unreadable: Vec<String>,
    /// How many plugins were found.
    pub scanned: usize,
    /// How many records (a plugin's repeats counted once).
    pub records: usize,
    /// Each found plugin's file.
    pub paths: Vec<(String, String)>,
}

/// A plugin's keys, and whether its repeats count towards [`Scan::records`].
pub type Keys = (Vec<(String, String, bool)>, bool);

/// An `.omwscripts` file's keys (`parse_omwscripts`): `("LuaScript", path)` for each `TAGS:
/// script.lua` line, the path with `/` between names, lower case, no leading `/`.
pub fn omwscripts_keys(bytes: &[u8]) -> Vec<(String, String, bool)> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for raw in luacore::omwscripts::py_splitlines(&text) {
        let line = luacore::omwscripts::py_strip(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(pos) = line.find(':') else { continue };
        let path = luacore::omwscripts::py_strip(&line[pos + 1..]).trim_matches('"').trim_matches('\'');
        let lowered = path.to_lowercase();
        if !lowered.ends_with(".lua") {
            continue;
        }
        out.push(("LuaScript".to_string(), lowered.replace('\\', "/").trim_start_matches('/').to_string(), false));
    }
    out
}

/// A plugin's keys as the native session reads them (`_scan_touch` with `NativeEspSession`):
/// a TES3 file's `(type, id, deleted)` keys, a record it defines twice counted once; an
/// `.omwscripts` file's declarations, each counted. Empty when it cannot be read.
pub fn plugin_keys(path: &std::path::Path) -> Keys {
    let Ok(bytes) = fs::read(path) else { return (Vec::new(), true) };
    if path.to_string_lossy().to_lowercase().ends_with(".omwscripts") {
        return (omwscripts_keys(&bytes), true);
    }
    (editcore::plugin::keys_and_cells(&bytes).map(|(k, _)| k).unwrap_or_default(), false)
}

/// A record id as the engines compare it: without case (ASCII; Morrowind.exe and OpenMW
/// both fold ASCII only). `Iron_Dagger` in one plugin and `iron_dagger` in another are one
/// record, and a conflict.
pub fn fold_id(id: &str) -> String {
    id.to_ascii_lowercase()
}

/// A plugin's file by name (`PluginFileIndex.find`); `None`: not found.
pub type Find<'a> = dyn Fn(&str) -> Option<PathBuf> + Sync + 'a;

/// [`Find`] over an [`PluginIndex`].
pub fn in_index(index: &PluginIndex) -> impl Fn(&str) -> Option<PathBuf> + Sync + '_ {
    move |name| index.find(name).map(std::path::Path::to_path_buf)
}

/// Read every plugin of `order` (side by side) and fold their keys in load order
/// (`_scan_touch`). `find` locates a plugin's file and `keys` reads it.
pub fn scan_touch(order: &[String], find: &Find<'_>, keys: &(dyn Fn(&std::path::Path) -> Keys + Sync)) -> Scan {
    let read: Vec<Option<(PathBuf, Keys)>> = par_map(order, |p| {
        let path = find(p)?;
        let k = keys(&path);
        Some((path, k))
    });
    let mut scan = Scan::default();
    let mut at: HashMap<(String, String), usize> = HashMap::new();
    for (plugin, got) in order.iter().zip(read) {
        let Some((path, (keys, repeats))) = got else {
            scan.unreadable.push(plugin.clone());
            continue;
        };
        scan.scanned += 1;
        scan.paths.push((plugin.clone(), path.to_string_lossy().into_owned()));
        let mut seen_here: HashSet<(String, String)> = HashSet::new();
        for (rtype, id, deleted) in keys {
            // The record is the same whatever the case its plugin spells the id in; the
            // first spelling met is the one shown.
            let key = (rtype, fold_id(&id));
            if repeats {
                scan.records += 1;
            }
            if !seen_here.insert(key.clone()) {
                continue;
            }
            if !repeats {
                scan.records += 1;
            }
            match at.get(&key) {
                Some(&i) => scan.touched[i].hits.push((plugin.clone(), deleted)),
                None => {
                    at.insert(key.clone(), scan.touched.len());
                    scan.touched.push(Touched { rtype: key.0, id, hits: vec![(plugin.clone(), deleted)] });
                }
            }
        }
    }
    scan
}

/// One conflict (or single): the record, its plugins in load order, the one that wins, whether
/// any is one of the user's own, and those that delete it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub rtype: String,
    pub id: String,
    pub plugins: Vec<String>,
    pub winner: String,
    pub involves_subset: bool,
    pub deleted_by: Vec<String>,
}

/// The scan's counts, as the Python's `stats` (`conflicts` or `singles` in `found`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub scanned: usize,
    pub unreadable: Vec<String>,
    pub records: usize,
    pub found: usize,
    pub paths: Vec<(String, String)>,
}

fn stats(scan: &Scan, found: usize) -> Stats {
    Stats { scanned: scan.scanned, unreadable: scan.unreadable.clone(), records: scan.records, found, paths: scan.paths.clone() }
}

/// Python's `str.lower()` as far as an id needs it.
fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// The records two or more plugins define (`detect_conflicts`), the user's own first, then by
/// type and id (without case).
pub fn conflicts(scan: &Scan, subset: &[String]) -> (Vec<Conflict>, Stats) {
    let mine: HashSet<String> = subset.iter().map(|s| lower(s)).collect();
    let mut out: Vec<Conflict> = scan
        .touched
        .iter()
        .filter(|t| t.hits.len() >= 2)
        .map(|t| {
            let plugins: Vec<String> = t.hits.iter().map(|(p, _)| p.clone()).collect();
            Conflict {
                rtype: t.rtype.clone(),
                id: t.id.clone(),
                winner: plugins.last().cloned().unwrap_or_default(),
                involves_subset: plugins.iter().any(|p| mine.contains(&lower(p))),
                deleted_by: t.hits.iter().filter(|(_, d)| *d).map(|(p, _)| p.clone()).collect(),
                plugins,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (!a.involves_subset, &a.rtype, lower(&a.id)).cmp(&(!b.involves_subset, &b.rtype, lower(&b.id)))
    });
    let n = out.len();
    (out, stats(scan, n))
}

/// The records exactly one plugin defines (`_list_singles`): the user's own (`mine`), or the
/// rest; by type and id.
pub fn singles(scan: &Scan, subset: &[String], mine_only: bool) -> (Vec<Conflict>, Stats) {
    let mine: HashSet<String> = subset.iter().map(|s| lower(s)).collect();
    let mut out: Vec<Conflict> = scan
        .touched
        .iter()
        .filter(|t| t.hits.len() == 1)
        .filter_map(|t| {
            let (plugin, deleted) = &t.hits[0];
            let is_mine = mine.contains(&lower(plugin));
            (is_mine == mine_only).then(|| Conflict {
                rtype: t.rtype.clone(),
                id: t.id.clone(),
                plugins: vec![plugin.clone()],
                winner: plugin.clone(),
                involves_subset: is_mine,
                deleted_by: if *deleted { vec![plugin.clone()] } else { Vec::new() },
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.rtype, lower(&a.id)).cmp(&(&b.rtype, lower(&b.id))));
    let n = out.len();
    (out, stats(scan, n))
}

/// The conflicts as the report reads (`format_conflict_report`): the user's own only with
/// `subset_only`, at most `limit` listed (0: all).
pub fn report(conflicts: &[Conflict], stats: &Stats, subset_only: bool, limit: usize) -> String {
    let shown: Vec<&Conflict> = conflicts.iter().filter(|c| c.involves_subset || !subset_only).collect();
    let n_sub = conflicts.iter().filter(|c| c.involves_subset).count();
    let mut lines = vec![format!(
        "Scanned {} plugin(s), {} record(s): {} conflicting record(s), {} involving your custom mods.",
        stats.scanned, stats.records, stats.found, n_sub
    )];
    if !stats.unreadable.is_empty() {
        let first: Vec<&str> = stats.unreadable.iter().take(8).map(String::as_str).collect();
        lines.push(format!(
            "NOTE: {} plugin(s) could not be read (not found on disk / unreadable): {}{}",
            stats.unreadable.len(),
            first.join(", "),
            if stats.unreadable.len() > 8 { " ..." } else { "" }
        ));
    }
    let capped: &[&Conflict] = if limit == 0 { &shown } else { &shown[..shown.len().min(limit)] };
    for c in capped {
        let star = if c.involves_subset { "* " } else { "  " };
        lines.push(format!("{star}[{}] {}", c.rtype, c.id));
        lines.push(format!("      {}   (wins: {})", c.plugins.join("  ->  "), c.winner));
        if !c.deleted_by.is_empty() {
            lines.push(format!("      deleted by: {}", c.deleted_by.join(", ")));
        }
    }
    if limit != 0 && shown.len() > limit {
        lines.push(format!("  ... and {} more (raise the limit or save the full report).", shown.len() - limit));
    }
    if capped.is_empty() {
        lines.push("  No conflicts to show.".into());
    }
    lines.join("\n")
}

/// A CSV field as Python's `csv.writer` writes it (minimal quoting: a field holding the
/// delimiter, a quote, `\r` or `\n` is quoted, its quotes doubled).
pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// A CSV row with Python's `\r\n` ending.
pub fn csv_row(fields: &[&str]) -> String {
    let mut row = fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(",");
    row.push_str("\r\n");
    row
}

/// The conflicts as `write_conflict_csv` writes them.
pub fn conflict_csv(conflicts: &[Conflict]) -> String {
    let mut out = csv_row(&["record_type", "record_id", "winner", "involves_custom", "deleted_by", "plugins_load_order"]);
    for c in conflicts {
        out.push_str(&csv_row(&[
            &c.rtype,
            &c.id,
            &c.winner,
            if c.involves_subset { "yes" } else { "no" },
            &c.deleted_by.join("; "),
            &c.plugins.join(" -> "),
        ]));
    }
    out
}

/// `fnmatch` without regard to case, as Python's on POSIX: `*`, `?`, `[seq]`, `[!seq]`.
fn glob(name: &[char], pat: &[char]) -> bool {
    match pat.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|i| glob(&name[i..], rest)),
        Some(('?', rest)) => !name.is_empty() && glob(&name[1..], rest),
        Some(('[', rest)) => {
            let Some(close) = rest.iter().skip(1).position(|&c| c == ']').map(|i| i + 1) else {
                return name.first() == Some(&'[') && glob(&name[1..], rest);
            };
            let Some(&c) = name.first() else { return false };
            let (neg, set) = match rest[..close].split_first() {
                Some(('!', s)) => (true, s),
                _ => (false, &rest[..close]),
            };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    hit |= set[i] <= c && c <= set[i + 2];
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != neg && glob(&name[1..], &rest[close + 1..])
        }
        Some((p, rest)) => name.first() == Some(p) && glob(&name[1..], rest),
    }
}

/// The plugins kept and those left out by `patterns` (`filter_plugins`): a pattern matches a
/// name without regard to case as a glob, and one without `*` or `?` also anywhere inside it.
pub fn filter_plugins(order: &[String], patterns: &[String]) -> (Vec<String>, Vec<String>) {
    let pats: Vec<String> = patterns.iter().map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect();
    if pats.is_empty() {
        return (order.to_vec(), Vec::new());
    }
    let (mut kept, mut out) = (Vec::new(), Vec::new());
    for name in order {
        let low = name.to_lowercase();
        let chars: Vec<char> = low.chars().collect();
        let hit = pats.iter().any(|p| {
            let pc: Vec<char> = p.chars().collect();
            glob(&chars, &pc) || (!p.contains('*') && !p.contains('?') && low.contains(p.as_str()))
        });
        if hit { out.push(name.clone()) } else { kept.push(name.clone()) }
    }
    (kept, out)
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn patterns_are_globs_or_words() {
        let order: Vec<String> = ["Morrowind.esm", "Vurt Grass.esp", "Light Fix.esp", "Patch1.esp"].iter().map(|s| (*s).to_string()).collect();
        let (kept, out) = filter_plugins(&order, &["*grass*".into(), "LIGHT".into(), "patch[0-9].esp".into()]);
        assert_eq!(kept, ["Morrowind.esm"]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn ids_differing_only_in_case_are_one_record() {
        let order: Vec<String> = ["A.esp", "B.esp"].iter().map(|s| (*s).to_string()).collect();
        let find = |n: &str| Some(PathBuf::from(n));
        let keys = |p: &std::path::Path| -> Keys {
            match p.to_str().unwrap() {
                "A.esp" => (vec![("Weapon".into(), "Iron_Dagger".into(), false)], false),
                _ => (vec![("Weapon".into(), "iron_dagger".into(), false), ("Armor".into(), "Iron_Dagger".into(), false)], false),
            }
        };
        let scan = scan_touch(&order, &find, &keys);
        assert_eq!(scan.touched.len(), 2, "the weapon once (in two spellings), the armor once");
        let w = scan.touched.iter().find(|t| t.rtype == "Weapon").unwrap();
        assert_eq!(w.id, "Iron_Dagger", "the first spelling is the one shown");
        assert_eq!(w.hits.len(), 2);
        let (conflicts, _) = conflicts(&scan, &[]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].winner, "B.esp");
        // the type still tells records apart
        assert_eq!(scan.touched.iter().find(|t| t.rtype == "Armor").unwrap().hits.len(), 1);
    }
}
