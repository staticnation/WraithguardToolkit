//! `CONT`: a container and what it holds. `tes3` container.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    /// `CNDT`: the weight the container can hold.
    pub encumbrance: f32,
    /// `FLAG`: `consts::container::*`.
    pub container_flags: u32,
    /// `NPCO`, one per entry: a count (negative for a restocking item) and a 32-byte item id.
    pub inventory: Vec<(i32, String)>,
}

impl Typed for Container {
    const TAG: Tag = *b"CONT";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"CNDT" => this.encumbrance = f32_of(rec, s)?,
                b"FLAG" => this.container_flags = u32_of(rec, s)?,
                b"SCRI" => this.script = text(s),
                b"NPCO" => {
                    let mut r = sized(rec, s, 36)?;
                    let count = r.i32()?;
                    let id = r.fixed_string(32)?;
                    this.inventory.push((count, id));
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
        out.text_if(b"MODL", &self.mesh)?;
        out.text_if(b"FNAM", &self.name)?;
        out.f32(b"CNDT", self.encumbrance);
        out.u32(b"FLAG", self.container_flags);
        out.text_if(b"SCRI", &self.script)?;
        for (count, id) in &self.inventory {
            out.sub(b"NPCO", |w| {
                w.i32(*count);
                w.fixed_string(id, 32)
            })?;
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_round_trips_and_a_short_npco_is_refused() {
        let mut rec = Record::new(b"CONT");
        rec.push(b"NAME", b"crate_01\0".to_vec());
        rec.push(b"MODL", b"o\\contain_crate_01.nif\0".to_vec());
        rec.push(b"FNAM", b"Crate\0".to_vec());
        rec.push(b"CNDT", 60.0f32.to_le_bytes().to_vec());
        rec.push(b"FLAG", consts::container::RESPAWNS.to_le_bytes().to_vec());
        rec.push(b"SCRI", b"crate_script\0".to_vec());
        let mut w = Writer::new();
        w.i32(2);
        w.fixed_string("ingred_bread_01", 32).unwrap();
        rec.push(b"NPCO", w.into_bytes());
        let mut w = Writer::new();
        w.i32(-1);
        w.fixed_string("random_food", 32).unwrap();
        rec.push(b"NPCO", w.into_bytes());
        let c = Container::from_raw(&rec).unwrap();
        assert_eq!(c.encumbrance, 60.0);
        assert_eq!(c.container_flags, consts::container::RESPAWNS);
        assert_eq!(c.inventory, vec![(2, "ingred_bread_01".to_string()), (-1, "random_food".to_string())]);
        assert_eq!(c.to_raw().unwrap(), rec);

        rec.subs[6].data.to_mut().truncate(35);
        let e = Container::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CONT::NPCO is 35 bytes"), "{e}");
    }
}
