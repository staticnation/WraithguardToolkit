//! `SOUN`: a sound. `tes3` sound.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sound {
    pub flags: u32,
    pub id: String,
    pub sound_path: String,
    pub data: SoundData,
}

/// `DATA`, 3 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundData {
    pub volume: u8,
    /// Minimum and maximum range.
    pub range: (u8, u8),
}

impl SoundData {
    pub const SIZE: usize = 3;

    pub fn read(r: &mut Reader<'_>) -> Result<SoundData> {
        Ok(SoundData { volume: r.u8()?, range: (r.u8()?, r.u8()?) })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u8(self.volume);
        w.u8(self.range.0);
        w.u8(self.range.1);
    }
}

impl Typed for Sound {
    const TAG: Tag = *b"SOUN";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.sound_path = text(s),
                b"DATA" => this.data = SoundData::read(&mut sized(rec, s, SoundData::SIZE)?)?,
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.text_if(b"FNAM", &self.sound_path)?;
        out.sub(b"DATA", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_round_trips_and_a_long_data_is_refused() {
        let mut rec = Record::new(b"SOUN");
        rec.push(b"NAME", b"Rain\0".to_vec());
        rec.push(b"FNAM", b"Fx\\envrn\\rain.wav\0".to_vec());
        rec.push(b"DATA", vec![255, 0, 30]);
        let s = Sound::from_raw(&rec).unwrap();
        assert_eq!(s.data, SoundData { volume: 255, range: (0, 30) });
        assert_eq!(s.to_raw().unwrap(), rec);

        rec.subs[2].data.to_mut().push(0);
        let e = Sound::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("SOUN::DATA is 4 bytes"), "{e}");
    }
}
