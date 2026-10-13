//! `AI_T`, `AI_W`, `AI_E`, `AI_F`, `AI_A` and `DODT`: an actor's AI packages and travel
//! destinations, carried by `NPC_` and `CREA`. `tes3` aipackage.rs.
//!
//! An escort or follow package may be followed by a `CNDT` naming the target's cell, and
//! a destination by a `DNAM` naming its cell, so both are read with the `Subs` cursor.

use super::*;

/// One AI package, which the record carries in file order.
#[derive(Clone, Debug, PartialEq)]
pub enum AiPackage {
    Travel(AiTravelPackage),
    Wander(AiWanderPackage),
    Escort(AiEscortPackage),
    Follow(AiFollowPackage),
    Activate(AiActivatePackage),
}

impl Default for AiPackage {
    fn default() -> Self {
        AiPackage::Travel(AiTravelPackage::default())
    }
}

/// `AI_T`, 16 bytes: the location, the reset byte, three padding bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiTravelPackage {
    pub location: [f32; 3],
    pub reset: u8,
}

/// `AI_W`, 14 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiWanderPackage {
    pub distance: u16,
    pub duration: u16,
    pub game_hour: u8,
    pub idle2: u8,
    pub idle3: u8,
    pub idle4: u8,
    pub idle5: u8,
    pub idle6: u8,
    pub idle7: u8,
    pub idle8: u8,
    pub idle9: u8,
    pub reset: i8,
}

/// `AI_E`, 48 bytes with one padding byte after `reset`, then an optional `CNDT` holding
/// `cell`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiEscortPackage {
    pub location: [f32; 3],
    pub duration: u16,
    /// A 32-byte fixed string.
    pub target: String,
    pub reset: u8,
    pub cell: String,
}

/// `AI_F`, 48 bytes with one padding byte after `reset`, then an optional `CNDT` holding
/// `cell`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiFollowPackage {
    pub location: [f32; 3],
    pub duration: u16,
    /// A 32-byte fixed string.
    pub target: String,
    pub reset: u8,
    pub cell: String,
}

/// `AI_A`, 33 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiActivatePackage {
    /// A 32-byte fixed string.
    pub target: String,
    pub reset: u8,
}

/// `DODT`, 24 bytes, then an optional `DNAM` holding `cell`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TravelDestination {
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub cell: String,
}

impl AiTravelPackage {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<AiTravelPackage> {
        let location = r.f32s::<3>()?;
        let reset = r.u8()?;
        r.skip(3)?;
        Ok(AiTravelPackage { location, reset })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32s(&self.location);
        w.u8(self.reset);
        w.bytes(&[0; 3]);
    }
}

impl AiWanderPackage {
    pub const SIZE: usize = 14;

    pub fn read(r: &mut Reader<'_>) -> Result<AiWanderPackage> {
        Ok(AiWanderPackage {
            distance: r.u16()?,
            duration: r.u16()?,
            game_hour: r.u8()?,
            idle2: r.u8()?,
            idle3: r.u8()?,
            idle4: r.u8()?,
            idle5: r.u8()?,
            idle6: r.u8()?,
            idle7: r.u8()?,
            idle8: r.u8()?,
            idle9: r.u8()?,
            reset: r.i8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u16(self.distance);
        w.u16(self.duration);
        w.u8(self.game_hour);
        w.u8(self.idle2);
        w.u8(self.idle3);
        w.u8(self.idle4);
        w.u8(self.idle5);
        w.u8(self.idle6);
        w.u8(self.idle7);
        w.u8(self.idle8);
        w.u8(self.idle9);
        w.i8(self.reset);
    }
}

impl AiEscortPackage {
    pub const SIZE: usize = 48;

    /// The `AI_E` body. `cell` comes from the `CNDT` after it and is left empty here.
    pub fn read(r: &mut Reader<'_>) -> Result<AiEscortPackage> {
        let location = r.f32s::<3>()?;
        let duration = r.u16()?;
        let target = r.fixed_string(32)?;
        let reset = r.u8()?;
        r.skip(1)?;
        Ok(AiEscortPackage { location, duration, target, reset, cell: String::new() })
    }

    /// The `AI_E` body, without the `CNDT`.
    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.f32s(&self.location);
        w.u16(self.duration);
        w.fixed_string(&self.target, 32)?;
        w.u8(self.reset);
        w.bytes(&[0; 1]);
        Ok(())
    }
}

impl AiFollowPackage {
    pub const SIZE: usize = 48;

