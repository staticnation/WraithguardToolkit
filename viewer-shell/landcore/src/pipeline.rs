//! From the plugins' changes to the cells to write (`land/pipeline.py`): merge each layer
//! plugin by plugin in load order, fill what no mod touched from the reference, bring in the
//! untouched neighbours, repair the seams, limit the slopes, recompute the normals, check
//! the borders, and drop the cells that change nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::cleaning::{clean_against_loaded, clean_landmass, CellDigest, CleaningReport};
use crate::grid::RelativeGrid;
use crate::heights::resolve_cell_normals;
use crate::landmass::{decode_plugin, differences, layer_onto, masked, Coords, Landmass, PluginRecords};
use crate::layers::{LandscapeDiff, LandscapeLayers, Layer, ALL_LAYERS};
use crate::merge::{self, merge_layer, ConflictParams};
use crate::meta::{layer, PluginMeta, Strategy};
use crate::seams::{find_tears, repair_seams, SeamReport, DEFAULT_FEATHER};
use crate::slope::{limit_slopes, SlopeReport};
use crate::textures::KnownTextures;

/// One cell of the merge (`MergedCell`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergedCell {
    pub coords: Coords,
    pub heights: Option<Vec<i64>>,
    pub textures: Option<Vec<i64>>,
    pub world_map: Option<Vec<i64>>,
    pub colors: Option<Vec<i64>>,
    pub normals: Option<Vec<i64>>,
    /// The plugins that changed it, in load order.
    pub editors: Vec<String>,
    /// Height vertices more than one plugin moved, and those settled far from an intent.
    pub contested: usize,
    pub major: usize,
    pub new_land: bool,
    /// Each layer as the one plugin that alone supplied it has it, for cleaning.
    pub sole_source: CellDigest,
}

/// The whole merge (`MergeOutcome`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeOutcome {
    pub cells: BTreeMap<Coords, MergedCell>,
    pub seams: SeamReport,
    pub slopes: SlopeReport,
    pub cleaning: CleaningReport,
    pub skipped_plugins: Vec<(String, String)>,
    pub borrowed: usize,
    /// What the Python logs and does not report: records left out, and why.
    pub notes: Vec<String>,
}

/// The four layers merged, in the Python's order (`_fold_change`); normals are recomputed.
const FOLDED: [(Layer, u32); 4] = [
    (Layer::Heights, layer::VERTEX_HEIGHTS),
    (Layer::Textures, layer::TEXTURES),
    (Layer::WorldMap, layer::WORLD_MAP),
    (Layer::Colors, layer::VERTEX_COLORS),
];

fn slot(cell: &mut MergedCell, which: Layer) -> &mut Option<Vec<i64>> {
    match which {
        Layer::Heights => &mut cell.heights,
        Layer::Textures => &mut cell.textures,
        Layer::WorldMap => &mut cell.world_map,
        Layer::Colors => &mut cell.colors,
        Layer::Normals => &mut cell.normals,
    }
}

fn digest_slot(d: &mut CellDigest, which: Layer) -> &mut Option<Vec<i64>> {
    match which {
        Layer::Heights => &mut d.heights,
        Layer::Textures => &mut d.textures,
        Layer::WorldMap => &mut d.world_map,
        Layer::Colors => &mut d.colors,
        Layer::Normals => &mut d.normals,
    }
}

fn fold_change(cell: &mut MergedCell, grids: &mut HashMap<Layer, RelativeGrid>, change: &LandscapeDiff, meta: Option<&PluginMeta>, default: Strategy) -> Result<(), String> {
    for (which, bit) in FOLDED {
        let Some(source) = change.grid(which) else { continue };
        let mut chosen = meta.map(|m| m.strategy_for(bit)).unwrap_or_default();
        if chosen == Strategy::Auto {
            chosen = if bit == layer::VERTEX_HEIGHTS { default } else { Strategy::Auto };
            if cell.new_land && bit == layer::VERTEX_HEIGHTS {
                chosen = Strategy::Overwrite;
            }
        }
        let first = !grids.contains_key(&which);
        let (merged, contested, major) = match grids.get(&which) {
            None => (source.clone(), 0, 0),
            Some(previous) => {
                let (m, report) = merge_layer(bit, previous, source, chosen, ConflictParams::default())?;
                (m, report.contested, report.major)
            }
        };
        grids.insert(which, merged);
        *digest_slot(&mut cell.sole_source, which) = if first { Some(source.flat()) } else { None };
        if bit == layer::VERTEX_HEIGHTS {
            cell.contested += contested;
            cell.major += major;
        }
    }
    Ok(())
}

