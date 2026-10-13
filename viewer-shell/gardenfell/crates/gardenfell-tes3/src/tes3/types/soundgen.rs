//! `SNDG`: a sound generator, the sound a creature makes for an action. `tes3` soundgen.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundGen {
    pub flags: u32,
    pub id: String,
    /// `consts::soundgen_type::*`: `DATA`, 4 bytes.
    pub sound_gen_type: u32,
    pub creature: String,
    pub sound: String,
}

impl Typed for SoundGen {
    const TAG: Tag = *b"SNDG";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"DATA" => this.sound_gen_type = u32_of(rec, s)?,
                b"CNAM" => this.creature = text(s),
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
        out.u32(b"DATA", self.sound_gen_type);
        out.text_if(b"CNAM", &self.creature)?;
        out.text_if(b"SNAM", &self.sound)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_gen_round_trips_and_a_short_data_is_refused() {
        let mut rec = Record::new(b"SNDG");
        rec.push(b"NAME", b"rat0004\0".to_vec());
        rec.push(b"DATA", consts::soundgen_type::MOAN.to_le_bytes().to_vec());
        rec.push(b"CNAM", b"rat\0".to_vec());
        rec.push(b"SNAM", b"rat moan\0".to_vec());
        let g = SoundGen::from_raw(&rec).unwrap();
        assert_eq!(
            g,
            SoundGen {
                flags: 0,
                id: "rat0004".into(),
                sound_gen_type: consts::soundgen_type::MOAN,
                creature: "rat".into(),
                sound: "rat moan".into(),
            }
        );
        assert_eq!(g.to_raw().unwrap(), rec);

        rec.subs[1].data.to_mut().truncate(2);
        let e = SoundGen::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("SNDG::DATA is 2 bytes"), "{e}");
    }
}