    /// The `AI_F` body. `cell` comes from the `CNDT` after it and is left empty here.
    pub fn read(r: &mut Reader<'_>) -> Result<AiFollowPackage> {
        let location = r.f32s::<3>()?;
        let duration = r.u16()?;
        let target = r.fixed_string(32)?;
        let reset = r.u8()?;
        r.skip(1)?;
        Ok(AiFollowPackage { location, duration, target, reset, cell: String::new() })
    }

    /// The `AI_F` body, without the `CNDT`.
    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.f32s(&self.location);
        w.u16(self.duration);
        w.fixed_string(&self.target, 32)?;
        w.u8(self.reset);
        w.bytes(&[0; 1]);
        Ok(())
    }
}

impl AiActivatePackage {
    pub const SIZE: usize = 33;

    pub fn read(r: &mut Reader<'_>) -> Result<AiActivatePackage> {
        Ok(AiActivatePackage { target: r.fixed_string(32)?, reset: r.u8()? })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.fixed_string(&self.target, 32)?;
        w.u8(self.reset);
        Ok(())
    }
}

impl TravelDestination {
    pub const SIZE: usize = 24;

    /// Reads the destination from its `DODT` subrecord and the `DNAM` that may come next,
    /// which the cursor is positioned just past the `DODT` to find.
    pub fn read(rec: &Record, dodt: &Sub, it: &mut Subs<'_>) -> Result<TravelDestination> {
        let mut r = sized(rec, dodt, Self::SIZE)?;
        let translation = r.f32s::<3>()?;
        let rotation = r.f32s::<3>()?;
        let cell = text_after(it, b"DNAM");
        Ok(TravelDestination { translation, rotation, cell })
    }

    /// Writes `DODT`, then `DNAM` when the cell is not empty.
    pub fn write(&self, out: &mut Out) -> Result<()> {
        out.sub(b"DODT", |w| {
            w.f32s(&self.translation);
            w.f32s(&self.rotation);
            Ok(())
        })?;
        out.text_if(b"DNAM", &self.cell)
    }
}

impl AiPackage {
    /// Whether a tag is one of the five package subrecords `read` takes.
    pub fn is_tag(tag: &Tag) -> bool {
        matches!(tag, b"AI_T" | b"AI_W" | b"AI_E" | b"AI_F" | b"AI_A")
    }

    /// Reads a package from its `AI_*` subrecord and, for an escort or follow package,
    /// the `CNDT` that may come next, which the cursor is positioned just past the `AI_*`
    /// to find. `tes3` leaves the cell empty when no `CNDT` follows, and so does this.
    pub fn read(rec: &Record, s: &Sub, it: &mut Subs<'_>) -> Result<AiPackage> {
        Ok(match &s.tag {
            b"AI_T" => AiPackage::Travel(AiTravelPackage::read(&mut sized(rec, s, AiTravelPackage::SIZE)?)?),
            b"AI_W" => AiPackage::Wander(AiWanderPackage::read(&mut sized(rec, s, AiWanderPackage::SIZE)?)?),
            b"AI_E" => {
                let mut p = AiEscortPackage::read(&mut sized(rec, s, AiEscortPackage::SIZE)?)?;
                p.cell = text_after(it, b"CNDT");
                AiPackage::Escort(p)
            }
            b"AI_F" => {
                let mut p = AiFollowPackage::read(&mut sized(rec, s, AiFollowPackage::SIZE)?)?;
                p.cell = text_after(it, b"CNDT");
                AiPackage::Follow(p)
            }
            b"AI_A" => {
                AiPackage::Activate(AiActivatePackage::read(&mut sized(rec, s, AiActivatePackage::SIZE)?)?)
            }
            _ => return Err(unexpected(rec, s)),
        })
    }

    /// Writes the package's subrecord, then `CNDT` for an escort or follow package whose
    /// cell is not empty.
    pub fn write(&self, out: &mut Out) -> Result<()> {
        match self {
            AiPackage::Travel(p) => out.sub(b"AI_T", |w| {
                p.write(w);
                Ok(())
            }),
            AiPackage::Wander(p) => out.sub(b"AI_W", |w| {
                p.write(w);
                Ok(())
            }),
            AiPackage::Escort(p) => {
                out.sub(b"AI_E", |w| p.write(w))?;
                out.text_if(b"CNDT", &p.cell)
            }
            AiPackage::Follow(p) => {
                out.sub(b"AI_F", |w| p.write(w))?;
                out.text_if(b"CNDT", &p.cell)
            }
            AiPackage::Activate(p) => out.sub(b"AI_A", |w| p.write(w)),
        }
    }
}

