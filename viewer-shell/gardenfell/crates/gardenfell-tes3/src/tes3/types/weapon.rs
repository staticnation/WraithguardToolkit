//! `WEAP`: a weapon. `tes3` weapon.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weapon {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub enchanting: String,
    pub data: WeaponData,
}

/// `WPDT`, 32 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponData {
    pub weight: f32,
    pub value: u32,
    /// `consts::weapon_type::*`.
    pub weapon_type: u16,
    pub health: u16,
    pub speed: f32,
    pub reach: f32,
    pub enchantment: u16,
    pub chop_min: u8,
    pub chop_max: u8,
    pub slash_min: u8,
    pub slash_max: u8,
    pub thrust_min: u8,
    pub thrust_max: u8,
    /// `consts::weapon::*`.
    pub flags: u32,
}

impl WeaponData {
    pub const SIZE: usize = 32;

    pub fn read(r: &mut Reader<'_>) -> Result<WeaponData> {
        Ok(WeaponData {
            weight: r.f32()?,
            value: r.u32()?,
            weapon_type: r.u16()?,
            health: r.u16()?,
            speed: r.f32()?,
            reach: r.f32()?,
            enchantment: r.u16()?,
            chop_min: r.u8()?,
            chop_max: r.u8()?,
            slash_min: r.u8()?,
            slash_max: r.u8()?,
            thrust_min: r.u8()?,
            thrust_max: r.u8()?,
            flags: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.u16(self.weapon_type);
        w.u16(self.health);
        w.f32(self.speed);
        w.f32(self.reach);
        w.u16(self.enchantment);
        w.u8(self.chop_min);
        w.u8(self.chop_max);
        w.u8(self.slash_min);
        w.u8(self.slash_max);
        w.u8(self.thrust_min);
        w.u8(self.thrust_max);
        w.u32(self.flags);
    }
}

impl Typed for Weapon {
    const TAG: Tag = *b"WEAP";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"WPDT" => this.data = WeaponData::read(&mut sized(rec, s, WeaponData::SIZE)?)?,
                b"SCRI" => this.script = text(s),
                b"ITEX" => this.icon = text(s),
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
        out.sub(b"WPDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"SCRI", &self.script)?;
        out.text_if(b"ITEX", &self.icon)?;
        out.text_if(b"ENAM", &self.enchanting)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_round_trips_and_a_short_wpdt_is_refused() {
        let mut rec = Record::new(b"WEAP");
        rec.push(b"NAME", b"iron dagger\0".to_vec());
        rec.push(b"MODL", b"w\\w_dagger_iron.nif\0".to_vec());
        rec.push(b"FNAM", b"Iron Dagger\0".to_vec());
        let data = WeaponData {
            weight: 3.0,
            value: 10,
            weapon_type: consts::weapon_type::SHORT_BLADE_ONE_HAND,
            health: 300,
            speed: 1.4,
            reach: 1.0,
            enchantment: 5,
            chop_min: 2,
            chop_max: 5,
            slash_min: 2,
            slash_max: 5,
            thrust_min: 3,
            thrust_max: 7,
            flags: consts::weapon::SILVER,
        };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"WPDT", w.into_bytes());
        rec.push(b"ITEX", b"w\\tx_dagger_iron.dds\0".to_vec());
        rec.push(b"ENAM", b"fire_en\0".to_vec());
        let wp = Weapon::from_raw(&rec).unwrap();
        assert_eq!(wp.data, data);
        assert_eq!(wp.enchanting, "fire_en");
        assert_eq!(wp.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(31);
        let e = Weapon::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("WEAP::WPDT is 31 bytes"), "{e}");
    }
}
