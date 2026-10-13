//! `SPEL`: a spell, with its effects. `tes3` spell.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spell {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub effects: Vec<Effect>,
    pub data: SpellData,
}

/// `SPDT`, 12 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellData {
    /// `consts::spell_type::*`.
    pub spell_type: u32,
    pub cost: u32,
    /// `consts::spell::*`.
    pub flags: u32,
}

impl SpellData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<SpellData> {
        Ok(SpellData { spell_type: r.u32()?, cost: r.u32()?, flags: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.spell_type);
        w.u32(self.cost);
        w.u32(self.flags);
    }
}

impl Typed for Spell {
    const TAG: Tag = *b"SPEL";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.name = text(s),
                b"SPDT" => this.data = SpellData::read(&mut sized(rec, s, SpellData::SIZE)?)?,
                b"ENAM" => this.effects.push(Effect::read(&mut sized(rec, s, Effect::SIZE)?)?),
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
        out.sub(b"SPDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        for e in &self.effects {
            out.sub(b"ENAM", |w| {
                e.write(w);
                Ok(())
            })?;
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spell_round_trips_and_a_short_enam_is_refused() {
        let mut rec = Record::new(b"SPEL");
        rec.push(b"NAME", b"fire bite\0".to_vec());
        rec.push(b"FNAM", b"Fire Bite\0".to_vec());
        let mut w = Writer::new();
        let data = SpellData {
            spell_type: consts::spell_type::SPELL,
            cost: 5,
            flags: consts::spell::AUTO_CALCULATE,
        };
        data.write(&mut w);
        rec.push(b"SPDT", w.into_bytes());
        let e1 = Effect {
            magic_effect: consts::effect::FIRE_DAMAGE as i16,
            skill: -1,
            attribute: -1,
            range: consts::effect_range::ON_TOUCH,
            area: 0,
            duration: 1,
            min_magnitude: 15,
            max_magnitude: 30,
        };
        let e2 = Effect { magic_effect: consts::effect::FROST_DAMAGE as i16, duration: 2, ..e1.clone() };
        for e in [&e1, &e2] {
            let mut w = Writer::new();
            e.write(&mut w);
            rec.push(b"ENAM", w.into_bytes());
        }
        let sp = Spell::from_raw(&rec).unwrap();
        assert_eq!(sp.effects, vec![e1, e2]);
        assert_eq!(sp.data.cost, 5);
        assert_eq!(sp.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(23);
        let e = Spell::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("SPEL::ENAM is 23 bytes"), "{e}");
    }
}
