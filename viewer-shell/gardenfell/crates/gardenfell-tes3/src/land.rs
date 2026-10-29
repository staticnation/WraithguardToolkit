//! Landscape decoding and terrain sampling.
//!
//! A LAND record carries a 65x65 height grid stored as running-sum deltas, a 65x65
//! grid of authored vertex normals, and a 16x16 texture grid stored in 4x4 blocks of
//! 4x4. All are decoded once per cell into flat arrays the sampler can index without
//! branching.
//!
//! # The surface
//!
//! **Morrowind's ground is triangles, and every land quad is split along the same
//! diagonal: from (x+1, y) to (x, y+1)** — south-east corner to north-west. Not a
//! bilinear patch, and not a checkerboard.
//!
//! That is measured, not assumed. A blade placed by Groundcover Generator sits at
//! exactly `ground + iZPositionModifier`, which turns "which surface" into a countable
//! question; 152,911 references from a real vanilla run answer it. On flat quads every
//! candidate surface agrees and 100% of blades land exactly, so the noise floor is zero.
//! On the rest:
//!
//! | surface | blades at exactly ground + z_mod |
//! |---|---|
//! | **this one** | **100.00 %** |
//! | checkerboard, `(row+col)&1` | 69.67 % |
//! | bilinear | 41.95 % |
//! | the other fixed diagonal | 39.28 % |
//!
//! The bilinear sampler this replaced was out by up to 320 units on real vanilla
//! terrain, worst exactly where terrain is most dramatic — the error term is
//! `|h00+h11-h01-h10|/4`, linear in the VHGT deltas. memory §11a has the full numbers
//! and the per-region breakdown. Do not "simplify" this back to a bilinear tap.

use crate::tes3::bytes::Reader;
use crate::tes3::types::{TextureIndices, VertexColors, VertexHeights, VertexNormals};
use glam::Vec3;

pub const VERTS: usize = 65;
/// The height the game gives a cell whose LAND record carries no VHGT: flat ground at
/// -2048, under the sea (OpenMW `ESM::Land::DEFAULT_HEIGHT`, loadland.hpp). Round 18gf.
pub const DEFAULT_HEIGHT: f32 = -2048.0;
pub const SUBDIV: usize = 16;
pub const HEIGHT_SCALE: f32 = 8.0;
pub const CELL_SIZE: f32 = 8192.0;
/// Distance between adjacent height samples.
pub const VERT_STEP: f32 = CELL_SIZE / (VERTS as f32 - 1.0);

#[derive(Clone)]
pub struct Land {
    pub grid: (i32, i32),
    pub heights: Box<[f32; VERTS * VERTS]>,
    /// The record's own per-vertex normals, when it has them — **the three signed
    /// bytes VNML holds**, not floats. `None` for a plugin that edits heights without
    /// rewriting VNML, and for the synthetic patch — both fall back to the normal of
    /// the triangle the point is standing on.
    ///
    /// Kept as bytes because a loaded world is memory-bound, not arithmetic-bound.
    /// Decoding VNML costs 14 ms across vanilla's 1,390 records; *keeping* the world's
    /// landscape costs 442 ms, and three f32 per vertex was 67 MB of the 89 MB held.
    /// The same grid as bytes is 17 MB. `normal_at` already takes a square root, so
    /// the three casts and the divide that turn a byte triple into a unit vector are
    /// lost in the noise of a sample that was going to normalise anyway.
    pub normals: Option<Box<[i8; VERTS * VERTS * 3]>>,
    /// The record's vertex colours — **VCLR, 65×65 RGB bytes, as stored** — when it has
    /// them. Round 11 item 11. The game multiplies the ground texture by these, and
    /// they are where most of a cell's darkening under cliffs, the brown of a worn path
    /// and the bleach of a beach actually come from; a landscape drawn without them is
    /// flatter and brighter than the one the player sees. `None` (drawn as white) for a
    /// record without the block. Bytes for the same reason `normals` are: 12 KB a cell
    /// against 50 KB as floats, and a colour is only ever read at one vertex at a time.
    ///
    /// Read for the preview only. Nothing in placement consumes it — a FRMR carries no
    /// colour, so there is nothing a rule could write about it.
    pub colours: Option<Box<[u8; VERTS * VERTS * 3]>>,
    pub vtex: Box<[u16; SUBDIV * SUBDIV]>,
    /// Load-order index of the plugin that wrote this record. LTEX indices are
    /// plugin-local, so a VTEX value is meaningless without it.
    pub plugin: i32,
}

