//! Object definitions by id, for the world the viewer draws.
//!
//! Every STAT, ACTI, CREA and LEVC (and the other placeable records) across the load
//! order, read into an [`ObjectDef`] when [`World::load_with`](crate::world::World)
//! merges the plugins, so a placed reference can be drawn with its model; leveled
//! creature lists resolve to a model through [`resolve_leveled`]. [`record_id`] names a
//! record by its id. (The module came from Gardenfell, where it also planned copying
//! records into an exported plugin; that went with the export.)

use std::collections::{HashMap, HashSet};


use crate::esp::{cstring, Subs, Tag};

/// The four kinds a card may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjKind {
    Static,
    Activator,
    Creature,
    LeveledCreature,
}

impl ObjKind {
    pub fn of_tag(t: Tag) -> Option<ObjKind> {
        match &t {
            b"STAT" => Some(ObjKind::Static),
            b"ACTI" => Some(ObjKind::Activator),
            b"CREA" => Some(ObjKind::Creature),
            b"LEVC" => Some(ObjKind::LeveledCreature),
            _ => None,
        }
    }
}

/// One record a card may name, as the load order defines it: the last definition's
/// kind, model and name, and every plugin that defines it (load-order indices, in
/// order). A definition a later plugin deletes is gone from the index altogether.
#[derive(Clone, Debug, Default)]
pub struct ObjectDef {
    pub id: String,
    pub kind: Option<ObjKind>,
    /// The model the game draws it with — a creature's `x` twin when the load order
    /// has one, since that is the file with the idle. Empty for a leveled list.
    pub model: String,
    /// The record's display name (FNAM), for the picker; empty on a static.
    pub name: String,
    /// The record's own scale (a creature's XSCL), when it states one. Robin: "A
    /// reference object placed from the load order should always have the scale it has
    /// in the reference record (if any is set) as its default" - the card is prefilled
    /// with it, so an exported reference carries the scale the record was made at.
    pub scale: Option<f32>,
    /// A leveled list's entries (CNAM ids), as its last definition has them. What the
    /// preview's model for the list is worked out from - see `resolve_leveled`.
    pub entries: Vec<String>,
    pub defined_in: Vec<usize>,
}

/// One record as `parse_plugin` reports it, before the merge.
#[derive(Clone, Debug)]
pub struct RawObject {
    pub id: String,
    pub kind: ObjKind,
    pub model: String,
    pub name: String,
    pub scale: Option<f32>,
    pub entries: Vec<String>,
    pub deleted: bool,
}

/// Reads the four kinds' identifying fields off a record body.
pub fn raw_object(tag: Tag, body: &[u8]) -> Option<RawObject> {
    let kind = ObjKind::of_tag(tag)?;
    let (mut id, mut model, mut name, mut deleted) = (String::new(), String::new(), String::new(), false);
    let mut scale = None;
    let mut entries = Vec::new();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"MODL" => model = cstring(s.data),
            b"FNAM" => name = cstring(s.data),
            // A leveled list's entries, in order; the INTV level beside each is not needed.
            b"CNAM" if kind == ObjKind::LeveledCreature => entries.push(cstring(s.data)),
            // A creature's own size, as the Construction Set's Scale field writes it.
            b"XSCL" if s.data.len() >= 4 => {
                let v = f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                if v.is_finite() && v > 0.0 {
                    scale = Some(v);
                }
            }
            b"DELE" => deleted = true,
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    Some(RawObject { id, kind, model, name, scale, entries, deleted })
}

/// The mesh the Construction Set draws a leveled creature list with - the "ninja monkey"
/// - and what the preview draws one with when its creatures do not all share a mesh.
pub const LEVELED_MARKER: &str = "marker_creature.nif";

/// Gives every leveled list its preview model. Robin: "If all creatures in the list use
/// the same mesh, use that mesh to render it at the position. If not all meshes are the
/// same, use the ninja monkey mesh that is used in TES3 Construction Set for leveled
/// creatures." A list inside a list is followed; an entry nothing defines, an empty list
/// and a list that loops all count as "not the same" and draw the marker.
pub fn resolve_leveled(objects: &mut HashMap<String, ObjectDef>) {
    // `path` holds the lists being walked, so a list that names itself, however far
    // round, is a loop and not a stack overflow; a creature may be reached twice.
    fn leaves(objects: &HashMap<String, ObjectDef>, id: &str, path: &mut HashSet<String>, out: &mut Vec<String>) -> bool {
        let k = id.to_ascii_lowercase();
        let Some(o) = objects.get(&k) else { return false };
        match o.kind {
            Some(ObjKind::Creature) => {
                if o.model.is_empty() {
                    return false;
                }
                out.push(o.model.clone());
                true
            }
            Some(ObjKind::LeveledCreature) => {
                if o.entries.is_empty() || !path.insert(k.clone()) {
                    return false;
                }
                let ok = o.entries.iter().all(|e| leaves(objects, e, path, out));
                path.remove(&k);
                ok
            }
            _ => false,
        }
    }
    let lists: Vec<String> = objects.iter().filter(|(_, o)| o.kind == Some(ObjKind::LeveledCreature)).map(|(k, _)| k.clone()).collect();
    for k in lists {
        let mut out = Vec::new();
        let mut path = HashSet::new();
        let all = leaves(objects, &k, &mut path, &mut out);
        // The same file however the records spell it; the first entry's spelling is kept.
        let model = if all && !out.is_empty() && out.iter().all(|m| m.eq_ignore_ascii_case(&out[0])) {
            out[0].clone()
        } else {
            LEVELED_MARKER.to_string()
        };
        if let Some(o) = objects.get_mut(&k) {
            o.model = model;
        }
    }
}

/* ---- records, by id, inside one plugin file ---------------------------------------- */

/// A record's id — NAME on nearly everything; a script keeps its name in SCHD's first
/// thirty-two bytes and has no NAME at all.
pub fn record_id(tag: Tag, body: &[u8]) -> Option<String> {
    if &tag == b"SCPT" {
        for s in Subs::new(body) {
            if &s.tag == b"SCHD" && s.data.len() >= 32 {
                return Some(cstring(&s.data[..32]));
            }
        }
        return None;
    }
    if matches!(&tag, b"TES3" | b"CELL" | b"LAND" | b"PGRD" | b"INFO" | b"DIAL") {
        return None;
    }
    for s in Subs::new(body) {
        if &s.tag == b"NAME" {
            return Some(cstring(s.data));
        }
    }
    None
}
