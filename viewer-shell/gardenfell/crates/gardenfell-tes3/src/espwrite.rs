//! TES3 plugin writer.
//!
//! The output defines its own STAT records and only adds references by grid, so it
//! needs no masters — it is fed to Distant Land or the OpenMW grass generator, never
//! loaded in-game, and a masterless plugin cannot be broken by a load-order change.
//!
//! Masters can be declared anyway, because some tools in the chain expect them:
//! Wrye Mash flags a masterless plugin, and a few distant-land workflows sort by
//! master list. It stays the caller's choice and the default is none.

use crate::esp::CellRef;
use crate::tes3::bytes::Writer as TesWriter;
use crate::tes3::raw::Record;

/// One past the highest reference number a plugin may give a reference of its **own**.
///
/// Round 18bc (A7). `FRMR`'s top byte is a **1-based index into the writing plugin's
/// master list**, so a reference the plugin created has a top byte of zero and the
/// numbers available to it are 1..=0x00FF_FFFF. Nothing used to check: past that the top
/// byte became 1 and the game read every later reference as an override of an object in
/// the plugin's first master — nothing at all, for a masterless grass plugin. No error,
/// and a file that looks fine while quietly losing its tail.
///
/// So a run that needs more numbers than this is written as **several plugins**. Robin:
/// "If we ever reach that limit on an exported .esp (somewhat likely, if someone wants to
/// make a huge landmass, which already exists for placeholder for the whole of Tamriel),
/// lets fill one .esp file up first, and then continue on another one, adding \"_2\" before
/// the .esp in the name of the second one." Each file numbers from 1 again, so the ceiling
/// applies per file and the cut falls on a cell boundary — a cell's references are one
/// consecutive run and splitting one across two files would be a cell in two plugins.
pub const FRMR_LIMIT: u32 = 0x0100_0000;

#[derive(Default)]
pub struct Writer {
    pub out: Vec<u8>,
    /// Round 18gf (backend-plan item 7): clamp every reference's scale to 0.5..2.0 on
    /// the way out, as the game does on the way in and `tes3` does on save
    /// (reference.rs:128). On for a plugin the game loads - the Objects plugin - and off
    /// for the groundcover plugin, whose scale MGE XE and OpenMW's groundcover store read
    /// as written and which a grass author tunes below 0.5 on purpose (`fSclMin 0.4`).
    /// Never clamped on read: what a plugin says is what the preview shows.
    pub clamp_scale: bool,
}

/// Round 18gh: each record is built as a `tes3::raw::Record` and written with
/// `Record::write`, the same layer that reads a plugin, so the writer and the reader
/// cannot disagree about where a size word goes. The bytes are the ones this wrote before
/// (the fixture digest says so): the subrecords in the same order with the same contents,
/// the header's second dword zero, the flags zero.
impl Writer {
    pub fn with_capacity(n: usize) -> Writer {
        Writer { out: Vec::with_capacity(n), clamp_scale: false }
    }
    /// [`with_capacity`](Self::with_capacity) for a plugin the game itself loads.
    pub fn for_game(n: usize) -> Writer {
        Writer { out: Vec::with_capacity(n), clamp_scale: true }
    }
    /// A zero-terminated string subrecord's bytes.
    ///
    /// Round 18bc (A3): one byte per character, the page [`crate::esp::cstring`]'s family
    /// reads back (round 18gh: Windows-1252, as the typed model decodes).
    fn zstr(s: &str) -> Vec<u8> {
        let mut v = Vec::with_capacity(crate::esp::latin1_len(s) + 1);
        crate::esp::latin1_into(s, &mut v);
        v.push(0);
        v
    }
    fn fixed(s: &str, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        for (slot, b) in v.iter_mut().zip(crate::esp::latin1(s).into_iter().take(n - 1)) {
            *slot = b;
        }
        v
    }
    /// Appends a finished record to the output.
    fn emit(&mut self, rec: &Record) {
        let mut w = TesWriter { out: std::mem::take(&mut self.out) };
        rec.write(&mut w);
        self.out = w.into_bytes();
    }

    /// Returns the offset of the record count inside the finished buffer, so a caller
    /// that does not know the count yet can patch it once everything is written.
    pub fn header(&mut self, num_records: u32, author: &str, desc: &str) -> usize {
        self.header_with_masters(num_records, author, desc, &[])
    }

