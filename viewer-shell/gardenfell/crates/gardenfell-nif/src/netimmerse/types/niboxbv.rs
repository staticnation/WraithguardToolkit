//! `NiBoxBV`: an oriented box bounding volume. `tes3` niboxbv.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiBoxBV {
    pub center: [f32; 3],
    /// The box's axes, the file's 3x3 row by row.
    pub axis: [[f32; 3]; 3],
    pub extents: [f32; 3],
}

impl Default for NiBoxBV {
    fn default() -> Self {
        NiBoxBV {
            center: [0.0; 3],
            axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            extents: [0.0; 3],
        }
    }
}

impl NiBoxBV {
    pub fn read(r: &mut Reader<'_>) -> Result<NiBoxBV> {
        Ok(NiBoxBV { center: r.vec3()?, axis: r.mat3()?, extents: r.vec3()? })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.vec3(&self.center);
        w.mat3(&self.axis);
        w.vec3(&self.extents);
        Ok(())
    }
}
