//! The engine's views of a TES3 (.esm/.esp) plugin.
//!
//! Round 18gh (backend-plan part 3, second half). This used to be the byte reader: a
//! `Records` walk over the file, a `Subs` walk over each body, and the cell and reference
//! parsers on top. A plugin's bytes are read in one place now, [`crate::tes3::raw`], with
//! the layouts in [`crate::tes3::types`]; what stays here is what the rest of the engine
//! and the page see of a plugin ([`CellRef`], [`RefNum`], [`CellHeader`], [`Ambi`],
//! [`PluginHead`]) and how each is filled from a typed value. `world::parse_plugin` does
//! the filling for a whole plugin.
//!
//! The little-endian helpers and the Windows-1252 string helpers stay for the writers and
//! for `nif.rs` until its own half of the round moves it onto `netimmerse`.

use crate::tes3::bytes::Reader;
use crate::tes3::types::Typed;
use crate::tes3::{raw, types};

pub type Tag = [u8; 4];

#[inline]
pub const fn tag(s: &[u8; 4]) -> Tag {
    *s
}

pub const TES3: Tag = *b"TES3";
pub const CELL: Tag = *b"CELL";
pub const LAND: Tag = *b"LAND";
pub const LTEX: Tag = *b"LTEX";
pub const REGN: Tag = *b"REGN";
pub const STAT: Tag = *b"STAT";

/// Record header flag: the object is persistent.
pub const REC_PERSISTENT: u32 = 0x0400;

#[inline]
pub fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
pub fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
#[inline]
pub fn i32le(b: &[u8], o: usize) -> i32 {
    u32le(b, o) as i32
}
#[inline]
pub fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32le(b, o))
}

/// A zero-terminated, possibly padded, latin-1 string.
pub fn cstr(b: &[u8]) -> &str {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    // Morrowind strings are latin-1; the subset we care about (ids, filenames)
    // is ASCII in practice. Fall back to a lossy borrow rather than allocating.
    std::str::from_utf8(&b[..end]).unwrap_or("")
}

/// Bytes as text up to the first NUL, Windows-1252. Round 18gh: everything the engine
/// reads from a plugin or an archive comes through the typed model's decoder now, and
/// this is the same decoder under the old name, for the tests that still call it.
pub fn cstring(b: &[u8]) -> String {
    crate::bytes::decode(b)
}

/// A Rust string back to the bytes a TES3 record holds.
///
/// Round 18bc (A3). The reader decodes a record's bytes to chars — byte `0xE9` becomes
/// `é` — so the writer has to encode the same way or the two disagree. It used to write
/// `s.as_bytes()`, i.e. UTF-8, which turned `é` into two bytes. For an exterior that is
/// cosmetic, because the game keys on the grid; **for an interior the NAME is the room's
/// identity**, so on a French, German or Russian install the plugin declared a brand-new
/// room with a mangled name, put the grass in that phantom room, and left the real one
/// bare. `esppatch::cell_key` compares the same two spellings, so a patch appended a
/// duplicate CELL instead of extending the source's.
///
/// Round 18gh: the reader is the typed model, which decodes Windows-1252, so this encodes
/// Windows-1252 (`tes3::bytes::char_to_byte`): one byte per character, the same byte the
/// model read. The name is the one the writers and tests have always called; latin-1 and
/// Windows-1252 are the same page outside 0x80 to 0x9F. A character the page cannot hold
/// can only come from text the person typed (an author line, a description, an id prefix),
/// never from a record this program read, and it becomes `?`, which is what every tool in
/// the chain does with it.
pub fn latin1_into(s: &str, out: &mut Vec<u8>) {
    if s.is_ascii() {
        out.extend_from_slice(s.as_bytes());
        return;
    }
    for c in s.chars() {
        out.push(crate::bytes::char_to_byte(c).unwrap_or(b'?'));
    }
}

pub fn latin1(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len());
    latin1_into(s, &mut v);
    v
}

/// How many bytes [`latin1_into`] will write, without writing them. One byte per
/// character; `s.len()` only for ASCII, where the two agree.
#[inline]
pub fn latin1_len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().count()
    }
}

// ---------------------------------------------------------------------------
// Reference numbers
// ---------------------------------------------------------------------------

