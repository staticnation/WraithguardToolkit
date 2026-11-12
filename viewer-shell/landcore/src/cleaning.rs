//! Dropping merged cells that change nothing (`land/cleaning.py`).
//!
//! A merged cell equal to the reference on every layer both carry repeats the masters; one
//! only a single mod edits, and that comes out exactly as that mod has it, repeats the mod.
//! Either way the load order already gives that terrain, so the cell is left out. The Python
//! compares BLAKE2 digests of the layers; equal layers are compared here directly, which is
//! the same answer.

use std::collections::{BTreeMap, BTreeSet};
use crate::grid::Value;

pub type Coords = (i64, i64);

/// The layers of a cell compared (`CellDigest`); `None` a layer it does not carry. Each
/// layer is held as a hash of its values ([`digest`]): cleaning only asks whether two
/// are equal, and a merge kept a full copy of every layer of every changed cell for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellDigest {
    pub heights: Option<u64>,
    pub normals: Option<u64>,
    pub world_map: Option<u64>,
    pub colors: Option<u64>,
    pub textures: Option<u64>,
}

/// A layer's values as a [`CellDigest`] holds them.
pub fn digest(values: &[Value]) -> u64 {
    digest_of(values.iter().copied())
}

/// [`digest`] of values as they come, nothing gathered first (a packed layer, a delta grid).
/// Only compared within one merge, never kept, so the hash is a fast one rather than SipHash
/// (2026-11-10: the digests were a fifth of the finish step): two values a step, each step
/// folded through a 128-bit product, the count folded in last.
pub fn digest_of(values: impl Iterator<Item = Value>) -> u64 {
    let mut values = values;
    let (mut h, mut n): (u64, u64) = (0x243F_6A88_85A3_08D3, 0);
    // Two values a step, read as they come (no state carried between them).
    while let Some(a) = values.next() {
        match values.next() {
            Some(b) => {
                n += 2;
                h = digest_fold(h ^ (u64::from(a as u32) | (u64::from(b as u32) << 32)), DIGEST_K);
            }
            None => {
                n += 1;
                h = digest_fold(h ^ u64::from(a as u32) ^ (1 << 63), DIGEST_K);
            }
        }
    }
    digest_fold(h ^ n, DIGEST_M)
}

const DIGEST_K: u64 = 0x9E37_79B9_7F4A_7C15;
const DIGEST_M: u64 = 0xBF58_476D_1CE4_E5B9;

#[inline]
fn digest_fold(a: u64, b: u64) -> u64 {
    let p = u128::from(a) * u128::from(b);
    (p as u64) ^ ((p >> 64) as u64)
}

impl CellDigest {
    fn layers(&self) -> [&Option<u64>; 5] {
        [&self.heights, &self.normals, &self.world_map, &self.colors, &self.textures]
    }

    pub fn is_empty(&self) -> bool {
        self.layers().iter().all(|l| l.is_none())
    }
}

/// A layer only differs when both sides carry it (`differs`).
fn differs(a: &Option<u64>, b: &Option<u64>) -> bool {
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
        // Every name is a plugin the merge treated as a mod, a late `.esm` among them.
        let editors = sources.get(coords).map_or(0, Vec::len);
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

/// The digest of a colour layer that is all white, which is what the engine draws where a
/// loaded `LAND` carries no `VCLR`.
fn white_colors() -> u64 {
    static WHITE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *WHITE.get_or_init(|| digest(&vec![255; crate::grid::LAND_NUM_VERTS * 3]))
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
        let differs_layer = |mine: &Option<u64>, theirs: Option<&Option<u64>>| match (mine, theirs.and_then(Option::as_ref)) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(a), Some(b)) => a != b,
        };
        let there = loaded.get(coords);
        // A loaded cell with no colours is drawn white: an all-white layer is no change to it
        // (a missing LAND is not that, and nor is colour over colour, or a white override of
        // terrain that has colours - those still differ).
        let colors_differ = match (grid.colors, there) {
            (Some(mine), Some(d)) if d.colors.is_none() => mine != white_colors(),
            _ => differs_layer(&grid.colors, there.map(|d| &d.colors)),
        };
        let differs = differs_layer(&grid.heights, there.map(|d| &d.heights))
            || differs_layer(&grid.world_map, there.map(|d| &d.world_map))
            || colors_differ
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

    fn d(h: Option<Vec<Value>>, c: Option<Vec<Value>>) -> CellDigest {
        CellDigest { heights: h.as_deref().map(digest), colors: c.as_deref().map(digest), ..Default::default() }
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

    /// The digest as it was first written (an `Option` carried between values): the faster
    /// loop gives the same numbers, so what it compares is unchanged.
    /// WG-A02: the loaded cell's missing colours are white. All white over them is no change;
    /// any other colour is, and so is white over colours the loaded cell has.
    #[test]
    fn absent_loaded_colors_are_white() {
        let white = vec![255; crate::grid::LAND_NUM_VERTS * 3];
        let mut tint = white.clone();
        tint[7] = 3;
        let merged = BTreeMap::from([
            ((0, 0), d(Some(vec![1]), Some(white.clone()))),
            ((1, 0), d(Some(vec![1]), Some(tint))),
            ((2, 0), d(Some(vec![1]), Some(white.clone()))),
            ((3, 0), d(Some(vec![1]), Some(white.clone()))),
        ]);
        let loaded = BTreeMap::from([
            ((0, 0), d(Some(vec![1]), None)),
            ((1, 0), d(Some(vec![1]), None)),
            ((2, 0), d(Some(vec![1]), Some(vec![9]))),
        ]);
        let (keep, _) = clean_against_loaded(&merged, &loaded);
        // (0,0) white on none: nothing; (1,0) tinted on none: kept; (2,0) white over colours: kept;
        // (3,0) no LAND loaded at all: kept.
        assert_eq!(keep, BTreeSet::from([(1, 0), (2, 0), (3, 0)]));
    }

    #[test]
    fn the_digest_is_the_same_as_the_first_form() {
        fn first(values: impl Iterator<Item = Value>) -> u64 {
            let fold = |a: u64, b: u64| {
                let p = u128::from(a) * u128::from(b);
                (p as u64) ^ ((p >> 64) as u64)
            };
            let (mut h, mut n, mut pending): (u64, u64, Option<u32>) = (0x243F_6A88_85A3_08D3, 0, None);
            for v in values {
                n += 1;
                match pending.take() {
                    None => pending = Some(v as u32),
                    Some(a) => h = fold(h ^ (u64::from(a) | (u64::from(v as u32) << 32)), 0x9E37_79B9_7F4A_7C15),
                }
            }
            if let Some(a) = pending {
                h = fold(h ^ u64::from(a) ^ (1 << 63), 0x9E37_79B9_7F4A_7C15);
            }
            fold(h ^ n, 0xBF58_476D_1CE4_E5B9)
        }
        for len in [0usize, 1, 2, 3, 4225, 12675] {
            let v: Vec<Value> = (0..len).map(|i| (i as Value * 7919) % 6011 - 3000).collect();
            assert_eq!(digest(&v), first(v.iter().copied()), "{len}");
        }
    }
}