/// VTEX is stored as 4x4 blocks of 4x4 cells; flatten into a plain 16x16 grid.
pub fn unswizzle_vtex(src: &[u16; 256]) -> Box<[u16; 256]> {
    let mut out = Box::new([0u16; 256]);
    let mut r = 0usize;
    for y1 in 0..4 {
        for x1 in 0..4 {
            for y2 in 0..4 {
                for x2 in 0..4 {
                    out[(y1 * 4 + y2) * 16 + (x1 * 4 + x2)] = src[r];
                    r += 1;
                }
            }
        }
    }
    out
}

/// VHGT is a base float followed by signed byte deltas: the first column
/// accumulates down the rows, and each row accumulates across from it.
///
/// Round 18gh: from the typed block (`tes3::types::VertexHeights`), which holds the base
/// and the 65 by 65 deltas as the file does; the running sums are the same operations in
/// the same order as the byte decoder this replaced, so every height comes out the same.
pub fn heights_of(v: &VertexHeights) -> Box<[f32; VERTS * VERTS]> {
    let mut h = Box::new([0f32; VERTS * VERTS]);
    let mut row_acc = v.offset;
    for (y, row) in v.data.iter().enumerate() {
        row_acc += row[0] as f32;
        let mut acc = row_acc;
        h[y * VERTS] = acc * HEIGHT_SCALE;
        for (x, d) in row.iter().enumerate().skip(1) {
            acc += *d as f32;
            h[y * VERTS + x] = acc * HEIGHT_SCALE;
        }
    }
    h
}

/// VNML is 65x65 triples of signed bytes, and that is how they are kept: the record's
/// own bytes, copied, with the length check the rest of the reader can then skip.
///
/// This used to unpack to `[[f32; 3]; 4225]` here, which is four times the memory for
/// a value every sampler normalises again anyway. See `Land::normals`.
///
/// Round 18gh: the engine reads the block through the typed model and converts it with
/// [`normals_of`]; this is that conversion for bytes, kept for the tests that build a
/// VNML by hand.
pub fn decode_vnml(data: &[u8]) -> Option<Box<[i8; VERTS * VERTS * 3]>> {
    if data.len() < VERTS * VERTS * 3 {
        return None;
    }
    let v = VertexNormals::read(&mut Reader::new(data)).ok()?;
    normals_of(&v)
}

/// The engine's normals from the typed `VNML` block, or `None` for a blank one.
pub fn normals_of(v: &VertexNormals) -> Option<Box<[i8; VERTS * VERTS * 3]>> {
    /* Round 18bf (I3): a block of nothing but zeros is **no VNML**, not a VNML that says
       every vertex points straight up.

       A zero triple has no direction; `unit_vnml` reads one as up so that a stray zero
       among real normals cannot become a NaN, and a *whole block* of them used to inherit
       that reading — so a cell with a blank VNML reported perfectly flat ground, every
       slope rule in it was inert, and blades stood bolt upright on a 45-degree hillside.
       The doc on `normal_at` describes the fallback to the triangle's own normal; `Some`
       here is what kept it from ever being reached. Declining the block is the whole fix:
       `Land::normals` of `None` already means "work it out from the heights" everywhere.

    One pass over 12,675 bytes at load, and it stops at the first non-zero byte in
    every record that has real normals — which is nearly all of them. */
    let mut all = v.data.iter().flatten().flatten();
    if all.all(|b| *b == 0) {
        return None;
    }
    let mut out = Box::new([0i8; VERTS * VERTS * 3]);
    for (dst, row) in out.chunks_exact_mut(3 * VERTS).zip(v.data.iter()) {
        for (d, s) in dst.chunks_exact_mut(3).zip(row.iter()) {
            for (x, y) in d.iter_mut().zip(s) {
                *x = *y;
            }
        }
    }
    Some(out)
}

