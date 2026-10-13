//! `NiGeomMorpherController`: the morpher controller every Morrowind mesh uses, with one
//! byte of its own. `tes3` nigeommorphercontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiGeomMorpherController {
    pub base: NiMorpherController,
    /// One byte in the file.
    pub always_update: bool,
}

impl Block for NiGeomMorpherController {
    const NAME: &'static str = "NiGeomMorpherController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiGeomMorpherController { base: NiMorpherController::read(r)?, always_update: r.bool_u8()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.bool_u8(self.always_update);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_geom_morpher_controller_round_trips() {
        let mut c = NiGeomMorpherController { always_update: true, ..Default::default() };
        c.base.data = Link::new(1);
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 2 + 16 + 4 + 4 + 1);
        assert_eq!(NiGeomMorpherController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
