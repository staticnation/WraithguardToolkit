//! Load-order resolution: plugins in, a merged world out.
//!
//! Every plugin is parsed on its own thread into an isolated result, then the
//! results are folded together in load order. Parsing is the expensive half and
//! it is embarrassingly parallel; merging is a few hash inserts per record and
//! has to be sequential to respect override order, so it stays on one thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::esp::*;
use crate::land::{colours_of, heights_of, normals_of, vtex_of, Land, DEFAULT_HEIGHT, VERTS};
use crate::nif;
use crate::pool;
use crate::tes3::raw;
use crate::tes3::types::Object;
use crate::vfs::Vfs;

/// Where the invented cell lives. Far outside anything Morrowind or Tamriel Rebuilt
/// uses — the real world runs to roughly ±160, and the largest province projects to a
/// couple of hundred — so it can never collide with a cell somebody actually has, and a
/// stray reference to it is obviously not a real place.
///
/// **Far out, but not absurdly far.** It used to be ±30 000, which put the patch at
/// ±2.5e8 in world units — and an `f32` there has a spacing of *sixteen units*. Every
/// blade's stored position snapped to a 16-unit grid while its height had been worked
/// out at the true position, so blades sat up to three units off the ground they were
/// measured against: sunk on ground falling away, floating on ground rising. Robin
/// reported it as grass looking "more sunk into the ground" on the patch than on real
/// terrain, and he was right — `tests/parity.rs` measured 3.07 units against 0.001 on a
/// real cell.
///
/// At ±900 the same spacing is half a unit, which is below anything the eye can find.
/// Real cells never had the problem: at ±200 the spacing is an eighth of a unit.
pub const PATCH_GRID: (i32, i32) = (-900, -900);

#[derive(Default)]
pub struct PluginData {
    /// The records of the placeable kinds this plugin deletes (`DELE` or the header's
    /// bit), by lowercased id and tag: what [`placed_types`] takes back out.
    pub ids_deleted: Vec<(String, Tag)>,
    /// Round 18gh: the raw layer could not read the file at all (a record claiming more
    /// bytes than there are, a subrecord past its record's end), and why. Everything else
    /// in here is empty then: there is no resync and no guess, the file is unreadable.
    pub unreadable: Option<String>,
    /// Round 18gh: records of a kind the world keeps whose subrecords did not fit the
    /// typed layout, skipped. Zero over every plugin Robin has; the fixtures that are
    /// malformed on purpose land here, with the reason on stderr under `GDN_ESP_DEBUG`.
    pub records_refused: usize,
    /// Round 18gf: records deleted by the header bit alone (no DELE), for the log.
    pub deleted_by_flag: usize,
    /// Round 18gf: LAND records given default heights, and LAND subrecords ignored because
    /// the record's DATA flags said they were not there.
    pub lands_defaulted: usize,
    pub lands_flagged_out: usize,
    pub masters: Vec<String>,
    pub ltex: Vec<(u32, String, String)>, // index, id, filename
    pub regions: Vec<(String, String)>,
    pub cells: Vec<RawCell>,
    pub lands: Vec<Land>,
    /// Round 18gf: grids whose LAND record this plugin deletes.
    pub lands_deleted: Vec<(i32, i32)>,
    /// Every model record: id, mesh, deleted, whether the record's class is one the
    /// game animates (round 18ag, see `x_twin`) and whether it carries a script (18ak).
    pub models: Vec<ModelDef>,
    /// Round 17m: the LIGH records among them, with what LHDT says about the light.
    pub lights: Vec<LightDef>,
    pub actors: Vec<ActorDef>,
    /// Round 18av: every STAT, ACTI, CREA and LEVC, for the object picker and the
    /// objects plugin (`objects.rs`).
    pub objects: Vec<crate::records::RawObject>,
    /// Round 18av: every record that has an id, by lowercased id and tag — what the
    /// objects plugin's dependency search asks "does a master define this?" of.
    pub ids: Vec<(String, Tag)>,
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
    /// Its tag: which of an id's types this definition is ([`placed_types`]).
    pub tag: Tag,
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
    /// Round 18cr: read the collision meshes beside the merge rather than after it. On
    /// by default; the test that pins the two paths equal turns it off for its reference.
    pub overlap_geometry: bool,
    /// Round 18gf (backend-plan D2): what to do with an `MVRF` whose reference word does
    /// not name the `FRMR` that follows it. The game (as `tes3` reads it, cell.rs:101-106)
    /// drops the move and keeps the reference where it was; OpenMW applies the move to
    /// whatever reference comes next (loadcell.cpp:295-321). True for an MGE XE or plain
    /// install, false for OpenMW. The Construction Set never writes a mismatch, so on a
    /// well-formed load order the two are the same world.
    pub mvrf_strict: bool,
    /// An id one plugin defines as one physical kind and another as another (a WEAP and
    /// a MISC, a STAT and a CREA): off (the default), a later record replaces the earlier
    /// kind (round 18gn, esp-format §9.4, as `merge_to_master` merges). On, OpenMW's
    /// rule ([`placed_types`]): every kind is kept, a deletion takes out its own kind only,
    /// and a placed object is the kind whose tag ranks highest, whatever the load order.
    pub openmw_kinds: bool,
    /// Wraithguard: read every placed mesh's collision geometry into `geom` (on, the
    /// default). Off for a front end that never asks `geom` - no grass to keep out of
    /// rocks - so the connect skips reading and parsing every mesh the load order places
    /// (33,267 meshes and 1.78 GB on a 621-plugin setup, 39-43 s on 16 threads). The
    /// counts that do not need a mesh (actors, corpses) are kept; `mesh_wanted` still
    /// says how many meshes the world places.
    pub geometry: bool,
}

impl Default for LoadOpts {
    fn default() -> Self {
        LoadOpts { script_plays: true, overlap_geometry: true, mvrf_strict: true, openmw_kinds: false, geometry: true }
    }
}

impl LoadOpts {
    /// The options an install's order file implies: an `openmw.cfg` is an OpenMW install,
    /// which reads a mismatched MVRF positionally; anything else is the game's own reader
    /// (round 18gf, backend-plan D2). The front ends start from this and set the rest.
    pub fn for_order_file(order_file: &Path) -> LoadOpts {
        LoadOpts { mvrf_strict: !crate::paths::is_openmw_cfg(order_file), ..Default::default() }
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
    /// Round 18gf: the record deletes the cell - a `DELE` subrecord or the header's
    /// deleted bit. The game forgets the cell's references; the landscape under an
    /// exterior stays, because that is a LAND record and not this one.
    pub deleted: bool,
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
}

/// Where a moved reference is now: an exterior grid or a room, as [`World::load_with`]
/// tracks it across the load order (round 18gf).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Where {
    Ext((i32, i32)),
    Int(String),
}

/// Round 18gf (backend-plan D2): under `mvrf_strict`, a move whose MVRF named another
/// reference than the FRMR that followed is dropped and the reference stays where it was,
/// as the game reads it (`tes3` cell.rs:101-106). OpenMW applies it anyway. Answers
/// whether a move was dropped.
fn drop_mismatched_move(r: &mut CellRef, opts: &LoadOpts) -> bool {
    if opts.mvrf_strict && r.moved_mismatch && r.moved_to.is_some() {
        r.moved_to = None;
        true
    } else {
        false
    }
}

/// A reference is leaving the place a previous move put it. If that place is not the cell
/// the record sits in, the copy there has to go too; `evict` collects those.
fn leave(where_is: &mut HashMap<RefNum, Where>, num: RefNum, here: &Where, evict: &mut Vec<(RefNum, Where)>) {
    if let Some(prev) = where_is.remove(&num) {
        if prev != *here {
            evict.push((num, prev));
        }
    }
}

/// Takes the collected references out of the cells a previous move left them in; answers
/// how many were actually there.
fn evict_all(
    cells: &mut HashMap<(i32, i32), Cell>,
    interiors: &mut HashMap<String, Interior>,
    evict: Vec<(RefNum, Where)>,
) -> usize {
    let mut n = 0;
    for (num, prev) in evict {
        let gone = match prev {
            Where::Ext(g) => cells.get_mut(&g).is_some_and(|c| c.refs.remove(&num).is_some()),
            Where::Int(k) => interiors.get_mut(&k).is_some_and(|r| r.refs.remove(&num).is_some()),
        };
        if gone {
            n += 1;
        }
    }
    n
}

/// An interior's name as [`World::interiors`] keys it — round 18bd (E8).
///
/// Lowercased **through the whole alphabet**. Six of the seven lookups used
/// `to_ascii_lowercase` and the insert used `to_lowercase`, so for any room whose name
/// carries a non-ASCII letter — and the reader maps a plugin's bytes one to a char
/// (Windows-1252 since 18gh, `bytes::decode`), so cp1251 uppercase Cyrillic (the first letter of essentially every interior
/// name in a Russian localisation) and cp1252 `Ä Ö Ü É À` all land there — the room
/// **opened, listed and drew normally** and then: no grass preview at all, the brush laid
/// nothing and reported zero objects, no tint, no surfaces read, and "clear the paint in
/// view" reported success having cleared nothing. Meanwhile `rules_in_scope` and the
/// Advanced export *did* use `to_lowercase`, so the export happily scoped that room — a
/// room grassed by mesh rules exported grass the preview could not show.
///
/// One function, so the two halves cannot drift apart again. Round 18gq: and since a
/// Russian install's records are read in Windows-1251 now, those first letters are the
/// Cyrillic capitals they always were, which `to_lowercase` folds and `to_ascii_lowercase`
/// never did.
pub fn room_key(name: &str) -> String {
    if name.is_ascii() { name.to_ascii_lowercase() } else { name.to_lowercase() }
}

/// Reserved mesh name for the body-sized slab that stands in for an NPC corpse.
/// It is not a file and never resolves through the overlay.
pub const CORPSE_SLAB: &str = "__corpse_slab";

/// Whether a reference is a body on the ground, and which kind. Only used to report
/// the census honestly — an NPC corpse and a dead creature block grass the same way,
/// but the panel counts them separately because they arrive by different routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corpse {
    No,
    /// An NPC, standing in with the slab because it has no mesh of its own.
    Npc,
    Creature,
}

