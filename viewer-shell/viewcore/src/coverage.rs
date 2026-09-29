//! Land-texture coverage over a cell.
//!
//! A LAND record paints one texture per 512-unit sub-cell, 16 across. Nothing in
//! the game draws that as a checkerboard: each texture is a layer with its own
//! alpha map, filtered linearly, so between two sub-cells of different textures
//! the ground is a gradient. Grass has to read the same gradient the eye sees, or
//! it stops dead on a line nobody can see.
//!
//! Two things follow from that, and both live here so there is one copy:
//!
//! 1. **The ring.** The field is 18x18, not 16x16: the cell's own sub-cells plus
//!    one texel taken from each neighbouring landscape. Without it the map clamps
//!    at its edge, and two adjoining cells each hold their border texture constant
//!    across the last half texel — which is the hard straight line at a cell join.
//!
//! 2. **The neighbour is the *world's*, not the viewer's.** The engine can always
//!    see every landscape it loaded. That matters because the page can not: it
//!    holds one cell, or nine, and clamps at the edge of what it happens to have
//!    open. Deriving the ring page-side therefore drew a border the engine never
//!    scattered against.
//!
//! `poisson` samples these fields directly; the shell ships them to the page as
//! the alpha maps it draws the ground from, so the ground the user sees is the
//! ground the grass was placed on, texel for texel.

use crate::land::{Land, SUBDIV};
use crate::world::World;

/// Field width: the 16 sub-cells plus one ring texel on every side.
pub const P: usize = SUBDIV + 2;
/// Texels in a field.
pub const FIELD: usize = P * P;

/// Which landscape and sub-cell a ring coordinate resolves to.
///
/// `gi`/`gj` run -1..=16. Inside the cell they are its own; outside they step into
/// the neighbouring landscape, and when there is no neighbour loaded the edge value
/// is held — there is nothing truer to say than "more of the same", and it is what a
/// lone cell looks like anyway.
#[inline]
pub fn resolve<'a>(world: &'a World, land: &'a Land, gi: i32, gj: i32) -> (&'a Land, usize, usize) {
    let dx = if gi < 0 {
        -1
    } else if gi >= SUBDIV as i32 {
        1
    } else {
        0
    };
    let dy = if gj < 0 {
        -1
    } else if gj >= SUBDIV as i32 {
        1
    } else {
        0
    };
    if dx == 0 && dy == 0 {
        return (land, gi as usize, gj as usize);
    }
    let g = (land.grid.0 + dx, land.grid.1 + dy);
    match world.lands.get(&g) {
        Some(l) => (l, gi.rem_euclid(SUBDIV as i32) as usize, gj.rem_euclid(SUBDIV as i32) as usize),
        None => (
            land,
            gi.clamp(0, SUBDIV as i32 - 1) as usize,
            gj.clamp(0, SUBDIV as i32 - 1) as usize,
        ),
    }
}

/// The LTEX id at a ring coordinate, in the case the plugin wrote it.
#[inline]
pub fn texture_at<'a>(world: &'a World, land: &'a Land, gi: i32, gj: i32) -> Option<&'a str> {
    let (l, ti, tj) = resolve(world, land, gi, gj);
    world.ltex_for(l.vtex[tj * SUBDIV + ti], l.plugin)
}

/// One drawable layer: a land texture and the mask saying where it is painted.
pub struct Layer {
    /// LTEX id as the plugin wrote it. Empty when the ground is unpainted.
    pub id: String,
    /// 18x18 mask, one bit per texel, row-major, bit `t & 7` of byte `t >> 3`.
    ///
    /// A texel carries exactly one texture, so every layer's mask is binary and the
    /// whole field packs into 41 bytes. The gradient is the linear filter's doing,
    /// not the mask's.
    pub mask: Vec<u8>,
    /// Fraction of the cell's **own** 256 sub-cells this texture paints. Ordering
    /// key, and what the texture list shows as a percentage.
    pub coverage: f32,
}

/// Every texture with any presence over the cell or its ring, most-painted first.
///
/// The order is the draw order: the commonest texture is the opaque base and the
/// rest blend over it. Only the cell's own sub-cells count towards that — a texture
/// that merely bleeds in from next door must not become the base.
pub fn layers(world: &World, land: &Land) -> Vec<Layer> {
    let mut ids: Vec<String> = Vec::new();
    let mut masks: Vec<Vec<u8>> = Vec::new();
    let mut counts: Vec<u32> = Vec::new();
    let bytes = FIELD.div_ceil(8);

    for j in 0..P {
        for i in 0..P {
            let (gi, gj) = (i as i32 - 1, j as i32 - 1);
            let id = texture_at(world, land, gi, gj).unwrap_or("").to_string();
            let k = match ids.iter().position(|x| *x == id) {
                Some(k) => k,
                None => {
                    ids.push(id);
                    masks.push(vec![0u8; bytes]);
                    counts.push(0);
                    ids.len() - 1
                }
            };
            let t = j * P + i;
            masks[k][t >> 3] |= 1 << (t & 7);
            let inner = (0..SUBDIV as i32).contains(&gi) && (0..SUBDIV as i32).contains(&gj);
            if inner {
                counts[k] += 1;
            }
        }
    }

    let mut out: Vec<Layer> = ids
        .into_iter()
        .zip(masks)
        .zip(counts)
        .map(|((id, mask), c)| Layer { id, mask, coverage: c as f32 / (SUBDIV * SUBDIV) as f32 })
        .collect();
    // Stable by construction: ties keep first-seen order, which is the ring walk's,
    // so the same cell always produces the same draw order.
    out.sort_by(|a, b| b.coverage.total_cmp(&a.coverage));
    out
}

/// The layers as the shell ships them, in draw order.
///
/// One writer, so the desktop command and `GardenfellCLI layers` cannot drift: the
/// second exists to test the first, which it cannot do if it builds its own JSON.
///
/// Masks go over as hex rather than base64 because 41 bytes is small enough that
/// the encoding does not matter and hex is readable in a failing test.
pub fn layers_json(ls: &[Layer]) -> String {
    let mut s = String::from("[");
    for (i, l) in ls.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"id\":\"");
        crate::json::escape_into(&l.id, &mut s);
        s.push_str("\",\"coverage\":");
        s.push_str(&format!("{:.6}", l.coverage));
        s.push_str(",\"mask\":\"");
        for b in &l.mask {
            s.push_str(&format!("{:02x}", b));
        }
        s.push_str("\"}");
    }
    s.push(']');
    s
}
