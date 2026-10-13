//! `SSCR`: a start script, a script the game runs when a new game begins. `tes3` startscript.rs.

use super::*;

/// The subrecords are the other way round from every other record: `DATA` holds the id
/// and `NAME` the script it starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartScript {
    pub flags: u32,
    pub id: String,
    pub script: String,
}

impl Typed for StartScript {
    const TAG: Tag = *b"SSCR";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"DATA" => this.id = text(s),
                b"NAME" => this.script = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"DATA", &self.id)?;
        out.text_if(b"NAME", &self.script)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_script_round_trips_and_a_stray_subrecord_is_refused() {
        let mut rec = Record::new(b"SSCR");
        rec.push(b"DATA", b"1\0".to_vec());
        rec.push(b"NAME", b"gf_grass_start\0".to_vec());
        let s = StartScript::from_raw(&rec).unwrap();
        assert_eq!(s, StartScript { flags: 0, id: "1".into(), script: "gf_grass_start".into() });
        assert_eq!(s.to_raw().unwrap(), rec);

        rec.push(b"FNAM", b"x\0".to_vec());
        let e = StartScript::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("SSCR::FNAM"), "{e}");
    }
}