/// What the reference holds for a layer no mod changed (`inherit_reference_layers`).
pub fn inherit_reference_layers(cell: &mut MergedCell, reference: &Landmass) {
    let Some(l) = reference.get(cell.coords) else { return };
    for which in [Layer::Heights, Layer::Textures, Layer::WorldMap, Layer::Colors] {
        let s = slot(cell, which);
        if s.is_none() {
            *s = l.get(which).as_deref().cloned();
        }
    }
}

/// What a plugin's change is measured against (the OpenMW fork's choice, and the original's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffModel {
    /// Against the masters (Merged Lands, the Python): a plugin's change is what it does to
    /// the vanilla terrain, so two mods that reshape different parts of a cell both survive
    /// even where the later one carries vanilla ground over the earlier one's work.
    #[default]
    Masters,
    /// Against what loaded before it (the OpenMW fork, 0.2): masters plus every earlier
    /// plugin's allowed layers. A plugin's change is what it does to the load order, so the
    /// result follows the load order's winners, with the seams repaired and each
    /// `.mergedlands.toml`'s choices on top.
    LoadOrder,
}

/// One plugin's change folded in the load-order model: each layer merged onto the cell's
/// running merge by value (`merge::merge_onto`), the masters' layer the start where they
/// have one.
fn fold_rolling(cell: &mut MergedCell, grids: &mut HashMap<Layer, RelativeGrid>, change: &LandscapeDiff, reference: &Landmass, meta: Option<&PluginMeta>, default: Strategy) -> Result<(), String> {
    for (which, bit) in FOLDED {
        let Some(source) = change.grid(which) else { continue };
        let mut chosen = meta.map(|m| m.strategy_for(bit)).unwrap_or_default();
        if chosen == Strategy::Auto {
            chosen = if bit == layer::VERTEX_HEIGHTS { default } else { Strategy::Auto };
            if cell.new_land && bit == layer::VERTEX_HEIGHTS {
                chosen = Strategy::Overwrite;
            }
        }
        let acc = match grids.entry(which) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let start = reference.get(cell.coords).and_then(|l| l.get(which).as_deref().cloned()).filter(|v| v.len() == source.reference.len());
                match start {
                    Some(values) => e.insert(RelativeGrid::new(values, source.side, source.components)?),
                    None => {
                        e.insert(source.clone());
                        continue;
                    }
                }
            }
        };
        let report = merge::merge_onto(acc, source, bit, chosen, ConflictParams::default())?;
        if bit == layer::VERTEX_HEIGHTS {
            cell.contested += report.contested;
            cell.major += report.major;
        }
    }
    Ok(())
}

