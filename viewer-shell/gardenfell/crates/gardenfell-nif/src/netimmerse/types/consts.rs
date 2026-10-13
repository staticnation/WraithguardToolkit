//! The named values of every enumeration and flag word the NIF layouts carry. `tes3`
//! enums.rs and the `flag_props!` blocks in its type files.
//!
//! A field that `tes3` types as an enum is the plain integer here, at the width the file
//! holds it (`i32` for most, `u16` for the ones that live in a flag word), so a value the
//! list does not have is kept and written back as read. One module per enumeration, the
//! constants named for `tes3`'s variants in SCREAMING_SNAKE_CASE, and one `*_flags` module
//! per block type that has a flag word, with each mask and, for a multi-bit field, its
//! shift.

/// `BoundType`: the type word that leads an `NiBoundingVolume`.
pub mod bound_type {
    pub const SPHERE: i32 = 0;
    pub const BOX: i32 = 1;
    pub const CAPSULE: i32 = 2;
    pub const LOZENGE: i32 = 3;
    pub const UNION: i32 = 4;
    pub const HALFSPACE: i32 = 5;
}

/// `KeyContent`: what a key track holds.
pub mod key_content {
    pub const FLOAT_KEY: i32 = 0;
    pub const POS_KEY: i32 = 1;
    pub const ROT_KEY: i32 = 2;
    pub const COLOR_KEY: i32 = 3;
    pub const TEXT_KEY: i32 = 4;
    pub const VIS_KEY: i32 = 5;
}

/// `KeyType`: the interpolation of a key track, the word after a non-zero key count.
pub mod key_type {
    pub const NO_INTERP: i32 = 0;
    pub const LIN: i32 = 1;
    pub const BEZ: i32 = 2;
    pub const TCB: i32 = 3;
    pub const EULER: i32 = 4;
}

/// `ForceType`: the shape of an `NiGravity` field.
pub mod force_type {
    pub const PLANAR: i32 = 0;
    pub const SPHERICAL: i32 = 1;
}

/// `ColorField`: the material color an `NiMaterialColorController` animates.
pub mod color_field {
    pub const AMBIENT: i32 = 0;
    pub const DIFFUSE: i32 = 1;
    pub const SPECULAR: i32 = 2;
    pub const EMISSIVE: i32 = 3;
}

/// `DecayType`: how an `NiParticleBomb` falls off with distance.
pub mod decay_type {
    pub const NONE: i32 = 0;
    pub const LINEAR: i32 = 1;
    pub const EXPONENTIAL: i32 = 2;
}

/// `SymmetryType`: the symmetry of an `NiParticleBomb`.
pub mod symmetry_type {
    pub const SPHERICAL: i32 = 0;
    pub const CYLINDRICAL: i32 = 1;
    pub const PLANAR: i32 = 2;
}

/// `BankDirection`: which way an `NiPathController` banks.
pub mod bank_direction {
    pub const NEGATIVE: i32 = -1;
    pub const POSITIVE: i32 = 1;
}

/// `PixelFormat`: the layout of an `NiPixelFormat`.
pub mod pixel_format {
    pub const RGB: i32 = 0;
    pub const RGBA: i32 = 1;
    pub const PAL: i32 = 2;
    pub const PAL_ALPHA: i32 = 3;
    pub const COMPRESS1: i32 = 4;
    pub const COMPRESS3: i32 = 5;
    pub const COMPRESS5: i32 = 6;
    pub const RGB24_NON_INTERLEAVED: i32 = 7;
    pub const BUMP: i32 = 8;
    pub const BUMP_LUMA: i32 = 9;
}

/// `AxisOrder`: the order of the three axes of an Euler rotation track.
pub mod axis_order {
    pub const XYZ: i32 = 0;
    pub const XZY: i32 = 1;
    pub const YZX: i32 = 2;
    pub const YXZ: i32 = 3;
    pub const ZXY: i32 = 4;
    pub const ZYX: i32 = 5;
    pub const XYX: i32 = 6;
    pub const YZY: i32 = 7;
    pub const ZXZ: i32 = 8;
}

/// `SortingMode`: how an `NiSortAdjustNode` sorts what is under it.
pub mod sorting_mode {
    pub const INHERIT: i32 = 0;
    pub const OFF: i32 = 1;
    pub const SUBSORT: i32 = 2;
    pub const GROUPED: i32 = 64;
}

