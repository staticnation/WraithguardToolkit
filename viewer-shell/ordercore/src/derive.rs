//! Turning what the tool observed into rules worth proposing, as `wraithguard/rules/derive.py`
//! does. Two tiers, kept apart on purpose, because they deserve different trust:
//!
//! - **Facts.** A plugin's master list is in its own header. If `B.esp` masters `A.esp` the game
//!   will not load `B` without `A`, and `A` loads first: certainties read out of the file.
//! - **Candidates.** The conflict scanner knows which plugins write the same records, which is
//!   *evidence of a relationship* without saying which. A candidate carries its evidence,
//!   proposes the reading the current order implies, offers the alternatives, and waits for a
//!   person. A tool that presented both tiers the same way would invite rubber-stamping guesses,
//!   and a wrong rule in a load-order file is worse than none: a wrong answer that looks
//!   researched.
//!
//! Every proposal starts with its evidence in the rule's citation. That is a *starting point*,
//! not a citation: [`needs_citation`] reports when it has not been improved on.
//!
//! Insertion order is meaningful here (the scanner's and the header reader's), so the maps of
//! the Python arrive as ordered slices.
//!
//! Known and left: `lower()` is Rust's `to_lowercase`.
//!
//! Written from the module's own description of what it proposes, not from any rule base.

use std::collections::{HashMap, HashSet};

use crate::authoring::{all_of, Expr, Rule};
use crate::rules::is_space;

/// The base game's own files: a rule saying a mod requires `Morrowind.esm` is noise.
pub const BASE_GAME_MASTERS: [&str; 3] = ["morrowind.esm", "tribunal.esm", "bloodmoon.esm"];

/// Words that suggest a plugin exists to patch other plugins. Only ever used to raise a proposal
/// to a person: a name is not evidence, which is why anything resting on it is a candidate.
const PATCH_WORDS: [&str; 6] = ["patch", "compat", "compatibility", "fix", "fixes", "glue"];

/// How much a proposal can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    /// Read straight out of the plugin files.
    Fact,
    /// Inferred from observed behaviour.
    Candidate,
}

impl Confidence {
    /// The word the Python uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Fact => "fact",
            Confidence::Candidate => "candidate",
        }
    }
}

/// A rule the tool suggests, with everything needed to judge it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    /// The rule as it would be written.
    pub rule: Rule,
    /// Fact or candidate.
    pub confidence: Confidence,
    /// Why, in one line.
    pub reason: String,
    /// What was actually observed, in the terms it was observed in.
    pub evidence: String,
    /// Other readings of the same evidence, for a candidate.
    pub alternatives: Vec<Rule>,
}

impl Proposal {
    /// Whether this came from the files rather than from inference.
    pub fn is_fact(&self) -> bool {
        self.confidence == Confidence::Fact
    }
}

/// One record the conflict scanner reported, as far as this reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictRecord {
    /// The plugins in load order; `None` when the record's `plugins` was not a list or tuple. A
    /// bare string is one of those: it is iterable, and iterating it yields characters, which
    /// once produced ten proposals about single letters.
    pub plugins: Option<Vec<String>>,
    /// The record type; `"?"` when the record gave none.
    pub rectype: String,
}

/// Whether a proposal still lacks a real citation: the evidence is prefilled into the rule's
/// `ref_` so the field is never blank, but evidence is not a citation.
pub fn needs_citation(proposal: &Proposal) -> bool {
    proposal.rule.ref_.trim_matches(is_space) == proposal.evidence.trim_matches(is_space)
}

fn is_base_game(name: &str) -> bool {
    BASE_GAME_MASTERS.contains(&name.trim_matches(is_space).to_lowercase().as_str())
}

fn active_only<'a>(names: &'a [String], active_lower: &HashSet<String>) -> Vec<&'a String> {
    names.iter().filter(|n| active_lower.contains(&n.to_lowercase())).collect()
}

fn active_lower(active: &[String]) -> HashSet<String> {
    active.iter().map(|n| n.to_lowercase()).collect()
}

/// `[Order]` rules from master dependencies: a plugin loads after its masters, which is a
/// statement of fact. One proposal per active master of each plugin; base game masters excluded.
pub fn order_from_masters(masters_by_plugin: &[(String, Vec<String>)], active: &[String]) -> Vec<Proposal> {
    let active_lower = active_lower(active);
    let mut out = Vec::new();
    for (plugin, masters) in masters_by_plugin {
        let relevant: Vec<&String> = active_only(masters, &active_lower)
            .into_iter()
            .filter(|m| !is_base_game(m) && m.to_lowercase() != plugin.to_lowercase())
            .collect();
        if relevant.is_empty() {
            continue;
        }
        let names = relevant.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", ");
        let evidence = format!("{plugin} declares {names} as master(s) in its header");
        for master in relevant {
            out.push(Proposal {
                rule: Rule {
                    kind: "Order".into(),
                    plugins: vec![master.clone(), plugin.clone()],
                    ref_: evidence.clone(),
                    ..Rule::default()
                },
                confidence: Confidence::Fact,
                reason: format!("{plugin} masters {master}, so it must load after it"),
                evidence: evidence.clone(),
                alternatives: Vec::new(),
            });
        }
    }
    out
}