/// A reference's identity, as the engine sees it.
///
/// The top byte of FRMR is a 1-based index into the *writing plugin's* master
/// list; zero means the reference is new in that plugin. Two references match —
/// and therefore override — only when both the originating file and the index
/// agree. Keying on the raw FRMR instead is what makes edited objects appear
/// twice and unrelated new objects clobber each other.
///
/// A negative `content_file` means "no file in this load order" — a master the plugin
/// names that is not installed. Round 18bf (I4): the *which* matters, not only the fact,
/// so each distinct missing master gets its own negative id from a table built over the
/// whole order (see `World::load`). Two plugins overriding references out of two
/// different uninstalled masters used to both land on `-1` and erase each other.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct RefNum {
    pub content_file: i32,
    pub index: u32,
}

impl RefNum {
    #[inline]
    pub fn resolve(raw: u32, plugin_idx: i32, masters: &[i32]) -> RefNum {
        let local = (raw >> 24) & 0xff;
        if local == 0 {
            RefNum { content_file: plugin_idx, index: raw }
        } else {
            let idx = local as usize - 1;
            /* A slot past the end of the plugin's own master list: a malformed file, and
               a different failure from a master that is simply not installed. Kept well
               clear of the missing-master ids (which are -1, -2, … over the load order)
               so the two cannot be confused for each other — round 18bf (I4). */
            let cf = masters.get(idx).copied().unwrap_or(i32::MIN + local as i32);
            RefNum { content_file: cf, index: raw & 0x00ff_ffff }
        }
    }
}

/// One placed object inside a cell.
#[derive(Clone, Debug)]
pub struct CellRef {
    pub num: RefNum,
    pub id: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: f32,
    pub deleted: bool,
    /// Set when the reference carries MVRF: it has been moved to `moved_to`.
    pub moved_to: Option<(i32, i32)>,
    /// Round 18gf: the MVRF that moved it named a different reference than the FRMR that
    /// followed. The Construction Set never writes that; a malformed plugin can. OpenMW
    /// applies the move anyway (positional), `tes3` drops it (the move, not the
    /// reference). Which one the world follows is the install's engine (`LoadOpts`).
    pub moved_mismatch: bool,
    /// Round 18gf: whether the reference sits after the cell's `NAM0`, which is what the
    /// format means by temporary. Everything before NAM0 is persistent; a door with a
    /// destination and a moved reference are persistent whatever their position
    /// (`tes3` reference.rs:226-243). The viewer does not care; the patcher and the
    /// Objects plugin do, because a reader counts NAM0's run as temporary.
    pub temporary: bool,
    /// Which plugin in the load order last supplied this reference. Filled in by the
    /// merge, not by the parser — a plugin reading its own file has no idea where it
    /// sits in anybody's order — and left at -1 until then.
    ///
    /// The viewport lists a cell's objects by plugin, which is how you find out that
    /// the rock you are looking at came from a mod rather than from Morrowind.esm.
    pub plugin: i32,
    /// Round 17y: where a door leads, when it teleports — `DODT`'s position and rotation
    /// and, for a door into an interior, `DNAM`'s cell name (empty for a door into the
    /// exterior, whose cell is wherever the position falls). Robin: a button in the object
    /// dialogue that "loads the cell that is connected to the teleporter".
    pub door: Option<DoorDest>,
}

/// A door's destination (round 17y).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DoorDest {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub cell: String,
}

impl Default for CellRef {
    fn default() -> Self {
        CellRef {
            num: RefNum { content_file: -1, index: 0 },
            id: String::new(),
            pos: [0.0; 3],
            rot: [0.0; 3],
            scale: 1.0,
            deleted: false,
            moved_to: None,
            moved_mismatch: false,
            temporary: false,
            plugin: -1,
            door: None,
        }
    }
}

