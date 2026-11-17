//! From the plugins' changes to the cells to write (`land/pipeline.py`): merge each layer
//! plugin by plugin in load order, fill what no mod touched from the reference, bring in the
//! untouched neighbours, repair the seams, limit the slopes, recompute the normals, check
//! the borders, and drop the cells that change nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::cleaning::{clean_against_loaded, clean_landmass, CellDigest, CleaningReport};
use crate::grid::RelativeGrid;

use crate::landmass::{differences, layer_onto, masked, replace_onto, Coords, IgnoredCells, Landmass, PluginRecords};
use crate::layers::{LandscapeDiff, LandscapeLayers, Layer, ALL_LAYERS};
use crate::merge::{self, merge_layer, ConflictParams};
use crate::meta::{layer, PluginMeta, Strategy};
use crate::seams::{repair_seams, SeamReport, DEFAULT_FEATHER};
use crate::slope::{limit_slopes, SlopeReport};
use crate::textures::KnownTextures;
use crate::grid::Value;

/// One cell of the merge (`MergedCell`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergedCell {
    pub coords: Coords,
    pub heights: Option<Vec<Value>>,
    pub textures: Option<Vec<Value>>,
    pub world_map: Option<Vec<Value>>,
    /// Colors and normals packed (a byte a value where they fit): as `i32` they were most of a
    /// merged cell (Static, 2026-11-17: 228 of 270 MB on `big_order` at the finish step), and one
    /// taken whole from the reference or the winning record shares that record's grid.
    pub colors: Option<crate::layers::Grid>,
    pub normals: Option<crate::layers::Grid>,
    /// Normals an explicit `Resolve` made, and normal-only patches, each with its geometry.
    pub normal_snapshots: Vec<crate::heights::NormalSnapshot>,
    /// The records of the plugins that touched the cell, in load order: normals and heights.
    pub normal_predecessors: Vec<crate::heights::NormalSnapshot>,
    /// The plugins that changed it, in load order.
    pub editors: Vec<String>,
    /// Height vertices more than one plugin moved, and those settled far from an intent.
    pub contested: usize,
    pub major: usize,
    pub new_land: bool,
    /// Each layer as the one plugin that alone supplied it has it, for cleaning.
    pub sole_source: CellDigest,
    /// The heights its world map was made for, where that is not the merge's own: the masters'
    /// for a map no plugin changed, the winning record's for one taken from it. Only the 81 the
    /// map reads (`emit::world_map_vertex`).
    pub world_map_heights: Option<Vec<Value>>,
}

/// The whole merge (`MergeOutcome`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeOutcome {
    pub cells: BTreeMap<Coords, MergedCell>,
    /// The world maps are left as the merge has them (the OpenMW fork): no entry is made again.
    pub keep_world_map: bool,
    /// Entries of authored world maps made again because a repair moved the height under them.
    pub world_map_regenerated: usize,
    /// Cells the repairs leave alone and never bring in ([`IgnoredCells`]).
    pub untouchable: BTreeSet<Coords>,
    /// What the last height repair did (`steps::fit_steps`).
    pub steps: crate::steps::StepReport,
    pub seams: SeamReport,
    pub slopes: SlopeReport,
    pub cleaning: CleaningReport,
    pub skipped_plugins: Vec<(String, String)>,
    pub borrowed: usize,
    /// Cells brought in to repair a seam the load order already has ([`ExistingSeams`]).
    pub existing_seam_cells: usize,
    /// Seams between two borrowed cells mended (`mend_between_borrowed`), and the cells they moved.
    pub existing_seam_borrowed: BTreeSet<Coords>,
    /// Which cells those were: their borders with terrain outside the merge are not checked.
    pub existing_seam_set: BTreeSet<Coords>,
    /// Those cells' heights as the terrain had them, finalized.
    pub existing_seam_original: BTreeMap<Coords, Vec<Value>>,
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

fn has_layer(cell: &MergedCell, which: Layer) -> bool {
    match which {
        Layer::Heights => cell.heights.is_some(),
        Layer::Textures => cell.textures.is_some(),
        Layer::WorldMap => cell.world_map.is_some(),
        Layer::Colors => cell.colors.is_some(),
        Layer::Normals => cell.normals.is_some(),
    }
}

/// A layer the merge worked out, set: the packed ones packed.
fn set_values(cell: &mut MergedCell, which: Layer, values: Vec<Value>) {
    match which {
        Layer::Heights => cell.heights = Some(values),
        Layer::Textures => cell.textures = Some(values),
        Layer::WorldMap => cell.world_map = Some(values),
        Layer::Colors => cell.colors = Some(crate::layers::pack_grid(values)),
        Layer::Normals => cell.normals = Some(crate::layers::pack_grid(values)),
    }
}

/// A layer taken whole from a record, where the cell has none: the packed ones share its grid.
fn fill_from(cell: &mut MergedCell, which: Layer, from: Option<&crate::layers::Grid>) {
    let Some(g) = from else { return };
    if has_layer(cell, which) {
        return;
    }
    match which {
        Layer::Colors => cell.colors = Some(g.clone()),
        Layer::Normals => cell.normals = Some(g.clone()),
        _ => set_values(cell, which, g.to_vec()),
    }
}

fn digest_slot(d: &mut CellDigest, which: Layer) -> &mut Option<u64> {
    match which {
        Layer::Heights => &mut d.heights,
        Layer::Textures => &mut d.textures,
        Layer::WorldMap => &mut d.world_map,
        Layer::Colors => &mut d.colors,
        Layer::Normals => &mut d.normals,
    }
}

