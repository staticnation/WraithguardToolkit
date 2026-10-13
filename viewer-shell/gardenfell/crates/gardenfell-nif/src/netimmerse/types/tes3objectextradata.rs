//! `TES3ObjectExtraData`: an extra data block Morrowind's own tools wrote, nothing of its
//! own beyond `NiExtraData`. `tes3` tes3objectextradata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TES3ObjectExtraData {
    pub base: NiExtraData,
}

impl Block for TES3ObjectExtraData {
    const NAME: &'static str = "TES3ObjectExtraData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(TES3ObjectExtraData { base: NiExtraData::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tes3_object_extra_data_round_trips() {
        let mut x = TES3ObjectExtraData::default();
        x.base.bytes_remaining = 0;
        x.base.next = Link::NONE;
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8);
        assert_eq!(TES3ObjectExtraData::read(&mut Reader::new(&w.out)).unwrap(), x);
    }
}
