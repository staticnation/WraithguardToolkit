//! The simplifier: fewer triangles for the same shape. From meshoptimizer 1.3's
//! `simplifier.cpp`, line for line (the TRACE statistics left out), so its answers are
//! the C++'s bit for bit (`tests/oracle.rs`). After Garland and Heckbert's quadric error
//! metrics (1997), Garland (1999), Lindstrom's out-of-core clustering (2000), Teschner et
//! al.'s spatial hashing (2003), Van Sandt et al.'s interpolation search (2019) and
//! Hoppe's attribute quadrics (1999, with Marschner 2000).
//!
//! The entry points are the C++'s: [`simplify`], [`simplify_with_attributes`],
//! [`simplify_with_update`], [`simplify_sloppy`], [`simplify_prune`],
//! [`simplify_points`] and [`simplify_scale`]. Strides are in floats, not bytes; a
//! `None` is the C++'s null pointer.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

/// `meshopt_SimplifyLockBorder`: border vertices do not move.
pub const LOCK_BORDER: u32 = 1 << 0;
/// `meshopt_SimplifySparse`: the index list uses a small part of the vertices.
pub const SPARSE: u32 = 1 << 1;
/// `meshopt_SimplifyErrorAbsolute`: errors in mesh units, not relative to its extent.
pub const ERROR_ABSOLUTE: u32 = 1 << 2;
/// `meshopt_SimplifyPrune`: small disconnected pieces may be removed whole.
pub const PRUNE: u32 = 1 << 3;
/// `meshopt_SimplifyRegularize`: more even triangles, at some cost in error.
pub const REGULARIZE: u32 = 1 << 4;
/// `meshopt_SimplifyPermissive`: collapses across attribute seams allowed.
pub const PERMISSIVE: u32 = 1 << 5;
/// `meshopt_SimplifyRegularizeLight`: a lighter `REGULARIZE`.
pub const REGULARIZE_LIGHT: u32 = 1 << 6;
/// `meshopt_SimplifyPreserveFolds`: creases kept.
pub const PRESERVE_FOLDS: u32 = 1 << 7;
/// `meshopt_SimplifyErrorClamped`: attribute error limited to the area it covers.
pub const ERROR_CLAMPED: u32 = 1 << 8;

/// `meshopt_SimplifyVertex_Lock`: this vertex does not move.
pub const VERTEX_LOCK: u8 = 1 << 0;
/// `meshopt_SimplifyVertex_Protect`: this vertex's attribute discontinuity is kept.
pub const VERTEX_PROTECT: u8 = 1 << 1;
/// `meshopt_SimplifyVertex_Priority`: this vertex's error weighs more.
pub const VERTEX_PRIORITY: u8 = 1 << 2;

const NONE: u32 = u32::MAX;

/* ---- edge adjacency --------------------------------------------------------------- */

#[derive(Clone, Copy, Default)]
struct Edge {
    next: u32,
    prev: u32,
}

struct EdgeAdjacency {
    offsets: Vec<u32>,
    data: Vec<Edge>,
}

impl EdgeAdjacency {
    fn new(index_count: usize, vertex_count: usize) -> Self {
        EdgeAdjacency { offsets: vec![0; vertex_count + 1], data: vec![Edge::default(); index_count] }
    }

    fn edges(&self, v: u32) -> &[Edge] {
        let (a, b) = (self.offsets[v as usize] as usize, self.offsets[v as usize + 1] as usize);
        &self.data[a..b]
    }
}

fn update_edge_adjacency(adjacency: &mut EdgeAdjacency, indices: &[u32], vertex_count: usize, remap: Option<&[u32]>) {
    let index_count = indices.len();
    let face_count = index_count / 3;
    // `offsets + 1` in the C++
    let offsets = &mut adjacency.offsets[1..];
    let data = &mut adjacency.data;

    // fill edge counts
    for o in offsets[..vertex_count].iter_mut() {
        *o = 0;
    }

    for &ix in indices {
        let v = match remap {
            Some(r) => r[ix as usize],
            None => ix,
        };
        debug_assert!((v as usize) < vertex_count);

        offsets[v as usize] += 1;
    }

    // fill offset table
    let mut offset: u32 = 0;

    for i in 0..vertex_count {
        let count = offsets[i];
        offsets[i] = offset;
        offset += count;
    }

    debug_assert!(offset as usize == index_count);

    // fill edge data
    for i in 0..face_count {
        let (mut a, mut b, mut c) = (indices[i * 3], indices[i * 3 + 1], indices[i * 3 + 2]);

        if let Some(r) = remap {
            a = r[a as usize];
            b = r[b as usize];
            c = r[c as usize];
        }

        data[offsets[a as usize] as usize] = Edge { next: b, prev: c };
        offsets[a as usize] += 1;

        data[offsets[b as usize] as usize] = Edge { next: c, prev: a };
        offsets[b as usize] += 1;

        data[offsets[c as usize] as usize] = Edge { next: a, prev: b };
        offsets[c as usize] += 1;
    }

    // finalize offsets
    adjacency.offsets[0] = 0;
    debug_assert!(adjacency.offsets[vertex_count] as usize == index_count);
}

/* ---- hashing ---------------------------------------------------------------------- */

trait Hasher {
    fn hash(&self, key: u32) -> usize;
    fn equal(&self, lhs: u32, rhs: u32) -> bool;
}

struct PositionHasher<'a> {
    vertex_positions: &'a [f32],
    vertex_stride_float: usize,
    sparse_remap: Option<&'a [u32]>,
}

impl Hasher for PositionHasher<'_> {
    fn hash(&self, index: u32) -> usize {
        let ri = match self.sparse_remap {
            Some(s) => s[index as usize],
            None => index,
        } as usize;
        let key = &self.vertex_positions[ri * self.vertex_stride_float..];

        let (mut x, mut y, mut z) = (key[0].to_bits(), key[1].to_bits(), key[2].to_bits());

        // replace negative zero with zero
        x = if x == 0x8000_0000 { 0 } else { x };
        y = if y == 0x8000_0000 { 0 } else { y };
        z = if z == 0x8000_0000 { 0 } else { z };

        // scramble bits to make sure that integer coordinates have entropy in lower bits
        x ^= x >> 17;
        y ^= y >> 17;
        z ^= z >> 17;

        // Optimized Spatial Hashing for Collision Detection of Deformable Objects
        (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663) ^ z.wrapping_mul(83492791)) as usize
    }

    fn equal(&self, lhs: u32, rhs: u32) -> bool {
        let li = match self.sparse_remap {
            Some(s) => s[lhs as usize],
            None => lhs,
        } as usize;
        let ri = match self.sparse_remap {
            Some(s) => s[rhs as usize],
            None => rhs,
        } as usize;

        let lv = &self.vertex_positions[li * self.vertex_stride_float..];
        let rv = &self.vertex_positions[ri * self.vertex_stride_float..];

        lv[0] == rv[0] && lv[1] == rv[1] && lv[2] == rv[2]
    }
}

struct RemapHasher<'a> {
    remap: &'a [u32],
}

impl Hasher for RemapHasher<'_> {
    fn hash(&self, id: u32) -> usize {
        id.wrapping_mul(0x5bd1e995) as usize
    }

    fn equal(&self, lhs: u32, rhs: u32) -> bool {
        self.remap[lhs as usize] == rhs
    }
}

fn hash_buckets2(count: usize) -> usize {
    let mut buckets = 1;
    while buckets < count + count / 4 {
        buckets *= 2;
    }

    buckets
}

/// The slot `key` is in, or the empty one it would go in (quadratic probing).
fn hash_lookup2<H: Hasher>(table: &[u32], buckets: usize, hash: &H, key: u32, empty: u32) -> usize {
    debug_assert!(buckets > 0);
    debug_assert!(buckets & (buckets - 1) == 0);

    let hashmod = buckets - 1;
    let mut bucket = hash.hash(key) & hashmod;

    for probe in 0..=hashmod {
        let item = table[bucket];

        if item == empty {
            return bucket;
        }

        if hash.equal(item, key) {
            return bucket;
        }

        // hash collision, quadratic probing
        bucket = (bucket + probe + 1) & hashmod;
    }

    unreachable!("Hash table is full")
}

/// The position remap (each vertex to the first with its position), and the wedge loops
/// when asked for (`wedge`).
fn build_position_remap(remap: &mut [u32], wedge: Option<&mut [u32]>, vertex_positions_data: &[f32], vertex_count: usize, vertex_stride_float: usize, sparse_remap: Option<&[u32]>) {
    let hasher = PositionHasher { vertex_positions: vertex_positions_data, vertex_stride_float, sparse_remap };

    let table_size = hash_buckets2(vertex_count);
    let mut table = vec![NONE; table_size];

    // build forward remap: for each vertex, which other (canonical) vertex does it map to?
    // we use position equivalence for this, and remap vertices to other existing vertices
    for i in 0..vertex_count {
        let index = i as u32;
        let entry = hash_lookup2(&table, table_size, &hasher, index, NONE);

        if table[entry] == NONE {
            table[entry] = index;
        }

        remap[index as usize] = table[entry];
    }

    let Some(wedge) = wedge else { return };

    // build wedge table: for each vertex, which other vertex is the next wedge that also maps to the same vertex?
    // entries in table form a (cyclic) wedge loop per vertex; for manifold vertices, wedge[i] == remap[i] == i
    for i in 0..vertex_count {
        wedge[i] = i as u32;
    }

    for i in 0..vertex_count {
        if remap[i] != i as u32 {
            let r = remap[i] as usize;

            wedge[i] = wedge[r];
            wedge[r] = i as u32;
        }
    }
}

/// Renumbers `indices` to the vertices they use, densely; returns the map back.
fn build_sparse_remap(indices: &mut [u32], vertex_count: usize) -> Vec<u32> {
    // use a bit set to compute the precise number of unique vertices
    let mut filter = vec![0u8; vertex_count.div_ceil(8)];

    let mut unique = 0usize;
    for &index in indices.iter() {
        debug_assert!((index as usize) < vertex_count);
        let (byte, bit) = (index as usize / 8, 1u8 << (index % 8));
        unique += ((filter[byte] & bit) == 0) as usize;
        filter[byte] |= bit;
    }

    let mut remap = vec![0u32; unique];
    let mut offset = 0usize;

    // temporary map dense => sparse
    let revremap_size = hash_buckets2(unique);
    let mut revremap = vec![NONE; revremap_size];

    // fill remap, using revremap as a helper, and rewrite indices in the same pass
    for i in 0..indices.len() {
        let index = indices[i];
        let entry = hash_lookup2(&revremap, revremap_size, &RemapHasher { remap: &remap }, index, NONE);

        if revremap[entry] == NONE {
            remap[offset] = index;
            revremap[entry] = offset as u32;
            offset += 1;
        }

        indices[i] = revremap[entry];
    }

    debug_assert!(offset == unique);

    remap
}

/* ---- vertex classification -------------------------------------------------------- */

// not on an attribute seam, not on any boundary
const KIND_MANIFOLD: u8 = 0;
// not on an attribute seam, has exactly two open edges
const KIND_BORDER: u8 = 1;
// on an attribute seam with exactly two attribute seam edges
const KIND_SEAM: u8 = 2;
// none of the above; these vertices can move as long as all wedges move to the target vertex
const KIND_COMPLEX: u8 = 3;
// complex vertex on a geometric border; can only move to another fringe vertex
const KIND_FRINGE: u8 = 4;
// none of the above; these vertices can't move
const KIND_LOCKED: u8 = 5;
const KIND_COUNT: usize = 6;

// manifold vertices can collapse onto anything
// border/seam vertices can collapse onto border/seam respectively, or locked
// complex vertices can collapse onto complex/locked
// fringe vertices can only collapse onto fringe vertices; border, manifold and complex vertices can collapse onto them
// a rule of thumb is that collapsing kind A into kind B preserves the kind B in the target vertex
// for example, while we could collapse Complex into Manifold, this would mean the target vertex isn't Manifold anymore
const CAN_COLLAPSE: [[u8; KIND_COUNT]; KIND_COUNT] = [
    [1, 1, 1, 1, 1, 1],
    [0, 1, 0, 0, 1, 1],
    [0, 0, 1, 0, 0, 1],
    [0, 0, 0, 1, 1, 1],
    [0, 0, 0, 0, 1, 0],
    [0, 0, 0, 0, 0, 0],
];

// if a vertex is manifold or seam, adjoining edges are guaranteed to have an opposite edge
// note that for seam edges, the opposite edge isn't present in the attribute-based topology
// but is present if you consider a position-only mesh variant
// while many complex collapses have the opposite edge, since complex vertices collapse to the
// same wedge, keeping opposite edges separate improves the quality by considering both targets
const HAS_OPPOSITE: [[u8; KIND_COUNT]; KIND_COUNT] = [
    [1, 1, 1, 1, 1, 1],
    [1, 0, 1, 0, 0, 0],
    [1, 1, 1, 0, 0, 1],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 1, 0, 0, 0],
];

fn has_edge(adjacency: &EdgeAdjacency, a: u32, b: u32) -> bool {
    adjacency.edges(a).iter().any(|e| e.next == b)
}

fn has_edge_wedge(adjacency: &EdgeAdjacency, a: u32, b: u32, remap: &[u32], wedge: &[u32]) -> bool {
    let mut v = a;

    loop {
        for e in adjacency.edges(v) {
            if remap[e.next as usize] == remap[b as usize] {
                return true;
            }
        }

        v = wedge[v as usize];
        if v == a {
            break;
        }
    }

    false
}

