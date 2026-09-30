//! Load-order resolution: plugins in, a merged world out.
//!
//! Every plugin is parsed on its own thread into an isolated result, then the
//! results are folded together in load order. Parsing is the expensive half and
//! it is embarrassingly parallel; merging is a few hash inserts per record and
//! has to be sequential to respect override order, so it stays on one thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::esp::*;
use crate::land::{decode_vclr, decode_vhgt, decode_vnml, unswizzle_vtex, Land};
use crate::nif;
use crate::pool;
use crate::vfs::Vfs;

mod from_crate;

#[derive(Default)]
pub struct PluginData {
    pub masters: Vec<String>,
    pub ltex: Vec<(u32, String, String)>, // index, id, filename
    pub regions: Vec<(String, String)>,
    pub cells: Vec<RawCell>,
    pub lands: Vec<Land>,
    /// Every model record: id, mesh, deleted, whether the record's class is one the
    /// game animates (round 18ag, see `x_twin`) and whether it carries a script (18ak).
    pub models: Vec<ModelDef>,
    /// Round 17m: the LIGH records among them, with what LHDT says about the light.
    pub lights: Vec<LightDef>,
    pub actors: Vec<ActorDef>,
    /// Round 18av: every STAT, ACTI, CREA and LEVC, for the object picker and the
    /// objects plugin (`objects.rs`).
    pub objects: Vec<crate::objects::RawObject>,
    /// Round 18av: every record that has an id, by lowercased id and tag — what the
    /// objects plugin's dependency search asks "does a master define this?" of.
    pub ids: Vec<(String, Tag)>,
    /// Wraithguard: the races, body parts, NPCs and worn items an NPC is assembled from
    /// (`npc.rs`).
    pub npc: crate::npc::NpcRaw,
    /// Wraithguard: each LAND record's grid, offset in the file and length (header included).
    pub land_at: Vec<((i32, i32), u64, u32)>,
    pub pathgrids: Vec<Pathgrid>,
    pub gmst: Vec<(String, f32)>,
    pub hello: Vec<(String, u8)>,
    pub records: usize,
}

/// A LIGH record's light, for the preview's night lighting (round 17m).
///
/// Robin: "Lights is it's own form of record in Morrowind engine, and in that record
/// there is information about how far they light things up. For light falloff etc. look
/// at OpenMW." That record is LIGH and the subrecord is LHDT, 24 bytes: a float weight,
/// then five int32 — value, time, radius, colour, flags. The colour's four bytes are
/// red, green, blue and one the game ignores (OpenMW's `colourFromRGB` forces alpha to
/// 1). Radius is a signed integer count of world units.
#[derive(Clone, Debug, Default)]
pub struct LightDef {
    pub id: String,
    /// As written. OpenMW floors it at 16 before using it (`createLightSource`), which
    /// is the renderer's business rather than the reader's.
    pub radius: i32,
    pub colour: [u8; 3],
    pub flags: i32,
    pub deleted: bool,
}

impl LightDef {
    /// 0x004: "negative light — i.e. darkness". OpenMW negates the diffuse and drops the
    /// specular for these.
    pub fn negative(&self) -> bool {
        self.flags & 0x004 != 0
    }
    /// 0x020: "off by default — does not burn while placed in a cell, but can burn when
    /// equipped". One placed in the world gives no light at all.
    pub fn off_default(&self) -> bool {
        self.flags & 0x020 != 0
    }
}

/// One model record as a plugin states it — a static, an activator, a door… (round 18ak).
#[derive(Clone, Debug)]
pub struct ModelDef {
    pub id: String,
    pub mesh: String,
    pub deleted: bool,
    /// The record's class is one the game animates (ACTI, CONT, DOOR, LIGH).
    pub animated_class: bool,
    /// The record names a script. Whether that sets an animated object *moving* depends
    /// on the engine the install runs (`LoadOpts::script_plays`); see `x_twin`'s use in
    /// `World::load_with`.
    pub scripted: bool,
}

/// Round 18al: what the install's engine does that bears on the world as loaded.
#[derive(Clone, Debug)]
pub struct LoadOpts {
    /// A script's plain `PlayGroup` sets its object moving — true for OpenMW and for the
    /// game's own executable, false under the Code Patch's "Improved animation support"
    /// (`mge::MCP_IMPROVED_ANIMATION`), where a flagless PlayGroup waits behind the
    /// `Idle` the object was born in and a banner's one-instant `Idle` never lets it
    /// through. Decides whether a scripted activator takes its moving `x` twin.
    pub script_plays: bool,
}

impl Default for LoadOpts {
    fn default() -> Self {
        LoadOpts { script_plays: true }
    }
}

/// A creature or NPC definition, kept only so that the ones that spawn dead can be
/// drawn and avoided. Living actors wander off; a corpse stays where it was placed,
/// and grass growing through it looks wrong.
#[derive(Clone, Debug)]
pub struct ActorDef {
    pub id: String,
    pub model: String,
    /// Round 18ag: the `x` twin of `model` when the load order has one — the file the
    /// game draws a *living* creature with (see `x_twin`). Kept beside `model` rather than
    /// in its place: a corpse is drawn from the plain file, still, and the twin would
    /// play its idle over the body.
    pub twin: Option<String>,
    pub creature: bool,
    /// None when the record uses auto-calculated stats and stores no health at all.
    pub health: Option<i32>,
    pub persistent: bool,
    pub deleted: bool,
}

impl ActorDef {
    /// Health 0 means the actor spawns dead. With auto-calculated stats there is no
    /// health to read, and an auto-calculated actor cannot be a corpse by design, so
    /// the "corpses persist" flag is the only remaining evidence.
    pub fn corpse(&self) -> bool {
        match self.health {
            Some(h) => h == 0,
            None => self.persistent,
        }
    }
}

pub struct RawCell {
    pub name: String,
    pub flags: u32,
    pub grid: (i32, i32),
    pub region: String,
    /// Where the water sits, for an interior that has any — `WHGT` or, in vanilla's
    /// spelling, `INTV`. Exteriors carry neither; their sea is at zero.
    pub water: Option<f32>,
    /// Round 17y: an interior's `AMBI`, when the record carries one.
    pub ambi: Option<crate::esp::Ambi>,
    pub refs: Vec<CellRef>,
}

/// One interior cell: a room, by name, with what is in it.
///
/// It used to be a name and a count — enough for the picker to offer one, which was the
/// whole of §58 513. Offering one that could not then be *looked at* turned out to be
/// worse than not offering it: `CellData.loadCell` asks for `(gx, gy)`, an interior has
/// no grid, and every one of them arrived as (0, 0) — so picking a room quietly loaded
/// whichever exterior happens to sit at the origin. Robin: "Viewing interiors doesn't
/// work, it just loads an exterior cell instead."
///
/// So the references are kept, merged by the same rules exteriors follow: a later plugin
/// replaces one in place, a deleted one goes, and one moved out lands in the exterior it
/// was moved to. What an interior still has none of is a **grid position and a
/// landscape** — which is why it stays out of `World.cells` (§58 514) and why nothing
/// scatters in one.
#[derive(Clone, Debug, Default)]
pub struct Interior {
    pub name: String,
    /// Wraithguard: every plugin (index into `World.plugins`) with a CELL record for this
    /// room, in load order - the cell map's coverage.
    pub touched_by: Vec<u32>,
    /// An interior's water level, where it has one. Unlike an exterior's sea this is not
    /// always zero, and a flooded room drawn at zero would be drawn wrong.
    pub water: Option<f32>,
    /// Which plugin's header said there is water here, and which one last said how high —
    /// indices into [`World::plugins`]. Two facts and often two different plugins, which
    /// is exactly why they are reported: when a room's water is somewhere nobody expects,
    /// the useful answer is the name of the plugin that put it there.
    pub water_flag_from: Option<usize>,
    pub water_from: Option<usize>,
    /// Round 17y: whether the last header said "behave like exterior" (flag 0x80) — a room
    /// with the sky's weather, sun and hours. Anything else is lit by `ambi` alone.
    pub quasi: bool,
    /// Round 18bc (A2): the last header's DATA flags, kept whole so the export can write
    /// them back. A plugin that adds references to a room restates the flags, and the last
    /// CELL record's flags win in the game's merge — so a plugin that guessed them would
    /// take away whatever it failed to guess. Vivec's cantons (0x80, behave like exterior)
    /// would lose the sky; a shrine marked illegal to sleep in (0x04) would become a bed.
    /// `quasi` above is this field's 0x80 bit, kept separate because the preview reads it
    /// by name everywhere.
    pub flags: u32,
    /// Round 17y: the room's own light, from the last plugin whose header stated one — a
    /// re-saved header without an AMBI does not take the light away, on the same
    /// reasoning as the water level.
    pub ambi: Option<crate::esp::Ambi>,
    /// Merged references, keyed by RefNum so later plugins override rather than duplicate.
    pub refs: HashMap<RefNum, CellRef>,
    /// What the load order did on the way, the same tally a cell keeps.
    pub prov: CellProv,
    /// Wraithguard: what a plugin took out of the room - deleted or moved away - as it
    /// stood before (`world::Gone`).
    pub gone: Vec<Gone>,
}

