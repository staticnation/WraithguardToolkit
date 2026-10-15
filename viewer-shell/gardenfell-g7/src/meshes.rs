//! Meshes: G7's `tes3::nif` stream into Gardenfell's `netimmerse::stream::Stream`.
//!
//! G7's crate reads the file; this converts every block to Robin's type of the same name,
//! field by field ([`meshes_gen`], generated from both crates' type definitions by
//! `tools/gen_g7_nif.py`), so Robin's mesh code runs unchanged on G7's parse
//! (`gardenfell_nif::nif::set_stream_reader`, patch 0002). The two models are the same
//! layouts; what differs is spelling:
//!
//! - G7 keeps vectors, matrices and quaternions as glam types, Robin as arrays: a matrix's
//!   rows (the file's order) are glam's columns, as G7 loads the nine floats straight into
//!   glam's column storage; a quaternion is `w, x, y, z` in the file and in Robin's array,
//!   where glam keeps `x, y, z, w`.
//! - G7 names its enums, Robin keeps their numbers.
//! - A link is a slot-map key in G7 and a block index in Robin's.
//!
//! A file with a block type this does not convert, or one G7's crate does not read (its
//! colour keys are linear only), is read by Robin's own reader instead: the answer is the
//! same model either way.

use std::collections::HashMap;

use gardenfell_nif::netimmerse::stream::{Link, Stream, HEADER, VERSION};
use gardenfell_nif::netimmerse::types as r;
use tes3::nif as g;
use tes3::nif::glam;

#[path = "meshes_gen.rs"]
mod meshes_gen;

/// What a conversion needs besides the value: where each of G7's blocks sits in the file.
pub struct Cx {
    index: HashMap<g::NiKey, i32>,
}

/// A G7 value as Robin's.
pub trait Conv<T> {
    fn conv(&self, cx: &Cx) -> T;
}

macro_rules! same {
    ($($t:ty),*) => { $(impl Conv<$t> for $t { fn conv(&self, _: &Cx) -> $t { self.clone() } })* };
}
same!(f32, f64, u8, i8, u16, i16, u32, i32, u64, i64, bool, String);

impl Conv<u32> for usize {
    fn conv(&self, _: &Cx) -> u32 {
        *self as u32
    }
}

impl<A: Conv<B>, B> Conv<Vec<B>> for Vec<A> {
    fn conv(&self, cx: &Cx) -> Vec<B> {
        self.iter().map(|x| x.conv(cx)).collect()
    }
}

impl<A: Conv<B>, B> Conv<Option<B>> for Option<A> {
    fn conv(&self, cx: &Cx) -> Option<B> {
        self.as_ref().map(|x| x.conv(cx))
    }
}

impl<A: Conv<B>, B, const N: usize> Conv<[B; N]> for [A; N] {
    fn conv(&self, cx: &Cx) -> [B; N] {
        std::array::from_fn(|i| self[i].conv(cx))
    }
}

impl<A: Conv<B>, B, C: Conv<D>, D> Conv<(B, D)> for (A, C) {
    fn conv(&self, cx: &Cx) -> (B, D) {
        (self.0.conv(cx), self.1.conv(cx))
    }
}

impl<A: Conv<B>, B> Conv<Box<B>> for Box<A> {
    fn conv(&self, cx: &Cx) -> Box<B> {
        Box::new((**self).conv(cx))
    }
}

impl Conv<[f32; 2]> for glam::Vec2 {
    fn conv(&self, _: &Cx) -> [f32; 2] {
        self.to_array()
    }
}

impl Conv<[f32; 3]> for glam::Vec3 {
    fn conv(&self, _: &Cx) -> [f32; 3] {
        self.to_array()
    }
}

impl Conv<[f32; 4]> for glam::Vec4 {
    fn conv(&self, _: &Cx) -> [f32; 4] {
        self.to_array()
    }
}

/// The file's (and Robin's) order, `w, x, y, z`.
impl Conv<[f32; 4]> for glam::Quat {
    fn conv(&self, _: &Cx) -> [f32; 4] {
        [self.w, self.x, self.y, self.z]
    }
}

/// The file's rows are glam's columns (G7 loads the floats in order into column storage).
impl Conv<[[f32; 3]; 3]> for glam::Mat3 {
    fn conv(&self, _: &Cx) -> [[f32; 3]; 3] {
        self.to_cols_array_2d()
    }
}

impl Conv<[[f32; 2]; 2]> for glam::Mat2 {
    fn conv(&self, _: &Cx) -> [[f32; 2]; 2] {
        self.to_cols_array_2d()
    }
}

impl<A, B> Conv<Link<B>> for g::NiLink<A> {
    fn conv(&self, cx: &Cx) -> Link<B> {
        Link::new(cx.index.get(&self.key).copied().unwrap_or(-1))
    }
}

