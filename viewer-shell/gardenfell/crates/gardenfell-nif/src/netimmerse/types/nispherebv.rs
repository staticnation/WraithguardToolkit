//! `NiSphereBV`: a sphere bounding volume, center and radius. `tes3` nispherebv.rs, which
//! holds the pair as an `NiBound`.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiSphereBV {
    pub center: [f32; 3],
    pub radius: f32,
}

impl NiSphereBV {
    pub fn read(r: &mut Reader<'_>) -> Result<NiSphereBV> {
        Ok(NiSphereBV { center: r.vec3()?, radius: r.f32()? })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.vec3(&self.center);
        w.f32(self.radius);
        Ok(())
    }
}
