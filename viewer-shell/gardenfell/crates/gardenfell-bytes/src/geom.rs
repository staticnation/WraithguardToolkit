//! Geometry shared by the scatter, the obstacle walk, the brush and the canopies: the
//! deterministic RNG every placement draws from, the 3x4 affine matrices a Morrowind
//! reference is placed with, and the ray-triangle test.
//!
//! Round 18ge: these lived in `poisson.rs` and were what tied `mesh_scatter`, `canopy`
//! and `terrain` back to the scatter. Nothing here knows about a world, a rule or a
//! cell; it is arithmetic, and it sits under everything that uses it.

// ---------------------------------------------------------------------------
// Deterministic RNG — same stream for the same seed on every machine.
// ---------------------------------------------------------------------------

pub struct Rng(u64);

impl Rng {
    #[inline]
    pub fn new(seed: u64) -> Rng {
        // SplitMix64 finaliser gives a well-distributed starting state even for
        // adjacent seeds, which matters because cells seed from their coords.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Rng(z ^ (z >> 31))
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    #[inline]
    pub fn f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / 16_777_216.0
    }
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}


/// Morrowind reference rotation: the three angles negated, composed X * Y * Z, so
/// Z is applied first and X last.
///
/// Order only shows when more than one angle is non-zero, which is why almost
/// everything looked right with the axes the other way round: buildings and trees
/// are placed with yaw alone. Rocks are tumbled about all three, and those came out
/// at visibly wrong angles — and their obstacle shapes with them.
pub fn ref_matrix(pos: [f32; 3], rot: [f32; 3], scale: f32) -> [f32; 12] {
    let (sx, cx) = (-rot[0]).sin_cos();
    let (sy, cy) = (-rot[1]).sin_cos();
    let (sz, cz) = (-rot[2]).sin_cos();
    let mut m = [
        cy * cz,
        -cy * sz,
        sy,
        cx * sz + sx * sy * cz,
        cx * cz - sx * sy * sz,
        -sx * cy,
        sx * sz - cx * sy * cz,
        sx * cz + cx * sy * sz,
        cx * cy,
        pos[0],
        pos[1],
        pos[2],
    ];
    for v in m.iter_mut().take(9) {
        *v *= scale;
    }
    m
}

/// The rotation that stands an object up along a ground normal.
fn align_matrix(n: [f32; 3]) -> [f32; 9] {
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l < 1e-9 {
        return [1., 0., 0., 0., 1., 0., 0., 0., 1.];
    }
    let (x, y, z) = (n[0] / l, n[1] / l, n[2] / l);
    // cross([0,0,1], n), whose z component is always zero
    let (ax, ay) = (-y, x);
    let al = (ax * ax + ay * ay).sqrt();
    if al < 1e-6 {
        return if z >= 0.0 {
            [1., 0., 0., 0., 1., 0., 0., 0., 1.]
        } else {
            [1., 0., 0., 0., -1., 0., 0., 0., -1.]
        };
    }
    let (ux, uy) = (ax / al, ay / al);
    let ang = z.clamp(-1.0, 1.0).acos();
    let (s, c) = ang.sin_cos();
    let ic = 1.0 - c;
    [
        c + ux * ux * ic,
        ux * uy * ic,
        uy * s,
        ux * uy * ic,
        c + uy * uy * ic,
        -ux * s,
        -uy * s,
        ux * s,
        c,
    ]
}

/// The three reference angles for an object standing on `normal` and spun by `yaw`.
///
/// The preview tilts each blade onto the ground normal as it draws it, but a plugin
/// has nowhere to put that except these three numbers, which are read back through
/// `ref_matrix`. So: build the rotation the preview draws, then decompose it exactly
/// the way `ref_matrix` composes it.
pub fn ref_angles(normal: Option<[f32; 3]>, yaw: f32) -> [f32; 3] {
    let (sz, cz) = yaw.sin_cos();
    let rz = [cz, -sz, 0., sz, cz, 0., 0., 0., 1.];
    let m = match normal {
        None => rz,
        Some(n) => {
            let a = align_matrix(n);
            let mut m = [0f32; 9];
            for r in 0..3 {
                for c in 0..3 {
                    m[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * rz[k * 3 + c]).sum();
                }
            }
            m
        }
    };
    // Invert the composition ref_matrix performs: m = Rx(-rx) Ry(-ry) Rz(-rz).
    let b = m[2].clamp(-1.0, 1.0).asin();
    let (a, c) = if b.cos().abs() > 1e-6 {
        ((-m[5]).atan2(m[8]), (-m[1]).atan2(m[0]))
    } else {
        // Straight along Y: yaw and roll are the same turn, so it all goes in one.
        (m[7].atan2(m[4]), 0.0)
    };
    [-a, -b, -c]
}

/// Inverts an affine transform whose linear part is a scaled rotation.
pub fn invert(m: &[f32; 12]) -> [f32; 12] {
    let s2: f32 = (0..3).map(|k| m[k] * m[k]).sum();
    let inv_s2 = if s2 > 1e-12 { 1.0 / s2 } else { 0.0 };
    let mut out = [0f32; 12];
    for r in 0..3 {
        for c in 0..3 {
            out[r * 3 + c] = m[c * 3 + r] * inv_s2; // transpose / s^2
        }
    }
    for r in 0..3 {
        out[9 + r] = -(0..3).map(|k| out[r * 3 + k] * m[9 + k]).sum::<f32>();
    }
    out
}

#[inline]
pub fn xform(m: &[f32; 12], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[9],
        m[3] * p[0] + m[4] * p[1] + m[5] * p[2] + m[10],
        m[6] * p[0] + m[7] * p[1] + m[8] * p[2] + m[11],
    ]
}

/// Moller-Trumbore. Returns the ray parameter at the hit, or -1 for a miss.
#[inline]
pub fn ray_tri(o: [f32; 3], d: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let p = [
        d[1] * e2[2] - d[2] * e2[1],
        d[2] * e2[0] - d[0] * e2[2],
        d[0] * e2[1] - d[1] * e2[0],
    ];
    let det = e1[0] * p[0] + e1[1] * p[1] + e1[2] * p[2];
    if det > -1e-9 && det < 1e-9 {
        return -1.0; // the ray runs along the face
    }
    let inv = 1.0 / det;
    let t = [o[0] - a[0], o[1] - a[1], o[2] - a[2]];
    let u = (t[0] * p[0] + t[1] * p[1] + t[2] * p[2]) * inv;
    if !(0.0..=1.0).contains(&u) {
        return -1.0;
    }
    let q = [
        t[1] * e1[2] - t[2] * e1[1],
        t[2] * e1[0] - t[0] * e1[2],
        t[0] * e1[1] - t[1] * e1[0],
    ];
    let v = (d[0] * q[0] + d[1] * q[1] + d[2] * q[2]) * inv;
    if v < 0.0 || u + v > 1.0 {
        return -1.0;
    }
    (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2]) * inv
}
