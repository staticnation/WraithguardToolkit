//! The order a part's triangles are drawn in (round 18ho).
//!
//! A GPU keeps the last few vertices it transformed in a small cache and reuses one when
//! the next triangle names it again. Morrowind's own meshes, and the older mods', come
//! out of their 2002 exporters with the triangles in whatever order the modelling tool
//! kept them, and the cache gets little of it: measured over 1,005 vanilla draw parts of
//! 500 triangles or more, a vertex is shaded 1.35 times per triangle as the files have
//! it and 0.93 times after this pass, thirty percent of the vertex work. Meshes exported
//! this decade (The Garden of Dreams' rock walls, the modern grass packs) are unwelded,
//! every vertex shaded once already, and come through unchanged; so does the picture,
//! since the triangles and the vertices are the same and only their order moves. The
//! vertex order is never touched: skins, morphs and the UV sets index vertices.
//!
//! Two passes, both ports of meshoptimizer's (zeux, MIT; `vcacheoptimizer.cpp` and
//! `overdrawoptimizer.cpp` of version 1.3, kept line for line so its analyzers are this
//! module's oracle, see `tests/triorder.rs`):
//!
//! - `optimize_vertex_cache`: Tom Forsyth's linear-speed vertex cache ordering (2006) with
//!   the score table meshoptimizer tuned for the cache profile of NVidia's and AMD's
//!   hardware. Integer arithmetic throughout, so the output is bit for bit the C++'s.
//! - `optimize_overdraw`: Sander, Nehab and Barczak's cluster sort (2007). The ordered
//!   triangles are cut into clusters where the cache would miss three vertices at once,
//!   the clusters split further until each is within a threshold of the whole mesh's
//!   cache rate, and the clusters sorted so the ones facing outward from the mesh's
//!   centroid come first: the ones in front for most viewpoints, so early depth testing
//!   rejects what they cover. Float sums of triangle areas decide the sort, so this one
//!   is checked against the C++ by its cache statistics rather than by its bytes.
//!
//! `analyze_vertex_cache` is the analyzer the measurements above were made with: a FIFO
//! model of a 16-entry cache, counting the vertices transformed.

/// meshoptimizer's `kVertexScoreTable`: the value of a vertex by its position in the
/// cache (index 0 is "not in the cache") and by how many triangles still need it.
const SCORE_CACHE: [f32; 17] = [
    0.0, 0.779, 0.791, 0.789, 0.981, 0.843, 0.726, 0.847, 0.882, 0.867, 0.799, 0.642, 0.613, 0.600, 0.568, 0.372,
    0.234,
];
const SCORE_LIVE: [f32; 9] = [0.0, 0.995, 0.713, 0.450, 0.404, 0.059, 0.005, 0.147, 0.006];
const CACHE_SIZE: usize = 16;
const VALENCE_MAX: usize = 8;

struct Adjacency {
    counts: Vec<u32>,
    offsets: Vec<u32>,
    data: Vec<u32>,
}

fn build_adjacency(indices: &[u32], vertex_count: usize) -> Adjacency {
    let face_count = indices.len() / 3;
    let mut counts = vec![0u32; vertex_count];
    for &i in indices {
        counts[i as usize] += 1;
    }
    let mut offsets = vec![0u32; vertex_count];
    let mut offset = 0u32;
    for i in 0..vertex_count {
        offsets[i] = offset;
        offset += counts[i];
    }
    let mut data = vec![0u32; indices.len()];
    for f in 0..face_count {
        for k in 0..3 {
            let v = indices[f * 3 + k] as usize;
            data[offsets[v] as usize] = f as u32;
            offsets[v] += 1;
        }
    }
    for i in 0..vertex_count {
        offsets[i] -= counts[i];
    }
    Adjacency { counts, offsets, data }
}

fn vertex_score(cache_position: i32, live_triangles: u32) -> f32 {
    let live = (live_triangles as usize).min(VALENCE_MAX);
    SCORE_CACHE[(1 + cache_position) as usize] + SCORE_LIVE[live]
}