    /// `masters` is `(file name, size in bytes)`, in load order. The size is what the
    /// game checks the master against; a wrong one makes the plugin look corrupt to
    /// some tools, so a caller that cannot stat the file should not be declaring it.
    pub fn header_with_masters(
        &mut self,
        num_records: u32,
        author: &str,
        desc: &str,
        masters: &[(String, u64)],
    ) -> usize {
        let at = self.out.len();
        let mut rec = Record::new(b"TES3");
        let mut hedr = Vec::with_capacity(300);
        hedr.extend_from_slice(&1.3f32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes()); // file type: plugin
        hedr.extend_from_slice(&Self::fixed(author, 32));
        hedr.extend_from_slice(&Self::fixed(desc, 256));
        hedr.extend_from_slice(&num_records.to_le_bytes());
        rec.push(b"HEDR", hedr);
        /* Where the count landed, handed back rather than recomputed by the caller: the
        record's 16-byte header, HEDR's 8-byte header, then 296 bytes before the count.
        It used to be found as "four bytes before the end of the header", which was
        true only while HEDR was the last subrecord — adding masters after it made
        that patch overwrite the final master's file size instead. */
        let num_at = at + 16 + 8 + 296;
        // MAST and its DATA come in pairs and in order; a reader pairs them by
        // position, so one without the other shifts every master after it.
        for (name, size) in masters {
            rec.push(b"MAST", Self::zstr(name));
            rec.push(b"DATA", size.to_le_bytes().to_vec());
        }
        self.emit(&rec);
        num_at
    }

    pub fn stat(&mut self, id: &str, mesh: &str) {
        let mut rec = Record::new(b"STAT");
        rec.push(b"NAME", Self::zstr(id));
        rec.push(b"MODL", Self::zstr(mesh));
        self.emit(&rec);
    }

    /// Round 18bc (A8): `NAM0`, the count of references the record carries.
    ///
    /// It used to be left out altogether, which readers tolerate. Writing it is better
    /// than silence for the tools that preallocate from it (MGE XE's Distant Land, Wrye
    /// Mash's cell report), and it is free here because the count is known before the
    /// first reference is written. It belongs in the header block, after the last of the
    /// cell's own subrecords and before the first `FRMR` — which is where it lands, since
    /// every caller writes its header first.
    #[inline]
    /// Writes one exterior cell full of references, with DATA flags of zero: the
    /// groundcover plugin's cells, which no engine merges into the world.
    pub fn cell(&mut self, name: &str, gx: i32, gy: i32, refs: &[OutRef], first_frmr: u32) -> u32 {
        self.cell_flagged(name, gx, gy, 0, refs, first_frmr)
    }

