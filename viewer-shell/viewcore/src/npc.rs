//! NPCs, assembled from their body parts for the cell viewer - Wraithguard.
//!
//! An NPC has no mesh of its own: the game builds one from a skeleton and body parts,
//! as OpenMW's `NpcAnimation` does, and this is that, for drawing:
//!
//! - **the skeleton**: the record's own `MODL` when it names one, else `base_anim.nif`
//!   (`base_anim_female.nif` for a woman, `base_animkna.nif` for the beast races), read
//!   from its `x` twin when the install has one, with the idle from the `.kf` beside it -
//!   so the figure stands and breathes rather than holding the bind pose's T;
//! - **what it wears**: each armour and clothing item in its inventory, the best one per
//!   equipment slot (armour by rating, clothing by value), laid on by OpenMW's priorities
//!   (a robe over a skirt over the rest; armour over clothing in the same place). A part
//!   an item names with no body part hides what would be there (a helmet's hair);
//! - **the body under it**: the race's skin parts for every place nothing covers, the
//!   woman's where the race has one and the man's otherwise, and the record's own head
//!   and hair;
//! - **hung as the game hangs them**: a part skinned to the skeleton is bound to the
//!   skeleton's own bones by name (its shapes filtered to `tri <bone>`, since a race's
//!   skins file carries every place at once); a rigid part is attached to its bone, and a
//!   left-side one mirrored, as OpenMW's `SceneUtil::attach` does;
//! - **at the race's size**: its height and weight for the sex;
//! - **a shield** on the left arm, from the item's own mesh when it names no body parts.
//!
//! Not done: weapons (the game shows them only drawn), the head's lip-sync morphs, and
//! werewolves. The page asks for an NPC by the virtual mesh path [`NPC_PREFIX`] + id.
//!
//! Copyright (c) 2026 StaticNation, MIT (as the rest of viewcore).

use std::collections::HashMap;

use crate::esp::{cstring, Subs, Tag};
use crate::nif::{self, DrawPart};
use crate::vfs::Vfs;

/// The mesh path the page asks for an assembled NPC by: this, then the NPC's id.
/// Forward slash: the page normalises every mesh path (`VFS.norm`) before it asks.
pub const NPC_PREFIX: &str = "__npc/";

/// The NPC id in a mesh path the page asked for, either slash accepted (and without the
/// options that may follow it, see [`npc_request`]).
pub fn npc_id_of(path: &str) -> Option<&str> {
    npc_request(path).map(|(id, _)| id)
}

/// How the page wants an NPC drawn, after a `?` on its path: `night` (a torch, where it
/// carries one, in place of its shield) and `drawn` (its weapon in its hand).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcOpts {
    pub night: bool,
    pub drawn: bool,
}

/// The NPC id and the options in a mesh path the page asked for.
pub fn npc_request(path: &str) -> Option<(&str, NpcOpts)> {
    let rest = path.strip_prefix(NPC_PREFIX).or_else(|| path.strip_prefix("__npc\\"))?;
    let (id, q) = rest.split_once('?').unwrap_or((rest, ""));
    if id.is_empty() {
        return None;
    }
    let mut o = NpcOpts::default();
    for f in q.split('&') {
        match f {
            "night" => o.night = true,
            "drawn" => o.drawn = true,
            _ => {}
        }
    }
    Some((id, o))
}

/// A BODY record: one body part's mesh.
#[derive(Clone, Debug, Default)]
pub struct BodyPartDef {
    pub id: String,
    pub model: String,
    /// The race it belongs to (FNAM), for the skin parts.
    pub race: String,
    /// BYDT's mesh part: 0 head, 1 hair, 2 neck, 3 chest, 4 groin, 5 hand, 6 wrist,
    /// 7 forearm, 8 upper arm, 9 foot, 10 ankle, 11 knee, 12 upper leg, 13 clavicle,
    /// 14 tail.
    pub part: u8,
    pub vampire: bool,
    pub female: bool,
    /// 0 skin, 1 clothing, 2 armour.
    pub kind: u8,
}

/// A RACE record, for the skeleton and the size.
#[derive(Clone, Debug, Default)]
pub struct RaceDef {
    pub name: String,
    pub beast: bool,
    /// Male, female.
    pub height: [f32; 2],
    pub weight: [f32; 2],
}

/// An NPC_ record: what the figure is built from, and (for the object inspector) who it is.
#[derive(Clone, Debug, Default)]
pub struct NpcDef {
    pub id: String,
    pub name: String,
    /// A skeleton of its own, when it names one.
    pub model: String,
    pub race: String,
    pub class: String,
    pub faction: String,
    pub script: String,
    pub head: String,
    pub hair: String,
    pub female: bool,
    /// Inventory: (count, item id). A negative count restocks.
    pub items: Vec<(i32, String)>,
    /// NPDT's level, for the leveled lists in the inventory.
    pub level: i32,
    /// NPCS: its spells and abilities (a vampire's among them).
    pub spells: Vec<String>,
}