/// An interior's name as [`World::interiors`] keys it — round 18bd (E8).
///
/// Lowercased **through the whole alphabet**. Six of the seven lookups used
/// `to_ascii_lowercase` and the insert used `to_lowercase`, so for any room whose name
/// carries a non-ASCII letter — and `esp::cstring` maps a plugin's bytes straight to
/// chars, so cp1251 uppercase Cyrillic (the first letter of essentially every interior
/// name in a Russian localisation) and cp1252 `Ä Ö Ü É À` all land there — the room
/// **opened, listed and drew normally** and then: no grass preview at all, the brush laid
/// nothing and reported zero objects, no tint, no surfaces read, and "clear the paint in
/// view" reported success having cleared nothing. Meanwhile `rules_in_scope` and the
/// Advanced export *did* use `to_lowercase`, so the export happily scoped that room — a
/// room grassed by mesh rules exported grass the preview could not show.
///
/// One function, so the two halves cannot drift apart again.
pub fn room_key(name: &str) -> String {
    if name.is_ascii() { name.to_ascii_lowercase() } else { name.to_lowercase() }
}

/// Reserved mesh name for the body-sized slab that stands in for an NPC corpse.
/// It is not a file and never resolves through the overlay.
pub const CORPSE_SLAB: &str = "__corpse_slab";

/// `marker_*` anywhere in the path: Morrowind's editor markers, which the game never
/// draws. Any path segment, not just the file name, so a mod that keeps them in a
/// `marker_stuff` folder is caught too — the same rule the page applies.
///
/// **Compared as bytes** (round 18bb). `esp::cstring` decodes a plugin's latin-1 MODL
/// byte for byte into chars, so a Cyrillic mesh path — a Russian, Polish or Czech
/// localisation, or any mod with non-ASCII mesh names — becomes a string whose byte
/// length is nearly twice its character count. `seg.len() >= 7 && seg[..7]` mixed the
/// two: the length test passed and the slice landed inside a character, which panics,
/// and this is on the path of every reference in the world (`obstacle_ref`), so the
/// whole load aborted. Bytes on both sides, and `eq_ignore_ascii_case` on `[u8]` folds
/// exactly the ASCII letters the game folds.
pub fn is_marker_mesh(model: &str) -> bool {
    model.split(|c| c == '\\' || c == '/').any(|seg| {
        let b = seg.as_bytes();
        b.len() >= 7 && b[..7].eq_ignore_ascii_case(b"marker_")
    })
}

/// The outline of a body lying down, as half-width along its length.
///
/// `(t, w)`: `t` runs -1 at the crown to +1 at the feet, `w` is the half-width as a
/// fraction of `SLAB_W`. A plain box was the first version and it read as a crate —
/// grass stopped in a rectangle, which is not what a body does to the ground. This
/// is still deliberately coarse: an obstacle shape, not a model.
///
/// **The same table exists in `08_scatter.js`'s `CORPSE_PROFILE`, and the two must
/// stay identical** — `t_corpseslab.js` compares the triangles they produce. Change
/// one, change both.
pub const CORPSE_PROFILE: [(f32, f32); 9] = [
    (-1.00, 0.30), // crown
    (-0.86, 0.46), // head
    (-0.72, 0.38), // neck
    (-0.55, 1.00), // shoulders — the widest point
    (-0.20, 0.86), // chest
    (0.10, 0.74),  // waist
    (0.45, 0.80),  // hips
    (0.78, 0.55),  // legs
    (1.00, 0.32),  // feet
];
pub const SLAB_L: f32 = 90.0;
pub const SLAB_W: f32 = 28.0;
pub const SLAB_H: f32 = 16.0;

/// A prone, body-shaped stand-in in object space, in game units.
///
/// The animation beside a mesh: `r\xsiltstrider.nif` walks by `r\xsiltstrider.kf`.
///
/// Resolved the way the mesh was, so a replacer's mesh takes the game's own `.kf` from
/// the archive when it ships none of its own — which is what the game does. `None` for a
/// mesh with no `.kf`, which is most of them, and for a path whose only dot is in a
/// folder's name (`mods\a.b\thing`).
///
/// Round 18be (G7): lives here rather than in the Tauri shell because the *world* needs
/// it too — the hull and the picture have to be built from the same pose.
pub fn kf_beside(v: &Vfs, path: &str) -> Option<nif::KfSequence> {
    let dot = path.rfind('.')?;
    if path[dot..].contains(['\\', '/']) {
        return None;
    }
    let kfp = format!("{}.kf", &path[..dot]);
    let loc = v.resolve_mesh(&kfp).or_else(|| v.resolve(&kfp))?;
    let bytes = v.load(&loc)?;
    nif::parse_kf(&bytes)
}

/// An NPC has no single mesh — the game assembles one from body parts — so nothing
/// can be read off disk to keep grass out of. Drawing or testing a standing figure
/// would misrepresent the thing that actually matters, which is the patch of ground
/// a body covers.
///
/// The outline above, extruded to `SLAB_H`: a quad strip for each cap and a wall
/// down each side. Quad-strip rather than a triangle fan because the outline is not
/// convex at the neck, and a fan would tuck a sliver of the head inside the body.
pub fn corpse_slab() -> nif::MeshGeom {
    let n = CORPSE_PROFILE.len();
    let pt = |i: usize, side: f32, z: f32| {
        let (t, w) = CORPSE_PROFILE[i];
        [t * SLAB_L, side * w * SLAB_W, z]
    };
    let mut tris: Vec<[f32; 3]> = Vec::with_capacity((n - 1) * 24 + 12);
    let mut quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
        tris.extend_from_slice(&[a, b, c, a, c, d]);
    };
    for i in 0..n - 1 {
        // floor and lid
        quad(pt(i, 1.0, 0.0), pt(i + 1, 1.0, 0.0), pt(i + 1, -1.0, 0.0), pt(i, -1.0, 0.0));
        quad(pt(i, 1.0, SLAB_H), pt(i, -1.0, SLAB_H), pt(i + 1, -1.0, SLAB_H), pt(i + 1, 1.0, SLAB_H));
        // the two long walls
        quad(pt(i, 1.0, 0.0), pt(i, 1.0, SLAB_H), pt(i + 1, 1.0, SLAB_H), pt(i + 1, 1.0, 0.0));
        quad(pt(i, -1.0, 0.0), pt(i + 1, -1.0, 0.0), pt(i + 1, -1.0, SLAB_H), pt(i, -1.0, SLAB_H));
    }
    // and the ends, so the hull is closed and the inside test means something
    quad(pt(0, -1.0, 0.0), pt(0, 1.0, 0.0), pt(0, 1.0, SLAB_H), pt(0, -1.0, SLAB_H));
    let e = n - 1;
    quad(pt(e, 1.0, 0.0), pt(e, -1.0, 0.0), pt(e, -1.0, SLAB_H), pt(e, 1.0, SLAB_H));

    let w_max = CORPSE_PROFILE.iter().fold(0.0f32, |m, (_, w)| m.max(*w)) * SLAB_W;
    nif::MeshGeom {
        tris: std::sync::Arc::new(tris),
        aabb_min: [-SLAB_L, -w_max, 0.0],
        aabb_max: [SLAB_L, w_max, SLAB_H],
        // The slab is a hull, not a rendered shape: it stands in for a body, and a
        // body is solid all the way through.
        shape: nif::ColShape::Hull,
        radius: (SLAB_L * SLAB_L + w_max * w_max + SLAB_H * SLAB_H * 0.25f32).sqrt(),
        scanned: 0,
    }
}

