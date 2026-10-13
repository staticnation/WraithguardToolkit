//! `MISC`: a miscellaneous item, keys included. `tes3` miscitem.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MiscItem {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub data: MiscItemData,
}

/// `MCDT`, 12 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MiscItemData {
    pub weight: f32,
    pub value: u32,
    /// `consts::misc_item::*`.
    pub flags: u32,
}

impl MiscItemData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<MiscItemData> {
        Ok(MiscItemData { weight: r.f32()?, value: r.u32()?, flags: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.u32(self.flags);
    }
}

impl Typed for MiscItem {
    const TAG: Tag = *b"MISC";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"MCDT" => this.data = MiscItemData::read(&mut sized(rec, s, MiscItemData::SIZE)?)?,
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
        out.sub(b"MCDT", |w| {
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
    fn a_misc_item_round_trips_and_a_short_mcdt_is_refused() {
        let mut rec = Record::new(b"MISC");
        rec.push(b"NAME", b"key_vault\0".to_vec());
        rec.push(b"MODL", b"m\\key.nif\0".to_vec());
        rec.push(b"FNAM", b"Vault Key\0".to_vec());
        let mut w = Writer::new();
        MiscItemData { weight: 0.1, value: 0, flags: consts::misc_item::KEY }.write(&mut w);
        rec.push(b"MCDT", w.into_bytes());
        rec.push(b"ITEX", b"m\\key.dds\0".to_vec());
        let m = MiscItem::from_raw(&rec).unwrap();
        assert_eq!(m.data.flags, consts::misc_item::KEY);
        assert_eq!(m.icon, "m\\key.dds");
        assert_eq!(m.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(8);
        let e = MiscItem::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("MISC::MCDT is 8 bytes"), "{e}");
    }
}
