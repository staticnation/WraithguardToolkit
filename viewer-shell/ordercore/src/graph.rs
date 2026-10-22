//! The sort's graph, as `wraithguard/sort/graph.py` and the first half of
//! `wraithguard/sort/engine.py` build it: which plugins are masters, whether an edge would
//! close a cycle, and the ordering edges from the curated order, the customs' header masters
//! and the rules.
//!
//! The graph holds three kinds of edge, added in this order, and an edge that would close a
//! cycle is **refused and remembered**, never added - so what was added first wins:
//!
//! 1. the curated plugins in their cfg order, each before the next. A custom already in the cfg
//!    is bridged over (it is not chained in place, so it can be moved);
//! 2. every header master a custom lists, before that custom (when that master is installed);
//! 3. the `[Order]` chains of the rules, highest priority first (a later file wins a
//!    contradiction, because its edges come first), and only an edge that touches a custom.
//!    A name in a chain that matches nothing installed is dropped, so its neighbours are
//!    joined: `A, Gone, X` gives `A -> X`.
//!
//! What is not here: the position each custom is given, the `[NearStart]`/`[NearEnd]` hints and
//! the tie-break among unrelated plugins (the sort's order is slice 2d), and the report of what
//! was refused (the caller words it; [`unique_conflicts`] gives it each once).
//!
//! What is not read is **declined**, as in [`crate::patterns`]: [`build_edges`] answers
//! [`Unsupported`] when a rule name is one [`expand`] declines, and the caller keeps Python's
//! answer.
//!
//! The Python is the reference and `tests/fixtures/graph_cases.json` pins both. Known and left:
//! when the curated order holds two spellings of one name, Python picks which one a master is
//! found as by set order (it differs from one process to the next); here the later one is
//! picked.
//!
//! Written from mlox's documented behaviour, not its code.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::patterns::expand;
pub use crate::patterns::Unsupported;

/// Edges as `{node: successors}`, in name order.
pub type Adjacency = BTreeMap<String, BTreeSet<String>>;

/// Whether a plugin is a master, which loads before the ordinary plugins (`is_master_file`):
/// its name ends in `.esm` or `.omwgame`, in any case.
pub fn is_master_file(name: &str) -> bool {
    let low = name.to_lowercase();
    low.ends_with(".esm") || low.ends_with(".omwgame")
}

/// Whether adding `start -> target` would close a cycle (`would_create_cycle`): `start` can
/// already be reached from `target`. A node to itself is a cycle.
pub fn would_create_cycle(adj: &Adjacency, start: &str, target: &str) -> bool {
    let mut stack: Vec<&str> = vec![target];
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(node) = stack.pop() {
        if node == start {
            return true;
        }
        if !seen.insert(node) {
            continue;
        }
        if let Some(next) = adj.get(node) {
            stack.extend(next.iter().map(String::as_str));
        }
    }
    false
}

/// The customs put on the curated order's spelling of their names, once each by name without
/// case (`build_and_sort`'s first step): OpenMW's file lookup ignores case, so `newmod.esp`
/// and `NewMod.esp` are one plugin and must be one node.
pub fn canonical_subset(base: &[String], subset: &[String]) -> Vec<String> {
    let by_lower: HashMap<String, &str> = base.iter().map(|n| (n.to_lowercase(), n.as_str())).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for n in subset {
        let canon = by_lower.get(&n.to_lowercase()).copied().unwrap_or(n.as_str());
        if seen.insert(canon.to_lowercase()) {
            out.push(canon.to_string());
        }
    }
    out
}

/// What [`build_edges`] built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edges {
    /// The customs as canonicalised ([`canonical_subset`]).
    pub subset: Vec<String>,
    /// The edges. Every plugin is a key, with no successors if it has none.
    pub adj: Adjacency,
    /// How many edges lead to each plugin; every plugin is a key.
    pub indegree: BTreeMap<String, usize>,
    /// The edges that were refused for closing a cycle, in the order they were refused
    /// (an edge refused twice is here twice; see [`unique_conflicts`]).
    pub conflicts: Vec<(String, String)>,
}

impl Edges {
    /// Every edge as `(before, after)`, in name order.
    pub fn edges(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::new();
        for (a, targets) in &self.adj {
            for b in targets {
                out.push((a.as_str(), b.as_str()));
            }
        }
        out
    }
}

struct Builder {
    adj: Adjacency,
    indegree: BTreeMap<String, usize>,
    conflicts: Vec<(String, String)>,
}

impl Builder {
    /// `a` before `b`, unless it is there already, is a self-edge, or closes a cycle (then it
    /// is remembered as refused).
    fn add_edge(&mut self, a: &str, b: &str) {
        if a == b || self.adj.get(a).is_some_and(|s| s.contains(b)) {
            return;
        }
        if would_create_cycle(&self.adj, a, b) {
            self.conflicts.push((a.to_string(), b.to_string()));
            return;
        }
        self.adj.entry(a.to_string()).or_default().insert(b.to_string());
        *self.indegree.entry(b.to_string()).or_insert(0) += 1;
    }
}

