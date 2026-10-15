//! Wraithguard: distant objects' models, simplified - the second half of the land past
//! the rings (`distant`, and the page's 58_wg_distant_objects.js).
//!
//! OpenMW draws the objects out there too ("object paging"), and MGE XE has done it for
//! Morrowind for years; how this budgets and simplifies follows MGE XE (Greatness7's Rust
//! rewrite, GPL-2 - its approach and default numbers, none of its code; see CREDITS):
//!
//! - **Size tiers.** A static is near, far or very far by its radius times its scale: up
//!   to 600 units near, up to 800 far, larger very far (MGE's `far_static_min_size` and
//!   `very_far_static_min_size`). Each tier is drawn out to its own band - in MGE 2, 4 and
//!   5 cells of a 5-cell draw distance; here the same shares (2/5, 4/5, all) of the cells
//!   past the loaded rings (`band_end`). Small things drop out first, which is the whole
//!   of the budget: most references never load at all.
//! - **One copy per model, simplified by error, not by count** (MGE's static mesh
//!   simplifier): identical vertices welded first (a NIF splits them at every seam, which
//!   reads as borders), then `simplify_with_attributes` with no triangle target and an
//!   error budget of 1% of the part's size - 2% for a far model, 4% for a very far one -
//!   borders locked, and the normals (weight 0.5), the UVs (weighted by how dense they are:
//!   the inverse square root of the mean UV area of a triangle, 10 where that is nothing)
//!   and the vertex colours (0.5, or 0 when they are all one colour) as attributes. Only
//!   the triangle list changes; the vertices stay, so every per-vertex array still lines
//!   up. A posed part (a skin, a morph) is left whole.
//!
//! MGE also nests its tiers in one index buffer, so a farther band draws a prefix of the
//! same list - that works because its tiers drop whole objects from a merged batch. Here
//! every model is drawn as instances, so the same saving is one simplified copy per model
//! and the tiers choosing which instances are drawn at all.
//!
//! Two ways to choose what to keep (the page's switch, Static's "step 3"):
//!
//! - **plain**: the error alone;
//! - **texture-aware**: after Zhao, Jiang & Guo, "A novel quadratic error metric mesh
//!   simplification algorithm for 3D building models based on 'local-vertex' texture
//!   features" (ISPRS Archives XLVIII-3/W2-2022): the busiest fifth of the vertices - by
//!   how much the texture under their UVs changes (from its small mip) and how much the
//!   surface bends there - are marked `VERTEX_PRIORITY`, which weighs their error up so
//!   detail stays where the eye goes. The paper's idea on meshoptimizer's own lever.

use std::collections::HashMap;

use crate::nif::DrawPart;
use meshopt_port::simplify::{simplify_with_attributes, LOCK_BORDER, VERTEX_PRIORITY};

/// How the vertices to keep are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Plain,
    Texture,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        if s.eq_ignore_ascii_case("texture") {
            Mode::Texture
        } else {
            Mode::Plain
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Mode::Plain => "plain",
            Mode::Texture => "texture",
        }
    }
}

/// The largest radius (game units, scaled) a near static has (MGE's `far_static_min_size`).
pub const FAR_STATIC_MIN_SIZE: f32 = 600.0;
/// The largest radius a far static has (MGE's `very_far_static_min_size`).
pub const VERY_FAR_STATIC_MIN_SIZE: f32 = 800.0;

/// A static's tier by its radius times its scale: 0 near, 1 far, 2 very far.
pub fn tier_of(radius: f32) -> u8 {
    if radius <= FAR_STATIC_MIN_SIZE {
        0
    } else if radius <= VERY_FAR_STATIC_MIN_SIZE {
        1
    } else {
        2
    }
}

/// How many cells past the loaded rings a tier is drawn, of `span` there are: MGE's
/// default bands (2, 4 and 5 cells of a 5-cell draw distance) as shares. At least one.
pub fn band_end(tier: u8, span: i32) -> i32 {
    let share = match tier {
        0 => 0.4,
        1 => 0.8,
        _ => 1.0,
    };
    ((span as f32 * share).round() as i32).max(1)
}

/// The error a model of a tier is simplified to, relative to its size (MGE's 0.01, with
/// room for distance: a far model is never seen as close as a near one).
fn target_error(tier: u8) -> f32 {
    match tier {
        0 => 0.01,
        1 => 0.02,
        _ => 0.04,
    }
}