/// Creatures and NPCs. Their definitions are read for corpse detection only.
fn is_actor_tag(t: Tag) -> bool {
    matches!(&t, b"CREA" | b"NPC_")
}

/// The Persistent bit in a record header — the CS labels it "Corpses persist".
const REC_PERSISTENT: u32 = 0x0400;

/// CREA NPDT is 96 bytes of int32 stats with Health 11th, at offset 40.
/// NPC_ NPDT is 52 bytes when the stats are explicit, Health an int16 at offset 38;
/// the 12-byte form means auto-calculated stats and carries no health at all.
fn actor_health(creature: bool, d: &[u8]) -> Option<i32> {
    if creature {
        if d.len() >= 96 {
            return Some(i32le(d, 40));
        }
        return None;
    }
    if d.len() >= 52 {
        return Some(u16le(d, 38) as i16 as i32);
    }
    None
}

/// Record types that carry a placeable mesh.
/// Round 18ag: the record classes the game animates, and so the ones whose mesh it
/// swaps for the `x` twin when there is one — see `x_twin`. OpenMW's `useAnim()`:
/// activators, containers, doors and lights; creatures and NPCs are actors and go their
/// own way. A **static** is deliberately not one of these: `furn_imp_flag_01_indoor` is a
/// STAT on `f\Furn_imp_flag_01.NIF`, the same mesh the activator `furn_imp_flag_01`
/// waves, and indoors it hangs still - which is the whole reason it is a separate record.
fn is_animated_class(t: Tag) -> bool {
    matches!(&t, b"ACTI" | b"CONT" | b"DOOR" | b"LIGH")
}

/// Round 18ag: the animated twin of a mesh, when the load order has one.
///
/// Robin: "References KIL_silt_Zirath, a_siltstrider etc. all use the Siltstrider.nif
/// mesh, but they still look the same as before, meaning they are static and not the
/// replacer I use." The port strider is an *activator* on `r\Siltstrider.NIF`, and no
/// record in Morrowind.esm names an `x` mesh at all. The game inserts an `x` before the
/// file name and, if that file exists, loads it - with the `.kf` beside it - instead:
/// `r\xSiltstrider.NIF` is the one that sways and carries the lanterns, and it is the
/// only file his replacer ships. OpenMW's `correctActorModelPath`, applied wherever
/// `useAnim()` says (`Scene::getModel`). 114 activators, 257 creatures and every NPC in
/// the base game use meshes with such a twin; the old MGE XE baked the plain one into
/// Distant Land and G7's fork bakes the twin, which is the difference Robin pointed at.
///
/// Kept in the file's own spelling with the `x` inserted, so the name shown in the object
/// dialogue is the name in the archive.
pub fn x_twin(vfs: &Vfs, mesh: &str) -> Option<String> {
    let cut = mesh.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    let name = &mesh[cut..];
    if name.is_empty() {
        return None;
    }
    // A mesh already called `x…` asks for `xx…`, which is simply not there.
    let twin = format!("{}x{}", &mesh[..cut], name);
    if vfs.resolve_mesh(&twin).is_some() { Some(twin) } else { None }
}

/// Round 18ak: whether the `x` twin at `twin` would move with nobody asking — its `.kf`
/// beside it has an `Idle` with length, which the game plays for any animated object.
/// A banner's `Idle` is one instant; a creature's, or the port strider's, is a loop.
/// Cached by the `.kf`'s path, since a mesh serves many records.
fn twin_moves_unbidden(vfs: &Vfs, twin: &str, seen: &mut HashMap<String, bool>) -> bool {
    let Some(dot) = twin.rfind('.') else { return false };
    if twin[dot..].contains(['\\', '/']) {
        return false;
    }
    let kfp = format!("{}.kf", &twin[..dot]);
    let key = kfp.to_ascii_lowercase().replace('\\', "/");
    if let Some(&v) = seen.get(&key) {
        return v;
    }
    let moves = vfs
        .resolve_mesh(&kfp)
        .and_then(|loc| vfs.load(&loc))
        .and_then(|b| crate::nif::parse_kf(&b))
        .map(|kf| kf.clip("Idle").is_some())
        .unwrap_or(false);
    seen.insert(key, moves);
    moves
}

fn is_model_tag(t: Tag) -> bool {
    matches!(
        &t,
        b"STAT"
            | b"ACTI"
            | b"DOOR"
            | b"CONT"
            | b"LIGH"
            | b"MISC"
            | b"WEAP"
            | b"ARMO"
            | b"CLOT"
            | b"BOOK"
            | b"ALCH"
            | b"APPA"
            | b"INGR"
            | b"LOCK"
            | b"PROB"
            | b"REPA"
    )
}

/// The id given to VTEX 0. It is not a record in any plugin, so it needs a name of
/// its own to be selectable in a config and listable in the cell's texture list.
pub const DEFAULT_LTEX: &str = "_land_default";
/// What the engine draws for it.
pub const DEFAULT_LTEX_FILE: &str = "_land_default.dds";

/// Reads every plugin with the engine's own subrecord walk, skipping the crate: for
/// comparing the two readers (see `set_own_reader_only`).
static OWN_READER_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Test hook: read plugins with the engine's own walk only (`true`), or with the crate
/// first (`false`, the default).
#[doc(hidden)]
pub fn set_own_reader_only(own: bool) {
    OWN_READER_ONLY.store(own, std::sync::atomic::Ordering::Relaxed);
}


/// One LAND record's body as the world keeps it, or `None` without a grid or heights.
pub fn decode_land(body: &[u8], plugin_idx: i32) -> Option<Land> {
    let (mut grid, mut heights, mut vtex, mut normals, mut colours) = (None, None, None, None, None);
    for s in Subs::new(body) {
        match &s.tag {
            b"INTV" if s.data.len() >= 8 => grid = Some((i32le(s.data, 0), i32le(s.data, 4))),
            b"VHGT" => heights = decode_vhgt(s.data),
            /* The authored per-vertex normals. Read because they are not a
               reconstruction of anything: measured against 300 vanilla cells,
               no triangulation and no gradient predicts VNML better than about
               half a degree at the median and several at p99, so deriving one
               is guessing at a number the file already states. It is what the
               game shades the ground with. See memory §11a. */
            b"VNML" => normals = decode_vnml(s.data),
            // The vertex colours, for the preview's ground. See `Land::colours`.
            b"VCLR" => colours = decode_vclr(s.data),
            b"VTEX" if s.data.len() >= 512 => {
                let mut raw = [0u16; 256];
                for (k, slot) in raw.iter_mut().enumerate() {
                    *slot = u16le(s.data, k * 2);
                }
                vtex = Some(unswizzle_vtex(&raw));
            }
            _ => {}
        }
    }
    let (grid, heights) = (grid?, heights?);
    Some(Land { grid, heights, normals, colours, vtex: vtex.unwrap_or_else(|| Box::new([0u16; 256])), plugin: plugin_idx })
}

/// Wraithguard: a path grid (PGRD) - its points, in the cell's own space for an exterior
/// (the game adds the cell's corner) and the world's for a room, and the edges between.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pathgrid {
    pub grid: (i32, i32),
    pub cell: String,
    pub points: Vec<[i32; 3]>,
    pub edges: Vec<(u32, u32)>,
    pub plugin: i32,
}

/// A PGRD record's body.
pub fn decode_pgrd(body: &[u8], plugin_idx: i32) -> Option<Pathgrid> {
    let mut p = Pathgrid { plugin: plugin_idx, ..Default::default() };
    let mut conns: Vec<u8> = Vec::new();
    let mut targets: Vec<u32> = Vec::new();
    let mut deleted = false;
    for s in Subs::new(body) {
        match &s.tag {
            b"DATA" if s.data.len() >= 8 => p.grid = (i32le(s.data, 0), i32le(s.data, 4)),
            b"NAME" => p.cell = cstring(s.data),
            b"DELE" => deleted = true,
            b"PGRP" => {
                for c in s.data.chunks_exact(16) {
                    p.points.push([i32le(c, 0), i32le(c, 4), i32le(c, 8)]);
                    conns.push(c[13]);
                }
            }
            b"PGRC" => {
                for c in s.data.chunks_exact(4) {
                    targets.push(u32le(c, 0));
                }
            }
            _ => {}
        }
    }
    if deleted {
        return None;
    }
    let mut at = 0usize;
    let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for (i, n) in conns.iter().enumerate() {
        for _ in 0..*n {
            // Each connection is listed from both ends; kept once, low index first.
            if let Some(&t) = targets.get(at) {
                let i = i as u32;
                if (t as usize) < p.points.len() && t != i {
                    let e = (i.min(t), i.max(t));
                    if seen.insert(e) {
                        p.edges.push(e);
                    }
                }
            }
            at += 1;
        }
    }
    Some(p)
}

