//! One struct per block type, every one of the 112 the format has, and the shared
//! pieces (bounds, keys, pixel formats) that several of them carry.
//!
//! Round 18gg. The layouts are `tes3`'s (`libs/nif/src/types/*.rs`), transcribed with
//! these standing differences:
//!
//! - Links are block indices ([`Link`]), as the file holds them. `tes3` keys its blocks
//!   in a slot map and writes them back in root-first order; here a stream writes the
//!   blocks it holds in the order it holds them, so a file comes back byte for byte.
//! - Vectors, matrices and quaternions are plain arrays in the file's own order (`[x, y,
//!   z]`, a 3x3 row by row, `[w, x, y, z]`). `tes3` uses `glam`, whose `Mat3` is column
//!   major, so its matrices are the transpose of what is here.
//! - Enumerations are the integer the file holds, with the known values as named
//!   constants in [`consts`], and flag words are plain integers; an unknown value stays.
//! - A parent type is a `base` field, as `tes3` does it: `NiNode.base` is the
//!   `NiAVObject`, whose `base` is the `NiObjectNET`, whose `base` is the `NiObject`.
//!   Every type in the family is a [`Block`], the abstract ones too, since a file may
//!   carry them.
//! - `NiParticleSystemController` is read in nif.xml's order (the particle count, the
//!   active count, then the particles), which `backend-audit.md` D14 checked against
//!   the game's own `ashcloud.nif`; `tes3` has the active count after the particles.
//!
//! The per-block files are named for the block in lower case (`ninode.rs`, `nitrishapedata.rs`).

use crate::bytes::{Reader, Result, Writer};

pub use super::stream::{Link, NifRead, NifWrite};

pub mod consts;

mod avoidnode;
mod brickniextradata;
mod bsmirrorednode;
mod niaccumulator;
mod nialphaaccumulator;
mod nialphacontroller;
mod nialphaproperty;
mod niambientlight;
mod niautonormalparticles;
mod niautonormalparticlesdata;
mod niavobject;
mod nibillboardnode;
mod nibltsource;
mod nibound;
mod niboundingvolume;
mod niboxbv;
mod nibsanimationmanager;
mod nibsanimationnode;
mod nibsparraycontroller;
mod nibsparticlenode;
mod nibspnode;
mod nicamera;
mod niclusteraccumulator;
mod nicollisionswitch;
mod nicolordata;
mod nicolorkey;
mod nidirectionallight;
mod niditherproperty;
mod nidx8renderer;
mod nidynamiceffect;
mod niemittermodifier;
mod niextradata;
mod niflipcontroller;
mod nifloatcontroller;
mod nifloatdata;
mod nifloatkey;
mod nifltanimationnode;
mod nifogproperty;
mod nigeometry;
mod nigeometrydata;
mod nigeommorphercontroller;
mod nigravity;
mod nikeyframecontroller;
mod nikeyframedata;
mod nikeyframemanager;
mod nilight;
mod nilightcolorcontroller;
mod nilines;
mod nilinesdata;
mod nilodnode;
mod nilookatcontroller;
mod nimaterialcolorcontroller;
mod nimaterialproperty;
mod nimorphdata;
mod nimorphercontroller;
mod ninode;
mod niobject;
mod niobjectnet;
mod nipalette;
mod niparticlebomb;
mod niparticlecollider;
mod niparticlecolormodifier;
mod niparticlegrowfade;
mod niparticlemodifier;
mod niparticlerotation;
mod niparticles;
mod niparticlesdata;
mod niparticlesystemcontroller;
mod nipathcontroller;
mod niperparticledata;
mod nipixeldata;
mod nipixelformat;
mod niplanarcollider;
mod nipointlight;
mod niposdata;
mod niposkey;
mod niproperty;
mod nirenderedcubemap;
mod nirenderedtexture;
mod nirenderer;
mod nirollcontroller;
mod nirotatingparticles;
mod nirotatingparticlesdata;
mod nirotdata;
mod nirotkey;
mod niscreenpolygon;
mod nisequence;
mod nisequencestreamhelper;
mod nishadeproperty;
mod niskindata;
mod niskininstance;
mod niskinpartition;
mod nisortadjustnode;
mod nisourcetexture;
mod nispecularproperty;
mod nispherebv;
mod nisphericalcollider;
mod nispotlight;
mod nistencilproperty;
mod nistringextradata;
mod niswitchnode;
mod nitextkeyextradata;
mod nitexture;
mod nitextureeffect;
mod nitexturingproperty;
mod nitimecontroller;
mod nitribasedgeom;
mod nitribasedgeomdata;
mod nitrishape;
mod nitrishapedata;
mod nitrishapedynamicdata;
mod nitristrips;
mod nitristripsdata;
mod niunionbv;
mod niuvcontroller;
mod niuvdata;
mod nivertexcolorproperty;
mod nivertweightsextradata;
mod niviscontroller;
mod nivisdata;
mod niwireframeproperty;
mod nizbufferproperty;
mod rootcollisionnode;
mod tes3objectextradata;

