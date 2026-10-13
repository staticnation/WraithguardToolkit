//! `ALCH`: a potion, with its effects. `tes3` alchemy.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Alchemy {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    /// `TEXT`: a potion's icon travels under this tag, not `ITEX`.
    pub icon: String,
    pub effects: Vec<Effect>,
    pub data: AlchemyData,
}

/// `ALDT`, 12 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlchemyData {
    pub weight: f32,
    pub value: u32,
    /// `consts::alchemy::*`.
    pub flags: u32,
}

impl AlchemyData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<AlchemyData> {
        Ok(AlchemyData { weight: r.f32()?, value: r.u32()?, flags: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.u32(self.flags);
    }
}

impl Typed for Alchemy {
    const TAG: Tag = *b"ALCH";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"TEXT" => this.icon = text(s),
                b"SCRI" => this.script = text(s),
                b"FNAM" => this.name = text(s),
                b"ALDT" => this.data = AlchemyData::read(&mut sized(rec, s, AlchemyData::SIZE)?)?,
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
        out.text_if(b"MODL", &self.mesh)?;
        out.text_if(b"TEXT", &self.icon)?;
        out.text_if(b"SCRI", &self.script)?;
        out.text_if(b"FNAM", &self.name)?;
        out.sub(b"ALDT", |w| {
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
    fn a_potion_round_trips_with_its_text_icon_and_a_short_aldt_is_refused() {
        let mut rec = Record::new(b"ALCH");
        rec.push(b"NAME", b"p_restore_health_s\0".to_vec());
        rec.push(b"MODL", b"m\\misc_potion_cheap_01.nif\0".to_vec());
        rec.push(b"TEXT", b"m\\tx_potion_cheap_01.dds\0".to_vec());
        rec.push(b"FNAM", b"Cheap Potion of Healing\0".to_vec());
        let mut w = Writer::new();
        AlchemyData { weight: 1.0, value: 15, flags: consts::alchemy::AUTO_CALCULATE }.write(&mut w);
        rec.push(b"ALDT", w.into_bytes());
        let e1 = Effect {
            magic_effect: consts::effect::RESTORE_HEALTH as i16,
            skill: -1,
            attribute: -1,
            range: consts::effect_range::ON_SELF,
            area: 0,
            duration: 5,
            min_magnitude: 1,
            max_magnitude: 1,
        };
        let mut w = Writer::new();
        e1.write(&mut w);
        rec.push(b"ENAM", w.into_bytes());
        let a = Alchemy::from_raw(&rec).unwrap();
        assert_eq!(a.icon, "m\\tx_potion_cheap_01.dds");
        assert_eq!(a.effects, vec![e1]);
        assert_eq!(a.to_raw().unwrap(), rec);

        rec.subs[4].data.to_mut().truncate(8);
        let e = Alchemy::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("ALCH::ALDT is 8 bytes"), "{e}");
    }
}
