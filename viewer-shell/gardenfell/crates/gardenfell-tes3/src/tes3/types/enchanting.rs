//! `ENCH`: an enchantment, with its effects. `tes3` enchanting.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Enchanting {
    pub flags: u32,
    pub id: String,
    pub effects: Vec<Effect>,
    pub data: EnchantingData,
}

/// `ENDT`, 16 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnchantingData {
    /// `consts::enchant_type::*`.
    pub enchant_type: u32,
    pub cost: u32,
    pub max_charge: u32,
    /// `consts::enchanting::*`.
    pub flags: u32,
}

impl EnchantingData {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<EnchantingData> {
        Ok(EnchantingData { enchant_type: r.u32()?, cost: r.u32()?, max_charge: r.u32()?, flags: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.enchant_type);
        w.u32(self.cost);
        w.u32(self.max_charge);
        w.u32(self.flags);
    }
}

impl Typed for Enchanting {
    const TAG: Tag = *b"ENCH";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"ENDT" => this.data = EnchantingData::read(&mut sized(rec, s, EnchantingData::SIZE)?)?,
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
        out.sub(b"ENDT", |w| {
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
    fn an_enchantment_round_trips_and_a_short_endt_is_refused() {
        let mut rec = Record::new(b"ENCH");
        rec.push(b"NAME", b"fire_en\0".to_vec());
        let mut w = Writer::new();
        EnchantingData {
            enchant_type: consts::enchant_type::CAST_ON_STRIKE,
            cost: 20,
            max_charge: 200,
            flags: consts::enchanting::AUTO_CALCULATE,
        }
        .write(&mut w);
        rec.push(b"ENDT", w.into_bytes());
        let e1 = Effect {
            magic_effect: consts::effect::FIRE_DAMAGE as i16,
            skill: -1,
            attribute: -1,
            range: consts::effect_range::ON_TOUCH,
            area: 0,
            duration: 1,
            min_magnitude: 5,
            max_magnitude: 10,
        };
        let mut w = Writer::new();
        e1.write(&mut w);
        rec.push(b"ENAM", w.into_bytes());
        let en = Enchanting::from_raw(&rec).unwrap();
        assert_eq!(en.data.max_charge, 200);
        assert_eq!(en.effects, vec![e1]);
        assert_eq!(en.to_raw().unwrap(), rec);

        rec.subs[1].data.to_mut().truncate(12);
        let e = Enchanting::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("ENCH::ENDT is 12 bytes"), "{e}");
    }
}
