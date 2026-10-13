//! How far a line segment is from a triangle.
//!
//! Clearance is a distance, not a bigger box. Grass is rejected when the blade's
//! column comes within the clearance of an object's actual surface, which means the
//! obstacle test has to be able to measure that distance rather than only ask
//! whether the column crosses a face. Inflating the bounding box instead does
//! nothing at all: the bounding box is only the broad phase, and the triangles
//! behind it still answer the same yes or no they always did.

/// Closest point on a triangle to `p`. Ericson's Voronoi-region form: work out
/// which feature of the triangle owns the point — a vertex, an edge, or the face —
/// and answer from that feature alone.
pub fn closest_on_tri(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let sub = |u: [f32; 3], v: [f32; 3]| [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
    let dot = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let add = |u: [f32; 3], v: [f32; 3], t: f32| [u[0] + v[0] * t, u[1] + v[1] * t, u[2] + v[2] * t];

    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let denom = d1 - d3;
        let v = if denom.abs() > 1e-20 { d1 / denom } else { 0.0 };
        return add(a, ab, v);
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let denom = d2 - d6;
        let w = if denom.abs() > 1e-20 { d2 / denom } else { 0.0 };
        return add(a, ac, w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let denom = (d4 - d3) + (d5 - d6);
        let w = if denom.abs() > 1e-20 { (d4 - d3) / denom } else { 0.0 };
        return add(b, sub(c, b), w);
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
}

/// Squared distance between two segments, both clamped to their ends.
pub fn seg_seg_dist2(p1: [f32; 3], q1: [f32; 3], p2: [f32; 3], q2: [f32; 3]) -> f32 {
    let sub = |u: [f32; 3], v: [f32; 3]| [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
    let dot = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    if a <= 1e-12 && e <= 1e-12 {
        return dot(r, r);
    }
    let (mut s, mut t) = (0.0f32, 0.0f32);
    if a <= 1e-12 {
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= 1e-12 {
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            s = if denom > 1e-12 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    let c1 = [p1[0] + d1[0] * s, p1[1] + d1[1] * s, p1[2] + d1[2] * s];
    let c2 = [p2[0] + d2[0] * t, p2[1] + d2[1] * t, p2[2] + d2[2] * t];
    let w = sub(c1, c2);
    dot(w, w)
}

/// Squared distance from a segment to a triangle, for segments that do not cross
/// it. The closest pair is either an end of the segment against the face, or one of
/// the three triangle edges against the segment; a crossing is the caller's job and
/// is already answered, more cheaply, by the intersection test.
pub fn seg_tri_dist2(p0: [f32; 3], p1: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let d2 = |u: [f32; 3], v: [f32; 3]| {
        let w = [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
        w[0] * w[0] + w[1] * w[1] + w[2] * w[2]
    };
    let mut best = d2(p0, closest_on_tri(p0, a, b, c));
    let e = d2(p1, closest_on_tri(p1, a, b, c));
    if e < best {
        best = e;
    }
    for (u, v) in [(a, b), (b, c), (c, a)] {
        let e = seg_seg_dist2(p0, p1, u, v);
        if e < best {
            best = e;
        }
    }
    best
}