/// The triangles reordered for the post-transform cache. `indices` is a triangle list
/// over `vertex_count` vertices; the result is the same triangles in a new order, each
/// keeping its own winding. An empty list comes back empty.
pub fn optimize_vertex_cache(indices: &[u32], vertex_count: usize) -> Vec<u32> {
    debug_assert!(indices.len() % 3 == 0);
    let face_count = indices.len() / 3;
    let mut out = vec![0u32; indices.len()];
    if face_count == 0 || vertex_count == 0 {
        return out;
    }
    let mut adj = build_adjacency(indices, vertex_count);
    // Live triangle counts alias the adjacency counts: a triangle is removed from the
    // lists as it is emitted, so the counts stay the live counts.
    let mut emitted = vec![false; face_count];
    let mut vertex_scores: Vec<f32> = (0..vertex_count).map(|i| vertex_score(-1, adj.counts[i])).collect();
    let mut triangle_scores: Vec<f32> = (0..face_count)
        .map(|f| {
            vertex_scores[indices[f * 3] as usize]
                + vertex_scores[indices[f * 3 + 1] as usize]
                + vertex_scores[indices[f * 3 + 2] as usize]
        })
        .collect();
    let mut cache = [0u32; CACHE_SIZE + 4];
    let mut cache_new = [0u32; CACHE_SIZE + 4];
    let mut cache_count = 0usize;
    let mut current: u32 = 0;
    let mut input_cursor: usize = 1;
    let mut output_triangle = 0usize;
    while current != u32::MAX {
        let t = current as usize;
        let (a, b, c) = (indices[t * 3], indices[t * 3 + 1], indices[t * 3 + 2]);
        out[output_triangle * 3] = a;
        out[output_triangle * 3 + 1] = b;
        out[output_triangle * 3 + 2] = c;
        output_triangle += 1;
        emitted[t] = true;
        triangle_scores[t] = 0.0;
        // The new triangle's vertices go to the front of the cache, the rest shift back.
        let mut cache_write = 0usize;
        cache_new[cache_write] = a;
        cache_write += 1;
        cache_new[cache_write] = b;
        cache_write += 1;
        cache_new[cache_write] = c;
        cache_write += 1;
        for &index in &cache[..cache_count] {
            cache_new[cache_write] = index;
            cache_write += ((index != a) & (index != b) & (index != c)) as usize;
        }
        std::mem::swap(&mut cache, &mut cache_new);
        cache_count = cache_write.min(CACHE_SIZE);
        // Take the emitted triangle out of its three vertices' lists.
        for k in 0..3 {
            let index = indices[t * 3 + k] as usize;
            let begin = adj.offsets[index] as usize;
            let n = adj.counts[index] as usize;
            for i in 0..n {
                if adj.data[begin + i] == current {
                    adj.data[begin + i] = adj.data[begin + n - 1];
                    adj.counts[index] -= 1;
                    break;
                }
            }
        }
        // Rescore the vertices in the cache and their triangles; the best triangle is next.
        let mut best_triangle = u32::MAX;
        let mut best_score = 0.0f32;
        for (i, &slot) in cache.iter().enumerate().take(cache_write) {
            let index = slot as usize;
            if adj.counts[index] == 0 {
                continue;
            }
            let cache_position = if i >= CACHE_SIZE { -1 } else { i as i32 };
            let score = vertex_score(cache_position, adj.counts[index]);
            let score_diff = score - vertex_scores[index];
            vertex_scores[index] = score;
            let begin = adj.offsets[index] as usize;
            let end = begin + adj.counts[index] as usize;
            for &tri in &adj.data[begin..end] {
                let tri_score = triangle_scores[tri as usize] + score_diff;
                if best_score < tri_score {
                    best_triangle = tri;
                    best_score = tri_score;
                }
                triangle_scores[tri as usize] = tri_score;
            }
        }
        current = best_triangle;
        if current == u32::MAX {
            // A dead end: the next unemitted triangle in input order.
            while input_cursor < face_count && emitted[input_cursor] {
                input_cursor += 1;
            }
            current = if input_cursor < face_count { input_cursor as u32 } else { u32::MAX };
        }
    }
    debug_assert_eq!(output_triangle, face_count);
    out
}

/// What a FIFO cache of `cache_size` entries does with this order: the vertices
/// transformed, per triangle (ACMR) and per distinct vertex used (ATVR, 1.0 is the floor).
pub fn analyze_vertex_cache(indices: &[u32], vertex_count: usize, cache_size: u32) -> (u32, f32, f32) {
    let mut stamps = vec![0u32; vertex_count];
    let mut timestamp = cache_size + 1;
    let mut transformed = 0u32;
    for &i in indices {
        let v = i as usize;
        if timestamp - stamps[v] > cache_size {
            stamps[v] = timestamp;
            timestamp += 1;
            transformed += 1;
        }
    }
    let unique = stamps.iter().filter(|&&s| s > 0).count();
    let acmr = if indices.is_empty() { 0.0 } else { transformed as f32 / (indices.len() / 3) as f32 };
    let atvr = if unique == 0 { 0.0 } else { transformed as f32 / unique as f32 };
    (transformed, acmr, atvr)
}

