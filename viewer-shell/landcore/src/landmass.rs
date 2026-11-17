//! The reference terrain and each plugin's changes against it (`land/landmass.py`).
//!
//! The reference is what the masters ship, later masters overriding earlier layer by layer.
//! A mod is then read as *which vertices it moved*, not *what the cell looks like*: that is
//! what lets two mods that reshape different parts of one cell both survive the merge.

use std::collections::{BTreeMap, HashMap};

use crate::layers::{diff_against_reference, DecodeError, Layer, LandscapeDiff, LandscapeLayers, ALL_LAYERS};
use crate::meta::{layer, PluginMeta};
use crate::read::Record;
use crate::textures::{translate_indices, KnownTextures};
use crate::grid::Value;

pub type Coords = (i64, i64);

/// One plugin's terrain records (`PluginRecords`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginRecords {
    pub name: String,
    pub records: Vec<Record>,
}

/// Every cell of the reference, and the master each came from (`Landmass`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Landmass {
    pub cells: BTreeMap<Coords, LandscapeLayers>,
    pub sources: BTreeMap<Coords, String>,
}

impl Landmass {
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn get(&self, coords: Coords) -> Option<&LandscapeLayers> {
        self.cells.get(&coords)
    }
}

/// Cells left out of the merge (the OpenMW fork's `cell_ignore` and `cell_ignore_by_plugin`):
/// `everywhere` for every plugin, so the cell is not merged, repaired or written at all;
/// `by_plugin` (names in lowercase) for one plugin's terrain there, which earlier and later
/// plugins still merge around. What the game loads is not changed by either: a plugin left
/// out of a cell still supplies it there unless the merge has something to write over it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IgnoredCells {
    pub everywhere: std::collections::BTreeSet<Coords>,
    pub by_plugin: BTreeMap<String, std::collections::BTreeSet<Coords>>,
}

impl IgnoredCells {
    /// The fork's default: Tamriel Rebuilt's Hunza Camp, four cells whose own discontinuity
    /// is meant (its `docs/hunza-camp.md`).
    pub fn hunza_camp() -> Self {
        IgnoredCells { everywhere: Default::default(), by_plugin: BTreeMap::from([("tr_mainland.esm".to_string(), [(-9, -46), (-9, -47), (-10, -46), (-10, -47)].into_iter().collect())]) }
    }

    pub fn is_empty(&self) -> bool {
        self.everywhere.is_empty() && self.by_plugin.values().all(|c| c.is_empty())
    }

    /// Whether `plugin`'s terrain at `coords` is left out.
    pub fn skips(&self, plugin: &str, coords: Coords) -> bool {
        self.everywhere.contains(&coords) || self.by_plugin.get(&plugin.to_ascii_lowercase()).is_some_and(|c| c.contains(&coords))
    }
}

/// The layer bits by grid, in the order masters are layered (`merge_master_layers`).
const MASTER_ORDER: [(Layer, u32); 5] = [
    (Layer::Heights, layer::VERTEX_HEIGHTS),
    (Layer::Normals, layer::VERTEX_NORMALS),
    (Layer::Colors, layer::VERTEX_COLORS),
    (Layer::Textures, layer::TEXTURES),
    (Layer::WorldMap, layer::WORLD_MAP),
];

/// A later master over an earlier one, layer by declared layer.
fn merge_master_layers(existing: &mut LandscapeLayers, incoming: LandscapeLayers) {
    let mut incoming = incoming;
    for (which, bit) in MASTER_ORDER {
        if incoming.declared & bit == 0 {
            continue;
        }
        if let Some(v) = incoming.get_mut(which).take() {
            *existing.get_mut(which) = Some(v);
            existing.declared |= bit;
        }
    }
}

/// A master's settings drop the layers it may not contribute (`_mask_layers`).
fn mask_layers(l: &mut LandscapeLayers, allowed: u32) {
    for (which, bit) in MASTER_ORDER {
        if allowed & bit == 0 {
            *l.get_mut(which) = None;
            l.declared &= !bit;
        }
    }
}

/// A plugin's undeleted `LAND` records, decoded and in the shared texture table
/// (`_landscape_records`, `_decode_cell`); a record that will not decode is logged in
/// `notes` and left out.
pub(crate) fn decoded(plugin: &str, records: &[Record], mapping: &HashMap<Value, Value>, notes: &mut Vec<String>) -> Result<Vec<LandscapeLayers>, String> {
    /* Each record decoded on its own, across the cores (2026-11-10: the reference landmass,
       12,634 cells of 23 masters, took 3.9 s on one); then taken in the records' order, so
       the cells, the notes and the first failure are the ones decoding in order gives. */
    let lands: Vec<&Record> = records.iter().filter(|r| matches!(r, Record::Land { deleted: false, .. })).collect();
    let each = crate::pipeline::par_each(lands, |r| decoded_one(plugin, r, mapping));
    let mut out = Vec::with_capacity(each.len());
    for got in each {
        let (l, note) = got?;
        notes.extend(note);
        out.extend(l);
    }
    Ok(out)
}

