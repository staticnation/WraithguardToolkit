//! `ARMO`: a piece of armor and the body slots it covers. `tes3` armor.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Armor {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub enchanting: String,
    pub biped_objects: Vec<BipedObject>,
    pub data: ArmorData,
}

/// `AODT`, 24 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArmorData {
    /// `consts::armor_type::*`.
    pub armor_type: u32,
    pub weight: f32,
    pub value: u32,
    pub health: u32,
    pub enchantment: u32,
    pub armor_rating: u32,
}

impl ArmorData {
    pub const SIZE: usize = 24;

    pub fn read(r: &mut Reader<'_>) -> Result<ArmorData> {
        Ok(ArmorData {
            armor_type: r.u32()?,
            weight: r.f32()?,
            value: r.u32()?,
            health: r.u32()?,
            enchantment: r.u32()?,
            armor_rating: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.armor_type);
        w.f32(self.weight);
        w.u32(self.value);
        w.u32(self.health);
        w.u32(self.enchantment);
        w.u32(self.armor_rating);
    }
}

impl Typed for Armor {
    const TAG: Tag = *b"ARMO";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"SCRI" => this.script = text(s),
                b"AODT" => this.data = ArmorData::read(&mut sized(rec, s, ArmorData::SIZE)?)?,
                b"ITEX" => this.icon = text(s),
                b"INDX" => this.biped_objects.push(BipedObject::read(rec, s, &mut it)?),
                b"ENAM" => this.enchanting = text(s),
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
        out.text_if(b"SCRI", &self.script)?;
        out.sub(b"AODT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"ITEX", &self.icon)?;
        for b in &self.biped_objects {
            b.write(&mut out)?;
        }
        out.text_if(b"ENAM", &self.enchanting)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_armor_round_trips_with_its_slots_and_a_stray_bnam_is_refused() {
        let mut rec = Record::new(b"ARMO");
        rec.push(b"NAME", b"iron_cuirass\0".to_vec());
        rec.push(b"MODL", b"a\\a_iron_cuirass.nif\0".to_vec());
        rec.push(b"FNAM", b"Iron Cuirass\0".to_vec());
        let data = ArmorData {
            armor_type: consts::armor_type::CUIRASS,
            weight: 30.0,
            value: 70,
            health: 400,
            enchantment: 2,
            armor_rating: 15,
        };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"AODT", w.into_bytes());
        rec.push(b"ITEX", b"a\\tx_iron_cuirass.dds\0".to_vec());
        rec.push(b"INDX", vec![consts::biped_object_type::CHEST]);
        rec.push(b"BNAM", b"a_iron_cuirass\0".to_vec());
        rec.push(b"INDX", vec![consts::biped_object_type::GROIN]);
        rec.push(b"BNAM", b"a_iron_groin\0".to_vec());
        rec.push(b"CNAM", b"a_iron_groin_f\0".to_vec());
        rec.push(b"INDX", vec![consts::biped_object_type::TAIL]);
        rec.push(b"ENAM", b"shield_en\0".to_vec());
        let a = Armor::from_raw(&rec).unwrap();
        assert_eq!(a.data, data);
        assert_eq!(a.biped_objects.len(), 3);
        assert_eq!(a.biped_objects[0].male_bodypart, "a_iron_cuirass");
        assert_eq!(a.biped_objects[1].female_bodypart, "a_iron_groin_f");
        assert_eq!(a.biped_objects[2].biped_object_type, consts::biped_object_type::TAIL);
        assert_eq!(a.enchanting, "shield_en");
        assert_eq!(a.to_raw().unwrap(), rec);

        let mut bad = Record::new(b"ARMO");
        bad.push(b"NAME", b"x\0".to_vec());
        bad.push(b"BNAM", b"no slot\0".to_vec());
        let e = Armor::from_raw(&bad).unwrap_err();
        assert!(e.what.contains("ARMO::BNAM"), "{e}");
    }
}
