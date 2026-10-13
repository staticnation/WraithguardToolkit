//! `MGEF`: a magic effect. `tes3` magiceffect.rs.

use super::*;

/// A magic effect record has no `NAME`: `INDX` is its identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MagicEffect {
    pub flags: u32,
    /// `consts::effect::*`: `INDX`, 4 bytes.
    pub effect_id: i32,
    pub icon: String,
    pub texture: String,
    pub bolt_sound: String,
    pub cast_sound: String,
    pub hit_sound: String,
    pub area_sound: String,
    pub cast_visual: String,
    pub bolt_visual: String,
    pub hit_visual: String,
    pub area_visual: String,
    pub description: String,
    pub data: MagicEffectData,
}

/// `MEDT`, 36 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MagicEffectData {
    /// `consts::effect_school::*`.
    pub school: u32,
    pub base_cost: f32,
    /// `consts::magic_effect::*`.
    pub flags: u32,
    /// RGB, each 0 to 255.
    pub color: (i32, i32, i32),
    pub speed: f32,
    pub size: f32,
    pub size_cap: f32,
}

impl MagicEffectData {
    pub const SIZE: usize = 36;

    pub fn read(r: &mut Reader<'_>) -> Result<MagicEffectData> {
        Ok(MagicEffectData {
            school: r.u32()?,
            base_cost: r.f32()?,
            flags: r.u32()?,
            color: (r.i32()?, r.i32()?, r.i32()?),
            speed: r.f32()?,
            size: r.f32()?,
            size_cap: r.f32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.school);
        w.f32(self.base_cost);
        w.u32(self.flags);
        w.i32(self.color.0);
        w.i32(self.color.1);
        w.i32(self.color.2);
        w.f32(self.speed);
        w.f32(self.size);
        w.f32(self.size_cap);
    }
}

impl Typed for MagicEffect {
    const TAG: Tag = *b"MGEF";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"INDX" => this.effect_id = i32_of(rec, s)?,
                b"MEDT" => this.data = MagicEffectData::read(&mut sized(rec, s, MagicEffectData::SIZE)?)?,
                b"ITEX" => this.icon = text(s),
                b"PTEX" => this.texture = text(s),
                b"BSND" => this.bolt_sound = text(s),
                b"CSND" => this.cast_sound = text(s),
                b"HSND" => this.hit_sound = text(s),
                b"ASND" => this.area_sound = text(s),
                b"CVFX" => this.cast_visual = text(s),
                b"BVFX" => this.bolt_visual = text(s),
                b"HVFX" => this.hit_visual = text(s),
                b"AVFX" => this.area_visual = text(s),
                b"DESC" => this.description = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.i32(b"INDX", self.effect_id);
        out.sub(b"MEDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"ITEX", &self.icon)?;
        out.text_if(b"PTEX", &self.texture)?;
        out.text_if(b"BSND", &self.bolt_sound)?;
        out.text_if(b"CSND", &self.cast_sound)?;
        out.text_if(b"HSND", &self.hit_sound)?;
        out.text_if(b"ASND", &self.area_sound)?;
        out.text_if(b"CVFX", &self.cast_visual)?;
        out.text_if(b"BVFX", &self.bolt_visual)?;
        out.text_if(b"HVFX", &self.hit_visual)?;
        out.text_if(b"AVFX", &self.area_visual)?;
        out.text_if(b"DESC", &self.description)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_magic_effect_round_trips_and_a_short_medt_is_refused() {
        let data = MagicEffectData {
            school: consts::effect_school::DESTRUCTION,
            base_cost: 5.0,
            flags: consts::magic_effect::HARMFUL | consts::magic_effect::ALLOW_SPELLMAKING,
            color: (255, 128, 0),
            speed: 1.0,
            size: 1.0,
            size_cap: 50.0,
        };
        let mut rec = Record::new(b"MGEF");
        rec.push(b"INDX", consts::effect::FIRE_DAMAGE.to_le_bytes().to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), MagicEffectData::SIZE);
        rec.push(b"MEDT", w.into_bytes());
        rec.push(b"ITEX", b"s\\tx_s_firedamage.tga\0".to_vec());
        rec.push(b"PTEX", b"vfx_flame01.tga\0".to_vec());
        rec.push(b"CSND", b"Destruction Cast\0".to_vec());
        rec.push(b"BVFX", b"VFX_DestructBolt\0".to_vec());
        rec.push(b"DESC", b"Burns.\0".to_vec());
        let m = MagicEffect::from_raw(&rec).unwrap();
        assert_eq!(m.effect_id, consts::effect::FIRE_DAMAGE);
        assert_eq!(m.data, data);
        assert_eq!(m.bolt_visual, "VFX_DestructBolt");
        assert!(m.bolt_sound.is_empty());
        assert_eq!(m.to_raw().unwrap(), rec);

        rec.subs[1].data.to_mut().truncate(32);
        let e = MagicEffect::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("MGEF::MEDT is 32 bytes"), "{e}");
    }
}
