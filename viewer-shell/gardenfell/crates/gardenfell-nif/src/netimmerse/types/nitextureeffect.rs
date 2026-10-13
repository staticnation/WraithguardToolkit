//! `NiTextureEffect`: a texture projected onto the nodes it affects, an environment map
//! or a projected light or shadow. `tes3` nitextureeffect.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiTextureEffect {
    pub base: NiDynamicEffect,
    /// The file's 3x3, row by row.
    pub projection_matrix: [[f32; 3]; 3],
    pub projection_translation: [f32; 3],
    /// `consts::filter_mode::*`.
    pub texture_filter: i32,
    /// `consts::clamp_mode::*`.
    pub texture_clamp: i32,
    /// `consts::texture_type::*`.
    pub texture_type: i32,
    /// `consts::coord_gen_type::*`.
    pub coordinate_generation_type: i32,
    pub source_texture: Link<NiSourceTexture>,
    pub clipping_plane_enable: u8,
    /// The normal and the constant of an `NiPlane`.
    pub clipping_plane: [f32; 4],
    pub ps2_l: i16,
    pub ps2_k: i16,
    pub unknown_byte1: u8,
    pub unknown_byte2: u8,
}

impl Default for NiTextureEffect {
    fn default() -> Self {
        NiTextureEffect {
            base: Default::default(),
            projection_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            projection_translation: [0.0; 3],
            texture_filter: 0,
            texture_clamp: 0,
            texture_type: 0,
            coordinate_generation_type: 0,
            source_texture: Link::NONE,
            clipping_plane_enable: 0,
            clipping_plane: [0.0; 4],
            ps2_l: 0,
            ps2_k: -75,
            unknown_byte1: 0,
            unknown_byte2: 0,
        }
    }
}

impl Block for NiTextureEffect {
    const NAME: &'static str = "NiTextureEffect";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTextureEffect {
            base: NiDynamicEffect::read(r)?,
            projection_matrix: r.mat3()?,
            projection_translation: r.vec3()?,
            texture_filter: r.i32()?,
            texture_clamp: r.i32()?,
            texture_type: r.i32()?,
            coordinate_generation_type: r.i32()?,
            source_texture: r.link()?,
            clipping_plane_enable: r.u8()?,
            clipping_plane: r.vec4()?,
            ps2_l: r.i16()?,
            ps2_k: r.i16()?,
            unknown_byte1: r.u8()?,
            unknown_byte2: r.u8()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.mat3(&self.projection_matrix);
        w.vec3(&self.projection_translation);
        w.i32(self.texture_filter);
        w.i32(self.texture_clamp);
        w.i32(self.texture_type);
        w.i32(self.coordinate_generation_type);
        w.link(self.source_texture);
        w.u8(self.clipping_plane_enable);
        w.vec4(&self.clipping_plane);
        w.i16(self.ps2_l);
        w.i16(self.ps2_k);
        w.u8(self.unknown_byte1);
        w.u8(self.unknown_byte2);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.source_texture.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_effect_round_trips_with_its_defaults_and_without() {
        let d = NiTextureEffect::default();
        assert_eq!(d.ps2_k, -75);
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiTextureEffect::read(&mut Reader::new(&w.out)).unwrap(), d);
        let e = NiTextureEffect {
            texture_filter: consts::filter_mode::TRILERP,
            texture_clamp: consts::clamp_mode::WRAP_S_WRAP_T,
            texture_type: consts::texture_type::ENVIRONMENT_MAP,
            coordinate_generation_type: consts::coord_gen_type::SPHERE_MAP,
            source_texture: Link::new(12),
            clipping_plane_enable: 1,
            clipping_plane: [0.0, 0.0, 1.0, 2.0],
            ps2_l: 3,
            ..Default::default()
        };
        let mut w = Writer::new();
        e.write(&mut w).unwrap();
        assert_eq!(NiTextureEffect::read(&mut Reader::new(&w.out)).unwrap(), e);
        let err = NiTextureEffect::read(&mut Reader::new(&w.out[..w.out.len() - 1])).unwrap_err();
        assert!(err.what.contains("1 bytes wanted"), "{err}");
    }
}