/// What a reference is, for obstacle purposes. See [`World::obstacle_ref`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleRef<'a> {
    /// An actor that is alive, and will not be standing here.
    LivingActor,
    /// An editor marker: invisible in game, so nothing to keep clear of.
    Marker,
    /// Nothing in the load order says what this id looks like.
    Unknown,
    /// Something with a shape. `model` is what to test against.
    Mesh { model: &'a str, corpse: Corpse },
}

/// `marker_*` anywhere in the path: Morrowind's editor markers, which the game never
/// draws. Any path segment, not just the file name, so a mod that keeps them in a
/// `marker_stuff` folder is caught too — the same rule the page applies.
///
/// **Compared as bytes** (round 18bb). The reader decodes a plugin's MODL one byte to
/// one char (Windows-1252 since 18gh), so a Cyrillic mesh path — a Russian, Polish or Czech
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
/// This is the only copy. The page had one too (`08_scatter.js`'s `CORPSE_PROFILE`,
/// compared by `t_corpseslab.js`) until the second engine replaced the page's scatter;
/// since then the page draws the slab the engine hands it and never builds one
/// (round 18hp cleaned the comment that still said otherwise).
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
    let accel = crate::triaccel::TriAccel::build(&tris);
    nif::MeshGeom {
        accel,
        tris: std::sync::Arc::new(tris),
        aabb_min: [-SLAB_L, -w_max, 0.0],
        aabb_max: [SLAB_L, w_max, SLAB_H],
        // The slab is a hull, not a rendered shape: it stands in for a body, and a
        // body is solid all the way through.
        shape: nif::ColShape::Hull,
        radius: (SLAB_L * SLAB_L + w_max * w_max + SLAB_H * SLAB_H * 0.25f32).sqrt(),
    }
}

/// Creatures and NPCs. Their definitions are read for corpse detection only.
fn is_actor_tag(t: Tag) -> bool {
    matches!(&t, b"CREA" | b"NPC_")
}

/// The Persistent bit in a record header — the CS labels it "Corpses persist".
const REC_PERSISTENT: u32 = 0x0400;
/// The record header's deleted bit. The Construction Set sets it alongside a `DELE`
/// subrecord; `tes3` treats either one as deletion, and so does the world (round 18gf,
/// backend-plan D1). What the game itself does with the bit alone is untested.
pub const REC_DELETED: u32 = 0x0020;

/// An actor's health as the engine wants it, from the typed record (round 18gh).
///
/// A creature's `NPDT` is 96 bytes of stats with Health among them; one without the
/// subrecord has no health to read. An NPC's `NPDT` carries its stats only in the 52-byte
/// form, where Health is a 16-bit word read as signed; the 12-byte form means
/// auto-calculated stats and carries no health at all.
fn actor_health(obj: &Object, has_npdt: bool) -> Option<i32> {
    match obj {
        Object::Creature(c) if has_npdt => Some(c.data.health as i32),
        Object::Npc(n) => n.data.stats.as_ref().map(|s| s.health as i16 as i32),
        _ => None,
    }
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
    let Some((kfp, key)) = twin_kf(twin) else { return false };
    if let Some(&v) = seen.get(&key) {
        return v;
    }
    let moves = kf_has_idle(vfs, &kfp);
    seen.insert(key, moves);
    moves
}

/// The `.kf` beside a twin mesh, and the key `twin_moves_unbidden` caches it by.
fn twin_kf(twin: &str) -> Option<(String, String)> {
    let dot = twin.rfind('.')?;
    if twin[dot..].contains(['\\', '/']) {
        return None;
    }
    let kfp = format!("{}.kf", &twin[..dot]);
    let key = kfp.to_ascii_lowercase().replace('\\', "/");
    Some((kfp, key))
}

/// Whether the `.kf` at `kfp` has an `Idle` with length (read and parsed whole).
fn kf_has_idle(vfs: &Vfs, kfp: &str) -> bool {
    vfs.resolve_mesh(kfp)
        .and_then(|loc| vfs.load(&loc))
        .and_then(|b| crate::nif::parse_kf(&b))
        .map(|kf| kf.clip("Idle").is_some())
        .unwrap_or(false)
}