impl CellRef {
    /// Round 18gh: the view of one typed reference, for a plugin at `plugin_idx` in the
    /// load order whose masters sit at `masters`.
    ///
    /// The model keeps `MVRF`'s destination even when its word named another reference
    /// than the `FRMR` that followed (`Reference::moved_index`), so `moved_to` is set
    /// either way and `moved_mismatch` says which, as the byte reader always did; the
    /// merge applies the install's rule (`LoadOpts::mvrf_strict`). The mismatch compares
    /// the reference index alone, since the master byte is unreliable in dirty plugins.
    /// A reference without `XSCL` has scale 1.0, and `plugin` is the merge's to fill.
    pub fn from_typed(r: types::Reference, plugin_idx: i32, masters: &[i32]) -> CellRef {
        let word = (r.mast_index << 24) | (r.refr_index & 0x00ff_ffff);
        CellRef {
            num: RefNum::resolve(word, plugin_idx, masters),
            id: r.id,
            pos: r.translation,
            rot: r.rotation,
            scale: r.scale.unwrap_or(1.0),
            deleted: r.deleted.unwrap_or(false),
            moved_to: r.moved_cell,
            moved_mismatch: r.moved_index.is_some_and(|(_, refr)| refr != r.refr_index),
            temporary: r.temporary,
            plugin: -1,
            door: r.destination.map(|d| DoorDest { pos: d.translation, rot: d.rotation, cell: d.cell }),
        }
    }
}

/// The cell's own fields, as the engine reads them off a typed cell.
///
/// Round 18gh: this used to be read by stopping at the first FRMR, because a CELL body
/// repeats NAME and DATA for every reference it contains; the typed `Cell` keeps the two
/// apart itself. Two things the typed form does not say are read from the raw record's
/// own block (the subrecords before the first `FRMR` or `MVRF`): whether a `DATA` was
/// there at all, and which spelling the water level came in.
pub struct CellHeader {
    pub name: String,
    pub flags: u32,
    pub grid: (i32, i32),
    pub region: String,
    pub has_data: bool,
    /// Water level, for an interior that has any. Exteriors have their sea at zero
    /// and never carry either spelling.
    ///
    /// Two spellings, one meaning. Morrowind.esm writes the level as an `INTV`
    /// integer — every one of its 1,134 interiors carries one and not a single
    /// `WHGT` — while the Tribunal-era Construction Set writes `WHGT` as a float
    /// and no `INTV`. Reading only `WHGT` put every vanilla room's water at zero:
    /// Addamasartus is `INTV -760`, so its pool became a flood to the ceiling.
    pub water: Option<f32>,
    /// Round 17y: an interior's own light — the `AMBI` subrecord: ambient colour, the
    /// colour of its "sunlight", fog colour and fog density. A room has no sky, so this
    /// is all the light it has; exteriors carry none.
    pub ambi: Option<Ambi>,
    /// Round 18gf: the record deletes the cell, by a `DELE` of its own (before the
    /// references, as `tes3` writes it, or after one's `DATA`) or by the record header's
    /// deleted bit (0x20).
    pub deleted: bool,
}

/// A cell's `AMBI` (round 17y). Colours are bytes as the record wrote them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ambi {
    pub ambient: [u8; 3],
    pub sunlight: [u8; 3],
    pub fog: [u8; 3],
    pub fog_density: f32,
}

/// Whether a cell's flags say "behave like exterior" (0x80): an interior with the sky's
/// weather, sun and hours — Vivec's cantons, Mournhold's plaza.
#[inline]
pub fn is_quasi_exterior(flags: u32) -> bool {
    flags & 0x80 != 0
}

/// The cell's own block of a raw record: every subrecord before the first `FRMR` or
/// `MVRF`, which is where the byte reader stopped.
fn cell_block<'a, 'b>(rec: &'a raw::Record<'b>) -> &'a [raw::Sub<'b>] {
    let end = rec.subs.iter().position(|s| matches!(&s.tag, b"FRMR" | b"MVRF")).unwrap_or(rec.subs.len());
    rec.subs.get(..end).unwrap_or(&[])
}

