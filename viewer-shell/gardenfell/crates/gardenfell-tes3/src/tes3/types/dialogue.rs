//! `DIAL`: a dialogue topic, which the `INFO` records after it belong to. `tes3` dialogue.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dialogue {
    pub flags: u32,
    pub id: String,
    /// `DATA`, one byte: `consts::dialogue_type::*`, a `u8` here, so compare with a cast.
    /// A deleted topic sometimes has a four-byte `DATA` instead, which `tes3` skips and
    /// so does this, leaving the type at zero; the round trip writes one byte.
    pub dialogue_type: u8,
}

impl Typed for Dialogue {
    const TAG: Tag = *b"DIAL";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"DATA" => {
                    if s.data.len() == 1 {
                        this.dialogue_type = s.data[0];
                    }
                }
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.u8(b"DATA", self.dialogue_type);
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialogue_round_trips_and_a_deleted_one_with_a_wide_data_comes_back_canonical() {
        let mut rec = Record::new(b"DIAL");
        rec.push(b"NAME", b"Greeting 1\0".to_vec());
        rec.push(b"DATA", vec![consts::dialogue_type::GREETING as u8]);
        let d = Dialogue::from_raw(&rec).unwrap();
        let greeting = consts::dialogue_type::GREETING as u8;
        assert_eq!(d, Dialogue { flags: 0, id: "Greeting 1".into(), dialogue_type: greeting });
        assert_eq!(d.to_raw().unwrap(), rec);

        let mut del = Record::new(b"DIAL");
        del.push(b"NAME", b"old topic\0".to_vec());
        del.push(b"DATA", vec![0, 0, 0, 0]);
        del.push(b"DELE", vec![0, 0, 0, 0]);
        let d = Dialogue::from_raw(&del).unwrap();
        assert_eq!(d.flags, consts::object::DELETED);
        assert_eq!(d.dialogue_type, 0);
        let back = d.to_raw().unwrap();
        assert_eq!(back.flags, consts::object::DELETED);
        assert_eq!(back.subs[1].data, vec![0]);
        assert_eq!(back.subs[2].tag, *b"DELE");

        let mut bad = rec.clone();
        bad.push(b"FNAM", b"x\0".to_vec());
        let e = Dialogue::from_raw(&bad).unwrap_err();
        assert!(e.what.contains("DIAL::FNAM"), "{e}");
    }
}
