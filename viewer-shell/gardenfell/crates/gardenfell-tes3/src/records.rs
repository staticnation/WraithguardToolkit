//! Object records: which record kinds a card may name, what a record says about itself
//! (id, model, script, the ids it depends on), and the leveled-list marker. Round 18ge:
//! moved out of `objects.rs` so the world can build its object table without depending on
//! the Objects-plugin planner that sits above it. Round 18gh: read off `tes3::raw::Record`
//! and the typed objects rather than off the bytes; a plugin for the Objects planner is
//! indexed as records ([`PluginIndex`]) and copied back with `Record::write`, which is
//! byte-exact.

use std::collections::{HashMap, HashSet};

use crate::esp::Tag;
use crate::tes3::bytes::{decode, Reader};
use crate::tes3::raw::{self, Record, FLAG_DELETED};
use crate::tes3::types::Object;

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
    pub fn tag(self) -> Tag {
        match self {
            ObjKind::Static => *b"STAT",
            ObjKind::Activator => *b"ACTI",
            ObjKind::Creature => *b"CREA",
            ObjKind::LeveledCreature => *b"LEVC",
        }
    }
    /// The word the page shows and the JSON carries.
    pub fn word(self) -> &'static str {
        match self {
            ObjKind::Static => "static",
            ObjKind::Activator => "activator",
            ObjKind::Creature => "creature",
            ObjKind::LeveledCreature => "leveled",
        }
    }
    pub fn from_word(s: &str) -> Option<ObjKind> {
        match s {
            "static" => Some(ObjKind::Static),
            "activator" => Some(ObjKind::Activator),
            "creature" => Some(ObjKind::Creature),
            "leveled" => Some(ObjKind::LeveledCreature),
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
    /// Deleted by a `DELE` subrecord or by the header's deleted bit (the typed flags carry
    /// both, backend-plan D1).
    pub deleted: bool,
}

/// The four kinds' identifying fields, off the typed object (round 18gh).
pub fn raw_object(obj: &Object) -> Option<RawObject> {
    let (id, kind, model, name, scale, entries, flags) = match obj {
        Object::Static(s) => (&s.id, ObjKind::Static, &s.mesh, "", None, Vec::new(), s.flags),
        Object::Activator(a) => {
            (&a.id, ObjKind::Activator, &a.mesh, a.name.as_str(), None, Vec::new(), a.flags)
        }
        // A creature's own size, as the Construction Set's Scale field writes it.
        Object::Creature(c) => (
            &c.id,
            ObjKind::Creature,
            &c.mesh,
            c.name.as_str(),
            c.scale.filter(|v| v.is_finite() && *v > 0.0),
            Vec::new(),
            c.flags,
        ),
        // A leveled list's entries, in order; the level beside each is not needed.
        Object::LeveledCreature(l) => (
            &l.id,
            ObjKind::LeveledCreature,
            &String::new(),
            "",
            None,
            l.creatures.iter().map(|(id, _)| id.clone()).collect(),
            l.flags,
        ),
        _ => return None,
    };
    if id.is_empty() {
        return None;
    }
    Some(RawObject {
        id: id.clone(),
        kind,
        model: model.clone(),
        name: name.to_string(),
        scale,
        entries,
        deleted: flags & FLAG_DELETED != 0,
    })
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

/// Whether a plugin file name is a master — `.esm`, however spelt.
pub fn is_esm(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".esm")
}

/// The three the objects plugin always names, when the load order has them.
pub const ALWAYS_MASTERS: [&str; 3] = ["morrowind.esm", "tribunal.esm", "bloodmoon.esm"];

/// A record's id — NAME on nearly everything; a script keeps its name in SCHD's first
/// thirty-two bytes and has no NAME at all. Off the raw record: every record of every
/// plugin passes through here, and the first subrecord is all it costs.
pub fn record_id(rec: &Record) -> Option<String> {
    if &rec.tag == b"SCPT" {
        let s = rec.sub(b"SCHD")?;
        return Reader::new(&s.data).fixed_string(32).ok();
    }
    if matches!(&rec.tag, b"TES3" | b"CELL" | b"LAND" | b"PGRD" | b"INFO" | b"DIAL") {
        return None;
    }
    rec.sub(b"NAME").map(|s| decode(&s.data))
}

/// The ids a record refers to and would be broken without: its script; an actor's or
/// container's inventory and an actor's spells; a leveled list's entries; an item's
/// enchantment and an armour's or clothing's body parts. What `plan` follows when asked
/// to copy the dependencies too. Off the raw record, since the question spans most of
/// the format's record kinds and asks only for a handful of strings.
pub fn deps_of(rec: &Record) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !s.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
            out.push(s);
        }
    };
    let tag = &rec.tag;
    let actor = matches!(tag, b"CREA" | b"NPC_");
    let list = matches!(tag, b"LEVC" | b"LEVI");
    let wearable = matches!(tag, b"ARMO" | b"CLOT");
    for s in &rec.subs {
        match &s.tag {
            b"SCRI" => push(decode(&s.data)),
            // An inventory entry: a count and a 32-byte id.
            b"NPCO" if (actor || tag == b"CONT") && s.data.len() >= 36 => {
                let mut r = Reader::new(&s.data);
                if r.skip(4).is_ok() {
                    if let Ok(id) = r.fixed_string(32) {
                        push(id);
                    }
                }
            }
            b"NPCS" if actor => push(decode(&s.data)),
            b"CNAM" | b"INAM" if list => push(decode(&s.data)),
            b"ENAM" if !matches!(tag, b"ENCH" | b"SPEL" | b"ALCH") => push(decode(&s.data)),
            b"BNAM" | b"CNAM" if wearable => push(decode(&s.data)),
            _ => {}
        }
    }
    out
}