/// `PixelLayout`: the pixel layout an `NiSourceTexture` asks for.
pub mod pixel_layout {
    pub const PALETTIZED8: i32 = 0;
    pub const HIGH_COLOR16: i32 = 1;
    pub const TRUE_COLOR32: i32 = 2;
    pub const COMPRESSED: i32 = 3;
    pub const BUMP_MAP: i32 = 4;
    pub const PALETTIZED4: i32 = 5;
    pub const DEFAULT: i32 = 6;
}

/// `UseMipMaps`: whether an `NiSourceTexture` wants mipmaps.
pub mod use_mipmaps {
    pub const NO: i32 = 0;
    pub const YES: i32 = 1;
    pub const DEFAULT: i32 = 2;
}

/// `AlphaFormat`: the alpha an `NiSourceTexture` asks for.
pub mod alpha_format {
    pub const NONE: i32 = 0;
    pub const BINARY: i32 = 1;
    pub const SMOOTH: i32 = 2;
    pub const DEFAULT: i32 = 3;
}

/// `StencilTestFunction`: the comparison of an `NiStencilProperty`.
pub mod stencil_test_function {
    pub const NEVER: i32 = 0;
    pub const LESS: i32 = 1;
    pub const EQUAL: i32 = 2;
    pub const LESS_EQUAL: i32 = 3;
    pub const GREATER: i32 = 4;
    pub const NOT_EQUAL: i32 = 5;
    pub const GREATER_EQUAL: i32 = 6;
    pub const ALWAYS: i32 = 7;
}

/// `Action`: what an `NiStencilProperty` does to the stencil on fail, z-fail and pass.
pub mod stencil_action {
    pub const KEEP: i32 = 0;
    pub const ZERO: i32 = 1;
    pub const REPLACE: i32 = 2;
    pub const INCREMENT: i32 = 3;
    pub const DECREMENT: i32 = 4;
    pub const INVERT: i32 = 5;
}

/// `DrawMode`: which faces an `NiStencilProperty` draws.
pub mod draw_mode {
    pub const DEFAULT: i32 = 0;
    pub const COUNTER_CLOCKWISE: i32 = 1;
    pub const CLOCKWISE: i32 = 2;
    pub const BOTH: i32 = 3;
}

/// `TextureType`: what an `NiTextureEffect` projects.
pub mod texture_type {
    pub const PROJECTED_LIGHT: i32 = 0;
    pub const PROJECTED_SHADOW: i32 = 1;
    pub const ENVIRONMENT_MAP: i32 = 2;
    pub const FOG_MAP: i32 = 3;
}

/// `CoordGenType`: how an `NiTextureEffect` generates its texture coordinates.
pub mod coord_gen_type {
    pub const WORLD_PARALLEL: i32 = 0;
    pub const WORLD_PERSPECTIVE: i32 = 1;
    pub const SPHERE_MAP: i32 = 2;
    pub const SPECULAR_CUBE_MAP: i32 = 3;
    pub const DIFFUSE_CUBE_MAP: i32 = 4;
}

/// `ClampMode`: texture wrapping in S and T.
pub mod clamp_mode {
    pub const CLAMP_S_CLAMP_T: i32 = 0;
    pub const CLAMP_S_WRAP_T: i32 = 1;
    pub const WRAP_S_CLAMP_T: i32 = 2;
    pub const WRAP_S_WRAP_T: i32 = 3;
}

/// `FilterMode`: texture filtering.
pub mod filter_mode {
    pub const NEAREST: i32 = 0;
    pub const BILERP: i32 = 1;
    pub const TRILERP: i32 = 2;
    pub const NEAREST_MIP_NEAREST: i32 = 3;
    pub const NEAREST_MIP_LERP: i32 = 4;
    pub const BILERP_MIP_NEAREST: i32 = 5;
}

/// `ApplyMode`: how an `NiTexturingProperty` combines its textures with the material.
pub mod apply_mode {
    pub const REPLACE: i32 = 0;
    pub const DECAL: i32 = 1;
    pub const MODULATE: i32 = 2;
    pub const HILIGHT: i32 = 3;
    pub const HILIGHT2: i32 = 4;
}