fn update_cache(a: u32, b: u32, c: u32, cache_size: u32, stamps: &mut [u32], timestamp: &mut u32) -> u32 {
    let mut misses = 0;
    for v in [a, b, c] {
        if *timestamp - stamps[v as usize] > cache_size {
            stamps[v as usize] = *timestamp;
            *timestamp += 1;
            misses += 1;
        }
    }
    misses
}

fn quantize_unorm(v: f32, bits: i32) -> i32 {
    let scale = ((1 << bits) - 1) as f32;
    let v = v.clamp(0.0, 1.0);
    (v * scale + 0.5) as i32
}

/// The cache-ordered triangles regrouped so the clusters likely in front are drawn
/// first. `positions` is three floats a vertex. `threshold` is how far from the cache
/// optimum a cluster may stray for it (meshoptimizer's recommended 1.05: five percent).
pub fn optimize_overdraw(indices: &[u32], positions: &[f32], vertex_count: usize, threshold: f32) -> Vec<u32> {
    debug_assert!(indices.len() % 3 == 0);
    let face_count = indices.len() / 3;
    if face_count == 0 || vertex_count == 0 {
        return indices.to_vec();
    }
    let cache_size = 16u32;
    let mut stamps = vec![0u32; vertex_count];
    // Hard boundaries: where a triangle misses on all three vertices, a new patch begins.
    let mut hard: Vec<u32> = Vec::new();
    {
        let mut timestamp = cache_size + 1;
        for f in 0..face_count {
            let m = update_cache(indices[f * 3], indices[f * 3 + 1], indices[f * 3 + 2], cache_size, &mut stamps, &mut timestamp);
            if f == 0 || m == 3 {
                hard.push(f as u32);
            }
        }
    }
    // Soft boundaries: each hard cluster cut again wherever the running cache rate has
    // reached the cluster's own rate times the threshold.
    let mut soft: Vec<u32> = Vec::new();
    {
        for s in stamps.iter_mut() {
            *s = 0;
        }
        let mut timestamp = 0u32;
        for it in 0..hard.len() {
            let start = hard[it] as usize;
            let end = if it + 1 < hard.len() { hard[it + 1] as usize } else { face_count };
            timestamp += cache_size + 1;
            let mut cluster_misses = 0u32;
            for f in start..end {
                cluster_misses += update_cache(indices[f * 3], indices[f * 3 + 1], indices[f * 3 + 2], cache_size, &mut stamps, &mut timestamp);
            }
            let cluster_threshold = threshold * (cluster_misses as f32 / (end - start) as f32);
            soft.push(start as u32);
            timestamp += cache_size + 1;
            let mut running_misses = 0u32;
            let mut running_faces = 0u32;
            for f in start..end {
                running_misses += update_cache(indices[f * 3], indices[f * 3 + 1], indices[f * 3 + 2], cache_size, &mut stamps, &mut timestamp);
                running_faces += 1;
                if running_misses as f32 / running_faces as f32 <= cluster_threshold {
                    soft.push((f + 1) as u32);
                    timestamp += cache_size + 1;
                    running_misses = 0;
                    running_faces = 0;
                }
            }
            // The last cut merges back into the cluster before it, which may be `end`.
            if *soft.last().unwrap() != start as u32 {
                soft.pop();
            }
        }
    }
    let clusters = soft;
    let cluster_count = clusters.len();
    // Sort data: each cluster's area-weighted centroid against the mesh centroid, along
    // the cluster's summed normal.
    let mut mesh_centroid = [0f32; 3];
    for v in 0..vertex_count {
        for k in 0..3 {
            mesh_centroid[k] += positions[v * 3 + k];
        }
    }
    for c in mesh_centroid.iter_mut() {
        *c /= vertex_count as f32;
    }
    let mut sort_data = vec![0f32; cluster_count];
    for (ci, &cstart) in clusters.iter().enumerate() {
        let begin = cstart as usize * 3;
        let end = if ci + 1 < cluster_count { clusters[ci + 1] as usize * 3 } else { indices.len() };
        let mut area_sum = 0f32;
        let mut centroid = [0f32; 3];
        let mut normal = [0f32; 3];
        let mut i = begin;
        while i < end {
            let p0 = &positions[indices[i] as usize * 3..][..3];
            let p1 = &positions[indices[i + 1] as usize * 3..][..3];
            let p2 = &positions[indices[i + 2] as usize * 3..][..3];
            let p10 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let p20 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let nx = p10[1] * p20[2] - p10[2] * p20[1];
            let ny = p10[2] * p20[0] - p10[0] * p20[2];
            let nz = p10[0] * p20[1] - p10[1] * p20[0];
            let area = (nx * nx + ny * ny + nz * nz).sqrt();
            for k in 0..3 {
                centroid[k] += (p0[k] + p1[k] + p2[k]) * (area / 3.0);
            }
            normal[0] += nx;
            normal[1] += ny;
            normal[2] += nz;
            area_sum += area;
            i += 3;
        }
        let inv_area = if area_sum == 0.0 { 0.0 } else { 1.0 / area_sum };
        for c in centroid.iter_mut() {
            *c *= inv_area;
        }
        let nl = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        let inv_nl = if nl == 0.0 { 0.0 } else { 1.0 / nl };
        for n in normal.iter_mut() {
            *n *= inv_nl;
        }
        let cv = [centroid[0] - mesh_centroid[0], centroid[1] - mesh_centroid[1], centroid[2] - mesh_centroid[2]];
        sort_data[ci] = cv[0] * normal[0] + cv[1] * normal[1] + cv[2] * normal[2];
    }
    // A counting sort on an 11-bit key, high dot products first.
    let mut sort_max = 1e-3f32;
    for &d in &sort_data {
        let a = d.abs();
        if sort_max < a {
            sort_max = a;
        }
    }
    const SORT_BITS: i32 = 11;
    let keys: Vec<usize> = sort_data
        .iter()
        .map(|&d| (quantize_unorm(0.5 - 0.5 * (d / sort_max), SORT_BITS) & ((1 << SORT_BITS) - 1)) as usize)
        .collect();
    let mut histogram = vec![0usize; 1 << SORT_BITS];
    for &k in &keys {
        histogram[k] += 1;
    }
    let mut sum = 0usize;
    for h in histogram.iter_mut() {
        let count = *h;
        *h = sum;
        sum += count;
    }
    let mut order = vec![0u32; cluster_count];
    for (i, &k) in keys.iter().enumerate() {
        order[histogram[k]] = i as u32;
        histogram[k] += 1;
    }
    let mut out = Vec::with_capacity(indices.len());
    for &ci in &order {
        let ci = ci as usize;
        let begin = clusters[ci] as usize * 3;
        let end = if ci + 1 < cluster_count { clusters[ci + 1] as usize * 3 } else { indices.len() };
        out.extend_from_slice(&indices[begin..end]);
    }
    debug_assert_eq!(out.len(), indices.len());
    out
}

