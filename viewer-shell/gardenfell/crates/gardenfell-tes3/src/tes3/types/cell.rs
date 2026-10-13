//! `CELL`: an interior or exterior cell and the references placed in it. `tes3` cell.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    pub flags: u32,
    pub name: String,
    pub data: CellData,
    pub region: Option<String>,
    /// `NAM5`: RGBA, the color on the world map.
    pub map_color: Option<[u8; 4]>,
    pub water_height: Option<f32>,
    pub atmosphere_data: Option<AtmosphereData>,
    /// The references in file order. `tes3` keeps them in a map keyed by
    /// `(mast_index, refr_index)`, so a key the file carries twice keeps only the last
    /// one there; here both entries are kept, and written back in this order.
    pub references: Vec<Reference>,
}

/// `DATA`, 12 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellData {
    /// `consts::cell::*`.
    pub flags: u32,
    /// The exterior grid, meaningless for an interior.
    pub grid: (i32, i32),
}

impl CellData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<CellData> {
        Ok(CellData { flags: r.u32()?, grid: (r.i32()?, r.i32()?) })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.flags);
        w.i32(self.grid.0);
        w.i32(self.grid.1);
    }
}

/// `AMBI`, 16 bytes: an interior's lighting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtmosphereData {
    pub ambient_color: [u8; 4],
    pub sunlight_color: [u8; 4],
    pub fog_color: [u8; 4],
    pub fog_density: f32,
}

