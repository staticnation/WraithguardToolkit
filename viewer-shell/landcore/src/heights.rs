//! Heights and normals (`land/heights.py`, `tes3fields/landscape.py`'s VHGT decoder, the
//! normals of `native/src/land.rs`).
//!
//! `VHGT` stores an `f32` offset and 65x65 signed-byte deltas, *doubly* cumulative: each
//! row's first delta accumulates onto a carried row height, the rest of the row from there.
//! Every value is scaled by 8. Python's rounding is kept to the bit: `int()` truncates, `//`
//! floors, `x ** 0.5` is `pow`.

use crate::grid::{HEIGHT_SCALE, LAND_NUM_VERTS, LAND_SIZE};
use crate::grid::Value;

/// The largest step `VHGT` stores between two vertices, in stored units.
pub const MAX_GRADIENT: Value = 127;
pub const MIN_GRADIENT: Value = -128;

/// `VHGT` to absolute heights, flat, truncated to ints as `LandscapeLayers` holds them;
/// `Err` for a non-finite height (the Python's `ValueError`).
pub fn decode_heights(raw: &[u8], offset: f64) -> Result<Vec<Value>, String> {
    if raw.len() < LAND_NUM_VERTS {
        return Err(format!("VHGT height data is {} bytes, needs {LAND_NUM_VERTS}", raw.len()));
    }
    let mut out = Vec::with_capacity(LAND_NUM_VERTS);
    let mut row_height = offset;
    for y in 0..LAND_SIZE {
        let base = y * LAND_SIZE;
        row_height += raw[base] as i8 as f64;
        let mut height = row_height;
        for x in 0..LAND_SIZE {
            if x > 0 {
                height += raw[base + x] as i8 as f64;
            }
            let v = height * HEIGHT_SCALE as f64;
            if !v.is_finite() {
                return Err("cannot convert float NaN or infinity to integer".into());
            }
            // A grid value is an `i32` (`grid::Value`): a broken offset far past any terrain
            // is refused like a non-finite one rather than wrapped in the merge's sums.
            if v.abs() > 2.0e8 {
                return Err(format!("a height of {v} is beyond what a land grid holds (an offset of {offset})"));
            }
            out.push(v.trunc() as Value);
        }
    }
    Ok(out)
}

/// The height `VHGT` can store at or below `v`: the encoder's own floor onto the eight-unit grid.
pub fn quantize_height(v: Value) -> Value {
    v.div_euclid(HEIGHT_SCALE) * HEIGHT_SCALE
}

/// Put every height on the grid [`encode_vertex_heights`] writes, in place. Equal heights stay
/// equal, so every copy of a vertex shared between cells keeps agreeing with the others.
/// Everything that reads the terrain after the merge (normals, seams checked, cleaning) is to
/// read it from here, so it sees what is saved and not what the encoder later makes of it.
pub fn finalize_heights(heights: &mut [Value]) {
    for v in heights {
        *v = quantize_height(*v);
    }
}

/// `heights` as [`finalize_heights`] leaves them.
pub fn finalized(heights: &[Value]) -> Vec<Value> {
    heights.iter().map(|&v| quantize_height(v)).collect()
}

/// `encode_vertex_heights`: the offset, the 65x65 deltas, and the vertices whose step had
/// to be clamped to fit a signed byte. Heights are whole world units here.
pub fn encode_vertex_heights(heights: &[Value]) -> (f32, Vec<i8>, Vec<(usize, usize)>) {
    let stored: Vec<Value> = heights.iter().map(|&v| v.div_euclid(HEIGHT_SCALE)).collect();
    let at = |x: usize, y: usize| stored[y * LAND_SIZE + x];
    let offset = at(0, 0);
    let mut deltas = vec![0i8; LAND_NUM_VERTS];
    let mut clamped = Vec::new();
    let mut fit = |value: Value, x: usize, y: usize| -> i8 {
        if value > MAX_GRADIENT {
            clamped.push((x, y));
            return MAX_GRADIENT as i8;
        }
        if value < MIN_GRADIENT {
            clamped.push((x, y));
            return MIN_GRADIENT as i8;
        }
        value as i8
    };
    for y in 1..LAND_SIZE {
        deltas[y * LAND_SIZE] = fit(at(0, y) - offset - (at(0, y - 1) - offset), 0, y);
    }
    for y in 0..LAND_SIZE {
        for x in 1..LAND_SIZE {
            deltas[y * LAND_SIZE + x] = fit(at(x, y) - at(x - 1, y), x, y);
        }
    }
    (offset as f32, deltas, clamped)
}