/// A part this small is kept as it is: there is nothing to gain and its shape is all it has.
const MIN_TRIANGLES: usize = 12;

/// How far the model reaches from its own origin, before the reference's scale.
pub fn radius(parts: &[DrawPart]) -> f32 {
    let mut r2 = 0.0f32;
    for p in parts {
        for v in p.pos.chunks(3) {
            if v.len() == 3 {
                r2 = r2.max(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
            }
        }
    }
    r2.sqrt()
}

/// A texture's small image (`img::Texture::thumb`): width, height, RGBA.
pub type Swatch = (u32, u32, Vec<u8>);

/// The index list with identical vertices welded: every index points at the first vertex
/// with the same position, normal, every UV set, colour and alpha. The vertices
/// themselves are untouched.
fn weld(p: &DrawPart, idx: &[u32], n: usize) -> Vec<u32> {
    let has_nrm = p.nrm.len() == n * 3;
    let uvs: Vec<&[f32]> = [&p.uv, &p.uv2, &p.uv_dark, &p.uv_decal, &p.uv_glow]
        .into_iter().filter(|uv| uv.len() == n * 2).map(|uv| uv.as_slice()).collect();
    let has_col = p.col.len() == n * 3;
    let has_alpha = p.col_alpha.len() == n;
    let mut first: HashMap<Vec<u32>, u32> = HashMap::with_capacity(n);
    let mut canon = vec![0u32; n];
    for i in 0..n {
        let mut key: Vec<u32> = p.pos[i * 3..i * 3 + 3].iter().map(|f| f.to_bits()).collect();
        if has_nrm {
            key.extend(p.nrm[i * 3..i * 3 + 3].iter().map(|f| f.to_bits()));
        }
        for uv in &uvs {
            key.extend(uv[i * 2..i * 2 + 2].iter().map(|f| f.to_bits()));
        }
        if has_col {
            key.extend(p.col[i * 3..i * 3 + 3].iter().map(|&c| c as u32));
        }
        if has_alpha {
            key.push(p.col_alpha[i] as u32);
        }
        canon[i] = *first.entry(key).or_insert(i as u32);
    }
    idx.iter().map(|&i| canon[i as usize]).collect()
}

/// The UVs' weight: the inverse square root of the mean UV area of a triangle, so a part
/// whose texture covers little of its map keeps its UVs better; 10 where that is nothing.
fn uv_weight(uv: &[f32], idx: &[u32]) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for t in idx.chunks(3) {
        if t.len() < 3 {
            continue;
        }
        let at = |i: u32| (uv[i as usize * 2], uv[i as usize * 2 + 1]);
        let ((u1, v1), (u2, v2), (u3, v3)) = (at(t[0]), at(t[1]), at(t[2]));
        sum += ((u2 - u1) * (v3 - v1) - (u3 - u1) * (v2 - v1)).abs() * 0.5;
        count += 1;
    }
    let mean = if count > 0 { sum / count as f32 } else { 0.0 };
    if mean > 1e-12 {
        1.0 / mean.sqrt()
    } else {
        10.0
    }
}

/// Every part simplified for a model of `tier` in `mode`. `swatch` answers a part's base
/// texture's small image, for the texture-aware mode (None: rated by its shape alone).
pub fn simplify_parts(parts: &mut [DrawPart], tier: u8, mode: Mode, swatch: &dyn Fn(&str) -> Option<Swatch>) {
    for p in parts.iter_mut() {
        let n = p.pos.len() / 3;
        let tris = p.idx.len() / 3;
        if p.skin.is_some() || p.morph.is_some() || tris <= MIN_TRIANGLES || n == 0 {
            continue;
        }
        let raw: Vec<u32> = p.idx.iter().map(|&i| i as u32).collect();
        if raw.iter().any(|&i| i as usize >= n) {
            continue;
        }
        let idx = weld(p, &raw, n);
        // Normals (3), UVs (2) and colours (3) as one attribute block a vertex; what the
        // part lacks is zeros at weight zero.
        let (has_nrm, has_uv, has_col) = (p.nrm.len() == n * 3, p.uv.len() == n * 2, p.col.len() == n * 3);
        let uniform = !has_col || p.col.chunks(3).all(|c| c == &p.col[0..3]);
        let mut attrs = vec![0.0f32; n * 8];
        for i in 0..n {
            if has_nrm {
                attrs[i * 8..i * 8 + 3].copy_from_slice(&p.nrm[i * 3..i * 3 + 3]);
            }
            if has_uv {
                attrs[i * 8 + 3..i * 8 + 5].copy_from_slice(&p.uv[i * 2..i * 2 + 2]);
            }
            if has_col {
                for k in 0..3 {
                    attrs[i * 8 + 5 + k] = p.col[i * 3 + k] as f32 / 255.0;
                }
            }
        }
        let (wn, wu, wc) = (
            if has_nrm { 0.5 } else { 0.0 },
            if has_uv { uv_weight(&p.uv, &idx) } else { 0.0 },
            if uniform { 0.0 } else { 0.5 },
        );
        let weights = [wn, wn, wn, wu, wu, wc, wc, wc];
        let lock = if mode == Mode::Texture { Some(priority(p, n, swatch(&p.tex).as_ref())) } else { None };
        let out = simplify_with_attributes(&idx, &p.pos, n, 3, &attrs, 8, &weights, 8, lock.as_deref(), 0, target_error(tier), LOCK_BORDER, None);
        if out.len() >= 3 && out.len() < idx.len() {
            p.idx = out.into_iter().map(|i| i as u16).collect();
        } else if idx != raw {
            // Nothing to gain, but the welded list draws the same and shares more vertices.
            p.idx = idx.into_iter().map(|i| i as u16).collect();
        }
    }
}