    /// Round 18gf: [`cell`](Self::cell) with the cell's DATA flags stated. The Objects
    /// plugin is loaded by the game, whose merge takes the last record's flags whole
    /// (`merge_to_master` merge_objects.rs:129), so it writes the flags the load order
    /// gave the cell - a zero would clear "has water" on every exterior it touched.
    pub fn cell_flagged(&mut self, name: &str, gx: i32, gy: i32, flags: u32, refs: &[OutRef], first_frmr: u32) -> u32 {
        // A reference costs about 76 bytes; reserving up front means the whole
        // cell is one growth step rather than a dozen.
        self.out.reserve(refs.len() * 80 + 64);
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", Self::zstr(name));
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&flags.to_le_bytes());
        data[4..8].copy_from_slice(&gx.to_le_bytes());
        data[8..12].copy_from_slice(&gy.to_le_bytes());
        rec.push(b"DATA", data.to_vec());
        self.refs(rec, refs, first_frmr)
    }

    /// The references of one cell: the persistent ones first, then `NAM0` with the count
    /// of temporary ones, then those. Writes the record and answers the next FRMR.
    ///
    /// Round 18gf: NAM0 is positional in the format - everything before it is persistent,
    /// everything after it temporary (`tes3` cell.rs:40, 74-78, 177-182) - and an actor
    /// has to be persistent to exist for the game (reference.rs:237-240). Grass is all
    /// temporary, so a grass cell comes out exactly as before.
    ///
    /// Round 18gi: the cell's own subrecords arrive as a [`Record`] and the references are
    /// streamed straight into the output after them, with the record's size patched in at
    /// the end. Building a `Record` of six million references (18gh) cost four
    /// allocations a reference and 700 ms an export on Robin's install; the bytes are the
    /// same, and the fixture digest says so.
    fn refs(&mut self, rec: Record, refs: &[OutRef], first_frmr: u32) -> u32 {
        let at = self.out.len();
        self.out.extend_from_slice(&rec.tag);
        self.out.extend_from_slice(&0u32.to_le_bytes()); // the size, patched below
        self.out.extend_from_slice(&rec.unknown.to_le_bytes());
        self.out.extend_from_slice(&rec.flags.to_le_bytes());
        for s in &rec.subs {
            self.sub(&s.tag, &s.data);
        }
        let mut frmr = first_frmr;
        let temporaries = refs.iter().filter(|r| !r.persistent).count();
        for r in refs.iter().filter(|r| r.persistent) {
            frmr = self.one_ref(r, frmr);
        }
        self.sub(b"NAM0", &(temporaries as u32).to_le_bytes());
        for r in refs.iter().filter(|r| !r.persistent) {
            frmr = self.one_ref(r, frmr);
        }
        let size = (self.out.len() - at - 16) as u32;
        self.out[at + 4..at + 8].copy_from_slice(&size.to_le_bytes());
        frmr
    }

    /// One subrecord, straight into the output.
    fn sub(&mut self, tag: &[u8; 4], data: &[u8]) {
        self.out.extend_from_slice(tag);
        self.out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.out.extend_from_slice(data);
    }

    fn one_ref(&mut self, r: &OutRef, frmr: u32) -> u32 {
        self.sub(b"FRMR", &frmr.to_le_bytes());
        // NAME: the id in the model's code page, then the NUL.
        let at = self.out.len();
        self.out.extend_from_slice(b"NAME");
        self.out.extend_from_slice(&0u32.to_le_bytes());
        crate::esp::latin1_into(r.id, &mut self.out);
        self.out.push(0);
        let n = (self.out.len() - at - 8) as u32;
        self.out[at + 4..at + 8].copy_from_slice(&n.to_le_bytes());
        // See `clamp_scale`: the game's range for a plugin the game loads.
        let scale = if self.clamp_scale { r.scale.clamp(0.5, 2.0) } else { r.scale };
        if (scale - 1.0).abs() > 1e-4 {
            self.sub(b"XSCL", &scale.to_le_bytes());
        }
        let mut data = [0u8; 24];
        for (i, v) in r.pos.iter().chain(r.rot.iter()).enumerate() {
            data[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.sub(b"DATA", &data);
        frmr + 1
    }
}

impl Writer {
    /// Writes one interior cell's worth of references.
    ///
    /// Minimal on purpose: the name, the room's own flags, and the new references. A room
    /// belongs to whatever plugin defined it — its lighting, water and ambience are that
    /// plugin's — and the game merges references by number, so saying nothing else is
    /// how the original's data survives. The grid in DATA is zero because an interior
    /// has none; the flags are what the engine reads.
    ///
    /// `src_flags` is the room's own DATA flags as the load order left them
    /// ([`crate::world::Interior::flags`]).
    pub fn cell_interior(
        &mut self,
        name: &str,
        src_flags: u32,
        water: Option<f32>,
        refs: &[OutRef],
        first_frmr: u32,
    ) -> u32 {
        self.out.reserve(refs.len() * 80 + 64);
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", Self::zstr(name));
        let mut data = [0u8; 12];
        /* The water flag rides along with the room's own answer. Whether a room has
           water is the flag's to say and the last CELL record's flags win in the game's
           merge — so a plugin that stayed silent about water would *drain* a flooded
           cave the moment it was loaded over it.

           Round 18bc (A2): and by exactly that reasoning, so do the other two flags a
           room can carry. This used to write `1 | water` and nothing else, which cleared
           0x80 (behave like exterior) and 0x04 (illegal to sleep here) on every room it
           touched: grass in one of Vivec's cantons or on Mournhold's plaza — quasi-
           exteriors, and precisely the interiors somebody would scatter in — took the
           sky's weather, sun and hours away from the room. Robin, round 17y: "interiors
           that are tagged to work as exteriors should of course still have their flags
           work as in exteriors too." Only those two travel: 0x01 is what makes the record
           an interior at all and 0x02 is answered by `water` just above, so both are
           this function's to state rather than to copy. */
        let flags = 1u32 | if water.is_some() { 2 } else { 0 } | (src_flags & (0x04 | 0x80));
        data[0..4].copy_from_slice(&flags.to_le_bytes());
        rec.push(b"DATA", data.to_vec());
        if let Some(wh) = water {
            rec.push(b"WHGT", wh.to_le_bytes().to_vec());
        }
        self.refs(rec, refs, first_frmr)
    }
}

/// Borrows its id from the shared id table, so emitting a reference costs no
/// allocation at all.
pub struct OutRef<'a> {
    pub id: &'a str,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: f32,
    /// Round 18gf: written before the cell's `NAM0`, where the format keeps persistent
    /// references. Actors are; grass is not.
    pub persistent: bool,
}

impl<'a> OutRef<'a> {
    pub fn from_ref(r: &'a CellRef) -> OutRef<'a> {
        OutRef { id: &r.id, pos: r.pos, rot: r.rot, scale: r.scale, persistent: !r.temporary }
    }
}

/// Morrowind object ids are length-limited, so a mesh path is turned into a
/// short, stable, collision-free id.
pub fn make_ids(prefix: &str, meshes: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(meshes.len());
    let mut used = std::collections::HashSet::new();
    for (i, m) in meshes.iter().enumerate() {
        let base: String = m
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(m)
            .trim_end_matches(".nif")
            .trim_end_matches(".NIF")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(20)
            .collect();
        let mut id = format!("{}{}", prefix, if base.is_empty() { "grass".into() } else { base });
        if id.len() > 31 {
            id.truncate(31);
        }
        let mut n = 0;
        while !used.insert(id.to_ascii_lowercase()) {
            n += 1;
            let suffix = format!("_{}", n);
            let keep = 31usize.saturating_sub(suffix.len()).min(id.len());
            id = format!("{}{}", &id[..keep], suffix);
            if n > 10_000 {
                id = format!("{}{}", prefix, i);
                break;
            }
        }
        out.push((m.clone(), id));
    }
    out
}