/// `_to_signed_byte`: saturate to a signed byte, NaN to zero, truncating.
fn to_signed_byte(v: f64) -> Value {
    if v.is_nan() {
        0
    } else if v >= 127.0 {
        127
    } else if v <= -128.0 {
        -128
    } else {
        v.trunc() as Value
    }
}

/// `vertex_normals_from_heights`, flat: 65x65 heights in, 65x65x3 normals out.
pub fn normals_from_heights(h: &[f64]) -> Vec<Value> {
    let fin = |v: f64| if v.is_finite() { v } else { 0.0 };
    let mut out = Vec::with_capacity(h.len() * 3);
    for y in 0..LAND_SIZE {
        for x in 0..LAND_SIZE {
            out.extend_from_slice(&normal_at(&|i| fin(h[i]), x, y));
        }
    }
    out
}

/// The normal at (`x`, `y`) of the heights `h` gives (by vertex index): from the vertex and
/// its east and north neighbours (the last row and column from the ones before them).
fn normal_at(h: &dyn Fn(usize) -> f64, x: usize, y: usize) -> [Value; 3] {
    let scale = HEIGHT_SCALE as f64;
    let step = 128.0 / scale;
    let limit = LAND_SIZE - 1;
    let fy = if y == limit { y - 1 } else { y };
    let fx = if x == limit { x - 1 } else { x };
    let here = h(fy * LAND_SIZE + fx) / scale;
    let east = h(fy * LAND_SIZE + fx + 1) / scale;
    let north = h((fy + 1) * LAND_SIZE + fx) / scale;
    let nx = -(east - here) * step;
    let ny = -(north - here) * step;
    let nz = step * step;
    // `** 0.5` in the Python is `pow`, which is not always `sqrt` to the bit.
    let length = (nx * nx + ny * ny + nz * nz).powf(0.5);
    if length == 0.0 {
        return [0, 0, 127];
    }
    let unit = length / 127.0;
    [to_signed_byte(nx / unit), to_signed_byte(ny / unit), to_signed_byte(nz / unit)]
}

/// The three vertices [`normal_at`] reads for the normal at (`x`, `y`): the vertex, its east and
/// its north neighbour, the last row and column using the triangle before them.
pub fn normal_triangle(x: usize, y: usize) -> [usize; 3] {
    let limit = LAND_SIZE - 1;
    let fy = if y == limit { y - 1 } else { y };
    let fx = if x == limit { x - 1 } else { x };
    [fy * LAND_SIZE + fx, fy * LAND_SIZE + fx + 1, (fy + 1) * LAND_SIZE + fx]
}

/// Where a cell's authored normals may be kept from: a record's normals and the heights they
/// were made for, read in place (`len` the values held; a source whose heights are not the
/// cell's size or whose normals are fewer than three a vertex has none to give).
#[derive(Clone, Copy)]
pub struct NormalSource<'a> {
    pub normals: &'a dyn Fn(usize) -> Value,
    pub normals_len: usize,
    pub heights: &'a dyn Fn(usize) -> Value,
    pub heights_len: usize,
}

/// One merged cell's normals (`resolve_cell_normals`): recomputed from its final heights,
/// keeping the reference's own normal wherever the terrain it describes did not move and that
/// normal is not `(0, 0, 0)`. Returns the normals and how many were kept.
pub fn resolve_cell_normals(heights: &[Value], original: Option<&[Value]>, base: Option<&[Value]>) -> (Vec<Value>, usize) {
    let (o_at, b_at) = (|i: usize| original.map_or(0, |o| o[i]), |i: usize| base.map_or(0, |b| b[i]));
    let source = match (original, base) {
        (Some(o), Some(b)) => Some(NormalSource { normals: &o_at, normals_len: o.len(), heights: &b_at, heights_len: b.len() }),
        _ => None,
    };
    resolve_cell_normals_from(heights, source.as_slice())
}