/// VCLR is 65x65 RGB byte triples, kept as the record has them (see `Land::colours`).
/// Round 18gh: the bytes form of [`colours_of`], kept for the tests.
pub fn decode_vclr(data: &[u8]) -> Option<Box<[u8; VERTS * VERTS * 3]>> {
    if data.len() < VERTS * VERTS * 3 {
        return None;
    }
    let v = VertexColors::read(&mut Reader::new(data)).ok()?;
    Some(colours_of(&v))
}

/// The engine's vertex colours from the typed `VCLR` block.
pub fn colours_of(v: &VertexColors) -> Box<[u8; VERTS * VERTS * 3]> {
    let mut out = Box::new([0u8; VERTS * VERTS * 3]);
    for (dst, row) in out.chunks_exact_mut(3 * VERTS).zip(v.data.iter()) {
        for (d, s) in dst.chunks_exact_mut(3).zip(row.iter()) {
            for (x, y) in d.iter_mut().zip(s) {
                *x = *y;
            }
        }
    }
    out
}

/// The engine's 16x16 texture grid from the typed `VTEX` block, which holds the indices
/// in file order (the 4x4 blocks of 4x4), unswizzled here as they always were.
pub fn vtex_of(v: &TextureIndices) -> Box<[u16; SUBDIV * SUBDIV]> {
    let mut raw = [0u16; SUBDIV * SUBDIV];
    for (slot, t) in raw.iter_mut().zip(v.data.iter().flatten()) {
        *slot = *t;
    }
    unswizzle_vtex(&raw)
}

/// One authored vertex normal, unit length.
///
/// Zero-length vectors happen in the wild — a record with a blank VNML block is not
/// rare — and are read as straight up rather than propagated as NaN, which is what the
/// decoder used to do at load time for all 4,225 of them whether or not anything ever
/// sampled that vertex.
///
/// Round 18bf (I3): `None` for a vertex whose triple is zero, so the caller can use the
/// triangle it is on instead of pretending the ground is level there. A record with a few
/// blank vertices among real ones is the same failure as a wholly blank block (which
/// `decode_vnml` now declines outright), one triangle at a time.
#[inline]
fn unit_vnml(n: &[i8; VERTS * VERTS * 3], k: usize) -> Option<[f32; 3]> {
    // Wraithguard (0019): `glam::Vec3` for the arithmetic, the same operations in the same
    // order as the hand-written lines it replaced (`mod glam_normals_agree` pins it bit
    // for bit); the stored and returned types stay arrays.
    let v = Vec3::new(n[k * 3] as f32, n[k * 3 + 1] as f32, n[k * 3 + 2] as f32);
    let l = v.length();
    if l > 0.0 {
        Some((v / l).to_array())
    } else {
        None
    }
}

impl Land {
    /// Which quad a cell-local point is in, and where inside it: `(x0, y0, tx, ty)`
    /// with `tx`/`ty` in 0..1 from the quad's low corner.
    #[inline]
    fn quad(&self, lx: f32, ly: f32) -> (usize, usize, f32, f32) {
        let fx = (lx / VERT_STEP).clamp(0.0, VERTS as f32 - 1.001);
        let fy = (ly / VERT_STEP).clamp(0.0, VERTS as f32 - 1.001);
        /* Round 18bo: the `.min` is a no-op. The clamp above already holds both under 64,
           so neither ever reaches `VERTS - 2` from below — but it says so in *integers*,
           and that is the difference. `heights` is a fixed-size `[f32; 65 * 65]`, so once
           the compiler can see that `x0` and `y0` are at most 63 it can prove every one of
           the quad's four corners is inside the array and drop the bounds checks on them.
           Through a float cast it cannot see that on its own, and `height_at` — eleven and
           a half million calls in one export of Robin's rules, 12% of the whole run — paid
           three of those checks every time. Nothing about which quad is chosen changes.

           The detour through `i32` is the same kind of saving and a larger one. A float
           cast in Rust saturates, and saturating to a 64-bit *unsigned* on x86-64 costs a
           bias-and-fix-up dance that these two lines were spending 374 million
           instructions a run on — 5% of the whole export, to read two numbers the clamp
           above has already put between 0 and 64. Saturating to `i32` is one instruction
           and a select, and over that range the two agree on every value, NaN (0) and all.
           Measured, not assumed: callgrind named these two lines. */
        let x0 = (fx as i32 as usize).min(VERTS - 2);
        let y0 = (fy as i32 as usize).min(VERTS - 2);
        (x0, y0, fx - x0 as f32, fy - y0 as f32)
    }

