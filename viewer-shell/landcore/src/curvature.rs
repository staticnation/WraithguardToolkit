//! How much structure a height surface has at a vertex (`land/curvature.py`): the mean angle
//! between its normal and its four neighbours'. Weighs a `Curvature` merge and, on request,
//! the slope limiter's split.

use crate::grid::HEIGHT_SCALE;

/// `_normal_at`, over a flat `side`x`side` grid of heights.
fn normal_at(rows: &[f64], side: usize, x: usize, y: usize) -> (f64, f64, f64) {
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
