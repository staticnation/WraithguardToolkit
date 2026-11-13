//! From the plugins' changes to the cells to write (`land/pipeline.py`): merge each layer
//! plugin by plugin in load order, fill what no mod touched from the reference, bring in the
//! untouched neighbours, repair the seams, limit the slopes, recompute the normals, check
//! the borders, and drop the cells that change nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::cleaning::{clean_against_loaded, clean_landmass, CellDigest, CleaningReport};
use crate::grid::RelativeGrid;

use crate::landmass::{differences, layer_onto, masked, replace_onto, Coords, Landmass, PluginRecords};
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
    pub colors: Option<Vec<Value>>,
    pub normals: Option<Vec<Value>>,
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
    /// Cells brought in to repair a seam the load order already has ([`ExistingSeams`]).
    pub existing_seam_cells: usize,
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

fn slot(cell: &mut MergedCell, which: Layer) -> &mut Option<Vec<Value>> {
    match which {
        Layer::Heights => &mut cell.heights,
        Layer::Textures => &mut cell.textures,
        Layer::WorldMap => &mut cell.world_map,
        Layer::Colors => &mut cell.colors,
        Layer::Normals => &mut cell.normals,
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
        *digest_slot(&mut cell.sole_source, which) = if first { Some(crate::cleaning::digest_of(source.reference.iter().zip(&source.delta).map(|(a, b)| a + b))) } else { None };
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
            *s = l.get(which).as_deref().map(crate::layers::Packed::to_vec);
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
    loaded: Option<&mut Landmass>,
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
    let mut work: Vec<(usize, u32, HashMap<Value, Value>)> = Vec::new();
    for (index, plugin) in plugins.iter().enumerate() {
        let meta = metas.get(&plugin.name);
        if meta.is_some_and(PluginMeta::is_previous_merge) {
            outcome.skipped_plugins.push((plugin.name.clone(), "a previous merge".into()));
            continue;
        }
        let allowed = meta.map(PluginMeta::allowed_layers).unwrap_or(ALL_LAYERS);
        if allowed == 0 {
            outcome.skipped_plugins.push((plugin.name.clone(), "every layer excluded".into()));
            if loaded.is_none() {
                continue;
            }
        }
        work.push((index, allowed, known.observe(&plugin.name, &plugin.records)));
    }
    let decoded_all = par_each(work, |(index, allowed, mapping)| {
        let plugin = &plugins[index];
        let mut notes = Vec::new();
        let cells = crate::landmass::decoded(&plugin.name, &plugin.records, &mapping, &mut notes);
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
    type CellOut = (Option<(MergedCell, HashMap<Layer, RelativeGrid>)>, Option<LandscapeLayers>);
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
        for (index, allowed, c) in touches {
            let plugin = &plugins[index];
            // The decoded cell is moved into the last copy that keeps it, cloned only when
            // both the rolling and the loaded landmass need one.
            let mut c = Some(c);
            if allowed != 0 {
                let meta = metas.get(&plugin.name);
                let against = rolling.as_ref().unwrap_or(reference);
                for change in differences(against, &plugin.name, std::slice::from_ref(c.as_ref().expect("the cell")), allowed)? {
                    let cell = cell.get_or_insert_with(|| MergedCell { coords, new_land: change.new_land, ..Default::default() });
                    cell.editors.push(plugin.name.clone());
                    cell.new_land = cell.new_land || change.new_land;
                    match model {
                        DiffModel::Masters => fold_change(cell, &mut grids, &change, meta, strategy)?,
                        DiffModel::LoadOrder => fold_rolling(cell, &mut grids, &change, reference, meta, strategy)?,
                    }
                }
                if let Some(r) = rolling.as_mut() {
                    let own = if here.is_some() { c.clone() } else { c.take() };
                    layer_onto(r, masked(own.expect("the cell"), allowed), &plugin.name);
                }
            }
            if let (Some(l), Some(c)) = (here.as_mut(), c.take()) {
                // What loads: the whole record of the last plugin to carry the cell.
                replace_onto(l, c, &plugin.name);
            }
        }
        Ok((cell.map(|c| (c, grids)), here.and_then(|mut l| l.cells.remove(&coords))))
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
            let (merged, here) = answer?;
            if let (Some(l), Some(h)) = (loaded.as_deref_mut(), here) {
                l.cells.insert(coords, h);
            }
            let Some((mut cell, grids)) = merged else { continue };
            for which in [Layer::Heights, Layer::Textures, Layer::WorldMap, Layer::Colors] {
                if let Some(g) = grids.get(&which) {
                    *slot(&mut cell, which) = Some(g.flat());
                }
            }
            inherit_reference_layers(&mut cell, reference);
            outcome.cells.insert(coords, cell);
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
        let read = |l: Option<&LandscapeLayers>| {
            let (normals, heights) = (l.and_then(|l| l.normals.clone()), l.and_then(|l| l.heights.clone()));
            normals.zip(heights)
        };
        let (winner, base) = (read(loaded.and_then(|l| l.get(*coords))), read(reference.get(*coords)));
        let at = |g: &crate::layers::Grid| {
            let g = g.clone();
            move |i: usize| g.at(i).unwrap_or(0)
        };
        let closures: Vec<_> = [&winner, &base].into_iter().flatten().map(|(n, hh)| (at(n), n.len(), at(hh), hh.len())).collect();
        let sources: Vec<crate::heights::NormalSource> =
            closures.iter().map(|(n, nl, hh, hl)| crate::heights::NormalSource { normals: n, normals_len: *nl, heights: hh, heights_len: *hl }).collect();
        let (n, kept) = crate::heights::resolve_cell_normals_from(h, &sources);
        cell.normals = Some(n);
        kept
    })
    .into_iter()
    .sum()
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
            if rule.except.contains(&here) || rule.except.contains(&other) {
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
        outcome.existing_seam_original.insert(c, crate::heights::finalized(&h.to_vec()));
        let mut cell = MergedCell { coords: c, heights: Some(h.to_vec()), ..Default::default() };
        inherit_reference_layers(&mut cell, base);
        outcome.cells.insert(c, cell);
    }
    outcome.existing_seam_cells = wanted.len();
    outcome.existing_seam_set = wanted.clone();
    wanted.len()
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
                // As a record of it would be saved, like the cells beside it.
                nearby.insert(o, crate::heights::finalized(&h));
            }
        }
    }
    outcome.seams.tears = crate::seams::find_tears_except(&written, Some(&nearby), borrowed, &outcome.existing_seam_set, &outcome.existing_seam_original);
}

