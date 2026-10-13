//! `CLOT`: a piece of clothing and the body slots it covers. `tes3` clothing.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clothing {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub enchanting: String,
    pub biped_objects: Vec<BipedObject>,
    pub data: ClothingData,
}

/// `CTDT`, 12 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClothingData {
    /// `consts::clothing_type::*`.
    pub clothing_type: u32,
    pub weight: f32,
    pub value: u16,
    pub enchantment: u16,
}

impl ClothingData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<ClothingData> {
        Ok(ClothingData { clothing_type: r.u32()?, weight: r.f32()?, value: r.u16()?, enchantment: r.u16()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.clothing_type);
        w.f32(self.weight);
        w.u16(self.value);
        w.u16(self.enchantment);
    }
}

impl Typed for Clothing {
    const TAG: Tag = *b"CLOT";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"CTDT" => this.data = ClothingData::read(&mut sized(rec, s, ClothingData::SIZE)?)?,
                b"SCRI" => this.script = text(s),
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
        out.sub(b"CTDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"SCRI", &self.script)?;
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
    fn a_clothing_round_trips_with_its_slots_and_a_short_ctdt_is_refused() {
        let mut rec = Record::new(b"CLOT");
        rec.push(b"NAME", b"common_shirt_01\0".to_vec());
        rec.push(b"MODL", b"c\\c_m_shirt_common_1.nif\0".to_vec());
        rec.push(b"FNAM", b"Common Shirt\0".to_vec());
        let data = ClothingData {
            clothing_type: consts::clothing_type::SHIRT,
            weight: 2.0,
            value: 3,
            enchantment: 10,
        };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"CTDT", w.into_bytes());
        rec.push(b"SCRI", b"shirt_script\0".to_vec());
        rec.push(b"ITEX", b"c\\tx_shirt_common_1.dds\0".to_vec());
        rec.push(b"INDX", vec![consts::biped_object_type::CHEST]);
        rec.push(b"BNAM", b"c_m_shirt_common_1_c\0".to_vec());
        rec.push(b"CNAM", b"c_f_shirt_common_1_c\0".to_vec());
        rec.push(b"INDX", vec![consts::biped_object_type::RIGHT_UPPER_ARM]);
        rec.push(b"BNAM", b"c_m_shirt_common_1_ua\0".to_vec());
        let c = Clothing::from_raw(&rec).unwrap();
        assert_eq!(c.data, data);
        assert_eq!(c.biped_objects.len(), 2);
        assert_eq!(c.biped_objects[0].female_bodypart, "c_f_shirt_common_1_c");
        assert_eq!(c.biped_objects[1].biped_object_type, consts::biped_object_type::RIGHT_UPPER_ARM);
        assert!(c.biped_objects[1].female_bodypart.is_empty());
        assert_eq!(c.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(10);
        let e = Clothing::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CLOT::CTDT is 10 bytes"), "{e}");
    }
}
