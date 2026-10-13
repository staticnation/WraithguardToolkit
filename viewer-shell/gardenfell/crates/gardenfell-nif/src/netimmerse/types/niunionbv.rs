//! `NiUnionBV`: a bounding volume that is the union of others. `tes3` niunionbv.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiUnionBV {
    pub bounding_volumes: Vec<NiBoundingVolume>,
}

impl NiUnionBV {
    pub fn read(r: &mut Reader<'_>) -> Result<NiUnionBV> {
        // A volume is at least its type word and a sphere's 16 bytes.
        Ok(NiUnionBV { bounding_volumes: r.counted(20, NiBoundingVolume::read)? })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.count(self.bounding_volumes.len());
        for v in &self.bounding_volumes {
            v.write(w)?;
        }
        Ok(())
    }
}
