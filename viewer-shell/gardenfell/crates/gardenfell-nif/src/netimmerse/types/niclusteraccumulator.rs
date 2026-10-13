//! `NiClusterAccumulator`: an accumulator that groups what it collects, nothing beyond the
//! base. `tes3` niclusteraccumulator.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiClusterAccumulator {
    pub base: NiAccumulator,
}

impl Block for NiClusterAccumulator {
    const NAME: &'static str = "NiClusterAccumulator";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiClusterAccumulator { base: NiAccumulator::read(r)? })
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
    fn a_cluster_accumulator_is_empty() {
        let mut w = Writer::new();
        NiClusterAccumulator::default().write(&mut w).unwrap();
        assert!(w.out.is_empty());
        let back = NiClusterAccumulator::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, NiClusterAccumulator::default());
    }
}
