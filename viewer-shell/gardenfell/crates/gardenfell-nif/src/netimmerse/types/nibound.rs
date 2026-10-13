//! `NiBound`: a bounding sphere, center and radius, as geometry data and sphere volumes
//! carry it. `tes3` nibound.rs.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiBound {
    pub center: [f32; 3],
    pub radius: f32,
}

impl NiBound {
    pub const SIZE: usize = 16;

    pub fn read(r: &mut Reader<'_>) -> Result<NiBound> {
        Ok(NiBound { center: r.vec3()?, radius: r.f32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.vec3(&self.center);
        w.f32(self.radius);
    }
}
