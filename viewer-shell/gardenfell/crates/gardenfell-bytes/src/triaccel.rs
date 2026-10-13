//! Bounding blocks over a mesh's triangles, so a query stops looking at all of them.
//!
//! Round 18bz. `Obstructions` is a uniform bucket grid over one cell's obstacles, and a
//! query takes one bucket and then walks **every triangle of every obstacle in it**. There
//! is no hierarchy inside a mesh at all: a blade standing under a tree tests the tree's
//! several thousand triangles, one at a time, and it does it once per dart that survives
//! spacing — of the order of 31 million times in a whole-world export. On Robin's data
//! `hit_except` was the largest single item in the scatter at 29.5%.
//!
//! Two costs, and this file removes both:
//!
//! 1. **Per-triangle bounds were recomputed on every query.** Twelve float min/max per
//!    triangle per query, over object-space coordinates that never change after the mesh is
//!    read. On the obstacle benchmark that was **891 M instructions, 26.4% of the run**,
//!    split between `column_hits` and `Obstructions::distance`. They are computed once here
//!    instead, when the mesh is read.
//! 2. **Nothing pruned.** Above the per-triangle bounds sit blocks of eight, and above those
//!    blocks of sixty-four, and so on to a single root — each holding the union of its
//!    children's boxes. A query descends from the root and skips whole blocks, so a small
//!    query box against a large mesh touches a logarithmic number of boxes rather than every
//!    triangle.
//!
//! # Why blocks of eight in the original order, and not a BVH
//!
//! A textbook BVH would sort the triangles by position, which gives tighter boxes — and
//! reorders the walk. Every query answer in this program has to be **identical**, byte for
//! byte in the plugin, and the answers here are not all order-free: `column_hits` returns
//! early on the first triangle the blade pierces, and counts ray crossings for its
//! inside-a-closed-shape parity. Reordering cannot change the *result* of either, but
//! proving that takes a paragraph per call site and has to be re-proved whenever one of them
//! changes.
//!
//! Blocks in the original order need no such proof: the accelerated walk visits a
//! **subsequence** of the triangles the flat walk visits, in the same order. So the only
//! question left is whether a skipped triangle could have mattered, and that is answered
//! once, below, by containment.
//!
//! # Why skipping is safe
//!
//! Every box here contains the boxes below it, and a triangle's box contains the triangle.
//! So if a triangle's box does not overlap the query box, no ancestor block that overlaps
//! the query box can be skipped on its account — and a triangle whose own box misses the
//! query box cannot satisfy any predicate the callers use, because each of those predicates
//! implies containment in the query box:
//!
//! * a ray hit inside the blade's length puts the hit point inside the blade's segment box;
//! * a surface within `clear` of the segment puts the triangle's box within `clear` of the
//!   segment's box, which is why the flat walk's own `near` test is exactly this test;
//! * a ray crossing counted for parity lies **inside the mesh**, so it lies on the part of
//!   the ray that is inside the root box — which is what [`TriAccel::exit_t`] measures.
//!
//! The one place any slack enters is that last one, and it is padded rather than argued
//! about: see `exit_t`.

use std::sync::Arc;

/// The branching factor. Eight keeps a block's eight boxes in two cache lines and makes the
/// arithmetic a shift.
const FAN: usize = 8;

/// Meshes smaller than this keep the flat walk. A box stand-in is twelve triangles and a
/// simple hull a few dozen: for those the bounds arithmetic the hierarchy saves is less than
/// the descent it costs, and the memory is not free — 24 bytes a triangle, over the 6,873
/// meshes of Robin's install that carry a collision shape.
const MIN_TRIS: usize = 48;

/// One axis-aligned box, as `[min, max]`.
type Box3 = [[f32; 3]; 2];

/// A mesh's triangle bounds and the blocks above them, in the triangles' own order.
pub struct TriAccel {
    /// `levels[0]` is one box per triangle; `levels[k+1]` is one box per eight boxes of
    /// `levels[k]`. The last level is a single box: the mesh's own bounds.
    levels: Vec<Vec<Box3>>,
}

#[inline]
fn overlaps(b: &Box3, lo: [f32; 3], hi: [f32; 3]) -> bool {
    b[1][0] >= lo[0]
        && b[0][0] <= hi[0]
        && b[1][1] >= lo[1]
        && b[0][1] <= hi[1]
        && b[1][2] >= lo[2]
        && b[0][2] <= hi[2]
}