/// Every plugin's changes merged, in load order (`merge_landmass`). `loaded`, when given,
/// gathers the terrain the engine itself would load from these plugins (settings ignored,
/// previous merges left out), for the load-order cleaning; it starts as the masters'.
pub fn merge_landmass(
    reference: &Landmass,
    plugins: &[PluginRecords],
    known: &mut KnownTextures,
    metas: &HashMap<String, PluginMeta>,
    strategy: Strategy,
    model: DiffModel,
    mut loaded: Option<&mut Landmass>,
) -> Result<MergeOutcome, String> {
    let mut outcome = MergeOutcome::default();
    let mut working: BTreeMap<Coords, HashMap<Layer, RelativeGrid>> = BTreeMap::new();
    let mut rolling = (model == DiffModel::LoadOrder).then(|| reference.clone());
    for plugin in plugins {
        let meta = metas.get(&plugin.name);
        if meta.is_some_and(PluginMeta::is_previous_merge) {
            outcome.skipped_plugins.push((plugin.name.clone(), "a previous merge".into()));
            continue;
        }
        let allowed = meta.map(PluginMeta::allowed_layers).unwrap_or(ALL_LAYERS);
        if allowed == 0 {
            outcome.skipped_plugins.push((plugin.name.clone(), "every layer excluded".into()));
            if let Some(l) = loaded.as_deref_mut() {
                for c in decode_plugin(plugin, known, &mut outcome.notes)? {
                    layer_onto(l, c, &plugin.name);
                }
            }
            continue;
        }
        let cells = decode_plugin(plugin, known, &mut outcome.notes)?;
        let against = rolling.as_ref().unwrap_or(reference);
        let changes = differences(against, &plugin.name, &cells, allowed)?;
        for change in changes {
            let cell = outcome.cells.entry(change.coords).or_insert_with(|| MergedCell { coords: change.coords, new_land: change.new_land, ..Default::default() });
            let grids = working.entry(change.coords).or_default();
            cell.editors.push(plugin.name.clone());
            cell.new_land = cell.new_land || change.new_land;
            match model {
                DiffModel::Masters => fold_change(cell, grids, &change, meta, strategy)?,
                DiffModel::LoadOrder => fold_rolling(cell, grids, &change, reference, meta, strategy)?,
            }
        }
        if let Some(r) = rolling.as_mut() {
            for c in &cells {
                layer_onto(r, masked(c.clone(), allowed), &plugin.name);
            }
        }
        if let Some(l) = loaded.as_deref_mut() {
            for c in cells {
                layer_onto(l, c, &plugin.name);
            }
        }
    }
    for (coords, grids) in working {
        let cell = outcome.cells.get_mut(&coords).unwrap();
        for which in [Layer::Heights, Layer::Textures, Layer::WorldMap, Layer::Colors] {
            if let Some(g) = grids.get(&which) {
                *slot(cell, which) = Some(g.flat());
            }
        }
        inherit_reference_layers(cell, reference);
    }
    Ok(outcome)
}

/// Each written cell's normals from its final heights, the reference's kept where the
/// height did not move (`resolve_normals`). Answers how many were kept.
pub fn resolve_normals(outcome: &mut MergeOutcome, reference: &Landmass) -> usize {
    let mut preserved = 0;
    for (coords, cell) in outcome.cells.iter_mut() {
        let Some(h) = &cell.heights else { continue };
        let l = reference.get(*coords);
        let (mut original, mut base) = (l.and_then(|l| l.normals.as_deref().map(Vec::as_slice)), l.and_then(|l| l.heights.as_deref().map(Vec::as_slice)));
        if original.is_none() || base.is_none_or(|b| b.len() != h.len()) {
            original = None;
            base = None;
        }
        let (n, kept) = resolve_cell_normals(h, original, base);
        cell.normals = Some(n);
        preserved += kept;
    }
    preserved
}

/// The untouched reference cells bordering the merge, brought in so their borders can be
/// reconciled (`add_reference_neighbours`); they are authoritative - never moved.
pub fn add_reference_neighbours(outcome: &mut MergeOutcome, reference: &Landmass) -> BTreeSet<Coords> {
    let existing: Vec<Coords> = outcome.cells.keys().copied().collect();
    let mut added = BTreeSet::new();
    for (x, y) in existing {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let c = (x + dx, y + dy);
            if outcome.cells.contains_key(&c) {
                continue;
            }
            let Some(l) = reference.get(c) else { continue };
            let Some(h) = &l.heights else { continue };
            let mut cell = MergedCell { coords: c, heights: Some(h.to_vec()), ..Default::default() };
            inherit_reference_layers(&mut cell, reference);
            outcome.cells.insert(c, cell);
            added.insert(c);
        }
    }
    added
}