/// A GMST's number: its INTV or FLTV (`None` for a string setting).
fn gmst_number(body: &[u8]) -> Option<(String, f32)> {
    let mut name = String::new();
    let mut val = None;
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => name = cstring(s.data),
            b"INTV" if s.data.len() >= 4 => val = Some(i32le(s.data, 0) as f32),
            b"FLTV" if s.data.len() >= 4 => val = Some(f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]])),
            _ => {}
        }
    }
    if name.is_empty() { None } else { val.map(|v| (name.to_ascii_lowercase(), v)) }
}

/// An NPC's or creature's id and its AI Hello (AIDT's first byte).
fn actor_hello(body: &[u8]) -> Option<(String, u8)> {
    let mut id = String::new();
    let mut hello = None;
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"AIDT" if !s.data.is_empty() => hello = Some(s.data[0]),
            _ => {}
        }
    }
    if id.is_empty() { None } else { hello.map(|h| (id.to_ascii_lowercase(), h)) }
}

pub fn parse_plugin(buf: &[u8], plugin_idx: i32, master_idx: &[i32], want_land: bool) -> PluginData {
    let own_only = OWN_READER_ONLY.load(std::sync::atomic::Ordering::Relaxed);
    parse_plugin_with(buf, plugin_idx, master_idx, want_land, !own_only)
}

/// `parse_plugin`, with the crate (`use_crate`) or with the subrecord walk alone.
fn parse_plugin_with(buf: &[u8], plugin_idx: i32, master_idx: &[i32], want_land: bool, use_crate: bool) -> PluginData {
    let mut d = PluginData::default();
    let mut recs = Records::new(buf);
    /* Round 18av: every record goes past now, not only the kinds the world keeps — the
       id index (`ids`) wants each one's name, which is its first subrecord and costs a
       glance. The kinds the world keeps are read as they were. */
    let want = |_: Tag| true;
    let keeps = |t: Tag| {
        t == TES3
            || t == CELL
            || t == LTEX
            || t == REGN
            || (want_land && t == LAND)
            || is_model_tag(t)
            || is_actor_tag(t)
            || t == *b"LEVC"
    };
    while let Some(rec) = recs.next_filtered(&want) {
        d.records += 1;
        if rec.tag != TES3 {
            if let Some(id) = crate::objects::record_id(rec.tag, rec.body) {
                d.ids.push((id.to_ascii_lowercase(), rec.tag));
            }
        }
        if let Some(o) = crate::objects::raw_object(rec.tag, rec.body) {
            d.objects.push(o);
        }
        crate::npc::collect(&mut d.npc, rec.tag, rec.body);
        /* Wraithguard: where each LAND record is (the merged-lands preview reads a cell's
           every version back from the file), the path grids, the numeric game settings, and
           each actor's AI Hello. */
        match &rec.tag {
            b"LAND" if want_land => {
                let at = rec.body.as_ptr() as usize - buf.as_ptr() as usize - 16;
                for s in Subs::new(rec.body) {
                    if &s.tag == b"INTV" && s.data.len() >= 8 {
                        d.land_at.push(((i32le(s.data, 0), i32le(s.data, 4)), at as u64, (16 + rec.body.len()) as u32));
                        break;
                    }
                }
            }
            b"PGRD" => {
                if let Some(p) = decode_pgrd(rec.body, plugin_idx) {
                    d.pathgrids.push(p);
                }
            }
            b"GMST" => {
                if let Some(g) = gmst_number(rec.body) {
                    d.gmst.push(g);
                }
            }
            b"NPC_" | b"CREA" => {
                if let Some(h) = actor_hello(rec.body) {
                    d.hello.push(h);
                }
            }
            _ => {}
        }
        if !keeps(rec.tag) {
            continue;
        }
        /* The records the world keeps are read by the crate (`from_crate`), each on its
           own; one it refuses, and the two kinds it cannot read without losing what was
           absent, go through the subrecord walk below. */
        if use_crate && from_crate::by_crate(rec.tag) {
            let at = rec.body.as_ptr() as usize - buf.as_ptr() as usize - 16;
            if let Some(obj) = from_crate::load(&buf[at..at + 16 + rec.body.len()]) {
                from_crate::take(&mut d, obj, rec.tag, rec.flags, plugin_idx, master_idx);
                continue;
            }
        }
        match rec.tag {
            TES3 => {
                for s in Subs::new(rec.body) {
                    if &s.tag == b"MAST" {
                        d.masters.push(cstring(s.data));
                    }
                }
            }
            LTEX => {
                let (mut id, mut idx, mut file, mut del) = (String::new(), 0u32, String::new(), false);
                for s in Subs::new(rec.body) {
                    match &s.tag {
                        b"NAME" => id = cstring(s.data),
                        b"INTV" if s.data.len() >= 4 => idx = u32le(s.data, 0),
                        b"DATA" => file = cstring(s.data),
                        b"DELE" => del = true,
                        _ => {}
                    }
                }
                if !del && !id.is_empty() {
                    d.ltex.push((idx, id, file));
                }
            }
            REGN => {
                let (mut id, mut fnam, mut del) = (String::new(), String::new(), false);
                for s in Subs::new(rec.body) {
                    match &s.tag {
                        b"NAME" => id = cstring(s.data),
                        b"FNAM" => fnam = cstring(s.data),
                        b"DELE" => del = true,
                        _ => {}
                    }
                }
                if !del && !id.is_empty() {
                    d.regions.push((id, fnam));
                }
            }
            LAND if want_land => {
                if let Some(l) = decode_land(rec.body, plugin_idx) {
                    d.lands.push(l);
                }
            }
            CELL => {
                let h = parse_cell_header(rec.body);
                if !h.has_data {
                    continue;
                }
                let mut refs = Vec::new();
                if h.refs_at < rec.body.len() {
                    parse_refs(&rec.body[h.refs_at..], plugin_idx, master_idx, &mut refs);
                }
                d.cells.push(RawCell { name: h.name, flags: h.flags, grid: h.grid,
                                       region: h.region, water: h.water, ambi: h.ambi, refs });
            }
            t if is_actor_tag(t) => {
                let creature = &t == b"CREA";
                let (mut id, mut modl, mut del) = (String::new(), String::new(), false);
                let mut health = None;
                for s in Subs::new(rec.body) {
                    match &s.tag {
                        b"NAME" => id = cstring(s.data),
                        b"MODL" => modl = cstring(s.data),
                        b"DELE" => del = true,
                        b"NPDT" => health = actor_health(creature, s.data),
                        _ => {}
                    }
                }
                if !id.is_empty() {
                    d.actors.push(ActorDef {
                        id,
                        model: modl,
                        twin: None,
                        creature,
                        health,
                        persistent: rec.flags & REC_PERSISTENT != 0,
                        deleted: del,
                    });
                }
            }
            t if is_model_tag(t) => {
                let (mut id, mut modl, mut del) = (String::new(), String::new(), false);
                let mut scripted = false;
                // Round 17m: LIGH records carry a light as well as a mesh.
                let mut lhdt: Option<&[u8]> = None;
                for s in Subs::new(rec.body) {
                    match &s.tag {
                        b"NAME" => id = cstring(s.data),
                        b"MODL" => modl = cstring(s.data),
                        b"SCRI" => scripted = !cstring(s.data).is_empty(),
                        b"DELE" => del = true,
                        b"LHDT" if s.data.len() >= 24 => lhdt = Some(s.data),
                        _ => {}
                    }
                }
                if !id.is_empty() {
                    if t == *b"LIGH" {
                        /* LHDT: weight f32, value i32, time i32, radius i32, colour
                           (r,g,b,unused bytes), flags i32. */
                        let (radius, colour, flags) = match lhdt {
                            Some(dd) => (
                                u32le(dd, 12) as i32,
                                [dd[16], dd[17], dd[18]],
                                u32le(dd, 20) as i32,
                            ),
                            None => (0, [255, 255, 255], 0),
                        };
                        d.lights.push(LightDef {
                            id: id.clone(),
                            radius,
                            colour,
                            flags,
                            deleted: del,
                        });
                    }
                    d.models.push(ModelDef { id, mesh: modl, deleted: del, animated_class: is_animated_class(t), scripted });
                }
            }
            _ => {}
        }
    }
    d
}