impl CellHeader {
    /// Round 18gh: the view of a typed cell, with `rec` the raw record it was read from.
    pub fn of(cell: &types::Cell, rec: &raw::Record) -> CellHeader {
        let block = cell_block(rec);
        let flags = cell.data.flags;
        /* The water, as it was always read: `WHGT` wins where a file somehow has both,
        being the newer spelling and the one exports write, and `INTV` counts only for
        an interior, because an exterior carrying one must not be read as a room with
        water. The typed `water_height` is whichever spelling came last, so it is the
        answer whenever `WHGT` is that one; the `WHGT` a later `INTV` overwrote is read
        back from its own four bytes, which the model has already checked. */
        let mut last_whgt: Option<&raw::Sub> = None;
        let mut whgt_last = false;
        let mut intv = false;
        for s in block {
            match &s.tag {
                b"WHGT" => {
                    last_whgt = Some(s);
                    whgt_last = true;
                }
                b"INTV" => {
                    intv = true;
                    whgt_last = false;
                }
                _ => {}
            }
        }
        let water = match last_whgt {
            Some(_) if whgt_last => cell.water_height,
            Some(s) => Reader::new(&s.data).f32().ok(),
            None if intv && is_interior(flags) => cell.water_height,
            None => None,
        };
        CellHeader {
            name: cell.name.clone(),
            flags,
            grid: cell.data.grid,
            region: cell.region.clone().unwrap_or_default(),
            has_data: block.iter().any(|s| &s.tag == b"DATA"),
            water,
            ambi: cell.atmosphere_data.as_ref().map(|a| Ambi {
                ambient: [a.ambient_color[0], a.ambient_color[1], a.ambient_color[2]],
                sunlight: [a.sunlight_color[0], a.sunlight_color[1], a.sunlight_color[2]],
                fog: [a.fog_color[0], a.fog_color[1], a.fog_color[2]],
                fog_density: a.fog_density,
            }),
            deleted: cell.flags & raw::FLAG_DELETED != 0,
        }
    }
}

/// Whether a raw CELL record carries a `DELE` of the cell's own: one outside any
/// reference's run, which ends at its `DATA` or its `DELE` (round 18gf). Only asked when
/// the header's deleted bit is set, to tell "the bit alone" from "the bit and the
/// subrecord" for the log; the typed cell's flags already say whether it is deleted.
pub fn cell_has_dele(rec: &raw::Record) -> bool {
    let mut in_ref = false;
    for s in &rec.subs {
        match &s.tag {
            b"FRMR" => in_ref = true,
            b"DATA" if in_ref => in_ref = false,
            b"DELE" if in_ref => in_ref = false,
            b"DELE" => return true,
            _ => {}
        }
    }
    false
}

#[inline]
pub fn is_interior(flags: u32) -> bool {
    flags & 0x01 != 0
}

/// What a plugin's own header says about itself, read without parsing the plugin.
///
/// Round 18bi. The plugin chooser has to show the whole load order — 562 plugins on
/// Robin's own install — and say which of them are masters of which, *before* anything is
/// parsed. `World::load_with` answers the same question, but only after reading every
/// plugin whole, which for Tamriel Rebuilt and Cyrodiil together is hundreds of megabytes
/// and the very wait the chooser exists to let somebody skip.
///
/// So this reads the first record and stops. A TES3 header is the first record in the
/// file by definition, its length is in its own 16-byte header, and a plugin with twenty
/// masters still fits in a couple of kilobytes.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PluginHead {
    /// The masters this plugin names, in the order it names them, lower-cased to match
    /// the way every other part of the engine compares plugin names.
    pub masters: Vec<String>,
    /// What the author wrote in the header, when there is anything there.
    pub desc: String,
}

/// Reads one plugin's header out of bytes that begin at the start of the file.
///
/// `None` when the bytes are not a plugin at all — too short, a first record that is not
/// `TES3`, or one the typed `Header` refuses. A header that is present but empty is
/// `Some` with nothing in it: a plugin with no masters is an ordinary thing and is not a
/// failure to read.
///
/// Round 18gh: the first record alone, through `raw::Record::read` and `types::Header`,
/// so a 200 MB plugin still costs its first kilobyte.
pub fn plugin_head(buf: &[u8]) -> Option<PluginHead> {
    if buf.len() < 16 || buf[0..4] != TES3 {
        return None;
    }
    let rec = raw::Record::read(&mut Reader::new(buf)).ok()?;
    let h = types::Header::from_raw(&rec).ok()?;
    Some(PluginHead {
        masters: h.masters.iter().map(|(name, _)| name.to_ascii_lowercase()).collect(),
        desc: h.description.trim().to_string(),
    })
}

/// How many bytes of a plugin `plugin_head` needs, read from the first 16 of them.
///
/// A caller with a file rather than a buffer reads 16 bytes, asks this, and then reads
/// exactly that much — two small reads instead of loading a 200 MB plugin to look at its
/// first kilobyte.
pub fn head_len(first16: &[u8]) -> Option<usize> {
    if first16.len() < 16 || first16[0..4] != TES3 {
        return None;
    }
    let mut r = Reader::new(first16);
    r.tag().ok()?;
    Some(16 + r.u32().ok()? as usize)
}