impl TriAccel {
    /// The hierarchy for a triangle soup, or `None` when the mesh is too small to be worth
    /// one — in which case every caller keeps the flat walk it has always had.
    ///
    /// `tris` is the soup as the readers store it: three consecutive points per triangle, a
    /// trailing one or two points ignored exactly as `chunks_exact(3)` ignores them.
    pub fn build(tris: &[[f32; 3]]) -> Option<Arc<TriAccel>> {
        let n = tris.len() / 3;
        if n < MIN_TRIS {
            return None;
        }
        let mut leaf: Vec<Box3> = Vec::with_capacity(n);
        for t in tris.chunks_exact(3) {
            let (a, b, c) = (t[0], t[1], t[2]);
            let mut lo = [0.0f32; 3];
            let mut hi = [0.0f32; 3];
            for i in 0..3 {
                lo[i] = a[i].min(b[i]).min(c[i]);
                hi[i] = a[i].max(b[i]).max(c[i]);
            }
            leaf.push([lo, hi]);
        }
        let mut levels = vec![leaf];
        while levels.last().map_or(0, |l| l.len()) > 1 {
            let below = levels.last().unwrap();
            let mut up: Vec<Box3> = Vec::with_capacity((below.len() + FAN - 1) / FAN);
            for block in below.chunks(FAN) {
                let mut lo = [f32::INFINITY; 3];
                let mut hi = [f32::NEG_INFINITY; 3];
                for b in block {
                    for i in 0..3 {
                        lo[i] = lo[i].min(b[0][i]);
                        hi[i] = hi[i].max(b[1][i]);
                    }
                }
                up.push([lo, hi]);
            }
            levels.push(up);
        }
        Some(Arc::new(TriAccel { levels }))
    }

    /// One triangle's bounds, for the callers that have their own use for them — the
    /// clearance test's `near`, which is this box against the query box.
    #[inline]
    pub fn bounds(&self, tri: usize) -> &Box3 {
        &self.levels[0][tri]
    }

    /// The whole mesh's bounds, as the union of its triangles'. Not the same thing as
    /// `MeshGeom::aabb_min/max`, which describe whatever shape was chosen; this is exactly
    /// what the triangles here span, which is what the reasoning about skipping needs.
    #[inline]
    pub fn root(&self) -> &Box3 {
        &self.levels[self.levels.len() - 1][0]
    }

    pub fn triangles(&self) -> usize {
        self.levels[0].len()
    }

    /// How far along the ray `o + t*d` a triangle of this mesh can possibly lie.
    ///
    /// Callers that count ray crossings need a *bounded* query box, and this is the bound:
    /// every triangle is inside the root box, so a hit point is on the part of the ray
    /// inside the root box, and no hit can have a parameter beyond where the ray leaves it.
    /// Returns 0 when the ray misses the box entirely, which is the case where no hit is
    /// possible at all.
    ///
    /// **The padding is the one piece of slack in this file.** The slab arithmetic below is
    /// a divide and a min/max per axis, so its answer can be a unit in the last place short
    /// of the true exit — and a triangle lying exactly on the boundary, hit by a ray that
    /// grazes it, would then be skipped and a parity would flip. The box is therefore grown
    /// by a thousandth of its own size before clipping, which is some ten million times the
    /// rounding and still small enough to prune. Growing the query box only ever adds
    /// candidates, so the padding cannot change an answer — it can only cost a few more
    /// triangle tests.
    pub fn exit_t(&self, o: [f32; 3], d: [f32; 3]) -> f32 {
        let r = self.root();
        let mut t_lo = 0.0f32;
        let mut t_hi = f32::INFINITY;
        for i in 0..3 {
            let pad = ((r[1][i] - r[0][i]).abs() * 1e-3).max(1e-3);
            let (lo, hi) = (r[0][i] - pad, r[1][i] + pad);
            if d[i].abs() <= f32::MIN_POSITIVE {
                // Parallel to this slab: inside it forever, or never.
                if o[i] < lo || o[i] > hi {
                    return 0.0;
                }
                continue;
            }
            let inv = 1.0 / d[i];
            let a = (lo - o[i]) * inv;
            let b = (hi - o[i]) * inv;
            t_lo = t_lo.max(a.min(b));
            t_hi = t_hi.min(a.max(b));
        }
        if !(t_hi >= t_lo) || !t_hi.is_finite() {
            return 0.0;
        }
        t_hi
    }

    /// Every triangle whose own bounds overlap `[lo, hi]`, in the mesh's own order.
    ///
    /// The descent is iterative over a small stack of `(level, index)`. A block whose box
    /// misses the query box is skipped whole, and at the bottom each triangle's own box is
    /// tested before the caller is troubled with it — so `f` sees exactly the triangles a
    /// flat walk with the same box test would have seen, and in the same order.
    #[inline]
    pub fn for_each_in_box(&self, lo: [f32; 3], hi: [f32; 3], mut f: impl FnMut(usize)) {
        let top = self.levels.len() - 1;
        if !overlaps(&self.levels[top][0], lo, hi) {
            return;
        }
        // Deepest-first, lowest-index-first, so the callback runs in triangle order.
        let mut stack: Vec<(usize, usize)> = Vec::with_capacity(self.levels.len() * FAN);
        stack.push((top, 0));
        while let Some((level, i)) = stack.pop() {
            if level == 0 {
                f(i);
                continue;
            }
            let below = &self.levels[level - 1];
            let first = i * FAN;
            let last = (first + FAN).min(below.len());
            // Pushed in reverse so the lowest index is popped first.
            for k in (first..last).rev() {
                if overlaps(&below[k], lo, hi) {
                    stack.push((level - 1, k));
                }
            }
        }
    }
}
