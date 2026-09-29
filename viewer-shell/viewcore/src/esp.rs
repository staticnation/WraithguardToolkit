//! TES3 (.esm/.esp) reader.
//!
//! The whole plugin is read into one allocation and then walked with slices, so
//! parsing costs one syscall plus a linear scan. Records we do not care about are
//! skipped by advancing past their body without ever touching the bytes, which is
//! what makes a filtered scan of an 80 MB master nearly free.

pub type Tag = [u8; 4];


pub const TES3: Tag = *b"TES3";
pub const CELL: Tag = *b"CELL";
pub const LAND: Tag = *b"LAND";
pub const LTEX: Tag = *b"LTEX";
pub const REGN: Tag = *b"REGN";


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


pub fn cstring(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}




/// One record's header plus a borrow of its body.
pub struct Record<'a> {
    pub tag: Tag,
    pub flags: u32,
    pub body: &'a [u8],
}

/// Walks the top level of a plugin. `want` decides whether a record's body is
/// handed over; records it rejects are skipped without being read.
pub struct Records<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Records<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn next_filtered(&mut self, want: &dyn Fn(Tag) -> bool) -> Option<Record<'a>> {
        loop {
            if self.pos + 16 > self.buf.len() {
                return None;
            }
            let h = &self.buf[self.pos..self.pos + 16];
            let t: Tag = [h[0], h[1], h[2], h[3]];
            let size = u32le(h, 4) as usize;
            let flags = u32le(h, 12);
            let body_at = self.pos + 16;
            if body_at + size > self.buf.len() {
                return None; // truncated or corrupt: stop cleanly
            }
            self.pos = body_at + size;
            if want(t) {
                return Some(Record { tag: t, flags, body: &self.buf[body_at..body_at + size] });
            }
        }
    }
}

/// Iterates the subrecords of a record body.
pub struct Subs<'a> {
    buf: &'a [u8],
    pos: usize,
}

pub struct Sub<'a> {
    pub tag: Tag,
    pub data: &'a [u8],
}

impl<'a> Subs<'a> {
    #[inline]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
}

impl<'a> Iterator for Subs<'a> {
    type Item = Sub<'a>;
    #[inline]
    fn next(&mut self) -> Option<Sub<'a>> {
        if self.pos + 8 > self.buf.len() {
            return None;
        }
        let h = &self.buf[self.pos..self.pos + 8];
        let t: Tag = [h[0], h[1], h[2], h[3]];
        let size = u32le(h, 4) as usize;
        let at = self.pos + 8;
        if at + size > self.buf.len() {
            return None;
        }
        self.pos = at + size;
        Some(Sub { tag: t, data: &self.buf[at..at + size] })
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
    /// Wraithguard: every plugin in the load order that supplied a version of this
    /// reference, oldest first (the last is `plugin`). What the ORI panel lists as
    /// "changed by". Filled in by the merge.
    pub touched: Vec<i32>,
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
            plugin: -1,
            door: None,
            touched: Vec::new(),
        }
    }
}