pub use avoidnode::*;
pub use brickniextradata::*;
pub use bsmirrorednode::*;
pub use niaccumulator::*;
pub use nialphaaccumulator::*;
pub use nialphacontroller::*;
pub use nialphaproperty::*;
pub use niambientlight::*;
pub use niautonormalparticles::*;
pub use niautonormalparticlesdata::*;
pub use niavobject::*;
pub use nibillboardnode::*;
pub use nibltsource::*;
pub use nibound::*;
pub use niboundingvolume::*;
pub use niboxbv::*;
pub use nibsanimationmanager::*;
pub use nibsanimationnode::*;
pub use nibsparraycontroller::*;
pub use nibsparticlenode::*;
pub use nibspnode::*;
pub use nicamera::*;
pub use niclusteraccumulator::*;
pub use nicollisionswitch::*;
pub use nicolordata::*;
pub use nicolorkey::*;
pub use nidirectionallight::*;
pub use niditherproperty::*;
pub use nidx8renderer::*;
pub use nidynamiceffect::*;
pub use niemittermodifier::*;
pub use niextradata::*;
pub use niflipcontroller::*;
pub use nifloatcontroller::*;
pub use nifloatdata::*;
pub use nifloatkey::*;
pub use nifltanimationnode::*;
pub use nifogproperty::*;
pub use nigeometry::*;
pub use nigeometrydata::*;
pub use nigeommorphercontroller::*;
pub use nigravity::*;
pub use nikeyframecontroller::*;
pub use nikeyframedata::*;
pub use nikeyframemanager::*;
pub use nilight::*;
pub use nilightcolorcontroller::*;
pub use nilines::*;
pub use nilinesdata::*;
pub use nilodnode::*;
pub use nilookatcontroller::*;
pub use nimaterialcolorcontroller::*;
pub use nimaterialproperty::*;
pub use nimorphdata::*;
pub use nimorphercontroller::*;
pub use ninode::*;
pub use niobject::*;
pub use niobjectnet::*;
pub use nipalette::*;
pub use niparticlebomb::*;
pub use niparticlecollider::*;
pub use niparticlecolormodifier::*;
pub use niparticlegrowfade::*;
pub use niparticlemodifier::*;
pub use niparticlerotation::*;
pub use niparticles::*;
pub use niparticlesdata::*;
pub use niparticlesystemcontroller::*;
pub use nipathcontroller::*;
pub use niperparticledata::*;
pub use nipixeldata::*;
pub use nipixelformat::*;
pub use niplanarcollider::*;
pub use nipointlight::*;
pub use niposdata::*;
pub use niposkey::*;
pub use niproperty::*;
pub use nirenderedcubemap::*;
pub use nirenderedtexture::*;
pub use nirenderer::*;
pub use nirollcontroller::*;
pub use nirotatingparticles::*;
pub use nirotatingparticlesdata::*;
pub use nirotdata::*;
pub use nirotkey::*;
pub use niscreenpolygon::*;
pub use nisequence::*;
pub use nisequencestreamhelper::*;
pub use nishadeproperty::*;
pub use niskindata::*;
pub use niskininstance::*;
pub use niskinpartition::*;
pub use nisortadjustnode::*;
pub use nisourcetexture::*;
pub use nispecularproperty::*;
pub use nispherebv::*;
pub use nisphericalcollider::*;
pub use nispotlight::*;
pub use nistencilproperty::*;
pub use nistringextradata::*;
pub use niswitchnode::*;
pub use nitextkeyextradata::*;
pub use nitexture::*;
pub use nitextureeffect::*;
pub use nitexturingproperty::*;
pub use nitimecontroller::*;
pub use nitribasedgeom::*;
pub use nitribasedgeomdata::*;
pub use nitrishape::*;
pub use nitrishapedata::*;
pub use nitrishapedynamicdata::*;
pub use nitristrips::*;
pub use nitristripsdata::*;
pub use niunionbv::*;
pub use niuvcontroller::*;
pub use niuvdata::*;
pub use nivertexcolorproperty::*;
pub use nivertweightsextradata::*;
pub use niviscontroller::*;
pub use nivisdata::*;
pub use niwireframeproperty::*;
pub use nizbufferproperty::*;
pub use rootcollisionnode::*;
pub use tes3objectextradata::*;

