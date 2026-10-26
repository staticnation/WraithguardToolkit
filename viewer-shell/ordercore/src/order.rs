//! The sort's order, as `wraithguard/sort/engine.py` places the customs among the curated
//! plugins: the graph of [`crate::graph`], a position for every custom, the `[NearStart]` and
//! `[NearEnd]` hints, and a stable topological sort over them.
//!
//! The curated plugins are never reordered; the customs go where the graph and the rules say.
//!
//! - **Position.** A custom is put *right after* the latest-loading plugin it must come after
//!   (a curated one, or another custom resolved the same way). Master files are no signal: they
//!   sit at the top and half the list depends on them. With no such plugin it goes *right before*
//!   the earliest-loading plugin it must come before. With neither it keeps its cfg place (if it
//!   is in the cfg) or goes to the end, in the order it was declared. A neighbour whose position
//!   came from this custom's own is skipped (it cannot ground it), and the positions are settled
//!   again until they stop changing.
//! - **Hints.** `[NearStart]`/`[NearEnd]` names pull a custom to the front or the back, in the
//!   order they are listed (the first mention fixes the rank). A hint is a position and not an
//!   edge: a real edge still wins in the placement.
//! - **Placement.** Kahn's algorithm. Among the plugins that are ready: masters first, then
//!   position, then name without case, then name.
//!
//! [`build_and_sort`] would answer [`Unsupported`] for a rule or hint name [`expand`] declines;
//! since every character of a pattern but its wildcards is plain text, none is.
//!
//! Positions are `f64`, with the same arithmetic in the same order as the Python (`1e-6` nudges),
//! so they agree to the bit. Settling a custom's position recurses along its chain of customs,
//! as the Python does (which raises its recursion limit for it): a chain of many thousands of
//! customs needs a larger stack than a test thread's.
//!
//! The Python is the reference and `tests/fixtures/order_cases.json` pins both. Known and left:
//! see [`crate::graph`] for two spellings of one name in the curated order.
//!
//! Written from mlox's documented behaviour, not its code.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use crate::graph::{build_edges, is_master_file, Adjacency, Edges};
use crate::patterns::expand;
pub use crate::patterns::Unsupported;

/// The nudge that puts a custom just past (or just short of) the plugin it is anchored to.
const POSITION_EPSILON: f64 = 1e-6;

/// Why a custom sits where it does (`anchor_out`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// Right after its anchor.
    After,
    /// Right before its anchor.
    Before,
    /// No signal: its cfg place, or the end.
    None,
    /// Pulled to the front by `[NearStart]`.
    NearStart,
    /// Pulled to the back by `[NearEnd]`.
    NearEnd,
}

impl How {
    /// The word the Python and the case files use.
    pub fn as_str(self) -> &'static str {
        match self {
            How::After => "after",
            How::Before => "before",
            How::None => "none",
            How::NearStart => "nearstart",
            How::NearEnd => "nearend",
        }
    }
}

/// What [`build_and_sort`] gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sorted {
    /// Every plugin once, in load order.
    pub order: Vec<String>,
    /// For each custom (its name in lower case): why it sits where it does, and the plugin it
    /// is anchored to (none for no signal and for a hint).
    pub anchors: BTreeMap<String, (How, Option<String>)>,
    /// The rule edges refused for closing a cycle, as [`crate::graph::Edges::conflicts`].
    pub conflicts: Vec<(String, String)>,
}

/// Where each custom is put, step 3 of the sort: the customs' positions from their graph
/// neighbours, settled until they stop changing.
struct Resolver<'a> {
    preds: HashMap<String, Vec<String>>,
    succs: HashMap<String, Vec<String>>,
    base_index: &'a HashMap<&'a str, usize>,
    subset_set: &'a HashSet<&'a str>,
    declared_end: HashMap<String, usize>,
    /// custom -> (position, anchor, how)
    resolved: HashMap<String, (f64, Option<String>, How)>,
    /// custom -> the customs its position was derived from
    derives: HashMap<String, BTreeSet<String>>,
    /// In flight: the graph is acyclic, but the lookups of predecessors and successors interlock.
    resolving: HashSet<String>,
}