/// The layers folded for one plugin's change: the four of [`FOLDED`], then the normals, which
/// are authored data of their own and go by the plugin's height strategy (`merge_landscape_diff`).
///
/// What `Auto` means is the model's. Against the masters it is the original Merged Lands':
/// heights, normals, colors and the world map are blended where two plugins both changed a
/// vertex (its `apply_merge_strategy` gives them `Resolve`), and textures are the later
/// plugin's. Against the load order it is the OpenMW fork's: the later plugin's for every layer
/// (its `load_order_auto_strategy`). `default` is the merge's own strategy for heights, when one
/// is asked for; a cell no master has takes the later plugin's heights either way.
fn folded_with_normals(meta: Option<&PluginMeta>, new_land: bool, default: Strategy, model: DiffModel) -> impl Iterator<Item = (Layer, u32, Strategy)> + '_ {
    let auto = move |bit: u32| match model {
        DiffModel::Masters if bit != layer::TEXTURES => Strategy::Resolve,
        _ => Strategy::Overwrite,
    };
    let heights = meta.map(|m| m.strategy_for(layer::VERTEX_HEIGHTS)).unwrap_or_default();
    let for_normals = match heights {
        Strategy::Auto => auto(layer::VERTEX_NORMALS),
        // Normals have no surface to weigh an edit by: blended plainly.
        Strategy::Curvature => Strategy::Resolve,
        other => other,
    };
    FOLDED
        .into_iter()
        .map(move |(which, bit)| {
            let mut chosen = meta.map(|m| m.strategy_for(bit)).unwrap_or_default();
            if chosen == Strategy::Auto {
                chosen = if bit == layer::VERTEX_HEIGHTS && default != Strategy::Auto { default } else { auto(bit) };
                // A cell no master has, against what loaded before it: the later plugin's heights
                // (the Python's rule). Against the masters it is blended as any other, from zero,
                // as Merged Lands does (`LandscapeDiff::from_difference` with no reference;
                // Static, 2026-11-17).
                if new_land && bit == layer::VERTEX_HEIGHTS && model == DiffModel::LoadOrder {
                    chosen = Strategy::Overwrite;
                }
            }
            (which, bit, chosen)
        })
        .chain(std::iter::once((Layer::Normals, layer::VERTEX_NORMALS, for_normals)))
}

/// What an explicit `Resolve` made of two plugins' normals where it is neither's: kept with the
/// heights merged so far, so it can be told later whether that terrain still stands
/// (`retain_resolved_normal_source`).
fn keep_resolved_normals(cell: &mut MergedCell, grids: &HashMap<Layer, RelativeGrid>, old: &RelativeGrid, new: &RelativeGrid, base_heights: Option<&crate::layers::Grid>) {
    let Some(merged) = grids.get(&Layer::Normals) else { return };
    let heights = match grids.get(&Layer::Heights) {
        Some(g) => crate::layers::pack_grid(g.flat()),
        None => match base_heights {
            Some(h) => h.clone(),
            None => return,
        },
    };
    let at = |g: &RelativeGrid, i: usize| [g.reference[i * 3] + g.delta[i * 3], g.reference[i * 3 + 1] + g.delta[i * 3 + 1], g.reference[i * 3 + 2] + g.delta[i * 3 + 2]];
    let selected: Vec<bool> = (0..merged.changed.len()).map(|i| old.changed[i] && new.changed[i] && at(merged, i) != at(old, i) && at(merged, i) != at(new, i)).collect();
    if selected.iter().any(|&s| s) {
        cell.normal_snapshots.push(crate::heights::NormalSnapshot { heights, normals: crate::layers::pack_grid(merged.flat()), selected: Some(selected) });
    }
}

fn fold_change(cell: &mut MergedCell, grids: &mut HashMap<Layer, RelativeGrid>, change: &LandscapeDiff, meta: Option<&PluginMeta>, default: Strategy, base_heights: Option<&crate::layers::Grid>) -> Result<(), String> {
    for (which, bit, chosen) in folded_with_normals(meta, cell.new_land, default, DiffModel::Masters) {
        let Some(source) = change.grid(which) else { continue };
        let first = !grids.contains_key(&which);
        let resolving = (which == Layer::Normals && chosen == Strategy::Resolve).then(|| grids.get(&which).cloned()).flatten();
        let (merged, contested, major) = match grids.get(&which) {
            None => (source.clone(), 0, 0),
            Some(previous) => {
                let (m, report) = merge_layer(bit, previous, source, chosen, ConflictParams::default())?;
                (m, report.contested, report.major)
            }
        };
        grids.insert(which, merged);
        *digest_slot(&mut cell.sole_source, which) = if first { Some(crate::cleaning::digest_of(source.reference.iter().zip(&source.delta).map(|(a, b)| a + b))) } else { None };
        if bit == layer::VERTEX_HEIGHTS {
            cell.contested += contested;
            cell.major += major;
        }
        if let Some(old) = resolving {
            keep_resolved_normals(cell, grids, &old, source, base_heights);
        }
    }
    Ok(())
}

/// What the reference holds for a layer no mod changed (`inherit_reference_layers`).
pub fn inherit_reference_layers(cell: &mut MergedCell, reference: &Landmass) {
    let Some(l) = reference.get(cell.coords) else { return };
    for which in [Layer::Heights, Layer::Normals, Layer::Textures, Layer::WorldMap, Layer::Colors] {
        fill_from(cell, which, l.get(which).as_ref());
    }
}

/// What a plugin's change is measured against (the OpenMW fork's choice, and the original's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffModel {
    /// Against the masters (Merged Lands, the Python): a plugin's change is what it does to
    /// the vanilla terrain, so two mods that reshape different parts of a cell both survive
    /// even where the later one carries vanilla ground over the earlier one's work.
    Masters,
    /// Against what loaded before it (the OpenMW fork, 0.2): masters plus every earlier
    /// plugin's allowed layers. A plugin's change is what it does to the load order, so the
    /// result follows the load order's winners, with the seams repaired and each
    /// `.mergedlands.toml`'s choices on top.
    #[default]
    LoadOrder,
}

/// One plugin's change folded in the load-order model: each layer merged onto the cell's
/// running merge by value (`merge::merge_onto`), the masters' layer the start where they
/// have one.
fn fold_rolling(cell: &mut MergedCell, grids: &mut HashMap<Layer, RelativeGrid>, change: &LandscapeDiff, reference: &Landmass, meta: Option<&PluginMeta>, default: Strategy, base_heights: Option<&crate::layers::Grid>) -> Result<(), String> {
    for (which, bit, chosen) in folded_with_normals(meta, cell.new_land, default, DiffModel::LoadOrder) {
        let Some(source) = change.grid(which) else { continue };
        let resolving = (which == Layer::Normals && chosen == Strategy::Resolve).then(|| grids.get(&which).cloned()).flatten();
        let acc = match grids.entry(which) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let start = reference.get(cell.coords).and_then(|l| l.get(which).as_deref().map(crate::layers::Packed::to_vec)).filter(|v| v.len() == source.reference.len());
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
        if let Some(old) = resolving {
            keep_resolved_normals(cell, grids, &old, source, base_heights);
        }
    }
    Ok(())
}

