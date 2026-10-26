//! MOMW's `plugin-order.yml` and what a load order is checked against it for, as
//! `wraithguard/momw.py` reads them.
//!
//! The file is a YAML list of entries, each with a `file_name`, perhaps a `for_mod`, a
//! `needs_cleaning` flag, the `on_lists` it belongs to, and some `depends`. It is read with a
//! YAML parser (`serde-saphyr`) into a typed entry, and **refused whole**, with the line and
//! column, on anything that is not that file: a syntax error, a duplicate key, a value of the
//! wrong kind (`on_lists: 5`, `needs_cleaning: maybe`), an entry that is not a mapping, or one
//! with no `file_name`. Keys it does not know (`depends` among them) are passed over, so a new
//! field from MOMW does not break a user. A wrong file is worse than none: a plugin misread as
//! off its list is treated as the user's own and reordered, which is what Wraithguard exists to
//! prevent. Update rules and Sources reads a download with this before it replaces the file,
//! so a broken download is refused and the old file kept.
//!
//! Before this the Python read the file with PyYAML when installed (which refuses only a
//! syntax error, and takes the last of two keys) and otherwise with a line reader that refused
//! nothing; they read 19 of the 34 test inputs differently. `tests/fixtures/momw_cases.json`
//! pins what this reads and what it refuses.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

/// One plugin of `plugin-order.yml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The plugin's file name, as the file spells it. Never empty.
    pub file_name: String,
    /// The mod it belongs to, when the entry says so.
    pub for_mod: Option<String>,
    /// The curated lists it is on, in file order.
    pub on_lists: Vec<String>,
    /// Whether the entry flags it for cleaning.
    pub needs_cleaning: bool,
}

/// An entry as the file holds it.
#[derive(Deserialize)]
struct Raw {
    file_name: Option<String>,
    #[serde(default)]
    for_mod: Option<String>,
    #[serde(default)]
    on_lists: Option<Vec<String>>,
    #[serde(default)]
    needs_cleaning: Option<bool>,
}

/// Why the file was refused, in words for the user (with the line and column when the YAML
/// says where).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// The parser's message as one line for a report: its first line, without the `error: ` in
/// front or the advice to a programmer after it (`, set DuplicateKeyPolicy ...`, `; use ...`).
fn said(message: &str) -> String {
    let first = message.lines().next().unwrap_or("").trim();
    let first = first.strip_prefix("error: ").unwrap_or(first);
    let cut = [", set ", "; use "].iter().filter_map(|m| first.find(m)).min().unwrap_or(first.len());
    first[..cut].to_string()
}

/// Read the entries out of `plugin-order.yml`'s text (see the module).
///
/// # Errors
/// [`Refused`] for anything that is not the file: see the module.
pub fn parse_text(text: &str) -> Result<Vec<Entry>, Refused> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let raw: Option<Vec<Raw>> = serde_saphyr::from_str(text).map_err(|e| Refused(said(&e.to_string())))?;
    let mut out = Vec::new();
    for (i, r) in raw.unwrap_or_default().into_iter().enumerate() {
        let Some(file_name) = r.file_name.filter(|n| !n.is_empty()) else {
            return Err(Refused(format!("entry {} has no file_name", i + 1)));
        };
        out.push(Entry {
            file_name,
            for_mod: r.for_mod,
            on_lists: r.on_lists.unwrap_or_default(),
            needs_cleaning: r.needs_cleaning.unwrap_or(false),
        });
    }
    Ok(out)
}

/// [`parse_text`] over the file's bytes, which must be UTF-8.
///
/// # Errors
/// [`Refused`], as [`parse_text`], or for bytes that are not UTF-8.
pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<Entry>, Refused> {
    let text = std::str::from_utf8(bytes).map_err(|e| Refused(format!("not UTF-8 ({e})")))?;
    parse_text(text)
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
        let got = parse_text("- file_name: A.esp\n  depends:\n    - file_name: Dep.esp\n").unwrap();
        assert_eq!(names(&got), ["A.esp"]);
        assert!(parse_text("- for_mod: Only\n- file_name: A.esp\n").is_err());
        assert!(parse_text("- file_name: \"\"\n").is_err());
    }

    #[test]
    fn a_wrong_file_is_refused_whole() {
        assert!(parse_text("- file_name: A.esp\n  file_name: B.esp\n").is_err());
        assert!(parse_text("- file_name: A.esp\n  on_lists: 5\n").is_err());
        assert!(parse_text("- file_name: A.esp\n  needs_cleaning: maybe\n").is_err());
        assert!(parse_text("file_name: A.esp\n").is_err());
        assert!(parse_text("- file_name: \"A.esp\n").is_err());
        assert!(parse_bytes(b"- file_name: \xff.esp\n").is_err());
    }

    #[test]
    fn a_byte_order_mark_comments_and_flow_lists_are_yaml() {
        let got = parse_text("\u{feff}# x\n- file_name: A.esp # note\n  on_lists: [a, b]\n").unwrap();
        assert_eq!(names(&got), ["A.esp"]);
        assert_eq!(got[0].on_lists, ["a", "b"]);
        assert_eq!(parse_text("# only a comment\n").unwrap(), []);
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
