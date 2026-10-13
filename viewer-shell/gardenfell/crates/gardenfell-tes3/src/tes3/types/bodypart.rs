//! `BODY`: a body part mesh of a race. `tes3` bodypart.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bodypart {
    pub flags: u32,
    pub id: String,
    /// `FNAM`: the race the part belongs to.
    pub race: String,
    pub mesh: String,
    pub data: BodypartData,
}

/// `BYDT`, 4 bytes, one per field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodypartData {
    /// `consts::bodypart_id::*`.
    pub part: u8,
    /// Nonzero for a vampire part. `tes3` reads this as a `bool` and writes it back as 0
    /// or 1, so a file with another byte here is kept as is only on this side.
    pub vampire: u8,
    /// `consts::bodypart::*`.
    pub flags: u8,
    /// `consts::bodypart_type::*`.
    pub bodypart_type: u8,
}

impl BodypartData {
    pub const SIZE: usize = 4;

    pub fn read(r: &mut Reader<'_>) -> Result<BodypartData> {
        Ok(BodypartData { part: r.u8()?, vampire: r.u8()?, flags: r.u8()?, bodypart_type: r.u8()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u8(self.part);
        w.u8(self.vampire);
        w.u8(self.flags);
        w.u8(self.bodypart_type);
    }
}

impl Typed for Bodypart {
    const TAG: Tag = *b"BODY";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.race = text(s),
                b"BYDT" => this.data = BodypartData::read(&mut sized(rec, s, BodypartData::SIZE)?)?,
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
        out.text_if(b"FNAM", &self.race)?;
        out.sub(b"BYDT", |w| {
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
    fn a_bodypart_round_trips_and_a_long_bydt_is_refused() {
        let mut rec = Record::new(b"BODY");
        rec.push(b"NAME", b"b_n_dark elf_f_head_01\0".to_vec());
        rec.push(b"MODL", b"b\\b_n_dark elf_f_head_01.nif\0".to_vec());
        rec.push(b"FNAM", b"Dark Elf\0".to_vec());
        rec.push(
            b"BYDT",
            vec![consts::bodypart_id::HEAD, 0, consts::bodypart::FEMALE, consts::bodypart_type::SKIN],
        );
        let b = Bodypart::from_raw(&rec).unwrap();
        assert_eq!(b.race, "Dark Elf");
        assert_eq!(
            b.data,
            BodypartData {
                part: consts::bodypart_id::HEAD,
                vampire: 0,
                flags: consts::bodypart::FEMALE,
                bodypart_type: consts::bodypart_type::SKIN,
            }
        );
        assert_eq!(b.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().push(0);
        let e = Bodypart::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("BODY::BYDT is 5 bytes"), "{e}");
    }
}