/// Every plugin's changes merged, in load order (`merge_landmass`). `loaded`, when given,
/// gathers the terrain the engine itself would load from these plugins (settings ignored,
/// previous merges left out), for the load-order cleaning; it starts as the masters'. The first
/// `early` plugins are listed before the masters: they are merged like the rest, but a master's
/// record at a cell loads over theirs, so theirs is what loads only where no master has the cell.
#[allow(clippy::too_many_arguments)]
pub fn merge_landmass(
    reference: &Landmass,
    mut plugins: Vec<PluginRecords>,
    known: &mut KnownTextures,
    metas: &HashMap<String, PluginMeta>,
    strategy: Strategy,
    model: DiffModel,
    loaded: Option<&mut Landmass>,
    early: usize,
    ignored: &IgnoredCells,
) -> Result<MergeOutcome, String> {
    /* Every cell is merged on its own - what a plugin does to one cell never reaches
       another - so the cells are shared out over the cores (Static, 2026-10-27: the merge
       took 65 s on one core for a 621-plugin order). The plugins are decoded first, in load
       order (the land textures each brings are numbered as they are met); then each cell
       runs through the plugins that touch it, in the same order, against its own rolling
       and loaded copies; the results are gathered back in coordinate order. */
    let mut outcome = MergeOutcome::default();
    // Each touch: the plugin's index, the layers it may change, its decoded cell.
    type Touch = (usize, u32, LandscapeLayers);
    let mut by_cell: BTreeMap<Coords, Vec<Touch>> = BTreeMap::new();
    /* The plugins' land textures numbered in load order (the table is shared, and a
       texture's number is the order it was first met), then their records decoded across
       the cores, then gathered in load order - their notes too, so the log reads as it did
       one plugin after another. */
    // Each plugin's records go with its work and are freed once it is decoded: only its name is
    // read after that, and a big order's records held to the end were a copy of all its land.
    // The plugin's index, the layers it may change, its texture numbering, its records.
    type Work = (usize, u32, HashMap<Value, Value>, Vec<crate::read::Record>);
    let mut work: Vec<Work> = Vec::new();
    for (index, plugin) in plugins.iter_mut().enumerate() {
        let meta = metas.get(&plugin.name);
        if meta.is_some_and(PluginMeta::is_previous_merge) {
            outcome.skipped_plugins.push((plugin.name.clone(), "a previous merge".into()));
            plugin.records = Vec::new();
            continue;
        }
        let allowed = meta.map(PluginMeta::allowed_layers).unwrap_or(ALL_LAYERS);
        if allowed == 0 {
            outcome.skipped_plugins.push((plugin.name.clone(), "every layer excluded".into()));
            if loaded.is_none() {
                plugin.records = Vec::new();
                continue;
            }
        }
        let mapping = known.observe(&plugin.name, &plugin.records);
        work.push((index, allowed, mapping, std::mem::take(&mut plugin.records)));
    }
    let decoded_all = par_each(work, |(index, allowed, mapping, records)| {
        let plugin = &plugins[index];
        let mut notes = Vec::new();
        let cells = crate::landmass::decoded(&plugin.name, &records, &mapping, &mut notes);
        drop(records);
        (index, allowed, cells, notes)
    });
    for (index, allowed, cells, notes) in decoded_all {
        // As one after another: this plugin's notes, then its error stops the merge.
        outcome.notes.extend(notes);
        for c in cells? {
            by_cell.entry(c.coords).or_default().push((index, allowed, c));
        }
    }
    let want_loaded = loaded.is_some();
    let loaded_ref: Option<&Landmass> = loaded.as_deref();
    // A cell's merge comes back flat, its working grids dropped on the core that made them: held
    // for every cell until the last was done, they were most of the merge's memory (the normals'
    // alone about 105 KB a cell; Static's order has 4,668). The flag: a plugin's world map merged.
    type CellOut = (Option<(MergedCell, bool)>, Option<LandscapeLayers>, Option<String>);
    let one = |coords: Coords, touches: Vec<Touch>| -> Result<CellOut, String> {
        let single = |l: Option<&LandscapeLayers>| {
            let mut m = Landmass::default();
            if let Some(l) = l {
                m.cells.insert(coords, l.clone());
            }
            m
        };
        let mut rolling = (model == DiffModel::LoadOrder).then(|| single(reference.get(coords)));
        let mut here = want_loaded.then(|| single(loaded_ref.and_then(|l| l.get(coords))));
        let mut cell: Option<MergedCell> = None;
        let mut grids: HashMap<Layer, RelativeGrid> = HashMap::new();
        let (mut snapshots, mut predecessors): (Vec<crate::heights::NormalSnapshot>, Vec<crate::heights::NormalSnapshot>) = (Vec::new(), Vec::new());
        for (index, allowed, c) in touches {
            let plugin = &plugins[index];
            // The decoded cell is moved into the last copy that keeps it, cloned only when
            // both the rolling and the loaded landmass need one.
            let mut c = Some(c);
            // A cell left out for this plugin: nothing of it is merged, and it still loads.
            let allowed = if ignored.skips(&plugin.name, coords) { 0 } else { allowed };
            if allowed != 0 {
                let meta = metas.get(&plugin.name);
                let against = rolling.as_ref().unwrap_or(reference);
                let base_heights = against.get(coords).and_then(|l| l.heights.clone());
                // This record's normals with the heights they were made for: its own, or for a
                // patch of normals alone the terrain under it now (the fork's `d8e5d87`).
                if allowed & layer::VERTEX_NORMALS != 0
                    && let Some(mine) = c.as_ref()
                    && mine.declared & layer::VERTEX_NORMALS != 0
                    && let Some(n) = &mine.normals
                    && let Some(h) = mine.heights.clone().or_else(|| base_heights.clone())
                {
                    let patch_alone = mine.heights.is_none();
                    let snapshot = crate::heights::NormalSnapshot { heights: h, normals: n.clone(), selected: None };
                    if patch_alone { snapshots.push(snapshot) } else { predecessors.push(snapshot) }
                }
                for change in differences(against, &plugin.name, std::slice::from_ref(c.as_ref().expect("the cell")), allowed)? {
                    let cell = cell.get_or_insert_with(|| MergedCell { coords, new_land: change.new_land, ..Default::default() });
                    cell.editors.push(plugin.name.clone());
                    cell.new_land = cell.new_land || change.new_land;
                    match model {
                        DiffModel::Masters => fold_change(cell, &mut grids, &change, meta, strategy, base_heights.as_ref())?,
                        DiffModel::LoadOrder => fold_rolling(cell, &mut grids, &change, reference, meta, strategy, base_heights.as_ref())?,
                    }
                }
                if let Some(r) = rolling.as_mut() {
                    let own = if here.is_some() { c.clone() } else { c.take() };
                    layer_onto(r, masked(own.expect("the cell"), allowed), &plugin.name);
                }
            }
            if let (Some(l), Some(c)) = (here.as_mut(), c.take()) {
                // What loads: the whole record of the last plugin to carry the cell. A plugin
                // listed before the masters is not the last where a master has the cell (the
                // early ones come first here, so all that is there yet is the masters').
                if index >= early || l.get(coords).is_none() {
                    replace_onto(l, c, &plugin.name);
                }
            }
        }
        if let Some(cell) = cell.as_mut() {
            cell.normal_snapshots.extend(snapshots);
            cell.normal_predecessors = predecessors;
        }
        let winner = here.as_mut().and_then(|l| l.sources.remove(&coords));
        let map_merged = grids.contains_key(&Layer::WorldMap);
        let cell = cell.map(|mut c| {
            for which in [Layer::Heights, Layer::Normals, Layer::Textures, Layer::WorldMap, Layer::Colors] {
                if let Some(g) = grids.remove(&which) {
                    set_values(&mut c, which, g.flat());
                }
            }
            (c, map_merged)
        });
        Ok((cell, here.and_then(|mut l| l.cells.remove(&coords)), winner))
    };
    // Each cell's touches handed to the core that takes it (taken out, so they move).
    let cells: Vec<(Coords, std::sync::Mutex<Vec<Touch>>)> = by_cell.into_iter().map(|(c, t)| (c, std::sync::Mutex::new(t))).collect();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(cells.len().max(1));
    /* Each core takes the next cell as it finishes one, rather than a fixed share: the cells
       a load order edits most are next to each other (a city, a coast), and a share of them
       kept one core busy while the others had finished. The answers are put back in the
       cells' order before they are used, so nothing depends on which core took which. */
    let next = std::sync::atomic::AtomicUsize::new(0);
    let parts: Vec<Vec<(usize, Result<CellOut, String>)>> = std::thread::scope(|s| {
        let jobs: Vec<_> = (0..threads)
            .map(|_| {
                let (cells, next, one) = (&cells, &next, &one);
                s.spawn(move || {
                    let mut got = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((c, t)) = cells.get(i) else { break };
                        let touches = std::mem::take(&mut *t.lock().unwrap_or_else(|p| p.into_inner()));
                        let r = one(*c, touches);
                        let stop = r.is_err();
                        got.push((i, r));
                        if stop {
                            break;
                        }
                    }
                    got
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap_or_else(|_| vec![(usize::MAX, Err("a merge thread panicked".to_string()))])).collect()
    });
    let mut answers: Vec<(usize, Result<CellOut, String>)> = parts.into_iter().flatten().collect();
    answers.sort_by_key(|(i, _)| *i);
    let mut loaded = loaded;
    {
        for (i, answer) in answers {
            let coords = cells.get(i).map(|(c, _)| *c).unwrap_or_default();
            let (merged, here, winner) = answer?;
            // A texture-only (or color-only) record must not replace the record of a plugin left
            // out of this cell where that record has the cell's heights: the whole record is
            // replaced, so its terrain would go (`remove_heightless_overrides_of_ignored_cells`).
            let ignored_winner_has_heights = winner.as_deref().or_else(|| loaded.as_deref().and_then(|l| l.sources.get(&coords)).map(String::as_str)).is_some_and(|n| ignored.skips(n, coords))
                && here.as_ref().or_else(|| loaded.as_deref().and_then(|l| l.get(coords))).is_some_and(|l| l.heights.is_some() && l.declared & layer::VERTEX_HEIGHTS != 0);
            if let (Some(l), Some(h)) = (loaded.as_deref_mut(), here) {
                l.cells.insert(coords, h);
                if let Some(w) = winner {
                    l.sources.insert(coords, w);
                }
            }
            let Some((mut cell, map_merged)) = merged else { continue };
            // A world map no plugin changed is the masters', made for the masters' heights.
            if !map_merged {
                cell.world_map_heights = reference.get(coords).and_then(|l| l.heights.as_deref()).map(map_samples);
            }
            inherit_reference_layers(&mut cell, reference);
            if ignored_winner_has_heights && cell.heights.is_none() {
                outcome.notes.push(format!("cell ({}, {}): not written, it has no heights of its own and would replace the terrain of a plugin left out of this cell", coords.0, coords.1));
                continue;
            }
            outcome.cells.insert(coords, cell);
        }
    }
    // The cells nothing may touch from here on: those left out everywhere, and those whose
    // loaded record is a plugin's left out there (its terrain stays as the game has it).
    outcome.untouchable = ignored.everywhere.clone();
    if let Some(l) = loaded.as_deref() {
        for (name, cells) in &ignored.by_plugin {
            outcome.untouchable.extend(cells.iter().filter(|c| l.sources.get(*c).is_some_and(|s| s.eq_ignore_ascii_case(name))));
        }
    }
    Ok(outcome)
}

/// Each written cell's normals from its final heights, an authored normal kept where the
/// three heights it is made from did not move (`resolve_normals`): first the record the engine
/// loads at the cell (`loaded`, when the merge is judged against the load order), then the
/// masters'. Answers how many were kept.
pub fn resolve_normals(outcome: &mut MergeOutcome, reference: &Landmass, loaded: Option<&Landmass>) -> usize {
    // Each cell on its own: shared out over the cores.
    let cells: Vec<(&Coords, &mut MergedCell)> = outcome.cells.iter_mut().collect();
    par_each(cells, |(coords, cell)| {
        let Some(h) = &cell.heights else { return 0 };
        let read = |l: Option<&LandscapeLayers>| l.and_then(|l| Some(crate::heights::NormalSnapshot { heights: l.heights.clone()?, normals: l.normals.clone()?, selected: None }));
        // The fork's order: the record the game loads, what `Resolve` made and the patches of
        // normals alone, then the plugins that touched the cell from the last back, the masters last.
        let (winner, base) = (read(loaded.and_then(|l| l.get(*coords))), read(reference.get(*coords)));
        // The cell's sources are this step's alone: freed with it (the predecessors were 190 MB
        // of `big_order`'s merge, held to here).
        let (snapshots, predecessors) = (std::mem::take(&mut cell.normal_snapshots), std::mem::take(&mut cell.normal_predecessors));
        let sources: Vec<&crate::heights::NormalSnapshot> = winner.iter().chain(&snapshots).chain(predecessors.iter().rev()).chain(base.iter()).collect();
        let authored = cell.normals.as_deref().map(crate::layers::Packed::to_vec);
        let (n, kept) = crate::heights::finalize_normals(h, authored.as_deref(), &sources);
        // Normals that come out as they went in keep their grid, which a record may share.
        let n = crate::layers::Packed::pack(n);
        if !cell.normals.as_deref().is_some_and(|old| old.same_values(&n)) {
            cell.normals = Some(std::sync::Arc::new(n));
        }
        kept
    })
    .into_iter()
    .sum()
}

/// The 81 heights a world map is made from, out of a cell's 65 x 65.
fn map_samples(h: &crate::layers::Packed) -> Vec<Value> {
    (0..crate::grid::WNAM_SIZE * crate::grid::WNAM_SIZE).map(|k| h.at(crate::emit::world_map_vertex(k)).unwrap_or(0)).collect()
}

/// `f` over `items` on every core, the answers in the items' order.
pub(crate) fn par_each<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len().max(1));
    if threads <= 1 {
        return items.into_iter().map(f).collect();
    }
    let per = items.len().div_ceil(threads);
    let mut chunks: Vec<Vec<T>> = Vec::new();
    let mut it = items.into_iter().peekable();
    while it.peek().is_some() {
        chunks.push(it.by_ref().take(per).collect());
    }
    let f = &f;
    std::thread::scope(|s| {
        let jobs: Vec<_> = chunks.into_iter().map(|c| s.spawn(move || c.into_iter().map(f).collect::<Vec<R>>())).collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    })
}

