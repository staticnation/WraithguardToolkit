//! How much structure a height surface has at a vertex (`land/curvature.py`): the mean angle
//! between its normal and its four neighbours'. Weighs a `Curvature` merge and, on request,
//! the slope limiter's split.
//!
//! In glam's `DVec3` (GLAM-PLAN step 1): f64 throughout, since these values decide terrain
//! merges, and the same operations in the same order as the arrays they replaced (a
//! normal divided by its length, not multiplied by its reciprocal), checked bit for bit.

use glam::DVec3;

use crate::grid::HEIGHT_SCALE;

/// `_normal_at`, over a flat `side`x`side` grid of heights.
fn normal_at(rows: &[f64], side: usize, x: usize, y: usize) -> DVec3 {
    let limit = side - 1;
    let fx = if x == limit { x - 1 } else { x };
    let fy = if y == limit { y - 1 } else { y };
    let scale = HEIGHT_SCALE as f64;
    let step = 128.0 / scale;
    let here = rows[fy * side + fx] / scale;
    let east = rows[fy * side + fx + 1] / scale;
    let north = rows[(fy + 1) * side + fx] / scale;
    let n = DVec3::new(-(east - here) * step, -(north - here) * step, step * step);
    let length = n.length();
    if length == 0.0 {
        return DVec3::Z;
    }
    n / length
}

/// `curvature_at`: the mean angle between a vertex's normal and its neighbours'.
pub fn curvature_at(rows: &[f64], side: usize, x: usize, y: usize) -> f64 {
    let here = normal_at(rows, side, x, y);
    let (mut total, mut counted) = (0.0, 0);
    for (dx, dy) in [(1i64, 0i64), (0, 1), (-1, 0), (0, -1)] {
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        if nx < 0 || ny < 0 || nx >= side as i64 || ny >= side as i64 {
            continue;
        }
        let o = normal_at(rows, side, nx as usize, ny as usize);
        total += here.dot(o).clamp(-1.0, 1.0).acos();
        counted += 1;
    }
    if counted > 0 { total / counted as f64 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tuple version this replaced, kept to check the glam one against.
    mod before {
        use crate::grid::HEIGHT_SCALE;
        /// `_normal_at`, over a flat `side`x`side` grid of heights.
        pub fn normal_at(rows: &[f64], side: usize, x: usize, y: usize) -> (f64, f64, f64) {
            let limit = side - 1;
            let fx = if x == limit { x - 1 } else { x };
            let fy = if y == limit { y - 1 } else { y };
            let scale = HEIGHT_SCALE as f64;
            let step = 128.0 / scale;
            let here = rows[fy * side + fx] / scale;
            let east = rows[fy * side + fx + 1] / scale;
            let north = rows[(fy + 1) * side + fx] / scale;
            let nx = -(east - here) * step;
            let ny = -(north - here) * step;
            let nz = step * step;
            let length = (nx * nx + ny * ny + nz * nz).sqrt();
            if length == 0.0 {
                return (0.0, 0.0, 1.0);
            }
            (nx / length, ny / length, nz / length)
        }

        /// `curvature_at`: the mean angle between a vertex's normal and its neighbours'.
        pub fn curvature_at(rows: &[f64], side: usize, x: usize, y: usize) -> f64 {
            let here = normal_at(rows, side, x, y);
            let (mut total, mut counted) = (0.0, 0);
            for (dx, dy) in [(1i64, 0i64), (0, 1), (-1, 0), (0, -1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= side as i64 || ny >= side as i64 {
                    continue;
                }
                let o = normal_at(rows, side, nx as usize, ny as usize);
                let dot = here.0 * o.0 + here.1 * o.1 + here.2 * o.2;
                total += dot.clamp(-1.0, 1.0).acos();
                counted += 1;
            }
            if counted > 0 { total / counted as f64 } else { 0.0 }
        }
    }

    #[test]
    fn glam_gives_the_old_answers_bit_for_bit() {
        let side = 65;
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for case in 0..8 {
            let rows: Vec<f64> = (0..side * side)
                .map(|i| match case {
                    0 => 0.0,
                    1 => (i % side) as f64 * 40.0,
                    2 => if (i / side) % 2 == 0 { 30_000.0 } else { -30_000.0 },
                    _ => (next() % 40_000) as f64 - 20_000.0,
                })
                .collect();
            for y in 0..side {
                for x in 0..side {
                    let (a, b) = (normal_at(&rows, side, x, y), before::normal_at(&rows, side, x, y));
                    assert_eq!((a.x.to_bits(), a.y.to_bits(), a.z.to_bits()), (b.0.to_bits(), b.1.to_bits(), b.2.to_bits()), "case {case} normal at {x},{y}");
                    assert_eq!(curvature_at(&rows, side, x, y).to_bits(), before::curvature_at(&rows, side, x, y).to_bits(), "case {case} at {x},{y}");
                }
            }
        }
    }
}
