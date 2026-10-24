//! MOMW's `plugin-order.yml` and what a load order is checked against it for, as
//! `wraithguard/momw.py` reads them.
//!
//! The file is long and very regular: a list of entries, each with a `file_name`, perhaps a
//! `for_mod`, a `needs_cleaning` flag, the `on_lists` it belongs to, and some `depends`. The
//! Python reads it with PyYAML when that is installed and otherwise with a small line
//! reader, so a stdlib-only Python still works. This is the line reader, written out: this
//! crate takes no dependencies, and it is the path with no YAML library in it. On the file
//! MOMW ships the two read the same; on anything else they can differ (a duplicate key, a
//! flow list, a trailing comment, a byte-order mark; PyYAML also raises on a malformed
//! file, which the reader does not), and `tests/fixtures/momw_cases.json` names the inputs
//! on which they do.
//!
//! The Python is the reference and the case file pins both.
//!
//! Written from MOMW's file as it is, not from its code.

use std::collections::{HashMap, HashSet};

use crate::rules::{is_space, splitlines};

/// One plugin of `plugin-order.yml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The plugin's file name, as the file spells it. Never empty.
    pub file_name: String,
    /// The mod it belongs to, when the entry says so (the last `for_mod` line wins).
    pub for_mod: Option<String>,
    /// The curated lists it is on, in file order.
    pub on_lists: Vec<String>,
    /// Whether the entry flags it for cleaning (`true`, `yes` or `1`, in any case).
    pub needs_cleaning: bool,
}

/// An entry being read: it only becomes an [`Entry`] once it has a `file_name`.
#[derive(Default)]
struct Partial {
    file_name: Option<String>,
    for_mod: Option<String>,
    on_lists: Vec<String>,
    needs_cleaning: bool,
}

impl Partial {
    fn finish(self) -> Option<Entry> {
        let file_name = self.file_name.filter(|n| !n.is_empty())?;
        Some(Entry {
            file_name,
            for_mod: self.for_mod,
            on_lists: self.on_lists,
            needs_cleaning: self.needs_cleaning,
        })
    }
}

/// What the reader is inside of at two spaces' indent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A key it does not read (or none yet).
    Other,
    /// `on_lists:`, whose four-space `- item` lines it reads.
    OnLists,
    /// `depends:`, whose nested entries it must not mistake for entries of their own.
    Depends,
}

/// A value without its surrounding space and quotes: Python's
/// `v.strip().strip('"').strip("'")`, which takes every quote at either end.
fn unquote(v: &str) -> &str {
    v.trim_matches(is_space).trim_matches('"').trim_matches('\'')
}

/// `key: value` on one line, into the entry: the first `file_name` stands, the last
/// `for_mod` does, and the flag is true for `true`, `yes` and `1` in any case.
fn apply_kv(entry: &mut Partial, s: &str) {
    let Some((k, v)) = s.split_once(':') else {
        return;
    };
    let v = unquote(v);
    match k.trim_matches(is_space) {
        "file_name" if entry.file_name.is_none() => entry.file_name = Some(v.to_string()),
        "for_mod" => entry.for_mod = Some(v.to_string()),
        "needs_cleaning" => {
            entry.needs_cleaning = matches!(v.to_lowercase().as_str(), "true" | "yes" | "1");
        }
        _ => {}
    }
}

