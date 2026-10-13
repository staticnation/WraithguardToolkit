//! `REPA`: a repair tool. `tes3` repairitem.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepairItem {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub data: RepairItemData,
}

/// `RIDT`, 16 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepairItemData {
    pub weight: f32,
    pub value: u32,
    pub uses: u32,
    pub quality: f32,
}

impl RepairItemData {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<RepairItemData> {
        Ok(RepairItemData { weight: r.f32()?, value: r.u32()?, uses: r.u32()?, quality: r.f32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.u32(self.uses);
        w.f32(self.quality);
    }
}

impl Typed for RepairItem {
    const TAG: Tag = *b"REPA";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"RIDT" => this.data = RepairItemData::read(&mut sized(rec, s, RepairItemData::SIZE)?)?,
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
        out.sub(b"RIDT", |w| {
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
    fn a_repair_item_round_trips_and_a_short_ridt_is_refused() {
        let mut rec = Record::new(b"REPA");
        rec.push(b"NAME", b"repair_prongs\0".to_vec());
        rec.push(b"MODL", b"m\\repair_prongs.nif\0".to_vec());
        rec.push(b"FNAM", b"Repair Prongs\0".to_vec());
        let data = RepairItemData { weight: 1.0, value: 6, uses: 10, quality: 1.0 };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"RIDT", w.into_bytes());
        rec.push(b"ITEX", b"m\\tx_repair_prongs.dds\0".to_vec());
        let r = RepairItem::from_raw(&rec).unwrap();
        assert_eq!(r.data, data);
        assert_eq!(r.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(12);
        let e = RepairItem::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("REPA::RIDT is 12 bytes"), "{e}");
    }
}
