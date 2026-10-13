//! `NiExtraData`: the base of every extra data block, a link to the next in the chain and
//! the byte count the original engine used to skip a block it did not know. `tes3`
//! niextradata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiExtraData {
    pub base: NiObject,
    pub next: Link<NiExtraData>,
    /// The size of what follows in the derived block, as the writer counted it. Kept as
    /// read, not recomputed.
    pub bytes_remaining: u32,
}

impl Block for NiExtraData {
    const NAME: &'static str = "NiExtraData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiExtraData { base: NiObject::read(r)?, next: r.link()?, bytes_remaining: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.next);
        w.u32(self.bytes_remaining);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.next.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_data_round_trips_in_eight_bytes() {
        let x = NiExtraData { next: Link::new(3), bytes_remaining: 12, ..Default::default() };
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8);
        assert_eq!(NiExtraData::read(&mut Reader::new(&w.out)).unwrap(), x);
    }
}
