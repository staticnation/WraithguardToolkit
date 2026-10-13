//! `FRMR`: an object placed in a cell, the run of subrecords after a `FRMR` dword up to
//! its `DATA` (or its `DELE`). Carried by `CELL`. `tes3` reference.rs.
//!
//! The `FRMR` dword itself, the `MVRF` and `CNDT` that may precede it and the cell's
//! `NAM0` are read and written by [`Cell`](super::Cell), which fills `mast_index`,
//! `refr_index`, `moved_cell` and `temporary` from them; this file is the run between.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reference {
    /// The top byte of the `FRMR` dword: the 1-based master the reference came from, 0
    /// for one the plugin itself placed.
    pub mast_index: u32,
    /// The low 24 bits of the `FRMR` dword.
    pub refr_index: u32,
    pub id: String,
    /// Whether the reference sits after the cell's `NAM0`.
    pub temporary: bool,
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: Option<f32>,
    /// The grid an `MVRF` and `CNDT` pair moved the reference to.
    pub moved_cell: Option<(i32, i32)>,
    /// Round 18gh (backend-plan D2): the `MVRF` word's own master and reference index,
    /// kept only when it differs from the `FRMR` that followed it. The Construction Set
    /// never writes that; a malformed plugin can, and the two engines disagree about it:
    /// `tes3` drops the move (cell.rs:101-106), OpenMW applies it to the reference that
    /// followed (loadcell.cpp:295-321). Both the move and this word are kept, so the caller
    /// chooses (`world::LoadOpts::mvrf_strict`) and the record writes back byte for byte.
    /// `None` for the well-formed case, which is every reference the Construction Set made.
    pub moved_index: Option<(u32, u32)>,
    pub owner: Option<String>,
    pub owner_global: Option<String>,
    pub owner_faction: Option<String>,
    pub owner_faction_rank: Option<i32>,
    pub charge_left: Option<u32>,
    pub health_left: Option<i32>,
    pub object_count: Option<u32>,
    /// `DODT`, and the `DNAM` cell name that follows it for a door into an interior.
    pub destination: Option<TravelDestination>,
    pub lock_level: Option<i32>,
    pub key: Option<String>,
    pub trap: Option<String>,
    pub soul: Option<String>,
    pub blocked: Option<u8>,
    pub deleted: Option<bool>,
}