fn classify_vertices(result: &mut [u8], loop_: &mut [u32], loopback: &mut [u32], vertex_count: usize, adjacency: &EdgeAdjacency, remap: &[u32], wedge: &[u32], vertex_lock: Option<&[u8]>, sparse_remap: Option<&[u32]>, options: u32) {
    for v in loop_[..vertex_count].iter_mut() {
        *v = NONE;
    }
    for v in loopback[..vertex_count].iter_mut() {
        *v = NONE;
    }

    // incoming & outgoing open edges: ~0u if no open edges, i if there are more than 1
    // note that this is the same data as required in loop[] arrays; loop[] data is only used for border/seam by default
    // in permissive mode we also use it to guide complex-complex collapses, so we fill it for all vertices
    let openinc = loopback;
    let openout = loop_;

    for i in 0..vertex_count {
        let vertex = i as u32;

        for e in adjacency.edges(vertex) {
            let target = e.next;

            if target == vertex {
                // degenerate triangles have two distinct edges instead of three, and the self edge
                // is bi-directional by definition; this can break border/seam classification by "closing"
                // the open edge from another triangle and falsely marking the vertex as manifold
                // instead we mark the vertex as having >1 open edges which turns it into locked/complex
                openinc[vertex as usize] = vertex;
                openout[vertex as usize] = vertex;
            } else if !has_edge(adjacency, target, vertex) {
                openinc[target as usize] = if openinc[target as usize] == NONE { vertex } else { target };
                openout[vertex as usize] = if openout[vertex as usize] == NONE { target } else { vertex };
            }
        }
    }

    for i in 0..vertex_count {
        let iu = i as u32;
        if remap[i] == iu {
            if wedge[i] == iu {
                // no attribute seam, need to check if it's manifold
                let (openi, openo) = (openinc[i], openout[i]);

                // note: we classify any vertices with no open edges as manifold
                // this is technically incorrect - if 4 triangles share an edge, we'll classify vertices as manifold
                // it's unclear if this is a problem in practice
                if openi == NONE && openo == NONE {
                    result[i] = KIND_MANIFOLD;
                } else if openi != NONE && openo != NONE && remap[openi as usize] == remap[openo as usize] && openi != iu {
                    // classify half-seams as seams (the branch below would mis-classify them as borders)
                    // half-seam is a single vertex that connects to both vertices of a potential seam
                    // treating these as seams allows collapsing the "full" seam vertex onto them
                    result[i] = KIND_SEAM;
                } else if openi != iu && openo != iu {
                    result[i] = KIND_BORDER;
                } else {
                    result[i] = KIND_LOCKED;
                }
            } else if wedge[wedge[i] as usize] == iu {
                // attribute seam; need to distinguish between Seam and Locked
                let w = wedge[i];
                let (openiv, openov) = (openinc[i], openout[i]);
                let (openiw, openow) = (openinc[w as usize], openout[w as usize]);

                // seam should have one open half-edge for each vertex, and the edges need to "connect" - point to the same vertex post-remap
                if openiv != NONE
                    && openiv != iu
                    && openov != NONE
                    && openov != iu
                    && openiw != NONE
                    && openiw != w
                    && openow != NONE
                    && openow != w
                    && remap[openiv as usize] == remap[openow as usize]
                    && remap[openov as usize] == remap[openiw as usize]
                    && remap[openiv as usize] != remap[openov as usize]
                {
                    result[i] = KIND_SEAM;
                } else {
                    // mismatched seam endpoints or disconnected seam; we don't have classification available
                    result[i] = KIND_LOCKED;
                }
            } else {
                // more than one vertex maps to this one; we don't have classification available
                result[i] = KIND_LOCKED;
            }
        } else {
            debug_assert!(remap[i] < iu);

            result[i] = result[remap[i] as usize];
        }
    }

    if options & PERMISSIVE != 0 {
        for i in 0..vertex_count {
            if result[i] == KIND_SEAM || result[i] == KIND_LOCKED {
                if remap[i] != i as u32 {
                    // only process primary vertices; wedges will be updated to match the primary vertex
                    result[i] = result[remap[i] as usize];
                    continue;
                }

                let mut v = i as u32;

                // vertex_lock may protect any wedge, not just the primary vertex, so we switch to complex only if no wedges are protected
                let mut protect = false;
                loop {
                    let rv = match sparse_remap {
                        Some(s) => s[v as usize],
                        None => v,
                    };
                    protect |= vertex_lock.is_some_and(|l| (l[rv as usize] & VERTEX_PROTECT) != 0);
                    v = wedge[v as usize];
                    if v == i as u32 {
                        break;
                    }
                }

                // check if any adjoining edge doesn't have an opposite edge (indicating vertex is on the border)
                let mut border = false;
                loop {
                    for e in adjacency.edges(v) {
                        border |= !has_edge_wedge(adjacency, e.next, v, remap, wedge);
                    }
                    v = wedge[v as usize];
                    if v == i as u32 {
                        break;
                    }
                }

                // border vertices with attribute discontinuities can only collapse along the border, so they need a separate tagging
                result[i] = if protect { result[i] } else if border { KIND_FRINGE } else { KIND_COMPLEX };
            }
        }
    }

    if let Some(vertex_lock) = vertex_lock {
        // vertex_lock may lock any wedge, not just the primary vertex, so we need to lock the primary vertex and relock any wedges
        for i in 0..vertex_count {
            let ri = match sparse_remap {
                Some(s) => s[i],
                None => i as u32,
            };

            if vertex_lock[ri as usize] & VERTEX_LOCK != 0 {
                result[remap[i] as usize] = KIND_LOCKED;
            }
        }

        for i in 0..vertex_count {
            if result[remap[i] as usize] == KIND_LOCKED {
                result[i] = KIND_LOCKED;
            }
        }
    }

    if options & LOCK_BORDER != 0 {
        for i in 0..vertex_count {
            if result[i] == KIND_BORDER || result[i] == KIND_FRINGE {
                result[i] = KIND_LOCKED;
            }
        }
    }
}

/* ---- positions and attributes ----------------------------------------------------- */

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Vector3 {
    x: f32,
    y: f32,
    z: f32,
}