// ---------------------------------------------------------------------------

pub struct Cell {
    pub name: String,
    /// Wraithguard: every plugin (index into `World.plugins`) with a CELL record for this
    /// grid, in load order - the cell map's coverage (a reference moved in does not count).
    pub touched_by: Vec<u32>,
    pub grid: (i32, i32),
    pub region: String,
    /// Where the water sits, for a cell that has any. Exteriors are always at zero;
    /// this is here for the preview, which draws a surface to judge height limits by.
    pub water: Option<f32>,
    /// Merged references, keyed by RefNum so later plugins override rather than duplicate.
    pub refs: HashMap<RefNum, CellRef>,
    /// What the load order did to this cell on the way to the state above. Each
    /// reference that survived says which plugin supplied it; these are the ones that
    /// did not survive, and are the difference between "this cell has five rocks" and
    /// "this cell had eight until three mods got to it".
    pub prov: CellProv,
    /// Wraithguard: what a plugin took out of the cell - deleted or moved away - as it
    /// stood before. What "this cell without that mod" puts back.
    pub gone: Vec<Gone>,
}

/// A reference a plugin took out of a cell: the version before, which plugin, and whether
/// it was moved (MVRF) rather than deleted.
#[derive(Clone, Debug)]
pub struct Gone {
    pub r: CellRef,
    pub by: i32,
    pub moved: bool,
}

/// What later plugins did to one cell's references.
#[derive(Default, Debug, Clone, Copy)]
pub struct CellProv {
    /// A reference a later plugin redefined in place.
    pub replaced: usize,
    /// One a later plugin marked deleted.
    pub deleted: usize,
    /// One moved out of this cell into another.
    pub moved_out: usize,
    /// One moved into this cell from another.
    pub moved_in: usize,
}

#[derive(Default)]
pub struct World {
    pub plugins: Vec<String>,
    /// Wraithguard: the indices (into `plugins`) of the plugins an openmw.cfg loads on
    /// `groundcover=` lines. Their references are grass, and the cell preview draws
    /// them through the grass renderer rather than as ordinary objects.
    pub groundcover: std::collections::HashSet<usize>,
    /// Round 18av: where each plugin was read from, in the same order — the objects
    /// plugin reads an `.esp` again to copy a record, and sizes its masters.
    pub plugin_paths: Vec<PathBuf>,
    /// Round 18av: every STAT, ACTI, CREA and LEVC in the load order, by lowercased id,
    /// with the plugins that define it. The object picker's list and the objects
    /// plugin's source of truth (`objects.rs`).
    pub objects: HashMap<String, crate::objects::ObjectDef>,
    /// Round 18av: every id-bearing record, by lowercased id: the plugins that define it
    /// and the tag each used. For "does a master have this dependency?".
    pub record_where: HashMap<String, Vec<(usize, Tag)>>,
    /// Per plugin: the MAST names its header lists, in order. The load-order report
    /// shows them, and flags a master that is not in the order (round 14).
    pub masters: Vec<Vec<String>>,
    /// Per plugin: local LTEX index -> texture id.
    pub ltex_by_plugin: Vec<HashMap<u32, String>>,
    pub ltex_file: HashMap<String, String>,
    pub regions: HashMap<String, String>,
    pub cells: HashMap<(i32, i32), Cell>,
    /// Interior cells, by lowercased name: the name as the last plugin to touch it spelled
    /// it, and how many references it holds.
    ///
    /// **They are not in `cells` and cannot be.** That map is keyed by grid position and
    /// an interior has none — it is a room, not a square of the world. Nothing places
    /// grass in one yet either: there is no landscape record to stand on. What they are
    /// here for is that the tool has to be able to *name* them before it can ever do
    /// anything with them, and a cell picker that pretends they do not exist is a picker
    /// that cannot grow into one that does. See memory §18 260 for the decision this
    /// begins to unpick.
    pub interiors: HashMap<String, Interior>,
    pub lands: HashMap<(i32, i32), Land>,
    pub models: HashMap<String, String>,
    /// Round 18al: the records whose `x` twin in `models` was earned by their script
    /// alone - the `OutsideBanner` banners - with the plain file each draws *indoors*.
    /// Robin: "Make them wave as they did before. Only outdoors though, no interior
    /// banners wave in the wind" and "Interiors that are tagged to work as exteriors
    /// should of course still have their flags work as in exteriors too." A twin whose
    /// own `Idle` loops is not here: it moves wherever it stands. Read through
    /// `model_in`; `models` itself stays the open-air answer, which is what every
    /// obstacle, surface and rule is keyed by.
    pub indoor_plain: HashMap<String, String>,
    /// Round 17m: the LIGH records by lowercased id — radius, colour and flags.
    pub lights: HashMap<String, LightDef>,
    /// Creatures and NPCs by lowercased id. Only the corpses among them matter, but
    /// the living ones are kept so a reference to one is recognised as an actor and
    /// skipped, rather than falling through and being treated as an unknown object.
    pub actors: HashMap<String, ActorDef>,
    /// Wraithguard: what NPCs are assembled from - races, body parts, the NPC records and
    /// the armour and clothing they wear (`npc.rs`).
    pub npc: crate::npc::NpcTables,
    /// Round 18bc (B8): plugins that resolved to a file and then could not be read, or
    /// were empty — one line each, for the connect report's warnings.
    ///
    /// The read used to be `std::fs::read(p).unwrap_or_default()`, which turned any I/O
    /// failure — a permission error, a locked file, an MO2 or network share that dropped
    /// — into a zero-byte buffer. The plugin still took its slot, still counted towards
    /// "8 plugins", and contributed no cells, no references, no LTEX table and no
    /// landscape. The shell reports plugins that fail to *resolve*; this was the gap, and
    /// the harder failure to diagnose, because the plugin was listed as loaded.
    pub plugin_failures: Vec<String>,
    /// Wraithguard: every plugin's LAND for a grid, in load order - (plugin, offset, length)
    /// in its file, read back by the merged-lands preview (`mland.rs`).
    pub land_edits: HashMap<(i32, i32), Vec<(usize, u64, u32)>>,
    /// The path grids, the last plugin's for each exterior grid and each room.
    pub pathgrids_ext: HashMap<(i32, i32), Pathgrid>,
    pub pathgrids_int: HashMap<String, Pathgrid>,
    /// The numeric game settings, by lowercased name (the last plugin's).
    pub gmst: HashMap<String, f32>,
    /// Each actor's AI Hello, by lowercased id.
    pub hello: HashMap<String, u8>,
    pub stats: LoadStats,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct LoadStats {
    pub plugins: usize,
    pub bytes: u64,
    pub records: usize,
    pub cells: usize,
    pub refs_merged: usize,
    pub refs_overridden: usize,
    pub refs_deleted: usize,
    pub refs_moved: usize,
    pub lands: usize,
    /// Actor definitions in the load order, and how many of them spawn dead.
    pub actors: usize,
    pub corpses: usize,
    pub ms_read: u128,
    pub ms_parse: u128,
    pub ms_merge: u128,
}

impl World {
    /// One interior by name, however it is spelled — round 18bd (E8). See [`room_key`].
    pub fn room(&self, name: &str) -> Option<&Interior> {
        self.interiors.get(&room_key(name))
    }

    /// Every LAND in the load order.
    pub fn real_lands(&self) -> impl Iterator<Item = (&(i32, i32), &crate::land::Land)> {
        self.lands.iter()
    }

