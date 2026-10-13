//! `NiTriBasedGeomData`: geometry data for triangle shapes, nothing of its own. `tes3`
//! nitribasedgeomdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriBasedGeomData {
    pub base: NiGeometryData,
}

impl Block for NiTriBasedGeomData {
    const NAME: &'static str = "NiTriBasedGeomData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTriBasedGeomData { base: NiGeometryData::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}