/// Wraithguard: the merge's twin questions, answered ahead and on the pool - every
/// animated-class model's `x` twin (by its mesh, exactly as written), and whether each
/// twin's `.kf` moves unbidden (by the key `twin_moves_unbidden` uses). The merge asked
/// both one record at a time: on a 621-plugin load order, 35,073 twin lookups (2.1 s)
/// and 614 `.kf` files read in turn (9.5-11.4 s). The answers do not depend on the order
/// they are asked in, so the merge's result is unchanged; this asks the superset (every
/// definition, winning or not), which is the same set the merge visited.
fn twins_ahead(parsed: &[PluginData], vfs: &Vfs) -> (HashMap<String, Option<String>>, HashMap<String, bool>) {
    let mut meshes: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for d in parsed {
        for md in &d.models {
            if md.animated_class && !md.deleted && !md.mesh.is_empty() && seen.insert(md.mesh.as_str()) {
                meshes.push(md.mesh.clone());
            }
        }
    }
    let twins: Vec<Option<String>> = pool::par_map(&meshes, |_, m| x_twin(vfs, m));
    let mut kfs: Vec<(String, String)> = Vec::new();
    let mut keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in twins.iter().flatten() {
        if let Some((kfp, key)) = twin_kf(t) {
            if keys.insert(key.clone()) {
                kfs.push((kfp, key));
            }
        }
    }
    let moves: Vec<bool> = pool::par_map(&kfs, |_, (kfp, _)| kf_has_idle(vfs, kfp));
    let kf_seen = kfs.into_iter().zip(moves).map(|((_, key), m)| (key, m)).collect();
    (meshes.into_iter().zip(twins).collect(), kf_seen)
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

/// Whether a record of this kind is one the world keeps a view of.
fn keeps(t: &Tag, want_land: bool) -> bool {
    *t == TES3
        || *t == CELL
        || *t == LTEX
        || *t == REGN
        || (want_land && *t == LAND)
        || is_model_tag(*t)
        || is_actor_tag(*t)
        || t == b"LEVC"
}

/// The id, mesh and whether a script is named, for the record kinds that carry a
/// placeable mesh (`is_model_tag`), moved out of the typed object.
fn model_parts(obj: Object) -> Option<(String, String, bool)> {
    Some(match obj {
        Object::Static(x) => (x.id, x.mesh, false),
        Object::Activator(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Door(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Container(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Light(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::MiscItem(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Weapon(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Armor(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Clothing(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Book(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Alchemy(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Apparatus(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Ingredient(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Lockpick(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::Probe(x) => (x.id, x.mesh, !x.script.is_empty()),
        Object::RepairItem(x) => (x.id, x.mesh, !x.script.is_empty()),
        _ => return None,
    })
}

/// Reads one plugin into its views. `parse_plugin_named` is the same with the plugin's
/// name for the `GDN_ESP_DEBUG` line; this one is for callers that read a file alone.
pub fn parse_plugin(buf: &[u8], plugin_idx: i32, master_idx: &[i32], want_land: bool) -> PluginData {
    parse_plugin_named(buf, "", plugin_idx, master_idx, want_land)
}

/// Round 18gh: the plugin is read through the typed model. `tes3::raw::parse_plugin`
/// gives every record with its subrecords, byte-exact; the kinds the world keeps go
/// through `tes3::types::Object::from_raw` and the views are filled from the typed object.
/// A record that does not fit its layout is skipped and counted (`records_refused`),
/// and a file the raw layer cannot read is an empty `PluginData` with `unreadable` set.
/// Nothing here reads a byte of the file itself; the few questions the typed form does
/// not answer (was there a `DATA` at all, which spelling the water came in, is this
/// `DELE` the cell's or a reference's) are asked of the raw record's subrecord tags.
/// What types a record for the world: `(record) -> its typed form, None for a kind it
/// does not type, or why it refused`. [`Object::from_raw`] unless another reader is set.
pub type RecordTyper = fn(&raw::Record) -> Result<Option<Object>, String>;

static RECORD_TYPER: std::sync::OnceLock<RecordTyper> = std::sync::OnceLock::new();

/// Type records with another reader than this crate's own (a program embedding the engine
/// with a reader of its own, whose types it converts to these). Set once, before the first
/// plugin is read; later calls are refused.
pub fn set_record_typer(typer: RecordTyper) -> Result<(), RecordTyper> {
    RECORD_TYPER.set(typer)
}

fn type_record(rec: &raw::Record) -> Result<Option<Object>, String> {
    match RECORD_TYPER.get() {
        Some(t) => t(rec),
        None => Object::from_raw(rec).map_err(|e| e.to_string()),
    }
}

pub fn parse_plugin_named(
    buf: &[u8],
    name: &str,
    plugin_idx: i32,
    master_idx: &[i32],
    want_land: bool,
) -> PluginData {
    let mut d = PluginData::default();
    let recs = match raw::parse_plugin(buf) {
        Ok(r) => r,
        Err(e) => {
            d.unreadable = Some(e.to_string());
            return d;
        }
    };
    let debug = std::env::var_os("GDN_ESP_DEBUG").is_some();
    for rec in recs {
        d.records += 1;
        /* Round 18av: every record goes past, not only the kinds the world keeps — the
        id index (`ids`) wants each one's name, which is its first subrecord and costs a
        glance at the raw record. The kinds the world keeps are typed below. */
        if rec.tag != TES3 {
            if let Some(id) = crate::records::record_id(&rec) {
                if is_placeable(&rec.tag) && (rec.flags & REC_DELETED != 0 || rec.has(b"DELE")) {
                    d.ids_deleted.push((id.to_ascii_lowercase(), rec.tag));
                }
                d.ids.push((id.to_ascii_lowercase(), rec.tag));
            }
        }
        if !keeps(&rec.tag, want_land) {
            continue;
        }
        let obj = match type_record(&rec) {
            Ok(Some(o)) => o,
            Ok(None) => continue,
            Err(e) => {
                d.records_refused += 1;
                if debug {
                    let id = crate::records::record_id(&rec).unwrap_or_default();
                    eprintln!("{name}: {} {id:?} refused: {e}", rec.tag_str());
                }
                continue;
            }
        };
        let del_flag = rec.flags & REC_DELETED != 0;
        /* Round 18gf: deleted by the header bit alone, with no DELE subrecord, is what
        the log counts. The typed object's flags already say *whether* it is deleted
        (the bit or the subrecord, D1); whether the subrecord was there is the raw
        record's to say, and for a cell only a DELE outside a reference's run counts.
        Asked of a cell only when the bit is set, which is nearly never. */
        let has_dele = if rec.tag == CELL { del_flag && cell_has_dele(&rec) } else { rec.has(b"DELE") };
        let by_flag = del_flag && !has_dele;
        if let Some(o) = crate::records::raw_object(&obj) {
            if by_flag {
                d.deleted_by_flag += 1;
            }
            d.objects.push(o);
        }
        match obj {
            Object::Header(h) => {
                d.masters.extend(h.masters.into_iter().map(|(name, _)| name));
            }
            Object::LandscapeTexture(t) => {
                if by_flag {
                    d.deleted_by_flag += 1;
                }
                if !(has_dele || del_flag) && !t.id.is_empty() {
                    d.ltex.push((t.index, t.id, t.file_name));
                }
            }
            Object::Region(r) => {
                if by_flag {
                    d.deleted_by_flag += 1;
                }
                if !(has_dele || del_flag) && !r.id.is_empty() {
                    d.regions.push((r.id, r.name));
                }
            }
            Object::Landscape(l) => {
                /* The typed record holds each block by presence and the DATA flags beside
                them, so the 18gf rule is applied here as it always was. The blocks are
                turned into the engine's arrays by `land.rs` (`heights_of` and friends),
                the same sums in the same order as the byte decoders they replaced. */
                let mut heights = l.vertex_heights.as_ref().map(heights_of);
                /* The authored per-vertex normals. Read because they are not a
                reconstruction of anything: measured against 300 vanilla cells,
                no triangulation and no gradient predicts VNML better than about
                half a degree at the median and several at p99, so deriving one
                is guessing at a number the file already states. It is what the
                game shades the ground with, so it is what a blade should stand
                on and what a slope test should measure. See memory §11a. */
                let mut normals = l.vertex_normals.as_ref().and_then(normals_of);
                // The vertex colours, for the preview's ground. See `Land::colours`.
                let mut colours = l.vertex_colors.as_ref().map(colours_of);
                let mut vtex = l.texture_indices.as_ref().map(vtex_of);
                /* Round 18gf: the record's DATA flags say which blocks are *in use* - 0x1
                heights and normals, 0x2 colours, 0x4 textures - and the game reads a
                block only when its flag is set (OpenMW loadland.cpp:103-125; `tes3`
                saves by the same flags). Absent DATA means every block that is there
                counts, which is how the fixtures have always been read. */
                if rec.has(b"DATA") {
                    let f = l.landscape_flags;
                    let mut off = 0;
                    if f & 0x1 == 0 && (heights.is_some() || normals.is_some()) {
                        off += heights.is_some() as usize + normals.is_some() as usize;
                        heights = None;
                        normals = None;
                    }
                    if f & 0x2 == 0 && colours.is_some() {
                        off += 1;
                        colours = None;
                    }
                    if f & 0x4 == 0 && vtex.is_some() {
                        off += 1;
                        vtex = None;
                    }
                    d.lands_flagged_out += off;
                }
                if by_flag {
                    d.deleted_by_flag += 1;
                }
                if rec.has(b"INTV") {
                    let grid = l.grid;
                    if has_dele || del_flag {
                        d.lands_deleted.push(grid);
                    } else {
                        /* Round 18gf (backend-plan item 4): a later LAND with the grid but
                           no heights used to be skipped, so the earlier plugin's ground
                           survived. Both engines replace the record whole and show flat
                           default ground (OpenMW `DEFAULT_HEIGHT`, -2048). */
                        let heights = match heights {
                            Some(h) => h,
                            None => {
                                d.lands_defaulted += 1;
                                Box::new([DEFAULT_HEIGHT; VERTS * VERTS])
                            }
                        };
                        d.lands.push(Land {
                            grid,
                            heights,
                            normals,
                            colours,
                            vtex: vtex.unwrap_or_else(|| Box::new([0u16; 256])),
                            plugin: plugin_idx,
                        });
                    }
                }
            }
            Object::Cell(c) => {
                let h = CellHeader::of(&c, &rec);
                if !h.has_data {
                    continue;
                }
                let refs: Vec<CellRef> = c
                    .references
                    .into_iter()
                    .map(|r| CellRef::from_typed(r, plugin_idx, master_idx))
                    .collect();
                if by_flag {
                    d.deleted_by_flag += 1;
                }
                d.cells.push(RawCell {
                    name: h.name,
                    flags: h.flags,
                    grid: h.grid,
                    region: h.region,
                    water: h.water,
                    ambi: h.ambi,
                    refs,
                    deleted: h.deleted,
                });
            }
            Object::Creature(_) | Object::Npc(_) => {
                let creature = rec.tag == *b"CREA";
                let health = actor_health(&obj, rec.has(b"NPDT"));
                let (id, modl) = match obj {
                    Object::Creature(c) => (c.id, c.mesh),
                    Object::Npc(n) => (n.id, n.mesh),
                    _ => continue,
                };
                if by_flag {
                    d.deleted_by_flag += 1;
                }
                if !id.is_empty() {
                    d.actors.push(ActorDef {
                        id,
                        model: modl,
                        twin: None,
                        creature,
                        health,
                        persistent: rec.flags & REC_PERSISTENT != 0,
                        deleted: has_dele || del_flag,
                    });
                }
            }
            obj if is_model_tag(rec.tag) => {
                // Round 17m: LIGH records carry a light as well as a mesh.
                let light = match &obj {
                    Object::Light(x) => Some(if rec.has(b"LHDT") {
                        (
                            x.data.radius as i32,
                            [x.data.color[0], x.data.color[1], x.data.color[2]],
                            x.data.flags as i32,
                        )
                    } else {
                        (0, [255, 255, 255], 0)
                    }),
                    _ => None,
                };
                let Some((id, modl, scripted)) = model_parts(obj) else { continue };
                if by_flag {
                    d.deleted_by_flag += 1;
                }
                let del = has_dele || del_flag;
                if !id.is_empty() {
                    if let Some((radius, colour, flags)) = light {
                        d.lights.push(LightDef { id: id.clone(), radius, colour, flags, deleted: del });
                    }
                    d.models.push(ModelDef {
                        id,
                        mesh: modl,
                        deleted: del,
                        animated_class: is_animated_class(rec.tag),
                        tag: rec.tag,
                        scripted,
                    });
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
}

/// What later plugins did to one cell's references.
#[derive(Default, Debug, Clone, Copy)]
pub struct CellProv {
    /// A reference a later plugin redefined in place.
    pub replaced: usize,
    /// One a later plugin marked deleted.
    pub deleted: usize,
    /// Round 18fc: a record for a reference an *earlier* plugin had deleted - a re-placement
    /// or a move. The game keeps the deleted flag and edits the hidden thing, so nothing
    /// comes back; counted so the panel can say why a plugin's flag is not there.
    pub kept_deleted: usize,
    /// One moved out of this cell into another.
    pub moved_out: usize,
    /// One moved into this cell from another.
    pub moved_in: usize,
}

/// One version of a reference's placement: what it is, where, and how big.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    pub id: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: f32,
}

impl Pose {
    /// The placement a reference's version states.
    pub fn of(r: &CellRef) -> Pose {
        Pose { id: r.id.clone(), pos: r.pos, rot: r.rot, scale: r.scale }
    }
}

/// Every version of one reference the load order supplied, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefHistory {
    /// Every plugin (load-order index) that supplied a version, in load order; the last
    /// is the reference's `plugin`.
    pub touched: Vec<i32>,
    /// Each version's placement, beside `touched` (one per entry, the last the one in
    /// force): what a later plugin moved, rotated or swapped.
    pub poses: Vec<Pose>,
}

/// A reference a plugin took out of a cell: the version that was there, the plugin that
/// took it (load-order index), and whether it was moved away rather than deleted.
#[derive(Clone, Debug)]
pub struct Gone {
    pub r: CellRef,
    pub by: i32,
    pub moved: bool,
}

/// What the load order did to one cell (an interior, or an exterior grid).
#[derive(Clone, Debug, Default)]
pub struct CellHistory {
    /// Every plugin (load-order index) with a CELL record for it, in load order (a
    /// reference moved in does not count).
    pub touched_by: Vec<u32>,
    /// The references a later plugin took out of it - deleted, or moved away - with the
    /// plugin that did it, so a front end can say what a plugin removed.
    pub gone: Vec<Gone>,
}

impl RefHistory {
    /// Records plugin `pi`'s version of `r` in `refs` (a field of [`History`], passed on
    /// its own so the merge can hold a cell's history at the same time).
    fn supplied(refs: &mut HashMap<RefNum, RefHistory>, r: &CellRef, pi: usize) {
        // Wraithguard: nearly every reference has one version. `Vec::push` on an empty Vec
        // takes room for four (a `Pose` is 56 bytes with its id: about 250 bytes a reference,
        // across 8.6M of them), so the first push is sized for one; a second grows it.
        let h = refs.entry(r.num).or_insert_with(|| RefHistory {
            touched: Vec::with_capacity(1),
            poses: Vec::with_capacity(1),
        });
        h.touched.push(pi as i32);
        h.poses.push(Pose::of(r));
    }
}

/// What each plugin did, kept beside the merged world rather than inside its cells and
/// references, so nothing that builds a `Cell` or a `CellRef` has to know about it.
#[derive(Clone, Debug, Default)]
pub struct History {
    /// By reference number (unique across the load order, so a reference moved to another
    /// cell keeps its history).
    pub refs: HashMap<RefNum, RefHistory>,
    /// By interior key (as `World::interiors`).
    pub interiors: HashMap<String, CellHistory>,
    /// By exterior grid.
    pub exteriors: HashMap<(i32, i32), CellHistory>,
}

impl History {
    /// A reference's history; empty for one the merge never saw.
    pub fn of(&self, num: &RefNum) -> &RefHistory {
        static EMPTY: RefHistory = RefHistory { touched: Vec::new(), poses: Vec::new() };
        self.refs.get(num).unwrap_or(&EMPTY)
    }

    /// An interior's history; empty for one no plugin had.
    pub fn interior(&self, key: &str) -> &CellHistory {
        static EMPTY: CellHistory = CellHistory { touched_by: Vec::new(), gone: Vec::new() };
        self.interiors.get(key).unwrap_or(&EMPTY)
    }

    /// An exterior cell's history; empty for one no plugin had.
    pub fn exterior(&self, grid: (i32, i32)) -> &CellHistory {
        static EMPTY: CellHistory = CellHistory { touched_by: Vec::new(), gone: Vec::new() };
        self.exteriors.get(&grid).unwrap_or(&EMPTY)
    }

}

impl CellHistory {
    /// Records that plugin `pi` has a CELL record for this cell.
    fn touched(&mut self, pi: usize) {
        if !self.touched_by.contains(&(pi as u32)) {
            self.touched_by.push(pi as u32);
        }
    }
}

#[derive(Default)]
pub struct World {
    pub plugins: Vec<String>,
    /// Round 18av: where each plugin was read from, in the same order — the objects
    /// plugin reads an `.esp` again to copy a record, and sizes its masters.
    pub plugin_paths: Vec<PathBuf>,
    /// Round 18av: every STAT, ACTI, CREA and LEVC in the load order, by lowercased id,
    /// with the plugins that define it. The object picker's list and the objects
    /// plugin's source of truth (`objects.rs`).
    pub objects: HashMap<String, crate::records::ObjectDef>,
    /// Round 18av: every id-bearing record, by lowercased id: the plugins that define it
    /// and the tag each used. For "does a master have this dependency?".
    pub record_where: HashMap<String, Vec<(usize, Tag)>>,
    /// Per plugin: the MAST names its header lists, in order. The load-order report
    /// shows them, and flags a master that is not in the order (round 14).
    pub masters: Vec<Vec<String>>,
    /// Per plugin: local LTEX index -> texture id.
    pub ltex_by_plugin: Vec<HashMap<u32, String>>,
    pub ltex_file: HashMap<String, String>,
    /// Round 18fv: texture ids the synthetic patch minted records for because no plugin
    /// defines them (lowercase). They exist so a rule on a texture the install lacks can
    /// still be previewed - drawn as a checkerboard, since there is no image - and they
    /// are left out of every list that says what the install *has*: the validator, the
    /// texture list, the plugin importer's names. Robin: "When there is no id that
    /// matches the Land texture on a rule, preview it with the gray and white checker
    /// board terrain texture."
    pub invented_ltex: std::collections::HashSet<String>,
    pub regions: HashMap<String, String>,
    pub cells: HashMap<(i32, i32), Cell>,
    /// Round 18gf: each exterior cell's DATA flags as the last CELL record for it stated
    /// them (0x02 is "has water", the one vanilla sets). The Objects plugin, which the game
    /// loads, writes a cell's flags back as it found them rather than as zero, because the
    /// game's merge takes the last record's flags whole and a zero would drain the cell.
    pub cell_flags: HashMap<(i32, i32), u32>,
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
    /// Round 18ey: the meshes at least one *animated-class* record names, lowercased and
    /// slash-normalised - the only ones whose `.kf` the game ever opens.
    ///
    /// Robin: *"For static pointing at an x mesh with a .kf beside it, how does the game
    /// understand that it is animated or not? Can we read that and act accordingly?"* It
    /// reads nothing from the mesh. A `.kf` is played through the animation system, and
    /// the animation system is attached to the classes `is_animated_class` lists (OpenMW's
    /// `useAnim()`: ACTI, CONT, DOOR, LIGH) and to actors. A STAT never gets one, so its
    /// `.kf` is never opened **even when its own `model` names an `x` file with a `.kf`
    /// sitting right beside it** - the case the tool got wrong, animating what the game
    /// draws still. Controllers *inside* the .nif are a different thing entirely and do
    /// run for a static: a NiBSAnimationNode, a NiUVController, a NiFlipController, a
    /// NiKeyframeController on a node. That is why Tamriel Rebuilt's water circle turns
    /// while a STAT pointing at `xbanner.nif` hangs still.
    ///
    /// Per mesh rather than per reference, which is the one compromise here: a mesh named
    /// by an activator *and* by a static is animated for both, because the parsed mesh and
    /// its pose are cached by path on each side of the wire. No vanilla record does it.
    pub anim_meshes: std::collections::HashSet<String>,
    /// Round 18ez: the lowercased ids whose *winning* definition is of an animated class -
    /// the per-reference half of the same question, which is the half the preview needs:
    /// a static and an activator can share one file (the activator's twin), and only the
    /// activator's references move.
    pub anim_ids: std::collections::HashSet<String>,
    /// Round 18ez: the meshes (lowercased, slash-normalised) with a `.kf` beside them in
    /// this load order - the only ones for which "still" is a different picture at all.
    /// A static on a mesh with no `.kf` draws the one face the mesh has, and asking the
    /// page to keep a second identical copy of it would cost a read and a batch for
    /// nothing; `refs_json` says `still` only for these.
    pub kf_meshes: std::collections::HashSet<String>,
    /// Round 17m: the LIGH records by lowercased id — radius, colour and flags.
    pub lights: HashMap<String, LightDef>,
    /// Creatures and NPCs by lowercased id. Only the corpses among them matter, but
    /// the living ones are kept so a reference to one is recognised as an actor and
    /// skipped, rather than falling through and being treated as an unknown object.
    pub actors: HashMap<String, ActorDef>,
    pub geom: HashMap<String, nif::MeshGeom>,
    /// Why each wanted mesh is missing from `geom`, by lowercased name: 1 not found
    /// in the overlay, 2 found but unreadable. The census needs to tell those apart —
    /// "the mod forgot to ship it" and "we could not read it" are different problems
    /// and the panel has always listed them on separate lines.
    pub mesh_failed: HashMap<String, u8>,
    /// Meshes that were found but could not be read, kept so the reason can be
    /// reported rather than guessed at.
    pub mesh_failures: Vec<String>,
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
    /// The half-width, in game units, of the part of the preview patch that is actually
    /// drawn — set only when the patch is painted wider than it is shown.
    ///
    /// "Cover it all" asks for a patch of a chosen size with no thinning anywhere inside
    /// it, and coverage cannot be made to stop on a line: it falls off over the half tile
    /// either side of the last painted texel. The only way to have both is to paint
    /// further out than the view reaches, so the falloff happens on ground nobody is
    /// looking at — and then to keep grass off that ground, or it stands in mid-air past
    /// the edge of the terrain. This is that boundary. `None` everywhere else, including
    /// every real cell: a real cell is drawn as far as it is painted.
    pub patch_clip: Option<f32>,
    /// The paint-only rule the preview patch is standing in for — set only when the
    /// patch was built to preview one. A paint-only rule is about no land texture, so a
    /// patch for it is bare ground; what makes its grass appear is the preview pass
    /// painting the patch's own square with this rule ([`crate::patch::overlay_paint`]),
    /// the same way a person would. The extent and the full switch ride along because
    /// the paint has to cover exactly the ground a texture patch would have carried —
    /// Robin: "I want only the chosen size to scatter the rule." `None` for a patch
    /// built for a texture, and for every real cell.
    pub patch_rule: Option<PatchRule>,
    pub stats: LoadStats,
    /// What each plugin did to the references and cells (see [`History`]).
    pub history: History,
}

/// See [`World::patch_rule`].
#[derive(Debug, Clone)]
pub struct PatchRule {
    /// The rule's id ([`crate::config::Selector::id`]).
    pub id: String,
    /// The patch size the person chose, in game units — the same number `install` took.
    pub extent: f32,
    /// Whether "Cover it all" was on.
    pub full: bool,
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
    /// Round 18fc: later records for references an earlier plugin deleted, dropped
    /// because the game keeps the flag (see the merge loop).
    pub refs_kept_deleted: usize,
    /// Round 18gf: moves dropped because the MVRF named another reference than the FRMR
    /// that followed, under `LoadOpts::mvrf_strict`. The reference stays where it was.
    pub refs_move_dropped: usize,
    /// Round 18gf: references taken out of the cell a *previous* move had put them in,
    /// because a later plugin moved them again or placed them somewhere else.
    pub refs_relocated: usize,
    /// Round 18gf: records that were deleted by the header's 0x20 bit alone, with no
    /// DELE subrecord - the Construction Set writes both, so these came from another tool.
    pub records_deleted_by_flag: usize,
    /// Round 18gf: cell records that deleted a cell.
    pub cells_deleted: usize,
    /// Round 18gf: LAND records with a grid but no usable VHGT, which replace the earlier
    /// landscape with flat default ground as the game does, rather than being skipped.
    pub lands_defaulted: usize,
    /// Round 18gf: LAND subrecords present in the file but switched off by the record's
    /// DATA flags, and therefore ignored as the game ignores them.
    pub lands_flagged_out: usize,
    /// Round 18gn (backend-plan item 11): ids a later plugin defined as another kind of
    /// physical object than an earlier one had (a STAT turned CREA, a LIGH turned STAT),
    /// so the earlier entry was taken out of its table. Zero on every vanilla order.
    pub ids_rekinded: usize,
    pub lands: usize,
    /// Actor definitions in the load order, and how many of them spawn dead.
    pub actors: usize,
    pub corpses: usize,
    /// References in the world that point at one of those corpses.
    pub corpse_refs: usize,
    pub meshes: usize,
    /// Diagnostics for the mesh pipeline, which is easy to have silently fail:
    /// referenced objects that name a mesh, of those how many were found on
    /// disk, and how many of those actually parsed.
    pub mesh_wanted: usize,
    pub mesh_unresolved: usize,
    pub mesh_unparsed: usize,
    pub mesh_collision: usize,
    /// Round 18cr: how many wanted meshes the read beside the merge did not cover, so
    /// `load_geometry` read them itself afterwards. Zero is the overlap working.
    pub mesh_late: usize,
    pub ms_read: u128,
    pub ms_parse: u128,
    pub ms_merge: u128,
    /// Wraithguard: `ms_merge` is the merge loop and the mesh read beside it, whichever is
    /// longer. These split it: how long the loop alone took (from the merge's start to its
    /// last plugin), and how long the loop then waited on the mesh read. A wait near zero
    /// means the loop is the bound; a long one, the mesh read is.
    pub ms_merge_loop: u128,
    pub ms_geometry_wait: u128,
    /// Wraithguard: where the exterior references' time goes, sampled (every 64th reference
    /// is timed and the sample scaled): recording each version in the history, and putting
    /// it in its cell. The rest of `ms_merge_loop` is neither.
    pub ms_merge_history: u128,
    pub ms_merge_inserts: u128,
    /// Wraithguard: the whole of `ms_merge_loop`, by part (every part timed, nothing
    /// sampled; the parts add up to the loop). In order, [`MERGE_PARTS`]: before the
    /// first plugin, each plugin's object definitions, its small tables, its lights,
    /// models and actors, its interiors, its exteriors (the history and the inserts are
    /// inside this one), moved and evicted references, and what each plugin leaves to free.
    pub ms_merge_parts: [u128; 8],
    /// Wraithguard: inside the definitions part, timed in full: the lights, the models,
    /// the actors (each loop whole), and, inside those, the `x` twin lookups and the
    /// twins' `.kf` reads. Then the counts: model records, twin lookups, `.kf` files read.
    pub ms_defs: [u128; 5],
    pub defs_counts: [u64; 3],
    /// Wraithguard: `twins_ahead` before the loop (inside its setup part): how long, and
    /// how many meshes it asked for a twin and `.kf` files it read.
    pub ms_twins_ahead: u128,
    /// Wraithguard: the mesh geometry read beside the merge (round 18cr's): how long
    /// listing every mesh the load order could want took (`pre_geometry`, before the
    /// merge), how many that is, how long the read took on the pool (wall), and, summed
    /// over its threads, ms reading the files, ms at the `.kf` beside them, ms parsing,
    /// and the bytes read (late reads in `load_geometry` included).
    pub ms_geometry_list: u128,
    pub mesh_pre: usize,
    pub ms_geometry_read: u128,
    pub geometry_clock: [u64; 4],
    pub twins_ahead: [u64; 2],
    pub ms_meshes: u128,
}

/// Wraithguard: the names of [`LoadStats::ms_merge_parts`], in its order.
pub const MERGE_PARTS: [&str; 8] =
    ["setup", "objects", "tables", "definitions", "interiors", "exteriors", "moves", "freeing"];

/// Wraithguard: `f`, its time added to `ns` and counted in `n` (the definitions' timers).
fn timed<T>(ns: &mut u128, n: &mut u64, f: impl FnOnce() -> T) -> T {
    let t = std::time::Instant::now();
    let v = f();
    *ns += t.elapsed().as_nanos();
    *n += 1;
    v
}

/// Wraithguard: a stopwatch over the merge's parts - `to(part)` closes the running part
/// and starts the next, so every moment of the loop lands in exactly one.
struct Laps {
    at: std::time::Instant,
    part: usize,
    ns: [u128; 8],
}

impl Laps {
    fn to(&mut self, part: usize) {
        let now = std::time::Instant::now();
        self.ns[self.part] += now.duration_since(self.at).as_nanos();
        self.at = now;
        self.part = part;
    }
}

impl World {
    /// One interior by name, however it is spelled — round 18bd (E8). See [`room_key`].
    pub fn room(&self, name: &str) -> Option<&Interior> {
        self.interiors.get(&room_key(name))
    }

    /// The landscape that is real ground: every LAND in the load order but the preview
    /// patch (`patch::PATCH_GRID`), the cell the simplified view invents. Everything that
    /// means "the whole world" walks this — an export, the region list, a reach answer.
    /// Round 18av found the patch in a whole-world export: after a rule had been looked at
    /// in the simplified view, the plugin carried a "Preview patch" cell at (-900, -900)
    /// full of grass, and a region split filed it under "(no region)".
    pub fn real_lands(&self) -> impl Iterator<Item = (&(i32, i32), &crate::land::Land)> {
        self.lands.iter().filter(|(g, _)| **g != PATCH_GRID)
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

    pub fn load(paths: &[PathBuf], vfs: &Vfs, want_land: bool) -> World {
        Self::load_with(paths, vfs, want_land, &LoadOpts::default())
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
            // Round 18gh: the header alone, through the typed `Header` (`esp::plugin_head`
            // reads just the first record), with the masters lower-cased as it gives them.
            let masters = plugin_head(b).map(|h| h.masters).unwrap_or_default();
            let mut out = Vec::with_capacity(masters.len());
            for m in masters {
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
            master_maps.push(out);
        }

        let t1 = std::time::Instant::now();
        let parsed: Vec<PluginData> = pool::par_map(&bufs, |i, b| {
            parse_plugin_named(
                b,
                names.get(i).map_or("", |n| n.as_str()),
                i as i32,
                &master_maps[i],
                want_land,
            )
        });
        let ms_parse = t1.elapsed().as_millis();
        /* Round 18gh: a file the raw layer refused loaded as no records, and the report
        says so beside the files that could not be read at all (round 18bc B8). The
        reason names the record and the byte, which is what to look at. */
        for (i, d) in parsed.iter().enumerate() {
            if let Some(why) = &d.unreadable {
                plugin_failures.push(format!(
                    "{}: not readable as a plugin, so it loaded as no records at all — {why}",
                    paths[i].display()
                ));
            }
        }

        /* Round 18cr: the collision meshes are read *while the merge runs*.

           The merge is one thread walking every plugin in order - it has to be, later
           overrides earlier - and on Robin's order it is 1.25 s; the mesh read after it
           was another 1.08 s on sixteen threads, waiting for the merge to say which
           meshes the world ends up wanting. It need not wait: every mesh the merged world
           *could* want is knowable from the parsed plugins alone - every id any cell
           references, and every mesh any definition of that id names (the `x` twin
           beside it too, since the merge may pick that) - so that superset is read on the
           pool beside the merge, and `load_geometry` then takes exactly the meshes it
           always took from what was read, reading any it did not find. The `geom` map,
           the counts and the failure lines come out the same by construction: the same
           list, the same parser, the same order; only the clock differs. */
        let t_list = std::time::Instant::now();
        let (pre_list, pre_anim): (Vec<String>, std::collections::HashSet<String>) =
            if opts.overlap_geometry && opts.geometry { pre_geometry(&parsed, vfs) } else { (Vec::new(), Default::default()) };
        let ms_geometry_list = t_list.elapsed().as_millis();
        let clock = GeomClock::default();
        let mut ms_geometry_read: u128 = 0;
        let t2 = std::time::Instant::now();
        let mut laps = Laps { at: t2, part: 0, ns: [0; 8] };
        let mut defs_ns = [0u128; 5];
        let mut defs_n = [0u64; 3];
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
        // Round 18ak: each twin's `.kf` read once, by path. Wraithguard: answered ahead,
        // on the pool, with every animated model's twin (`twins_ahead`).
        let t_twins = std::time::Instant::now();
        let (twin_of, mut kf_seen) = twins_ahead(&parsed, vfs);
        let ms_twins_ahead = t_twins.elapsed().as_millis();
        let n_kf_ahead = kf_seen.len() as u64;
        /* Wraithguard (0017): a deletion does not stand against a later plugin. The last
           record for a reference is the one that stands, whole: a plugin that deletes it
           and a later plugin whose plain record names it again bring it back, with the later
           plugin's placement; a deletion that comes last still deletes it. Round 18fc kept
           a `dead` set here and dropped every record after a deletion (the game keeps the
           object and hides it, from the community: "moving it just moves the hidden
           thing"); Morrowind.exe takes the later record (Wraithguard's in-game test,
           `tests/ingame/` `WG_Del_*.esp`: five piles), and OpenMW's `CellRefList::load`
           replaces the earlier record whole too. */
        /* Round 18gf (backend-plan item 5): where each moved reference is now. A reference
           plugin B moved from X to Y and plugin C then moves from X to Z used to be drawn
           in Y *and* Z, because a move only removed it from the cell the record sat in.
           OpenMW wipes the earlier lease (store.cpp:688-705); so does this, by remembering
           where the last move put it. Only moves are tracked: a plain record for a
           reference in the cell it was written in is the ordinary override. */
        let mut where_is: HashMap<RefNum, Where> = HashMap::new();

        /* The superset's reads, started here and collected after the merge. A scoped
           thread rather than the pool directly: the pool would block this thread, and
           the merge is what this thread is for. Inside, `par_map` owns the machine. */
        let mut ms_merge_loop: u128 = 0;
        let mut ms_geometry_wait: u128 = 0;
        let mut ns_history: u128 = 0;
        let mut ns_inserts: u128 = 0;
        let mut n_refs: u64 = 0;
        let pre: HashMap<String, GeomRead> = std::thread::scope(|sc| {
            let reader = (!pre_list.is_empty()).then(|| {
                let (pa, pl, ck) = (&pre_anim, &pre_list, &clock);
                sc.spawn(move || {
                    let t = std::time::Instant::now();
                    let v = pool::par_map(pl, |_, m| {
                        read_geometry(vfs, m, pa.contains(&m.replace('\\', "/").to_ascii_lowercase()), ck)
                    });
                    (v, t.elapsed().as_millis())
                })
            });
        // Which type each id is placed as, when plugins define it as more than one.
        let placed = if opts.openmw_kinds { placed_types(&parsed) } else { HashMap::new() };
        let is_placed = |tag: &Tag, id: &str| placed.get(&id.to_ascii_lowercase()).map_or(true, |t| t == tag);
        /* Wraithguard: the history holds an entry for every reference any plugin supplied;
           room for them all at once, rather than a rehash of millions at every doubling. */
        w.history.refs.reserve(
            parsed.iter().map(|d| d.cells.iter().map(|c| c.refs.len()).sum::<usize>()).sum(),
        );
        for (pi, d) in parsed.into_iter().enumerate() {
            laps.to(2);
            w.stats.records += d.records;
            w.stats.records_deleted_by_flag += d.deleted_by_flag;
            w.stats.lands_defaulted += d.lands_defaulted;
            w.stats.lands_flagged_out += d.lands_flagged_out;
            w.masters.push(d.masters.clone());
            laps.to(1);
            /* Round 18av: the object index and the id index. A deleted definition takes
               the record out of the picker; a later definition wins the kind, model and
               name, and every defining plugin is remembered for the masters question. */
            for o in d.objects {
                if !is_placed(&o.kind.tag(), &o.id) {
                    continue;
                }
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
                e.model = if o.kind == crate::records::ObjKind::Creature && !o.model.is_empty() {
                    x_twin(vfs, &o.model).unwrap_or(o.model)
                } else {
                    o.model
                };
                if !e.defined_in.contains(&pi) {
                    e.defined_in.push(pi);
                }
            }
            laps.to(2);
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
            // Round 18gf: a deleted LAND record takes the landscape away (`merge_to_master`
            // remove_deleted.rs:434-436); the cell under it keeps its references.
            for g in d.lands_deleted {
                w.lands.remove(&g);
            }
            /* Round 18gn (backend-plan item 11, esp-format §9.4): every physical object
               shares one id namespace in the game, and a later plugin's record replaces an
               earlier one of another kind, type and all (`merge_to_master`: "For 'physical'
               objects the id should be unique amongst all other physical objects"). The
               three tables stay three, because everything downstream is keyed by them, but
               a record arriving in one takes the id out of the others, and a deletion takes
               it out of all of them, whichever kind the deleting record was. Counted, so a
               digest that moved can be read against the connect log. */
            laps.to(3);
            let t_defs = std::time::Instant::now();
            let lit: std::collections::HashSet<String> =
                d.lights.iter().map(|l| l.id.to_ascii_lowercase()).collect();
            for l in d.lights {
                if !is_placed(b"LIGH", &l.id) {
                    continue;
                }
                let k = l.id.to_ascii_lowercase();
                if l.deleted {
                    w.lights.remove(&k);
                } else {
                    w.lights.insert(k, l);
                }
            }
            let t_models = std::time::Instant::now();
            defs_ns[0] += t_models.duration_since(t_defs).as_nanos();
            for md in d.models {
                defs_n[0] += 1;
                if !is_placed(&md.tag, &md.id) {
                    continue;
                }
                let k = md.id.to_ascii_lowercase();
                if w.actors.remove(&k).is_some() {
                    w.stats.ids_rekinded += 1;
                }
                if !lit.contains(&k) && w.lights.remove(&k).is_some() {
                    w.stats.ids_rekinded += 1;
                }
                if md.deleted {
                    w.models.remove(&k);
                    w.anim_ids.remove(&k);
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
                        let twin = timed(&mut defs_ns[3], &mut defs_n[1], || match twin_of.get(&md.mesh) {
                            Some(t) => t.clone(),
                            None => x_twin(vfs, &md.mesh),
                        });
                        match twin {
                            Some(t) if {
                                let cached = kf_seen.len();
                                let tk = std::time::Instant::now();
                                let m = twin_moves_unbidden(vfs, &t, &mut kf_seen);
                                if kf_seen.len() > cached {
                                    defs_ns[4] += tk.elapsed().as_nanos();
                                    defs_n[2] += 1;
                                }
                                m
                            } => {
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
                    // Round 18ey: and whether this mesh is one the game would ever
                    // animate, which is what decides if its `.kf` is opened at all.
                    if md.animated_class {
                        w.anim_meshes.insert(mesh.replace('\\', "/").to_ascii_lowercase());
                        w.anim_ids.insert(k.clone());
                    } else {
                        // A later plugin can redefine an activator as a static; the last
                        // word wins here as it does for the mesh.
                        w.anim_ids.remove(&k);
                    }
                    w.models.insert(k, mesh);
                }
            }
            let t_actors = std::time::Instant::now();
            defs_ns[1] += t_actors.duration_since(t_models).as_nanos();
            for mut a in d.actors {
                if !is_placed(if a.creature { b"CREA" } else { b"NPC_" }, &a.id) {
                    continue;
                }
                let k = a.id.to_ascii_lowercase();
                if w.models.remove(&k).is_some() {
                    w.stats.ids_rekinded += 1;
                    w.anim_ids.remove(&k);
                    w.indoor_plain.remove(&k);
                    w.lights.remove(&k);
                }
                if a.deleted {
                    w.actors.remove(&k);
                } else {
                    // Round 18ag: a creature's twin, looked up once here (an NPC has no
                    // mesh of its own to twin).
                    a.twin = if a.creature && !a.model.is_empty() {
                        timed(&mut defs_ns[3], &mut defs_n[1], || x_twin(vfs, &a.model))
                    } else {
                        None
                    };
                    /* Round 18ey: an actor is animated too - a living creature walks by
                       the `.kf` beside its twin - so both files it may be drawn from count
                       as meshes the game animates. The corpse is drawn from the plain file
                       and still, which is what `twin` beside `model` is for (18ag); the
                       plain file is listed here all the same, because a *living* actor is
                       drawn from it when the load order ships no twin. */
                    for m in [Some(&a.model), a.twin.as_ref()].into_iter().flatten() {
                        if !m.is_empty() {
                            w.anim_meshes.insert(m.replace('\\', "/").to_ascii_lowercase());
                        }
                    }
                    // A later plugin replaces the definition outright, health and all,
                    // which is how mods turn a living actor into a corpse.
                    w.actors.insert(k, a);
                }
            }
            defs_ns[2] += t_actors.elapsed().as_nanos();
            for rc in d.cells {
                laps.to(if is_interior(rc.flags) { 4 } else { 5 });
                if is_interior(rc.flags) {
                    /* Kept by name, and the later plugin wins — the same rule exteriors
                       follow, said with a name instead of a grid. An interior with no name
                       at all is not a place anybody can pick, so it is dropped here rather
                       than filling the list with blanks. */
                    if rc.name.trim().is_empty() {
                        continue;
                    }
                    let key = room_key(&rc.name);
                    // Round 18gf: a record that deletes the room takes it out of the world,
                    // references and all, as `tes3`'s deleted bit and OpenMW's erase do.
                    if rc.deleted {
                        w.stats.cells_deleted += 1;
                        if let Some(old) = w.interiors.remove(&key) {
                            for n in old.refs.keys() {
                                where_is.remove(n);
                            }
                        }
                        w.history.interiors.remove(&key);
                        room_levels.remove(&key);
                        continue;
                    }
                    let room = w.interiors.entry(key.clone()).or_insert_with(|| Interior {
                        name: rc.name.clone(),
                        ..Interior::default()
                    });
                    room.name = rc.name.clone();
                    let hist = w.history.interiors.entry(key.clone()).or_default();
                    hist.touched(pi);
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
                    let here = Where::Int(key.clone());
                    let mut evict: Vec<(RefNum, Where)> = Vec::new();
                    for mut r in rc.refs {
                        r.plugin = pi as i32;
                        // Every version of the reference, and its placement in each.
                        RefHistory::supplied(&mut w.history.refs, &r, pi);
                        if drop_mismatched_move(&mut r, opts) {
                            w.stats.refs_move_dropped += 1;
                        }
                        // 0017: the last record stands; a deletion takes the reference out of
                        // where it is, and a later plain record places it again.
                        if r.deleted {
                            leave(&mut where_is, r.num, &here, &mut evict);
                        }
                        if let Some(target) = r.moved_to {
                            w.stats.refs_moved += 1;
                            room.prov.moved_out += 1;
                            if let Some(old) = room.refs.remove(&r.num) {
                                hist.gone.push(Gone { r: old, by: pi as i32, moved: true });
                            }
                            leave(&mut where_is, r.num, &here, &mut evict);
                            where_is.insert(r.num, Where::Ext(target));
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
                                hist.gone.push(Gone { r: old, by: pi as i32, moved: false });
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
                    laps.to(6);
                    w.stats.refs_relocated += evict_all(&mut w.cells, &mut w.interiors, evict);
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
                        });
                        dst.prov.moved_in += 1;
                        dst.refs.insert(r.num, r);
                    }
                    continue;
                }
                // Round 18gf: a record that deletes the cell forgets its references. The
                // landscape is a LAND record of its own and stays.
                if rc.deleted {
                    w.stats.cells_deleted += 1;
                    if let Some(old) = w.cells.remove(&rc.grid) {
                        for n in old.refs.keys() {
                            where_is.remove(n);
                        }
                    }
                    w.history.exteriors.remove(&rc.grid);
                    continue;
                }
                let cell = w.cells.entry(rc.grid).or_insert_with(|| Cell {
                    name: rc.name.clone(),
                    grid: rc.grid,
                    region: rc.region.clone(),
                    water: Some(0.0),
                    refs: HashMap::new(),
                    prov: CellProv::default(),
                });
                if !rc.name.is_empty() {
                    cell.name = rc.name.clone();
                }
                let hist = w.history.exteriors.entry(rc.grid).or_default();
                hist.touched(pi);
                if !rc.region.is_empty() {
                    cell.region = rc.region.clone();
                }
                w.cell_flags.insert(rc.grid, rc.flags);
                cell.refs.reserve(rc.refs.len());
                let mut moves: Vec<((i32, i32), CellRef)> = Vec::new();
                let here = Where::Ext(rc.grid);
                let mut evict: Vec<(RefNum, Where)> = Vec::new();
                for mut r in rc.refs {
                    // Which plugin the surviving reference came from. Recorded here
                    // because this is the only place that knows both the reference and
                    // where its plugin sits in the order.
                    r.plugin = pi as i32;
                    // Every version of the reference, and its placement in each.
                    n_refs += 1;
                    let sample = n_refs & 63 == 0;
                    let t_hist = sample.then(std::time::Instant::now);
                    RefHistory::supplied(&mut w.history.refs, &r, pi);
                    if let Some(t) = t_hist {
                        ns_history += t.elapsed().as_nanos() * 64;
                    }
                    if drop_mismatched_move(&mut r, opts) {
                        w.stats.refs_move_dropped += 1;
                    }
                    // 0017: see the interior branch - the last record stands.
                    if r.deleted {
                        leave(&mut where_is, r.num, &here, &mut evict);
                    }
                    if let Some(target) = r.moved_to {
                        w.stats.refs_moved += 1;
                        cell.prov.moved_out += 1;
                        if let Some(old) = cell.refs.remove(&r.num) {
                            hist.gone.push(Gone { r: old, by: pi as i32, moved: true });
                        }
                        leave(&mut where_is, r.num, &here, &mut evict);
                        where_is.insert(r.num, Where::Ext(target));
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
                            hist.gone.push(Gone { r: old, by: pi as i32, moved: false });
                        }
                        continue;
                    }
                    let t_ins = sample.then(std::time::Instant::now);
                    let replaced = cell.refs.insert(r.num, r).is_some();
                    if let Some(t) = t_ins {
                        ns_inserts += t.elapsed().as_nanos() * 64;
                    }
                    if replaced {
                        w.stats.refs_overridden += 1;
                        cell.prov.replaced += 1;
                    } else {
                        w.stats.refs_merged += 1;
                    }
                }
                laps.to(6);
                w.stats.refs_relocated += evict_all(&mut w.cells, &mut w.interiors, evict);
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
                    });
                    dst.prov.moved_in += 1;
                    dst.refs.insert(r.num, r);
                }
            }
            laps.to(7);
        }
            laps.to(7);
            ms_merge_loop = t2.elapsed().as_millis();
            let t_wait = std::time::Instant::now();
            let got = match reader {
                Some(h) => match h.join() {
                    Ok((v, ms)) => {
                        ms_geometry_read = ms;
                        pre_list.iter().map(|m| m.to_ascii_lowercase()).zip(v).collect()
                    }
                    Err(_) => HashMap::new(),   // a panic in a read: read again below
                },
                None => HashMap::new(),
            };
            ms_geometry_wait = t_wait.elapsed().as_millis();
            got
        });
        /* Round 18ez: which of the meshes the load order draws have a `.kf` beside them.
           One resolve per distinct mesh, against the overlay's index. */
        {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for m in w.models.values() {
                let k = m.replace('\\', "/").to_ascii_lowercase();
                if !seen.insert(k.clone()) {
                    continue;
                }
                let Some(dot) = m.rfind('.') else { continue };
                if m[dot..].contains(['\\', '/']) {
                    continue;
                }
                let kfp = format!("{}.kf", &m[..dot]);
                if vfs.resolve_mesh(&kfp).or_else(|| vfs.resolve(&kfp)).is_some() {
                    w.kf_meshes.insert(k);
                }
            }
        }
        w.stats.cells = w.cells.len();
        w.stats.lands = w.lands.len();
        w.stats.ms_merge = t2.elapsed().as_millis();
        w.stats.ms_merge_loop = ms_merge_loop;
        w.stats.ms_geometry_wait = ms_geometry_wait;
        w.stats.ms_merge_history = ns_history / 1_000_000;
        w.stats.ms_merge_inserts = ns_inserts / 1_000_000;
        w.stats.ms_merge_parts = laps.ns.map(|n| n / 1_000_000);
        w.stats.ms_defs = defs_ns.map(|n| n / 1_000_000);
        w.stats.defs_counts = defs_n;
        w.stats.ms_twins_ahead = ms_twins_ahead;
        w.stats.twins_ahead = [twin_of.len() as u64, n_kf_ahead];
        // Round 18ax: every leveled list draws as its creatures' one mesh, or the marker.
        crate::records::resolve_leveled(&mut w.objects);

        w.load_geometry(vfs, pre, &pre_anim, &clock, opts.geometry);
        w.stats.mesh_pre = pre_list.len();
        w.stats.ms_geometry_list = ms_geometry_list;
        w.stats.ms_geometry_read = ms_geometry_read;
        w.stats.geometry_clock = clock.totals();
        w
    }

    /// What a reference is, as far as obstacle avoidance is concerned.
    ///
    /// This is the single place the question "should grass keep clear of this, and
    /// why not if not" is answered. Three callers depend on it giving the same answer
    /// every time: the obstacle index, the per-cell census behind the stats panel, and
    /// the right-click verdict. When they each decided for themselves, they disagreed
    /// — which is the whole reason the page's copy of this was deleted.
    ///
    /// Living actors are `LivingActor` on purpose: they walk away from wherever the
    /// plugin put them, so a permanent hole in the grass under a starting spot would
    /// be wrong. A corpse does not move, so it counts.
    pub fn obstacle_ref(&self, id_lc: &str) -> ObstacleRef<'_> {
        if let Some(a) = self.actors.get(id_lc) {
            if !a.corpse() {
                return ObstacleRef::LivingActor;
            }
            // An NPC has no mesh of its own — the game assembles one out of body
            // parts — so it stands in with a body-sized slab.
            if !a.creature || a.model.is_empty() {
                return ObstacleRef::Mesh { model: CORPSE_SLAB, corpse: Corpse::Npc };
            }
            return ObstacleRef::Mesh { model: a.model.as_str(), corpse: Corpse::Creature };
        }
        let Some(model) = self.models.get(id_lc) else {
            return ObstacleRef::Unknown;
        };
        // An editor marker is invisible in game — a travel destination, a door target,
        // the north arrow — and there is one at almost every town gate. Blocking grass
        // around them left bare rings the player walks through. The page has always
        // filtered these by path; the engine had not, so the two disagreed wherever a
        // marker stood.
        if is_marker_mesh(model) {
            return ObstacleRef::Marker;
        }
        ObstacleRef::Mesh { model: model.as_str(), corpse: Corpse::No }
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

    /// The mesh to test against, or None when this is not something grass dodges.
    /// A thin reading of [`World::obstacle_ref`] for callers that only need the name —
    /// `load_geometry` deciding what to read off disk, mostly.
    pub fn obstacle_mesh(&self, id_lc: &str) -> Option<&str> {
        match self.obstacle_ref(id_lc) {
            ObstacleRef::Mesh { model, .. } => Some(model),
            _ => None,
        }
    }

    /// [`obstacle_mesh`](Self::obstacle_mesh) for an id of any case, without allocating.
    ///
    /// Round 18bw. Every caller had the same line — `obstacle_mesh(&r.id.to_ascii_lowercase())`
    /// — and the statics pass ran it for **every reference in every cell** to reach an early
    /// return: of the order of 2.8 million `String`s an export, on sixteen threads against
    /// one heap, of which about one in a hundred was used for anything.
    ///
    /// A TES3 record id is at most 32 bytes in the file format, so the lowercase copy goes
    /// on the stack and anything longer (a malformed plugin, or a future format) falls back
    /// to the allocating path rather than being truncated into the wrong answer.
    pub fn obstacle_mesh_of(&self, id: &str) -> Option<&str> {
        let b = id.as_bytes();
        if b.len() <= 64 && b.is_ascii() {
            let mut buf = [0u8; 64];
            for (i, &c) in b.iter().enumerate() {
                buf[i] = c.to_ascii_lowercase();
            }
            // ASCII lowercased byte by byte is still valid UTF-8 of the same length.
            let lc = std::str::from_utf8(&buf[..b.len()]).unwrap_or("");
            return self.obstacle_mesh(lc);
        }
        self.obstacle_mesh(&id.to_ascii_lowercase())
    }

    /// Why a mesh this world wanted is not in `geom`: 1 not found in the overlay,
    /// 2 found but unreadable. Absent means it loaded.
    pub fn mesh_failure(&self, model_lc: &str) -> Option<u8> {
        self.mesh_failed.get(model_lc).copied()
    }

    /// Load collision geometry for every mesh actually referenced by a cell.
    ///
    /// `pre` (round 18cr) is what was read beside the merge, by lowercased mesh path -
    /// taken where it answers for a wanted mesh, the rest read here as before.
    fn load_geometry(
        &mut self,
        vfs: &Vfs,
        mut pre: HashMap<String, GeomRead>,
        pre_anim: &std::collections::HashSet<String>,
        clock: &GeomClock,
        read: bool,
    ) {
        /* Round 18ey: the pre-read decided whether to bind each mesh's `.kf` from the
           *superset* of definitions, because it runs beside the merge and has to. Where
           the merge then disagreed - a mesh some losing plugin defined as an activator and
           the winning one defines as a static, or the other way about - the pre-read's
           answer is thrown away here and the mesh read again with the right one. A pose
           cannot be undone after the fact, and one wrong hull is one rock the grass grows
           through. Nothing in a normal load order hits this; it costs a set lookup a mesh
           when it does not. */
        pre.retain(|k, _| {
            let key = k.replace('\\', "/");
            pre_anim.contains(&key) == self.anim_meshes.contains(&key)
        });
        let t = std::time::Instant::now();
        let mut used: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut want_slab = false;
        let mut corpse_refs = 0usize;
        /* Round 18gx: the rooms' references too. Only the exteriors' were walked, so a
           mesh placed indoors and nowhere outside had no geometry, and the room scatter
           (which never built an obstacle index until this round either) grew grass through
           every bench and crate standing on a painted floor. Robin, in a monastery built
           from exterior pieces: "It doesn't seem that grass growing on meshes in interiors
           are actually avoiding objects as they should." The pre-read already reads these
           meshes, since it lists every id any cell references; this keeps what it read. */
        let all_refs = self.cells.values().map(|c| &c.refs).chain(self.interiors.values().map(|i| &i.refs));
        for refs in all_refs {
            for r in refs.values() {
                let key = r.id.to_ascii_lowercase();
                if self.actors.contains_key(&key) {
                    // Counted here rather than at parse time: what matters is how many
                    // corpses are actually placed, not how many are defined.
                    if self.obstacle_mesh(&key).is_some() {
                        corpse_refs += 1;
                    }
                }
                if let Some(m) = self.obstacle_mesh(&key) {
                    if m == CORPSE_SLAB {
                        want_slab = true;
                    } else if seen.insert(m.to_ascii_lowercase()) {
                        used.push(m.to_string());
                    }
                }
            }
        }
        self.stats.actors = self.actors.len();
        self.stats.corpses = self.actors.values().filter(|a| a.corpse()).count();
        self.stats.corpse_refs = corpse_refs;
        if !read {
            // Wraithguard (`LoadOpts::geometry` off): the counts, and no mesh read.
            self.stats.mesh_wanted = used.len() + usize::from(want_slab);
            self.stats.ms_meshes = t.elapsed().as_millis();
            return;
        }
        if want_slab {
            self.geom.insert(CORPSE_SLAB.to_string(), corpse_slab());
        }
        // The slab is geometry without a file, so it counts as wanted-and-found rather
        // than being left to make the two totals disagree.
        self.stats.mesh_wanted = used.len() + usize::from(want_slab);
        // 0 = parsed, 1 = not found in the overlay, 2 = found but unreadable
        /* What the merge-time read did not cover is read now, in one map; then the two
           are put in `used`'s order, which is the order the failure lines have always
           been in. */
        let todo: Vec<String> = used.iter().filter(|m| !pre.contains_key(&m.to_ascii_lowercase())).cloned().collect();
        self.stats.mesh_late = todo.len();
        let now: Vec<GeomRead> = pool::par_map(&todo, |_, mesh| {
            read_geometry(vfs, mesh, self.anim_meshes.contains(&mesh.replace('\\', "/").to_ascii_lowercase()), clock)
        });
        for (m, r) in todo.iter().zip(now) {
            pre.insert(m.to_ascii_lowercase(), r);
        }
        let loaded: Vec<(u8, Option<(String, nif::MeshGeom)>)> = used
            .iter()
            .map(|m| match pre.remove(&m.to_ascii_lowercase()) {
                Some((why, Some(g))) => (why, Some((m.to_ascii_lowercase(), g))),
                Some((why, None)) => (why, None),
                None => (1, None),
            })
            .collect();
        for (i, (why, item)) in loaded.into_iter().enumerate() {
            match why {
                1 => {
                    self.stats.mesh_unresolved += 1;
                    self.mesh_failed.insert(used[i].to_ascii_lowercase(), 1);
                    self.mesh_failures.push(format!("not in the overlay: {}", used[i]));
                }
                2 => {
                    self.stats.mesh_unparsed += 1;
                    self.mesh_failed.insert(used[i].to_ascii_lowercase(), 2);
                    self.mesh_failures.push(format!("unreadable: {}", used[i]));
                }
                _ => {}
            }
            if let Some((k, g)) = item {
                if g.shape == nif::ColShape::Hull {
                    self.stats.mesh_collision += 1;
                }
                self.geom.insert(k, g);
            }
        }
        self.stats.meshes = self.geom.len();
        self.stats.ms_meshes = t.elapsed().as_millis();
    }
}

/// One mesh's collision read: 0 parsed, 1 not in the overlay, 2 unreadable — and the
/// geometry when it parsed. What `load_geometry` has always computed per mesh, as a
/// function so it can run beside the merge (round 18cr).
type GeomRead = (u8, Option<nif::MeshGeom>);

fn read_geometry(vfs: &Vfs, mesh: &str, animates: bool, clock: &GeomClock) -> GeomRead {
    let t = std::time::Instant::now();
    let Some(p) = vfs.resolve_mesh(mesh) else { return (1u8, None) };
    let Some(buf) = vfs.load(&p) else { return (1u8, None) };
    clock.add(&clock.ns_load, t);
    clock.bytes.fetch_add(buf.len() as u64, std::sync::atomic::Ordering::Relaxed);
    /* Round 18be (G7): with the `.kf` beside it, so the hull is built from the
       pose the viewport draws. See `nif::parse_with_kf`.
       Round 18ey: **and only when the game would ever open that `.kf`** - when some
       animated-class record or some actor names this mesh (`World::anim_meshes`). A STAT
       pointing straight at an `x` file with a `.kf` beside it is drawn from the mesh's own
       rest pose by the game, because a static never gets an animation object at all; the
       hull has to be built from the same pose the viewport draws, so the two answers move
       together or not at all. */
    let _ = animates;
    /* Round 18ey: **and the same `animates` question as the payload's, deliberately not
       acted on here either.** The hull and the picture have to be built from one pose
       (18be, G7), so the two move together or not at all - and the picture's half is per
       reference rather than per mesh. See the note in `mesh_payload`. */
    let t = std::time::Instant::now();
    let kf = kf_beside(vfs, mesh);
    clock.add(&clock.ns_kf, t);
    let t = std::time::Instant::now();
    let out = match nif::parse_with_kf(&buf, kf.as_ref()) {
        Some(g) => (0, Some(g)),
        None => (2, None),
    };
    clock.add(&clock.ns_parse, t);
    out
}

/// Wraithguard: where the geometry reads' time goes, summed over the pool's threads (so
/// it can exceed the wall time): reading each mesh file, finding and reading the `.kf`
/// beside it, and parsing; and the bytes read.
#[derive(Default)]
pub struct GeomClock {
    ns_load: std::sync::atomic::AtomicU64,
    ns_kf: std::sync::atomic::AtomicU64,
    ns_parse: std::sync::atomic::AtomicU64,
    bytes: std::sync::atomic::AtomicU64,
}

impl GeomClock {
    fn add(&self, to: &std::sync::atomic::AtomicU64, since: std::time::Instant) {
        to.fetch_add(since.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    }
    /// Milliseconds reading, at the `.kf`, parsing; then the bytes read.
    fn totals(&self) -> [u64; 4] {
        let g = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
        [g(&self.ns_load) / 1_000_000, g(&self.ns_kf) / 1_000_000, g(&self.ns_parse) / 1_000_000, g(&self.bytes)]
    }
}

/// Every mesh the merged world could want a hull for, from the parsed plugins alone
/// (round 18cr): the ids any cell references, and every mesh any definition of one of
/// those ids names - with the `x` twin beside it when the class is one the merge might
/// substitute. A superset of what `load_geometry` will ask for, never a subset: an id's
/// final definition is one of its definitions, and its final mesh is that definition's
/// mesh or its twin. Actors count as their own model (a corpse's file; a living one is
/// no obstacle and its file is read for nothing, which is the price of not waiting).
/// Deduplicated by lowercase path, in first-seen order.
fn pre_geometry(parsed: &[PluginData], vfs: &Vfs) -> (Vec<String>, std::collections::HashSet<String>) {
    // The ids referenced anywhere, gathered per plugin on the pool and joined.
    let per: Vec<std::collections::HashSet<String>> = pool::par_map(parsed, |_, d| {
        let mut ids = std::collections::HashSet::new();
        for c in &d.cells {
            for r in &c.refs {
                ids.insert(r.id.to_ascii_lowercase());
            }
        }
        ids
    });
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in per {
        ids.extend(s);
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut want = |m: &str| {
        if m.is_empty() || m == CORPSE_SLAB || is_marker_mesh(m) {
            return;
        }
        if seen.insert(m.to_ascii_lowercase()) {
            out.push(m.to_string());
        }
    };
    /* Round 18ey: and, beside the list, which of those meshes the game would ever
       animate. The merge fills `World::anim_meshes` from the definitions that *win*; this
       is the same question asked of the superset, because the pre-read runs before the
       merge and a mesh read with its `.kf` bound cannot be un-posed afterwards. A superset
       here means the pre-read may pose a mesh the winning definition would have left
       still - so `load_geometry` re-reads exactly those, below, rather than keeping the
       pre-read's answer. */
    let mut anim: std::collections::HashSet<String> = std::collections::HashSet::new();
    for d in parsed {
        for md in &d.models {
            if md.deleted || md.mesh.is_empty() || !ids.contains(&md.id.to_ascii_lowercase()) {
                continue;
            }
            want(&md.mesh);
            if md.animated_class {
                anim.insert(md.mesh.replace('\\', "/").to_ascii_lowercase());
                if let Some(t) = x_twin(vfs, &md.mesh) {
                    anim.insert(t.replace('\\', "/").to_ascii_lowercase());
                    want(&t);
                }
            }
        }
        for a in &d.actors {
            if a.deleted || !a.creature || a.model.is_empty() || !ids.contains(&a.id.to_ascii_lowercase()) {
                continue;
            }
            anim.insert(a.model.replace('\\', "/").to_ascii_lowercase());
            if let Some(t) = x_twin(vfs, &a.model) {
                anim.insert(t.replace('\\', "/").to_ascii_lowercase());
            }
            want(&a.model);
        }
    }
    (out, anim)
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

// The plugin-name test lives in `crate::paths` (18ge), re-exported so
// `world::is_plugin_name` and `world::PLUGIN_EXTS` still resolve.
pub use crate::paths::{is_plugin, is_plugin_name, PLUGIN_EXTS};

/// Reads a load order from an OpenMW `openmw.cfg`, an MO2 profile `Morrowind.ini`,
/// or a plain `Morrowind.ini` in the game folder.
///
/// Only entries that name a file with records in them count — see `PLUGIN_EXTS`. An
/// `openmw.cfg` is read as its whole chain (round 18gn, `openmwcfg`), so `content=` lines
/// in a `config=` folder count too.
pub fn read_load_order(ini: &Path) -> Vec<String> {
    let text = crate::openmwcfg::order_text(ini);
    if text.is_empty() {
        return Vec::new();
    }
    let mut indexed: Vec<(usize, String)> = Vec::new();
    let mut content: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("content=") {
            let name = rest.trim();
            if is_plugin(name) {
                content.push(name.to_string());
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
        return content;
    }
    indexed.sort_by_key(|(n, _)| *n);
    indexed.into_iter().map(|(_, s)| s).collect()
}

/// The kinds a placed object can be: OpenMW's id index holds these and no others.
const PLACEABLE: [&Tag; 21] = [
    b"ACTI", b"ALCH", b"APPA", b"ARMO", b"BODY", b"BOOK", b"CLOT", b"CONT", b"CREA", b"DOOR", b"INGR", b"LEVC", b"LEVI",
    b"LIGH", b"LOCK", b"MISC", b"NPC_", b"PROB", b"REPA", b"STAT", b"WEAP",
];

fn is_placeable(tag: &Tag) -> bool {
    PLACEABLE.contains(&tag)
}

/// For every id the load order defines as more than one placeable kind, the kind a placed
/// object of it is - as OpenMW resolves it, not by load order. OpenMW keeps each kind's
/// records apart (both versions exist), and after loading builds one id -> kind index by
/// walking its stores in the order of their record-name number (the tag read as a
/// little-endian `u32`), the later store overwriting: so the kind whose tag is greatest
/// that way wins - in effect the last letter (`WEAP` over `MISC`, `NPC_` over all). A
/// deletion takes that kind's record out first. The Construction Set refuses such a load
/// order ("Matching IDs") and Morrowind.exe will not load it; OpenMW does, and places this.
pub fn placed_types(parsed: &[PluginData]) -> HashMap<String, Tag> {
    let mut live: std::collections::HashSet<(Tag, String)> = std::collections::HashSet::new();
    let mut ever: HashMap<String, std::collections::HashSet<Tag>> = HashMap::new();
    for d in parsed {
        for (id, tag) in &d.ids {
            if is_placeable(tag) {
                live.insert((*tag, id.clone()));
                ever.entry(id.clone()).or_default().insert(*tag);
            }
        }
        for (id, tag) in &d.ids_deleted {
            live.remove(&(*tag, id.clone()));
        }
    }
    // Only ids some plugin made more than one kind; for each, the live kind that ranks
    // highest. An id with no live kind left is not in the map (its deletions apply).
    ever.into_iter()
        .filter(|(_, kinds)| kinds.len() > 1)
        .filter_map(|(id, kinds)| {
            kinds
                .into_iter()
                .filter(|t| live.contains(&(*t, id.clone())))
                .max_by_key(|t| u32::from_le_bytes(*t))
                .map(|t| (id, t))
        })
        .collect()
}