    /// Resolve a VTEX value against the plugin that wrote the LAND record.
    /// Indices are plugin-local, so the same number means different textures in
    /// different files; fall back down the load order when a plugin edits land
    /// without shipping its own LTEX.
    pub fn ltex_for(&self, vtex: u16, plugin: i32) -> Option<&str> {
        if vtex == 0 {
            // Not "no texture": index 0 is the engine's built-in land texture, which
            // is what unpainted ground is drawn with. Whole stretches of vanilla
            // landscape were never painted — the bed of the Odai in Balmora is one —
            // so treating 0 as nothing leaves them bare here and textured in game.
            return Some(DEFAULT_LTEX);
        }
        let ix = (vtex - 1) as u32;
        let start = plugin.max(0) as usize;
        for i in (0..=start.min(self.ltex_by_plugin.len().saturating_sub(1))).rev() {
            if let Some(id) = self.ltex_by_plugin[i].get(&ix) {
                return Some(id.as_str());
            }
        }
        None
    }


    /// Round 18al: `load`, with what the install says about the engine it runs on.
    pub fn load_with(paths: &[PathBuf], vfs: &Vfs, want_land: bool, opts: &LoadOpts) -> World {
        let t0 = std::time::Instant::now();
        // Round 18bc (B8): what could not be read is said out loud, not turned into an
        // empty plugin. The load still goes on with the rest — one unreadable file in a
        // hundred-plugin order is a warning, not a reason to come up with no install.
        let reads: Vec<Result<Vec<u8>, String>> =
            pool::par_map(paths, |_, p| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())));
        let ms_read = t0.elapsed().as_millis();
        let mut plugin_failures: Vec<String> = Vec::new();
        let bufs: Vec<Vec<u8>> = reads
            .into_iter()
            .enumerate()
            .map(|(i, r)| match r {
                Ok(b) if b.is_empty() => {
                    plugin_failures.push(format!(
                        "{}: the file is empty, so it loaded as no records at all",
                        paths[i].display()
                    ));
                    b
                }
                Ok(b) => b,
                Err(e) => {
                    plugin_failures.push(format!("could not be read, so it loaded as no records at all — {e}"));
                    Vec::new()
                }
            })
            .collect();

        let names: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
            .collect();
        let lower: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();

        // A plugin's master list must map to load-order indices before its
        // references can be resolved, and the header is at the very front of the
        // file, so read just that first.
        /* Round 18bf (I4): a master that is not installed gets its **own** negative id,
           from a table of missing names built across the whole order.

           It used to be a flat `-1` for every one of them. `RefNum` is
           `(content_file, index)` and the merge keys references on it, so two plugins
           each overriding a reference out of a *different* uninstalled master collided
           the moment the two FRMR indices matched — and they match constantly, because
           an index is just a counter inside its own file. The second reference erased
           the first, silently, and the object it edited went back to whatever the rest
           of the order said. `merge.rs` pins this for a plugin's own references; the
           missing-master case only ever had a single-plugin test, which cannot see it.

           Keyed by name rather than by slot, so the *same* missing master named by two
           plugins still collides — which is correct, those really do override each other
           — while two different ones never do. The first missing master is still -1, so
           nothing that only ever sees one of them changes. */
        let mut missing: Vec<String> = Vec::new();
        let mut master_maps: Vec<Vec<i32>> = Vec::with_capacity(bufs.len());
        for b in bufs.iter() {
            let mut recs = Records::new(b);
            let mut out = Vec::new();
            if let Some(rec) = recs.next_filtered(&|t| t == TES3) {
                for s in Subs::new(rec.body) {
                    if &s.tag == b"MAST" {
                        let m = cstring(s.data).to_ascii_lowercase();
                        out.push(match lower.iter().position(|n| *n == m) {
                            Some(i) => i as i32,
                            None => {
                                let k = match missing.iter().position(|n| *n == m) {
                                    Some(k) => k,
                                    None => {
                                        missing.push(m);
                                        missing.len() - 1
                                    }
                                };
                                -1 - k as i32
                            }
                        });
                    }
                }
            }
            master_maps.push(out);
        }

        let t1 = std::time::Instant::now();
        let parsed: Vec<PluginData> =
            pool::par_map(&bufs, |i, b| parse_plugin(b, i as i32, &master_maps[i], want_land));
        let ms_parse = t1.elapsed().as_millis();

        let t2 = std::time::Instant::now();
        let mut w = World { plugins: names, plugin_paths: paths.to_vec(), plugin_failures, ..Default::default() };
        // VTEX 0 belongs to no plugin, so its file has to be stated rather than read.
        w.ltex_file.insert(DEFAULT_LTEX.to_string(), DEFAULT_LTEX_FILE.to_string());
        w.stats.plugins = paths.len();
        w.stats.bytes = bufs.iter().map(|b| b.len() as u64).sum();
        w.stats.ms_read = ms_read;
        w.stats.ms_parse = ms_parse;

        /* The last water level anybody stated for a room, by name — which is not the same
           thing as the level in the last record about it. See the merge below. */
        let mut room_levels: HashMap<String, (f32, usize)> = HashMap::new();
        // Round 18ak: each twin's `.kf` read once, by path.
        let mut kf_seen: HashMap<String, bool> = HashMap::new();