/// The ordering graph (`build_edges`, which is `build_and_sort`'s first half).
///
/// - `base`: the curated order, frozen.
/// - `subset`: the customs, in any spelling ([`canonical_subset`] reads them).
/// - `blocks`: the `[Order]` chains as `(names, priority)`; a rule file's index is its priority,
///   so a later file wins.
/// - `masters`: `{custom, in lower case: the masters its header lists}`.
///
/// # Errors
/// [`Unsupported`] when a name in a chain is one [`expand`] declines.
pub fn build_edges(
    base: &[String],
    subset: &[String],
    blocks: &[(Vec<String>, i64)],
    masters: &BTreeMap<String, Vec<String>>,
) -> Result<Edges, Unsupported> {
    let subset = canonical_subset(base, subset);
    let in_base: HashSet<&str> = base.iter().map(String::as_str).collect();
    let mut pool: Vec<String> = base.to_vec();
    pool.extend(subset.iter().filter(|n| !in_base.contains(n.as_str())).cloned());
    let subset_set: HashSet<&str> = subset.iter().map(String::as_str).collect();
    let node_lower: HashMap<String, &str> = pool.iter().map(|n| (n.to_lowercase(), n.as_str())).collect();

    let mut b = Builder {
        adj: pool.iter().map(|n| (n.clone(), BTreeSet::new())).collect(),
        indegree: pool.iter().map(|n| (n.clone(), 0)).collect(),
        conflicts: Vec::new(),
    };

    // 1) the curated plugins in their order; the customs in the cfg are bridged over.
    let frozen: Vec<&str> = base.iter().map(String::as_str).filter(|n| !subset_set.contains(n)).collect();
    for pair in frozen.windows(2) {
        b.add_edge(pair[0], pair[1]);
    }

    // 1b) each master a custom lists, before it.
    if !masters.is_empty() {
        for p in &subset {
            let Some(listed) = masters.get(&p.to_lowercase()) else { continue };
            for m in listed {
                if let Some(&mn) = node_lower.get(&m.to_lowercase()) {
                    if !mn.is_empty() && mn != p {
                        b.add_edge(mn, p);
                    }
                }
            }
        }
    }

    // 2) the rules, highest priority first (the sort is stable: equal priorities keep their
    //    order), and only an edge that touches a custom.
    let mut order: Vec<&(Vec<String>, i64)> = blocks.iter().collect();
    order.sort_by_key(|x| std::cmp::Reverse(x.1));
    for (names, _priority) in order {
        let mut survivors: Vec<Vec<&str>> = Vec::new();
        for tok in names {
            let matches = expand(tok, &pool)?;
            if !matches.is_empty() {
                survivors.push(matches);
            }
        }
        for pair in survivors.windows(2) {
            for a in &pair[0] {
                for c in &pair[1] {
                    if a == c {
                        continue;
                    }
                    if subset_set.contains(a) || subset_set.contains(c) {
                        b.add_edge(a, c);
                    }
                }
            }
        }
    }

    Ok(Edges { subset, adj: b.adj, indegree: b.indegree, conflicts: b.conflicts })
}

/// The refused edges once each, by name without case, in the order first seen
/// (`_unique_conflicts`): what the sort's report tells the user.
pub fn unique_conflicts(conflicts: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut out = Vec::new();
    for (a, b) in conflicts {
        if seen.insert((a.to_lowercase(), b.to_lowercase())) {
            out.push((a.clone(), b.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn pairs(e: &Edges) -> Vec<(&str, &str)> {
        e.edges()
    }

    #[test]
    fn masters_end_in_esm_or_omwgame_in_any_case() {
        assert!(is_master_file("Morrowind.ESM") && is_master_file("g.OmwGame"));
        assert!(!is_master_file("mod.esp") && !is_master_file("mod.esm.bak"));
    }

    #[test]
    fn a_node_to_itself_is_a_cycle_and_a_loop_elsewhere_is_not_followed() {
        let mut adj = Adjacency::new();
        adj.insert("X".into(), ["Y".to_string()].into());
        adj.insert("Y".into(), ["X".to_string()].into());
        assert!(would_create_cycle(&adj, "A", "A"));
        assert!(!would_create_cycle(&adj, "A", "X"));
        assert!(would_create_cycle(&adj, "Y", "X"));
    }

    #[test]
    fn customs_take_the_cfgs_spelling_once() {
        let base = names(&["A.esp", "Mod.esp"]);
        let got = canonical_subset(&base, &names(&["mod.ESP", "New.esp", "NEW.esp", "Mod.esp"]));
        assert_eq!(got, ["Mod.esp", "New.esp"]);
    }

    #[test]
    fn a_chain_is_bridged_over_what_is_not_installed() {
        let base = names(&["A.esp"]);
        let blocks = vec![(names(&["A.esp", "Gone.esp", "X.esp"]), 0)];
        let e = build_edges(&base, &names(&["X.esp"]), &blocks, &BTreeMap::new()).unwrap();
        assert_eq!(pairs(&e), [("A.esp", "X.esp")]);
        assert_eq!(e.indegree["X.esp"], 1);
    }

    #[test]
    fn the_later_file_wins_and_the_loser_is_remembered() {
        let base = names(&["A.esp"]);
        let blocks = vec![(names(&["X.esp", "Y.esp"]), 0), (names(&["Y.esp", "X.esp"]), 1)];
        let e = build_edges(&base, &names(&["X.esp", "Y.esp"]), &blocks, &BTreeMap::new()).unwrap();
        assert_eq!(pairs(&e), [("Y.esp", "X.esp")]);
        assert_eq!(e.conflicts, [("X.esp".to_string(), "Y.esp".to_string())]);
    }

    #[test]
    fn a_name_that_is_declined_declines_the_whole_graph() {
        let blocks = vec![(names(&["Mod [x]*.esp", "X.esp"]), 0)];
        let got = build_edges(&names(&["A.esp"]), &names(&["X.esp"]), &blocks, &BTreeMap::new());
        assert_eq!(got, Err(Unsupported));
    }

    #[test]
    fn a_refusal_made_twice_is_told_once() {
        let twice =
            vec![("X.esp".to_string(), "A.esp".to_string()), ("x.ESP".to_string(), "a.esp".to_string())];
        assert_eq!(unique_conflicts(&twice), [twice[0].clone()]);
    }
}