/// [`resolve_cell_normals`] over any number of `sources`, tried in order (the record the engine
/// loads first, then the masters'). A normal is kept from a source only where all three heights
/// it is worked out from ([`normal_triangle`]) are the cell's own, so a neighbour that moved
/// re-lights the vertex beside it, and the reused edge triangle is checked as the one it is. A
/// source's heights are compared as they would be saved (floored to the eight-unit grid) as well
/// as exactly. A normal is worked out only where no source's is kept (Merge Lands' finish step,
/// 2026-11-10: the normals were a third of it).
pub fn resolve_cell_normals_from(heights: &[Value], sources: &[NormalSource]) -> (Vec<Value>, usize) {
    let mut normals = vec![0; heights.len() * 3];
    let mut preserved = 0;
    let usable: Vec<&NormalSource> = sources.iter().filter(|s| s.heights_len == heights.len() && s.normals_len >= normals.len()).collect();
    let h = |i: usize| heights[i] as f64;
    for y in 0..LAND_SIZE {
        for x in 0..LAND_SIZE {
            let i = y * LAND_SIZE + x;
            let triangle = normal_triangle(x, y);
            let kept = usable.iter().find_map(|s| {
                let same = triangle.iter().all(|&v| {
                    let theirs = (s.heights)(v);
                    heights[v] == theirs || heights[v] == quantize_height(theirs)
                });
                let t = [(s.normals)(i * 3), (s.normals)(i * 3 + 1), (s.normals)(i * 3 + 2)];
                (same && t != [0, 0, 0]).then_some(t)
            });
            match kept {
                Some(t) => {
                    normals[i * 3..i * 3 + 3].copy_from_slice(&t);
                    preserved += 1;
                }
                None => normals[i * 3..i * 3 + 3].copy_from_slice(&normal_at(&h, x, y)),
            }
        }
    }
    (normals, preserved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_round_trip() {
        let heights: Vec<Value> = (0..LAND_NUM_VERTS).map(|i| ((i % 65) as Value * 37 - (i / 65) as Value * 11) * 8 - 3000).collect();
        let (offset, deltas, clamped) = encode_vertex_heights(&heights);
        assert!(clamped.is_empty());
        let raw: Vec<u8> = deltas.iter().map(|&d| d as u8).collect();
        assert_eq!(decode_heights(&raw, offset as f64).unwrap(), heights);
        // Python's // floors: -3 // 8 is -1.
        assert_eq!(encode_vertex_heights(&vec![-3; LAND_NUM_VERTS]).0, -1.0);
    }

    /// Finalized heights are what a decode of their own encoding gives back, so nothing
    /// computed from them (normals above all) can go stale in the encoder.
    #[test]
    fn finalized_heights_survive_the_encoder() {
        // Rough (every vertex off the grid) but with steps a byte can hold.
        let rough: Vec<Value> = (0..LAND_NUM_VERTS).map(|i| ((i * 7919) % 801) as Value - 400).collect();
        let fin = finalized(&rough);
        assert!(fin.iter().all(|v| v % HEIGHT_SCALE == 0));
        assert!(fin.iter().zip(&rough).all(|(f, r)| f <= r && r - f < HEIGHT_SCALE));
        let (offset, deltas, clamped) = encode_vertex_heights(&fin);
        assert!(clamped.is_empty());
        let raw: Vec<u8> = deltas.iter().map(|&d| d as u8).collect();
        assert_eq!(decode_heights(&raw, offset as f64).unwrap(), fin);
        // Equal in, equal out: a vertex two cells share cannot come apart.
        assert_eq!(quantize_height(-3), quantize_height(-3));
        assert_eq!(quantize_height(-3), -8);
        assert_eq!(quantize_height(13), 8);
    }

    #[test]
    fn steep_steps_are_clamped() {
        let mut h = vec![0; LAND_NUM_VERTS];
        h[1] = 8 * 500;
        let (_, d, clamped) = encode_vertex_heights(&h);
        assert_eq!((d[1], d[2]), (127, -128));
        assert_eq!(clamped, vec![(1, 0), (2, 0)]);
    }

    /// A ramp of heights, the normal marked at every vertex: what a source says it kept.
    fn marked(base: &[Value], marker: [Value; 3]) -> (Vec<Value>, Vec<Value>) {
        (base.to_vec(), (0..LAND_NUM_VERTS).flat_map(|_| marker).collect())
    }

    fn resolve(heights: &[Value], sources: &[(&[Value], &[Value])]) -> (Vec<Value>, usize) {
        let closures: Vec<_> = sources.iter().map(|(h, n)| (move |i: usize| h[i], move |i: usize| n[i], h.len(), n.len())).collect();
        let list: Vec<NormalSource> = closures.iter().map(|(h, n, hl, nl)| NormalSource { normals: n, normals_len: *nl, heights: h, heights_len: *hl }).collect();
        resolve_cell_normals_from(heights, &list)
    }

    fn marker_at(n: &[Value], x: usize, y: usize) -> bool {
        let i = (y * LAND_SIZE + x) * 3;
        n[i..i + 3] == [5, 6, 100]
    }

    /// WG-A03: a normal is made from the vertex, its east and its north neighbour, so a vertex
    /// whose own height did not move is re-lit when a neighbour it reads did.
    #[test]
    fn a_neighbour_that_moved_relights_a_vertex_that_did_not() {
        let base: Vec<Value> = (0..LAND_NUM_VERTS).map(|i| ((i % LAND_SIZE) * 8 + (i / LAND_SIZE) * 16) as Value).collect();
        let (bh, bn) = marked(&base, [5, 6, 100]);
        let mut moved = base.clone();
        moved[20 * LAND_SIZE + 10] += 80; // (10, 20)
        let (n, kept) = resolve(&moved, &[(&bh, &bn)]);
        // (10, 20) itself, (9, 20) whose east it is, and (10, 19) whose north it is: re-lit.
        for (x, y) in [(10, 20), (9, 20), (10, 19)] {
            assert!(!marker_at(&n, x, y), "({x}, {y})");
        }
        // Everything else is the source's, and the count says so.
        assert!(marker_at(&n, 11, 20) && marker_at(&n, 10, 21) && marker_at(&n, 0, 0) && marker_at(&n, 64, 64));
        assert_eq!(kept, LAND_NUM_VERTS - 3);
    }

    /// The last row and column reuse the triangle before them: the edge normal follows the
    /// vertex at 63 (and, in the last row, the row below it), and the corner at 64, 64, which
    /// no normal reads, moves nothing.
    #[test]
    fn the_reused_edge_triangles_are_checked_as_they_are() {
        let base: Vec<Value> = (0..LAND_NUM_VERTS).map(|i| ((i % LAND_SIZE) * 8 + (i / LAND_SIZE) * 16) as Value).collect();
        let (bh, bn) = marked(&base, [5, 6, 100]);
        // (63, 30) moved: (64, 30) reuses the triangle at 63 and is re-lit with it.
        let mut m = base.clone();
        m[30 * LAND_SIZE + 63] += 80;
        let (n, _) = resolve(&m, &[(&bh, &bn)]);
        // (64, 29) too: its triangle is the one at 63, whose north is the vertex that moved.
        for (x, y) in [(63, 30), (62, 30), (63, 29), (64, 30), (64, 29)] {
            assert!(!marker_at(&n, x, y), "({x}, {y})");
        }
        assert!(marker_at(&n, 64, 31) && marker_at(&n, 61, 30));
        // (64, 64) is read by no normal.
        let mut m = base.clone();
        m[LAND_NUM_VERTS - 1] += 80;
        let (n, kept) = resolve(&m, &[(&bh, &bn)]);
        assert_eq!(kept, LAND_NUM_VERTS);
        assert!(marker_at(&n, 64, 64));
        // (63, 63) is: the last row and column all read the triangle it starts.
        let mut m = base.clone();
        m[63 * LAND_SIZE + 63] += 80;
        let (n, _) = resolve(&m, &[(&bh, &bn)]);
        for (x, y) in [(63, 63), (64, 63), (63, 64), (64, 64)] {
            assert!(!marker_at(&n, x, y), "({x}, {y})");
        }
    }

    /// The first source whose triangle still holds gives the normal; a zero normal gives none.
    #[test]
    fn sources_are_tried_in_order_and_a_zero_normal_is_no_normal() {
        let base: Vec<Value> = vec![0; LAND_NUM_VERTS];
        let winner = (base.clone(), (0..LAND_NUM_VERTS).flat_map(|_| [5, 6, 100]).collect::<Vec<Value>>());
        let master = (base.clone(), (0..LAND_NUM_VERTS).flat_map(|_| [1, 2, 120]).collect::<Vec<Value>>());
        let (n, kept) = resolve(&base, &[(&winner.0, &winner.1), (&master.0, &master.1)]);
        assert!(marker_at(&n, 3, 3) && kept == LAND_NUM_VERTS);
        // The winner's heights are not the cell's: the master's normal is the one kept.
        let mut other = base.clone();
        other[3 * LAND_SIZE + 3] = 800;
        let (n, _) = resolve(&base, &[(&other, &winner.1), (&master.0, &master.1)]);
        assert_eq!(&n[(3 * LAND_SIZE + 3) * 3..][..3], &[1, 2, 120]);
        // A zero normal is not kept: it is worked out (flat ground, straight up).
        let zeros = vec![0; LAND_NUM_VERTS * 3];
        let (n, kept) = resolve(&base, &[(&base, &zeros)]);
        assert_eq!((kept, &n[..3]), (0, &[0, 0, 127][..]));
        // A finalized height is the source's own height floored: it still counts as the same.
        let mut raw = base.clone();
        raw.iter_mut().for_each(|v| *v = 13);
        let floored: Vec<Value> = raw.iter().map(|&v| quantize_height(v)).collect();
        let (_, kept) = resolve(&floored, &[(&raw, &winner.1)]);
        assert_eq!(kept, LAND_NUM_VERTS);
    }

    #[test]
    fn flat_normals_point_up() {
        let n = normals_from_heights(&vec![0.0; LAND_NUM_VERTS]);
        assert_eq!(&n[..3], &[0, 0, 127]);
    }
}
