//! `NiMorpherController`: a controller that blends its geometry's vertices between the
//! targets of an `NiMorphData`. The base of `NiGeomMorpherController`. `tes3`
//! nimorphercontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiMorpherController {
    pub base: NiTimeController,
    pub data: Link<NiMorphData>,
}

impl Block for NiMorpherController {
    const NAME: &'static str = "NiMorpherController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiMorpherController { base: NiTimeController::read(r)?, data: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.data);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.data.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_morpher_controller_round_trips() {
        let c = NiMorpherController { data: Link::new(9), ..Default::default() };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiMorpherController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