/// Seams the terrain already has, repaired as well as those the merge makes (off unless
/// asked for): two neighbouring cells that disagree about the vertices they share, in terrain
/// no plugin of the merge changed. Both are brought in free to move, and the border is feathered
/// like any other. Merged Lands does this for the whole working terrain; Wraithguard by default
/// repairs only the borders of cells a plugin changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExistingSeams {
    /// The widest gap, in world units, a border may have and still be repaired. A wider one is
    /// taken to be meant (a cliff, a map edge): 24 is three eight-unit steps, the few units
    /// of disagreement a stray rounding leaves.
    pub max_gap: Value,
    /// Cells whose borders are never repaired, either side of them.
    pub except: BTreeSet<Coords>,
}

impl Default for ExistingSeams {
    fn default() -> Self {
        ExistingSeams { max_gap: 24, except: BTreeSet::new() }
    }
}

/// Cells written as `X,Y`, separated by spaces, commas-after-a-pair or semicolons
/// (`-10,-47; 3,4`): the cells [`ExistingSeams::except`] leaves alone, as a page or a settings file
/// has them. A piece that is not a cell is an error, not skipped, because a skipped exception is a
/// border repaired that was meant to stay.
pub fn parse_cell_list(text: &str) -> Result<BTreeSet<Coords>, String> {
    let mut out = BTreeSet::new();
    // "5, 6" is one cell: no space is taken to end a piece beside a comma.
    let mut tight = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == ',' {
            tight.truncate(tight.trim_end().len());
        }
        if ch.is_whitespace() && tight.ends_with(',') {
            continue;
        }
        tight.push(ch);
    }
    for piece in tight.split(|c: char| c == ';' || c.is_whitespace()).map(str::trim).filter(|p| !p.is_empty()) {
        let cell = piece.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<i64>().ok()?, y.trim().parse::<i64>().ok()?)));
        match cell {
            Some(c) => {
                out.insert(c);
            }
            None => return Err(format!("\"{piece}\" is not a cell: write it as X,Y, for example -10,-47")),
        }
    }
    Ok(out)
}

