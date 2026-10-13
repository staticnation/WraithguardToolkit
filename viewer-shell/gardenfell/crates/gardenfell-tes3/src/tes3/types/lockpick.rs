//! `LOCK`: a lockpick. `tes3` lockpick.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lockpick {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub data: LockpickData,
}

/// `LKDT`, 16 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LockpickData {
    pub weight: f32,
    pub value: u32,
    pub quality: f32,
    pub uses: u32,
}

impl LockpickData {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<LockpickData> {
        Ok(LockpickData { weight: r.f32()?, value: r.u32()?, quality: r.f32()?, uses: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.f32(self.quality);
        w.u32(self.uses);
    }
}

impl Typed for Lockpick {
    const TAG: Tag = *b"LOCK";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"LKDT" => this.data = LockpickData::read(&mut sized(rec, s, LockpickData::SIZE)?)?,
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
        out.sub(b"LKDT", |w| {
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
    fn a_lockpick_round_trips_and_a_short_lkdt_is_refused() {
        let mut rec = Record::new(b"LOCK");
        rec.push(b"NAME", b"pick_apprentice_01\0".to_vec());
        rec.push(b"MODL", b"m\\pick_apprentice_01.nif\0".to_vec());
        rec.push(b"FNAM", b"Apprentice's Lockpick\0".to_vec());
        let data = LockpickData { weight: 0.25, value: 15, quality: 1.0, uses: 25 };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"LKDT", w.into_bytes());
        rec.push(b"ITEX", b"m\\tx_pick_apprentice.dds\0".to_vec());
        let l = Lockpick::from_raw(&rec).unwrap();
        assert_eq!(l.data, data);
        assert_eq!(l.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(12);
        let e = Lockpick::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("LOCK::LKDT is 12 bytes"), "{e}");
    }
}
