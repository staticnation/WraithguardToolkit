//! `NiBSPArrayController`: Bethesda's particle system controller that emits from the
//! vertices or faces of its emitter. Nothing of its own beyond
//! `NiParticleSystemController`. `tes3` nibsparraycontroller.rs.
//!
//! Its own flag lives in `base.base.flags`: `consts::bsp_array_controller_flags::AT_VERTICES`
//! (bit 4) emits from the vertices rather than the faces.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiBSPArrayController {
    pub base: NiParticleSystemController,
}

impl Block for NiBSPArrayController {
    const NAME: &'static str = "NiBSPArrayController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBSPArrayController { base: NiParticleSystemController::read(r)? })
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
    fn a_bsp_array_controller_round_trips() {
        let mut c = NiBSPArrayController::default();
        c.base.base.flags = consts::bsp_array_controller_flags::AT_VERTICES;
        c.base.base.target = Link::new(1);
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiBSPArrayController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
