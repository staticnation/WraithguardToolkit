//! `BSGN`: a birthsign. `tes3` birthsign.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Birthsign {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub texture: String,
    pub description: String,
    /// `NPCS`, one per spell, each written as a 32-byte fixed string.
    pub spells: Vec<String>,
}

impl Typed for Birthsign {
    const TAG: Tag = *b"BSGN";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.name = text(s),
                b"TNAM" => this.texture = text(s),
                b"DESC" => this.description = text(s),
                b"NPCS" => this.spells.push(text(s)),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.text_if(b"FNAM", &self.name)?;
        out.text_if(b"TNAM", &self.texture)?;
        out.text_if(b"DESC", &self.description)?;
        for spell in &self.spells {
            out.sub(b"NPCS", |w| w.fixed_string(spell, 32))?;
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_birthsign_round_trips_and_an_overlong_spell_id_is_refused_on_write() {
        let mut rec = Record::new(b"BSGN");
        rec.push(b"NAME", b"The Warrior\0".to_vec());
        rec.push(b"FNAM", b"The Warrior\0".to_vec());
        rec.push(b"TNAM", b"_land_default.tga\0".to_vec());
        rec.push(b"DESC", b"Those born under the Warrior.\0".to_vec());
        let mut w = Writer::new();
        w.fixed_string("warwyrd", 32).unwrap();
        rec.push(b"NPCS", w.into_bytes());
        let b = Birthsign::from_raw(&rec).unwrap();
        assert_eq!(b.spells, vec!["warwyrd".to_string()]);
        assert_eq!(b.to_raw().unwrap(), rec);

        let long = Birthsign { spells: vec!["x".repeat(33)], ..b };
        assert!(long.to_raw().is_err());

        let mut rec = Record::new(b"BSGN");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"INTV", vec![0; 4]);
        let e = Birthsign::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("BSGN::INTV"), "{e}");
    }
}
