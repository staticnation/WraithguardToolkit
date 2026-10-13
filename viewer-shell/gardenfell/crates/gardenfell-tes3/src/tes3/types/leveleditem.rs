//! `LEVI`: a leveled item list. `tes3` leveleditem.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeveledItem {
    pub flags: u32,
    pub id: String,
    /// `DATA`: `consts::leveled_item::*`.
    pub leveled_item_flags: u32,
    /// `NNAM`: the percent chance the list yields nothing.
    pub chance_none: u8,
    /// One `INAM` (the item id) and `INTV` (the player level it needs) per entry, after an
    /// `INDX` holding the count. The count is written back as the list's length, as `tes3`
    /// does, so a file whose count disagrees with its entries comes back corrected.
    pub items: Vec<(String, u16)>,
}

impl Typed for LeveledItem {
    const TAG: Tag = *b"LEVI";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"DATA" => this.leveled_item_flags = u32_of(rec, s)?,
                b"NNAM" => this.chance_none = u8_of(rec, s)?,
                b"INDX" => this.items.reserve(u32_of(rec, s)? as usize),
                b"INAM" => this.items.push((text(s), 0)),
                b"INTV" => match this.items.last_mut() {
                    Some(item) => item.1 = u16_of(rec, s)?,
                    None => return Err(unexpected_after(rec, s, b"INAM")),
                },
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.u32(b"DATA", self.leveled_item_flags);
        out.u8(b"NNAM", self.chance_none);
        if !self.items.is_empty() {
            out.u32(b"INDX", self.items.len() as u32);
            for (item, level) in &self.items {
                out.text(b"INAM", item)?;
                out.u16(b"INTV", *level);
            }
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leveled_item_round_trips_and_an_empty_list_has_no_indx() {
        let mut rec = Record::new(b"LEVI");
        rec.push(b"NAME", b"random_food\0".to_vec());
        rec.push(b"DATA", consts::leveled_item::CALCULATE_FOR_EACH_ITEM.to_le_bytes().to_vec());
        rec.push(b"NNAM", vec![25]);
        rec.push(b"INDX", 2u32.to_le_bytes().to_vec());
        rec.push(b"INAM", b"ingred_bread_01\0".to_vec());
        rec.push(b"INTV", 1u16.to_le_bytes().to_vec());
        rec.push(b"INAM", b"ingred_crab_meat_01\0".to_vec());
        rec.push(b"INTV", 5u16.to_le_bytes().to_vec());
        let l = LeveledItem::from_raw(&rec).unwrap();
        assert_eq!(l.chance_none, 25);
        assert_eq!(l.items, vec![("ingred_bread_01".to_string(), 1), ("ingred_crab_meat_01".to_string(), 5)]);
        assert_eq!(l.to_raw().unwrap(), rec);

        let empty = LeveledItem { id: "nothing".into(), ..Default::default() };
        let back = empty.to_raw().unwrap();
        assert_eq!(back.subs.iter().map(|s| s.tag).collect::<Vec<_>>(), vec![*b"NAME", *b"DATA", *b"NNAM"]);
        assert_eq!(LeveledItem::from_raw(&back).unwrap(), empty);
    }

    #[test]
    fn an_intv_before_any_inam_is_refused() {
        let mut rec = Record::new(b"LEVI");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"INTV", 1u16.to_le_bytes().to_vec());
        let e = LeveledItem::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("LEVI::INTV where INAM was expected"), "{e}");
    }
}
