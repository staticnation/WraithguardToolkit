//! `STAT`: a static object. `tes3` static_.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Static {
    /// The header flags, with `consts::object::DELETED` set for a `DELE` record.
    pub flags: u32,
    pub id: String,
    pub mesh: String,
}

impl Typed for Static {
    const TAG: Tag = *b"STAT";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
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
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_static_round_trips_and_a_deleted_one_gets_its_dele() {
        let mut rec = Record::new(b"STAT");
        rec.push(b"NAME", b"rock\0".to_vec());
        rec.push(b"MODL", b"f\\rock.nif\0".to_vec());
        let s = Static::from_raw(&rec).unwrap();
        assert_eq!(s, Static { flags: 0, id: "rock".into(), mesh: "f\\rock.nif".into() });
        assert_eq!(s.to_raw().unwrap(), rec);

        let mut del = rec.clone();
        del.flags = consts::object::DELETED;
        let s = Static::from_raw(&del).unwrap();
        assert_eq!(s.flags, consts::object::DELETED);
        let back = s.to_raw().unwrap();
        assert_eq!(back.subs.last().unwrap().tag, *b"DELE");
        assert_eq!(Static::from_raw(&back).unwrap(), s);
    }
}