/// An ARMO, CLOT, WEAP or LIGH record: its type, how good it is, and the body parts it
/// puts on (armour and clothing) or its mesh (a shield, a weapon, a torch).
#[derive(Clone, Debug, Default)]
pub struct WearDef {
    pub armor: bool,
    pub weapon: bool,
    /// A light that can be carried: a torch.
    pub torch: bool,
    /// AODT's or CTDT's type.
    pub kind: u32,
    /// What the auto-equip compares: an armour's rating, a garment's value.
    pub rank: i32,
    /// (part reference, male body part, female body part).
    pub parts: Vec<(u8, String, String)>,
    /// The item's own mesh: what a shield is drawn with on the arm.
    pub model: String,
    /// A light's radius and colour (LHDT), for a carried torch's light.
    pub radius: f32,
    pub colour: [u8; 3],
}

/// The light a carried torch throws: where it is in the NPC's own space, its colour
/// (0..1) and its radius.
#[derive(Clone, Debug, PartialEq)]
pub struct CarriedLight {
    pub at: [f32; 3],
    pub colour: [f32; 3],
    pub radius: f32,
}

/// An NPC as [`assemble`] builds it.
pub struct Assembled {
    pub parts: Vec<DrawPart>,
    pub systems: Vec<nif::ParticleSystem>,
    pub light: Option<CarriedLight>,
}

/// The records an NPC is built from, by lowercased id.
#[derive(Clone, Debug, Default)]
pub struct NpcTables {
    pub bodies: HashMap<String, BodyPartDef>,
    pub races: HashMap<String, RaceDef>,
    pub npcs: HashMap<String, NpcDef>,
    pub wear: HashMap<String, WearDef>,
    /// LEVI: a leveled item list's entries, (level, item id).
    pub levi: HashMap<String, Vec<(i32, String)>>,
}

/// One plugin's share, before the merge: definitions, and what it deletes.
#[derive(Clone, Debug, Default)]
pub struct NpcRaw {
    pub tables: NpcTables,
    /// (tag, lowercased id) of every deleted record of these kinds.
    pub deleted: Vec<(Tag, String)>,
}

impl NpcTables {
    /// A later plugin's records over these: deletions first, then its definitions.
    pub fn merge(&mut self, raw: NpcRaw) {
        for (tag, k) in raw.deleted {
            match &tag {
                b"BODY" => {
                    self.bodies.remove(&k);
                }
                b"RACE" => {
                    self.races.remove(&k);
                }
                b"NPC_" => {
                    self.npcs.remove(&k);
                }
                b"LEVI" => {
                    self.levi.remove(&k);
                }
                _ => {
                    self.wear.remove(&k);
                }
            }
        }
        self.bodies.extend(raw.tables.bodies);
        self.races.extend(raw.tables.races);
        self.npcs.extend(raw.tables.npcs);
        self.wear.extend(raw.tables.wear);
        self.levi.extend(raw.tables.levi);
    }
}

fn f32le(d: &[u8], at: usize) -> f32 {
    d.get(at..at + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0.0)
}
fn i32le(d: &[u8], at: usize) -> i32 {
    d.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
}

/// Whether `collect` reads this kind of record.
pub fn wants(tag: Tag) -> bool {
    matches!(&tag, b"BODY" | b"RACE" | b"NPC_" | b"ARMO" | b"CLOT" | b"WEAP" | b"LIGH" | b"LEVI")
}

