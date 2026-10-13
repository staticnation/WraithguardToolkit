//! `NiTriBasedGeom`: a geometry drawn as triangles, nothing of its own. `tes3`
//! nitribasedgeom.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriBasedGeom {
    pub base: NiGeometry,
}

impl Block for NiTriBasedGeom {
    const NAME: &'static str = "NiTriBasedGeom";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTriBasedGeom { base: NiGeometry::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}
