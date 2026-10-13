//! `NiSequenceStreamHelper`: the root of a `.kf` file, a named block whose controller
//! chain holds the animation and whose extra data holds the text keys. Nothing of its own
//! beyond `NiObjectNET`. `tes3` nisequencestreamhelper.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSequenceStreamHelper {
    pub base: NiObjectNET,
}

impl Block for NiSequenceStreamHelper {
    const NAME: &'static str = "NiSequenceStreamHelper";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSequenceStreamHelper { base: NiObjectNET::read(r)? })
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
    fn a_stream_helper_round_trips() {
        let mut h = NiSequenceStreamHelper::default();
        h.base.controller = Link::new(1);
        h.base.extra_data = Link::new(2);
        let mut w = Writer::new();
        h.write(&mut w).unwrap();
        assert_eq!(NiSequenceStreamHelper::read(&mut Reader::new(&w.out)).unwrap(), h);
    }
}