impl Resolver<'_> {
    fn no_signal_pos(&self, n: &str) -> f64 {
        match self.base_index.get(n) {
            Some(&i) => i as f64,
            None => self.declared_end.get(n).copied().unwrap_or(0) as f64,
        }
    }

    /// Whether `x`'s position was derived from `n`'s, transitively. Anchoring `n` against such
    /// a value would be circular.
    fn derives_from(&self, x: &str, n: &str) -> bool {
        let mut stack: Vec<&str> = vec![x];
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(c) = stack.pop() {
            if c == n {
                return true;
            }
            if !seen.insert(c) {
                continue;
            }
            if let Some(next) = self.derives.get(c) {
                stack.extend(next.iter().map(String::as_str));
            }
        }
        false
    }

    fn note_derived(&mut self, n: &str, from: &str) {
        let inherited: Vec<String> =
            self.derives.get(from).map(|s| s.iter().cloned().collect()).unwrap_or_default();
        let deps = self.derives.entry(n.to_string()).or_default();
        deps.insert(from.to_string());
        deps.extend(inherited);
    }

    fn skip_interlocked(&mut self, n: &str, other: &str) {
        self.derives.entry(n.to_string()).or_default().insert(other.to_string());
    }

    fn final_pos(&mut self, n: &str) -> f64 {
        if let Some(got) = self.resolved.get(n) {
            return got.0;
        }
        self.resolving.insert(n.to_string());
        self.derives.entry(n.to_string()).or_default();
        let out = self.compute(n);
        self.resolving.remove(n);
        out
    }

    fn compute(&mut self, n: &str) -> f64 {
        // 1. "after": right after the latest-loading non-master plugin n must load after.
        let preds = self.preds.get(n).cloned().unwrap_or_default();
        let mut best: Option<(f64, String)> = None;
        for p in &preds {
            if is_master_file(p) {
                continue; // the master block, top of the list: no position signal
            }
            let bp = if self.subset_set.contains(p.as_str()) {
                if self.resolving.contains(p) {
                    self.skip_interlocked(n, p);
                    continue;
                }
                let v = self.final_pos(p);
                if self.derives_from(p, n) {
                    continue; // p's position came from n's: circular
                }
                v + POSITION_EPSILON // right after that custom
            } else {
                self.base_index.get(p.as_str()).copied().unwrap_or(0) as f64 + 0.5
            };
            let better = match &best {
                None => true,
                Some((b, _)) => bp > *b,
            };
            if better {
                best = Some((bp, p.clone()));
            }
        }
        if let Some((val, anchor)) = best {
            if self.subset_set.contains(anchor.as_str()) {
                self.note_derived(n, &anchor);
            }
            self.resolved.insert(n.to_string(), (val, Some(anchor), How::After));
            return val;
        }
        // 2. "before": just before the earliest-loading thing n must load before.
        let succs = self.succs.get(n).cloned().unwrap_or_default();
        let mut low: Option<(f64, String)> = None;
        for s in &succs {
            let bs = if self.subset_set.contains(s.as_str()) {
                if self.resolving.contains(s) {
                    self.skip_interlocked(n, s);
                    continue;
                }
                let v = self.final_pos(s);
                if self.derives_from(s, n) {
                    continue;
                }
                v - POSITION_EPSILON // just before that custom
            } else {
                // just before that curated plugin
                self.base_index.get(s.as_str()).copied().unwrap_or(0) as f64 - 0.5 + POSITION_EPSILON
            };
            let better = match &low {
                None => true,
                Some((l, _)) => bs < *l,
            };
            if better {
                low = Some((bs, s.clone()));
            }
        }
        if let Some((val, anchor)) = low {
            if self.subset_set.contains(anchor.as_str()) {
                self.note_derived(n, &anchor);
            }
            self.resolved.insert(n.to_string(), (val, Some(anchor), How::Before));
            return val;
        }
        // 3. Neither: the cfg place, or the end.
        let val = self.no_signal_pos(n);
        self.resolved.insert(n.to_string(), (val, None, How::None));
        val
    }
}