/// Reads one record of the kinds above into `raw`.
pub fn collect(raw: &mut NpcRaw, tag: Tag, body: &[u8]) {
    if !wants(tag) {
        return;
    }
    let mut id = String::new();
    let mut deleted = false;
    let mut b = BodyPartDef::default();
    let mut r = RaceDef::default();
    let mut n = NpcDef::default();
    let mut w = WearDef { armor: &tag == b"ARMO", weapon: &tag == b"WEAP", ..Default::default() };
    let mut levi: Vec<(i32, String)> = Vec::new();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"DELE" => deleted = true,
            b"MODL" => {
                b.model = cstring(s.data);
                n.model = cstring(s.data);
                w.model = cstring(s.data);
            }
            b"FNAM" => {
                b.race = cstring(s.data);
                r.name = cstring(s.data);
                n.name = cstring(s.data);
            }
            // BODY
            b"BYDT" if s.data.len() >= 4 => {
                b.part = s.data[0];
                b.vampire = s.data[1] != 0;
                b.female = s.data[2] & 1 != 0;
                b.kind = s.data[3];
            }
            // RACE: skill bonuses (56 bytes), attributes (64), then height, weight, flags.
            b"RADT" if s.data.len() >= 140 => {
                r.height = [f32le(s.data, 120), f32le(s.data, 124)];
                r.weight = [f32le(s.data, 128), f32le(s.data, 132)];
                r.beast = i32le(s.data, 136) & 2 != 0;
            }
            // NPC_
            b"RNAM" => n.race = cstring(s.data),
            b"CNAM" if &tag == b"NPC_" => n.class = cstring(s.data),
            b"ANAM" => n.faction = cstring(s.data),
            b"BNAM" if &tag == b"NPC_" => n.head = cstring(s.data),
            b"KNAM" => n.hair = cstring(s.data),
            b"SCRI" => n.script = cstring(s.data),
            b"FLAG" if s.data.len() >= 4 => n.female = i32le(s.data, 0) & 1 != 0,
            b"NPCO" if s.data.len() >= 36 => n.items.push((i32le(s.data, 0), cstring(&s.data[4..36]))),
            b"NPCS" => n.spells.push(cstring(s.data)),
            b"NPDT" if &tag == b"NPC_" && s.data.len() >= 2 => n.level = i16::from_le_bytes([s.data[0], s.data[1]]) as i32,
            // WEAP: weight, value, then the type.
            b"WPDT" if s.data.len() >= 10 => {
                w.rank = i32le(s.data, 4);
                w.kind = i16::from_le_bytes([s.data[8], s.data[9]]) as u32;
            }
            // LIGH: weight, value, time, radius, colour, flags (2: can be carried).
            b"LHDT" if s.data.len() >= 24 => {
                w.rank = i32le(s.data, 4);
                w.radius = i32le(s.data, 12).max(0) as f32;
                w.colour = [s.data[16], s.data[17], s.data[18]];
                w.torch = i32le(s.data, 20) & 2 != 0;
            }
            // LEVI: each entry's item, then its level.
            b"INAM" if &tag == b"LEVI" => levi.push((0, cstring(s.data))),
            b"INTV" if &tag == b"LEVI" && s.data.len() >= 2 => {
                if let Some(e) = levi.last_mut() {
                    e.0 = i16::from_le_bytes([s.data[0], s.data[1]]) as i32;
                }
            }
            // ARMO / CLOT
            b"AODT" if s.data.len() >= 24 => {
                w.kind = i32le(s.data, 0) as u32;
                w.rank = i32le(s.data, 20);
            }
            b"CTDT" if s.data.len() >= 12 => {
                w.kind = i32le(s.data, 0) as u32;
                w.rank = u16::from_le_bytes([s.data[8], s.data[9]]) as i32;
            }
            b"INDX" if !s.data.is_empty() => w.parts.push((s.data[0], String::new(), String::new())),
            b"BNAM" => {
                if let Some(p) = w.parts.last_mut() {
                    p.1 = cstring(s.data);
                }
            }
            b"CNAM" => {
                if let Some(p) = w.parts.last_mut() {
                    p.2 = cstring(s.data);
                }
            }
            _ => {}
        }
    }
    if id.is_empty() {
        return;
    }
    let k = id.to_ascii_lowercase();
    if deleted {
        raw.deleted.push((tag, k));
        return;
    }
    match &tag {
        b"BODY" => {
            b.id = id;
            raw.tables.bodies.insert(k, b);
        }
        b"RACE" => {
            raw.tables.races.insert(k, r);
        }
        b"NPC_" => {
            n.id = id;
            raw.tables.npcs.insert(k, n);
        }
        b"LEVI" => {
            raw.tables.levi.insert(k, levi);
        }
        _ => {
            raw.tables.wear.insert(k, w);
        }
    }
}

/* ---- the assembly --------------------------------------------------------------- */

/// The part references (ESM::PartReferenceType) and the bone each hangs on.
const BONES: [&str; 27] = [
    "Head", "Head", "Neck", "Chest", "Groin", "Groin", "Right Hand", "Left Hand", "Right Wrist",
    "Left Wrist", "Shield Bone", "Right Forearm", "Left Forearm", "Right Upper Arm",
    "Left Upper Arm", "Right Foot", "Left Foot", "Right Ankle", "Left Ankle", "Right Knee",
    "Left Knee", "Right Upper Leg", "Left Upper Leg", "Right Clavicle", "Left Clavicle",
    "Weapon Bone", "Tail",
];
const PRT_HEAD: usize = 0;
const PRT_HAIR: usize = 1;

/// A skin body part's mesh part, and the part references it fills.
fn skin_prts(part: u8) -> &'static [usize] {
    match part {
        2 => &[2],
        3 => &[3],
        4 => &[4],
        5 => &[6, 7],
        6 => &[8, 9],
        7 => &[11, 12],
        8 => &[13, 14],
        9 => &[15, 16],
        10 => &[17, 18],
        11 => &[19, 20],
        12 => &[21, 22],
        13 => &[23, 24],
        14 => &[26],
        _ => &[],
    }
}

/// OpenMW's equipment slots that show, in `NpcAnimation::updateParts`' order, with
/// their base priorities: robe, skirt, helmet, cuirass, greaves, left and right
/// pauldron, boots, left and right gauntlet, shirt, pants, the shield.
const SLOTS: [(u8, i32); 13] =
    [(0, 11), (1, 3), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (9, 0), (10, 0), (11, 0), (12, 0)];

