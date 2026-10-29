//! `draw_from_geom`'s flat face normals (patch 0019: `glam::Vec3`) against the hand-written
//! lines they replaced, bit for bit, and the properties of the result.

use gardenfell_nif::nif::{MeshGeom, draw_from_geom};
use std::sync::Arc;

/// The face normal as the code computed it before glam.
fn reference(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    [n[0] / len, n[1] / len, n[2] / len]
}

fn lcg(s: &mut u64) -> f32 {
    *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*s >> 33) as u32) as f32 / (1u32 << 31) as f32 * 2000.0 - 1000.0
}

fn geom(tris: Vec<[f32; 3]>) -> MeshGeom {
    MeshGeom {
        tris: Arc::new(tris),
        aabb_min: [0.0; 3],
        aabb_max: [0.0; 3],
        shape: Default::default(),
        radius: 0.0,
        accel: None,
    }
}

#[test]
fn flat_normals_are_the_reference_bit_for_bit() {
    let mut s = 3u64;
    let mut tris = Vec::new();
    for _ in 0..3000 {
        for _ in 0..3 {
            tris.push([lcg(&mut s), lcg(&mut s), lcg(&mut s)]);
        }
    }
    // a degenerate triangle (a point) and a sliver: the 1e-6 floor on the length
    tris.extend([[1.0, 2.0, 3.0]; 3]);
    tris.extend([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
    let parts = draw_from_geom(&geom(tris.clone()), "t");
    assert_eq!(parts.len(), 1);
    let p = &parts[0];
    assert_eq!(p.nrm.len(), tris.len() * 3);
    for (i, t) in tris.chunks_exact(3).enumerate() {
        let want = reference(t[0], t[1], t[2]);
        for corner in 0..3 {
            let at = (i * 3 + corner) * 3;
            for k in 0..3 {
                assert_eq!(p.nrm[at + k].to_bits(), want[k].to_bits(), "triangle {i} corner {corner} component {k}");
            }
            assert_eq!(&p.pos[at..at + 3], &t[corner], "position");
        }
    }
}