/// A block type: its name as the file spells it, and the two directions. The body is
/// read after the type name has been, and written without it; [`NiType`] adds the name.
pub trait Block: Sized + Default {
    const NAME: &'static str;
    fn read(r: &mut Reader<'_>) -> Result<Self>;
    fn write(&self, w: &mut Writer) -> Result<()>;
    /// Calls `f` on every link the block holds, its parents' included, so a stream can
    /// renumber its blocks (`Stream::reorder`). A link of `-1` is passed too; `f` leaves
    /// it alone.
    fn links(&mut self, f: &mut dyn FnMut(&mut i32));
}

macro_rules! ni_types {
    ($($var:ident),* $(,)?) => {
        /// Any block the format has.
        #[derive(Clone, Debug, PartialEq)]
        pub enum NiType {
            $($var($var),)*
        }

        /// Every block type name the format has, in `tes3`'s order.
        pub const NAMES: &[&str] = &[$($var::NAME),*];

        impl NiType {
            /// Reads the block whose type name was just read. An error for a name the
            /// format does not have.
            pub fn read(name: &str, r: &mut Reader<'_>) -> Result<NiType> {
                $(if name == $var::NAME { return Ok(NiType::$var($var::read(r)?)); })*
                r.error(format!("{name:?} is not a block type the format has"))
            }
            /// Writes the body, not the type name.
            pub fn write(&self, w: &mut Writer) -> Result<()> {
                match self { $(NiType::$var(x) => x.write(w),)* }
            }
            pub fn name(&self) -> &'static str {
                match self { $(NiType::$var(_) => $var::NAME,)* }
            }
            /// Every link the block holds; see [`Block::links`].
            pub fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
                match self { $(NiType::$var(x) => x.links(f),)* }
            }
        }

        $(
            impl From<$var> for NiType {
                fn from(x: $var) -> NiType { NiType::$var(x) }
            }
            impl<'a> TryFrom<&'a NiType> for &'a $var {
                type Error = ();
                fn try_from(t: &'a NiType) -> std::result::Result<&'a $var, ()> {
                    match t { NiType::$var(x) => Ok(x), _ => Err(()) }
                }
            }
            impl<'a> TryFrom<&'a mut NiType> for &'a mut $var {
                type Error = ();
                fn try_from(t: &'a mut NiType) -> std::result::Result<&'a mut $var, ()> {
                    match t { NiType::$var(x) => Ok(x), _ => Err(()) }
                }
            }
        )*
    };
}

