//! `NiRollController`: a float controller that rolls its target about an axis. Nothing of
//! its own beyond `NiFloatController`. `tes3` nirollcontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiRollController {
    pub base: NiFloatController,
}

impl Block for NiRollController {
    const NAME: &'static str = "NiRollController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiRollController { base: NiFloatController::read(r)? })
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
    fn a_roll_controller_round_trips() {
        let mut c = NiRollController::default();
        c.base.base.frequency = 0.5;
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiRollController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