/// Positions moved into the unit cube (when `result`), and the extent they had; the
/// minimum corner into `out_offset`.
fn rescale_positions(result: Option<&mut [Vector3]>, vertex_positions_data: &[f32], vertex_count: usize, vertex_stride_float: usize, sparse_remap: Option<&[u32]>, out_offset: Option<&mut [f32; 3]>) -> f32 {
    let mut minv = [f32::MAX; 3];
    let mut maxv = [-f32::MAX; 3];

    let mut result = result;
    for i in 0..vertex_count {
        let ri = match sparse_remap {
            Some(s) => s[i],
            None => i as u32,
        } as usize;
        let v = &vertex_positions_data[ri * vertex_stride_float..];

        if let Some(r) = result.as_deref_mut() {
            r[i] = Vector3 { x: v[0], y: v[1], z: v[2] };
        }

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

    if let Some(r) = result {
        let scale = if extent == 0.0 { 0.0 } else { 1.0 / extent };

        for i in 0..vertex_count {
            r[i].x = (r[i].x - minv[0]) * scale;
            r[i].y = (r[i].y - minv[1]) * scale;
            r[i].z = (r[i].z - minv[2]) * scale;
        }
    }

    if let Some(o) = out_offset {
        *o = minv;
    }

    extent
}

fn rescale_attributes(result: &mut [f32], vertex_attributes_data: &[f32], vertex_count: usize, vertex_attributes_stride_float: usize, attribute_weights: &[f32], attribute_count: usize, attribute_remap: &[u32], sparse_remap: Option<&[u32]>) {
    for i in 0..vertex_count {
        let ri = match sparse_remap {
            Some(s) => s[i],
            None => i as u32,
        } as usize;

        for k in 0..attribute_count {
            let rk = attribute_remap[k] as usize;
            let a = vertex_attributes_data[ri * vertex_attributes_stride_float + rk];

            result[i * attribute_count + k] = a * attribute_weights[rk];
        }
    }
}

fn finalize_vertices(vertex_positions_data: &mut [f32], vertex_positions_stride_float: usize, vertex_attributes_data: Option<&mut [f32]>, vertex_attributes_stride_float: usize, attribute_weights: &[f32], attribute_count: usize, vertex_count: usize, vertex_positions: &[Vector3], vertex_attributes: &[f32], sparse_remap: Option<&[u32]>, attribute_remap: &[u32], vertex_scale: f32, vertex_offset: &[f32; 3], vertex_kind: &[u8], vertex_update: &[u8], vertex_lock: Option<&[u8]>) {
    let mut vertex_attributes_data = vertex_attributes_data;
    for i in 0..vertex_count {
        if vertex_update[i] == 0 {
            continue;
        }

        let ri = match sparse_remap {
            Some(s) => s[i],
            None => i as u32,
        } as usize;

        // updating externally locked vertices is not allowed
        if vertex_lock.is_some_and(|l| (l[ri] & VERTEX_LOCK) != 0) {
            continue;
        }

        // moving locked vertices may result in floating point drift
        if vertex_kind[i] != KIND_LOCKED {
            let p = vertex_positions[i];
            let v = &mut vertex_positions_data[ri * vertex_positions_stride_float..];

            v[0] = p.x * vertex_scale + vertex_offset[0];
            v[1] = p.y * vertex_scale + vertex_offset[1];
            v[2] = p.z * vertex_scale + vertex_offset[2];
        }

        if attribute_count != 0 {
            let sa = &vertex_attributes[i * attribute_count..];
            let va = &mut vertex_attributes_data.as_deref_mut().expect("attributes")[ri * vertex_attributes_stride_float..];

            for k in 0..attribute_count {
                let rk = attribute_remap[k] as usize;

                va[rk] = sa[k] / attribute_weights[rk];
            }
        }
    }
}

/* ---- quadrics --------------------------------------------------------------------- */

const MAX_ATTRIBUTES: usize = 32;

/// a00*x^2 + a11*y^2 + a22*z^2 + 2*a10*xy + 2*a20*xz + 2*a21*yz + 2*b0*x + 2*b1*y + 2*b2*z + c
#[derive(Clone, Copy, Default, Debug)]
struct Quadric {
    a00: f32,
    a11: f32,
    a22: f32,
    a10: f32,
    a20: f32,
    a21: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    c: f32,
    w: f32,
}

/// gx*x + gy*y + gz*z + gw
#[derive(Clone, Copy, Default, Debug)]
struct QuadricGrad {
    gx: f32,
    gy: f32,
    gz: f32,
    gw: f32,
}

#[derive(Clone, Copy, Default, Debug)]
struct Reservoir {
    x: f32,
    y: f32,
    z: f32,
    r: f32,
    g: f32,
    b: f32,
    w: f32,
}

/// The C++'s `Collapse`, whose third field is a union of `bidi`, `error` and `errorui`:
/// kept as the raw bits, read as each.
#[derive(Clone, Copy, Default, Debug)]
struct Collapse {
    v0: u32,
    v1: u32,
    bits: u32,
}

impl Collapse {
    fn bidi(&self) -> u32 {
        self.bits
    }
    fn error(&self) -> f32 {
        f32::from_bits(self.bits)
    }
    fn set_error(&mut self, e: f32) {
        self.bits = e.to_bits();
    }
    fn errorui(&self) -> u32 {
        self.bits
    }
}

fn normalize(v: &mut Vector3) -> f32 {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();

    if length > 0.0 {
        v.x /= length;
        v.y /= length;
        v.z /= length;
    }

    length
}

fn quadric_add(q: &mut Quadric, r: &Quadric) {
    q.a00 += r.a00;
    q.a11 += r.a11;
    q.a22 += r.a22;
    q.a10 += r.a10;
    q.a20 += r.a20;
    q.a21 += r.a21;
    q.b0 += r.b0;
    q.b1 += r.b1;
    q.b2 += r.b2;
    q.c += r.c;
    q.w += r.w;
}

fn quadric_add_grad(g: &mut QuadricGrad, r: &QuadricGrad) {
    g.gx += r.gx;
    g.gy += r.gy;
    g.gz += r.gz;
    g.gw += r.gw;
}

fn quadric_add_grads(g: &mut [QuadricGrad], r: &[QuadricGrad], attribute_count: usize) {
    for k in 0..attribute_count {
        g[k].gx += r[k].gx;
        g[k].gy += r[k].gy;
        g[k].gz += r[k].gz;
        g[k].gw += r[k].gw;
    }
}

fn quadric_eval(q: &Quadric, v: &Vector3) -> f32 {
    let mut rx = (q.b0 + q.a10 * v.y) * 2.0;
    let mut ry = (q.b1 + q.a21 * v.z) * 2.0;
    let mut rz = (q.b2 + q.a20 * v.x) * 2.0;

    rx += q.a00 * v.x;
    ry += q.a11 * v.y;
    rz += q.a22 * v.z;

    q.c + rx * v.x + ry * v.y + rz * v.z
}

fn quadric_error(q: &Quadric, v: &Vector3) -> f32 {
    let r = quadric_eval(q, v);
    let s = if q.w == 0.0 { 0.0 } else { 1.0 / q.w };

    r.abs() * s
}

fn quadric_error_attr(q: &Quadric, g: &[QuadricGrad], attribute_count: usize, v: &Vector3, va: &[f32], clamped: bool) -> f32 {
    let mut r = quadric_eval(q, v);

    // see quadricFromAttributes for general derivation; here we need to add the parts of (eval(pos) - attr)^2 that depend on attr
    for k in 0..attribute_count {
        let a = va[k];
        let gk = v.x * g[k].gx + v.y * g[k].gy + v.z * g[k].gz + g[k].gw;

        r += a * (a * q.w - 2.0 * gk);
    }

    // note: unlike position error, we do not normalize by Q.w to retain edge scaling as described in quadricFromAttributes
    // we do support clamping, with the error limited to the cumulative weight (= area), which makes perceptual error map to position error more closely
    let e = r.abs();
    if clamped {
        if e < q.w {
            e
        } else {
            q.w
        }
    } else {
        e
    }
}

fn quadric_from_plane(q: &mut Quadric, a: f32, b: f32, c: f32, d: f32, w: f32) {
    let aw = a * w;
    let bw = b * w;
    let cw = c * w;
    let dw = d * w;

    q.a00 = a * aw;
    q.a11 = b * bw;
    q.a22 = c * cw;
    q.a10 = a * bw;
    q.a20 = a * cw;
    q.a21 = b * cw;
    q.b0 = a * dw;
    q.b1 = b * dw;
    q.b2 = c * dw;
    q.c = d * dw;
    q.w = w;
}

fn quadric_from_point(q: &mut Quadric, x: f32, y: f32, z: f32, w: f32) {
    q.a00 = w;
    q.a11 = w;
    q.a22 = w;
    q.a10 = 0.0;
    q.a20 = 0.0;
    q.a21 = 0.0;
    q.b0 = -x * w;
    q.b1 = -y * w;
    q.b2 = -z * w;
    q.c = (x * x + y * y + z * z) * w;
    q.w = w;
}

fn quadric_from_triangle(q: &mut Quadric, p0: &Vector3, p1: &Vector3, p2: &Vector3, weight: f32) {
    let p10 = Vector3 { x: p1.x - p0.x, y: p1.y - p0.y, z: p1.z - p0.z };
    let p20 = Vector3 { x: p2.x - p0.x, y: p2.y - p0.y, z: p2.z - p0.z };

    // normal = cross(p1 - p0, p2 - p0)
    let mut normal = Vector3 { x: p10.y * p20.z - p10.z * p20.y, y: p10.z * p20.x - p10.x * p20.z, z: p10.x * p20.y - p10.y * p20.x };
    let area = normalize(&mut normal);

    let distance = normal.x * p0.x + normal.y * p0.y + normal.z * p0.z;

    // we use sqrtf(area) so that the error is scaled linearly; this tends to improve silhouettes
    quadric_from_plane(q, normal.x, normal.y, normal.z, -distance, area.sqrt() * weight);
}

fn quadric_from_triangle_edge(q: &mut Quadric, p0: &Vector3, p1: &Vector3, p2: &Vector3, weight: f32) {
    let p10 = Vector3 { x: p1.x - p0.x, y: p1.y - p0.y, z: p1.z - p0.z };

    // edge length; keep squared length around for projection correction
    let lengthsq = p10.x * p10.x + p10.y * p10.y + p10.z * p10.z;
    let length = lengthsq.sqrt();

    // p20p = length of projection of p2-p0 onto p1-p0; note that p10 is unnormalized so we need to correct it later
    let p20 = Vector3 { x: p2.x - p0.x, y: p2.y - p0.y, z: p2.z - p0.z };
    let p20p = p20.x * p10.x + p20.y * p10.y + p20.z * p10.z;

    // perp = perpendicular vector from p2 to line segment p1-p0
    // note: since p10 is unnormalized we need to correct the projection; we scale p20 instead to take advantage of normalize below
    let mut perp = Vector3 { x: p20.x * lengthsq - p10.x * p20p, y: p20.y * lengthsq - p10.y * p20p, z: p20.z * lengthsq - p10.z * p20p };
    normalize(&mut perp);

    let distance = perp.x * p0.x + perp.y * p0.y + perp.z * p0.z;

    // note: the weight is scaled linearly with edge length; this has to match the triangle weight
    quadric_from_plane(q, perp.x, perp.y, perp.z, -distance, length * weight);
}

fn quadric_from_attributes(q: &mut Quadric, g: &mut [QuadricGrad], p0: &Vector3, p1: &Vector3, p2: &Vector3, va0: &[f32], va1: &[f32], va2: &[f32], attribute_count: usize) {
    // for each attribute we want to encode the following function into the quadric:
    // (eval(pos) - attr)^2
    // where eval(pos) interpolates attribute across the triangle like so:
    // eval(pos) = pos.x * gx + pos.y * gy + pos.z * gz + gw
    // where gx/gy/gz/gw are gradients
    let p10 = Vector3 { x: p1.x - p0.x, y: p1.y - p0.y, z: p1.z - p0.z };
    let p20 = Vector3 { x: p2.x - p0.x, y: p2.y - p0.y, z: p2.z - p0.z };

    // normal = cross(p1 - p0, p2 - p0)
    let normal = Vector3 { x: p10.y * p20.z - p10.z * p20.y, y: p10.z * p20.x - p10.x * p20.z, z: p10.x * p20.y - p10.y * p20.x };
    let area = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt() * 0.5;

    // quadric is weighted with the square of edge length (= area)
    // this equalizes the units with the positional error (which, after normalization, is a square of distance)
    // as a result, a change in weighted attribute of 1 along distance d is approximately equivalent to a change in position of d
    let w = area;

    // we compute gradients using barycentric coordinates (see the C++ for the derivation)
    let v0 = p10;
    let v1 = p20;
    let d00 = v0.x * v0.x + v0.y * v0.y + v0.z * v0.z;
    let d01 = v0.x * v1.x + v0.y * v1.y + v0.z * v1.z;
    let d11 = v1.x * v1.x + v1.y * v1.y + v1.z * v1.z;
    let denom = d00 * d11 - d01 * d01;
    let denomr = if denom == 0.0 { 0.0 } else { 1.0 / denom };

    // precompute gradient factors
    let gx1 = (d11 * v0.x - d01 * v1.x) * denomr;
    let gx2 = (d00 * v1.x - d01 * v0.x) * denomr;
    let gy1 = (d11 * v0.y - d01 * v1.y) * denomr;
    let gy2 = (d00 * v1.y - d01 * v0.y) * denomr;
    let gz1 = (d11 * v0.z - d01 * v1.z) * denomr;
    let gz2 = (d00 * v1.z - d01 * v0.z) * denomr;

    *q = Quadric::default();

    q.w = w;

    for k in 0..attribute_count {
        let (a0, a1, a2) = (va0[k], va1[k], va2[k]);

        // compute gradient of eval(pos) for x/y/z/w
        let gx = gx1 * (a1 - a0) + gx2 * (a2 - a0);
        let gy = gy1 * (a1 - a0) + gy2 * (a2 - a0);
        let gz = gz1 * (a1 - a0) + gz2 * (a2 - a0);
        let gw = a0 - p0.x * gx - p0.y * gy - p0.z * gz;

        // quadric encodes (eval(pos)-attr)^2 (see the C++)
        q.a00 += w * (gx * gx);
        q.a11 += w * (gy * gy);
        q.a22 += w * (gz * gz);

        q.a10 += w * (gy * gx);
        q.a20 += w * (gz * gx);
        q.a21 += w * (gz * gy);

        q.b0 += w * (gx * gw);
        q.b1 += w * (gy * gw);
        q.b2 += w * (gz * gw);

        q.c += w * (gw * gw);

        // the only remaining sum components are ones that depend on attr; these will be added during error evaluation, see quadricError
        g[k].gx = w * gx;
        g[k].gy = w * gy;
        g[k].gz = w * gz;
        g[k].gw = w * gw;
    }
}

fn quadric_volume_gradient(g: &mut QuadricGrad, p0: &Vector3, p1: &Vector3, p2: &Vector3) {
    let p10 = Vector3 { x: p1.x - p0.x, y: p1.y - p0.y, z: p1.z - p0.z };
    let p20 = Vector3 { x: p2.x - p0.x, y: p2.y - p0.y, z: p2.z - p0.z };

    // normal = cross(p1 - p0, p2 - p0)
    let mut normal = Vector3 { x: p10.y * p20.z - p10.z * p20.y, y: p10.z * p20.x - p10.x * p20.z, z: p10.x * p20.y - p10.y * p20.x };
    let area = normalize(&mut normal) * 0.5;

    g.gx = normal.x * area;
    g.gy = normal.y * area;
    g.gz = normal.z * area;
    g.gw = (-p0.x * normal.x - p0.y * normal.y - p0.z * normal.z) * area;
}

fn quadric_solve(p: &mut Vector3, q: &Quadric, gv: &QuadricGrad) -> bool {
    // solve A*p = -b where A is the quadric matrix and b is the linear term
    let (a00, a11, a22) = (q.a00, q.a11, q.a22);
    let (a10, a20, a21) = (q.a10, q.a20, q.a21);
    let (x0, x1, x2) = (-q.b0, -q.b1, -q.b2);

    let eps = 1e-6f32 * q.w;

    // LDL decomposition: A = LDL^T
    let d0 = a00;
    let l10 = a10 / d0;
    let l20 = a20 / d0;

    let d1 = a11 - a10 * l10;
    let dl21 = a21 - a20 * l10;
    let l21 = dl21 / d1;

    let d2 = a22 - a20 * l20 - dl21 * l21;

    // solve L*y = x
    let y0 = x0;
    let y1 = x1 - l10 * y0;
    let y2 = x2 - l20 * y0 - l21 * y1;

    // solve D*z = y
    let z0 = y0 / d0;
    let z1 = y1 / d1;
    let z2 = y2 / d2;

    // augment system with linear constraint GV using Lagrange multiplier
    let (a30, a31, a32) = (gv.gx, gv.gy, gv.gz);
    let x3 = -gv.gw;

    let l30 = a30 / d0;
    let dl31 = a31 - a30 * l10;
    let l31 = dl31 / d1;
    let dl32 = a32 - a30 * l20 - dl31 * l21;
    let l32 = dl32 / d2;
    let d3 = 0.0 - a30 * l30 - dl31 * l31 - dl32 * l32;

    let y3 = x3 - l30 * y0 - l31 * y1 - l32 * y2;
    let z3 = if d3.abs() > eps { y3 / d3 } else { 0.0 }; // if d3 is zero, we can ignore the constraint

    // substitute L^T*p = z
    let lambda = z3;
    let pz = z2 - l32 * lambda;
    let py = z1 - l21 * pz - l31 * lambda;
    let px = z0 - l10 * py - l20 * pz - l30 * lambda;

    p.x = px;
    p.y = py;
    p.z = pz;

    d0.abs() > eps && d1.abs() > eps && d2.abs() > eps
}

fn quadric_reduce_attributes(q: &mut Quadric, a: &Quadric, g: &[QuadricGrad], attribute_count: usize) {
    // update vertex quadric with attribute quadric; multiply by vertex weight to minimize normalized error
    q.a00 += a.a00 * q.w;
    q.a11 += a.a11 * q.w;
    q.a22 += a.a22 * q.w;
    q.a10 += a.a10 * q.w;
    q.a20 += a.a20 * q.w;
    q.a21 += a.a21 * q.w;
    q.b0 += a.b0 * q.w;
    q.b1 += a.b1 * q.w;
    q.b2 += a.b2 * q.w;

    let iaw = if a.w == 0.0 { 0.0 } else { q.w / a.w };

    // update linear system based on attribute gradients (BB^T/a)
    for k in 0..attribute_count {
        let gk = &g[k];

        q.a00 -= (gk.gx * gk.gx) * iaw;
        q.a11 -= (gk.gy * gk.gy) * iaw;
        q.a22 -= (gk.gz * gk.gz) * iaw;
        q.a10 -= (gk.gx * gk.gy) * iaw;
        q.a20 -= (gk.gx * gk.gz) * iaw;
        q.a21 -= (gk.gy * gk.gz) * iaw;

        q.b0 -= (gk.gx * gk.gw) * iaw;
        q.b1 -= (gk.gy * gk.gw) * iaw;
        q.b2 -= (gk.gz * gk.gw) * iaw;
    }
}

fn fill_face_quadrics(vertex_quadrics: &mut [Quadric], volume_gradients: Option<&mut [QuadricGrad]>, indices: &[u32], vertex_positions: &[Vector3], remap: &[u32]) {
    let mut volume_gradients = volume_gradients;
    let mut i = 0;
    while i < indices.len() {
        let (i0, i1, i2) = (indices[i] as usize, indices[i + 1] as usize, indices[i + 2] as usize);

        let mut q = Quadric::default();
        quadric_from_triangle(&mut q, &vertex_positions[i0], &vertex_positions[i1], &vertex_positions[i2], 1.0);

        quadric_add(&mut vertex_quadrics[remap[i0] as usize], &q);
        quadric_add(&mut vertex_quadrics[remap[i1] as usize], &q);
        quadric_add(&mut vertex_quadrics[remap[i2] as usize], &q);

        if let Some(vg) = volume_gradients.as_deref_mut() {
            let mut gv = QuadricGrad::default();
            quadric_volume_gradient(&mut gv, &vertex_positions[i0], &vertex_positions[i1], &vertex_positions[i2]);

            quadric_add_grad(&mut vg[remap[i0] as usize], &gv);
            quadric_add_grad(&mut vg[remap[i1] as usize], &gv);
            quadric_add_grad(&mut vg[remap[i2] as usize], &gv);
        }
        i += 3;
    }
}

fn fill_vertex_quadrics(vertex_quadrics: &mut [Quadric], vertex_positions: &[Vector3], vertex_count: usize, remap: &[u32], vertex_lock: Option<&[u8]>, sparse_remap: Option<&[u32]>, options: u32) {
    // by default, we use a very small weight to improve triangulation and numerical stability without affecting the shape or error
    let factor = if options & REGULARIZE_LIGHT != 0 {
        1e-2f32
    } else if options & REGULARIZE != 0 {
        1e-1f32
    } else {
        1e-7f32
    };

    for i in 0..vertex_count {
        if remap[i] != i as u32 {
            continue;
        }

        // increase regularization weight for vertices marked as priority; for now we only examine the primary vertex
        let ri = match sparse_remap {
            Some(s) => s[i],
            None => i as u32,
        } as usize;
        let priority = vertex_lock.is_some_and(|l| (l[ri] & VERTEX_PRIORITY) != 0);

        let p = vertex_positions[i];
        let w = vertex_quadrics[i].w * (if priority { 1.0 } else { factor });

        let mut q = Quadric::default();
        quadric_from_point(&mut q, p.x, p.y, p.z, w);

        quadric_add(&mut vertex_quadrics[i], &q);
    }
}

fn fill_edge_quadrics(vertex_quadrics: &mut [Quadric], indices: &[u32], vertex_positions: &[Vector3], remap: &[u32], vertex_kind: &[u8], loop_: &[u32], loopback: &[u32]) {
    const NEXT: [usize; 4] = [1, 2, 0, 1];
    let mut i = 0;
    while i < indices.len() {
        for e in 0..3 {
            let i0 = indices[i + e];
            let i1 = indices[i + NEXT[e]];

            let k0 = vertex_kind[i0 as usize];
            let k1 = vertex_kind[i1 as usize];

            // early out: manifold vertices never lie on a border
            if k0 == KIND_MANIFOLD || k1 == KIND_MANIFOLD {
                continue;
            }

            // one of the vertices must be border/seam/fringe; additional checks help validate the border
            if k0 != KIND_BORDER && k0 != KIND_SEAM && k0 != KIND_FRINGE && k1 != KIND_BORDER && k1 != KIND_SEAM && k1 != KIND_FRINGE {
                continue;
            }

            // if i0/i1 are border/seam we check if they are on the same edge loop
            // note that we need to add the error even for edges that connect e.g. border & locked
            // if we don't do that, the adjacent border->border edge won't have correct errors for corners
            if (k0 == KIND_BORDER || k0 == KIND_SEAM) && loop_[i0 as usize] != i1 {
                continue;
            }

            if (k1 == KIND_BORDER || k1 == KIND_SEAM) && loopback[i1 as usize] != i0 {
                continue;
            }

            // fringe vertices don't have proper edge loops, so we need to add edge quadrics to fringe-fringe or fringe-locked edges
            // note that border-fringe edges are still processed, guarded by the loop check above
            if (k0 == KIND_FRINGE || k1 == KIND_FRINGE) && (k0 == KIND_COMPLEX || k1 == KIND_COMPLEX) {
                continue;
            }

            let i2 = indices[i + NEXT[e + 1]];

            // we try hard to maintain border edge geometry; seam edges can move more freely
            // due to topological restrictions on collapses, seam quadrics slightly improve collapse structure but aren't critical
            const EDGE_WEIGHT_SEAM: f32 = 0.5; // applied twice due to opposite edges
            const EDGE_WEIGHT_BORDER: f32 = 10.0;

            let edge_weight = if k0 == KIND_SEAM || k1 == KIND_SEAM { EDGE_WEIGHT_SEAM } else { EDGE_WEIGHT_BORDER };

            let (p0, p1, p2) = (&vertex_positions[i0 as usize], &vertex_positions[i1 as usize], &vertex_positions[i2 as usize]);
            let mut q = Quadric::default();
            quadric_from_triangle_edge(&mut q, p0, p1, p2, edge_weight);

            let mut qt = Quadric::default();
            quadric_from_triangle(&mut qt, p0, p1, p2, edge_weight);

            // mix edge quadric with triangle quadric to stabilize collapses in both directions; both quadrics inherit edge weight so that their error is added
            qt.w = 0.0;
            quadric_add(&mut q, &qt);

            quadric_add(&mut vertex_quadrics[remap[i0 as usize] as usize], &q);
            quadric_add(&mut vertex_quadrics[remap[i1 as usize] as usize], &q);
        }
        i += 3;
    }
}

fn opposite_vertex(adjacency: &EdgeAdjacency, a: u32, b: u32, remap: &[u32], wedge: &[u32]) -> u32 {
    let mut v = a;
    let mut r = NONE;

    loop {
        for e in adjacency.edges(v) {
            if remap[e.next as usize] == remap[b as usize] {
                r = if r == NONE { e.prev } else { a };
            }
        }

        v = wedge[v as usize];
        if v == a {
            break;
        }
    }

    // we only return the opposite vertex if it's unique
    if r == a {
        NONE
    } else {
        r
    }
}

fn sub(a: &Vector3, b: &Vector3) -> Vector3 {
    Vector3 { x: a.x - b.x, y: a.y - b.y, z: a.z - b.z }
}

fn fill_fold_quadrics(vertex_quadrics: &mut [Quadric], adjacency: &EdgeAdjacency, vertex_positions: &[Vector3], vertex_count: usize, remap: &[u32], wedge: &[u32]) {
    for i0 in 0..vertex_count {
        for i in adjacency.offsets[i0]..adjacency.offsets[i0 + 1] {
            let i1 = adjacency.data[i as usize].next;
            let i2 = adjacency.data[i as usize].prev;

            // since we only process paired edges each edge should occur twice; skip redundant edges
            if remap[i1 as usize] > remap[i0] {
                continue;
            }

            let i3 = opposite_vertex(adjacency, i1, i0 as u32, remap, wedge);

            if i3 == NONE {
                continue;
            }

            let v0 = &vertex_positions[i0];
            let p10 = sub(&vertex_positions[i1 as usize], v0);
            let p20 = sub(&vertex_positions[i2 as usize], v0);
            let p30 = sub(&vertex_positions[i3 as usize], v0);

            let normal = Vector3 { x: p10.y * p20.z - p10.z * p20.y, y: p10.z * p20.x - p10.x * p20.z, z: p10.x * p20.y - p10.y * p20.x };
            let opposite = Vector3 { x: p30.y * p10.z - p30.z * p10.y, y: p30.z * p10.x - p30.x * p10.z, z: p30.x * p10.y - p30.y * p10.x };
            let nl = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
            let ol = (opposite.x * opposite.x + opposite.y * opposite.y + opposite.z * opposite.z).sqrt();

            // identify folds by checking dihedral angle against a somewhat arbitrary ~30 degree cutoff
            if normal.x * opposite.x + normal.y * opposite.y + normal.z * opposite.z >= -0.85f32 * nl * ol {
                continue;
            }

            // use both triangles for symmetry since we won't visit this edge again
            let (mut q, mut qo) = (Quadric::default(), Quadric::default());
            quadric_from_triangle_edge(&mut q, v0, &vertex_positions[i1 as usize], &vertex_positions[i2 as usize], 1.0);
            quadric_from_triangle_edge(&mut qo, v0, &vertex_positions[i1 as usize], &vertex_positions[i3 as usize], 1.0);
            quadric_add(&mut q, &qo);

            quadric_add(&mut vertex_quadrics[remap[i0] as usize], &q);
            quadric_add(&mut vertex_quadrics[remap[i1 as usize] as usize], &q);
        }
    }
}

fn fill_attribute_quadrics(attribute_quadrics: &mut [Quadric], attribute_gradients: &mut [QuadricGrad], indices: &[u32], vertex_positions: &[Vector3], vertex_attributes: &[f32], attribute_count: usize) {
    let mut i = 0;
    while i < indices.len() {
        let (i0, i1, i2) = (indices[i] as usize, indices[i + 1] as usize, indices[i + 2] as usize);

        let mut qa = Quadric::default();
        let mut g = [QuadricGrad::default(); MAX_ATTRIBUTES];
        quadric_from_attributes(
            &mut qa,
            &mut g,
            &vertex_positions[i0],
            &vertex_positions[i1],
            &vertex_positions[i2],
            &vertex_attributes[i0 * attribute_count..],
            &vertex_attributes[i1 * attribute_count..],
            &vertex_attributes[i2 * attribute_count..],
            attribute_count,
        );

        quadric_add(&mut attribute_quadrics[i0], &qa);
        quadric_add(&mut attribute_quadrics[i1], &qa);
        quadric_add(&mut attribute_quadrics[i2], &qa);

        quadric_add_grads(&mut attribute_gradients[i0 * attribute_count..], &g, attribute_count);
        quadric_add_grads(&mut attribute_gradients[i1 * attribute_count..], &g, attribute_count);
        quadric_add_grads(&mut attribute_gradients[i2 * attribute_count..], &g, attribute_count);
        i += 3;
    }
}

// does triangle ABC flip when C is replaced with D?
fn has_triangle_flip(a: &Vector3, b: &Vector3, c: &Vector3, d: &Vector3) -> bool {
    let eb = sub(b, a);
    let ec = sub(c, a);
    let ed = sub(d, a);

    let nbc = Vector3 { x: eb.y * ec.z - eb.z * ec.y, y: eb.z * ec.x - eb.x * ec.z, z: eb.x * ec.y - eb.y * ec.x };
    let nbd = Vector3 { x: eb.y * ed.z - eb.z * ed.y, y: eb.z * ed.x - eb.x * ed.z, z: eb.x * ed.y - eb.y * ed.x };

    let ndp = nbc.x * nbd.x + nbc.y * nbd.y + nbc.z * nbd.z;
    let abc = nbc.x * nbc.x + nbc.y * nbc.y + nbc.z * nbc.z;
    let abd = nbd.x * nbd.x + nbd.y * nbd.y + nbd.z * nbd.z;

    // scale is cos(angle); somewhat arbitrarily set to ~75 degrees
    // note that the "pure" check is ndp <= 0 (90 degree cutoff) but that allows flipping through a series of close-to-90 collapses
    ndp <= 0.25f32 * (abc * abd).sqrt()
}

fn has_triangle_flips(adjacency: &EdgeAdjacency, vertex_positions: &[Vector3], collapse_remap: &[u32], i0: u32, i1: u32) -> bool {
    debug_assert!(collapse_remap[i0 as usize] == i0);
    debug_assert!(collapse_remap[i1 as usize] == i1);

    let v0 = &vertex_positions[i0 as usize];
    let v1 = &vertex_positions[i1 as usize];

    for e in adjacency.edges(i0) {
        let a = collapse_remap[e.next as usize];
        let b = collapse_remap[e.prev as usize];

        // skip triangles that will get collapsed by i0->i1 collapse or already got collapsed previously
        if a == i1 || b == i1 || a == b {
            continue;
        }

        // early-out when at least one triangle flips due to a collapse
        if has_triangle_flip(&vertex_positions[a as usize], &vertex_positions[b as usize], v0, v1) {
            return true;
        }
    }

    false
}

fn has_triangle_flips_to(adjacency: &EdgeAdjacency, vertex_positions: &[Vector3], i0: u32, v1: &Vector3) -> bool {
    let v0 = &vertex_positions[i0 as usize];

    for e in adjacency.edges(i0) {
        if has_triangle_flip(&vertex_positions[e.next as usize], &vertex_positions[e.prev as usize], v0, v1) {
            return true;
        }
    }

    false
}

fn get_neighborhood_radius(adjacency: &EdgeAdjacency, vertex_positions: &[Vector3], i0: u32) -> f32 {
    let v0 = &vertex_positions[i0 as usize];

    let mut result = 0.0f32;

    for e in adjacency.edges(i0) {
        let va = &vertex_positions[e.next as usize];
        let vb = &vertex_positions[e.prev as usize];

        let da = (va.x - v0.x) * (va.x - v0.x) + (va.y - v0.y) * (va.y - v0.y) + (va.z - v0.z) * (va.z - v0.z);
        let db = (vb.x - v0.x) * (vb.x - v0.x) + (vb.y - v0.y) * (vb.y - v0.y) + (vb.z - v0.z) * (vb.z - v0.z);

        result = if result < da { da } else { result };
        result = if result < db { db } else { result };
    }

    result.sqrt()
}

fn get_complex_target(v: u32, target: u32, remap: &[u32], loop_: &[u32], loopback: &[u32]) -> u32 {
    let r = remap[target as usize];

    // use loop metadata to guide complex collapses towards the correct wedge
    // this works for edges on attribute discontinuities because loop/loopback track the single half-edge without a pair, similar to seams
    if loop_[v as usize] != NONE && remap[loop_[v as usize] as usize] == r {
        loop_[v as usize]
    } else if loopback[v as usize] != NONE && remap[loopback[v as usize] as usize] == r {
        loopback[v as usize]
    } else {
        target
    }
}

fn bound_edge_collapses(adjacency: &EdgeAdjacency, vertex_count: usize, index_count: usize, vertex_kind: &[u8]) -> usize {
    let mut dual_count = 0usize;

    for i in 0..vertex_count {
        let k = vertex_kind[i];
        let e = (adjacency.offsets[i + 1] - adjacency.offsets[i]) as usize;

        dual_count += if k == KIND_MANIFOLD || k == KIND_SEAM { e } else { 0 };
    }

    debug_assert!(dual_count <= index_count);

    // pad capacity by 3 so that we can check for overflow once per triangle instead of once per edge
    (index_count - dual_count / 2) + 3
}

fn pick_edge_collapses(collapses: &mut [Collapse], collapse_capacity: usize, indices: &[u32], remap: &[u32], vertex_kind: &[u8], loop_: &[u32], loopback: &[u32]) -> usize {
    const NEXT: [usize; 3] = [1, 2, 0];
    let mut collapse_count = 0usize;

    let mut i = 0;
    while i < indices.len() {
        // this should never happen as boundEdgeCollapses should give an upper bound for the collapse count, but in an unlikely event it does we can just drop extra collapses
        if collapse_count + 3 > collapse_capacity {
            break;
        }

        for e in 0..3 {
            let i0 = indices[i + e];
            let i1 = indices[i + NEXT[e]];

            // this can happen either when input has a zero-length edge, or when we perform collapses for complex
            // topology w/seams and collapse a manifold vertex that connects to both wedges onto one of them
            // we leave edges like this alone since they may be important for preserving mesh integrity
            if remap[i0 as usize] == remap[i1 as usize] {
                continue;
            }

            let k0 = vertex_kind[i0 as usize] as usize;
            let k1 = vertex_kind[i1 as usize] as usize;

            // the edge has to be collapsible in at least one direction
            if (CAN_COLLAPSE[k0][k1] | CAN_COLLAPSE[k1][k0]) == 0 {
                continue;
            }

            // manifold and seam edges should occur twice (i0->i1 and i1->i0) - skip redundant edges
            if HAS_OPPOSITE[k0][k1] != 0 && remap[i1 as usize] > remap[i0 as usize] {
                continue;
            }

            // two vertices are on a border or a seam, but there's no direct edge between them
            // this indicates that they belong to two different edge loops and we should not collapse this edge
            // loop[] and loopback[] track half edges so we only need to check one of them
            if (k0 as u8 == KIND_BORDER || k0 as u8 == KIND_SEAM) && k1 as u8 != KIND_MANIFOLD && loop_[i0 as usize] != i1 {
                continue;
            }
            if (k1 as u8 == KIND_BORDER || k1 as u8 == KIND_SEAM) && k0 as u8 != KIND_MANIFOLD && loopback[i1 as usize] != i0 {
                continue;
            }

            // edge can be collapsed in either direction - we will pick the one with minimum error
            // note: we evaluate error later during collapse ranking, here we just tag the edge as bidirectional
            if (CAN_COLLAPSE[k0][k1] & CAN_COLLAPSE[k1][k0]) != 0 {
                collapses[collapse_count] = Collapse { v0: i0, v1: i1, bits: 1 };
                collapse_count += 1;
            } else {
                // edge can only be collapsed in one direction
                let e0 = if CAN_COLLAPSE[k0][k1] != 0 { i0 } else { i1 };
                let e1 = if CAN_COLLAPSE[k0][k1] != 0 { i1 } else { i0 };

                collapses[collapse_count] = Collapse { v0: e0, v1: e1, bits: 0 };
                collapse_count += 1;
            }
        }
        i += 3;
    }

    collapse_count
}

fn rank_edge_collapses(collapses: &mut [Collapse], vertex_positions: &[Vector3], vertex_attributes: &[f32], vertex_quadrics: &[Quadric], attribute_quadrics: &[Quadric], attribute_gradients: &[QuadricGrad], attribute_count: usize, remap: &[u32], wedge: &[u32], vertex_kind: &[u8], loop_: &[u32], loopback: &[u32], clamped: bool) {
    let ac = attribute_count;
    let aerr = |v: u32, t: u32| -> f32 {
        quadric_error_attr(&attribute_quadrics[v as usize], &attribute_gradients[v as usize * ac..], ac, &vertex_positions[t as usize], &vertex_attributes[t as usize * ac..], clamped)
    };
    for c in collapses.iter_mut() {
        let i0 = c.v0;
        let i1 = c.v1;
        let bidi = c.bidi() != 0;

        let mut ei = quadric_error(&vertex_quadrics[remap[i0 as usize] as usize], &vertex_positions[i1 as usize]);
        let mut ej = if bidi { quadric_error(&vertex_quadrics[remap[i1 as usize] as usize], &vertex_positions[i0 as usize]) } else { f32::MAX };

        if attribute_count != 0 {
            ei += aerr(i0, i1);
            ej += if bidi { aerr(i1, i0) } else { 0.0 };

            // seam edges need to aggregate attribute errors between primary and secondary edges, as attribute quadrics are separate
            if vertex_kind[i0 as usize] == KIND_SEAM {
                // for seam collapses we need to find the seam pair; this is a bit tricky since we need to rely on edge loops as target vertex may be locked (and thus have more than two wedges)
                let s0 = wedge[i0 as usize];
                let mut s1 = if loop_[i0 as usize] == i1 { loopback[s0 as usize] } else { loop_[s0 as usize] };

                debug_assert!(wedge[s0 as usize] == i0); // s0 may be equal to i0 for half-seams

                // note: this should never happen due to the assertion above, but when disabled if we ever hit this case we'll get a memory safety issue; for now play it safe
                s1 = if s1 != NONE { s1 } else { wedge[i1 as usize] };

                ei += aerr(s0, s1);
                ej += if bidi { aerr(s1, s0) } else { 0.0 };
            } else {
                // complex edges can have multiple wedges, so we need to aggregate errors for all wedges based on the selected target
                if vertex_kind[i0 as usize] == KIND_COMPLEX || vertex_kind[i0 as usize] == KIND_FRINGE {
                    let mut v = wedge[i0 as usize];
                    while v != i0 {
                        let t = get_complex_target(v, i1, remap, loop_, loopback);

                        ei += aerr(v, t);
                        v = wedge[v as usize];
                    }
                }

                if (vertex_kind[i1 as usize] == KIND_COMPLEX || vertex_kind[i1 as usize] == KIND_FRINGE) && bidi {
                    let mut v = wedge[i1 as usize];
                    while v != i1 {
                        let t = get_complex_target(v, i0, remap, loop_, loopback);

                        ej += aerr(v, t);
                        v = wedge[v as usize];
                    }
                }
            }
        }

        // pick edge direction with minimal error (branchless)
        let rev = bidi & (ej < ei);

        c.v0 = if rev { i1 } else { i0 };
        c.v1 = if rev { i0 } else { i1 };
        c.set_error(if ej < ei { ej } else { ei });
    }
}

fn sort_edge_collapses(sort_order: &mut [u32], collapses: &[Collapse]) {
    // we use counting sort to order collapses by error; since the exact sort order is not as critical,
    // only top 12 bits of exponent+mantissa (8 bits of exponent and 4 bits of mantissa) are used.
    // to avoid excessive stack usage, we clamp the exponent range as collapses with errors much higher than 1 are not useful.
    const SORT_BITS: u32 = 12;
    const SORT_BINS: u32 = 2048 + 512; // exponent range [-127, 32)

    let key_of = |c: &Collapse| -> usize {
        // skip sign bit since error is non-negative
        let error = c.errorui();
        let key = (error << 1) >> (32 - SORT_BITS);
        (if key < SORT_BINS { key } else { SORT_BINS - 1 }) as usize
    };

    // fill histogram for counting sort
    let mut histogram = [0u32; SORT_BINS as usize];

    for c in collapses {
        histogram[key_of(c)] += 1;
    }

    // compute offsets based on histogram data
    let mut histogram_sum = 0usize;

    for h in histogram.iter_mut() {
        let count = *h as usize;
        *h = histogram_sum as u32;
        histogram_sum += count;
    }

    debug_assert!(histogram_sum == collapses.len());

    // compute sort order based on offsets
    for (i, c) in collapses.iter().enumerate() {
        let key = key_of(c);
        sort_order[histogram[key] as usize] = i as u32;
        histogram[key] += 1;
    }
}

fn perform_edge_collapses(collapse_remap: &mut [u32], collapse_locked: &mut [u8], collapses: &[Collapse], collapse_order: &[u32], remap: &[u32], wedge: &[u32], vertex_kind: &[u8], loop_: &[u32], loopback: &[u32], vertex_positions: &[Vector3], adjacency: &EdgeAdjacency, triangle_collapse_goal: usize, error_limit: f32, result_error: &mut f32) -> usize {
    let collapse_count = collapses.len();
    let mut edge_collapses = 0usize;
    let mut triangle_collapses = 0usize;

    // most collapses remove 2 triangles; use this to establish a bound on the pass in terms of error limit
    // note that edge_collapse_goal is an estimate; triangle_collapse_goal will be used to actually limit collapses
    let mut edge_collapse_goal = triangle_collapse_goal / 2;

    for i in 0..collapse_count {
        let c = collapses[collapse_order[i] as usize];

        if c.error() > error_limit {
            break;
        }

        if triangle_collapses >= triangle_collapse_goal {
            break;
        }

        // we limit the error in each pass based on the error of optimal last collapse; since many collapses will be locked
        // as they will share vertices with other successful collapses, we need to increase the acceptable error by some factor
        let error_goal = if edge_collapse_goal < collapse_count { 1.5f32 * collapses[collapse_order[edge_collapse_goal] as usize].error() } else { f32::MAX };

        // on average, each collapse is expected to lock 6 other collapses; to avoid degenerate passes on meshes with odd
        // topology, we only abort if we got over 1/6 collapses accordingly.
        if c.error() > error_goal && c.error() > *result_error && triangle_collapses > triangle_collapse_goal / 6 {
            break;
        }

        let i0 = c.v0;
        let i1 = c.v1;

        let r0 = remap[i0 as usize];
        let r1 = remap[i1 as usize];

        let kind = vertex_kind[i0 as usize];

        // we don't collapse vertices that had source or target vertex involved in a collapse
        // it's important to not move the vertices twice since it complicates the tracking/remapping logic
        // it's important to not move other vertices towards a moved vertex to preserve error since we don't re-rank collapses mid-pass
        if (collapse_locked[r0 as usize] | collapse_locked[r1 as usize]) != 0 {
            continue;
        }

        if has_triangle_flips(adjacency, vertex_positions, collapse_remap, r0, r1) {
            // adjust collapse goal since this collapse is invalid and shouldn't factor into error goal
            edge_collapse_goal += 1;

            continue;
        }

        debug_assert!(collapse_remap[r0 as usize] == r0);
        debug_assert!(collapse_remap[r1 as usize] == r1);

        if kind == KIND_COMPLEX || kind == KIND_FRINGE {
            collapse_remap[i0 as usize] = i1;

            // remap all vertices in the complex to the target vertex, using the same ranking that we used to evaluate the collapses
            let mut v = wedge[i0 as usize];
            while v != i0 {
                collapse_remap[v as usize] = get_complex_target(v, i1, remap, loop_, loopback);
                v = wedge[v as usize];
            }
        } else if kind == KIND_SEAM {
            // for seam collapses we need to move the seam pair together; this is a bit tricky since we need to rely on edge loops as target vertex may be locked (and thus have more than two wedges)
            let s0 = wedge[i0 as usize];
            let mut s1 = if loop_[i0 as usize] == i1 { loopback[s0 as usize] } else { loop_[s0 as usize] };
            debug_assert!(wedge[s0 as usize] == i0); // s0 may be equal to i0 for half-seams

            // note: this should never happen due to the assertion above, but when disabled if we ever hit this case we'll get a memory safety issue; for now play it safe
            s1 = if s1 != NONE { s1 } else { wedge[i1 as usize] };

            collapse_remap[i0 as usize] = i1;
            collapse_remap[s0 as usize] = s1;
        } else {
            debug_assert!(wedge[i0 as usize] == i0);

            collapse_remap[i0 as usize] = i1;
        }

        // note: we technically don't need to lock r1 if it's a locked vertex, as it can't move and its quadric won't be used
        // however, this results in slightly worse error on some meshes because the locked collapses get an unfair advantage wrt scheduling
        collapse_locked[r0 as usize] = 1;
        collapse_locked[r1 as usize] = 1;

        // border edges collapse 1 triangle, other edges collapse 2 or more
        // fringe edges often collapse 1 but may also collapse 2 because they don't respect loops, so we conservatively assume 2
        triangle_collapses += if kind == KIND_BORDER { 1 } else { 2 };
        edge_collapses += 1;

        *result_error = if *result_error < c.error() { c.error() } else { *result_error };
    }

    edge_collapses
}

fn update_quadrics(collapse_remap: &[u32], vertex_count: usize, vertex_quadrics: &mut [Quadric], volume_gradients: Option<&mut [QuadricGrad]>, attribute_quadrics: &mut [Quadric], attribute_gradients: &mut [QuadricGrad], attribute_count: usize, vertex_positions: &[Vector3], remap: &[u32], vertex_error: &mut f32) {
    let mut volume_gradients = volume_gradients;
    for i in 0..vertex_count {
        if collapse_remap[i] == i as u32 {
            continue;
        }

        let i0 = i as u32;
        let i1 = collapse_remap[i];

        let r0 = remap[i0 as usize];
        let r1 = remap[i1 as usize];

        // ensure we only update vertex_quadrics once: primary vertex must be moved if any wedge is moved
        if i0 == r0 {
            let q0 = vertex_quadrics[r0 as usize];
            quadric_add(&mut vertex_quadrics[r1 as usize], &q0);

            if let Some(vg) = volume_gradients.as_deref_mut() {
                let g0 = vg[r0 as usize];
                quadric_add_grad(&mut vg[r1 as usize], &g0);
            }
        }

        if attribute_count != 0 {
            let a0 = attribute_quadrics[i0 as usize];
            quadric_add(&mut attribute_quadrics[i1 as usize], &a0);
            let g0: Vec<QuadricGrad> = attribute_gradients[i0 as usize * attribute_count..(i0 as usize + 1) * attribute_count].to_vec();
            quadric_add_grads(&mut attribute_gradients[i1 as usize * attribute_count..], &g0, attribute_count);

            if i0 == r0 {
                // when attributes are used, distance error needs to be recomputed as collapses don't track it; it is safe to do this after the quadric adjustment
                let derr = quadric_error(&vertex_quadrics[r0 as usize], &vertex_positions[r1 as usize]);
                *vertex_error = if *vertex_error < derr { derr } else { *vertex_error };
            }
        }
    }
}

fn solve_positions(vertex_positions: &mut [Vector3], vertex_count: usize, vertex_quadrics: &[Quadric], volume_gradients: Option<&[QuadricGrad]>, attribute_quadrics: &[Quadric], attribute_gradients: &[QuadricGrad], attribute_count: usize, remap: &[u32], wedge: &[u32], adjacency: &EdgeAdjacency, vertex_kind: &[u8], vertex_update: &[u8]) {
    for i in 0..vertex_count {
        if vertex_update[i] == 0 {
            continue;
        }

        // moving vertices on an attribute discontinuity may result in extrapolating UV outside of the chart bounds
        // moving vertices on a border requires a stronger edge quadric to preserve the border geometry
        if vertex_kind[i] != KIND_MANIFOLD && vertex_kind[i] != KIND_COMPLEX {
            continue;
        }

        if remap[i] != i as u32 {
            vertex_positions[i] = vertex_positions[remap[i] as usize];
            continue;
        }

        let vp = vertex_positions[i];

        let mut q = vertex_quadrics[i];
        let mut gv = QuadricGrad::default();

        // add a point quadric for regularization to stabilize the solution
        let mut r = Quadric::default();
        quadric_from_point(&mut r, vp.x, vp.y, vp.z, q.w * 1e-4f32);
        quadric_add(&mut q, &r);

        if attribute_count != 0 {
            // optimal point simultaneously minimizes attribute quadrics for all wedges
            let mut v = i as u32;
            loop {
                quadric_reduce_attributes(&mut q, &attribute_quadrics[v as usize], &attribute_gradients[v as usize * attribute_count..], attribute_count);
                v = wedge[v as usize];
                if v == i as u32 {
                    break;
                }
            }

            // minimizing attribute quadrics results in volume loss so we incorporate volume gradient as a constraint
            if let Some(vg) = volume_gradients {
                gv = vg[i];
            }
        }

        let mut p = Vector3::default();
        if !quadric_solve(&mut p, &q, &gv) {
            continue;
        }

        // reject updates that move the vertex too far from its neighborhood
        // this detects and fixes most cases when the quadric is not well-defined
        let nr = get_neighborhood_radius(adjacency, vertex_positions, i as u32);
        let dp = (p.x - vp.x) * (p.x - vp.x) + (p.y - vp.y) * (p.y - vp.y) + (p.z - vp.z) * (p.z - vp.z);

        if dp > nr * nr {
            continue;
        }

        // reject updates that would flip a neighboring triangle, as we do for edge collapse
        if has_triangle_flips_to(adjacency, vertex_positions, i as u32, &p) {
            continue;
        }

        // reject updates that increase positional error too much; allow some tolerance to improve attribute quality
        if quadric_error(&vertex_quadrics[i], &p) > quadric_error(&vertex_quadrics[i], &vp) * 1.5f32 + 1e-6f32 {
            continue;
        }

        vertex_positions[i] = p;
    }
}

fn solve_attributes(vertex_positions: &[Vector3], vertex_attributes: &mut [f32], vertex_count: usize, attribute_quadrics: &[Quadric], attribute_gradients: &[QuadricGrad], attribute_count: usize, remap: &[u32], wedge: &[u32], vertex_kind: &[u8], vertex_update: &[u8]) {
    let ac = attribute_count;
    for i in 0..vertex_count {
        if vertex_update[i] == 0 {
            continue;
        }

        if remap[i] != i as u32 {
            continue;
        }

        for k in 0..attribute_count {
            let mut shared = NONE;

            // for complex vertices, preserve attribute continuity and use highest weight wedge if values were shared
            if vertex_kind[i] == KIND_COMPLEX || vertex_kind[i] == KIND_FRINGE {
                shared = i as u32;

                let mut v = wedge[i];
                while v != i as u32 {
                    if vertex_attributes[v as usize * ac + k] != vertex_attributes[i * ac + k] {
                        shared = NONE;
                    } else if shared != NONE && attribute_quadrics[v as usize].w > attribute_quadrics[shared as usize].w {
                        shared = v;
                    }
                    v = wedge[v as usize];
                }
            }

            // update attributes for all wedges
            let mut v = i as u32;
            loop {
                let r = if shared == NONE { v } else { shared } as usize;

                let p = &vertex_positions[i]; // same for all wedges
                let a = &attribute_quadrics[r];
                let g = &attribute_gradients[r * ac + k];

                let iw = if a.w == 0.0 { 0.0 } else { 1.0 / a.w };
                let av = (g.gx * p.x + g.gy * p.y + g.gz * p.z + g.gw) * iw;

                vertex_attributes[v as usize * ac + k] = av;
                v = wedge[v as usize];
                if v == i as u32 {
                    break;
                }
            }
        }
    }
}

/// The index list after a pass's collapses, degenerate triangles dropped; returns its
/// new length (the list is rewritten in place).
fn remap_index_buffer_collapse(indices: &mut [u32], index_count: usize, collapse_remap: &[u32], remap: &[u32]) -> usize {
    let mut write = 0;

    let mut i = 0;
    while i < index_count {
        let v0 = collapse_remap[indices[i] as usize];
        let v1 = collapse_remap[indices[i + 1] as usize];
        let v2 = collapse_remap[indices[i + 2] as usize];

        // we never move the vertex twice during a single pass
        debug_assert!(collapse_remap[v0 as usize] == v0);
        debug_assert!(collapse_remap[v1 as usize] == v1);
        debug_assert!(collapse_remap[v2 as usize] == v2);

        // collapse zero area triangles even if they are not topologically degenerate
        // this is required to cleanup manifold->seam collapses when a vertex is collapsed onto a seam pair
        // as well as complex collapses and some other cases where cross wedge collapses are performed
        let r0 = remap[v0 as usize];
        let r1 = remap[v1 as usize];
        let r2 = remap[v2 as usize];

        if r0 != r1 && r0 != r2 && r1 != r2 {
            indices[write] = v0;
            indices[write + 1] = v1;
            indices[write + 2] = v2;
            write += 3;
        }
        i += 3;
    }

    write
}

fn remap_edge_loops(loop_: &mut [u32], vertex_count: usize, collapse_remap: &[u32]) {
    for i in 0..vertex_count {
        // note: this is a no-op for vertices that were remapped
        // ideally we would clear the loop entries for those for consistency, even though they aren't going to be used
        // however, the remapping process needs loop information for remapped vertices, so this would require a separate pass
        if loop_[i] != NONE {
            let l = loop_[i];
            let r = collapse_remap[l as usize];

            // i == r is a special case when the seam edge is collapsed in a direction opposite to where loop goes
            if i as u32 == r {
                loop_[i] = if loop_[l as usize] != NONE { collapse_remap[loop_[l as usize] as usize] } else { NONE };
            } else {
                loop_[i] = r;
            }
        }
    }
}

fn follow(parents: &mut [u32], mut index: u32) -> u32 {
    while index != parents[index as usize] {
        let parent = parents[index as usize];
        parents[index as usize] = parents[parent as usize];
        index = parent;
    }

    index
}

fn build_components(components: &mut [u32], vertex_count: usize, indices: &[u32], remap: &[u32]) -> usize {
    const NEXT: [usize; 4] = [1, 2, 0, 1];
    for i in 0..vertex_count {
        components[i] = i as u32;
    }

    // compute a unique (but not sequential!) index for each component via union-find
    let mut i = 0;
    while i < indices.len() {
        for e in 0..3 {
            let i0 = indices[i + e];
            let i1 = indices[i + NEXT[e]];

            let mut r0 = remap[i0 as usize];
            let mut r1 = remap[i1 as usize];

            r0 = follow(components, r0);
            r1 = follow(components, r1);

            // merge components with larger indices into components with smaller indices
            // this guarantees that the root of the component is always the one with the smallest index
            if r0 != r1 {
                components[(if r0 < r1 { r1 } else { r0 }) as usize] = if r0 < r1 { r0 } else { r1 };
            }
        }
        i += 3;
    }

    // make sure each element points to the component root *before* we renumber the components
    for i in 0..vertex_count {
        if remap[i] == i as u32 {
            components[i] = follow(components, i as u32);
        }
    }

    let mut next_component: u32 = 0;

    // renumber components using sequential indices
    // a sequential pass is sufficient because component root always has the smallest index
    // note: it is unsafe to use follow() in this pass because we're replacing component links with sequential indices inplace
    for i in 0..vertex_count {
        if remap[i] == i as u32 {
            let root = components[i];
            debug_assert!(root as usize <= i); // make sure we already computed the component for non-roots
            components[i] = if root == i as u32 {
                next_component += 1;
                next_component - 1
            } else {
                components[root as usize]
            };
        } else {
            debug_assert!((remap[i] as usize) < i); // make sure we already computed the component
            components[i] = components[remap[i] as usize];
        }
    }

    next_component as usize
}

fn measure_components(component_errors: &mut [f32], component_count: usize, components: &[u32], vertex_positions: &[Vector3], vertex_count: usize) {
    for e in component_errors[..component_count * 4].iter_mut() {
        *e = 0.0;
    }

    // compute approximate sphere center for each component as an average
    for i in 0..vertex_count {
        let c = components[i] as usize;
        debug_assert!(c < component_count);

        let v = vertex_positions[i];

        component_errors[c * 4] += v.x;
        component_errors[c * 4 + 1] += v.y;
        component_errors[c * 4 + 2] += v.z;
        component_errors[c * 4 + 3] += 1.0; // weight
    }

    // complete the center computation, and reinitialize [3] as a radius
    for i in 0..component_count {
        let w = component_errors[i * 4 + 3];
        let iw = if w == 0.0 { 0.0 } else { 1.0 / w };

        component_errors[i * 4] *= iw;
        component_errors[i * 4 + 1] *= iw;
        component_errors[i * 4 + 2] *= iw;
        component_errors[i * 4 + 3] = 0.0; // radius
    }

    // compute squared radius for each component
    for i in 0..vertex_count {
        let c = components[i] as usize;

        let dx = vertex_positions[i].x - component_errors[c * 4];
        let dy = vertex_positions[i].y - component_errors[c * 4 + 1];
        let dz = vertex_positions[i].z - component_errors[c * 4 + 2];
        let r = dx * dx + dy * dy + dz * dz;

        component_errors[c * 4 + 3] = if component_errors[c * 4 + 3] < r { r } else { component_errors[c * 4 + 3] };
    }

    // we've used the output buffer as scratch space, so we need to move the results to proper indices
    // note: we keep the squared error to make it match quadric error metric
    for i in 0..component_count {
        component_errors[i] = component_errors[i * 4 + 3];
    }
}

fn prune_components(indices: &mut [u32], index_count: usize, components: &[u32], component_errors: &[f32], error_cutoff: f32, nexterror: &mut f32) -> usize {
    let mut write = 0;
    let mut min_error = f32::MAX;

    let mut i = 0;
    while i < index_count {
        let (v0, v1, v2) = (indices[i], indices[i + 1], indices[i + 2]);
        let c = components[v0 as usize] as usize;
        debug_assert!(c as u32 == components[v1 as usize] && c as u32 == components[v2 as usize]);

        if component_errors[c] > error_cutoff {
            min_error = if min_error > component_errors[c] { component_errors[c] } else { min_error };

            indices[write] = v0;
            indices[write + 1] = v1;
            indices[write + 2] = v2;
            write += 3;
        }
        i += 3;
    }

    // update next error with the smallest error of the remaining components
    *nexterror = min_error;
    write
}

struct CellHasher<'a> {
    vertex_ids: Option<&'a [u32]>,
}

