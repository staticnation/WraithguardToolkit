//! The analysers: how a mesh's order does on the GPU's vertex fetch, and how much it
//! overdraws. From meshoptimizer 1.3's `indexanalyzer.cpp` (vertex fetch) and
//! `rasterizer.cpp` (overdraw, coverage; after Nicolas Capens, "Advanced Rasterization",
//! 2004). The vertex cache analyser is Gardenfell's (`triorder::analyze_vertex_cache`).
//!
//! The rasterizer is kept operation for operation, in the C++'s order and in `f32`, so the
//! counts come out as the C++'s (`tests/oracle.rs`); `f32::min`/`max` are not used where
//! the C++ compares, since they differ from it on NaN.

/// `meshopt_VertexFetchStatistics`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VertexFetchStatistics {
    pub bytes_fetched: u32,
    /// fetched bytes / vertex buffer size; best case 1.0 (each byte is fetched once)
    pub overfetch: f32,
}

/// `meshopt_OverdrawStatistics`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverdrawStatistics {
    pub pixels_covered: u32,
    pub pixels_shaded: u32,
    /// shaded pixels / covered pixels; best case 1.0
    pub overdraw: f32,
}

/// `meshopt_CoverageStatistics`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CoverageStatistics {
    /// the ratio of the viewport covered, looking down each axis
    pub coverage: [f32; 3],
    /// viewport size in mesh coordinates
    pub extent: f32,
}

/// `meshopt_analyzeVertexFetch`: a direct-mapped 128 KB cache of 64-byte lines over a
/// vertex buffer of `vertex_size`-byte vertices, read in index order.
pub fn analyze_vertex_fetch(indices: &[u32], vertex_count: usize, vertex_size: usize) -> VertexFetchStatistics {
    assert!(vertex_size > 0 && vertex_size <= 256);

    let mut result = VertexFetchStatistics::default();

    let mut vertex_visited = vec![0u8; vertex_count];

    const K_CACHE_LINE: usize = 64;
    const K_CACHE_SIZE: usize = 128 * 1024;

    // simple direct mapped cache; on typical mesh data this is close to 4-way cache, and this model is a gross approximation anyway
    let mut cache = [0usize; K_CACHE_SIZE / K_CACHE_LINE];

    for &index in indices {
        let index = index as usize;
        debug_assert!(index < vertex_count);

        vertex_visited[index] = 1;

        let start_address = index * vertex_size;
        let end_address = start_address + vertex_size;

        let start_tag = start_address / K_CACHE_LINE;
        let end_tag = (end_address + K_CACHE_LINE - 1) / K_CACHE_LINE;

        debug_assert!(start_tag < end_tag);

        for tag in start_tag..end_tag {
            let line = tag % cache.len();

            // we store +1 since cache is filled with 0 by default
            result.bytes_fetched = result.bytes_fetched.wrapping_add(((cache[line] != tag + 1) as u32) * K_CACHE_LINE as u32);
            cache[line] = tag + 1;
        }
    }

    let mut unique_vertex_count: usize = 0;

    for i in 0..vertex_count {
        unique_vertex_count += vertex_visited[i] as usize;
    }

    result.overfetch = if unique_vertex_count == 0 { 0.0 } else { result.bytes_fetched as f32 / (unique_vertex_count * vertex_size) as f32 };

    result
}

const K_VIEWPORT: usize = 256;

/// The C++'s `OverdrawBuffer`: `z[y][x][2]` and `overdraw[y][x][2]`, flattened.
struct OverdrawBuffer {
    z: Vec<f32>,
    overdraw: Vec<u32>,
}

impl OverdrawBuffer {
    fn new() -> Self {
        OverdrawBuffer { z: vec![0.0; K_VIEWPORT * K_VIEWPORT * 2], overdraw: vec![0; K_VIEWPORT * K_VIEWPORT * 2] }
    }
    fn clear(&mut self) {
        self.z.iter_mut().for_each(|v| *v = 0.0);
        self.overdraw.iter_mut().for_each(|v| *v = 0);
    }
    #[inline]
    fn at(y: usize, x: usize, s: usize) -> usize {
        (y * K_VIEWPORT + x) * 2 + s
    }
}

