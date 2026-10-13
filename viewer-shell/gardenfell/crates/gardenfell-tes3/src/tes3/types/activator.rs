//! `ACTI`: an activator. `tes3` activator.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activator {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
}

impl Typed for Activator {
    const TAG: Tag = *b"ACTI";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"SCRI" => this.script = text(s),
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
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_activator_round_trips_and_an_unknown_subrecord_is_named() {
        let mut rec = Record::new(b"ACTI");
        rec.push(b"NAME", b"ex_bell\0".to_vec());
        rec.push(b"MODL", b"x\\ex_bell.nif\0".to_vec());
        rec.push(b"FNAM", b"Bell\0".to_vec());
        rec.push(b"SCRI", b"bell_script\0".to_vec());
        let a = Activator::from_raw(&rec).unwrap();
        assert_eq!(
            a,
            Activator {
                flags: 0,
                id: "ex_bell".into(),
                name: "Bell".into(),
                script: "bell_script".into(),
                mesh: "x\\ex_bell.nif".into(),
            }
        );
        assert_eq!(a.to_raw().unwrap(), rec);

        rec.push(b"ITEX", b"x\0".to_vec());
        let e = Activator::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("ACTI::ITEX"), "{e}");
    }
}
