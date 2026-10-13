//! `NiAutoNormalParticles`: particles whose normals face the camera, nothing of its own.
//! `tes3` niautonormalparticles.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiAutoNormalParticles {
    pub base: NiParticles,
}

impl Block for NiAutoNormalParticles {
    const NAME: &'static str = "NiAutoNormalParticles";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAutoNormalParticles { base: NiParticles::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}