    /// The three corners of the triangle a point stands on, as indices into a 65x65
    /// grid, plus its barycentric weights. **This is the one place the surface is
    /// defined**; height, normal and slope all go through it, so they cannot describe
    /// different ground.
    ///
    /// The quad is cut from (x+1, y) to (x, y+1). Below that line the triangle is the
    /// low corner and the two next to it; above it, the high corner and the same two.
    #[inline]
    fn tri(&self, lx: f32, ly: f32) -> ([usize; 3], [f32; 3]) {
        let (x0, y0, tx, ty) = self.quad(lx, ly);
        let i = |x: usize, y: usize| y * VERTS + x;
        if tx + ty <= 1.0 {
            // (x0,y0) - (x0+1,y0) - (x0,y0+1)
            ([i(x0, y0), i(x0 + 1, y0), i(x0, y0 + 1)], [1.0 - tx - ty, tx, ty])
        } else {
            // (x0+1,y0+1) - (x0,y0+1) - (x0+1,y0)
            let (u, v) = (1.0 - tx, 1.0 - ty);
            ([i(x0 + 1, y0 + 1), i(x0, y0 + 1), i(x0 + 1, y0)], [1.0 - u - v, u, v])
        }
    }

    /// Height at a cell-local position, on the triangle the game draws.
    #[inline]
    pub fn height_at(&self, lx: f32, ly: f32) -> f32 {
        let (v, w) = self.tri(lx, ly);
        let h = &self.heights;
        h[v[0]] * w[0] + h[v[1]] * w[1] + h[v[2]] * w[2]
    }

    /// Surface normal: the record's authored vertex normals interpolated across the
    /// triangle, or the triangle's own face normal when the record has none.
    ///
    /// VNML is preferred because it is not derivable — no reconstruction from the
    /// heights predicts it (§11a) — and because it is what the game shades with, so a
    /// blade tilted onto it matches the ground you can see. The fallback matters: a
    /// plugin may rewrite VHGT without touching VNML, and the synthetic preview patch
    /// has no record at all.
    #[inline]
    pub fn normal_at(&self, lx: f32, ly: f32) -> [f32; 3] {
        let (v, w) = self.tri(lx, ly);
        if let Some(n) = &self.normals {
            // Round 18bf (I3): all three corners, or none of them — a triangle with a
            // blank corner is shaded from its own geometry rather than from two thirds of
            // an answer.
            let (Some(a), Some(b), Some(c)) =
                (unit_vnml(n, v[0]), unit_vnml(n, v[1]), unit_vnml(n, v[2]))
            else {
                return self.face_normal(v);
            };
            let out = Vec3::from(a) * w[0] + Vec3::from(b) * w[1] + Vec3::from(c) * w[2];
            let l = out.length();
            if l > 1e-6 {
                return (out / l).to_array();
            }
        }
        self.face_normal(v)
    }

    /// The normal of one triangle of the height grid, pointing up.
    #[inline]
    fn face_normal(&self, v: [usize; 3]) -> [f32; 3] {
        let h = &self.heights;
        let p = |k: usize| {
            Vec3::new((k % VERTS) as f32 * VERT_STEP, (k / VERTS) as f32 * VERT_STEP, h[k])
        };
        let (a, b, c) = (p(v[0]), p(v[1]), p(v[2]));
        let mut n = (b - a).cross(c - a);
        if n.z < 0.0 {
            n = -n;
        }
        let l = n.length();
        if l > 1e-9 {
            (n / l).to_array()
        } else {
            [0.0, 0.0, 1.0]
        }
    }

    /// Cosine of the slope angle: the z of the same normal the blade is tilted onto,
    /// so the cull and the tilt cannot disagree about how steep the ground is.
    #[inline]
    pub fn slope_cos(&self, lx: f32, ly: f32) -> f32 {
        self.normal_at(lx, ly)[2]
    }

