//! `NiAccumulator`: the base of the blocks that collect and sort what is drawn, nothing of
//! its own. `tes3` niaccumulator.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiAccumulator {
    pub base: NiObject,
}

impl Block for NiAccumulator {
    const NAME: &'static str = "NiAccumulator";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAccumulator { base: NiObject::read(r)? })
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
    fn an_accumulator_is_empty() {
        let mut w = Writer::new();
        NiAccumulator::default().write(&mut w).unwrap();
        assert!(w.out.is_empty());
        assert_eq!(NiAccumulator::read(&mut Reader::new(&w.out)).unwrap(), NiAccumulator::default());
    }
}