/// A plugin as the Objects planner reads it (round 18gh): every record, and the ones that
/// have an id by lowercased id, so a copy is `Record::write` of the record itself. A file
/// the raw layer refuses indexes as empty.
#[derive(Default)]
pub struct PluginIndex<'a> {
    pub records: Vec<Record<'a>>,
    pub by_id: HashMap<String, Vec<usize>>,
}

impl<'a> PluginIndex<'a> {
    /// The index with every record's bytes copied, free of the plugin buffer it was read
    /// from (round 18gi): the Objects plugin keeps indexes across files, so it takes this.
    pub fn into_owned(self) -> PluginIndex<'static> {
        PluginIndex { records: self.records.into_iter().map(Record::into_owned).collect(), by_id: self.by_id }
    }
    /// The records called `id`, with their tags.
    pub fn get(&self, id: &str) -> Option<Vec<(Tag, &Record<'a>)>> {
        let at = self.by_id.get(&id.to_ascii_lowercase())?;
        Some(at.iter().filter_map(|&i| self.records.get(i)).map(|r| (r.tag, r)).collect())
    }
}

/// Indexes a plugin's records by id.
pub fn index_records(buf: &[u8]) -> PluginIndex<'_> {
    let records = raw::parse_plugin(buf).unwrap_or_default();
    let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rec) in records.iter().enumerate() {
        if let Some(id) = record_id(rec) {
            by_id.entry(id.to_ascii_lowercase()).or_default().push(i);
        }
    }
    PluginIndex { records, by_id }
}

/// A record's bytes, header included, as the file held them.
pub fn record_bytes(rec: &Record) -> Vec<u8> {
    let mut w = crate::tes3::bytes::Writer::with_capacity(16 + rec.body_len());
    rec.write(&mut w);
    w.into_bytes()
}

/// The sound generators a creature owns: SNDG records whose CNAM names it. Keyed by the
/// creature rather than by their own ids, which nobody types.
pub fn sound_gens_of<'a, 'b>(ix: &'a PluginIndex<'b>, creature: &str) -> Vec<&'a Record<'b>> {
    ix.records
        .iter()
        .filter(|r| &r.tag == b"SNDG")
        .filter(|r| r.subs_tagged(b"CNAM").any(|s| decode(&s.data).eq_ignore_ascii_case(creature)))
        .collect()
}