/// The key the ready plugins are taken in: masters first, then position, then name without
/// case, then name.
struct Rank {
    master: u8,
    pos: f64,
    lower: String,
    name: String,
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        self.master
            .cmp(&other.master)
            .then(self.pos.partial_cmp(&other.pos).unwrap_or(Ordering::Equal))
            .then_with(|| self.lower.cmp(&other.lower))
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for Rank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Rank {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Rank {}

/// Step 4: Kahn's algorithm over the positions. Edges always win over the position.
fn kahn_place(
    adj: &Adjacency,
    mut indeg: BTreeMap<String, usize>,
    pos: &HashMap<String, f64>,
    nb: usize,
) -> Vec<String> {
    let rank = |n: &str| Rank {
        master: u8::from(!is_master_file(n)),
        pos: pos.get(n).copied().unwrap_or(nb as f64),
        lower: n.to_lowercase(),
        name: n.to_string(),
    };
    let mut ready: BinaryHeap<Reverse<Rank>> =
        adj.keys().filter(|n| indeg[n.as_str()] == 0).map(|n| Reverse(rank(n))).collect();
    let mut result: Vec<String> = Vec::new();
    while let Some(Reverse(top)) = ready.pop() {
        let n = top.name;
        if let Some(targets) = adj.get(&n) {
            for m in targets {
                if let Some(d) = indeg.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        ready.push(Reverse(rank(m)));
                    }
                }
            }
        }
        result.push(n);
    }
    if result.len() != adj.len() {
        // Edges that would close a cycle are refused as they are added, so this cannot be
        // reached; the Python keeps the same backstop: the rest at the end.
        let placed: HashSet<&str> = result.iter().map(String::as_str).collect();
        let mut remaining: Vec<String> =
            adj.keys().filter(|n| !placed.contains(n.as_str())).cloned().collect();
        remaining.sort_by_key(|n| n.to_lowercase());
        result.extend(remaining);
    }
    result
}