fn compute_depth_gradients(x1: f32, y1: f32, z1: f32, x2: f32, y2: f32, z2: f32, x3: f32, y3: f32, z3: f32) -> (f32, f32, f32) {
    // z2 = z1 + dzdx * (x2 - x1) + dzdy * (y2 - y1)
    // z3 = z1 + dzdx * (x3 - x1) + dzdy * (y3 - y1)
    // (x2-x1 y2-y1)(dzdx) = (z2-z1)
    // (x3-x1 y3-y1)(dzdy)   (z3-z1)
    // we'll solve it with Cramer's rule
    let det = (x2 - x1) * (y3 - y1) - (y2 - y1) * (x3 - x1);
    let invdet = if det == 0.0 { 0.0 } else { 1.0 / det };

    let dzdx = ((z2 - z1) * (y3 - y1) - (y2 - y1) * (z3 - z1)) * invdet;
    let dzdy = ((x2 - x1) * (z3 - z1) - (z2 - z1) * (x3 - x1)) * invdet;

    (det, dzdx, dzdy)
}

/// C++ `int(f)`: truncation toward zero (in range, the same as Rust's saturating `as`).
#[inline]
fn to_int(f: f32) -> i32 {
    f as i32
}

// half-space fixed point triangle rasterizer
#[allow(clippy::many_single_char_names)]
fn rasterize(buffer: &mut OverdrawBuffer, v1x: f32, v1y: f32, mut v1z: f32, mut v2x: f32, mut v2y: f32, v2z: f32, mut v3x: f32, mut v3y: f32, v3z: f32) {
    // compute depth gradients
    let (det, mut dzx, mut dzy) = compute_depth_gradients(v1x, v1y, v1z, v2x, v2y, v2z, v3x, v3y, v3z);
    let sign = (det > 0.0) as usize;

    // flip backfacing triangles to simplify rasterization logic
    if sign != 0 {
        // flipping v2 & v3 preserves depth gradients since they're based on v1; only v1z is used below
        std::mem::swap(&mut v2x, &mut v3x);
        std::mem::swap(&mut v2y, &mut v3y);

        // flip depth since we rasterize backfacing triangles to second buffer with reverse Z; only v1z is used below
        v1z = K_VIEWPORT as f32 - v1z;
        dzx = -dzx;
        dzy = -dzy;
    }
    // coordinates, 28.4 fixed point
    let x1 = to_int(16.0f32 * v1x + 0.5f32);
    let x2 = to_int(16.0f32 * v2x + 0.5f32);
    let x3 = to_int(16.0f32 * v3x + 0.5f32);

    let y1 = to_int(16.0f32 * v1y + 0.5f32);
    let y2 = to_int(16.0f32 * v2y + 0.5f32);
    let y3 = to_int(16.0f32 * v3y + 0.5f32);

    // bounding rectangle, clipped against viewport
    // since we rasterize pixels with covered centers, min >0.5 should round up
    // as for max, due to top-left filling convention we will never rasterize right/bottom edges
    // so max >= 0.5 should round down for inclusive bounds, and up for exclusive (in our case)
    let mut minx = if x1 < x2 { x1 } else { x2 };
    minx = if minx < x3 { minx } else { x3 };
    minx = (minx + 7) >> 4;
    minx = if minx < 0 { 0 } else { minx };

    let mut miny = if y1 < y2 { y1 } else { y2 };
    miny = if miny < y3 { miny } else { y3 };
    miny = (miny + 7) >> 4;
    miny = if miny < 0 { 0 } else { miny };

    let mut maxx = if x1 > x2 { x1 } else { x2 };
    maxx = if maxx > x3 { maxx } else { x3 };
    maxx = (maxx + 7) >> 4;
    maxx = if maxx > K_VIEWPORT as i32 { K_VIEWPORT as i32 } else { maxx };

    let mut maxy = if y1 > y2 { y1 } else { y2 };
    maxy = if maxy > y3 { maxy } else { y3 };
    maxy = (maxy + 7) >> 4;
    maxy = if maxy > K_VIEWPORT as i32 { K_VIEWPORT as i32 } else { maxy };

    // deltas, 28.4 fixed point
    let dx12 = x1 - x2;
    let dx23 = x2 - x3;
    let dx31 = x3 - x1;

    let dy12 = y1 - y2;
    let dy23 = y2 - y3;
    let dy31 = y3 - y1;

    // fill convention correction
    let tl1 = (dy12 < 0 || (dy12 == 0 && dx12 > 0)) as i32;
    let tl2 = (dy23 < 0 || (dy23 == 0 && dx23 > 0)) as i32;
    let tl3 = (dy31 < 0 || (dy31 == 0 && dx31 > 0)) as i32;

    // half edge equations, 24.8 fixed point
    // note that we offset minx/miny by half pixel since we want to rasterize pixels with covered centers
    let fx = (minx << 4) + 8;
    let fy = (miny << 4) + 8;
    let mut cy1 = dx12.wrapping_mul(fy - y1).wrapping_sub(dy12.wrapping_mul(fx - x1)).wrapping_add(tl1 - 1);
    let mut cy2 = dx23.wrapping_mul(fy - y2).wrapping_sub(dy23.wrapping_mul(fx - x2)).wrapping_add(tl2 - 1);
    let mut cy3 = dx31.wrapping_mul(fy - y3).wrapping_sub(dy31.wrapping_mul(fx - x3)).wrapping_add(tl3 - 1);
    let mut zy = v1z + (dzx * (fx - x1) as f32 + dzy * (fy - y1) as f32) * (1.0f32 / 16.0f32);

    // signed left shift is UB for negative numbers so use unsigned-signed casts
    let sx1 = ((dy12 as u32) << 4) as i32;
    let sx2 = ((dy23 as u32) << 4) as i32;
    let sx3 = ((dy31 as u32) << 4) as i32;
    let sy1 = ((dx12 as u32) << 4) as i32;
    let sy2 = ((dx23 as u32) << 4) as i32;
    let sy3 = ((dx31 as u32) << 4) as i32;

    for y in miny..maxy {
        let mut cx1 = cy1;
        let mut cx2 = cy2;
        let mut cx3 = cy3;
        let mut zx = zy;

        for x in minx..maxx {
            // check if all CXn are non-negative
            if (cx1 | cx2 | cx3) >= 0 {
                let i = OverdrawBuffer::at(y as usize, x as usize, sign);
                if zx >= buffer.z[i] {
                    buffer.z[i] = zx;
                    buffer.overdraw[i] = buffer.overdraw[i].wrapping_add(1);
                }
            }

            cx1 = cx1.wrapping_sub(sx1);
            cx2 = cx2.wrapping_sub(sx2);
            cx3 = cx3.wrapping_sub(sx3);
            zx += dzx;
        }

        cy1 = cy1.wrapping_add(sy1);
        cy2 = cy2.wrapping_add(sy2);
        cy3 = cy3.wrapping_add(sy3);
        zy += dzy;
    }
}

