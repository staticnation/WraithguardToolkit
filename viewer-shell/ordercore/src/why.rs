//! "Why is this plugin here?" (Static 2026-11-24, "lets do all this"): what put one plugin
//! where the load order has it, as the sort reads it (`graph` and `order`):
//!
//! - every rule block that names it: the rule file and the line its header is on, the names
//!   the block puts before it and after it that are installed, and which of those the order has
//!   on the wrong side (a rule not applied: another rule, the curated list or a cycle won);
//!   `[NearStart]` and `[NearEnd]` hints too;
//! - the masters its header lists, each with where it loads (a master after it is named);
//! - and from the caller, what the sort said: its place in the curated list or where it came
//!   from, the plugin it was anchored to, and the rules refused for it.
//!
//! Read-only: the rule files are read again here, so a block that names the plugin through a
//! wildcard (`Tamriel_Data*.esm`) or a `<VER>` is found as the sort found it ([`expand`]).
//!
//! Written from mlox's documented behaviour, not its code.

use std::collections::HashMap;
use std::path::Path;

use crate::patterns::expand;
use crate::rules::{blocks_with_lines, Kind};

/// One rule block that names the plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleHit {
    /// The rule file's name (not its folder).
    pub file: String,
    /// The line its header is on, from 1.
    pub line: usize,
    /// `order`, `nearstart` or `nearend`.
    pub kind: &'static str,
    /// The block's names, as written.
    pub names: Vec<String>,
    /// The installed plugins it puts before this one (an `[Order]` block), nearest first.
    pub before: Vec<String>,
    /// The installed plugins it puts after this one, nearest first.
    pub after: Vec<String>,
    /// Of `before` and `after`, those the order has on the other side.
    pub broken: Vec<String>,
}

/// One master the plugin's header lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterAt {
    /// Its name, as the header spells it.
    pub name: String,
    /// Its place in the order (from 0), or `None` when it is not in it.
    pub at: Option<usize>,
}

/// The rule blocks of `files` (`(name, text)` in the order the sort reads them) that name
/// `plugin`, measured against `order` (every plugin, in load order).
pub fn rule_hits(plugin: &str, order: &[String], files: &[(String, String)]) -> Vec<RuleHit> {
    let pos: HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    let me = plugin.to_lowercase();
    let mine = pos.get(&me).copied();
    let only = [plugin.to_string()];
    let names_of = |tok: &str| -> Vec<String> { expand(tok, order).unwrap_or_default().into_iter().map(str::to_string).collect() };
    let mut out = Vec::new();
    for (file, text) in files {
        for (block, line) in blocks_with_lines(text) {
            let hit = block.names.iter().position(|t| expand(t, &only).is_ok_and(|v| !v.is_empty()));
            let Some(at) = hit else { continue };
            let (mut before, mut after, mut broken) = (Vec::new(), Vec::new(), Vec::new());
            if block.kind == Kind::Order {
                let mut seen = std::collections::HashSet::new();
                for tok in block.names[..at].iter().rev() {
                    for n in names_of(tok) {
                        if n.to_lowercase() == me || !seen.insert(n.to_lowercase()) {
                            continue;
                        }
                        if let (Some(m), Some(&p)) = (mine, pos.get(&n.to_lowercase())) {
                            if p > m {
                                broken.push(n.clone());
                            }
                        }
                        before.push(n);
                    }
                }
                for tok in &block.names[at + 1..] {
                    for n in names_of(tok) {
                        if n.to_lowercase() == me || !seen.insert(n.to_lowercase()) {
                            continue;
                        }
                        if let (Some(m), Some(&p)) = (mine, pos.get(&n.to_lowercase())) {
                            if p < m {
                                broken.push(n.clone());
                            }
                        }
                        after.push(n);
                    }
                }
            }
            out.push(RuleHit { file: file.clone(), line, kind: block.kind.as_str(), names: block.names.clone(), before, after, broken });
        }
    }
    out
}

/// The masters the plugin file at `path` lists in its header, each with its place in `order`.
pub fn masters_at(path: &Path, order: &[String]) -> Vec<MasterAt> {
    let pos: HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    crate::inputs::read_masters(path).into_iter().map(|m| MasterAt { at: pos.get(&m.to_lowercase()).copied(), name: m }).collect()
}

/// The rule files of `entries` (files, or folders of `.txt` files, as the sort takes them) as
/// `(file name, text)`, in the order the sort reads them. A file that does not read is left out.
pub fn read_rule_files(entries: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for e in entries {
        for f in crate::plan::rule_files(e) {
            if let Ok(t) = crate::plan::rule_text(&f) {
                out.push((Path::new(&f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), t));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn the_rules_that_name_it() {
        let order = s(&["Morrowind.esm", "Tamriel_Data.esm", "A.esp", "B.esp", "C.esp"]);
        let text = "; a comment\n[Order]\nTamriel_Data*.esm\nB.esp\nGone.esp\nC.esp\n\n[Order]\nC.esp\nB.esp\n[NearEnd]\nB.esp\n[Order]\nA.esp\nC.esp\n";
        let hits = rule_hits("b.esp", &order, &[("mlox_base.txt".into(), text.into())]);
        assert_eq!(hits.len(), 3, "{hits:?}");
        assert_eq!((hits[0].line, hits[0].kind), (2, "order"));
        assert_eq!(hits[0].before, s(&["Tamriel_Data.esm"]));
        assert_eq!(hits[0].after, s(&["C.esp"]));
        assert!(hits[0].broken.is_empty());
        // C before B: the order has C after B, so that rule was not applied.
        assert_eq!((hits[1].line, hits[1].before.clone(), hits[1].broken.clone()), (8, s(&["C.esp"]), s(&["C.esp"])));
        assert_eq!((hits[2].line, hits[2].kind), (11, "nearend"));
    }
}