/// The slot (index into `SLOTS`) an item goes in, when it shows at all.
fn slot_of(w: &WearDef) -> Option<u8> {
    if w.weapon || w.torch {
        return None;
    }
    if w.armor {
        match w.kind {
            0 => Some(2),
            1 => Some(3),
            2 => Some(5),
            3 => Some(6),
            4 => Some(4),
            5 => Some(7),
            6 | 9 => Some(8),
            7 | 10 => Some(9),
            8 => Some(12),
            _ => None,
        }
    } else {
        match w.kind {
            0 => Some(11),
            1 => Some(7),
            2 => Some(10),
            4 => Some(0),
            5 => Some(9),
            6 => Some(8),
            7 => Some(1),
            _ => None,
        }
    }
}

fn load(vfs: &Vfs, model: &str) -> Option<Vec<u8>> {
    let loc = vfs.resolve_mesh(model).or_else(|| vfs.resolve(model))?;
    vfs.load(&loc)
}

/// Where `model` resolves in this setup, as a cache key: a different file behind the
/// same name (another load order, a replacer switched on) is a different key.
fn loc_key(vfs: &Vfs, model: &str) -> String {
    match vfs.resolve_mesh(model).or_else(|| vfs.resolve(model)) {
        Some(l) => format!("{l:?}"),
        None => String::new(),
    }
}

/* Every NPC in a cell used to read its skeleton and the idle `.kf` beside it (the whole
   of xbase_anim.kf, a few megabytes of keys) and every body part file afresh: a town of
   a hundred and fifty people parsed the same half-dozen files a hundred and fifty times
   over, and the cell's load crawled. Read once per file as it resolves, and shared. */
type Cache<T> = std::sync::Mutex<HashMap<String, std::sync::Arc<T>>>;
static SKELETONS: std::sync::OnceLock<Cache<Option<nif::Skeleton>>> = std::sync::OnceLock::new();
static PART_FILES: std::sync::OnceLock<Cache<Option<Vec<DrawPart>>>> = std::sync::OnceLock::new();

fn cached<T>(cell: &'static std::sync::OnceLock<Cache<T>>, key: String, make: impl FnOnce() -> T) -> std::sync::Arc<T> {
    let map = cell.get_or_init(Default::default);
    if let Some(v) = map.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return v;
    }
    // Made outside the lock: two threads may both read a file once, which is harmless.
    let v = std::sync::Arc::new(make());
    if let Ok(mut m) = map.lock() {
        // A few hundred files at most in any setup; cleared if it ever grows past that
        // (a long session through many load orders).
        if m.len() > 4096 {
            m.clear();
        }
        m.insert(key, v.clone());
    }
    v
}

/// A body part file's skinned draw parts, read once per file.
fn part_file(vfs: &Vfs, model: &str) -> std::sync::Arc<Option<Vec<DrawPart>>> {
    let key = loc_key(vfs, model);
    if key.is_empty() {
        return std::sync::Arc::new(None);
    }
    cached(&PART_FILES, key, || load(vfs, model).and_then(|b| nif::read_parts_skinned(&b)))
}

/// The skeleton an NPC stands on, with the idle bound when there is one, read once per
/// skeleton file and `.kf` pair.
fn skeleton(t: &NpcTables, vfs: &Vfs, npc: &NpcDef, beast: bool) -> Option<std::sync::Arc<Option<nif::Skeleton>>> {
    let base = if !npc.model.is_empty() {
        npc.model.clone()
    } else if beast {
        "base_animkna.nif".to_string()
    } else if npc.female {
        "base_anim_female.nif".to_string()
    } else {
        "base_anim.nif".to_string()
    };
    let twin = crate::world::x_twin(vfs, &base);
    let file = twin.clone().unwrap_or_else(|| base.clone());
    let kf_of = |p: &str| p.rfind('.').map(|d| format!("{}.kf", &p[..d])).unwrap_or_default();
    let shared = if beast { "xbase_animkna.nif" } else { "xbase_anim.nif" };
    let key = format!(
        "{}|{}|{}|{}",
        loc_key(vfs, &file),
        loc_key(vfs, &base),
        twin.as_deref().map(|p| loc_key(vfs, &kf_of(p))).unwrap_or_default(),
        loc_key(vfs, &kf_of(shared))
    );
    let s = cached(&SKELETONS, key, || skeleton_read(t, vfs, npc, beast));
    if s.is_some() { Some(s) } else { None }
}