/// `[Requires]` rules from master dependencies: also a fact, the plugin names the master in its
/// own header. Several masters become one rule with an `[ALL ...]` consequent. The active list is
/// not read (the Python takes it and deletes it).
pub fn requires_from_masters(
    masters_by_plugin: &[(String, Vec<String>)],
    _active: &[String],
) -> Vec<Proposal> {
    let mut out = Vec::new();
    for (plugin, masters) in masters_by_plugin {
        let relevant: Vec<&String> = masters
            .iter()
            .filter(|m| !is_base_game(m) && m.to_lowercase() != plugin.to_lowercase())
            .collect();
        if relevant.is_empty() {
            continue;
        }
        let consequent = if relevant.len() == 1 {
            Expr::Plugin(relevant[0].clone())
        } else {
            all_of(relevant.iter().map(|m| Expr::Plugin((*m).clone())).collect())
        };
        let names = relevant.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", ");
        let evidence = format!("{plugin} declares {names} as master(s) in its header");
        out.push(Proposal {
            rule: Rule {
                kind: "Requires".into(),
                expressions: vec![Expr::Plugin(plugin.clone()), consequent],
                message: format!("{plugin} requires {names}"),
                ref_: evidence.clone(),
                ..Rule::default()
            },
            confidence: Confidence::Fact,
            reason: format!("{plugin} cannot load without {names}"),
            evidence,
            alternatives: Vec::new(),
        });
    }
    out
}

/// `[Patch]` rules for plugins that look like compatibility patches: half fact (it masters two
/// or more active mods, from its header), half guess (that makes it a *patch*, from its
/// filename), so a candidate.
pub fn patch_candidates(masters_by_plugin: &[(String, Vec<String>)], active: &[String]) -> Vec<Proposal> {
    let active_lower = active_lower(active);
    let mut out = Vec::new();
    for (plugin, masters) in masters_by_plugin {
        let stem = plugin.rsplit_once('.').map_or(plugin.as_str(), |(s, _)| s).to_lowercase();
        let spaced = stem.replace(['-', '_'], " ");
        let words: HashSet<&str> = spaced.split(is_space).filter(|w| !w.is_empty()).collect();
        if !PATCH_WORDS.iter().any(|w| words.contains(w)) {
            continue;
        }
        let originals: Vec<&String> = active_only(masters, &active_lower)
            .into_iter()
            .filter(|m| !is_base_game(m) && m.to_lowercase() != plugin.to_lowercase())
            .collect();
        if originals.len() < 2 {
            continue;
        }
        let listed = originals.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", ");
        let evidence = format!("{plugin} masters {listed}, and its name suggests a patch");
        out.push(Proposal {
            rule: Rule {
                kind: "Patch".into(),
                expressions: vec![
                    Expr::Plugin(plugin.clone()),
                    all_of(originals.iter().map(|o| Expr::Plugin((*o).clone())).collect()),
                ],
                message: format!(
                    "{plugin} patches {} together",
                    originals.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(" and ")
                ),
                ref_: evidence.clone(),
                ..Rule::default()
            },
            confidence: Confidence::Candidate,
            reason: "masters two or more active mods and is named like a patch".to_string(),
            evidence,
            alternatives: Vec::new(),
        });
    }
    out
}

/// Conflicting records per pair of plugins, by record type, in the order first seen.
type PairCounts = Vec<((String, String), Vec<(String, usize)>)>;

/// Conflicting records per pair of plugins, by record type: `((earlier, later), [(type, count)])`
/// in the order first seen, each pair keyed in the order the scanner listed the plugins (load
/// order), so the key carries the direction.
fn conflict_pairs(conflicts: &[ConflictRecord]) -> PairCounts {
    let mut pairs: PairCounts = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    for conflict in conflicts {
        let Some(names) = &conflict.plugins else { continue };
        for (i, earlier) in names.iter().enumerate() {
            for later in &names[i + 1..] {
                let key = (earlier.clone(), later.clone());
                let at = *index.entry(key.clone()).or_insert_with(|| {
                    pairs.push((key, Vec::new()));
                    pairs.len() - 1
                });
                let counts = &mut pairs[at].1;
                match counts.iter_mut().find(|(t, _)| *t == conflict.rectype) {
                    Some(c) => c.1 += 1,
                    None => counts.push((conflict.rectype.clone(), 1)),
                }
            }
        }
    }
    pairs
}