impl Hasher for CellHasher<'_> {
    fn hash(&self, i: u32) -> usize {
        let mut h = match self.vertex_ids {
            Some(v) => v[i as usize],
            None => i,
        };

        // MurmurHash2 finalizer
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1e995);
        h ^= h >> 15;
        h as usize
    }

    fn equal(&self, lhs: u32, rhs: u32) -> bool {
        match self.vertex_ids {
            Some(v) => v[lhs as usize] == v[rhs as usize],
            None => lhs == rhs,
        }
    }
}

struct TriangleHasher<'a> {
    indices: &'a [u32],
}

impl Hasher for TriangleHasher<'_> {
    fn hash(&self, i: u32) -> usize {
        let tri = &self.indices[i as usize * 3..];

        // Optimized Spatial Hashing for Collision Detection of Deformable Objects
        (tri[0].wrapping_mul(73856093) ^ tri[1].wrapping_mul(19349663) ^ tri[2].wrapping_mul(83492791)) as usize
    }

    fn equal(&self, lhs: u32, rhs: u32) -> bool {
        let lt = &self.indices[lhs as usize * 3..];
        let rt = &self.indices[rhs as usize * 3..];

        lt[0] == rt[0] && lt[1] == rt[1] && lt[2] == rt[2]
    }
}

