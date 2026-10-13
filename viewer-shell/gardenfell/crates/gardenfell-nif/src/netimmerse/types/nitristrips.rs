//! `NiTriStrips`: a triangle strip mesh, nothing of its own. `tes3` nitristrips.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriStrips {
    pub base: NiTriBasedGeom,
}

impl Block for NiTriStrips {
    const NAME: &'static str = "NiTriStrips";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTriStrips { base: NiTriBasedGeom::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}
