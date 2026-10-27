//! The conflict map (port step 4.3, slice 10): conflicting records gathered onto the world
//! grid (`wraithguard/viz/geometry.py`, `parse_grid` and `group_by_cell`, which
//! `viz/conflictmap.py` draws). Only records whose id carries a grid are placed: landscapes
//! and exterior cells (`"(x, y)"`) and cell-scoped records of an exterior
//! (`"Balmora (-3, -2)"`); objects, dialogue and interiors belong to the list.

use std::collections::BTreeMap;

use crate::records::Conflict;

/// Grid coordinates past this are taken as garbage (the engine's limit).
pub const GRID_LIMIT: i64 = 128;

/// A record id's grid (`parse_grid`): the `(x, y)` it ends with, inside [`GRID_LIMIT`].
pub fn parse_grid(id: &str) -> Option<(i64, i64)> {
    let t = id.trim_end();
    let t = t.strip_suffix(')')?;
    let open = t.rfind('(')?;
    let inner = &t[open + 1..];
    let (x, y) = inner.split_once(',')?;
    let num = |s: &str| -> Option<i64> {
        let s = s.trim();
        let digits = s.strip_prefix('-').unwrap_or(s);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    // The Python's pattern allows spaces only around the comma.
    if x != x.trim_start() || y != y.trim_end() {
        return None;
    }
    let (x, y) = (num(x)?, num(y)?);
    (x.abs() <= GRID_LIMIT && y.abs() <= GRID_LIMIT).then_some((x, y))
}

/// One cell's conflicts (`CellConflicts`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellConflicts {
    pub x: i64,
    pub y: i64,
    /// Conflicting records placed here.
    pub total: usize,
    /// Those involving the user's own plugins.
    pub mine: usize,
    /// Records by type.
    pub types: BTreeMap<String, usize>,
    /// The plugins conflicting here, first met first.
    pub plugins: Vec<String>,
    /// Each plugin's records here by type.
    pub by_plugin: BTreeMap<String, BTreeMap<String, usize>>,
    /// How many records each plugin wins here.
    pub winners: BTreeMap<String, usize>,
}

/// The conflicts gathered by cell (`group_by_cell`), by `(x, y)`.
pub fn group_by_cell(conflicts: &[Conflict]) -> BTreeMap<(i64, i64), CellConflicts> {
    let mut out: BTreeMap<(i64, i64), CellConflicts> = BTreeMap::new();
    for c in conflicts {
        let Some((x, y)) = parse_grid(&c.id) else { continue };
        let e = out.entry((x, y)).or_insert_with(|| CellConflicts { x, y, ..CellConflicts::default() });
        let rtype = if c.rtype.is_empty() { "?" } else { c.rtype.as_str() };
        *e.types.entry(rtype.to_string()).or_default() += 1;
        for p in &c.plugins {
            if !e.plugins.contains(p) {
                e.plugins.push(p.clone());
            }
            *e.by_plugin.entry(p.clone()).or_default().entry(rtype.to_string()).or_default() += 1;
        }
        if !c.winner.is_empty() {
            *e.winners.entry(c.winner.clone()).or_default() += 1;
        }
        e.total += 1;
        e.mine += usize::from(c.involves_subset);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_are_read_from_the_end_of_an_id() {
        assert_eq!(parse_grid("(43, -45)"), Some((43, -45)));
        assert_eq!(parse_grid("Balmora (-3, -2)"), Some((-3, -2)));
        assert_eq!(parse_grid("(1 ,2)"), Some((1, 2)));
        assert_eq!(parse_grid("(1,2)  "), Some((1, 2)));
        assert_eq!(parse_grid("Vivec, Arena"), None);
        assert_eq!(parse_grid("(500, 1)"), None);
        assert_eq!(parse_grid("( 1, 2)"), None);
    }

    #[test]
    fn cells_count_their_records_plugins_and_winners() {
        let c = |t: &str, id: &str, ps: &[&str], mine: bool| Conflict {
            rtype: t.into(),
            id: id.into(),
            plugins: ps.iter().map(|p| (*p).to_string()).collect(),
            winner: (*ps.last().unwrap()).to_string(),
            involves_subset: mine,
            deleted_by: Vec::new(),
        };
        let g = group_by_cell(&[
            c("Landscape", "(1, 2)", &["A.esp", "B.esp"], false),
            c("PathGrid", "Seyda Neen (1, 2)", &["A.esp", "C.esp"], true),
            c("Npc", "fargoth", &["A.esp", "B.esp"], false),
        ]);
        let e = &g[&(1, 2)];
        assert_eq!((e.total, e.mine), (2, 1));
        assert_eq!(e.plugins, ["A.esp", "B.esp", "C.esp"]);
        assert_eq!(e.by_plugin["A.esp"].len(), 2);
        assert_eq!(e.winners["C.esp"], 1);
        assert_eq!(g.len(), 1);
    }
}