/// [`skeleton`], read from the files.
fn skeleton_read(t: &NpcTables, vfs: &Vfs, npc: &NpcDef, beast: bool) -> Option<nif::Skeleton> {
    let base = if !npc.model.is_empty() {
        npc.model.clone()
    } else if beast {
        "base_animkna.nif".to_string()
    } else if npc.female {
        "base_anim_female.nif".to_string()
    } else {
        "base_anim.nif".to_string()
    };
    let _ = t;
    let twin = crate::world::x_twin(vfs, &base);
    let file = twin.clone().unwrap_or_else(|| base.clone());
    let bytes = load(vfs, &file).or_else(|| load(vfs, &base))?;
    // The idle: from the `.kf` beside the skeleton, else the shared one for the kind.
    let mut kf = twin.as_deref().and_then(|p| crate::world::kf_beside(vfs, p)).filter(|k| k.idle().is_some());
    if kf.is_none() {
        let shared = if beast { "xbase_animkna.nif" } else { "xbase_anim.nif" };
        kf = crate::world::kf_beside(vfs, shared).filter(|k| k.idle().is_some());
    }
    nif::Skeleton::read(&bytes, kf.as_ref())
}

/// Picks the body part id for each part reference: what the NPC wears first, then its
/// head, hair and skin wherever nothing covers. An empty id is a reserved, bare place.
#[cfg(test)]
fn choose(t: &NpcTables, npc: &NpcDef, beast: bool) -> [Option<String>; 27] {
    choose_kit(t, npc, beast).chosen
}

/// A leveled item list as one item: the entry of the highest level the NPC has reached
/// (the lowest when it has reached none), followed through lists inside lists. The
/// game rolls this when the NPC is first met; the viewer takes the same pick every time.
fn level_item<'a>(t: &'a NpcTables, id: &'a str, level: i32) -> Option<&'a str> {
    let mut at = id;
    for _ in 0..8 {
        let Some(list) = t.levi.get(&at.to_ascii_lowercase()) else { return Some(at) };
        let fit = list.iter().filter(|(l, _)| *l <= level.max(1)).max_by_key(|(l, _)| *l);
        let pick = fit.or_else(|| list.iter().min_by_key(|(l, _)| *l))?;
        at = &pick.1;
    }
    None
}

/// What an NPC is dressed in: the body part for each place, and what it carries.
struct Kit<'a> {
    chosen: [Option<String>; 27],
    /// A shield's own mesh, when the shield names no body parts.
    shield: Option<&'a WearDef>,
    weapon: Option<&'a WearDef>,
    torch: Option<&'a WearDef>,
}

/// Whether the NPC is a vampire: one of its spells is a vampirism ability (the base
/// game's `Vampire Attributes`, and every clan's).
fn is_vampire(npc: &NpcDef) -> bool {
    npc.spells.iter().any(|s| s.to_ascii_lowercase().starts_with("vampire"))
}

/// [`choose`], and the shield, weapon and torch the NPC carries.
fn choose_kit<'a>(t: &'a NpcTables, npc: &'a NpcDef, beast: bool) -> Kit<'a> {
    let mut chosen: [Option<String>; 27] = Default::default();
    let mut prio = [0i32; 27];
    // The best item per slot, and the best weapon and torch.
    let mut best: [Option<&WearDef>; 13] = Default::default();
    let (mut weapon, mut torch): (Option<&WearDef>, Option<&WearDef>) = (None, None);
    for (count, item) in &npc.items {
        if *count == 0 {
            continue;
        }
        let Some(item) = level_item(t, item, npc.level) else { continue };
        let Some(w) = t.wear.get(&item.to_ascii_lowercase()) else { continue };
        // Arrows and bolts are not held; a thrown weapon is.
        if w.weapon && w.kind != 12 && w.kind != 13 && weapon.map(|c| w.rank > c.rank).unwrap_or(true) {
            weapon = Some(w);
        }
        if w.torch && torch.map(|c| w.rank > c.rank).unwrap_or(true) {
            torch = Some(w);
        }
        let Some(slot) = slot_of(w) else { continue };
        // The beast races wear no boots (OpenMW: they cannot).
        if beast && slot == 7 {
            continue;
        }
        let cur = &mut best[slot as usize];
        if cur.map(|c| w.rank > c.rank).unwrap_or(true) {
            *cur = Some(w);
        }
    }
    for (slot, base) in SLOTS {
        let Some(w) = best[slot as usize] else { continue };
        let p = ((base + 1) << 1) + if w.armor { 1 } else { 0 };
        for (prt, male, female) in &w.parts {
            let prt = *prt as usize;
            if prt >= 27 || p <= prio[prt] {
                continue;
            }
            prio[prt] = p;
            let id = if npc.female && !female.is_empty() { female } else { male };
            chosen[prt] = Some(id.clone());
        }
    }
    // Head and hair, from the record - a vampire's head from the race's vampire part.
    if prio[PRT_HEAD] < 1 && is_vampire(npc) {
        let race = npc.race.to_ascii_lowercase();
        for want_female in [npc.female, false] {
            if let Some(b) = t.bodies.values().find(|b| {
                b.vampire && b.part == 0 && b.female == want_female && b.race.to_ascii_lowercase() == race
            }) {
                chosen[PRT_HEAD] = Some(b.id.clone());
                prio[PRT_HEAD] = 1;
                break;
            }
        }
    }
    if prio[PRT_HEAD] < 1 && !npc.head.is_empty() {
        chosen[PRT_HEAD] = Some(npc.head.clone());
        prio[PRT_HEAD] = 1;
    }
    if prio[PRT_HAIR] < 1 && !npc.hair.is_empty() {
        chosen[PRT_HAIR] = Some(npc.hair.clone());
        prio[PRT_HAIR] = 1;
    }
    // The race's skin: the NPC's sex where the race has the part, the man's otherwise.
    let race = npc.race.to_ascii_lowercase();
    for want_female in [npc.female, false] {
        for b in t.bodies.values() {
            if b.kind != 0 || b.vampire || b.female != want_female || b.race.to_ascii_lowercase() != race {
                continue;
            }
            if b.id.to_ascii_lowercase().ends_with("1st") {
                continue;
            }
            for &prt in skin_prts(b.part) {
                if prio[prt] < 1 {
                    prio[prt] = 1;
                    chosen[prt] = Some(b.id.clone());
                }
            }
        }
    }
    Kit { chosen, shield: best[12].filter(|w| w.parts.is_empty()), weapon, torch }
}