fn compute_vertex_ids(vertex_ids: &mut [u32], vertex_positions: &[Vector3], vertex_lock: Option<&[u8]>, vertex_count: usize, grid_size: i32) {
    debug_assert!((1..=1024).contains(&grid_size));
    let cell_scale = (grid_size - 1) as f32;

    for i in 0..vertex_count {
        let v = &vertex_positions[i];

        let xi = (v.x * cell_scale + 0.5f32) as i32;
        let yi = (v.y * cell_scale + 0.5f32) as i32;
        let zi = (v.z * cell_scale + 0.5f32) as i32;

        if vertex_lock.is_some_and(|l| (l[i] & VERTEX_LOCK) != 0) {
            vertex_ids[i] = (1u32 << 30) | i as u32;
        } else {
            vertex_ids[i] = (xi.wrapping_shl(20) | yi.wrapping_shl(10) | zi) as u32;
        }
    }
}

fn count_triangles(vertex_ids: &[u32], indices: &[u32]) -> usize {
    let mut result = 0;

    let mut i = 0;
    while i < indices.len() {
        let id0 = vertex_ids[indices[i] as usize];
        let id1 = vertex_ids[indices[i + 1] as usize];
        let id2 = vertex_ids[indices[i + 2] as usize];

        result += ((id0 != id1) & (id0 != id2) & (id1 != id2)) as usize;
        i += 3;
    }

    result
}