        for (pi, d) in parsed.into_iter().enumerate() {
            w.stats.records += d.records;
            w.masters.push(d.masters.clone());
            w.npc.merge(d.npc);
            /* Round 18av: the object index and the id index. A deleted definition takes
               the record out of the picker; a later definition wins the kind, model and
               name, and every defining plugin is remembered for the masters question. */
            for o in d.objects {
                let k = o.id.to_ascii_lowercase();
                if o.deleted {
                    w.objects.remove(&k);
                    continue;
                }
                let e = w.objects.entry(k).or_default();
                e.id = o.id;
                e.kind = Some(o.kind);
                e.name = o.name;
                e.scale = o.scale;
                e.entries = o.entries;
                e.model = if o.kind == crate::objects::ObjKind::Creature && !o.model.is_empty() {
                    x_twin(vfs, &o.model).unwrap_or(o.model)
                } else {
                    o.model
                };
                if !e.defined_in.contains(&pi) {
                    e.defined_in.push(pi);
                }
            }
            for (id, tag) in d.ids {
                w.record_where.entry(id).or_default().push((pi, tag));
            }
            let mut ix = HashMap::new();
            for (i, id, file) in d.ltex {
                ix.insert(i, id.clone());
                w.ltex_file.insert(id.to_ascii_lowercase(), file);
            }
            w.ltex_by_plugin.push(ix);
            for (id, fnam) in d.regions {
                let disp = if fnam.is_empty() { id.clone() } else { fnam };
                w.regions.insert(id.to_ascii_lowercase(), disp);
            }
            for l in d.lands {
                w.lands.insert(l.grid, l); // later plugin wins
            }
            for (g, at, len) in d.land_at {
                w.land_edits.entry(g).or_default().push((pi, at, len));
            }
            for p in d.pathgrids {
                if p.grid == (0, 0) && !p.cell.is_empty() {
                    w.pathgrids_int.insert(room_key(&p.cell), p.clone());
                }
                w.pathgrids_ext.insert(p.grid, p);
            }
            for (k, v) in d.gmst {
                w.gmst.insert(k, v);
            }
            for (k, v) in d.hello {
                w.hello.insert(k, v);
            }
            for l in d.lights {
                let k = l.id.to_ascii_lowercase();
                if l.deleted {
                    w.lights.remove(&k);
                } else {
                    w.lights.insert(k, l);
                }
            }
            for md in d.models {
                let k = md.id.to_ascii_lowercase();
                if md.deleted {
                    w.models.remove(&k);
                    w.indoor_plain.remove(&k);
                } else if !md.mesh.is_empty() {
                    /* Round 18ag: the game's own substitution, made once, here, so that
                       every reader of `models` - the draw, the collision, the `.kf`
                       beside the mesh - sees the file the game actually loads.

                       Round 18ak: for the classes the game animates, but only where the
                       object would *move*. The game always loads the twin, then plays its
                       `Idle`; for a banner that clip has no length, and the sway and the
                       gale are `Idle2` and `Idle3`, which only a script ever calls for -
                       `OutsideBanner`, on 80 of the game's banner activators. The other 31
                       (`furn_bannerd_goods_hlaalu`, the Vivec Hlaalu banner, the tavern
                       variants, the shop signs) carry no script and hang still in every
                       weather. So an unscripted object takes the twin only when the twin's
                       own `Idle` has length - the port strider's twenty-six seconds - and
                       keeps the plain, still file otherwise. Robin: "Are there any
                       weathers where the banners are still in the game?" These, in all of
                       them.

                       Round 18al: and a scripted one takes it where its script can set
                       it moving (`LoadOpts::script_plays`). Robin: "There are some
                       banners that move in Gardenfell that doesn't in my load order, like
                       f\xFurn_bannerD_welcome_01.NIF … What makes them not move in the
                       game?" All three carry `OutsideBanner`, which asks for `Idle2` with
                       a plain `PlayGroup` - no flag. His game runs the Code Patch's
                       "Improved animation support" (feature 132, in `mcpatch\installed`,
                       named in MWSE.log), whose readme states what a flagless PlayGroup
                       then does: "The controller waits until the current animation is
                       complete before starting the given animation." The current
                       animation is the `Idle` the object was born in, one instant long,
                       and by his report it is never counted complete: the breeze stays
                       queued and the banner hangs still. The unpatched engine does leave
                       it (OpenMW issue #3385 watched them blow), and so does OpenMW. The
                       option is the engine's; the shell passes `true` whatever the
                       install says, on Robin's word: "for banners we make an exception:
                       Make them wave as they did before. Only outdoors though, no interior
                       banners wave in the wind" - the outdoors half is `indoor_plain`. */
                    let mesh = if md.animated_class {
                        match x_twin(vfs, &md.mesh) {
                            Some(t) if twin_moves_unbidden(vfs, &t, &mut kf_seen) => {
                                w.indoor_plain.remove(&k);
                                t
                            }
                            Some(t) if opts.script_plays && md.scripted => {
                                /* Round 18al, Robin: "for banners we make an exception:
                                   Make them wave as they did before. Only outdoors though,
                                   no interior banners wave in the wind." A twin the script
                                   alone earned is for the open air; indoors the record
                                   draws its plain file (`model_in`). */
                                w.indoor_plain.insert(k.clone(), md.mesh);
                                t
                            }
                            _ => {
                                w.indoor_plain.remove(&k);
                                md.mesh
                            }
                        }
                    } else {
                        w.indoor_plain.remove(&k);
                        md.mesh
                    };
                    w.models.insert(k, mesh);
                }
            }
            for mut a in d.actors {
                let k = a.id.to_ascii_lowercase();
                if a.deleted {
                    w.actors.remove(&k);
                } else {
                    // Round 18ag: a creature's twin, looked up once here (an NPC has no
                    // mesh of its own to twin).
                    a.twin = if a.creature && !a.model.is_empty() { x_twin(vfs, &a.model) } else { None };
                    // A later plugin replaces the definition outright, health and all,
                    // which is how mods turn a living actor into a corpse.
                    w.actors.insert(k, a);
                }
            }
            for rc in d.cells {
                if is_interior(rc.flags) {
                    /* Kept by name, and the later plugin wins — the same rule exteriors
                       follow, said with a name instead of a grid. An interior with no name
                       at all is not a place anybody can pick, so it is dropped here rather
                       than filling the list with blanks. */
                    if rc.name.trim().is_empty() {
                        continue;
                    }
                    let key = room_key(&rc.name);
                    let room = w.interiors.entry(key.clone()).or_insert_with(|| Interior {
                        name: rc.name.clone(),
                        ..Interior::default()
                    });
                    room.name = rc.name.clone();
                    if !room.touched_by.contains(&(pi as u32)) {
                        room.touched_by.push(pi as u32);
                    }
                    /* **Whether** a room has water is the last record's DATA flag; **how
                       high** is the last level anybody stated. Two facts, two sources, and
                       they have to be kept apart:

                       * the flag is a statement, and the last plugin to make it wins — a
                         stale level under a dry flag is noise, which is how Balmora, Lucky
                         Lockup came to be drawn flooded to the rafters;
                       * a missing level is **not** a statement. Half the plugins that
                         re-save a cell header carry no level subrecord at all, because the
                         tools that write them (merged patches, cleaned plugins) drop it,
                         and reading that silence as "water at zero" is what put
                         Addamasartus — vanilla's own `INTV -760` — under a flood to the
                         roof of the cave the moment any patch touched it.

                       So the level is remembered across the load order and only a record
                       that actually names one replaces it. A room whose flag is on and
                       whose level nobody ever stated has its water at zero, which is what
                       the flag alone means. */
                    if let Some(level) = rc.water {
                        room_levels.insert(key.clone(), (level, pi));
                    }
                    let stated = room_levels.get(&key).copied();
                    room.water = if rc.flags & 0x02 != 0 {
                        Some(stated.map(|(z, _)| z).unwrap_or(0.0))
                    } else {
                        None
                    };
                    room.water_flag_from = Some(pi);
                    room.water_from = stated.map(|(_, from)| from);
                    // Round 17y: the flag is the last header's; the light is the last stated.
                    room.quasi = crate::esp::is_quasi_exterior(rc.flags);
                    room.flags = rc.flags;
                    if rc.ambi.is_some() {
                        room.ambi = rc.ambi;
                    }
                    /* The same three things that can happen to a reference, said once
                       more. A room's contents are merged rather than replaced wholesale,
                       because a patch that moves one crate in a shop should not take the
                       rest of the shop with it. */
                    let mut moves: Vec<((i32, i32), CellRef)> = Vec::new();
                    for mut r in rc.refs {
                        r.plugin = pi as i32;
                        let prev = room.refs.get(&r.num);
                        r.touched = prev.map(|p| p.touched.clone()).unwrap_or_default();
                        r.poses = prev.map(|p| p.poses.clone()).unwrap_or_default();
                        r.touched.push(pi as i32);
                        r.poses.push(r.pose());
                        if let Some(target) = r.moved_to {
                            w.stats.refs_moved += 1;
                            room.prov.moved_out += 1;
                            if let Some(old) = room.refs.remove(&r.num) {
                                room.gone.push(Gone { r: old, by: pi as i32, moved: true });
                            }
                            if !r.deleted {
                                /* Round 18bd (E7): **the flag comes off on the way.**
                                   The reference is removed from the cell it left before
                                   it is pushed here, so a `CellRef` still carrying
                                   `moved_to` could only ever be found in the cell it
                                   *arrived* in — where the flag means the opposite of
                                   what nine call sites read it as ("this reference is not
                                   here"). The painted-statics scatter returned early on
                                   it, the brush refused to paint it, it cast no canopy,
                                   and it was left out of `wanted_meshes`, `meshes_in`,
                                   `meshes_in_interior` and the reach test — while
                                   `Obstructions::build` and `load_geometry` included it.
                                   So a mod that moves a rock from Ald Ruhn to Balmora
                                   gave you a rock you could not paint and could not grow
                                   grass on, that blocked grass anyway, with a painted
                                   rule on it doing nothing. It has moved; it is here now. */
                                r.moved_to = None;
                                moves.push((target, r));
                            }
                            continue;
                        }
                        if r.deleted {
                            w.stats.refs_deleted += 1;
                            room.prov.deleted += 1;
                            if let Some(old) = room.refs.remove(&r.num) {
                                room.gone.push(Gone { r: old, by: pi as i32, moved: false });
                            }
                            continue;
                        }
                        if room.refs.insert(r.num, r).is_some() {
                            w.stats.refs_overridden += 1;
                            room.prov.replaced += 1;
                        } else {
                            w.stats.refs_merged += 1;
                        }
                    }
                    /* A reference moved *out* of a room lands on the grid it was moved
                       to, which is an exterior — that is what a grid is. Dropping these
                       would lose an object the game does place, and lose it in the one
                       direction nobody would think to look. */
                    for (target, r) in moves {
                        let dst = w.cells.entry(target).or_insert_with(|| Cell {
                            name: String::new(),
                            grid: target,
                            region: String::new(),
                            water: Some(0.0),
                            refs: HashMap::new(),
                            prov: CellProv::default(),
                            touched_by: Vec::new(),
                            gone: Vec::new(),
                        });
                        dst.prov.moved_in += 1;
                        dst.refs.insert(r.num, r);
                    }
                    continue;
                }
                let cell = w.cells.entry(rc.grid).or_insert_with(|| Cell {
                    name: rc.name.clone(),
                    grid: rc.grid,
                    region: rc.region.clone(),
                    water: Some(0.0),
                    refs: HashMap::new(),
                    prov: CellProv::default(),
                    touched_by: Vec::new(),
                    gone: Vec::new(),
                });
                if !cell.touched_by.contains(&(pi as u32)) {
                    cell.touched_by.push(pi as u32);
                }
                if !rc.name.is_empty() {
                    cell.name = rc.name.clone();
                }
                if !rc.region.is_empty() {
                    cell.region = rc.region.clone();
                }
                let mut moves: Vec<((i32, i32), CellRef)> = Vec::new();
                for mut r in rc.refs {
                    // Which plugin the surviving reference came from. Recorded here
                    // because this is the only place that knows both the reference and
                    // where its plugin sits in the order.
                    r.plugin = pi as i32;
                    let prev = cell.refs.get(&r.num);
                    r.touched = prev.map(|p| p.touched.clone()).unwrap_or_default();
                    r.poses = prev.map(|p| p.poses.clone()).unwrap_or_default();
                    r.touched.push(pi as i32);
                    r.poses.push(r.pose());
                    if let Some(target) = r.moved_to {
                        w.stats.refs_moved += 1;
                        cell.prov.moved_out += 1;
                        if let Some(old) = cell.refs.remove(&r.num) {
                            cell.gone.push(Gone { r: old, by: pi as i32, moved: true });
                        }
                        if !r.deleted {
                            // Round 18bd (E7): it has arrived; it is not "not here". See
                            // the interior branch above for the whole of it.
                            r.moved_to = None;
                            moves.push((target, r));
                        }
                        continue;
                    }
                    if r.deleted {
                        w.stats.refs_deleted += 1;
                        cell.prov.deleted += 1;
                        if let Some(old) = cell.refs.remove(&r.num) {
                            cell.gone.push(Gone { r: old, by: pi as i32, moved: false });
                        }
                        continue;
                    }
                    if cell.refs.insert(r.num, r).is_some() {
                        w.stats.refs_overridden += 1;
                        cell.prov.replaced += 1;
                    } else {
                        w.stats.refs_merged += 1;
                    }
                }
                // A moved reference lands in the cell it was moved to, which may
                // be one we have not seen yet.
                for (target, r) in moves {
                    let dst = w.cells.entry(target).or_insert_with(|| Cell {
                        name: String::new(),
                        grid: target,
                        region: String::new(),
                        water: Some(0.0),
                        refs: HashMap::new(),
                        prov: CellProv::default(),
                        touched_by: Vec::new(),
                        gone: Vec::new(),
                    });
                    dst.prov.moved_in += 1;
                    dst.refs.insert(r.num, r);
                }
            }
        }
        w.stats.cells = w.cells.len();
        w.stats.lands = w.lands.len();
        w.stats.ms_merge = t2.elapsed().as_millis();
        // Round 18ax: every leveled list draws as its creatures' one mesh, or the marker.
        crate::objects::resolve_leveled(&mut w.objects);

