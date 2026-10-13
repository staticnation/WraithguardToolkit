//! `DOOR`: a door, with its open and close sounds. `tes3` door.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Door {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub open_sound: String,
    pub close_sound: String,
}

impl Typed for Door {
    const TAG: Tag = *b"DOOR";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"SCRI" => this.script = text(s),
                b"SNAM" => this.open_sound = text(s),
                b"ANAM" => this.close_sound = text(s),
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
        out.text_if(b"SNAM", &self.open_sound)?;
        out.text_if(b"ANAM", &self.close_sound)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_door_round_trips_and_an_unknown_subrecord_is_named() {
        let mut rec = Record::new(b"DOOR");
        rec.push(b"NAME", b"in_hlaalu_door\0".to_vec());
        rec.push(b"MODL", b"i\\in_hlaalu_door.nif\0".to_vec());
        rec.push(b"FNAM", b"Door\0".to_vec());
        rec.push(b"SNAM", b"Door Wood Open\0".to_vec());
        rec.push(b"ANAM", b"Door Wood Close\0".to_vec());
        let d = Door::from_raw(&rec).unwrap();
        assert_eq!(
            d,
            Door {
                flags: 0,
                id: "in_hlaalu_door".into(),
                name: "Door".into(),
                script: String::new(),
                mesh: "i\\in_hlaalu_door.nif".into(),
                open_sound: "Door Wood Open".into(),
                close_sound: "Door Wood Close".into(),
            }
        );
        assert_eq!(d.to_raw().unwrap(), rec);

        rec.push(b"ITEX", b"x\0".to_vec());
        let e = Door::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("DOOR::ITEX"), "{e}");
    }
}
