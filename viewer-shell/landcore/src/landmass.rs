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
fn decoded(plugin: &str, records: &[Record], mapping: &HashMap<i64, i64>, notes: &mut Vec<String>) -> Result<Vec<LandscapeLayers>, String> {
    let mut out = Vec::new();
    for r in records {
        let Record::Land { deleted, .. } = r else { continue };
        if *deleted {
            continue;
        }
        let mut l = match LandscapeLayers::from_record(r) {
            Ok(Some(l)) => l,
            Ok(None) => continue,
            Err(DecodeError::Skip(why)) => {
                notes.push(format!("{plugin}: skipping an unreadable landscape record: {why}"));
                continue;
            }
            Err(DecodeError::Fatal(why)) => return Err(why),
        };
        if let Some(t) = l.textures.take() {
            let (values, unknown) = translate_indices(&t, mapping);
            if !unknown.is_empty() {
                let listed: Vec<String> = unknown.keys().take(8).map(|v| v.to_string()).collect();
                notes.push(format!(
                    "{plugin}: cell ({}, {}) paints with {} texture index/indices no LTEX record defines ({}) -- left untranslated. A master is probably missing.",
                    l.coords.0,
                    l.coords.1,
                    unknown.len(),
                    listed.join(", ")
                ));
            }
            l.textures = Some(crate::layers::Grid::new(values));
        }
        out.push(l);
    }
    Ok(out)
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
pub fn build_reference(masters: &[PluginRecords], known: &mut KnownTextures, metas: &HashMap<String, PluginMeta>, notes: &mut Vec<String>) -> Result<(Landmass, Landmass), String> {
    let mut land = Landmass::default();
    let mut loaded = Landmass::default();
    for master in masters {
        let meta = metas.get(&master.name);
        if meta.is_some_and(PluginMeta::is_previous_merge) {
            continue;
        }
        let allowed = meta.map(PluginMeta::allowed_layers).unwrap_or(ALL_LAYERS);
        for l in decode_plugin(master, known, notes)? {
            layer_onto(&mut loaded, l.clone(), &master.name);
            layer_onto(&mut land, masked(l, allowed), &master.name);
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
        if d.is_modified() || !d.missing.is_empty() {
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
