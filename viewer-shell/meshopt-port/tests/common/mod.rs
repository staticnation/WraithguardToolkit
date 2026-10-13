//! The meshes the oracle tests run on, made the same way for the C++ (through
//! `examples/dump_parts.rs`) and for the tests: every NiTriShapeData in the page's fixture
//! meshes, as the file has it, and a few generated ones large enough to load the caches.
#![allow(dead_code)]

use gardenfell_nif::netimmerse::stream::Stream;
use gardenfell_nif::netimmerse::types::NiType;

pub struct Part {
    pub name: String,
    pub pos: Vec<f32>,
    pub idx: Vec<u32>,
}

impl Part {
    pub fn vertex_count(&self) -> usize {
        self.pos.len() / 3
    }
}

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/tests/fixture/Data Files/meshes")
}

/// Every NiTriShapeData of a NIF, in block order.
pub fn nif_parts(bytes: &[u8], name: &str) -> Vec<Part> {
    let Ok(s) = Stream::parse(bytes) else { return Vec::new() };
    let mut out = Vec::new();
    for blk in &s.blocks {
        if let NiType::NiTriShapeData(d) = blk {
            let pos: Vec<f32> = d.base.base.vertices.iter().flat_map(|v| v.iter().copied()).collect();
            let idx: Vec<u32> = d.triangles.iter().flat_map(|t| t.iter().map(|&i| i as u32)).collect();
            let nv = pos.len() / 3;
            if idx.is_empty() || idx.iter().any(|&i| i as usize >= nv) {
                continue;
            }
            out.push(Part { name: format!("{name}#{}", out.len()), pos, idx });
        }
    }
    out
}

fn fixture_parts() -> Vec<Part> {
    let mut files = Vec::new();
    for sub in std::fs::read_dir(fixture_dir()).unwrap() {
        let sub = sub.unwrap().path();
        if sub.is_dir() {
            for f in std::fs::read_dir(&sub).unwrap() {
                let f = f.unwrap().path();
                if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("nif")) {
                    files.push(f);
                }
            }
        }
    }
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let rel = format!("{}/{}", f.parent().unwrap().file_name().unwrap().to_string_lossy(), f.file_name().unwrap().to_string_lossy());
        out.extend(nif_parts(&std::fs::read(&f).unwrap(), &rel));
    }
    out
}

/// A `n` x `n` quad grid, rows in order, slightly bent so no axis sees it edge-on.
fn grid(n: u32) -> Part {
    let mut pos = Vec::new();
    for y in 0..=n {
        for x in 0..=n {
            let (fx, fy) = (x as f32 / n as f32, y as f32 / n as f32);
            // A polynomial bend, not sin/cos: the platforms' maths libraries may differ
            // in the last bit, and the pinned values are bit for bit.
            pos.extend_from_slice(&[fx, fy, 0.25 * fx * (1.0 - fx) * (0.5 - fy)]);
        }
    }
    let mut idx = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let a = y * (n + 1) + x;
            idx.extend_from_slice(&[a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]);
        }
    }
    Part { name: format!("grid{n}"), pos, idx }
}

/// The same grid with its triangles shuffled (a fixed LCG): an exporter's worst order.
fn shuffled(mut p: Part) -> Part {
    let tris = p.idx.len() / 3;
    let mut seed: u32 = 12345;
    for i in (1..tris).rev() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let j = (seed >> 8) as usize % (i + 1);
        for k in 0..3 {
            p.idx.swap(i * 3 + k, j * 3 + k);
        }
    }
    p.name = format!("{}-shuffled", p.name);
    p
}

/// A cube-sphere (six `n` x `n` faces pushed out to the unit sphere; `sqrt` only, which
/// IEEE rounds the same everywhere): seen from every axis, front and back faces.
fn sphere(n: u32) -> Part {
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    // (u axis, v axis, normal axis, normal sign)
    let faces = [(0usize, 1usize, 2usize, 1.0f32), (1, 0, 2, -1.0), (2, 0, 1, 1.0), (0, 2, 1, -1.0), (1, 2, 0, 1.0), (2, 1, 0, -1.0)];
    for (u_ax, v_ax, n_ax, sign) in faces {
        let base = (pos.len() / 3) as u32;
        for j in 0..=n {
            for i in 0..=n {
                let mut p = [0.0f32; 3];
                p[u_ax] = 2.0 * i as f32 / n as f32 - 1.0;
                p[v_ax] = 2.0 * j as f32 / n as f32 - 1.0;
                p[n_ax] = sign;
                let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                pos.extend_from_slice(&[p[0] / l, p[1] / l, p[2] / l]);
            }
        }
        for j in 0..n {
            for i in 0..n {
                let a = base + j * (n + 1) + i;
                idx.extend_from_slice(&[a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]);
            }
        }
    }
    Part { name: format!("sphere{n}"), pos, idx }
}

/// Eight overlapping, slightly offset copies of a bent quad grid: overdraw well above 1,
/// and the vertex buffer in an order the triangles do not follow (copies interleaved).
fn layers() -> Part {
    let g = grid(12);
    let nv = g.vertex_count() as u32;
    let mut pos = Vec::new();
    for v in 0..nv as usize {
        for l in 0..8 {
            let o = l as f32 * 0.05;
            pos.extend_from_slice(&[g.pos[v * 3] + o, g.pos[v * 3 + 1] + o * 0.5, g.pos[v * 3 + 2] + o]);
        }
    }
    let mut idx = Vec::new();
    for l in 0..8u32 {
        idx.extend(g.idx.iter().map(|&i| i * 8 + l));
    }
    Part { name: "layers".into(), pos, idx }
}

pub fn all_parts() -> Vec<Part> {
    let mut v = fixture_parts();
    v.push(grid(40));
    v.push(shuffled(grid(40)));
    v.push(sphere(20));
    v.push(shuffled(sphere(20)));
    v.push(layers());
    v
}

/// FNV-1a over the bytes of a `u32` list (Robin's `tests/triorder.rs` hash).
pub fn fnv(v: &[u32]) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for &x in v {
        for k in 0..4 {
            h ^= ((x >> (k * 8)) & 255) as u64;
            h = h.wrapping_mul(1099511628211);
        }
    }
    h
}

/// The parts as one file the C++ oracle reads: a count, then per part its name (length
/// and bytes), vertex count, index count, positions and indices, all little-endian.
pub fn dump(parts: &[Part]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend((parts.len() as u32).to_le_bytes());
    for p in parts {
        b.extend((p.name.len() as u32).to_le_bytes());
        b.extend(p.name.as_bytes());
        b.extend((p.vertex_count() as u32).to_le_bytes());
        b.extend((p.idx.len() as u32).to_le_bytes());
        for f in &p.pos {
            b.extend(f.to_le_bytes());
        }
        for i in &p.idx {
            b.extend(i.to_le_bytes());
        }
    }
    b
}
