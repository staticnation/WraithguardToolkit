//! `NiRotatingParticles`: particles that spin, each with its own rotation in the data.
//! Nothing of its own. `tes3` nirotatingparticles.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiRotatingParticles {
    pub base: NiParticles,
}

impl Block for NiRotatingParticles {
    const NAME: &'static str = "NiRotatingParticles";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiRotatingParticles { base: NiParticles::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}