/// The texture-aware mode's flags: `VERTEX_PRIORITY` on the busiest fifth of a part's
/// vertices, by texture detail under their UVs (0.7) and how much the surface bends there
/// (0.3).
fn priority(p: &DrawPart, n: usize, sw: Option<&Swatch>) -> Vec<u8> {
    // How much the surface bends at each vertex: 1 - the length of the mean of the unit
    // normals of the triangles around it (0 on a flat run, towards 1 at a sharp crease).
    let mut sum = vec![[0f32; 3]; n];
    let mut cnt = vec![0u32; n];
    for t in p.idx.chunks(3) {
        if t.len() < 3 {
            continue;
        }
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if a >= n || b >= n || c >= n {
            continue;
        }
        let v = |i: usize| [p.pos[i * 3], p.pos[i * 3 + 1], p.pos[i * 3 + 2]];
        let (pa, pb, pc) = (v(a), v(b), v(c));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let nn = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let l = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2]).sqrt();
        if l <= 0.0 {
            continue;
        }
        for &i in &[a, b, c] {
            for k in 0..3 {
                sum[i][k] += nn[k] / l;
            }
            cnt[i] += 1;
        }
    }
    // How much the texture changes under each vertex's UV: how far the four texels around
    // its texel of the swatch differ from it in luminance (wrapped, as the base map is drawn).
    let lum = |sw: &Swatch, u: f32, v: f32| -> f32 {
        let (w, h, px) = (sw.0 as i64, sw.1 as i64, &sw.2);
        let x = ((u - u.floor()) * w as f32) as i64;
        let y = ((v - v.floor()) * h as f32) as i64;
        let at = |x: i64, y: i64| {
            let o = (((y.rem_euclid(h)) * w + x.rem_euclid(w)) * 4) as usize;
            if o + 2 < px.len() {
                (0.299 * px[o] as f32 + 0.587 * px[o + 1] as f32 + 0.114 * px[o + 2] as f32) / 255.0
            } else {
                0.0
            }
        };
        // Against the texel itself, not across it: a difference across skips the centre,
        // and the finest detail (a texel-wide pattern) would read as none.
        let c = at(x, y);
        ((at(x + 1, y) - c).abs() + (at(x - 1, y) - c).abs() + (at(x, y + 1) - c).abs() + (at(x, y - 1) - c).abs()) * 0.25
    };
    let mut score = vec![0f32; n];
    for i in 0..n {
        let bend = if cnt[i] > 0 {
            let m = [sum[i][0] / cnt[i] as f32, sum[i][1] / cnt[i] as f32, sum[i][2] / cnt[i] as f32];
            1.0 - (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt().min(1.0)
        } else {
            0.0
        };
        let detail = match (sw, p.uv.len() == n * 2) {
            (Some(sw), true) if sw.0 > 2 && sw.1 > 2 => lum(sw, p.uv[i * 2], p.uv[i * 2 + 1]),
            _ => 0.0,
        };
        score[i] = 0.7 * detail + 0.3 * bend;
    }
    // The busiest fifth, by score; none when nothing stands out.
    let mut sorted: Vec<f32> = score.iter().copied().filter(|s| *s > 0.0).collect();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let cut = sorted.get(n / 5).copied().unwrap_or(f32::MAX);
    score.iter().map(|s| if *s > 0.0 && *s >= cut { VERTEX_PRIORITY } else { 0 }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bent grid as one part, with UVs across it.
    fn grid(k: usize) -> DrawPart {
        let mut p = DrawPart::default();
        for y in 0..=k {
            for x in 0..=k {
                let (fx, fy) = (x as f32 / k as f32, y as f32 / k as f32);
                p.pos.extend_from_slice(&[fx * 100.0, fy * 100.0, 10.0 * fx * (1.0 - fx)]);
                p.uv.extend_from_slice(&[fx, fy]);
            }
        }
        for y in 0..k {
            for x in 0..k {
                let a = (y * (k + 1) + x) as u16;
                let s = (k + 1) as u16;
                p.idx.extend_from_slice(&[a, a + 1, a + s + 1, a, a + s + 1, a + s]);
            }
        }
        p
    }

    #[test]
    fn fewer_triangles_further_out_and_the_vertices_untouched() {
        let mut counts = Vec::new();
        for tier in 0..3 {
            let mut parts = vec![grid(16)];
            let pos = parts[0].pos.clone();
            simplify_parts(&mut parts, tier, Mode::Plain, &|_| None);
            assert_eq!(parts[0].pos, pos, "the vertices stay");
            assert!(parts[0].idx.iter().all(|&i| (i as usize) < pos.len() / 3));
            counts.push(parts[0].idx.len() / 3);
        }
        assert!(counts[0] < 512 && counts[1] <= counts[0] && counts[2] <= counts[1], "{counts:?}");
    }

    #[test]
    fn tiers_and_bands_as_mge_has_them() {
        assert_eq!((tier_of(600.0), tier_of(601.0), tier_of(800.0), tier_of(2000.0)), (0, 1, 1, 2));
        // MGE's defaults: 2, 4 and 5 cells of 5.
        assert_eq!((band_end(0, 5), band_end(1, 5), band_end(2, 5)), (2, 4, 5));
        assert_eq!(band_end(0, 1), 1, "at least one cell");
    }

    #[test]
    fn a_seam_of_split_vertices_is_welded() {
        // Two triangles sharing an edge through four vertices, two of them duplicates.
        let p = DrawPart {
            pos: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0],
            ..Default::default()
        };
        let w = weld(&p, &[0, 1, 2, 3, 5, 4], 6);
        assert_eq!(w, vec![0, 1, 2, 1, 5, 2]);
    }

    #[test]
    fn welding_preserves_secondary_uv_and_vertex_alpha_seams() {
        let raw = [0, 1, 2, 3, 5, 4];
        for channel in 0..5 {
            let mut p = DrawPart {
                pos: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0],
                ..Default::default()
            };
            if channel == 4 {
                p.col_alpha = vec![255; 6];
                p.col_alpha[3] = 128;
            } else {
                let uv = match channel {
                    0 => &mut p.uv2,
                    1 => &mut p.uv_dark,
                    2 => &mut p.uv_decal,
                    _ => &mut p.uv_glow,
                };
                *uv = vec![0.0; 12];
                uv[6] = 1.0;
            }
            // Vertices 1 and 3 share position/base attributes, but differ in this
            // channel. Vertex 4 is still identical to 2 and should be welded.
            assert_eq!(weld(&p, &raw, 6), vec![0, 1, 2, 3, 5, 2], "channel {channel}");
        }
    }

    #[test]
    fn the_texture_aware_mode_marks_where_the_texture_is_busy() {
        let p = grid(8);
        // Left half flat grey, right half a checker.
        let (w, h) = (16u32, 16u32);
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 128 } else if (x + y) % 2 == 0 { 255 } else { 0 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let flags = priority(&p, p.pos.len() / 3, Some(&(w, h, px)));
        let marked: Vec<usize> = (0..flags.len()).filter(|&i| flags[i] == VERTEX_PRIORITY).collect();
        assert!(!marked.is_empty());
        assert!(marked.iter().all(|&i| p.uv[i * 2] >= 0.45), "only the checker side: {marked:?}");
    }

    #[test]
    fn a_small_or_posed_part_is_kept_whole() {
        let mut parts = vec![grid(2)];
        let idx = parts[0].idx.clone();
        simplify_parts(&mut parts, 2, Mode::Texture, &|_| None);
        assert_eq!(parts[0].idx, idx);
    }
}