fn fill_vertex_cells(table: &mut [u32], table_size: usize, vertex_cells: &mut [u32], vertex_ids: &[u32], vertex_count: usize) -> usize {
    let hasher = CellHasher { vertex_ids: Some(vertex_ids) };

    for t in table[..table_size].iter_mut() {
        *t = NONE;
    }

    let mut result = 0usize;

    for i in 0..vertex_count {
        let entry = hash_lookup2(table, table_size, &hasher, i as u32, NONE);

        if table[entry] == NONE {
            table[entry] = i as u32;
            vertex_cells[i] = result as u32;
            result += 1;
        } else {
            vertex_cells[i] = vertex_cells[table[entry] as usize];
        }
    }

    result
}

fn count_vertex_cells(table: &mut [u32], table_size: usize, vertex_ids: &[u32], vertex_count: usize) -> usize {
    let hasher = CellHasher { vertex_ids: None };

    for t in table[..table_size].iter_mut() {
        *t = NONE;
    }

    let mut result = 0usize;

    for i in 0..vertex_count {
        let id = vertex_ids[i];
        let entry = hash_lookup2(table, table_size, &hasher, id, NONE);

        result += (table[entry] == NONE) as usize;
        table[entry] = id;
    }

    result
}

fn fill_cell_quadrics(cell_quadrics: &mut [Quadric], indices: &[u32], vertex_positions: &[Vector3], vertex_cells: &[u32]) {
    let mut i = 0;
    while i < indices.len() {
        let (i0, i1, i2) = (indices[i] as usize, indices[i + 1] as usize, indices[i + 2] as usize);

        let c0 = vertex_cells[i0] as usize;
        let c1 = vertex_cells[i1] as usize;
        let c2 = vertex_cells[i2] as usize;

        let single_cell = (c0 == c1) & (c0 == c2);

        let mut q = Quadric::default();
        quadric_from_triangle(&mut q, &vertex_positions[i0], &vertex_positions[i1], &vertex_positions[i2], if single_cell { 3.0 } else { 1.0 });

        if single_cell {
            quadric_add(&mut cell_quadrics[c0], &q);
        } else {
            quadric_add(&mut cell_quadrics[c0], &q);
            quadric_add(&mut cell_quadrics[c1], &q);
            quadric_add(&mut cell_quadrics[c2], &q);
        }
        i += 3;
    }
}

const DUMMY_COLOR: [f32; 3] = [0.0, 0.0, 0.0];

fn fill_cell_reservoirs(cell_reservoirs: &mut [Reservoir], cell_count: usize, vertex_positions: &[Vector3], vertex_colors: Option<&[f32]>, vertex_colors_stride_float: usize, vertex_count: usize, vertex_cells: &[u32]) {
    for i in 0..vertex_count {
        let cell = vertex_cells[i] as usize;
        let v = &vertex_positions[i];
        let r = &mut cell_reservoirs[cell];

        let color: &[f32] = match vertex_colors {
            Some(c) => &c[i * vertex_colors_stride_float..],
            None => &DUMMY_COLOR,
        };

        r.x += v.x;
        r.y += v.y;
        r.z += v.z;
        r.r += color[0];
        r.g += color[1];
        r.b += color[2];
        r.w += 1.0;
    }

    for r in cell_reservoirs[..cell_count].iter_mut() {
        let iw = if r.w == 0.0 { 0.0 } else { 1.0 / r.w };

        r.x *= iw;
        r.y *= iw;
        r.z *= iw;
        r.r *= iw;
        r.g *= iw;
        r.b *= iw;
    }
}

fn fill_cell_remap(cell_remap: &mut [u32], cell_errors: &mut [f32], cell_count: usize, vertex_cells: &[u32], cell_quadrics: &[Quadric], vertex_positions: &[Vector3], vertex_count: usize) {
    for c in cell_remap[..cell_count].iter_mut() {
        *c = NONE;
    }

    for i in 0..vertex_count {
        let cell = vertex_cells[i] as usize;
        let error = quadric_error(&cell_quadrics[cell], &vertex_positions[i]);

        if cell_remap[cell] == NONE || cell_errors[cell] > error {
            cell_remap[cell] = i as u32;
            cell_errors[cell] = error;
        }
    }
}

fn fill_cell_remap_colors(cell_remap: &mut [u32], cell_errors: &mut [f32], cell_count: usize, vertex_cells: &[u32], cell_reservoirs: &[Reservoir], vertex_positions: &[Vector3], vertex_colors: Option<&[f32]>, vertex_colors_stride_float: usize, color_weight: f32, vertex_count: usize) {
    for c in cell_remap[..cell_count].iter_mut() {
        *c = NONE;
    }

    for i in 0..vertex_count {
        let cell = vertex_cells[i] as usize;
        let v = &vertex_positions[i];
        let r = &cell_reservoirs[cell];

        let color: &[f32] = match vertex_colors {
            Some(c) => &c[i * vertex_colors_stride_float..],
            None => &DUMMY_COLOR,
        };

        let pos_error = (v.x - r.x) * (v.x - r.x) + (v.y - r.y) * (v.y - r.y) + (v.z - r.z) * (v.z - r.z);
        let col_error = (color[0] - r.r) * (color[0] - r.r) + (color[1] - r.g) * (color[1] - r.g) + (color[2] - r.b) * (color[2] - r.b);
        let error = pos_error + color_weight * col_error;

        if cell_remap[cell] == NONE || cell_errors[cell] > error {
            cell_remap[cell] = i as u32;
            cell_errors[cell] = error;
        }
    }
}

