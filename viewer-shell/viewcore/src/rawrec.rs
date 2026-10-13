//! Wraithguard's raw plugin walker: records and their subrecords as byte slices, and the
//! cell header and reference run read straight off a CELL body. Kept from viewcore's own
//! reader when `esp` became Gardenfell's (whose raw layer is `gardenfell_tes3::tes3::raw`):
//! the inspector, the NPC assembly and the object index read records this way.

use crate::esp::{cstring, f32le, i32le, is_interior, u32le, Ambi, CellRef, DoorDest, RefNum, Tag};

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