fn digest_cell(cell: &MergedCell) -> CellDigest {
    {
        let d = |v: &Option<Vec<Value>>| v.as_deref().map(crate::cleaning::digest);
        CellDigest { heights: d(&cell.heights), normals: None, world_map: d(&cell.world_map), colors: d(&cell.colors), textures: d(&cell.textures) }
    }
}

fn digest_reference(l: &LandscapeLayers) -> CellDigest {
    {
        let d = |v: &Option<crate::layers::Grid>| v.as_deref().map(crate::layers::Packed::digest);
        // Heights as a record of them would be saved: the merged cell's are finalized, so a
        // reference off the eight-unit grid is compared on the grid too (a cell the merge
        // did not change is then not kept for the encoder's rounding alone).
        let heights = l.heights.as_deref().map(|h| crate::cleaning::digest(&crate::heights::finalized(&h.to_vec())));
        CellDigest { heights, normals: None, world_map: d(&l.world_map), colors: d(&l.colors), textures: d(&l.textures) }
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
    let borrowed = add_reference_neighbours(outcome, reference);
    lap("neighbours");
    if let Some(rule) = existing {
        add_existing_seam_cells(outcome, loaded.unwrap_or(reference), rule, &borrowed);
        lap("existing seams");
    }
    outcome.borrowed = borrowed.len();
    let mut heights: BTreeMap<Coords, Vec<Value>> = outcome.cells.iter_mut().filter_map(|(c, cell)| cell.heights.take().map(|h| (*c, h))).collect();
    if !heights.is_empty() {
        // The reference's heights read in place where a corner needs the terrain outside the
        // merge, nothing unpacked.
        let anchor = |c: Coords, i: usize| reference.get(c).and_then(|l| l.heights.as_deref()).and_then(|h| h.at(i));
        outcome.seams = repair_seams(&mut heights, DEFAULT_FEATHER, Some(&anchor), &borrowed)?;
        lap("seams");
        // What is saved is heights on VHGT's eight-unit grid, so that is what everything
        // after this reads: the slopes (a step is legal or not as it will be stored), the
        // normals (worked out from the stored terrain, not from terrain the encoder then
        // moves under them), the border check and the cleaning. Equal heights stay equal, so
        // every copy of a shared vertex still agrees.
        for h in heights.values_mut() {
            crate::heights::finalize_heights(h);
        }
        lap("finalize");
        outcome.slopes = limit_slopes(&mut heights, &borrowed);
        lap("slopes");
        crate::seams::restore_pinned(&mut heights, &outcome.existing_seam_set, &outcome.existing_seam_original);
    }
    for (c, h) in heights {
        outcome.cells.get_mut(&c).unwrap().heights = Some(h);
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
    outcome.cells.retain(|c, _| keep.contains(c));
    // A cell written replaces the record the engine would otherwise load, whole. A layer the
    // merge has nothing to say about is then the winning record's own, or the saved record
    // would drop it (heights are not filled: the merge always has its own where it has any).
    if let Some(loaded) = loaded {
        for (c, cell) in outcome.cells.iter_mut() {
            let Some(winner) = loaded.get(*c) else { continue };
            for which in [Layer::Textures, Layer::WorldMap, Layer::Colors] {
                let slot = slot(cell, which);
                if slot.is_none() {
                    *slot = winner.get(which).as_deref().map(crate::layers::Packed::to_vec);
                }
            }
        }
    }
    lap("cleaning");
    Ok(times)
}