/// Parses the reference run of a CELL body.
///
/// MVRF/CNDT precede the FRMR they describe, so a pending marker is carried
/// forward rather than applied to the reference that came before.
pub fn parse_refs(body: &[u8], plugin_idx: i32, masters: &[i32], out: &mut Vec<CellRef>) {
    let mut cur: Option<CellRef> = None;
    let mut pending_move: Option<Option<(i32, i32)>> = None;

    for s in Subs::new(body) {
        match &s.tag {
            b"MVRF" => {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
                pending_move = Some(None);
            }
            b"CNDT" => {
                if let Some(slot) = pending_move.as_mut() {
                    if s.data.len() >= 8 {
                        *slot = Some((i32le(s.data, 0), i32le(s.data, 4)));
                    }
                }
            }
            b"FRMR" => {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
                if s.data.len() >= 4 {
                    let raw = u32le(s.data, 0);
                    cur = Some(CellRef {
                        num: RefNum::resolve(raw, plugin_idx, masters),
                        moved_to: pending_move.take().flatten(),
                        ..Default::default()
                    });
                } else {
                    pending_move = None;
                }
            }
            b"NAME" => {
                if let Some(c) = cur.as_mut() {
                    c.id = cstring(s.data);
                }
            }
            b"XSCL" => {
                if let Some(c) = cur.as_mut() {
                    if s.data.len() >= 4 {
                        c.scale = f32le(s.data, 0);
                    }
                }
            }
            b"DELE" => {
                if let Some(c) = cur.as_mut() {
                    c.deleted = true;
                }
            }
            /* The door's teleport: 24 bytes like DATA, and once only kept apart from it.
               Round 17y keeps it - position and rotation at the far side - and the DNAM
               that follows names the interior it opens into; a door into the exterior has
               no DNAM. */
            b"DODT" => {
                if let Some(c) = cur.as_mut() {
                    if s.data.len() >= 24 {
                        c.door = Some(DoorDest {
                            pos: [f32le(s.data, 0), f32le(s.data, 4), f32le(s.data, 8)],
                            rot: [f32le(s.data, 12), f32le(s.data, 16), f32le(s.data, 20)],
                            cell: String::new(),
                        });
                    }
                }
            }
            b"DNAM" => {
                if let Some(c) = cur.as_mut() {
                    if let Some(d) = c.door.as_mut() {
                        d.cell = cstring(s.data);
                    }
                }
            }
            b"DATA" => {
                if let Some(c) = cur.as_mut() {
                    if s.data.len() >= 24 {
                        c.pos = [f32le(s.data, 0), f32le(s.data, 4), f32le(s.data, 8)];
                        c.rot = [f32le(s.data, 12), f32le(s.data, 16), f32le(s.data, 20)];
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(c) = cur.take() {
        out.push(c);
    }
}

/// Reads the cell's own header fields, stopping at the first FRMR.
///
/// A CELL body repeats NAME and DATA for every reference it contains, so reading
/// past the first FRMR overwrites the cell's name and grid with the last
/// reference's.
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
    pub refs_at: usize,
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

pub fn parse_cell_header(body: &[u8]) -> CellHeader {
    let mut h = CellHeader {
        name: String::new(),
        flags: 0,
        grid: (0, 0),
        region: String::new(),
        has_data: false,
        water: None,
        ambi: None,
        refs_at: body.len(),
    };
    /* Held aside rather than written straight into the header: `INTV` can arrive
       before the `DATA` that says whether this is even an interior, and an exterior
       carrying one must not be read as a room with water. `WHGT` wins where a file
       somehow has both — it is the newer spelling and the one exports write. */
    let mut intv: Option<f32> = None;
    let mut pos = 0usize;
    while pos + 8 <= body.len() {
        let t: Tag = [body[pos], body[pos + 1], body[pos + 2], body[pos + 3]];
        let size = u32le(body, pos + 4) as usize;
        let at = pos + 8;
        if at + size > body.len() {
            break;
        }
        if &t == b"FRMR" || &t == b"MVRF" {
            h.refs_at = pos;
            settle_water(&mut h, intv);
            return h;
        }
        match &t {
            b"NAME" => h.name = cstring(&body[at..at + size]),
            b"RGNN" => h.region = cstring(&body[at..at + size]),
            b"WHGT" if size >= 4 => {
                h.water = Some(f32le(body, at));
            }
            b"INTV" if size >= 4 => {
                intv = Some(i32le(body, at) as f32);
            }
            b"DATA" if size >= 12 => {
                h.flags = u32le(body, at);
                h.grid = (i32le(body, at + 4), i32le(body, at + 8));
                h.has_data = true;
            }
            // Four little-endian dwords: three RGBA colours (R in the low byte) and a float.
            b"AMBI" if size >= 16 => {
                let col = |o: usize| [body[at + o], body[at + o + 1], body[at + o + 2]];
                h.ambi = Some(Ambi {
                    ambient: col(0),
                    sunlight: col(4),
                    fog: col(8),
                    fog_density: f32le(body, at + 12),
                });
            }
            _ => {}
        }
        pos = at + size;
    }
    settle_water(&mut h, intv);
    h
}

/// The `INTV` level counts only for an interior, and only where no `WHGT` spoke.
fn settle_water(h: &mut CellHeader, intv: Option<f32>) {
    if h.water.is_none() && is_interior(h.flags) {
        h.water = intv;
    }
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
/// `None` when the bytes are not a plugin at all — too short, or a first record that is
/// not `TES3`. A header that is present but empty is `Some` with nothing in it: a plugin
/// with no masters is an ordinary thing and is not a failure to read.
pub fn plugin_head(buf: &[u8]) -> Option<PluginHead> {
    if buf.len() < 16 || buf[0..4] != TES3 {
        return None;
    }
    let size = u32le(buf, 4) as usize;
    let end = 16usize.checked_add(size)?.min(buf.len());
    let mut out = PluginHead::default();
    for s in Subs::new(&buf[16..end]) {
        match &s.tag {
            b"MAST" => out.masters.push(cstring(s.data).to_ascii_lowercase()),
            /* The description sits inside HEDR, after the version, the file type and a
               32-byte company field: 4 + 4 + 32, then 256 bytes of text. Anything shorter
               is a header this cannot read the description out of, and the masters — the
               part that matters here — are in their own subrecords regardless. */
            b"HEDR" if s.data.len() >= 40 + 256 => {
                out.desc = cstring(&s.data[40..40 + 256]).trim().to_string();
            }
            _ => {}
        }
    }
    Some(out)
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
    Some(16 + u32le(first16, 4) as usize)
}