/// `SourceVertexMode`: what an `NiVertexColorProperty` takes from the vertex colors.
pub mod source_vertex_mode {
    pub const IGNORE: i32 = 0;
    pub const EMISSIVE: i32 = 1;
    pub const AMBIENT_DIFFUSE: i32 = 2;
}

/// `LightingMode`: which lighting terms an `NiVertexColorProperty` applies.
pub mod lighting_mode {
    pub const EMISSIVE: i32 = 0;
    pub const EMISSIVE_AMBIENT_DIFFUSE: i32 = 1;
}

/// `AlphaBlendFunction`: a blend factor, held in the `NiAlphaProperty` flag word.
pub mod alpha_blend_function {
    pub const ONE: u16 = 0;
    pub const ZERO: u16 = 1;
    pub const SRC_COLOR: u16 = 2;
    pub const INV_SRC_COLOR: u16 = 3;
    pub const DST_COLOR: u16 = 4;
    pub const INV_DST_COLOR: u16 = 5;
    pub const SRC_ALPHA: u16 = 6;
    pub const INV_SRC_ALPHA: u16 = 7;
    pub const DST_ALPHA: u16 = 8;
    pub const INV_DST_ALPHA: u16 = 9;
    pub const SRC_ALPHA_SAT: u16 = 10;
}

/// `AlphaTestFunction`: the alpha test, held in the `NiAlphaProperty` flag word.
pub mod alpha_test_function {
    pub const ALWAYS: u16 = 0;
    pub const LESS: u16 = 1;
    pub const EQUAL: u16 = 2;
    pub const LESS_EQUAL: u16 = 3;
    pub const GREATER: u16 = 4;
    pub const NOT_EQUAL: u16 = 5;
    pub const GREATER_EQUAL: u16 = 6;
    pub const NEVER: u16 = 7;
}

/// `PropagateMode`: collision propagation, held in the `NiAVObject` flag word.
pub mod propagate_mode {
    pub const NONE: u16 = 0;
    pub const USE_TRIANGLES: u16 = 1;
    pub const USE_OBBS: u16 = 2;
    pub const CONTINUE: u16 = 3;
}

/// `Axis`: an axis, held in the `NiLookAtController` flag word.
pub mod axis {
    pub const X: u16 = 0;
    pub const Y: u16 = 1;
    pub const Z: u16 = 2;
}

/// `CycleType`: how a controller loops, held in the `NiTimeController` flag word.
pub mod cycle_type {
    pub const CYCLE: u16 = 0;
    pub const REVERSE: u16 = 1;
    pub const CLAMP: u16 = 2;
}

/// `ZBufferTestFunction`: the depth test, held in the `NiZBufferProperty` flag word.
pub mod zbuffer_test_function {
    pub const ALWAYS: u16 = 0;
    pub const LESS: u16 = 1;
    pub const EQUAL: u16 = 2;
    pub const LESS_EQUAL: u16 = 3;
    pub const GREATER: u16 = 4;
    pub const NOT_EQUAL: u16 = 5;
    pub const GREATER_EQUAL: u16 = 6;
    pub const NEVER: u16 = 7;
}

/// The `NiAVObject` flag word.
pub mod av_flags {
    pub const APP_CULLED: u16 = 0x0001;
    /// A `propagate_mode` value.
    pub const PROPAGATE_MODE_MASK: u16 = 0x0006;
    pub const PROPAGATE_MODE_SHIFT: u16 = 1;
    pub const VISUAL: u16 = 0x0008;
}

/// The `NiTimeController` flag word.
pub mod controller_flags {
    /// A `cycle_type` value.
    pub const CYCLE_TYPE_MASK: u16 = 0x0006;
    pub const CYCLE_TYPE_SHIFT: u16 = 1;
    pub const ACTIVE: u16 = 0x0008;
}

/// The `NiAlphaProperty` flag word.
pub mod alpha_property_flags {
    pub const ALPHA_BLENDING: u16 = 0x0001;
    /// An `alpha_blend_function` value.
    pub const SRC_BLEND_MODE_MASK: u16 = 0x001E;
    pub const SRC_BLEND_MODE_SHIFT: u16 = 1;
    /// An `alpha_blend_function` value.
    pub const DST_BLEND_MODE_MASK: u16 = 0x01E0;
    pub const DST_BLEND_MODE_SHIFT: u16 = 5;
    pub const ALPHA_TESTING: u16 = 0x0200;
    /// An `alpha_test_function` value.
    pub const TEST_MODE_MASK: u16 = 0x1C00;
    pub const TEST_MODE_SHIFT: u16 = 10;
}