/// The triangles' corners scaled into the viewport, per index. Returns them and the extent.
fn transform_triangles(indices: &[u32], vertex_positions: &[f32], vertex_count: usize, vertex_stride_float: usize) -> (Vec<f32>, f32) {
    let mut triangles = vec![0.0f32; indices.len() * 3];

    let mut minv = [f32::MAX; 3];
    let mut maxv = [-f32::MAX; 3];

    for i in 0..vertex_count {
        let v = &vertex_positions[i * vertex_stride_float..];

        for j in 0..3 {
            let vj = v[j];

            minv[j] = if minv[j] > vj { vj } else { minv[j] };
            maxv[j] = if maxv[j] < vj { vj } else { maxv[j] };
        }
    }

    let mut extent = 0.0f32;

    extent = if (maxv[0] - minv[0]) < extent { extent } else { maxv[0] - minv[0] };
    extent = if (maxv[1] - minv[1]) < extent { extent } else { maxv[1] - minv[1] };
    extent = if (maxv[2] - minv[2]) < extent { extent } else { maxv[2] - minv[2] };

    let scale = if extent == 0.0 { 0.0 } else { K_VIEWPORT as f32 / extent };

    for (i, &index) in indices.iter().enumerate() {
        debug_assert!((index as usize) < vertex_count);

        let v = &vertex_positions[index as usize * vertex_stride_float..];

        triangles[i * 3] = (v[0] - minv[0]) * scale;
        triangles[i * 3 + 1] = (v[1] - minv[1]) * scale;
        triangles[i * 3 + 2] = (v[2] - minv[2]) * scale;
    }

    (triangles, extent)
}

