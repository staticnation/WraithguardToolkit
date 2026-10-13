//! `APPA`: an alchemy apparatus. `tes3` apparatus.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Apparatus {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub data: ApparatusData,
}

/// `AADT`, 16 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApparatusData {
    /// `consts::apparatus_type::*`.
    pub apparatus_type: u32,
    pub quality: f32,
    pub weight: f32,
    pub value: u32,
}

impl ApparatusData {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<ApparatusData> {
        Ok(ApparatusData { apparatus_type: r.u32()?, quality: r.f32()?, weight: r.f32()?, value: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.apparatus_type);
        w.f32(self.quality);
        w.f32(self.weight);
        w.u32(self.value);
    }
}

impl Typed for Apparatus {
    const TAG: Tag = *b"APPA";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"SCRI" => this.script = text(s),
                b"AADT" => this.data = ApparatusData::read(&mut sized(rec, s, ApparatusData::SIZE)?)?,
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
        out.text_if(b"SCRI", &self.script)?;
        out.sub(b"AADT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"ITEX", &self.icon)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apparatus_round_trips_and_a_short_aadt_is_refused() {
        let mut rec = Record::new(b"APPA");
        rec.push(b"NAME", b"apparatus_a_mortar_01\0".to_vec());
        rec.push(b"MODL", b"m\\apparatus_a_mortar_01.nif\0".to_vec());
        rec.push(b"FNAM", b"Apprentice's Mortar and Pestle\0".to_vec());
        let data = ApparatusData {
            apparatus_type: consts::apparatus_type::MORTAR_AND_PESTLE,
            quality: 0.5,
            weight: 1.0,
            value: 10,
        };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"AADT", w.into_bytes());
        rec.push(b"ITEX", b"m\\tx_mortar_01.dds\0".to_vec());
        let a = Apparatus::from_raw(&rec).unwrap();
        assert_eq!(a.data, data);
        assert_eq!(a.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(12);
        let e = Apparatus::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("APPA::AADT is 12 bytes"), "{e}");
    }
}
