//! `NiAVObject`: anything with a place in the scene, a transform, properties and an
//! optional bounding volume. `tes3` niavobject.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiAVObject {
    pub base: NiObjectNET,
    /// `consts::av_flags::*`: bit 0 app culled, bits 1-2 propagate mode, bit 3 visual.
    pub flags: u16,
    pub translation: [f32; 3],
    /// The file's 3x3, row by row.
    pub rotation: [[f32; 3]; 3],
    pub scale: f32,
    pub velocity: [f32; 3],
    pub properties: Vec<Link<NiProperty>>,
    /// Present when the file's has-bounding-volume word is non-zero.
    pub bounding_volume: Option<NiBoundingVolume>,
}

impl Default for NiAVObject {
    fn default() -> Self {
        NiAVObject {
            base: Default::default(),
            flags: 0,
            translation: [0.0; 3],
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            scale: 1.0,
            velocity: [0.0; 3],
            properties: Vec::new(),
            bounding_volume: None,
        }
    }
}

impl Block for NiAVObject {
    const NAME: &'static str = "NiAVObject";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObjectNET::read(r)?;
        let flags = r.u16()?;
        let translation = r.vec3()?;
        let rotation = r.mat3()?;
        let scale = r.f32()?;
        let velocity = r.vec3()?;
        let properties = r.links()?;
        let bounding_volume = if r.bool_u32()? { Some(NiBoundingVolume::read(r)?) } else { None };
        Ok(NiAVObject { base, flags, translation, rotation, scale, velocity, properties, bounding_volume })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.flags);
        w.vec3(&self.translation);
        w.mat3(&self.rotation);
        w.f32(self.scale);
        w.vec3(&self.velocity);
        w.links(&self.properties);
        w.bool_u32(self.bounding_volume.is_some());
        if let Some(bv) = &self.bounding_volume {
            bv.write(w)?;
        }
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        for l in &mut self.properties {
            f(&mut l.index);
        }
    }
}

impl NiAVObject {
    pub fn app_culled(&self) -> bool {
        self.flags & consts::av_flags::APP_CULLED != 0
    }
}