ni_types! {
    AvoidNode,
    BrickNiExtraData,
    BSMirroredNode,
    NiAccumulator,
    NiAlphaAccumulator,
    NiAlphaController,
    NiAlphaProperty,
    NiAmbientLight,
    NiAutoNormalParticles,
    NiAutoNormalParticlesData,
    NiAVObject,
    NiBillboardNode,
    NiBltSource,
    NiBSAnimationManager,
    NiBSAnimationNode,
    NiBSPArrayController,
    NiBSParticleNode,
    NiBSPNode,
    NiCamera,
    NiClusterAccumulator,
    NiCollisionSwitch,
    NiColorData,
    NiDirectionalLight,
    NiDitherProperty,
    NiDX8Renderer,
    NiDynamicEffect,
    NiEmitterModifier,
    NiExtraData,
    NiFlipController,
    NiFloatController,
    NiFloatData,
    NiFltAnimationNode,
    NiFogProperty,
    NiGeometry,
    NiGeometryData,
    NiGeomMorpherController,
    NiGravity,
    NiKeyframeController,
    NiKeyframeData,
    NiKeyframeManager,
    NiLight,
    NiLightColorController,
    NiLines,
    NiLinesData,
    NiLODNode,
    NiLookAtController,
    NiMaterialColorController,
    NiMaterialProperty,
    NiMorphData,
    NiMorpherController,
    NiNode,
    NiObject,
    NiObjectNET,
    NiPalette,
    NiParticleBomb,
    NiParticleCollider,
    NiParticleColorModifier,
    NiParticleGrowFade,
    NiParticleModifier,
    NiParticleRotation,
    NiParticles,
    NiParticlesData,
    NiParticleSystemController,
    NiPathController,
    NiPixelData,
    NiPlanarCollider,
    NiPointLight,
    NiPosData,
    NiProperty,
    NiRenderedCubeMap,
    NiRenderedTexture,
    NiRenderer,
    NiRollController,
    NiRotatingParticles,
    NiRotatingParticlesData,
    NiRotData,
    NiScreenPolygon,
    NiSequenceStreamHelper,
    NiShadeProperty,
    NiSkinData,
    NiSkinInstance,
    NiSkinPartition,
    NiSortAdjustNode,
    NiSourceTexture,
    NiSpecularProperty,
    NiSphericalCollider,
    NiSpotLight,
    NiStencilProperty,
    NiStringExtraData,
    NiSwitchNode,
    NiTextKeyExtraData,
    NiTexture,
    NiTextureEffect,
    NiTexturingProperty,
    NiTimeController,
    NiTriBasedGeom,
    NiTriBasedGeomData,
    NiTriShape,
    NiTriShapeData,
    NiTriShapeDynamicData,
    NiTriStrips,
    NiTriStripsData,
    NiUVController,
    NiUVData,
    NiVertexColorProperty,
    NiVertWeightsExtraData,
    NiVisController,
    NiVisData,
    NiWireframeProperty,
    NiZBufferProperty,
    RootCollisionNode,
    TES3ObjectExtraData,
}

impl Default for NiType {
    fn default() -> NiType {
        NiType::NiObject(NiObject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_table_has_every_type_once() {
        assert_eq!(NAMES.len(), 112);
        let mut seen = std::collections::BTreeSet::new();
        for n in NAMES {
            assert!(seen.insert(*n), "{n} twice");
        }
        assert!(NAMES.contains(&"NiNode"));
        assert!(NAMES.contains(&"RootCollisionNode"));
    }

    #[test]
    fn an_unknown_type_name_is_refused_by_name() {
        let mut r = Reader::new(&[]);
        let e = NiType::read("NiNothing", &mut r).unwrap_err();
        assert!(e.what.contains("\"NiNothing\""), "{e}");
    }
}