fn filter_triangles(destination: &mut [u32], tritable: &mut [u32], tritable_size: usize, indices: &[u32], vertex_cells: &[u32], cell_remap: &[u32]) -> usize {
    for t in tritable[..tritable_size].iter_mut() {
        *t = NONE;
    }

    let mut result = 0usize;

    let mut i = 0;
    while i < indices.len() {
        let c0 = vertex_cells[indices[i] as usize];
        let c1 = vertex_cells[indices[i + 1] as usize];
        let c2 = vertex_cells[indices[i + 2] as usize];

        if c0 != c1 && c0 != c2 && c1 != c2 {
            let mut a = cell_remap[c0 as usize];
            let mut b = cell_remap[c1 as usize];
            let mut c = cell_remap[c2 as usize];

            if b < a && b < c {
                let t = a;
                a = b;
                b = c;
                c = t;
            } else if c < a && c < b {
                let t = c;
                c = b;
                b = a;
                a = t;
            }

            destination[result * 3] = a;
            destination[result * 3 + 1] = b;
            destination[result * 3 + 2] = c;

            let entry = hash_lookup2(tritable, tritable_size, &TriangleHasher { indices: destination }, result as u32, NONE);

            if tritable[entry] == NONE {
                tritable[entry] = result as u32;
                result += 1;
            }
        }
        i += 3;
    }

    result * 3
}

fn interpolate(y: f32, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // three point interpolation from "revenge of interpolation search" paper
    let num = (y1 - y) * (x1 - x2) * (x1 - x0) * (y2 - y0);
    let den = (y2 - y) * (x1 - x2) * (y0 - y1) + (y0 - y) * (x1 - x0) * (y1 - y2);
    x1 + (if den == 0.0 { 0.0 } else { num / den })
}

/// `meshopt_SimplifyInternalSolve`: [`simplify_with_update`]'s option.
const INTERNAL_SOLVE: u32 = 1 << 29;

/// A buffer the C++ reads, and in [`simplify_with_update`] also writes.
enum Data<'a> {
    Read(&'a [f32]),
    Write(&'a mut [f32]),
}

impl Data<'_> {
    fn get(&self) -> &[f32] {
        match self {
            Data::Read(d) => d,
            Data::Write(d) => d,
        }
    }
}

/// `meshopt_simplifyEdge`: the edge-collapse simplifier all three entry points share.
/// `result` holds the indices on entry (the C++ copies them into `destination` first)
/// and the simplified ones on return, the count returned.
fn simplify_edge(
    result: &mut [u32],
    index_count: usize,
    mut vertex_positions_data: Data,
    mut vertex_count: usize,
    vertex_positions_stride_float: usize,
    mut vertex_attributes_data: Option<Data>,
    vertex_attributes_stride_float: usize,
    attribute_weights: &[f32],
    mut attribute_count: usize,
    vertex_lock: Option<&[u8]>,
    target_index_count: usize,
    target_error: f32,
    options: u32,
    out_result_error: Option<&mut f32>,
) -> usize {
    assert!(index_count % 3 == 0);
    assert!((3..=64).contains(&vertex_positions_stride_float));
    assert!(target_index_count <= index_count);
    assert!(target_error >= 0.0);
    assert!(vertex_attributes_stride_float >= attribute_count && vertex_attributes_stride_float <= 64);
    assert!(attribute_count <= MAX_ATTRIBUTES);
    for i in 0..attribute_count {
        assert!(attribute_weights[i] >= 0.0);
    }

    // build an index remap and update indices/vertex_count to minimize the subsequent work
    // note: as a consequence, errors will be computed relative to the subset extent
    let sparse_remap_v = if options & SPARSE != 0 {
        let r = build_sparse_remap(&mut result[..index_count], vertex_count);
        vertex_count = r.len();
        Some(r)
    } else {
        None
    };
    let sparse_remap = sparse_remap_v.as_deref();

    // build adjacency information
    let mut adjacency = EdgeAdjacency::new(index_count, vertex_count);
    update_edge_adjacency(&mut adjacency, &result[..index_count], vertex_count, None);

    // build position remap that maps each vertex to the one with identical position
    // wedge table stores next vertex with identical position for each vertex
    let mut remap = vec![0u32; vertex_count];
    let mut wedge = vec![0u32; vertex_count];
    build_position_remap(&mut remap, Some(&mut wedge), vertex_positions_data.get(), vertex_count, vertex_positions_stride_float, sparse_remap);

    // classify vertices; vertex kind determines collapse rules, see kCanCollapse
    let mut vertex_kind = vec![0u8; vertex_count];
    let mut loop_ = vec![0u32; vertex_count];
    let mut loopback = vec![0u32; vertex_count];
    classify_vertices(&mut vertex_kind, &mut loop_, &mut loopback, vertex_count, &adjacency, &remap, &wedge, vertex_lock, sparse_remap, options);

    let mut vertex_positions = vec![Vector3::default(); vertex_count];
    let mut vertex_offset = [0.0f32; 3];
    let vertex_scale = rescale_positions(Some(&mut vertex_positions), vertex_positions_data.get(), vertex_count, vertex_positions_stride_float, sparse_remap, Some(&mut vertex_offset));

    let mut vertex_attributes: Vec<f32> = Vec::new();
    let mut attribute_remap = [0u32; MAX_ATTRIBUTES];

    if attribute_count != 0 {
        // remap attributes to only include ones with weight > 0 to minimize memory/compute overhead for quadrics
        let mut attributes_used = 0;
        for i in 0..attribute_count {
            if attribute_weights[i] > 0.0 {
                attribute_remap[attributes_used] = i as u32;
                attributes_used += 1;
            }
        }

        attribute_count = attributes_used;
        vertex_attributes = vec![0.0; vertex_count * attribute_count];
        let data = vertex_attributes_data.as_ref().expect("attributes").get();
        rescale_attributes(&mut vertex_attributes, data, vertex_count, vertex_attributes_stride_float, attribute_weights, attribute_count, &attribute_remap, sparse_remap);
    }

    let mut vertex_quadrics = vec![Quadric::default(); vertex_count];

    let mut attribute_quadrics: Vec<Quadric> = Vec::new();
    let mut attribute_gradients: Vec<QuadricGrad> = Vec::new();
    let mut volume_gradients: Option<Vec<QuadricGrad>> = None;

    if attribute_count != 0 {
        attribute_quadrics = vec![Quadric::default(); vertex_count];
        attribute_gradients = vec![QuadricGrad::default(); vertex_count * attribute_count];

        if options & INTERNAL_SOLVE != 0 {
            volume_gradients = Some(vec![QuadricGrad::default(); vertex_count]);
        }
    }

    fill_face_quadrics(&mut vertex_quadrics, volume_gradients.as_deref_mut(), &result[..index_count], &vertex_positions, &remap);
    fill_vertex_quadrics(&mut vertex_quadrics, &vertex_positions, vertex_count, &remap, vertex_lock, sparse_remap, options);
    fill_edge_quadrics(&mut vertex_quadrics, &result[..index_count], &vertex_positions, &remap, &vertex_kind, &loop_, &loopback);

    if options & PRESERVE_FOLDS != 0 {
        fill_fold_quadrics(&mut vertex_quadrics, &adjacency, &vertex_positions, vertex_count, &remap, &wedge);
    }

    if attribute_count != 0 {
        fill_attribute_quadrics(&mut attribute_quadrics, &mut attribute_gradients, &result[..index_count], &vertex_positions, &vertex_attributes, attribute_count);
    }

    let mut components: Vec<u32> = Vec::new();
    let mut component_errors: Vec<f32> = Vec::new();
    let mut component_count = 0usize;
    let mut component_nexterror = 0.0f32;

    if options & PRUNE != 0 {
        components = vec![0u32; vertex_count];
        component_count = build_components(&mut components, vertex_count, &result[..index_count], &remap);

        component_errors = vec![0.0f32; component_count * 4]; // overallocate for temporary use inside measureComponents
        measure_components(&mut component_errors, component_count, &components, &vertex_positions, vertex_count);

        component_nexterror = f32::MAX;
        for i in 0..component_count {
            component_nexterror = if component_nexterror > component_errors[i] { component_errors[i] } else { component_nexterror };
        }
    }

    let collapse_capacity = bound_edge_collapses(&adjacency, vertex_count, index_count, &vertex_kind);

    let mut edge_collapses = vec![Collapse::default(); collapse_capacity];
    let mut collapse_order = vec![0u32; collapse_capacity];
    let mut collapse_remap = vec![0u32; vertex_count];
    let mut collapse_locked = vec![0u8; vertex_count];

    let mut result_count = index_count;
    let mut result_error = 0.0f32;
    let mut vertex_error = 0.0f32;

    // target_error input is linear; we need to adjust it to match quadricError units
    let error_scale = if options & ERROR_ABSOLUTE != 0 { vertex_scale } else { 1.0 };
    let error_limit = (target_error * target_error) / (error_scale * error_scale);

    while result_count > target_index_count {
        // note: throughout the simplification process adjacency structure reflects welded topology for result-in-progress
        update_edge_adjacency(&mut adjacency, &result[..result_count], vertex_count, Some(&remap));

        let edge_collapse_count = pick_edge_collapses(&mut edge_collapses, collapse_capacity, &result[..result_count], &remap, &vertex_kind, &loop_, &loopback);
        debug_assert!(edge_collapse_count <= collapse_capacity);

        // no edges can be collapsed any more due to topology restrictions
        if edge_collapse_count == 0 {
            break;
        }

        rank_edge_collapses(&mut edge_collapses[..edge_collapse_count], &vertex_positions, &vertex_attributes, &vertex_quadrics, &attribute_quadrics, &attribute_gradients, attribute_count, &remap, &wedge, &vertex_kind, &loop_, &loopback, (options & ERROR_CLAMPED) != 0);

        sort_edge_collapses(&mut collapse_order, &edge_collapses[..edge_collapse_count]);

        let triangle_collapse_goal = (result_count - target_index_count) / 3;

        for i in 0..vertex_count {
            collapse_remap[i] = i as u32;
        }

        for l in collapse_locked.iter_mut() {
            *l = 0;
        }

        let collapses = perform_edge_collapses(&mut collapse_remap, &mut collapse_locked, &edge_collapses[..edge_collapse_count], &collapse_order, &remap, &wedge, &vertex_kind, &loop_, &loopback, &vertex_positions, &adjacency, triangle_collapse_goal, error_limit, &mut result_error);

        // no edges can be collapsed any more due to hitting the error limit or triangle collapse limit
        if collapses == 0 {
            break;
        }

        update_quadrics(&collapse_remap, vertex_count, &mut vertex_quadrics, volume_gradients.as_deref_mut(), &mut attribute_quadrics, &mut attribute_gradients, attribute_count, &vertex_positions, &remap, &mut vertex_error);

        // updateQuadrics will update vertex error if we use attributes, but if we don't then result_error and vertex_error are equivalent
        vertex_error = if attribute_count == 0 { result_error } else { vertex_error };

        // note: we update loops following edge collapses, but after this we might still have stale loop data
        // this can happen when a triangle with a loop edge gets collapsed along a non-loop edge
        // that works since a loop that points to a vertex that is no longer connected is not affecting collapse logic
        remap_edge_loops(&mut loop_, vertex_count, &collapse_remap);
        remap_edge_loops(&mut loopback, vertex_count, &collapse_remap);

        result_count = remap_index_buffer_collapse(result, result_count, &collapse_remap, &remap);

        if (options & PRUNE) != 0 && result_count > target_index_count && component_nexterror <= vertex_error {
            result_count = prune_components(result, result_count, &components, &component_errors, vertex_error, &mut component_nexterror);
        }
    }

    // at this point, component_nexterror might be stale: component it references may have been removed through a series of edge collapses
    let mut component_nextstale = true;

    // we're done with the regular simplification but we're still short of the target; try pruning more aggressively towards error_limit
    while (options & PRUNE) != 0 && result_count > target_index_count && component_nexterror <= error_limit {
        let component_cutoff = if component_nexterror * 1.5f32 < error_limit { component_nexterror * 1.5f32 } else { error_limit };

        // track maximum error in eligible components as we are increasing resulting error
        let mut component_maxerror = 0.0f32;
        for i in 0..component_count {
            if component_errors[i] > component_maxerror && component_errors[i] <= component_cutoff {
                component_maxerror = component_errors[i];
            }
        }

        let new_count = prune_components(result, result_count, &components, &component_errors, component_cutoff, &mut component_nexterror);
        if new_count == result_count && !component_nextstale {
            break;
        }

        component_nextstale = false; // pruneComponents guarantees next error is up to date
        result_count = new_count;
        result_error = if result_error < component_maxerror { component_maxerror } else { result_error };
        vertex_error = if vertex_error < component_maxerror { component_maxerror } else { vertex_error };
    }

    // if solve is requested, update input buffers destructively from internal data
    if options & INTERNAL_SOLVE != 0 {
        let vertex_update = &mut collapse_locked; // reuse as scratch space
        for u in vertex_update.iter_mut() {
            *u = 0;
        }

        // limit quadric solve to vertices that are still used in the result
        for i in 0..result_count {
            let v = result[i] as usize;

            // mark the vertex for finalizeVertices and root vertex for solve*
            vertex_update[remap[v] as usize] = 1;
            vertex_update[v] = 1;
        }

        // edge adjacency may be stale as we haven't updated it after last series of edge collapses
        update_edge_adjacency(&mut adjacency, &result[..result_count], vertex_count, Some(&remap));

        solve_positions(&mut vertex_positions, vertex_count, &vertex_quadrics, volume_gradients.as_deref(), &attribute_quadrics, &attribute_gradients, attribute_count, &remap, &wedge, &adjacency, &vertex_kind, vertex_update);

        if attribute_count != 0 {
            solve_attributes(&vertex_positions, &mut vertex_attributes, vertex_count, &attribute_quadrics, &attribute_gradients, attribute_count, &remap, &wedge, &vertex_kind, vertex_update);
        }

        if let Data::Write(positions) = &mut vertex_positions_data {
            let attrs = match vertex_attributes_data.as_mut() {
                Some(Data::Write(a)) => Some(&mut **a),
                _ => None,
            };
            finalize_vertices(positions, vertex_positions_stride_float, attrs, vertex_attributes_stride_float, attribute_weights, attribute_count, vertex_count, &vertex_positions, &vertex_attributes, sparse_remap, &attribute_remap, vertex_scale, &vertex_offset, &vertex_kind, vertex_update, vertex_lock);
        }
    }

    // convert resulting indices back into the dense space of the larger mesh
    if let Some(sr) = sparse_remap {
        for i in 0..result_count {
            result[i] = sr[result[i] as usize];
        }
    }

    // result_error is quadratic; we need to remap it back to linear
    if let Some(e) = out_result_error {
        *e = result_error.sqrt() * error_scale;
    }

    result_count
}