impl AtmosphereData {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<AtmosphereData> {
        Ok(AtmosphereData {
            ambient_color: r.array::<4>()?,
            sunlight_color: r.array::<4>()?,
            fog_color: r.array::<4>()?,
            fog_density: r.f32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.bytes(&self.ambient_color);
        w.bytes(&self.sunlight_color);
        w.bytes(&self.fog_color);
        w.f32(self.fog_density);
    }
}

impl Typed for Cell {
    const TAG: Tag = *b"CELL";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        /* Round 18gh: not `flags_of`, which sets the deleted bit for a `DELE` anywhere in
        the record, and a cell's references carry their own. A reference's `DELE` ends its
        run inside `Reference::read` and never reaches this loop, so a `DELE` seen here is
        the cell's wherever it sits: before the references, as `tes3` writes it, or after
        one's `DATA`, as some tools do. That is how `tes3` reads it (cell.rs) and how the
        engine always has (`world::RawCell::deleted`). */
        let mut this = Self { flags: rec.flags, ..Default::default() };
        let mut temp_refs_section = false;
        // The MVRF word's own indices and the CNDT destination grid, taken by the next FRMR.
        let mut pending_move: Option<((u32, u32), (i32, i32))> = None;
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.name = text(s),
                b"DATA" => this.data = CellData::read(&mut sized(rec, s, CellData::SIZE)?)?,
                b"RGNN" => this.region = Some(text(s)),
                b"NAM5" => this.map_color = Some(sized(rec, s, 4)?.array::<4>()?),
                b"WHGT" => this.water_height = Some(f32_of(rec, s)?),
                b"AMBI" => {
                    // Some editors pad AMBI past 16 bytes; tes3 reads the first 16 and skips
                    // the rest, and a shorter one is refused.
                    let mut r = if s.data.len() > AtmosphereData::SIZE {
                        reader(s)
                    } else {
                        sized(rec, s, AtmosphereData::SIZE)?
                    };
                    this.atmosphere_data = Some(AtmosphereData::read(&mut r)?);
                }
                b"NAM0" => {
                    // The count of temporary references, which the position says anyway.
                    let _num_temp_refs = i32_of(rec, s)?;
                    temp_refs_section = true;
                }
                b"MVRF" => {
                    let word = unpack(u32_of(rec, s)?);
                    // An MVRF is always paired with a CNDT naming the destination cell.
                    let c = it.expect(b"CNDT")?;
                    let mut r = sized(rec, c, 8)?;
                    let moved_cell = (r.i32()?, r.i32()?);
                    pending_move = Some((word, moved_cell));
                }
                b"FRMR" => {
                    let indices = unpack(u32_of(rec, s)?);
                    let mut reference = Reference::read(rec, &mut it)?;
                    reference.mast_index = indices.0;
                    reference.refr_index = indices.1;
                    reference.temporary = temp_refs_section;
                    /* The engine pairs MVRF with the next FRMR by position. Round 18gh: a
                    pending move is kept whatever its word said, and a word that is not
                    the FRMR's is kept beside it as `moved_index`, so the record writes
                    back as it was and the caller applies its engine's rule (`tes3` drops
                    such a move, OpenMW applies it). */
                    if let Some((word, moved_cell)) = pending_move.take() {
                        reference.moved_cell = Some(moved_cell);
                        if word != indices {
                            reference.moved_index = Some(word);
                        }
                    }
                    this.references.push(reference);
                }
                b"INTV" => {
                    // The integer water height of plugin version 1.2, which comes back as
                    // a float WHGT, as tes3 writes it.
                    let water_height = i32_of(rec, s)?;
                    this.water_height = Some(water_height as f32);
                }
                b"DELE" => this.flags |= FLAG_DELETED,
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.name)?;
        out.sub(b"DATA", |w| {
            self.data.write(w);
            Ok(())
        })?;
        if let Some(v) = &self.region {
            out.text(b"RGNN", v)?;
        }
        if let Some(v) = &self.map_color {
            out.bytes(b"NAM5", v.to_vec());
        }
        if let Some(v) = self.water_height {
            out.f32(b"WHGT", v);
        }
        if let Some(v) = &self.atmosphere_data {
            out.sub(b"AMBI", |w| {
                v.write(w);
                Ok(())
            })?;
        }
        // tes3 writes a cell's DELE here, before its references, not last.
        out.dele();
        // tes3 sorts the references by `Reference::sort_key` here. They are written in the
        // order they are held, and `sort_references` puts them in tes3's order first.
        let mut num_temp_refs = 0;
        for (i, reference) in self.references.iter().enumerate() {
            let packed_indices = pack((reference.mast_index, reference.refr_index));
            // NAM0 goes where the file had it: before the first reference that was read
            // after it. tes3 decides this from `persistent()` instead, which moves a door
            // with a destination ahead of NAM0 even when the file had it after; the
            // `sort_references` helper applies that order for a caller that wants it.
            if num_temp_refs == 0 && reference.temporary {
                num_temp_refs = self.references.len() - i;
                out.u32(b"NAM0", num_temp_refs as u32);
            }
            if let Some(v) = &reference.moved_cell {
                // The MVRF's own word where it named another reference (round 18gh).
                out.u32(b"MVRF", reference.moved_index.map(pack).unwrap_or(packed_indices));
                out.sub(b"CNDT", |w| {
                    w.i32(v.0);
                    w.i32(v.1);
                    Ok(())
                })?;
            }
            out.u32(b"FRMR", packed_indices);
            reference.write(&mut out)?;
        }
        Ok(out.finish())
    }
}

impl Cell {
    pub fn is_interior(&self) -> bool {
        self.data.flags & consts::cell::IS_INTERIOR != 0
    }

    pub fn is_exterior(&self) -> bool {
        !self.is_interior()
    }

    pub fn exterior_coords(&self) -> Option<(i32, i32)> {
        if self.is_exterior() {
            Some(self.data.grid)
        } else {
            None
        }
    }

    pub fn get_region(&self) -> &str {
        self.region.as_deref().unwrap_or("Wilderness")
    }

    /// Puts the references in the order `tes3` writes them: persistent before temporary,
    /// a master's before the plugin's own, and by index within one source.
    pub fn sort_references(&mut self) {
        self.references.sort_by_key(Reference::sort_key);
    }

    pub fn sort_key(&self) -> (Option<(i32, i32)>, &str) {
        (self.exterior_coords(), &self.name)
    }
}

/// The `FRMR` dword: the master index in the top byte, the reference index below it.
const fn unpack(packed_indices: u32) -> (u32, u32) {
    let mast_index = packed_indices >> 24;
    let refr_index = packed_indices & 0xFFFFFF;
    (mast_index, refr_index)
}

