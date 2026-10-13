//! `LEVC`: a leveled creature list. `tes3` leveledcreature.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeveledCreature {
    pub flags: u32,
    pub id: String,
    /// `DATA`: `consts::leveled_creature::*`.
    pub leveled_creature_flags: u32,
    /// `NNAM`: the percent chance the list yields nothing.
    pub chance_none: u8,
    /// One `CNAM` (the creature id) and `INTV` (the player level it needs) per entry, after
    /// an `INDX` holding the count. The count is written back as the list's length, as
    /// `tes3` does, so a file whose count disagrees with its entries comes back corrected.
    pub creatures: Vec<(String, u16)>,
}

impl Typed for LeveledCreature {
    const TAG: Tag = *b"LEVC";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"DATA" => this.leveled_creature_flags = u32_of(rec, s)?,
                b"NNAM" => this.chance_none = u8_of(rec, s)?,
                b"INDX" => this.creatures.reserve(u32_of(rec, s)? as usize),
                b"CNAM" => this.creatures.push((text(s), 0)),
                b"INTV" => match this.creatures.last_mut() {
                    Some(creature) => creature.1 = u16_of(rec, s)?,
                    None => return Err(unexpected_after(rec, s, b"CNAM")),
                },
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.u32(b"DATA", self.leveled_creature_flags);
        out.u8(b"NNAM", self.chance_none);
        if !self.creatures.is_empty() {
            out.u32(b"INDX", self.creatures.len() as u32);
            for (creature, level) in &self.creatures {
                out.text(b"CNAM", creature)?;
                out.u16(b"INTV", *level);
            }
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leveled_creature_round_trips_and_an_empty_list_has_no_indx() {
        let mut rec = Record::new(b"LEVC");
        rec.push(b"NAME", b"ex_wilderness_lev+0\0".to_vec());
        rec.push(b"DATA", consts::leveled_creature::CALCULATE_FROM_ALL_LEVELS.to_le_bytes().to_vec());
        rec.push(b"NNAM", vec![40]);
        rec.push(b"INDX", 2u32.to_le_bytes().to_vec());
        rec.push(b"CNAM", b"rat\0".to_vec());
        rec.push(b"INTV", 1u16.to_le_bytes().to_vec());
        rec.push(b"CNAM", b"nix-hound\0".to_vec());
        rec.push(b"INTV", 3u16.to_le_bytes().to_vec());
        let l = LeveledCreature::from_raw(&rec).unwrap();
        assert_eq!(l.leveled_creature_flags, consts::leveled_creature::CALCULATE_FROM_ALL_LEVELS);
        assert_eq!(l.creatures, vec![("rat".to_string(), 1), ("nix-hound".to_string(), 3)]);
        assert_eq!(l.to_raw().unwrap(), rec);

        let empty = LeveledCreature { id: "nothing".into(), ..Default::default() };
        let back = empty.to_raw().unwrap();
        assert_eq!(back.subs.iter().map(|s| s.tag).collect::<Vec<_>>(), vec![*b"NAME", *b"DATA", *b"NNAM"]);
        assert_eq!(LeveledCreature::from_raw(&back).unwrap(), empty);
    }

    #[test]
    fn an_intv_before_any_cnam_is_refused() {
        let mut rec = Record::new(b"LEVC");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"INTV", 1u16.to_le_bytes().to_vec());
        let e = LeveledCreature::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("LEVC::INTV where CNAM was expected"), "{e}");
    }
}