/// `meshopt_simplify`: `indices` reduced towards `target_index_count` indices without
/// going over `target_error` (relative to the mesh's extent unless `ERROR_ABSOLUTE`).
/// Returns the new index list; `result_error`, the error reached.
pub fn simplify(indices: &[u32], vertex_positions: &[f32], vertex_count: usize, vertex_positions_stride_float: usize, target_index_count: usize, target_error: f32, options: u32, result_error: Option<&mut f32>) -> Vec<u32> {
    assert!(options & INTERNAL_SOLVE == 0); // use simplify_with_update instead
    let mut out = indices.to_vec();
    let n = simplify_edge(&mut out, indices.len(), Data::Read(vertex_positions), vertex_count, vertex_positions_stride_float, None, 0, &[], 0, None, target_index_count, target_error, options, result_error);
    out.truncate(n);
    out
}

/// `meshopt_simplifyWithAttributes`: as [`simplify`], the attributes (`attribute_count`
/// floats per vertex, `vertex_attributes_stride_float` apart, each weighted by
/// `attribute_weights`: UVs, normals, colours) kept as they were where they meet, and
/// `vertex_lock` (one byte per vertex: `VERTEX_LOCK`, `VERTEX_PROTECT`, `VERTEX_PRIORITY`).
pub fn simplify_with_attributes(indices: &[u32], vertex_positions: &[f32], vertex_count: usize, vertex_positions_stride_float: usize, vertex_attributes: &[f32], vertex_attributes_stride_float: usize, attribute_weights: &[f32], attribute_count: usize, vertex_lock: Option<&[u8]>, target_index_count: usize, target_error: f32, options: u32, result_error: Option<&mut f32>) -> Vec<u32> {
    assert!(options & INTERNAL_SOLVE == 0); // use simplify_with_update instead
    let mut out = indices.to_vec();
    let n = simplify_edge(&mut out, indices.len(), Data::Read(vertex_positions), vertex_count, vertex_positions_stride_float, Some(Data::Read(vertex_attributes)), vertex_attributes_stride_float, attribute_weights, attribute_count, vertex_lock, target_index_count, target_error, options, result_error);
    out.truncate(n);
    out
}

/// `meshopt_simplifyWithUpdate`: as [`simplify_with_attributes`], in place, and the
/// vertices still used moved (and their attributes changed) to fit the result better.
/// Returns the new index count; `indices` holds them at its front.
pub fn simplify_with_update(indices: &mut [u32], vertex_positions: &mut [f32], vertex_count: usize, vertex_positions_stride_float: usize, vertex_attributes: Option<&mut [f32]>, vertex_attributes_stride_float: usize, attribute_weights: &[f32], attribute_count: usize, vertex_lock: Option<&[u8]>, target_index_count: usize, target_error: f32, options: u32, result_error: Option<&mut f32>) -> usize {
    let index_count = indices.len();
    simplify_edge(indices, index_count, Data::Write(vertex_positions), vertex_count, vertex_positions_stride_float, vertex_attributes.map(Data::Write), vertex_attributes_stride_float, attribute_weights, attribute_count, vertex_lock, target_index_count, target_error, options | INTERNAL_SOLVE, result_error)
}

/// `meshopt_simplifySloppy`: vertices clustered on a grid, much faster than [`simplify`]
/// and much rougher; topology is not kept. `vertex_lock` (`VERTEX_LOCK`) keeps vertices
/// out of the clustering.
pub fn simplify_sloppy(indices: &[u32], vertex_positions_data: &[f32], vertex_count: usize, vertex_positions_stride_float: usize, vertex_lock: Option<&[u8]>, target_index_count: usize, target_error: f32, out_result_error: Option<&mut f32>) -> Vec<u32> {
    let index_count = indices.len();
    assert!(index_count % 3 == 0);
    assert!((3..=64).contains(&vertex_positions_stride_float));
    assert!(target_index_count <= index_count);
    assert!(target_error >= 0.0);

    // we expect to get ~2 triangles/vertex in the output
    let target_cell_count = target_index_count / 6;

    let mut vertex_positions = vec![Vector3::default(); vertex_count];
    rescale_positions(Some(&mut vertex_positions), vertex_positions_data, vertex_count, vertex_positions_stride_float, None, None);

    // find the optimal grid size using guided binary search
    let mut vertex_ids = vec![0u32; vertex_count];

    const INTERPOLATION_PASSES: i32 = 5;

    // invariant: # of triangles in min_grid <= target_count
    let mut min_grid = (1.0f32 / (if target_error < 1e-3f32 { 1e-3f32 } else if target_error < 1.0 { target_error } else { 1.0 })) as i32;
    let mut max_grid: i32 = 1025;
    let mut min_triangles = 0usize;
    let mut max_triangles = index_count / 3;

    // when we're error-limited, we compute the triangle count for the min. size; this accelerates convergence and provides the correct answer when we can't use a larger grid
    if min_grid > 1 || vertex_lock.is_some() {
        compute_vertex_ids(&mut vertex_ids, &vertex_positions, vertex_lock, vertex_count, min_grid);
        min_triangles = count_triangles(&vertex_ids, indices);
    }

    // instead of starting in the middle, let's guess as to what the answer might be! triangle count usually grows as a square of grid size...
    let mut next_grid_size = ((target_cell_count as f32).sqrt() + 0.5f32) as i32;

    for pass in 0..10 + INTERPOLATION_PASSES {
        if min_triangles >= target_index_count / 3 || max_grid - min_grid <= 1 {
            break;
        }

        // we clamp the prediction of the grid size to make sure that the search converges
        let mut grid_size = next_grid_size;
        grid_size = if grid_size <= min_grid { min_grid + 1 } else if grid_size >= max_grid { max_grid - 1 } else { grid_size };

        compute_vertex_ids(&mut vertex_ids, &vertex_positions, vertex_lock, vertex_count, grid_size);
        let triangles = count_triangles(&vertex_ids, indices);

        let tip = interpolate((target_index_count / 3) as f32, min_grid as f32, min_triangles as f32, grid_size as f32, triangles as f32, max_grid as f32, max_triangles as f32);

        if triangles <= target_index_count / 3 {
            min_grid = grid_size;
            min_triangles = triangles;
        } else {
            max_grid = grid_size;
            max_triangles = triangles;
        }

        // we start by using interpolation search - it usually converges faster
        // however, interpolation search has a worst case of O(N) so we switch to binary search after a few iterations which converges in O(logN)
        next_grid_size = if pass < INTERPOLATION_PASSES { (tip + 0.5f32) as i32 } else { (min_grid + max_grid) / 2 };
    }

    if min_triangles == 0 {
        if let Some(e) = out_result_error {
            *e = 1.0;
        }

        return Vec::new();
    }

    // build vertex->cell association by mapping all vertices with the same quantized position to the same cell
    let table_size = hash_buckets2(vertex_count);
    let mut table = vec![0u32; table_size];

    let mut vertex_cells = vec![0u32; vertex_count];

    compute_vertex_ids(&mut vertex_ids, &vertex_positions, vertex_lock, vertex_count, min_grid);
    let cell_count = fill_vertex_cells(&mut table, table_size, &mut vertex_cells, &vertex_ids, vertex_count);

    // build a quadric for each target cell
    let mut cell_quadrics = vec![Quadric::default(); cell_count];

    fill_cell_quadrics(&mut cell_quadrics, indices, &vertex_positions, &vertex_cells);

    // for each target cell, find the vertex with the minimal error
    let mut cell_remap = vec![0u32; cell_count];
    let mut cell_errors = vec![0.0f32; cell_count];

    fill_cell_remap(&mut cell_remap, &mut cell_errors, cell_count, &vertex_cells, &cell_quadrics, &vertex_positions, vertex_count);

    // compute error
    let mut result_error = 0.0f32;

    for i in 0..cell_count {
        result_error = if result_error < cell_errors[i] { cell_errors[i] } else { result_error };
    }

    // vertex collapses often result in duplicate triangles; we need a table to filter them out
    let tritable_size = hash_buckets2(min_triangles);
    let mut tritable = vec![0u32; tritable_size];

    let mut destination = vec![0u32; index_count];
    let write = filter_triangles(&mut destination, &mut tritable, tritable_size, indices, &vertex_cells, &cell_remap);
    destination.truncate(write);

    if let Some(e) = out_result_error {
        *e = result_error.sqrt();
    }

    destination
}

/// `meshopt_simplifyPrune`: the disconnected pieces smaller than `target_error` (relative
/// to the mesh's extent) removed, nothing else changed.
pub fn simplify_prune(indices: &[u32], vertex_positions_data: &[f32], vertex_count: usize, vertex_positions_stride_float: usize, target_error: f32) -> Vec<u32> {
    let index_count = indices.len();
    assert!(index_count % 3 == 0);
    assert!((3..=64).contains(&vertex_positions_stride_float));
    assert!(target_error >= 0.0);

    let mut result = indices.to_vec();

    // build position remap that maps each vertex to the one with identical position
    let mut remap = vec![0u32; vertex_count];
    build_position_remap(&mut remap, None, vertex_positions_data, vertex_count, vertex_positions_stride_float, None);

    let mut vertex_positions = vec![Vector3::default(); vertex_count];
    rescale_positions(Some(&mut vertex_positions), vertex_positions_data, vertex_count, vertex_positions_stride_float, None, None);

    let mut components = vec![0u32; vertex_count];
    let component_count = build_components(&mut components, vertex_count, indices, &remap);

    let mut component_errors = vec![0.0f32; component_count * 4]; // overallocate for temporary use inside measureComponents
    measure_components(&mut component_errors, component_count, &components, &vertex_positions, vertex_count);

    let mut component_nexterror = 0.0f32;
    let result_count = prune_components(&mut result, index_count, &components, &component_errors, target_error * target_error, &mut component_nexterror);

    result.truncate(result_count);
    result
}

/// `meshopt_simplifyPoints`: a point cloud reduced to at most `target_vertex_count`
/// points, by position and (when given, weighted by `color_weight`) colour. Returns the
/// vertices kept.
pub fn simplify_points(vertex_positions_data: &[f32], vertex_count: usize, vertex_positions_stride_float: usize, vertex_colors: Option<&[f32]>, vertex_colors_stride_float: usize, color_weight: f32, target_vertex_count: usize) -> Vec<u32> {
    assert!((3..=64).contains(&vertex_positions_stride_float));
    assert!(vertex_colors_stride_float == 0 || (3..=64).contains(&vertex_colors_stride_float));
    assert!(vertex_colors.is_none() || vertex_colors_stride_float != 0);
    assert!(target_vertex_count <= vertex_count);

    let target_cell_count = target_vertex_count;

    if target_cell_count == 0 {
        return Vec::new();
    }

    let mut vertex_positions = vec![Vector3::default(); vertex_count];
    rescale_positions(Some(&mut vertex_positions), vertex_positions_data, vertex_count, vertex_positions_stride_float, None, None);

    // find the optimal grid size using guided binary search
    let mut vertex_ids = vec![0u32; vertex_count];

    let table_size = hash_buckets2(vertex_count);
    let mut table = vec![0u32; table_size];

    const INTERPOLATION_PASSES: i32 = 5;

    // invariant: # of vertices in min_grid <= target_count
    let mut min_grid: i32 = 0;
    let mut max_grid: i32 = 1025;
    let mut min_vertices = 0usize;
    let mut max_vertices = vertex_count;

    // instead of starting in the middle, let's guess as to what the answer might be! surface point count usually grows as a square of grid size...
    let mut next_grid_size = ((target_cell_count as f32).sqrt() + 0.5f32) as i32;

    for pass in 0..10 + INTERPOLATION_PASSES {
        debug_assert!(min_vertices < target_vertex_count);
        debug_assert!(max_grid - min_grid > 1);

        // we clamp the prediction of the grid size to make sure that the search converges
        let mut grid_size = next_grid_size;
        grid_size = if grid_size <= min_grid { min_grid + 1 } else if grid_size >= max_grid { max_grid - 1 } else { grid_size };

        compute_vertex_ids(&mut vertex_ids, &vertex_positions, None, vertex_count, grid_size);
        let vertices = count_vertex_cells(&mut table, table_size, &vertex_ids, vertex_count);

        let tip = interpolate(target_vertex_count as f32, min_grid as f32, min_vertices as f32, grid_size as f32, vertices as f32, max_grid as f32, max_vertices as f32);

        if vertices <= target_vertex_count {
            min_grid = grid_size;
            min_vertices = vertices;
        } else {
            max_grid = grid_size;
            max_vertices = vertices;
        }

        if vertices == target_vertex_count || max_grid - min_grid <= 1 {
            break;
        }

        // we start by using interpolation search - it usually converges faster
        // however, interpolation search has a worst case of O(N) so we switch to binary search after a few iterations which converges in O(logN)
        next_grid_size = if pass < INTERPOLATION_PASSES { (tip + 0.5f32) as i32 } else { (min_grid + max_grid) / 2 };
    }

    if min_vertices == 0 {
        return Vec::new();
    }

    // build vertex->cell association by mapping all vertices with the same quantized position to the same cell
    let mut vertex_cells = vec![0u32; vertex_count];

    compute_vertex_ids(&mut vertex_ids, &vertex_positions, None, vertex_count, min_grid);
    let cell_count = fill_vertex_cells(&mut table, table_size, &mut vertex_cells, &vertex_ids, vertex_count);

    // accumulate points into a reservoir for each target cell
    let mut cell_reservoirs = vec![Reservoir::default(); cell_count];

    fill_cell_reservoirs(&mut cell_reservoirs, cell_count, &vertex_positions, vertex_colors, vertex_colors_stride_float, vertex_count, &vertex_cells);

    // for each target cell, find the vertex with the minimal error
    let mut cell_remap = vec![0u32; cell_count];
    let mut cell_errors = vec![0.0f32; cell_count];

    // we scale the color weight to bring it to the same scale as position so that error addition makes sense
    let color_weight_scaled = color_weight * (if min_grid == 1 { 1.0 } else { 1.0 / (min_grid - 1) as f32 });

    fill_cell_remap_colors(&mut cell_remap, &mut cell_errors, cell_count, &vertex_cells, &cell_reservoirs, &vertex_positions, vertex_colors, vertex_colors_stride_float, color_weight_scaled * color_weight_scaled, vertex_count);

    // copy results to the output
    debug_assert!(cell_count <= target_vertex_count);
    cell_remap
}

/// `meshopt_simplifyScale`: the mesh's extent, what [`simplify`]'s relative errors are
/// relative to (multiply a result error by it for mesh units).
pub fn simplify_scale(vertex_positions: &[f32], vertex_count: usize, vertex_positions_stride_float: usize) -> f32 {
    assert!((3..=64).contains(&vertex_positions_stride_float));

    rescale_positions(None, vertex_positions, vertex_count, vertex_positions_stride_float, None, None)
}