/// Read the entries out of `plugin-order.yml`'s text (`parse_plugin_order_text`).
///
/// An entry starts at a line that begins `- ` and is kept when it has a `file_name`. Keys
/// at two spaces' indent are read; `on_lists` items are the `- x` lines at four or more
/// spaces under it; everything else, `depends` and what is nested in it included, is
/// ignored. A byte-order mark in front of the first dash hides that entry.
pub fn parse_text(text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut cur: Option<Partial> = None;
    let mut mode = Mode::Other;
    for line in splitlines(text) {
        let stripped = line.trim_matches(is_space);
        if stripped.is_empty() || line.trim_start_matches(is_space).starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            entries.extend(cur.take().and_then(Partial::finish));
            let mut fresh = Partial::default();
            mode = Mode::Other;
            apply_kv(&mut fresh, rest);
            cur = Some(fresh);
            continue;
        }
        let Some(entry) = cur.as_mut() else {
            continue;
        };
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent == 2 && line.contains(':') {
            let key = stripped.split(':').next().unwrap_or("").trim_matches(is_space);
            mode = match key {
                "on_lists" => Mode::OnLists,
                "depends" => Mode::Depends,
                _ => Mode::Other,
            };
            if mode == Mode::Other {
                apply_kv(entry, stripped);
            }
        } else if indent >= 4 && mode == Mode::OnLists {
            if let Some(item) = stripped.strip_prefix("- ") {
                let item = unquote(item);
                if !item.is_empty() {
                    entry.on_lists.push(item.to_string());
                }
            }
        }
    }
    entries.extend(cur.and_then(Partial::finish));
    entries
}

/// The plugins on one curated list (`curated_for_list`): the lower-cased names for
/// membership tests, and the names as the file spells them in its order. The list is
/// matched without regard to case; an empty name or one nothing is on gives empty sets.
pub fn curated_for_list(entries: &[Entry], list_name: &str) -> (HashSet<String>, Vec<String>) {
    let wanted = list_name.to_lowercase();
    let mut set = HashSet::new();
    let mut order = Vec::new();
    if wanted.is_empty() {
        return (set, order);
    }
    for e in entries {
        if e.on_lists.iter().any(|n| n.to_lowercase() == wanted) {
            set.insert(e.file_name.to_lowercase());
            order.push(e.file_name.clone());
        }
    }
    (set, order)
}

/// The lower-cased names the file flags for cleaning (`needs_cleaning_set`).
pub fn needs_cleaning_set(entries: &[Entry]) -> HashSet<String> {
    entries.iter().filter(|e| e.needs_cleaning).map(|e| e.file_name.to_lowercase()).collect()
}

/// Whether the curated list accounts for a plugin of the cfg (`curated_covers`): it names it
/// outright, or the plugin is the umo download of a needs-cleaning list plugin, which
/// arrives with one `clean_` in front. Only one is stripped, and the rest must be both on the
/// list and flagged.
pub fn curated_covers(
    name: &str,
    curated_lower: &HashSet<String>,
    needs_cleaning_lower: &HashSet<String>,
) -> bool {
    let lowered = name.to_lowercase();
    if curated_lower.contains(&lowered) {
        return true;
    }
    lowered.strip_prefix("clean_").is_some_and(|base| {
        curated_lower.contains(base) && needs_cleaning_lower.contains(base)
    })
}

/// Check the cfg's curated plugins against the list's canonical order
/// (`base_order_matches_yml`). Only plugins in both are compared, so the user's own between
/// them do not count; one note is given for the first plugin that comes before its
/// predecessor. Read-only: it reports and never reorders.
pub fn base_order_matches_yml(base: &[String], curated_order: &[String]) -> Vec<String> {
    let mut rank: HashMap<String, i64> = HashMap::new();
    for (i, n) in curated_order.iter().enumerate() {
        rank.insert(n.to_lowercase(), i as i64);
    }
    let mut prev_rank = -1;
    let mut prev_name: &str = "";
    for name in base {
        let Some(&r) = rank.get(&name.to_lowercase()) else {
            continue;
        };
        if r < prev_rank {
            return vec![format!(
                "[LIST ORDER] '{name}' appears after '{prev_name}' in your cfg, but the \
                 curated list order has it BEFORE. Your base order may have drifted from the \
                 canonical list order (or a tool reordered it)."
            )];
        }
        prev_rank = r;
        prev_name = name.as_str();
    }
    Vec::new()
}