/// Place the customs among the curated plugins (`build_and_sort`).
///
/// - `base`: the curated order, frozen.
/// - `subset`: the customs, in any spelling (as [`crate::graph::canonical_subset`] reads them).
/// - `blocks`: the `[Order]` chains as `(names, priority)`; a rule file's index is its priority.
/// - `masters`: `{custom, in lower case: the masters its header lists}`.
/// - `nearstart`, `nearend`: the hint patterns, in the order the rule files list them.
///
/// # Errors
/// [`Unsupported`] when a rule or hint name is one [`expand`] declines.
pub fn build_and_sort(
    base: &[String],
    subset: &[String],
    blocks: &[(Vec<String>, i64)],
    masters: &BTreeMap<String, Vec<String>>,
    nearstart: &[String],
    nearend: &[String],
) -> Result<Sorted, Unsupported> {
    let Edges { subset, adj, indegree, conflicts } = build_edges(base, subset, blocks, masters)?;
    let nb = base.len();
    let base_index: HashMap<&str, usize> = base.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut pool: Vec<String> = base.to_vec();
    pool.extend(subset.iter().filter(|n| !base_index.contains_key(n.as_str())).cloned());
    let subset_set: HashSet<&str> = subset.iter().map(String::as_str).collect();

    // The customs' graph neighbours; sorted by name without case so the result does not depend
    // on the order the edges were added in.
    let mut preds: HashMap<String, Vec<String>> = HashMap::new();
    let mut succs: HashMap<String, Vec<String>> = HashMap::new();
    for (a, targets) in &adj {
        for b in targets {
            if subset_set.contains(b.as_str()) {
                preds.entry(b.clone()).or_default().push(a.clone());
            }
            if subset_set.contains(a.as_str()) {
                succs.entry(a.clone()).or_default().push(b.clone());
            }
        }
    }
    for list in preds.values_mut().chain(succs.values_mut()) {
        list.sort_by_key(|n| n.to_lowercase());
    }

    let mut r = Resolver {
        preds,
        succs,
        base_index: &base_index,
        subset_set: &subset_set,
        declared_end: subset.iter().enumerate().map(|(j, n)| (n.clone(), nb + j)).collect(),
        resolved: HashMap::new(),
        derives: HashMap::new(),
        resolving: HashSet::new(),
    };
    for n in &subset {
        r.final_pos(n);
    }
    // A custom resolved while a neighbour was in flight may hold a degraded value (the
    // neighbour's contribution was skipped). Settle the ones that depended on another custom's,
    // until the values stop changing; a DAG settles within its depth in rounds.
    for _round in 0..=subset.len() {
        let mut changed = false;
        let again: Vec<&String> =
            subset.iter().filter(|x| r.derives.get(x.as_str()).is_some_and(|d| !d.is_empty())).collect();
        for n in again {
            let old = r.resolved.remove(n.as_str()).map_or(0.0, |v| v.0);
            r.derives.insert(n.clone(), BTreeSet::new());
            r.final_pos(n);
            if r.resolved[n.as_str()].0 != old {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut pos: HashMap<String, f64> = base_index.iter().map(|(n, &i)| (n.to_string(), i as f64)).collect();
    let mut anchors: BTreeMap<String, (How, Option<String>)> = BTreeMap::new();
    for n in &subset {
        let (val, anchor, how) = r.resolved[n.as_str()].clone();
        match how {
            How::After | How::Before => {
                pos.insert(n.clone(), val);
            }
            _ => {
                // no signal: the cfg place (already in `pos`), or the end in declared order
                pos.entry(n.clone()).or_insert(r.declared_end[n.as_str()] as f64);
            }
        }
        anchors.insert(n.to_lowercase(), (how, anchor));
    }

    // [NearStart]/[NearEnd]: a position, not an edge, and only for customs. The rank is the
    // order they are listed in; the first mention fixes it.
    let mut hinted: HashSet<&str> = HashSet::new();
    for (pats, to_start) in [(nearstart, true), (nearend, false)] {
        let mut rank = 0usize;
        for pat in pats {
            for n in expand(pat, &pool)? {
                if !subset_set.contains(n) || hinted.contains(n) {
                    continue;
                }
                hinted.insert(n);
                let j = rank;
                rank += 1;
                let value = if to_start {
                    -1.0 + j as f64 * POSITION_EPSILON
                } else {
                    (2 * nb + subset.len() + j) as f64
                };
                pos.insert(n.to_string(), value);
                let how = if to_start { How::NearStart } else { How::NearEnd };
                anchors.insert(n.to_lowercase(), (how, None));
            }
        }
    }

    let order = kahn_place(&adj, indegree, &pos, nb);
    Ok(Sorted { order, anchors, conflicts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn sort(base: &[&str], subset: &[&str], blocks: &[(&[&str], i64)], ns: &[&str], ne: &[&str]) -> Sorted {
        let blocks: Vec<(Vec<String>, i64)> = blocks.iter().map(|(b, p)| (names(b), *p)).collect();
        build_and_sort(&names(base), &names(subset), &blocks, &BTreeMap::new(), &names(ns), &names(ne))
            .unwrap()
    }

    #[test]
    fn a_custom_with_no_signal_goes_to_the_end() {
        let s = sort(&["A.esp", "B.esp"], &["X.esp"], &[], &[], &[]);
        assert_eq!(s.order, ["A.esp", "B.esp", "X.esp"]);
        assert_eq!(s.anchors["x.esp"], (How::None, None));
    }

    #[test]
    fn right_after_a_curated_plugin_and_before_the_next() {
        let s = sort(&["A.esp", "B.esp", "C.esp"], &["X.esp"], &[(&["B.esp", "X.esp"], 0)], &[], &[]);
        assert_eq!(s.order, ["A.esp", "B.esp", "X.esp", "C.esp"]);
        assert_eq!(s.anchors["x.esp"], (How::After, Some("B.esp".to_string())));
    }

    #[test]
    fn a_before_signal_places_a_custom_just_short_of_its_successor() {
        let s = sort(&["A.esp", "B.esp", "C.esp"], &["X.esp"], &[(&["X.esp", "C.esp"], 0)], &[], &[]);
        assert_eq!(s.order, ["A.esp", "B.esp", "X.esp", "C.esp"]);
        assert_eq!(s.anchors["x.esp"].0, How::Before);
    }

    #[test]
    fn a_custom_master_floats_into_the_master_block() {
        let s = sort(&["Morrowind.esm", "A.esp"], &["Mine.esm"], &[], &[], &[]);
        assert_eq!(s.order, ["Morrowind.esm", "Mine.esm", "A.esp"]);
    }

    #[test]
    fn hints_keep_the_order_they_are_listed_in() {
        let s = sort(&["Base.esp"], &["A.esp", "B.esp", "C.esp"], &[], &[], &["C.esp", "B.esp", "A.esp"]);
        assert_eq!(s.order, ["Base.esp", "C.esp", "B.esp", "A.esp"]);
        assert_eq!(s.anchors["c.esp"].0, How::NearEnd);
    }

    #[test]
    fn an_edge_wins_over_a_hint() {
        // the hint and the edge agree: the front is before B
        let s = sort(&["A.esp", "B.esp"], &["X.esp"], &[(&["X.esp", "B.esp"], 0)], &["X.esp"], &[]);
        assert_eq!(s.order, ["X.esp", "A.esp", "B.esp"]);
        // the edge says after B: the hint gives way
        let s = sort(&["A.esp", "B.esp"], &["X.esp"], &[(&["B.esp", "X.esp"], 0)], &["X.esp"], &[]);
        assert_eq!(s.order, ["A.esp", "B.esp", "X.esp"]);
    }

    #[test]
    fn a_custom_after_an_unanchored_custom_is_not_dragged_to_its_fallback_end() {
        let s = sort(&["A.esp", "B.esp"], &["X.esp", "Y.esp"], &[(&["X.esp", "Y.esp"], 0)], &[], &[]);
        assert_eq!(s.order, ["A.esp", "B.esp", "X.esp", "Y.esp"]);
    }
}