impl Reference {
    /// Reads the run after a `FRMR`, stopping after `DATA` or `DELE`, or at the end of the
    /// record. `mast_index`, `refr_index`, `temporary`, `moved_cell` and `moved_index` are
    /// the caller's.
    pub fn read(rec: &Record, it: &mut Subs<'_>) -> Result<Reference> {
        let mut this = Reference::default();
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"UNAM" => this.blocked = Some(u8_of(rec, s)?),
                b"XSCL" => this.scale = Some(f32_of(rec, s)?),
                b"ANAM" => this.owner = Some(text(s)),
                b"BNAM" => this.owner_global = Some(text(s)),
                b"CNAM" => this.owner_faction = Some(text(s)),
                b"INDX" => this.owner_faction_rank = Some(i32_of(rec, s)?),
                b"XSOL" => this.soul = Some(text(s)),
                b"XCHG" => this.charge_left = Some(u32_of(rec, s)?),
                b"INTV" => this.health_left = Some(i32_of(rec, s)?),
                b"NAM9" => this.object_count = Some(u32_of(rec, s)?),
                b"DODT" => {
                    let mut r = sized(rec, s, 24)?;
                    let translation = r.f32s::<3>()?;
                    let rotation = r.f32s::<3>()?;
                    let cell = if it.peek_tag() == Some(b"DNAM") {
                        text(it.expect(b"DNAM")?)
                    } else {
                        String::new()
                    };
                    this.destination = Some(TravelDestination { translation, rotation, cell });
                }
                b"FLTV" => this.lock_level = Some(i32_of(rec, s)?),
                b"KNAM" => this.key = Some(text(s)),
                b"TNAM" => this.trap = Some(text(s)),
                b"DATA" => {
                    let mut r = sized(rec, s, 24)?;
                    this.translation = r.f32s::<3>()?;
                    this.rotation = r.f32s::<3>()?;
                    break;
                }
                b"DELE" => {
                    this.deleted = Some(true);
                    break;
                }
                _ => return Err(unexpected(rec, s)),
            }
        }
        // tes3 zeroes a non-finite translation or rotation and drops a non-finite scale
        // here; the file's values are kept.
        Ok(this)
    }

    /// Writes the run after the `FRMR`, which the caller has written.
    pub fn write(&self, out: &mut Out) -> Result<()> {
        out.text(b"NAME", &self.id)?;
        if let Some(v) = self.blocked {
            out.u8(b"UNAM", v);
        }
        if let Some(scale) = self.scale {
            // tes3 clamps the scale to 0.5..2.0 and drops a 1.0 on the plugin's own
            // references. The file's value is written as it is: Morrowind.esm itself
            // carries explicit 1.0 scales, and a value that was there comes back.
            out.f32(b"XSCL", scale);
        }
        if let Some(v) = &self.owner {
            out.text(b"ANAM", v)?;
        }
        if let Some(v) = &self.owner_global {
            out.text(b"BNAM", v)?;
        }
        if let Some(v) = &self.owner_faction {
            out.text(b"CNAM", v)?;
        }
        if let Some(v) = self.owner_faction_rank {
            out.i32(b"INDX", v);
        }
        if let Some(v) = &self.soul {
            out.text(b"XSOL", v)?;
        }
        if let Some(v) = self.charge_left {
            out.u32(b"XCHG", v);
        }
        if let Some(v) = self.health_left {
            out.i32(b"INTV", v);
        }
        if let Some(v) = self.object_count {
            // tes3 drops a count of 1 on the plugin's own references; see XSCL above.
            out.u32(b"NAM9", v);
        }
        if let Some(d) = &self.destination {
            out.sub(b"DODT", |w| {
                w.f32s(&d.translation);
                w.f32s(&d.rotation);
                Ok(())
            })?;
            out.text_if(b"DNAM", &d.cell)?;
        }
        if let Some(v) = self.lock_level {
            out.i32(b"FLTV", v);
        }
        if let Some(v) = &self.key {
            out.text(b"KNAM", v)?;
        }
        if let Some(v) = &self.trap {
            out.text(b"TNAM", v)?;
        }
        if self.deleted.is_some() {
            out.bytes(b"DELE", vec![0, 0, 0, 0]);
        } else {
            out.sub(b"DATA", |w| {
                w.f32s(&self.translation);
                w.f32s(&self.rotation);
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn deleted(&self) -> bool {
        matches!(self.deleted, Some(true))
    }

    /// Whether the reference belongs before the cell's `NAM0`: a moved reference and a
    /// door with a destination always do, everything else trusts `temporary`.
    pub fn persistent(&self) -> bool {
        if self.moved_cell.is_some() {
            return true;
        }
        if self.destination.is_some() {
            return true;
        }
        !self.temporary
    }

    /// `tes3`'s order for a cell's references: persistent ones first, then by master (the
    /// plugin's own last), then by index.
    pub fn sort_key(&self) -> (bool, u32, u32) {
        (
            !self.persistent(),
            match self.mast_index {
                0 => u32::MAX,
                i => i,
            },
            self.refr_index,
        )
    }
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
    fn a_reference_run_round_trips_and_stops_at_its_data() {
        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"in_de_door\0".to_vec());
        rec.push(b"XSCL", 1.5f32.to_le_bytes().to_vec());
        rec.push(b"ANAM", b"fargoth\0".to_vec());
        rec.push(b"INDX", 2i32.to_le_bytes().to_vec());
        rec.push(b"NAM9", 3u32.to_le_bytes().to_vec());
        rec.push(b"DODT", f32s(&[1.0, 2.0, 3.0, 0.0, 0.0, 1.5]));
        rec.push(b"DNAM", b"Seyda Neen, Arrille's Tradehouse\0".to_vec());
        rec.push(b"FLTV", 50i32.to_le_bytes().to_vec());
        rec.push(b"KNAM", b"key_arrille\0".to_vec());
        rec.push(b"DATA", f32s(&[100.0, 200.0, 300.0, 0.0, 0.0, 3.0]));
        rec.push(b"RGNN", b"after\0".to_vec());
        let mut it = Subs::new(&rec);
        let r = Reference::read(&rec, &mut it).unwrap();
        assert_eq!(it.peek_tag(), Some(b"RGNN"));
        assert_eq!(r.id, "in_de_door");
        assert_eq!(r.scale, Some(1.5));
        assert_eq!(r.owner_faction_rank, Some(2));
        assert_eq!(r.object_count, Some(3));
        assert_eq!(r.destination.as_ref().map(|d| d.cell.as_str()), Some("Seyda Neen, Arrille's Tradehouse"));
        assert_eq!(r.translation, [100.0, 200.0, 300.0]);
        assert!(r.persistent());
        let mut out = Out::new(b"CELL", 0);
        r.write(&mut out).unwrap();
        assert_eq!(out.finish().subs.as_slice(), &rec.subs[..10]);

        // A deleted reference ends at its DELE and writes one back in place of DATA.
        let mut del = Record::new(b"CELL");
        del.push(b"NAME", b"gone\0".to_vec());
        del.push(b"DELE", vec![0; 4]);
        let r = Reference::read(&del, &mut Subs::new(&del)).unwrap();
        assert!(r.deleted());
        let mut out = Out::new(b"CELL", 0);
        r.write(&mut out).unwrap();
        assert_eq!(out.finish().subs, del.subs);
    }

    #[test]
    fn a_default_scale_and_count_are_kept_and_a_short_xscl_is_refused() {
        // tes3 drops these two when they hold their defaults on the plugin's own reference.
        // Morrowind.esm writes them, so they are kept, and the read-back is the value.
        let r = Reference { id: "x".into(), scale: Some(1.0), object_count: Some(1), ..Default::default() };
        let mut out = Out::new(b"CELL", 0);
        r.write(&mut out).unwrap();
        let tags: Vec<Tag> = out.finish().subs.iter().map(|s| s.tag).collect();
        assert_eq!(tags, vec![*b"NAME", *b"XSCL", *b"NAM9", *b"DATA"]);
        let r = Reference { id: "x".into(), ..Default::default() };
        let mut out = Out::new(b"CELL", 0);
        r.write(&mut out).unwrap();
        let tags: Vec<Tag> = out.finish().subs.iter().map(|s| s.tag).collect();
        assert_eq!(tags, vec![*b"NAME", *b"DATA"]);

        let mut rec = Record::new(b"CELL");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"XSCL", vec![0; 3]);
        let e = Reference::read(&rec, &mut Subs::new(&rec)).unwrap_err();
        assert!(e.what.contains("CELL::XSCL is 3 bytes"), "{e}");
    }
}
