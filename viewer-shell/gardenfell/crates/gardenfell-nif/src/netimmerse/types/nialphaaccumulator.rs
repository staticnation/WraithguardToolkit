//! `NiAlphaAccumulator`: the accumulator that sorts translucent shapes back to front,
//! nothing beyond the base. `tes3` nialphaaccumulator.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiAlphaAccumulator {
    pub base: NiClusterAccumulator,
}

impl Block for NiAlphaAccumulator {
    const NAME: &'static str = "NiAlphaAccumulator";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAlphaAccumulator { base: NiClusterAccumulator::read(r)? })
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
    fn an_alpha_accumulator_is_empty() {
        let mut w = Writer::new();
        NiAlphaAccumulator::default().write(&mut w).unwrap();
        assert!(w.out.is_empty());
        let back = NiAlphaAccumulator::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, NiAlphaAccumulator::default());
    }
}