/// One `LAND` record decoded (`decoded`'s work for a record): the cell, or none (a record
/// with no terrain, or one left out, said in the note); `Err` for a failure that stops all.
fn decoded_one(plugin: &str, r: &Record, mapping: &HashMap<Value, Value>) -> Result<(Option<LandscapeLayers>, Option<String>), String> {
    let mut note = None;
    {
        let mut l = match LandscapeLayers::from_record(r) {
            Ok(Some(l)) => l,
            Ok(None) => return Ok((None, None)),
            Err(DecodeError::Skip(why)) => return Ok((None, Some(format!("{plugin}: skipping an unreadable landscape record: {why}")))),
            Err(DecodeError::Fatal(why)) => return Err(why),
        };
        if let Some(t) = l.textures.take() {
            let (values, unknown) = translate_indices(&t.to_vec(), mapping);
            if !unknown.is_empty() {
                let listed: Vec<String> = unknown.keys().take(8).map(|v| v.to_string()).collect();
                note = Some(format!(
                    "{plugin}: cell ({}, {}) paints with {} texture index/indices no LTEX record defines ({}) -- left untranslated. A master is probably missing.",
                    l.coords.0,
                    l.coords.1,
                    unknown.len(),
                    listed.join(", ")
                ));
            }
            l.textures = Some(crate::layers::pack_grid(values));
        }
        Ok((Some(l), note))
    }
}

/// A cell's layers laid over a landmass, as the engine lays a later record over an earlier
/// one: each layer it declares and carries replaces the one below (`merge_master_layers`;
/// the fork's `merge_tes3_landscape`); `source` becomes the cell's source.
pub fn layer_onto(land: &mut Landmass, l: LandscapeLayers, source: &str) {
    let coords = l.coords;
    match land.cells.get_mut(&coords) {
        None => {
            land.cells.insert(coords, l);
        }
        Some(existing) => merge_master_layers(existing, l),
    }
    land.sources.insert(coords, source.to_string());
}

/// A cell's whole record laid over a landmass, as the engine loads it: the later record
/// replaces the earlier one at its coordinate, layers it omits included (OpenMW's
/// `Store<ESM::Land>::load`). This is the loaded terrain the load-order cleaning compares
/// against; [`layer_onto`] is the merge's own composition, which keeps an earlier layer a
/// later record omits and so is not what loads.
///
/// The engine reads a layer only when the record's `DATA` flags declare it (OpenMW's
/// `Land::load` sets a data type when the subrecord is there *and* its flag is): a subrecord
/// the flags leave out is not terrain that loads, so it is dropped here. Kept, an undeclared
/// `VCLR` read as the cell's colors when the game draws white there.
pub fn replace_onto(land: &mut Landmass, mut l: LandscapeLayers, source: &str) {
    let coords = l.coords;
    let declared = l.declared;
    mask_layers(&mut l, declared);
    land.cells.insert(coords, l);
    land.sources.insert(coords, source.to_string());
}

/// A cell's layers with those `allowed` leaves out removed (`_mask_layers`).
pub fn masked(mut l: LandscapeLayers, allowed: u32) -> LandscapeLayers {
    if allowed != ALL_LAYERS {
        mask_layers(&mut l, allowed);
    }
    l
}

/// The reference landmass from the masters (`build_reference`); a master marked a previous
/// merge is left out, and a master's settings mask what it contributes. Also answers the
/// masters as the engine loads them, settings ignored (the start of the loaded landmass the
/// load-order cleaning compares against).
pub fn build_reference(masters: Vec<PluginRecords>, known: &mut KnownTextures, metas: &HashMap<String, PluginMeta>, ignored: &IgnoredCells, notes: &mut Vec<String>) -> Result<(Landmass, Landmass), String> {
    let mut land = Landmass::default();
    let mut loaded = Landmass::default();
    for master in masters {
        let meta = metas.get(&master.name);
        if meta.is_some_and(PluginMeta::is_previous_merge) {
            continue;
        }
        let allowed = meta.map(PluginMeta::allowed_layers).unwrap_or(ALL_LAYERS);
        // Each master's records freed once it is decoded: nothing reads them after.
        let cells = decode_plugin(&master, known, notes)?;
        let name = master.name;
        for l in cells {
            replace_onto(&mut loaded, l.clone(), &name);
            // A cell left out for this master is still what loads; it is not the reference.
            if !ignored.skips(&name, l.coords) {
                layer_onto(&mut land, masked(l, allowed), &name);
            }
        }
    }
    Ok((land, loaded))
}

/// A plugin's `LAND` records decoded, its textures in the shared table (which learns its
/// `LTEX` records first).
pub fn decode_plugin(plugin: &PluginRecords, known: &mut KnownTextures, notes: &mut Vec<String>) -> Result<Vec<LandscapeLayers>, String> {
    let mapping = known.observe(&plugin.name, &plugin.records);
    decoded(&plugin.name, &plugin.records, &mapping, notes)
}

/// What a plugin's decoded cells change against `reference`: those modified, or that declare
/// a layer they do not carry.
pub fn differences(reference: &Landmass, plugin: &str, cells: &[LandscapeLayers], allowed: u32) -> Result<Vec<LandscapeDiff>, String> {
    let mut out = Vec::new();
    for l in cells {
        let d = diff_against_reference(plugin, l, reference.get(l.coords), allowed)?;
        if d.is_modified() || d.has_terrain_data() || !d.missing.is_empty() {
            out.push(d);
        }
    }
    Ok(out)
}

/// What a plugin changed, cell by cell, against the masters (`plugin_differences`).
pub fn plugin_differences(reference: &Landmass, plugin: &PluginRecords, known: &mut KnownTextures, allowed: u32, notes: &mut Vec<String>) -> Result<Vec<LandscapeDiff>, String> {
    let cells = decode_plugin(plugin, known, notes)?;
    differences(reference, &plugin.name, &cells, allowed)
}