fn check_borders(outcome: &mut MergeOutcome, reference: &Landmass, borrowed: &BTreeSet<Coords>) {
    let written: BTreeMap<Coords, Vec<i64>> = outcome.cells.iter().filter_map(|(c, cell)| cell.heights.clone().map(|h| (*c, h))).collect();
    if written.is_empty() {
        return;
    }
    let mut nearby: BTreeMap<Coords, Vec<i64>> = BTreeMap::new();
    for c in written.keys() {
        for (dx, dy) in [(-1, 0), (1, 0), (0, 1), (0, -1)] {
            let o = (c.0 + dx, c.1 + dy);
            if written.contains_key(&o) || nearby.contains_key(&o) {
                continue;
            }
            if let Some(h) = reference.get(o).and_then(|l| l.heights.as_deref().cloned()) {
                nearby.insert(o, h);
            }
        }
    }
    outcome.seams.tears = find_tears(&written, Some(&nearby), borrowed);
}

fn digest_cell(cell: &MergedCell) -> CellDigest {
    CellDigest { heights: cell.heights.clone(), normals: None, world_map: cell.world_map.clone(), colors: cell.colors.clone(), textures: cell.textures.clone() }
}

fn digest_reference(l: &LandscapeLayers) -> CellDigest {
    CellDigest { heights: l.heights.as_deref().cloned(), normals: None, world_map: l.world_map.as_deref().cloned(), colors: l.colors.as_deref().cloned(), textures: l.textures.as_deref().cloned() }
}

/// Seams, slopes, normals, borders and cleaning, in that order (`finish`, every step on).
/// `loaded` given, cleaning drops what the load order already loads
/// ([`clean_against_loaded`]); otherwise what the masters or a single mod already give
/// ([`clean_landmass`]).
pub fn finish(outcome: &mut MergeOutcome, reference: &Landmass, loaded: Option<&Landmass>) -> Result<(), String> {
    let borrowed = add_reference_neighbours(outcome, reference);
    outcome.borrowed = borrowed.len();
    let mut heights: BTreeMap<Coords, Vec<i64>> = outcome.cells.iter_mut().filter_map(|(c, cell)| cell.heights.take().map(|h| (*c, h))).collect();
    if !heights.is_empty() {
        let anchor: BTreeMap<Coords, Vec<i64>> = reference.cells.iter().filter_map(|(c, l)| l.heights.as_deref().cloned().map(|h| (*c, h))).collect();
        outcome.seams = repair_seams(&mut heights, DEFAULT_FEATHER, Some(&anchor), &borrowed)?;
        outcome.slopes = limit_slopes(&mut heights, &borrowed);
    }
    for (c, h) in heights {
        outcome.cells.get_mut(&c).unwrap().heights = Some(h);
    }
    resolve_normals(outcome, reference);
    check_borders(outcome, reference, &borrowed);
    let merged: BTreeMap<Coords, CellDigest> = outcome.cells.iter().map(|(c, cell)| (*c, digest_cell(cell))).collect();
    let refs: BTreeMap<Coords, CellDigest> = outcome.cells.keys().filter_map(|c| reference.get(*c).map(|l| (*c, digest_reference(l)))).collect();
    let sources: BTreeMap<Coords, Vec<String>> = outcome.cells.iter().map(|(c, cell)| (*c, cell.editors.clone())).collect();
    let originals: BTreeMap<Coords, CellDigest> = outcome.cells.iter().map(|(c, cell)| (*c, cell.sole_source.clone())).collect();
    let (keep, report) = match loaded {
        Some(loaded) => {
            let there: BTreeMap<Coords, CellDigest> = outcome.cells.keys().filter_map(|c| loaded.get(*c).map(|l| (*c, digest_reference(l)))).collect();
            clean_against_loaded(&merged, &there)
        }
        None => clean_landmass(&merged, &refs, &sources, &originals),
    };
    outcome.cleaning = report;
    outcome.cells.retain(|c, _| keep.contains(c));
    Ok(())
}