    /* `vtex_at` used to be here: the raw VTEX value under a point, nearest texel. It
       had no callers. Every texture question in the engine goes through
       `coverage.rs` instead, because the answer needs the one-texel ring from the
       neighbouring landscapes and a point sampler cannot see it (§17c 248y). A
       nearest-texel shortcut sitting beside the real rule is an invitation to use it. */
}

/// `v.floor() as i32`, without the libcall.
///
/// Round 18bo. Baseline x86-64 has no rounding instruction — SSE4.1's `roundss` is not
/// in the target this ships for — so `f32::floor` is a *call* into compiler-builtins'
/// software `floorf`. Callgrind counted **24 million** of them in one export of Robin's
/// rules, 5.2% of the whole scatter, every one of them immediately cast to an integer.
///
/// A truncating cast is one instruction, and floor differs from it only for negative
/// numbers that are not already whole. So this is not an approximation: it agrees with
/// `v.floor() as i32` on every f32 there is, the saturating edges included — NaN casts
/// to 0 and fails the comparison, +∞ casts to `i32::MAX` and fails it, −∞ casts to
/// `i32::MIN` and fails it too (it is not *less than* `i32::MIN as f32`, it equals it
/// after the cast back), and `saturating_sub` keeps the one remaining edge honest.
#[inline]
pub fn floor_i32(v: f32) -> i32 {
    let t = v as i32;
    if v < t as f32 {
        t.saturating_sub(1)
    } else {
        t
    }
}

#[cfg(test)]
mod floor_i32_agrees {
    use super::floor_i32;

    /// Not a spot check: the whole point of the helper is that it is *exactly* the thing
    /// it replaces, so the test is the equality itself over everything interesting.
    #[test]
    fn on_every_edge_the_cast_has() {
        let mut v = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            -0.5,
            -0.0001,
            0.9999,
            -1.9999,
            8191.999,
            -8192.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            i32::MAX as f32,
            i32::MIN as f32,
            (i32::MAX as f32) * 2.0,
            (i32::MIN as f32) * 2.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
        ];
        // And a sweep through the range the scatter actually works in: cell grids,
        // vertex quads, bucket indices — all of them small signed numbers.
        let mut x = -300.0f32;
        while x < 300.0 {
            v.push(x);
            x += 0.0625;
        }
        for &a in &v {
            assert_eq!(floor_i32(a), a.floor() as i32, "floor_i32({a})");
        }
    }
}

/// Wraithguard (0019): the normal math is `glam::Vec3` now; these are the hand-written lines
/// it replaced, kept as the reference, and the two must agree **bit for bit** (the same
/// operations in the same order; a tolerance would let a changed order through).
#[cfg(test)]
mod glam_normals_agree {
    use super::*;

    fn ref_unit_vnml(n: &[i8; VERTS * VERTS * 3], k: usize) -> Option<[f32; 3]> {
        let x = n[k * 3] as f32;
        let y = n[k * 3 + 1] as f32;
        let z = n[k * 3 + 2] as f32;
        let l = (x * x + y * y + z * z).sqrt();
        if l > 0.0 {
            Some([x / l, y / l, z / l])
        } else {
            None
        }
    }

