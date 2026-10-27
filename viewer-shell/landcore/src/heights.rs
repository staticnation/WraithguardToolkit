//! Heights and normals (`land/heights.py`, `tes3fields/landscape.py`'s VHGT decoder, the
//! normals of `native/src/land.rs`).
//!
//! `VHGT` stores an `f32` offset and 65x65 signed-byte deltas, *doubly* cumulative: each
//! row's first delta accumulates onto a carried row height, the rest of the row from there.
//! Every value is scaled by 8. Python's rounding is kept to the bit: `int()` truncates, `//`
//! floors, `x ** 0.5` is `pow`.

use crate::grid::{HEIGHT_SCALE, LAND_NUM_VERTS, LAND_SIZE};

/// The largest step `VHGT` stores between two vertices, in stored units.
pub const MAX_GRADIENT: i64 = 127;
pub const MIN_GRADIENT: i64 = -128;

/// `VHGT` to absolute heights, flat, truncated to ints as `LandscapeLayers` holds them;
/// `Err` for a non-finite height (the Python's `ValueError`).
pub fn decode_heights(raw: &[u8], offset: f64) -> Result<Vec<i64>, String> {
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
            out.push(v.trunc() as i64);
        }
    }
    Ok(out)
}

/// `encode_vertex_heights`: the offset, the 65x65 deltas, and the vertices whose step had
/// to be clamped to fit a signed byte. Heights are whole world units here.
pub fn encode_vertex_heights(heights: &[i64]) -> (f32, Vec<i8>, Vec<(usize, usize)>) {
    let stored: Vec<i64> = heights.iter().map(|&v| v.div_euclid(HEIGHT_SCALE)).collect();
    let at = |x: usize, y: usize| stored[y * LAND_SIZE + x];
    let offset = at(0, 0);
    let mut deltas = vec![0i8; LAND_NUM_VERTS];
    let mut clamped = Vec::new();
    let mut fit = |value: i64, x: usize, y: usize| -> i8 {
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
fn to_signed_byte(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else if v >= 127.0 {
        127
    } else if v <= -128.0 {
        -128
    } else {
        v.trunc() as i64
    }
}

/// `vertex_normals_from_heights`, flat: 65x65 heights in, 65x65x3 normals out.
pub fn normals_from_heights(h: &[f64]) -> Vec<i64> {
    let scale = HEIGHT_SCALE as f64;
    let step = 128.0 / scale;
    let limit = LAND_SIZE - 1;
    let fin = |v: f64| if v.is_finite() { v } else { 0.0 };
    let mut out = Vec::with_capacity(h.len() * 3);
    for y in 0..LAND_SIZE {
        let fy = if y == limit { y - 1 } else { y };
        for x in 0..LAND_SIZE {
            let fx = if x == limit { x - 1 } else { x };
            let here = fin(h[fy * LAND_SIZE + fx]) / scale;
            let east = fin(h[fy * LAND_SIZE + fx + 1]) / scale;
            let north = fin(h[(fy + 1) * LAND_SIZE + fx]) / scale;
            let nx = -(east - here) * step;
            let ny = -(north - here) * step;
            let nz = step * step;
            // `** 0.5` in the Python is `pow`, which is not always `sqrt` to the bit.
            let length = (nx * nx + ny * ny + nz * nz).powf(0.5);
            if length == 0.0 {
                out.extend_from_slice(&[0, 0, 127]);
                continue;
            }
            let unit = length / 127.0;
            out.push(to_signed_byte(nx / unit));
            out.push(to_signed_byte(ny / unit));
            out.push(to_signed_byte(nz / unit));
        }
    }
    out
}

/// One merged cell's normals (`resolve_cell_normals`): recomputed from its final heights,
/// keeping the reference's own normal wherever the height did not move and that normal is not
/// `(0, 0, 0)`. Returns the normals and how many were kept.
pub fn resolve_cell_normals(heights: &[i64], original: Option<&[i64]>, base: Option<&[i64]>) -> (Vec<i64>, usize) {
    let hf: Vec<f64> = heights.iter().map(|&v| v as f64).collect();
    let mut normals = normals_from_heights(&hf);
    let mut preserved = 0;
    if let (Some(orig), Some(base)) = (original, base)
        && base.len() == heights.len()
        && orig.len() >= normals.len()
    {
        for i in 0..heights.len() {
            if heights[i] != base[i] {
                continue;
            }
            let t = &orig[i * 3..i * 3 + 3];
            if t == [0, 0, 0] {
                continue;
            }
            normals[i * 3..i * 3 + 3].copy_from_slice(t);
            preserved += 1;
        }
    }
    (normals, preserved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_round_trip() {
        let heights: Vec<i64> = (0..LAND_NUM_VERTS).map(|i| ((i % 65) as i64 * 37 - (i / 65) as i64 * 11) * 8 - 3000).collect();
        let (offset, deltas, clamped) = encode_vertex_heights(&heights);
        assert!(clamped.is_empty());
        let raw: Vec<u8> = deltas.iter().map(|&d| d as u8).collect();
        assert_eq!(decode_heights(&raw, offset as f64).unwrap(), heights);
        // Python's // floors: -3 // 8 is -1.
        assert_eq!(encode_vertex_heights(&vec![-3; LAND_NUM_VERTS]).0, -1.0);
    }

    #[test]
    fn steep_steps_are_clamped() {
        let mut h = vec![0; LAND_NUM_VERTS];
        h[1] = 8 * 500;
        let (_, d, clamped) = encode_vertex_heights(&h);
        assert_eq!((d[1], d[2]), (127, -128));
        assert_eq!(clamped, vec![(1, 0), (2, 0)]);
    }

    #[test]
    fn flat_normals_point_up() {
        let n = normals_from_heights(&vec![0.0; LAND_NUM_VERTS]);
        assert_eq!(&n[..3], &[0, 0, 127]);
    }
}