/// The notes for a finished sort (`post_sort_findings`): the plugins flagged for cleaning,
/// then the cfg's unmanaged ("orphan") ones, then the order drift. With no entries there is
/// nothing to check against.
///
/// `final_order` is the sorted order, or `None` (or empty) when no sort ran, when the cfg's
/// own order is checked instead; `curated_set` and `declared_lower` are lower-cased.
pub fn post_sort_findings(
    final_order: Option<&[String]>,
    base: &[String],
    entries: &[Entry],
    curated_set: &HashSet<String>,
    curated_order: &[String],
    declared_lower: &HashSet<String>,
    list_name: Option<&str>,
) -> Vec<String> {
    if entries.is_empty() {
        return Vec::new();
    }
    let active = match final_order {
        Some(order) if !order.is_empty() => order,
        _ => base,
    };
    let nc = needs_cleaning_set(entries);
    let mut out: Vec<String> = active
        .iter()
        .filter(|n| nc.contains(&n.to_lowercase()))
        .map(|n| {
            format!(
                "[NEEDS CLEANING] '{n}' should be cleaned with TES3CMD (flagged in \
                 plugin-order.yml)."
            )
        })
        .collect();
    if let Some(list) = list_name.filter(|l| !l.is_empty()) {
        if !curated_set.is_empty() {
            out.extend(
                base.iter()
                    .filter(|n| {
                        !curated_covers(n, curated_set, &nc)
                            && !declared_lower.contains(&n.to_lowercase())
                    })
                    .map(|n| {
                        format!(
                            "[ORPHAN] '{n}' is in your cfg but not on the '{list}' list and not \
                             in your customizations -- an unmanaged custom plugin (fine if \
                             intentional)."
                        )
                    }),
            );
            out.extend(base_order_matches_yml(base, curated_order));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|e| e.file_name.as_str()).collect()
    }

    #[test]
    fn an_entry_needs_a_name_and_depends_is_not_an_entry() {
        let got = parse_text(
            "- for_mod: Only\n- file_name: A.esp\n  depends:\n    - file_name: Dep.esp\n- file_name: \"\"\n",
        );
        assert_eq!(names(&got), ["A.esp"]);
    }

    #[test]
    fn quotes_and_space_come_off_a_value() {
        assert_eq!(unquote(" \"'x'\" "), "x");
        assert_eq!(unquote("He said \"hi\""), "He said \"hi");
    }

    #[test]
    fn the_first_file_name_and_the_last_for_mod_stand() {
        let got = parse_text("- file_name: A.esp\n  file_name: B.esp\n  for_mod: 1\n  for_mod: 2\n");
        assert_eq!(got[0].file_name, "A.esp");
        assert_eq!(got[0].for_mod.as_deref(), Some("2"));
    }

    #[test]
    fn a_byte_order_mark_hides_the_first_dash() {
        let got = parse_text("\u{feff}- file_name: A.esp\n- file_name: B.esp\n");
        assert_eq!(names(&got), ["B.esp"]);
    }

    #[test]
    fn one_clean_prefix_is_stripped() {
        let cur: HashSet<String> = ["foo.esp".to_string()].into();
        let nc: HashSet<String> = ["foo.esp".to_string()].into();
        assert!(curated_covers("Clean_Foo.esp", &cur, &nc));
        assert!(!curated_covers("clean_clean_foo.esp", &cur, &nc));
        assert!(!curated_covers("Clean_Foo.esp", &cur, &HashSet::new()));
    }

    #[test]
    fn drift_is_reported_once() {
        let order: Vec<String> = ["A", "B", "C"].iter().map(|s| s.to_string()).collect();
        let reversed: Vec<String> = ["C", "B", "A"].iter().map(|s| s.to_string()).collect();
        assert!(base_order_matches_yml(&order, &order).is_empty());
        assert_eq!(base_order_matches_yml(&reversed, &order).len(), 1);
    }
}