/// How many world units the shared vertices of two neighbours disagree by at most, `a` the cell
/// to the west (or south) of `b`; `None` if either has no heights.
fn border_gap(a: &LandscapeLayers, b: &LandscapeLayers, east: bool) -> Option<Value> {
    let (ha, hb) = (a.heights.as_deref()?, b.heights.as_deref()?);
    let last = crate::grid::LAND_SIZE - 1;
    let mut widest = 0;
    for t in 0..crate::grid::LAND_SIZE {
        let (ia, ib) = if east { (t * crate::grid::LAND_SIZE + last, t * crate::grid::LAND_SIZE) } else { (last * crate::grid::LAND_SIZE + t, t) };
        widest = widest.max((ha.at(ia)? - hb.at(ib)?).abs());
    }
    Some(widest)
}

/// The cells either side of every existing seam [`ExistingSeams`] allows, brought into the merge
/// from `base` (what loads, or the masters), unmoved. A seam between a cell the merge has and a
/// neighbour is left to the borders it repairs anyway, except against one it only borrowed
/// (`borrowed`, held fixed): the neighbour beyond it is brought in, free to move, to meet it.
/// Answers how many were brought in.
pub fn add_existing_seam_cells(outcome: &mut MergeOutcome, base: &Landmass, rule: &ExistingSeams, borrowed: &BTreeSet<Coords>) -> usize {
    let mut wanted: BTreeSet<Coords> = BTreeSet::new();
    let has = |c: &Coords| outcome.cells.contains_key(c);
    for (&(x, y), l) in &base.cells {
        for (east, other) in [(true, (x + 1, y)), (false, (x, y + 1))] {
            let here = (x, y);
            if rule.except.contains(&here) || rule.except.contains(&other) || outcome.untouchable.contains(&here) || outcome.untouchable.contains(&other) {
                continue;
            }
            // Neither in the merge, or exactly one a borrowed cell and the other free.
            let free_here = !has(&here);
            let free_there = !has(&other);
            let against_borrowed = (free_here && borrowed.contains(&other)) || (free_there && borrowed.contains(&here));
            if !((free_here && free_there) || against_borrowed) {
                continue;
            }
            let Some(n) = base.get(other) else { continue };
            if matches!(border_gap(l, n, east), Some(g) if g > 0 && g <= rule.max_gap) {
                for c in [here, other] {
                    if !has(&c) {
                        wanted.insert(c);
                    }
                }
            }
        }
    }
    for &c in &wanted {
        let Some(h) = base.get(c).and_then(|l| l.heights.as_deref()) else { continue };
        // As the terrain has them; put on the cell's phase once that is known.
        outcome.existing_seam_original.insert(c, h.to_vec());
        let mut cell = MergedCell { coords: c, heights: Some(h.to_vec()), ..Default::default() };
        inherit_reference_layers(&mut cell, base);
        outcome.cells.insert(c, cell);
    }
    outcome.existing_seam_cells = wanted.len();
    outcome.existing_seam_set = wanted.clone();
    wanted.len()
}

