//! `BrickNiExtraData`: an extra data block the format names but gives nothing of its own
//! beyond `NiExtraData`. `tes3` brickniextradata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrickNiExtraData {
    pub base: NiExtraData,
}

impl Block for BrickNiExtraData {
    const NAME: &'static str = "BrickNiExtraData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(BrickNiExtraData { base: NiExtraData::read(r)? })
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
    fn brick_extra_data_round_trips() {
        let mut x = BrickNiExtraData::default();
        x.base.next = Link::new(1);
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(BrickNiExtraData::read(&mut Reader::new(&w.out)).unwrap(), x);
    }
}