/// Where a weapon hangs when it is not drawn - OpenMW's sheathing bones, which only a
/// skeleton made for weapon sheathing has - by WPDT type.
fn sheath_bone(kind: u32) -> Option<&'static str> {
    Some(match kind {
        0 => "Bip01 ShortBladeOneHand",
        1 => "Bip01 LongBladeOneHand",
        2 => "Bip01 LongBladeTwoClose",
        3 => "Bip01 BluntOneHand",
        4 => "Bip01 BluntTwoClose",
        5 => "Bip01 BluntTwoWide",
        6 => "Bip01 SpearTwoWide",
        7 => "Bip01 AxeOneHand",
        8 => "Bip01 AxeTwoClose",
        9 => "Bip01 MarksmanBow",
        10 => "Bip01 MarksmanCrossbow",
        11 => "Bip01 MarksmanThrown",
        _ => return None,
    })
}

/// `model` with `_sh` before its extension: a weapon's scabbard.
fn sheath_of(model: &str) -> String {
    match model.rfind('.') {
        Some(d) => format!("{}_sh{}", &model[..d], &model[d..]),
        None => format!("{model}_sh"),
    }
}

/// The NPC `id` as draw parts, in its own space (feet at the origin, facing +Y), or
/// `None` when the load order does not define it or nothing of it could be read.
pub fn assemble(t: &NpcTables, vfs: &Vfs, id: &str, opts: NpcOpts) -> Option<Assembled> {
    let npc = t.npcs.get(&id.to_ascii_lowercase())?;
    let race = t.races.get(&npc.race.to_ascii_lowercase());
    let beast = race.map(|r| r.beast).unwrap_or(false);
    let skel_read = skeleton(t, vfs, npc, beast)?;
    let skel: &nif::Skeleton = skel_read.as_ref().as_ref()?;
    let kit = choose_kit(t, npc, beast);
    let chosen = &kit.chosen;
    let mut systems: Vec<nif::ParticleSystem> = Vec::new();

    let mut out: Vec<DrawPart> = Vec::new();
    // A file already used for a place, with the shapes taken from it: a race's skins file
    // serves a dozen places, and each shape in it is drawn once.
    let mut taken: Vec<(String, String)> = Vec::new();
    for (prt, body) in chosen.iter().enumerate() {
        let Some(body) = body else { continue };
        if body.is_empty() {
            continue;
        }
        let Some(def) = t.bodies.get(&body.to_ascii_lowercase()) else { continue };
        if def.model.is_empty() {
            continue;
        }
        let key = def.model.to_ascii_lowercase();
        let file = part_file(vfs, &def.model);
        let Some(parts) = file.as_ref().clone() else { continue };
        let bone = BONES[prt];
        let skinned = parts.iter().any(|p| p.skin.is_some());
        if skinned {
            let filter = format!("tri {}", bone.to_ascii_lowercase());
            let mut pick: Vec<DrawPart> = parts
                .iter()
                .filter(|p| p.skin.is_some() && p.name.to_ascii_lowercase().starts_with(&filter))
                .cloned()
                .collect();
            /* A file for one place whose shapes are named otherwise: all of its skinned
               shapes. Not a file that names its shapes by place (a race's skins file):
               there, a place it has no shape for is simply not in it. */
            let by_place = parts.iter().any(|p| {
                let n = p.name.to_ascii_lowercase();
                BONES.iter().any(|b| n.starts_with(&format!("tri {}", b.to_ascii_lowercase())))
            });
            if pick.is_empty() && !by_place {
                pick = parts.iter().filter(|p| p.skin.is_some()).cloned().collect();
            }
            for mut p in pick {
                let tag = (key.clone(), p.name.to_ascii_lowercase());
                if taken.contains(&tag) {
                    continue;
                }
                taken.push(tag);
                p.morph = None;
                nif::bind_to_skeleton(&mut p, skel);
                out.push(p);
            }
        } else {
            let tag = (key.clone(), bone.to_string());
            if taken.contains(&tag) {
                continue;
            }
            taken.push(tag);
            for mut p in parts {
                p.morph = None;
                if nif::attach_to_bone(&mut p, skel, bone, bone.starts_with("Left")) {
                    out.push(p);
                }
            }
        }
    }
    // Carried: a mesh of its own on a bone, with its particle systems (a torch's flame).
    let mut carry = |model: &str, bone: &str, out: &mut Vec<DrawPart>| -> bool {
        let Some(buf) = load(vfs, model) else { return false };
        let Some(parts) = nif::read_parts_skinned(&buf) else { return false };
        let mut any = false;
        for mut p in parts {
            p.morph = None;
            if nif::attach_to_bone(&mut p, skel, bone, false) {
                out.push(p);
                any = true;
            }
        }
        let mut sys = nif::parse_particles(&buf);
        if any && nif::attach_systems(&mut sys, skel, bone) {
            systems.extend(sys);
        }
        any
    };
    /* The left hand: a torch at night (OpenMW's NPCs take one out after dark, in place of
       the shield), else the shield's own mesh when its armour names no body parts. */
    let torch = kit.torch.filter(|_| opts.night && !beast);
    let mut light: Option<CarriedLight> = None;
    match (torch, kit.shield) {
        (Some(w), _) if !w.model.is_empty() => {
            /* And its light, as OpenMW gives a carried light: the LIGH record's radius
               and colour, at the mesh's AttachLight (its origin when it has none), on the
               hand's bone as the flame is. */
            if carry(&w.model, BONES[10], &mut out) && w.radius > 0.0 {
                let local = load(vfs, &w.model).and_then(|b| nif::read_for_draw(&b, None)).and_then(|r| r.attach).unwrap_or([0.0; 3]);
                if let Some(at) = nif::bone_point(skel, BONES[10], local) {
                    light = Some(CarriedLight { at, colour: w.colour.map(|c| c as f32 / 255.0), radius: w.radius });
                }
            }
        }
        (_, Some(w)) if chosen[10].is_none() && !w.model.is_empty() => {
            carry(&w.model, BONES[10], &mut out);
        }
        _ => {}
    }
    // The weapon: in the hand when drawn; else in its sheath, where the skeleton has one.
    if let Some(w) = kit.weapon.filter(|w| !w.model.is_empty()) {
        if opts.drawn {
            carry(&w.model, BONES[25], &mut out);
        } else if let Some(bone) = sheath_bone(w.kind).filter(|b| skel.bone(b, nif::identity()).is_some()) {
            let scabbard = sheath_of(&w.model);
            if !carry(&scabbard, bone, &mut out) {
                carry(&w.model, bone, &mut out);
            }
        }
    }
    if out.is_empty() {
        return None;
    }
    // The race's size for the sex: weight across, height up.
    if let Some(r) = race {
        let i = if npc.female { 1 } else { 0 };
        let (w, h) = (r.weight[i], r.height[i]);
        if w > 0.0 && h > 0.0 && ((w - 1.0).abs() > 1e-3 || (h - 1.0).abs() > 1e-3) {
            let m = [w, 0., 0., 0., w, 0., 0., 0., h, 0., 0., 0.];
            for p in &mut out {
                nif::prepend_transform(p, &m, 1.0);
            }
            nif::prepend_systems(&mut systems, &m, 1.0);
            if let Some(l) = light.as_mut() {
                l.at = [l.at[0] * w, l.at[1] * w, l.at[2] * h];
            }
        }
    }
    Some(Assembled { parts: out, systems, light })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }
    fn z(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    #[test]
    fn options_follow_the_id() {
        assert_eq!(npc_request("__npc/caius cosades?night&drawn"), Some(("caius cosades", NpcOpts { night: true, drawn: true })));
        assert_eq!(npc_request("__npc/fargoth"), Some(("fargoth", NpcOpts::default())));
        assert_eq!(npc_id_of("__npc/fargoth?night"), Some("fargoth"));
    }

    #[test]
    fn a_leveled_item_is_the_highest_the_npc_has_reached() {
        let mut t = NpcTables::default();
        t.levi.insert("random helm".into(), vec![(1, "iron_helm".into()), (5, "steel_helm".into()), (10, "inner list".into())]);
        t.levi.insert("inner list".into(), vec![(1, "imperial_helm".into())]);
        assert_eq!(level_item(&t, "Random Helm", 7), Some("steel_helm"));
        assert_eq!(level_item(&t, "Random Helm", 12), Some("imperial_helm"));
        assert_eq!(level_item(&t, "Random Helm", 0), Some("iron_helm"));
        assert_eq!(level_item(&t, "plain_item", 3), Some("plain_item"));
    }

    #[test]
    fn a_vampire_takes_the_race_s_vampire_head() {
        let mut t = NpcTables::default();
        t.bodies.insert("b_v_dunmer_m_head_01".into(), BodyPartDef { id: "b_v_dunmer_m_head_01".into(), race: "Dark Elf".into(), vampire: true, ..Default::default() });
        let npc = NpcDef { race: "Dark Elf".into(), head: "own_head".into(), spells: vec!["Vampire Attributes".into()], ..Default::default() };
        assert_eq!(choose(&t, &npc, false)[0].as_deref(), Some("b_v_dunmer_m_head_01"));
        let plain = NpcDef { spells: Vec::new(), ..npc };
        assert_eq!(choose(&t, &plain, false)[0].as_deref(), Some("own_head"));
    }

    #[test]
    fn the_page_path_names_the_npc_either_slash() {
        // The page's VFS.norm turns the backslash into a slash before asking.
        assert_eq!(npc_id_of("__npc/caius cosades"), Some("caius cosades"));
        assert_eq!(npc_id_of("__npc\\Caius Cosades"), Some("Caius Cosades"));
        assert_eq!(npc_id_of("__npc/"), None);
        assert_eq!(npc_id_of("meshes/__npc/x"), None);
    }

    #[test]
    fn reads_an_armour_piece_and_its_parts() {
        let mut body = sub(b"NAME", &z("iron_cuirass"));
        let mut aodt = vec![0u8; 24];
        aodt[0] = 1; // cuirass
        aodt[20] = 12; // rating
        body.extend(sub(b"AODT", &aodt));
        body.extend(sub(b"INDX", &[3]));
        body.extend(sub(b"BNAM", &z("a_iron_cuirass")));
        body.extend(sub(b"INDX", &[23]));
        body.extend(sub(b"BNAM", &z("a_iron_pauldron_r")));
        body.extend(sub(b"CNAM", &z("a_iron_pauldron_rf")));
        let mut raw = NpcRaw::default();
        collect(&mut raw, *b"ARMO", &body);
        let w = &raw.tables.wear["iron_cuirass"];
        assert!(w.armor);
        assert_eq!((w.kind, w.rank), (1, 12));
        assert_eq!(w.parts[1], (23, "a_iron_pauldron_r".into(), "a_iron_pauldron_rf".into()));
        assert_eq!(slot_of(w), Some(3));
    }

    #[test]
    fn a_torch_keeps_its_light() {
        let mut lhdt = Vec::new();
        lhdt.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        lhdt.extend_from_slice(&5i32.to_le_bytes()); // value
        lhdt.extend_from_slice(&600i32.to_le_bytes()); // time
        lhdt.extend_from_slice(&256i32.to_le_bytes()); // radius
        lhdt.extend_from_slice(&[255, 200, 120, 0]); // colour
        lhdt.extend_from_slice(&(2i32 | 16).to_le_bytes()); // can carry, fire
        let body = [sub(b"NAME", &z("torch")), sub(b"MODL", &z("l\\torch.nif")), sub(b"LHDT", &lhdt)].concat();
        let mut raw = NpcRaw::default();
        collect(&mut raw, *b"LIGH", &body);
        let w = &raw.tables.wear["torch"];
        assert!(w.torch);
        assert_eq!((w.radius, w.colour), (256.0, [255, 200, 120]));
    }

    #[test]
    fn armour_beats_clothing_and_skin_fills_the_rest() {
        let mut t = NpcTables::default();
        let wear = |armor, kind, parts: &[(u8, &str)]| WearDef {
            armor,
            kind,
            rank: 1,
            parts: parts.iter().map(|(p, m)| (*p, m.to_string(), String::new())).collect(),
            model: String::new(),
            ..Default::default()
        };
        t.wear.insert("shirt".into(), wear(false, 2, &[(3, "c_shirt_chest"), (13, "c_shirt_arm")]));
        t.wear.insert("cuirass".into(), wear(true, 1, &[(3, "a_cuirass")]));
        t.wear.insert("helm".into(), wear(true, 0, &[(0, "a_helm"), (1, "")]));
        for (id, part) in [("b_chest", 3u8), ("b_upperarm", 8)] {
            t.bodies.insert(id.into(), BodyPartDef { id: id.into(), model: "x.nif".into(), race: "Nord".into(), part, ..Default::default() });
        }
        let npc = NpcDef {
            race: "Nord".into(),
            head: "head".into(),
            hair: "hair".into(),
            items: vec![(1, "shirt".into()), (1, "cuirass".into()), (1, "helm".into())],
            ..Default::default()
        };
        let c = choose(&t, &npc, false);
        assert_eq!(c[3].as_deref(), Some("a_cuirass"));
        assert_eq!(c[13].as_deref(), Some("c_shirt_arm"));
        assert_eq!(c[14].as_deref(), Some("b_upperarm"));
        assert_eq!(c[0].as_deref(), Some("a_helm"));
        assert_eq!(c[1].as_deref(), Some(""));
    }
}