/// G7's fieldless enums, as the numbers Robin keeps.
macro_rules! repr_enums {
    ($($e:ident),* $(,)?) => {
        $(
            impl crate::meshes::Conv<i32> for tes3::nif::$e {
                fn conv(&self, _: &crate::meshes::Cx) -> i32 { *self as i32 }
            }
            impl crate::meshes::Conv<u32> for tes3::nif::$e {
                fn conv(&self, _: &crate::meshes::Cx) -> u32 { *self as u32 }
            }
            impl crate::meshes::Conv<u16> for tes3::nif::$e {
                fn conv(&self, _: &crate::meshes::Cx) -> u16 { *self as u16 }
            }
            impl crate::meshes::Conv<u8> for tes3::nif::$e {
                fn conv(&self, _: &crate::meshes::Cx) -> u8 { *self as u8 }
            }
        )*
    };
}
pub(crate) use repr_enums;

/* The bounding volume: G7 wraps the kinds in a struct, and its sphere in an `NiBound`. */

impl Conv<r::NiBoundingVolume> for g::NiBoundingVolume {
    fn conv(&self, cx: &Cx) -> r::NiBoundingVolume {
        match &self.bound_data {
            g::BoundData::NiBoxBV(b) => r::NiBoundingVolume::Box(r::NiBoxBV {
                center: b.center.conv(cx),
                axis: b.axis.conv(cx),
                extents: b.extents.conv(cx),
            }),
            g::BoundData::NiSphereBV(s) => r::NiBoundingVolume::Sphere(r::NiSphereBV {
                center: s.bound.center.conv(cx),
                radius: s.bound.radius,
            }),
            g::BoundData::NiUnionBV(u) => r::NiBoundingVolume::Union(r::NiUnionBV {
                bounding_volumes: u.bounding_volumes.conv(cx),
            }),
        }
    }
}

/// The engine's mesh hook: G7's crate reads the file and every block is converted; a file
/// either does not take is read by Robin's reader (the same model either way).
pub fn read_stream(buf: &[u8]) -> Result<Stream, String> {
    let buf = as_4002(buf);
    match g7_stream(&buf) {
        Some(s) => Ok(s),
        None => Stream::parse(&buf).map_err(|e| e.to_string()),
    }
}

/// Keep Wraithguard 4.3's compatibility with Morrowind's 4.0.0.0 meshes: their
/// layout is the same as 4.0.0.2, so both readers can use the newer header.
fn as_4002(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const V4000: &[u8] = b"NetImmerse File Format, Version 4.0.0.0\n";
    let at = HEADER.len();
    if bytes.len() >= at + 4
        && (&bytes[..at] == V4000 || &bytes[..at] == HEADER.as_bytes())
        && bytes[at..at + 4] == 0x0400_0000u32.to_le_bytes()
    {
        let mut normalized = bytes.to_vec();
        normalized[..at].copy_from_slice(HEADER.as_bytes());
        normalized[at..at + 4].copy_from_slice(&VERSION.to_le_bytes());
        return std::borrow::Cow::Owned(normalized);
    }
    std::borrow::Cow::Borrowed(bytes)
}

/// G7's parse converted, or None when it does not read the file or a block is one this
/// does not convert.
pub fn g7_stream(buf: &[u8]) -> Option<Stream> {
    // Robin's checks, so a file is refused for the same reasons: the header line, then
    // exactly version 4.0.0.2.
    let line_end = buf.iter().take(64).position(|&b| b == b'\n')?;
    let header = String::from_utf8_lossy(&buf[..=line_end]).into_owned();
    if !header.starts_with("NetImmerse File Format") {
        return None;
    }
    let version = u32::from_le_bytes(buf.get(line_end + 1..line_end + 5)?.try_into().ok()?);
    if version != VERSION {
        return None;
    }
    // G7's reader wants the standard header line.
    let bytes: std::borrow::Cow<[u8]> = if header == HEADER {
        std::borrow::Cow::Borrowed(buf)
    } else {
        let mut b = HEADER.as_bytes().to_vec();
        b.extend_from_slice(&buf[line_end + 1..]);
        std::borrow::Cow::Owned(b)
    };
    let s = std::panic::catch_unwind(|| g::NiStream::from_bytes(&bytes).ok())
        .ok()
        .flatten()?;
    let cx = Cx {
        index: s
            .objects
            .keys()
            .enumerate()
            .map(|(i, k)| (k, i as i32))
            .collect(),
    };
    let mut blocks = Vec::with_capacity(s.objects.len());
    for t in s.objects.values() {
        blocks.push(meshes_gen::block(t, &cx)?);
    }
    let roots = s.roots.iter().map(|l| l.conv(&cx)).collect();
    Some(Stream {
        header,
        version,
        blocks,
        roots,
    })
}

/// The base every block has: no fields on either side.
impl Conv<r::NiObject> for g::NiObject {
    fn conv(&self, _: &Cx) -> r::NiObject {
        r::NiObject
    }
}