    fn ref_face_normal(h: &[f32; VERTS * VERTS], v: [usize; 3]) -> [f32; 3] {
        let p = |k: usize| [(k % VERTS) as f32 * VERT_STEP, (k / VERTS) as f32 * VERT_STEP, h[k]];
        let (a, b, c) = (p(v[0]), p(v[1]), p(v[2]));
        let (u, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let mut n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        if n[2] < 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if l > 1e-9 {
            [n[0] / l, n[1] / l, n[2] / l]
        } else {
            [0.0, 0.0, 1.0]
        }
    }

    fn ref_normal_at(land: &Land, lx: f32, ly: f32) -> [f32; 3] {
        let (v, w) = land.tri(lx, ly);
        if let Some(n) = &land.normals {
            if let (Some(a), Some(b), Some(c)) =
                (ref_unit_vnml(n, v[0]), ref_unit_vnml(n, v[1]), ref_unit_vnml(n, v[2]))
            {
                let mut out = [
                    a[0] * w[0] + b[0] * w[1] + c[0] * w[2],
                    a[1] * w[0] + b[1] * w[1] + c[1] * w[2],
                    a[2] * w[0] + b[2] * w[1] + c[2] * w[2],
                ];
                let l = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
                if l > 1e-6 {
                    out[0] /= l;
                    out[1] /= l;
                    out[2] /= l;
                    return out;
                }
            }
        }
        ref_face_normal(&land.heights, v)
    }

    /// A small deterministic generator (the same on every machine).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn unit(&mut self) -> f32 {
            self.next() as f32 / (1u32 << 31) as f32
        }
    }

    fn land(seed: u64, with_normals: bool, blank_every: usize) -> Land {
        let mut r = Lcg(seed);
        let mut heights = Box::new([0f32; VERTS * VERTS]);
        for h in heights.iter_mut() {
            *h = (r.unit() - 0.5) * 3000.0;
        }
        let normals = with_normals.then(|| {
            let mut n = Box::new([0i8; VERTS * VERTS * 3]);
            for (i, b) in n.iter_mut().enumerate() {
                *b = if blank_every > 0 && (i / 3) % blank_every == 0 { 0 } else { (r.next() % 255) as i32 as i8 };
            }
            n
        });
        Land {
            grid: (0, 0),
            heights,
            normals,
            colours: None,
            vtex: Box::new([0u16; SUBDIV * SUBDIV]),
            plugin: 0,
        }
    }

    fn same(a: [f32; 3], b: [f32; 3], what: &str) {
        for k in 0..3 {
            assert_eq!(a[k].to_bits(), b[k].to_bits(), "{what}: component {k}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn unit_vnml_is_the_reference_for_every_byte_triple_that_matters() {
        let mut n = Box::new([0i8; VERTS * VERTS * 3]);
        let mut r = Lcg(7);
        for b in n.iter_mut() {
            *b = (r.next() % 256) as u8 as i8;
        }
        // blank, one axis, the extremes
        n[0..3].copy_from_slice(&[0, 0, 0]);
        n[3..6].copy_from_slice(&[0, 0, 127]);
        n[6..9].copy_from_slice(&[-128, -128, -128]);
        n[9..12].copy_from_slice(&[127, 127, 127]);
        for k in 0..VERTS * VERTS {
            match (unit_vnml(&n, k), ref_unit_vnml(&n, k)) {
                (Some(a), Some(b)) => same(a, b, &format!("vertex {k}")),
                (None, None) => {}
                (a, b) => panic!("vertex {k}: {a:?} vs {b:?}"),
            }
        }
    }

    #[test]
    fn face_normals_are_the_reference_and_point_up() {
        for seed in 1..=4 {
            let land = land(seed, false, 0);
            let mut r = Lcg(seed * 31);
            for _ in 0..4000 {
                let (lx, ly) = (r.unit() * 8190.0, r.unit() * 8190.0);
                let (v, _) = land.tri(lx, ly);
                let (n, want) = (land.face_normal(v), ref_face_normal(&land.heights, v));
                same(n, want, &format!("seed {seed} at {lx},{ly}"));
                assert!(n[2] >= 0.0, "a face normal points up: {n:?}");
            }
        }
        // a flat quad: straight up; a degenerate triangle (all three the same vertex): the fallback
        let mut flat = land(9, false, 0);
        flat.heights.iter_mut().for_each(|h| *h = 100.0);
        same(flat.face_normal([0, 1, VERTS]), [0.0, 0.0, 1.0], "flat");
        same(flat.face_normal([5, 5, 5]), [0.0, 0.0, 1.0], "degenerate");
    }

    #[test]
    fn normal_at_is_the_reference_with_authored_normals_with_blank_corners_and_without() {
        for (seed, with, blank) in [(11, true, 0), (12, true, 5), (13, true, 1), (14, false, 0)] {
            let land = land(seed, with, blank);
            let mut r = Lcg(seed * 17);
            for _ in 0..4000 {
                let (lx, ly) = (r.unit() * 8200.0 - 5.0, r.unit() * 8200.0 - 5.0);
                same(land.normal_at(lx, ly), ref_normal_at(&land, lx, ly), &format!("seed {seed} at {lx},{ly}"));
            }
        }
    }
}