/// `[Order]` rules for plugins that edit the same records. That is evidence of a relationship,
/// not of *which*: the proposal states the current order and carries the reverse (and a
/// `[Conflict]`) as alternatives, so accepting is a decision rather than a default. Pairs with
/// fewer than `minimum_records` records are ignored; the busiest pair comes first.
pub fn order_candidates_from_conflicts(conflicts: &[ConflictRecord], minimum_records: i64) -> Vec<Proposal> {
    let mut out: Vec<(usize, Proposal)> = Vec::new();
    for ((earlier, later), types) in conflict_pairs(conflicts) {
        let total: usize = types.iter().map(|t| t.1).sum();
        if (total as i64) < minimum_records {
            continue;
        }
        // Counter.most_common(): by count, highest first, ties in the order first seen
        let mut by_count = types.clone();
        by_count.sort_by_key(|a| std::cmp::Reverse(a.1));
        let breakdown =
            by_count.iter().map(|(name, count)| format!("{count} {name}")).collect::<Vec<_>>().join(", ");
        let evidence = format!("{total} record(s) edited by both {earlier} and {later}: {breakdown}");
        out.push((
            total,
            Proposal {
                rule: Rule {
                    kind: "Order".into(),
                    plugins: vec![earlier.clone(), later.clone()],
                    comment: format!("{later} currently wins these records"),
                    ref_: evidence.clone(),
                    ..Rule::default()
                },
                confidence: Confidence::Candidate,
                reason: format!("{earlier} and {later} edit {total} of the same record(s); whichever loads later wins them"),
                evidence: evidence.clone(),
                alternatives: vec![
                    Rule {
                        kind: "Order".into(),
                        plugins: vec![later.clone(), earlier.clone()],
                        comment: format!("{earlier} would win these records instead"),
                        ref_: evidence.clone(),
                        ..Rule::default()
                    },
                    Rule {
                        kind: "Conflict".into(),
                        expressions: vec![Expr::Plugin(earlier.clone()), Expr::Plugin(later.clone())],
                        message: format!("{earlier} and {later} edit the same records"),
                        ref_: evidence,
                        ..Rule::default()
                    },
                ],
            },
        ));
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.0));
    out.into_iter().map(|(_, p)| p).collect()
}

/// Every proposal, facts first (the reader should meet what is certainly true before what needs
/// their judgement), then candidates.
pub fn propose_all(
    masters_by_plugin: &[(String, Vec<String>)],
    active: &[String],
    conflicts: &[ConflictRecord],
    minimum_records: i64,
) -> Vec<Proposal> {
    let mut all = order_from_masters(masters_by_plugin, active);
    all.extend(requires_from_masters(masters_by_plugin, active));
    all.extend(patch_candidates(masters_by_plugin, active));
    all.extend(order_candidates_from_conflicts(conflicts, minimum_records));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(list: &[&str]) -> Vec<String> {
        list.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn a_master_is_a_fact_and_the_base_game_is_noise() {
        let masters = vec![("B.esp".to_string(), s(&["Morrowind.esm", "A.esp", "Gone.esp", "b.ESP"]))];
        let active = s(&["Morrowind.esm", "A.esp", "B.esp"]);
        let got = order_from_masters(&masters, &active);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].rule.plugins, ["A.esp", "B.esp"]);
        assert!(got[0].is_fact());
        let req = requires_from_masters(&masters, &active);
        assert_eq!(req[0].reason, "B.esp cannot load without A.esp, Gone.esp");
    }

    #[test]
    fn a_patch_needs_a_patch_word_and_two_active_masters() {
        let masters = vec![
            ("A_B-Compat Patch.esp".to_string(), s(&["A.esp", "B.esp"])),
            ("Plain.esp".to_string(), s(&["A.esp", "B.esp"])),
        ];
        let got = patch_candidates(&masters, &s(&["A.esp", "B.esp"]));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].confidence, Confidence::Candidate);
    }

    #[test]
    fn the_busiest_pair_comes_first_and_a_bare_string_is_skipped() {
        let rec = |plugins: Option<&[&str]>, t: &str| ConflictRecord {
            plugins: plugins.map(s),
            rectype: t.to_string(),
        };
        let conflicts = vec![
            rec(Some(&["A.esp", "B.esp"]), "NPC_"),
            rec(Some(&["A.esp", "C.esp"]), "CELL"),
            rec(Some(&["A.esp", "C.esp"]), "CELL"),
            rec(None, "X"),
        ];
        let got = order_candidates_from_conflicts(&conflicts, 1);
        assert_eq!(got[0].evidence, "2 record(s) edited by both A.esp and C.esp: 2 CELL");
        assert_eq!(got.len(), 2);
        assert!(order_candidates_from_conflicts(&conflicts, 2).len() == 1);
    }

    #[test]
    fn evidence_is_not_a_citation_until_it_is_replaced() {
        let mut p =
            order_from_masters(&[("B.esp".to_string(), s(&["A.esp"]))], &s(&["A.esp", "B.esp"])).remove(0);
        assert!(needs_citation(&p));
        p.rule.ref_ = "the readme".to_string();
        assert!(!needs_citation(&p));
    }
}
