//! Dropping merged cells that change nothing (`land/cleaning.py`).
//!
//! A merged cell equal to the reference on every layer both carry repeats the masters; one
//! only a single mod edits, and that comes out exactly as that mod has it, repeats the mod.
//! Either way the load order already gives that terrain, so the cell is left out. The Python
//! compares BLAKE2 digests of the layers; equal layers are compared here directly, which is
//! the same answer.

use std::collections::{BTreeMap, BTreeSet};

pub type Coords = (i64, i64);

/// The layers of a cell compared (`CellDigest`); `None` a layer it does not carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellDigest {
    pub heights: Option<Vec<i64>>,
    pub normals: Option<Vec<i64>>,
    pub world_map: Option<Vec<i64>>,
    pub colors: Option<Vec<i64>>,
    pub textures: Option<Vec<i64>>,
}

impl CellDigest {
    fn layers(&self) -> [&Option<Vec<i64>>; 5] {
        [&self.heights, &self.normals, &self.world_map, &self.colors, &self.textures]
    }

    pub fn is_empty(&self) -> bool {
        self.layers().iter().all(|l| l.is_none())
    }
}

/// A layer only differs when both sides carry it (`differs`).
fn differs(a: &Option<Vec<i64>>, b: &Option<Vec<i64>>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a != b)
}

fn differs_any(a: &CellDigest, b: &CellDigest) -> bool {
    a.layers().iter().zip(b.layers()).any(|(x, y)| differs(x, y))
}

/// Whether a plugin name is a mod rather than a master (`is_mod`).
pub fn is_mod(name: &str) -> bool {
    let low = name.to_lowercase();
    low.ends_with(".esp") || low.ends_with(".omwaddon")
}

/// What cleaning did (`CleaningReport`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CleaningReport {
    pub unmodified: usize,
    pub single_source: usize,
    pub kept: usize,
    pub kept_for_seams: usize,
}

impl CleaningReport {
    pub fn dropped(&self) -> usize {
        self.unmodified + self.single_source
    }
}

/// The cells worth writing (`clean_landmass`).
pub fn clean_landmass(
    merged: &BTreeMap<Coords, CellDigest>,
    reference: &BTreeMap<Coords, CellDigest>,
    sources: &BTreeMap<Coords, Vec<String>>,
    originals: &BTreeMap<Coords, CellDigest>,
) -> (BTreeSet<Coords>, CleaningReport) {
    let mut report = CleaningReport::default();
    let mut keep = BTreeSet::new();
    for (coords, grid) in merged {
        if let Some(base) = reference.get(coords)
            && !base.is_empty()
            && !differs_any(grid, base)
        {
            report.unmodified += 1;
            continue;
        }
        let editors = sources.get(coords).map(|s| s.iter().filter(|n| is_mod(n)).count()).unwrap_or(0);
        if editors == 1 {
            let original = originals.get(coords);
            if let Some(o) = original
                && !o.is_empty()
                && !differs_any(grid, o)
            {
                report.single_source += 1;
                continue;
            }
            if original.is_some() {
                report.kept_for_seams += 1;
            }
        }
        keep.insert(*coords);
        report.kept += 1;
    }
    (keep, report)
}

/// The cells worth writing, judged against what the load order already loads (the OpenMW
/// fork's `clean_landmass_diff`): a merged cell is kept when a layer it carries differs from
/// the loaded cell's, or the loaded cell lacks it; a cell that comes out exactly as the game
/// would load it anyway is left out. This covers both rules of [`clean_landmass`] and more -
/// a cell two mods touch whose merge equals the last one's record - and keeps a cell whose
/// merge equals the masters where a mod's excluded layer would otherwise win.
pub fn clean_against_loaded(merged: &BTreeMap<Coords, CellDigest>, loaded: &BTreeMap<Coords, CellDigest>) -> (BTreeSet<Coords>, CleaningReport) {
    let mut report = CleaningReport::default();
    let mut keep = BTreeSet::new();
    for (coords, grid) in merged {
        let differs_layer = |mine: &Option<Vec<i64>>, theirs: Option<&Option<Vec<i64>>>| match (mine, theirs.and_then(Option::as_ref)) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(a), Some(b)) => a != b,
        };
        let there = loaded.get(coords);
        let differs = differs_layer(&grid.heights, there.map(|d| &d.heights))
            || differs_layer(&grid.world_map, there.map(|d| &d.world_map))
            || differs_layer(&grid.colors, there.map(|d| &d.colors))
            || differs_layer(&grid.textures, there.map(|d| &d.textures));
        if differs {
            keep.insert(*coords);
            report.kept += 1;
        } else {
            report.unmodified += 1;
        }
    }
    (keep, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(h: Option<Vec<i64>>, c: Option<Vec<i64>>) -> CellDigest {
        CellDigest { heights: h, colors: c, ..Default::default() }
    }

    #[test]
    fn against_the_load_order() {
        let merged = BTreeMap::from([((0, 0), d(Some(vec![1]), None)), ((1, 0), d(Some(vec![1]), Some(vec![5]))), ((2, 0), d(Some(vec![1]), None))]);
        let loaded = BTreeMap::from([((0, 0), d(Some(vec![1]), Some(vec![9]))), ((1, 0), d(Some(vec![1]), None))]);
        let (keep, r) = clean_against_loaded(&merged, &loaded);
        // (0,0) is what loads already; (1,0) carries colours the loaded cell lacks; (2,0) loads nothing.
        assert_eq!(keep, BTreeSet::from([(1, 0), (2, 0)]));
        assert_eq!((r.unmodified, r.kept, r.dropped()), (1, 2, 1));
    }
}