fn rasterize_triangles(buffer: &mut OverdrawBuffer, triangles: &[f32], index_count: usize, axis: usize) {
    let mut i = 0;
    while i < index_count {
        let vn0 = &triangles[3 * i..3 * i + 3];
        let vn1 = &triangles[3 * (i + 1)..3 * (i + 1) + 3];
        let vn2 = &triangles[3 * (i + 2)..3 * (i + 2) + 3];

        match axis {
            0 => rasterize(buffer, vn0[2], vn0[1], vn0[0], vn1[2], vn1[1], vn1[0], vn2[2], vn2[1], vn2[0]),
            1 => rasterize(buffer, vn0[0], vn0[2], vn0[1], vn1[0], vn1[2], vn1[1], vn2[0], vn2[2], vn2[1]),
            2 => rasterize(buffer, vn0[1], vn0[0], vn0[2], vn1[1], vn1[0], vn1[2], vn2[1], vn2[0], vn2[2]),
            _ => {}
        }
        i += 3;
    }
}

fn check_positions(indices: &[u32], vertex_positions: &[f32], vertex_count: usize, vertex_stride_float: usize) {
    assert!(indices.len() % 3 == 0);
    assert!((3..=64).contains(&vertex_stride_float));
    assert!(vertex_count == 0 || vertex_positions.len() >= (vertex_count - 1) * vertex_stride_float + 3);
}

/// `meshopt_analyzeOverdraw`: the mesh drawn from the three axes into a 256x256 depth
/// buffer, front faces and back faces apart, counting how often each pixel is shaded.
/// `vertex_positions` is `x, y, z` per vertex, `vertex_stride_float` floats apart (3 for
/// packed positions; the C++ takes the stride in bytes).
pub fn analyze_overdraw(indices: &[u32], vertex_positions: &[f32], vertex_count: usize, vertex_stride_float: usize) -> OverdrawStatistics {
    check_positions(indices, vertex_positions, vertex_count, vertex_stride_float);

    let mut result = OverdrawStatistics::default();

    let (triangles, _) = transform_triangles(indices, vertex_positions, vertex_count, vertex_stride_float);

    let mut buffer = OverdrawBuffer::new();

    for axis in 0..3 {
        buffer.clear();
        rasterize_triangles(&mut buffer, &triangles, indices.len(), axis);

        for &overdraw in &buffer.overdraw {
            result.pixels_covered += (overdraw > 0) as u32;
            result.pixels_shaded = result.pixels_shaded.wrapping_add(overdraw);
        }
    }

    result.overdraw = if result.pixels_covered != 0 { result.pixels_shaded as f32 / result.pixels_covered as f32 } else { 0.0 };

    result
}

/// `meshopt_analyzeCoverage`: the share of the viewport the mesh covers, looking down each
/// axis, and the viewport's size in mesh units.
pub fn analyze_coverage(indices: &[u32], vertex_positions: &[f32], vertex_count: usize, vertex_stride_float: usize) -> CoverageStatistics {
    check_positions(indices, vertex_positions, vertex_count, vertex_stride_float);

    let mut result = CoverageStatistics::default();

    let (triangles, extent) = transform_triangles(indices, vertex_positions, vertex_count, vertex_stride_float);

    let mut buffer = OverdrawBuffer::new();

    for axis in 0..3 {
        buffer.clear();
        rasterize_triangles(&mut buffer, &triangles, indices.len(), axis);

        let mut covered: u32 = 0;

        for p in 0..K_VIEWPORT * K_VIEWPORT {
            covered += ((buffer.overdraw[p * 2] | buffer.overdraw[p * 2 + 1]) > 0) as u32;
        }

        result.coverage[axis] = covered as f32 / (K_VIEWPORT * K_VIEWPORT) as f32;
    }

    result.extent = extent;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_facing_one_axis() {
        // A unit square in the XY plane, two triangles: seen down Z it covers the whole
        // viewport once; down X and Y it is edge-on.
        let pos = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0f32];
        let idx = [0u32, 1, 2, 0, 2, 3];
        let o = analyze_overdraw(&idx, &pos, 4, 3);
        assert_eq!(o.pixels_covered, 256 * 256);
        assert_eq!(o.pixels_shaded, 256 * 256);
        assert_eq!(o.overdraw, 1.0);
        let c = analyze_coverage(&idx, &pos, 4, 3);
        assert_eq!(c.extent, 1.0);
        assert_eq!(c.coverage[2], 1.0);
    }

    #[test]
    fn fetch_in_order_reads_each_byte_once() {
        let idx: Vec<u32> = (0..300).collect();
        let s = analyze_vertex_fetch(&idx, 300, 32);
        assert_eq!(s.bytes_fetched, 300 * 32);
        assert_eq!(s.overfetch, 1.0);
    }
}
