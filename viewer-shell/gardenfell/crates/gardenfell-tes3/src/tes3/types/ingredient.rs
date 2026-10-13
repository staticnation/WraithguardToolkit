//! `INGR`: an alchemy ingredient and its four effects. `tes3` ingredient.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ingredient {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub data: IngredientData,
}

/// `IRDT`, 56 bytes: weight and value, then the four effects as three parallel arrays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IngredientData {
    pub weight: f32,
    pub value: u32,
    /// `consts::effect::*`, `NONE` for an empty slot.
    pub effects: [i32; 4],
    /// `consts::skill::*`, one per effect.
    pub skills: [i32; 4],
    /// `consts::attribute::*`, one per effect.
    pub attributes: [i32; 4],
}

impl IngredientData {
    pub const SIZE: usize = 56;

    pub fn read(r: &mut Reader<'_>) -> Result<IngredientData> {
        Ok(IngredientData {
            weight: r.f32()?,
            value: r.u32()?,
            effects: r.i32s::<4>()?,
            skills: r.i32s::<4>()?,
            attributes: r.i32s::<4>()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.i32s(&self.effects);
        w.i32s(&self.skills);
        w.i32s(&self.attributes);
    }
}

impl Typed for Ingredient {
    const TAG: Tag = *b"INGR";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"IRDT" => this.data = IngredientData::read(&mut sized(rec, s, IngredientData::SIZE)?)?,
                b"SCRI" => this.script = text(s),
                b"ITEX" => this.icon = text(s),
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
        out.text_if(b"FNAM", &self.name)?;
        out.sub(b"IRDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"SCRI", &self.script)?;
        out.text_if(b"ITEX", &self.icon)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ingredient_round_trips_and_a_short_irdt_is_refused() {
        let mut rec = Record::new(b"INGR");
        rec.push(b"NAME", b"ingred_bread_01\0".to_vec());
        rec.push(b"MODL", b"n\\ingred_bread_01.nif\0".to_vec());
        rec.push(b"FNAM", b"Bread\0".to_vec());
        let mut effects = [consts::effect::NONE; 4];
        effects[0] = consts::effect::RESTORE_FATIGUE;
        let data = IngredientData {
            weight: 0.2,
            value: 1,
            effects,
            skills: [consts::skill::NONE; 4],
            attributes: [consts::attribute::NONE; 4],
        };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"IRDT", w.into_bytes());
        rec.push(b"ITEX", b"n\\tx_bread_01.dds\0".to_vec());
        let i = Ingredient::from_raw(&rec).unwrap();
        assert_eq!(i.data, data);
        assert_eq!(i.data.effects[0], consts::effect::RESTORE_FATIGUE);
        assert_eq!(i.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(52);
        let e = Ingredient::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("INGR::IRDT is 52 bytes"), "{e}");
    }
}
