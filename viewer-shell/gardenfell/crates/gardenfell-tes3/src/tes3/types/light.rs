//! `LIGH`: a light. `tes3` light.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Light {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub sound: String,
    pub data: LightData,
}

/// `LHDT`, 24 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightData {
    pub weight: f32,
    pub value: u32,
    pub time: i32,
    pub radius: u32,
    /// RGBA.
    pub color: [u8; 4],
    /// `consts::light::*`.
    pub flags: u32,
}

impl LightData {
    pub const SIZE: usize = 24;

    pub fn read(r: &mut Reader<'_>) -> Result<LightData> {
        Ok(LightData {
            weight: r.f32()?,
            value: r.u32()?,
            time: r.i32()?,
            radius: r.u32()?,
            color: r.array::<4>()?,
            flags: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.i32(self.time);
        w.u32(self.radius);
        w.bytes(&self.color);
        w.u32(self.flags);
    }
}

impl Typed for Light {
    const TAG: Tag = *b"LIGH";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"ITEX" => this.icon = text(s),
                b"LHDT" => this.data = LightData::read(&mut sized(rec, s, LightData::SIZE)?)?,
                b"SCRI" => this.script = text(s),
                b"SNAM" => this.sound = text(s),
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
        out.text_if(b"ITEX", &self.icon)?;
        out.sub(b"LHDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"SCRI", &self.script)?;
        out.text_if(b"SNAM", &self.sound)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_light_round_trips_and_a_short_lhdt_is_refused() {
        let mut rec = Record::new(b"LIGH");
        rec.push(b"NAME", b"torch\0".to_vec());
        rec.push(b"MODL", b"l\\torch.nif\0".to_vec());
        rec.push(b"FNAM", b"Torch\0".to_vec());
        let mut w = Writer::new();
        LightData { weight: 1.5, value: 10, time: -1, radius: 256, color: [255, 200, 100, 0], flags: 0x13 }
            .write(&mut w);
        rec.push(b"LHDT", w.into_bytes());
        rec.push(b"SNAM", b"fire\0".to_vec());
        let l = Light::from_raw(&rec).unwrap();
        assert_eq!(l.data.radius, 256);
        assert_eq!(l.data.flags & consts::light::FIRE, consts::light::FIRE);
        assert_eq!(l.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(20);
        let e = Light::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("LIGH::LHDT is 20 bytes"), "{e}");
    }
}