/// Existing seams between two cells the merge only borrowed (Static, 2026-11-16, the one border
/// left open on the audit's order at 24 units). A borrowed cell is held where it is, so that the
/// cells the merge changed meet it ([`add_reference_neighbours`]), and the existing-seam repair
/// only takes a seam with a side outside the merge ([`add_existing_seam_cells`]): one between two
/// borrowed cells had no one to mend it. Here, where the two are no more than `rule.max_gap`
/// apart, each vertex only those two cells hold (the border without its two ends: an end is a
/// corner other cells hold too) is set to the two heights' mean, so each moves half the gap at
/// most - 12 units at 24 - along one line of vertices. Answers the cells it moved.
fn mend_between_borrowed(heights: &mut BTreeMap<Coords, Vec<Value>>, borrowed: &BTreeSet<Coords>, rule: &ExistingSeams, untouchable: &BTreeSet<Coords>) -> BTreeSet<Coords> {
    let n = crate::grid::LAND_SIZE;
    let last = n - 1;
    let mut moved = BTreeSet::new();
    for &a in borrowed {
        for (east, b) in [(true, (a.0 + 1, a.1)), (false, (a.0, a.1 + 1))] {
            if !borrowed.contains(&b) || [a, b].iter().any(|c| rule.except.contains(c) || untouchable.contains(c)) {
                continue;
            }
            let (Some(ha), Some(hb)) = (heights.get(&a), heights.get(&b)) else { continue };
            let at = |t: usize| if east { (t * n + last, t * n) } else { (last * n + t, t) };
            let widest = (1..last).map(|t| { let (i, j) = at(t); (ha[i] - hb[j]).abs() }).max().unwrap_or(0);
            if widest == 0 || widest > rule.max_gap {
                continue;
            }
            let means: Vec<(usize, usize, Value)> = (1..last).map(|t| { let (i, j) = at(t); (i, j, (ha[i] + hb[j]).div_euclid(2)) }).collect();
            for (i, j, m) in means {
                heights.get_mut(&a).expect("a")[i] = m;
                heights.get_mut(&b).expect("b")[j] = m;
            }
            moved.insert(a);
            moved.insert(b);
        }
    }
    moved
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
    let written: BTreeMap<Coords, Vec<Value>> = outcome.cells.iter().filter_map(|(c, cell)| cell.heights.clone().map(|h| (*c, h))).collect();
    if written.is_empty() {
        return;
    }
    let mut nearby: BTreeMap<Coords, Vec<Value>> = BTreeMap::new();
    for c in written.keys() {
        for (dx, dy) in [(-1, 0), (1, 0), (0, 1), (0, -1)] {
            let o = (c.0 + dx, c.1 + dy);
            if written.contains_key(&o) || nearby.contains_key(&o) {
                continue;
            }
            if let Some(h) = reference.get(o).and_then(|l| l.heights.as_deref().map(crate::layers::Packed::to_vec)) {
                // As the game loads it.
                nearby.insert(o, h);
            }
        }
    }
    outcome.seams.tears = crate::seams::find_tears_except(&written, Some(&nearby), borrowed, &outcome.existing_seam_set, &outcome.existing_seam_original);
}

/// The borders of the cells written that meet terrain the game loads off the eight-unit grid.
/// A record's heights are its offset and steps of eight units, so a neighbor saved with a
/// fractional offset (3,204, say) cannot be met exactly by a cell written on the grid (3,200):
/// the merge works with that neighbor floored, and [`check_borders`] with it, so the few units
/// between them are looked for here, against the neighbor as it loads. `merged` is the cells
/// the merge had before cleaning, for a merge not judged against the load order (there a cell
/// cleaned away may load as a plugin's own, which the masters do not say).
fn off_grid_borders(outcome: &MergeOutcome, reference: &Landmass, loaded: Option<&Landmass>, merged: &BTreeSet<Coords>) -> Vec<crate::seams::Tear> {
    let base = loaded.unwrap_or(reference);
    let mut found = Vec::new();
    for (c, cell) in &outcome.cells {
        let Some(mine) = cell.heights.as_deref() else { continue };
        for (dx, dy) in [(-1, 0), (1, 0), (0, 1), (0, -1)] {
            let o = (c.0 + dx, c.1 + dy);
            if outcome.cells.contains_key(&o) || merged.contains(&o) {
                continue;
            }
            let Some(theirs) = base.get(o).and_then(|l| l.heights.as_deref()) else { continue };
            let (mut count, mut worst) = (0, 0);
            for t in 0..crate::grid::LAND_SIZE {
                let (a, b) = crate::seams::side_vertex(dx, dy, t);
                let Some(real) = theirs.at(b) else { continue };
                // Only what the floor hid: a border left apart on purpose is not this.
                if mine[a] != real && (mine[a] - real).abs() < crate::grid::HEIGHT_SCALE {
                    count += 1;
                    worst = worst.max((mine[a] - real).abs());
                }
            }
            if count > 0 {
                found.push(crate::seams::Tear { left: *c, right: Some(o), vertices: count, worst });
            }
        }
    }
    found.sort_by_key(|t| -t.worst);
    found
}

fn digest_cell(cell: &MergedCell) -> CellDigest {
    {
        let d = |v: &Option<Vec<Value>>| v.as_deref().map(crate::cleaning::digest);
        // The same digest as of the values unpacked (`Packed::digest`).
        let p = |v: &Option<crate::layers::Grid>| v.as_deref().map(crate::layers::Packed::digest);
        CellDigest { heights: d(&cell.heights), normals: p(&cell.normals), world_map: d(&cell.world_map), colors: p(&cell.colors), textures: d(&cell.textures) }
    }
}