        w
    }

    /// Round 18al: the file a model record draws where it stands - its `models` entry in
    /// the open air (an exterior, or a room flagged to behave like one), and indoors the
    /// plain, still file for a record whose twin only its script earned (`indoor_plain`).
    /// `indoors` is "an interior that is not a quasi-exterior".
    pub fn model_in(&self, id_lc: &str, indoors: bool) -> Option<&str> {
        if indoors {
            if let Some(p) = self.indoor_plain.get(id_lc) {
                return Some(p.as_str());
            }
        }
        self.models.get(id_lc).map(|s| s.as_str())
    }

}

impl World {
    /// Which cells and regions each land texture is actually painted in.
    ///
    /// The scope picker uses this to offer only places a texture really occurs,
    /// instead of every cell in the world. Walking every landscape's 16x16
    /// texture grid is the only way to know, so it is done once here rather than
    /// per keystroke in the UI.
    pub fn texture_usage(&self) -> HashMap<String, (Vec<String>, Vec<String>)> {
        let mut out: HashMap<String, (std::collections::BTreeSet<String>, std::collections::BTreeSet<String>)> =
            HashMap::new();
        for (grid, land) in &self.lands {
            let (name, region) = self
                .cells
                .get(grid)
                .map(|c| (c.name.clone(), c.region.clone()))
                .unwrap_or_default();
            let mut seen: Vec<u16> = Vec::with_capacity(8);
            for v in land.vtex.iter() {
                if seen.contains(v) {
                    continue;
                }
                seen.push(*v);
                let Some(tex) = self.ltex_for(*v, land.plugin) else { continue };
                let e = out.entry(tex.to_ascii_lowercase()).or_default();
                if !name.is_empty() {
                    e.0.insert(name.clone());
                }
                if !region.is_empty() {
                    e.1.insert(region.clone());
                }
            }
        }
        out.into_iter()
            .map(|(k, (c, r))| (k, (c.into_iter().collect(), r.into_iter().collect())))
            .collect()
    }
}

/// The extensions a load order entry can have and still be a file with records in it.
///
/// Round 18ab. OpenMW's `content=` is not only plugins: `.omwscripts` names a list of Lua
/// scripts, which has no records, no cells and no landscape. Robin's own cfg holds exactly
/// one `content=` line and it is one of those — so the load order was "not empty" by a
/// count of one, the world opened with nothing in it, and the program connected happily to
/// a game with no cells rather than saying what was wrong.
pub const PLUGIN_EXTS: [&str; 4] = [".esm", ".esp", ".omwgame", ".omwaddon"];

/// Whether a file name ends in one of [`PLUGIN_EXTS`] — `.omwaddon` and `.omwgame`
/// included, which is what makes an OpenMW install's own addons countable.
pub fn is_plugin_name(name: &str) -> bool {
    is_plugin(name)
}

fn is_plugin(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    PLUGIN_EXTS.iter().any(|e| n.ends_with(e))
}

/// Reads a load order from an OpenMW `openmw.cfg`, an MO2 profile `Morrowind.ini`,
/// or a plain `Morrowind.ini` in the game folder.
///
/// Only entries that name a file with records in them count — see `PLUGIN_EXTS`.
pub fn read_load_order(ini: &Path) -> Vec<String> {
    read_load_order_text(&std::fs::read_to_string(ini).unwrap_or_default())
}

/// The plugins an openmw.cfg's text loads on `groundcover=` lines, in order.
pub fn read_groundcover_text(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("groundcover="))
        .map(|n| n.trim().to_string())
        .filter(|n| is_plugin(n))
        .collect()
}

/// The plugins named by an openmw.cfg's text (`content=`, then `groundcover=`) or a
/// Morrowind.ini's (`GameFileN=`), in load order.
pub fn read_load_order_text(text: &str) -> Vec<String> {
    let mut indexed: Vec<(usize, String)> = Vec::new();
    let mut content: Vec<String> = Vec::new();
    // Wraithguard: OpenMW's `groundcover=` plugins, loaded after every `content=` one.
    // Their references are the grass the cell preview shows.
    let mut groundcover: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("content=") {
            let name = rest.trim();
            if is_plugin(name) {
                content.push(name.to_string());
            }
        } else if let Some(rest) = t.strip_prefix("groundcover=") {
            let name = rest.trim();
            if is_plugin(name) {
                groundcover.push(name.to_string());
            }
        } else if let Some(rest) = t.strip_prefix("GameFile") {
            if let Some(eq) = rest.find('=') {
                if let Ok(n) = rest[..eq].trim().parse::<usize>() {
                    indexed.push((n, rest[eq + 1..].trim().to_string()));
                }
            }
        }
    }
    if !content.is_empty() {
        for g in groundcover {
            if !content.iter().any(|c| c.eq_ignore_ascii_case(&g)) {
                content.push(g);
            }
        }
        return content;
    }
    indexed.sort_by_key(|(n, _)| *n);
    indexed.into_iter().map(|(_, s)| s).collect()
}

#[cfg(test)]
mod wraithguard_order_tests {
    use super::read_load_order_text;

    #[test]
    fn groundcover_loads_after_content() {
        let cfg = "content=Morrowind.esm\ngroundcover=Grass.esp\ncontent=Mod.esp\n";
        assert_eq!(read_load_order_text(cfg), ["Morrowind.esm", "Mod.esp", "Grass.esp"]);
    }

    #[test]
    fn a_plugin_on_both_lists_loads_once() {
        let cfg = "content=Morrowind.esm\ncontent=Grass.esp\ngroundcover=grass.esp\n";
        assert_eq!(read_load_order_text(cfg), ["Morrowind.esm", "Grass.esp"]);
    }
}