/// The next subrecord's text when it carries `tag`, taking it, or an empty string.
fn text_after(it: &mut Subs<'_>, tag: &Tag) -> String {
    if it.peek_tag() == Some(tag) {
        if let Some(s) = it.next() {
            return text(s);
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_and_destinations_take_their_cells_and_leave_the_rest() {
        let mut rec = Record::new(b"NPC_");
        let mut w = Writer::new();
        w.f32s(&[1.0, 2.0, 3.0]);
        w.f32s(&[0.0, 0.0, 1.5]);
        rec.push(b"DODT", w.into_bytes());
        rec.push(b"DNAM", b"Balmora, Guild of Mages\0".to_vec());
        let mut w = Writer::new();
        w.f32s(&[4.0, 5.0, 6.0]);
        w.f32s(&[0.0, 0.0, 0.0]);
        rec.push(b"DODT", w.into_bytes());
        let mut w = Writer::new();
        AiWanderPackage {
            distance: 512,
            duration: 5,
            game_hour: 0,
            idle2: 60,
            reset: 1,
            ..Default::default()
        }
        .write(&mut w);
        rec.push(b"AI_W", w.into_bytes());
        let mut w = Writer::new();
        let escort = AiEscortPackage {
            location: [1.0, 1.0, 1.0],
            duration: 10,
            target: "player".into(),
            reset: 1,
            cell: String::new(),
        };
        escort.write(&mut w).unwrap();
        assert_eq!(w.len(), AiEscortPackage::SIZE);
        rec.push(b"AI_E", w.into_bytes());
        rec.push(b"CNDT", b"Vivec\0".to_vec());
        let mut w = Writer::new();
        AiActivatePackage { target: "lever_01".into(), reset: 0 }.write(&mut w).unwrap();
        rec.push(b"AI_A", w.into_bytes());
        rec.push(b"NAME", b"x\0".to_vec());

        let mut it = Subs::new(&rec);
        let s = it.next().unwrap();
        let d1 = TravelDestination::read(&rec, s, &mut it).unwrap();
        assert_eq!(d1.cell, "Balmora, Guild of Mages");
        assert_eq!(d1.rotation, [0.0, 0.0, 1.5]);
        let s = it.next().unwrap();
        let d2 = TravelDestination::read(&rec, s, &mut it).unwrap();
        assert!(d2.cell.is_empty());
        assert_eq!(it.peek_tag(), Some(b"AI_W"));

        let mut packages = Vec::new();
        while let Some(s) = it.next() {
            if !AiPackage::is_tag(&s.tag) {
                it.back();
                break;
            }
            packages.push(AiPackage::read(&rec, s, &mut it).unwrap());
        }
        assert_eq!(packages.len(), 3);
        assert!(matches!(&packages[0], AiPackage::Wander(p) if p.distance == 512 && p.idle2 == 60));
        assert!(matches!(&packages[1], AiPackage::Escort(p) if p.target == "player" && p.cell == "Vivec"));
        assert!(matches!(&packages[2], AiPackage::Activate(p) if p.target == "lever_01"));
        assert_eq!(it.peek_tag(), Some(b"NAME"));

        let mut out = Out::new(b"NPC_", 0);
        d1.write(&mut out).unwrap();
        d2.write(&mut out).unwrap();
        for p in &packages {
            p.write(&mut out).unwrap();
        }
        let back = out.finish();
        assert_eq!(back.subs, rec.subs[..7].to_vec());
    }

    #[test]
    fn a_short_package_is_refused_by_name() {
        let mut rec = Record::new(b"CREA");
        rec.push(b"AI_T", vec![0; 12]);
        let mut it = Subs::new(&rec);
        let s = it.next().unwrap();
        let e = AiPackage::read(&rec, s, &mut it).unwrap_err();
        assert!(e.what.contains("CREA::AI_T is 12 bytes"), "{e}");

        let mut rec = Record::new(b"CREA");
        rec.push(b"DODT", vec![0; 25]);
        let mut it = Subs::new(&rec);
        let s = it.next().unwrap();
        let e = TravelDestination::read(&rec, s, &mut it).unwrap_err();
        assert!(e.what.contains("CREA::DODT is 25 bytes"), "{e}");
    }
}