/// The `NiBSAnimationNode` flag word, on top of `av_flags`.
pub mod bs_animation_node_flags {
    pub const ANIMATED: u16 = 0x0020;
    pub const NOT_RANDOM: u16 = 0x0040;
}

/// The `NiBSPArrayController` flag word, on top of `controller_flags`.
pub mod bsp_array_controller_flags {
    pub const AT_VERTICES: u16 = 0x0010;
}

/// The `NiBSParticleNode` flag word, on top of `av_flags`.
pub mod bs_particle_node_flags {
    pub const FOLLOW: u16 = 0x0080;
}

/// The `NiCollisionSwitch` flag word, on top of `av_flags`.
pub mod collision_switch_flags {
    pub const PROPAGATE: u16 = 0x0020;
}

/// The `NiFltAnimationNode` flag word, on top of `av_flags` and `switch_node_flags`.
pub mod flt_animation_node_flags {
    pub const BOUNCE: u16 = 0x0040;
}

/// The `NiGeometry` flag word, on top of `av_flags`.
pub mod geometry_flags {
    pub const COMPRESS_VERTICES: u16 = 0x0008;
    pub const COMPRESS_NORMALS: u16 = 0x0010;
    pub const COMPRESS_UV_SETS: u16 = 0x0020;
    pub const SHADOW: u16 = 0x0040;
}

/// The `NiLightColorController` flag word, on top of `controller_flags`.
pub mod light_color_controller_flags {
    pub const AMBIENT: u16 = 0x0010;
}

/// The `NiLookAtController` flag word, on top of `controller_flags`.
pub mod look_at_controller_flags {
    pub const FLIP: u16 = 0x0010;
    /// An `axis` value.
    pub const AXIS_MASK: u16 = 0x0060;
    pub const AXIS_SHIFT: u16 = 5;
}

/// The `NiSwitchNode` flag word, on top of `av_flags`.
pub mod switch_node_flags {
    pub const UPDATE_ONLY_ACTIVE: u16 = 0x0020;
}

/// The `NiWireframeProperty` flag word.
pub mod wireframe_property_flags {
    pub const WIREFRAME: u16 = 0x0001;
}

/// The `NiZBufferProperty` flag word.
pub mod zbuffer_property_flags {
    pub const Z_BUFFER_TEST: u16 = 0x0001;
    pub const Z_BUFFER_WRITE: u16 = 0x0002;
    /// A `zbuffer_test_function` value.
    pub const TEST_FUNCTION_MASK: u16 = 0x003C;
    pub const TEST_FUNCTION_SHIFT: u16 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shifted_fields_fit_their_masks() {
        // The largest value of each field, shifted into place, stays inside its mask.
        fn fits(value: u16, shift: u16, mask: u16) -> bool {
            (value << shift) & !mask == 0
        }
        use alpha_property_flags as ap;
        use av_flags as av;
        use controller_flags as cf;
        use look_at_controller_flags as la;
        use zbuffer_property_flags as zb;
        assert!(fits(propagate_mode::CONTINUE, av::PROPAGATE_MODE_SHIFT, av::PROPAGATE_MODE_MASK));
        assert!(fits(cycle_type::CLAMP, cf::CYCLE_TYPE_SHIFT, cf::CYCLE_TYPE_MASK));
        assert!(fits(alpha_blend_function::SRC_ALPHA_SAT, ap::SRC_BLEND_MODE_SHIFT, ap::SRC_BLEND_MODE_MASK));
        assert!(fits(alpha_blend_function::SRC_ALPHA_SAT, ap::DST_BLEND_MODE_SHIFT, ap::DST_BLEND_MODE_MASK));
        assert!(fits(alpha_test_function::NEVER, ap::TEST_MODE_SHIFT, ap::TEST_MODE_MASK));
        assert!(fits(axis::Z, la::AXIS_SHIFT, la::AXIS_MASK));
        assert!(fits(zbuffer_test_function::NEVER, zb::TEST_FUNCTION_SHIFT, zb::TEST_FUNCTION_MASK));
        assert_eq!(key_type::TCB, 3);
        assert_eq!(bound_type::UNION, 4);
    }
}