/// [`reorder_triangles`], kept only when the part's vertex cache does not come out worse
/// (the passes are heuristics: on a part the file already has in good order they can give
/// back a little). Returns whether the new order was kept. The cell load uses this; the
/// overdraw and vertex fetch figures are measured where the mesh is rewritten
/// (`conflictcore::nif_order`), not here.
pub fn reorder_triangles_if_not_worse(idx: &mut [u16], positions: &[f32]) -> bool {
    let vertex_count = positions.len() / 3;
    if idx.len() < 3 || idx.len() % 3 != 0 || vertex_count == 0 {
        return false;
    }
    let wide = |t: &[u16]| -> Vec<u32> { t.iter().map(|&i| i as u32).collect() };
    if wide(idx).iter().any(|&i| i as usize >= vertex_count) {
        return false;
    }
    let before = analyze_vertex_cache(&wide(idx), vertex_count, 16).1;
    let mut trial = idx.to_vec();
    reorder_triangles(&mut trial, positions);
    let after = analyze_vertex_cache(&wide(&trial), vertex_count, 16).1;
    if after <= before + 1e-6 {
        idx.copy_from_slice(&trial);
        true
    } else {
        false
    }
}

/// Both passes over a part's 16-bit index list, in place. The vertices are untouched.
pub fn reorder_triangles(idx: &mut [u16], positions: &[f32]) {
    if idx.len() < 3 || idx.len() % 3 != 0 {
        return;
    }
    let vertex_count = positions.len() / 3;
    if vertex_count == 0 {
        return;
    }
    let wide: Vec<u32> = idx.iter().map(|&i| i as u32).collect();
    if wide.iter().any(|&i| i as usize >= vertex_count) {
        return;
    }
    let cached = optimize_vertex_cache(&wide, vertex_count);
    let ordered = optimize_overdraw(&cached, positions, vertex_count, 1.05);
    for (d, s) in idx.iter_mut().zip(ordered) {
        *d = s as u16;
    }
}