fn digest_reference(l: &LandscapeLayers) -> CellDigest {
    {
        let d = |v: &Option<crate::layers::Grid>| v.as_deref().map(crate::layers::Packed::digest);
        // Heights as the game loads them: a merged cell keeps its piece's phase, so one the
        // merge did not change still compares equal, fractional offset or not.
        let heights = d(&l.heights);
        CellDigest { heights, normals: d(&l.normals), world_map: d(&l.world_map), colors: d(&l.colors), textures: d(&l.textures) }
    }
}

/// Seams, slopes, normals, borders and cleaning, in that order (`finish`, every step on).
/// `loaded` given, cleaning drops what the load order already loads
/// ([`clean_against_loaded`]); otherwise what the masters or a single mod already give
/// ([`clean_landmass`]).
pub fn finish(outcome: &mut MergeOutcome, reference: &Landmass, loaded: Option<&Landmass>) -> Result<(), String> {
    finish_timed(outcome, reference, loaded).map(|_| ())
}

/// The steps of [`finish`], each with the time it took (for the merge's log: on a real load
/// order the finish took longer than the merge, 2026-11-10).
pub type FinishTimes = Vec<(&'static str, std::time::Duration)>;

/// [`finish`], answering how long each of its steps took.
pub fn finish_timed(outcome: &mut MergeOutcome, reference: &Landmass, loaded: Option<&Landmass>) -> Result<FinishTimes, String> {
    finish_with(outcome, reference, loaded, None)
}

/// [`finish_timed`], also repairing the seams the terrain already has where `existing` says to.
pub fn finish_with(outcome: &mut MergeOutcome, reference: &Landmass, loaded: Option<&Landmass>, existing: Option<&ExistingSeams>) -> Result<FinishTimes, String> {
    let mut times: FinishTimes = Vec::new();
    let mut at = std::time::Instant::now();
    let mut lap = |name: &'static str| {
        let now = std::time::Instant::now();
        times.push((name, now - at));
        at = now;
    };
    // The ring round the cells the merge changed first; the cells of an existing seam come after
    // and have none of their own: their other borders, with neighbours the merge leaves out, stay
    // as the terrain has them (a wider gap there is taken to be meant).
    // First, as the OpenMW fork does (`complete_landmass_layers`, before its seam repair): a cell
    // written replaces the record the engine would otherwise load, whole, so a layer the merge
    // has nothing for is the winning record's own, or the saved record would drop it for the
    // engine's default. That record's layers are taken as the game has them, whatever its
    // plugin's settings leave out of the merge: the settings say what is merged, and this is
    // what keeps the cell as it loads. Filled here, they go through the repairs like the rest,
    // and a cell that then is only what loads is cleaned away.
    if let Some(loaded) = loaded {
        for (c, cell) in outcome.cells.iter_mut() {
            let Some(winner) = loaded.get(*c) else { continue };
            if cell.world_map.is_none() && winner.world_map.is_some() {
                cell.world_map_heights = winner.heights.as_deref().map(map_samples);
            }
            for which in [Layer::Heights, Layer::Normals, Layer::Textures, Layer::WorldMap, Layer::Colors] {
                fill_from(cell, which, winner.get(which).as_ref());
            }
        }
    }
    let borrowed = add_reference_neighbours(outcome, reference);
    lap("neighbours");
    if let Some(rule) = existing {
        add_existing_seam_cells(outcome, loaded.unwrap_or(reference), rule, &borrowed);
        lap("existing seams");
    }
    outcome.borrowed = borrowed.len();
    let mut heights: BTreeMap<Coords, Vec<Value>> = outcome.cells.iter_mut().filter_map(|(c, cell)| cell.heights.take().map(|h| (*c, h))).collect();
    // The heights each world map entry was made from, for the cells that carry a map: the
    // merge's own where a plugin's map was merged (what the repairs below then move is looked for
    // against them), else those of the record the map came from (the masters', or the winner's),
    // so a map made for ground a plugin has since moved is made again where it moved.
    let mapped: BTreeMap<Coords, Vec<Value>> = heights
        .iter()
        .filter(|(c, _)| outcome.cells[*c].world_map.as_ref().is_some_and(|w| w.iter().any(|&v| v != 0)))
        .map(|(c, h)| {
            let n = crate::grid::WNAM_SIZE * crate::grid::WNAM_SIZE;
            let samples = outcome.cells[c].world_map_heights.clone().filter(|f| f.len() == n);
            (*c, samples.unwrap_or_else(|| (0..n).map(|k| h[crate::emit::world_map_vertex(k)]).collect()))
        })
        .collect();
    let mut phases: BTreeMap<Coords, Value> = BTreeMap::new();
    if !heights.is_empty() {
        // The reference's heights read in place where a corner needs the terrain outside the
        // merge, nothing unpacked.
        let anchor = |c: Coords, i: usize| reference.get(c).and_then(|l| l.heights.as_deref()).and_then(|h| h.at(i));
        outcome.seams = repair_seams(&mut heights, DEFAULT_FEATHER, Some(&anchor), &borrowed)?;
        if let Some(rule) = existing {
            outcome.existing_seam_borrowed = mend_between_borrowed(&mut heights, &borrowed, rule, &outcome.untouchable);
        }
        lap("seams");
        // What is saved is heights on VHGT's eight-unit grid, so that is what everything
        // after this reads: the slopes (a step is legal or not as it will be stored), the
        // normals (worked out from the stored terrain, not from terrain the encoder then
        // moves under them), the border check and the cleaning. The grid is the one most
        // vertices of each connected piece of terrain already have (the OpenMW fork's
        // `quantize_height_components`): a fractional offset's heights stay as they are, and
        // equal heights stay equal, so every copy of a shared vertex still agrees.
        phases = crate::steps::quantize_components(&mut heights);
        for (c, g) in outcome.existing_seam_original.iter_mut() {
            let phase = phases.get(c).copied().unwrap_or(0);
            g.iter_mut().for_each(|v| *v = crate::steps::quantize_to_phase(*v, phase));
        }
        lap("finalize");
        outcome.slopes = limit_slopes(&mut heights, &borrowed);
        lap("slopes");
        // The terrain outside the merge as it loads, for a corner that can be mended.
        let base = loaded.unwrap_or(reference);
        let outside = |c: Coords, i: usize| base.get(c).and_then(|l| l.heights.as_deref()).and_then(|h| h.at(i));
        let ceiling = existing.map_or(0, |r| r.max_gap);
        crate::seams::restore_pinned(&mut heights, &outcome.existing_seam_set, &outcome.existing_seam_original, Some(&outside), ceiling);
        for (c, g) in heights.iter_mut() {
            let phase = phases.get(c).copied().unwrap_or(0);
            g.iter_mut().for_each(|v| *v = crate::steps::quantize_to_phase(*v, phase));
        }
        // Last, the fork's own repair for what the limiter left (it does not move a vertex
        // shared with terrain outside the merge): a step no record can store has its higher
        // end lowered in every cell that has the vertex, a cell outside the merge brought in
        // for it. After this nothing is left for the encoder to clamp.
        let untouchable = outcome.untouchable.clone();
        let whole = |c: Coords| if untouchable.contains(&c) { None } else { base.get(c) }.and_then(|l| l.heights.as_deref()).map(crate::layers::Packed::to_vec);
        outcome.steps = crate::steps::fit_steps(&mut heights, &whole);
        for c in outcome.steps.brought_in.clone() {
            let mut cell = MergedCell { coords: c, ..Default::default() };
            inherit_reference_layers(&mut cell, reference);
            if let Some(winner) = loaded.and_then(|l| l.get(c)) {
                for which in [Layer::Textures, Layer::WorldMap, Layer::Colors] {
                    fill_from(&mut cell, which, winner.get(which).as_ref());
                }
            }
            outcome.cells.insert(c, cell);
        }
        lap("steps");
    }
    for (c, h) in heights {
        let cell = outcome.cells.get_mut(&c).unwrap();
        // An authored world map is kept as it is (WG-A07), but an entry of it describes the
        // height it was made from: where that height has moved (a plugin's, seam repair, the
        // slope limiter), the entry is made again from the height saved, as the engine would.
        if let (false, Some(was), Some(map)) = (outcome.keep_world_map, mapped.get(&c), cell.world_map.as_mut()) {
            for (k, entry) in map.iter_mut().enumerate() {
                let now = h[crate::emit::world_map_vertex(k)];
                if now != crate::steps::quantize_to_phase(was[k], phases.get(&c).copied().unwrap_or(0)) {
                    *entry = crate::emit::generated_world_map_entry(now);
                    outcome.world_map_regenerated += 1;
                }
            }
        }
        cell.heights = Some(h);
    }
    resolve_normals(outcome, reference, loaded);
    lap("normals");
    check_borders(outcome, reference, &borrowed);
    lap("borders");
    let merged: BTreeMap<Coords, CellDigest> = par_each(outcome.cells.iter().collect(), |(c, cell)| (*c, digest_cell(cell))).into_iter().collect();
    let refs: BTreeMap<Coords, CellDigest> =
        par_each(outcome.cells.keys().filter_map(|c| reference.get(*c).map(|l| (*c, l))).collect(), |(c, l)| (c, digest_reference(l))).into_iter().collect();
    lap("digests");
    let sources: BTreeMap<Coords, Vec<String>> = outcome.cells.iter().map(|(c, cell)| (*c, cell.editors.clone())).collect();
    let originals: BTreeMap<Coords, CellDigest> = outcome.cells.iter().map(|(c, cell)| (*c, cell.sole_source.clone())).collect();
    let (keep, report) = match loaded {
        Some(loaded) => {
            let there: BTreeMap<Coords, CellDigest> =
                par_each(outcome.cells.keys().filter_map(|c| loaded.get(*c).map(|l| (*c, l))).collect(), |(c, l)| (c, digest_reference(l))).into_iter().collect();
            clean_against_loaded(&merged, &there)
        }
        None => clean_landmass(&merged, &refs, &sources, &originals),
    };
    outcome.cleaning = report;
    let merged_cells: BTreeSet<Coords> = if loaded.is_some() { BTreeSet::new() } else { outcome.cells.keys().copied().collect() };
    outcome.cells.retain(|c, _| keep.contains(c));
    // A cell left with no layer at all is no record to write.
    outcome.cells.retain(|_, cell| cell.heights.is_some() || cell.colors.is_some() || cell.textures.is_some());
    outcome.seams.off_grid = off_grid_borders(outcome, reference, loaded, &merged_cells);
    lap("cleaning");
    Ok(times)
}

