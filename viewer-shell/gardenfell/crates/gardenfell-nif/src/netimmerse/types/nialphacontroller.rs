//! `NiAlphaController`: a float controller that drives a material's alpha. Nothing of its
//! own beyond `NiFloatController`. `tes3` nialphacontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiAlphaController {
    pub base: NiFloatController,
}

impl Block for NiAlphaController {
    const NAME: &'static str = "NiAlphaController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAlphaController { base: NiFloatController::read(r)? })
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
    fn an_alpha_controller_round_trips() {
        let mut c = NiAlphaController::default();
        c.base.data = Link::new(7);
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiAlphaController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