const fn pack(indices: (u32, u32)) -> u32 {
    let (mast_index, refr_index) = indices;
    refr_index | (mast_index << 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32s(v: &[f32]) -> Vec<u8> {
        let mut w = Writer::new();
        w.f32s(v);
        w.into_bytes()
    }

    #[test]
    fn a_cell_round_trips_with_a_moved_reference_and_a_nam0_boundary() {
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"Seyda Neen\0".to_vec());
        let mut w = Writer::new();
        CellData { flags: consts::cell::HAS_WATER, grid: (-2, -9) }.write(&mut w);
        rec.push(b"DATA", w.into_bytes());
        rec.push(b"RGNN", b"Bitter Coast Region\0".to_vec());
        rec.push(b"NAM5", vec![10, 20, 30, 0]);
        rec.push(b"WHGT", 0f32.to_le_bytes().to_vec());
        // A master's reference, moved here from the cell next door: persistent.
        rec.push(b"MVRF", 0x0100_0005u32.to_le_bytes().to_vec());
        rec.push(b"CNDT", [(-3i32).to_le_bytes(), (-9i32).to_le_bytes()].concat());
        rec.push(b"FRMR", 0x0100_0005u32.to_le_bytes().to_vec());
        rec.push(b"NAME", b"flora_bc_fern_02\0".to_vec());
        rec.push(b"DATA", f32s(&[-16000.0, -70000.0, 10.0, 0.0, 0.0, 1.0]));
        // The plugin's own reference, after NAM0: temporary.
        rec.push(b"NAM0", 1u32.to_le_bytes().to_vec());
        rec.push(b"FRMR", 7u32.to_le_bytes().to_vec());
        rec.push(b"NAME", b"flora_bc_grass_01\0".to_vec());
        rec.push(b"XSCL", 1.5f32.to_le_bytes().to_vec());
        rec.push(b"DATA", f32s(&[-16100.0, -70100.0, 12.0, 0.0, 0.0, 2.0]));
        let c = Cell::from_raw(&rec).unwrap();
        assert!(c.is_exterior());
        assert_eq!(c.exterior_coords(), Some((-2, -9)));
        assert_eq!(c.get_region(), "Bitter Coast Region");
        assert_eq!(c.references.len(), 2);
        let a = &c.references[0];
        assert_eq!((a.mast_index, a.refr_index), (1, 5));
        assert_eq!(a.moved_cell, Some((-3, -9)));
        assert!(!a.temporary);
        let b = &c.references[1];
        assert_eq!((b.mast_index, b.refr_index), (0, 7));
        assert!(b.temporary);
        assert_eq!(b.scale, Some(1.5));
        assert_eq!(c.to_raw().unwrap(), rec);

        // A padded AMBI reads, and comes back at 16 bytes; a deleted cell has its DELE
        // before its references.
        let mut int = Record::new(b"CELL");
        int.flags = consts::object::DELETED;
        int.push(b"NAME", b"Arrille's Tradehouse\0".to_vec());
        let mut w = Writer::new();
        CellData { flags: consts::cell::IS_INTERIOR, grid: (0, 0) }.write(&mut w);
        int.push(b"DATA", w.into_bytes());
        let mut w = Writer::new();
        let ambient = AtmosphereData {
            ambient_color: [1, 2, 3, 0],
            sunlight_color: [4, 5, 6, 0],
            fog_color: [7, 8, 9, 0],
            fog_density: 1.0,
        };
        ambient.write(&mut w);
        let mut ambi = w.into_bytes();
        ambi.extend_from_slice(&[0; 4]);
        int.push(b"AMBI", ambi);
        int.push(b"FRMR", 1u32.to_le_bytes().to_vec());
        int.push(b"NAME", b"barrel_01\0".to_vec());
        int.push(b"DATA", f32s(&[0.0; 6]));
        let c = Cell::from_raw(&int).unwrap();
        assert!(c.is_interior());
        assert_eq!(c.atmosphere_data.as_ref().map(|a| a.fog_color), Some([7, 8, 9, 0]));
        let back = c.to_raw().unwrap();
        let tags: Vec<Tag> = back.subs.iter().map(|s| s.tag).collect();
        assert_eq!(tags, vec![*b"NAME", *b"DATA", *b"AMBI", *b"DELE", *b"FRMR", *b"NAME", *b"DATA"]);
        assert_eq!(back.subs[2].data.len(), 16);
        assert_eq!(Cell::from_raw(&back).unwrap(), c);
    }

    /// Round 18gh (backend-plan D2): an MVRF naming another reference than the FRMR after
    /// it keeps the move and the word, and comes back byte for byte. 26 references in
    /// Robin's install are this shape; `to_raw` used to drop their MVRF and CNDT.
    #[test]
    fn a_mismatched_mvrf_keeps_its_own_word_and_round_trips() {
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"\0".to_vec());
        let mut w = Writer::new();
        CellData { flags: 0, grid: (1, 1) }.write(&mut w);
        rec.push(b"DATA", w.into_bytes());
        rec.push(b"NAM0", 1u32.to_le_bytes().to_vec());
        rec.push(b"MVRF", 0x0100_0002u32.to_le_bytes().to_vec());
        rec.push(b"CNDT", [2i32.to_le_bytes(), 1i32.to_le_bytes()].concat());
        rec.push(b"FRMR", 0x0100_0001u32.to_le_bytes().to_vec());
        rec.push(b"NAME", b"rock\0".to_vec());
        rec.push(b"DATA", f32s(&[0.0; 6]));
        let c = Cell::from_raw(&rec).unwrap();
        let r = &c.references[0];
        assert_eq!((r.mast_index, r.refr_index), (1, 1));
        assert_eq!(r.moved_cell, Some((2, 1)), "the move is kept for the caller to judge");
        assert_eq!(r.moved_index, Some((1, 2)), "with the word that named another reference");
        assert_eq!(c.to_raw().unwrap(), rec);
        assert_eq!(Cell::from_raw(&c.to_raw().unwrap()).unwrap(), c);

        // A master byte that differs alone is kept the same way; a matching word is not.
        rec.subs[3].data = 0x0200_0001u32.to_le_bytes().to_vec().into();
        let c = Cell::from_raw(&rec).unwrap();
        assert_eq!(c.references[0].moved_index, Some((2, 1)));
        assert_eq!(c.to_raw().unwrap(), rec);
        rec.subs[3].data = 0x0100_0001u32.to_le_bytes().to_vec().into();
        let c = Cell::from_raw(&rec).unwrap();
        assert_eq!(c.references[0].moved_index, None);
        assert_eq!(c.references[0].moved_cell, Some((2, 1)));
        assert_eq!(c.to_raw().unwrap(), rec);
    }

    /// Round 18gh: a reference's DELE is the reference's, a DELE outside a reference's run
    /// is the cell's, wherever it sits. `flags_of` used to call the cell deleted for a
    /// deleted reference, and `to_raw` then wrote a DELE the file never had.
    #[test]
    fn a_references_dele_does_not_delete_the_cell() {
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"Room\0".to_vec());
        rec.push(b"DATA", vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        rec.push(b"FRMR", 1u32.to_le_bytes().to_vec());
        rec.push(b"NAME", b"gone\0".to_vec());
        rec.push(b"DELE", vec![0; 4]);
        let c = Cell::from_raw(&rec).unwrap();
        assert_eq!(c.flags & FLAG_DELETED, 0, "the cell is not deleted");
        assert!(c.references[0].deleted());
        assert_eq!(c.to_raw().unwrap(), rec);

        // The cell's own DELE after a reference's DATA is still the cell's.
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"Room\0".to_vec());
        rec.push(b"DATA", vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        rec.push(b"FRMR", 1u32.to_le_bytes().to_vec());
        rec.push(b"NAME", b"kept\0".to_vec());
        rec.push(b"DATA", f32s(&[0.0; 6]));
        rec.push(b"DELE", vec![0; 4]);
        let c = Cell::from_raw(&rec).unwrap();
        assert_ne!(c.flags & FLAG_DELETED, 0);
        assert!(!c.references[0].deleted());
    }

    #[test]
    fn an_mvrf_without_its_cndt_and_a_short_data_are_refused() {
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"DATA", vec![0; 12]);
        rec.push(b"MVRF", 5u32.to_le_bytes().to_vec());
        rec.push(b"FRMR", 5u32.to_le_bytes().to_vec());
        let e = Cell::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CELL::FRMR where CNDT was expected"), "{e}");

        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"DATA", vec![0; 8]);
        let e = Cell::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CELL::DATA is 8 bytes"), "{e}");
    }
}