/// Cells left out of the merge as a page or a settings file writes them: entries split by
/// lines or `|`; `Plugin.esp=X,Y;X,Y` leaves that plugin's terrain out at those cells, a plain
/// cell list leaves the cells out for every plugin. An entry that does not read is an error.
pub fn parse_ignored_cells(text: &str) -> Result<IgnoredCells, String> {
    let mut out = IgnoredCells::default();
    for entry in text.split(['\n', '|']).map(str::trim).filter(|e| !e.is_empty()) {
        match entry.split_once('=') {
            Some((plugin, cells)) => {
                let plugin = plugin.trim();
                if plugin.is_empty() {
                    return Err(format!("\"{entry}\" names no plugin before the ="));
                }
                out.by_plugin.entry(plugin.to_ascii_lowercase()).or_default().extend(parse_cell_list(cells).map_err(|e| format!("{plugin}: {e}"))?);
            }
            None => out.everywhere.extend(parse_cell_list(entry)?),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse_cell_list;

    #[test]
    fn ignored_cells_read_for_every_plugin_and_for_one() {
        let got = super::parse_ignored_cells("TR_Mainland.esm=-9,-46;-9,-47 | 3,4\nOther.esp=1,1").unwrap();
        assert_eq!(got.everywhere.into_iter().collect::<Vec<_>>(), vec![(3, 4)]);
        assert_eq!(got.by_plugin["tr_mainland.esm"].len(), 2);
        assert!(got.by_plugin.contains_key("other.esp"));
        assert!(super::parse_ignored_cells("").unwrap().is_empty());
        assert!(super::parse_ignored_cells("=1,1").is_err());
        assert!(super::parse_ignored_cells("A.esp=odd").unwrap_err().starts_with("A.esp:"));
    }

    #[test]
    fn a_cell_list_reads_pairs_and_refuses_the_rest() {
        let got: Vec<_> = parse_cell_list("-10,-47; 3,4  5, 6\n").unwrap().into_iter().collect();
        assert_eq!(got, vec![(-10, -47), (3, 4), (5, 6)]);
        assert!(parse_cell_list("").unwrap().is_empty());
        assert!(parse_cell_list("3,4; odd").unwrap_err().contains("\"odd\" is not a cell"));
        assert!(parse_cell_list("3,").is_err());
    }
}
