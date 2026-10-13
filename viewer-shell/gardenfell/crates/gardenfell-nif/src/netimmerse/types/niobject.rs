//! `NiObject`: the root of every block type, with nothing of its own. `tes3` niobject.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiObject;

impl Block for NiObject {
    const NAME: &'static str = "NiObject";

    fn read(_r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiObject)
    }

    fn write(&self, _w: &mut Writer) -> Result<()> {
        Ok(())
    }

    fn links(&mut self, _f: &mut dyn FnMut(&mut i32)) {}
}
