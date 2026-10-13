//! `NiLines`: a geometry drawn as line segments, nothing of its own. `tes3` nilines.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiLines {
    pub base: NiGeometry,
}

impl Block for NiLines {
    const NAME: &'static str = "NiLines";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiLines { base: NiGeometry::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}
