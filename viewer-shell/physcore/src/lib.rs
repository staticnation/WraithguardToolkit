//! Wraithguard: physics for placing objects - the Editor's Physics drop.
//!
//! The objects being placed fall, tumble and come to rest on the scene as OpenMW collides
//! with it: every other object's collision shape (its hull, or its visible mesh where it
//! has none) and the land, as triangle meshes. What falls is the convex hull of its own
//! collision shape. OpenMW itself never moves objects with physics (it collides, it does
//! not simulate them), so this is an editor's tool, as the Construction Kit's drop is: the
//! surfaces are the game's, the settling is Rapier's (dimforge, Apache-2.0).
//!
//! Units are the game's: about 70 to the meter, so gravity is 9.8 m/s squared as 690
//! units, and Rapier is told its lengths are that many to the meter.
//!
//! Rotations are Morrowind's `rot` (x, y, z radians) as the viewer turns them into a
//! matrix (`refMatrix` in `ui/src/18_cellpreview.js`): [`matrix`] and [`angles`] are that
//! and its inverse.

use rapier3d::glamx::{Mat3, Quat, Vec3};
use rapier3d::prelude::*;

/// Morrowind units to the meter (1 unit = 1.4224 cm).
pub const UNITS_PER_METER: f32 = 70.3;

/// How the drop runs.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Units per second squared, downward.
    pub gravity: f32,
    pub friction: f32,
    /// Bounce, 0 to 1.
    pub restitution: f32,
    /// The longest it runs, in seconds.
    pub max_seconds: f32,
    /// Only falling: no tumbling (rotations locked).
    pub drop_only: bool,
    /// Frames kept for showing the fall, per second.
    pub fps: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { gravity: 9.81 * UNITS_PER_METER, friction: 0.7, restitution: 0.05, max_seconds: 6.0, drop_only: false, fps: 30.0 }
    }
}

/// Something that does not move: triangles in world units, three vertices each.
pub struct Static {
    pub tris: Vec<[f32; 9]>,
}

/// Something that falls: its collision's vertices in its own units, where it stands
/// (`pos`), its `rot` and its scale.
#[derive(Clone, Debug)]
pub struct Body {
    pub key: String,
    pub points: Vec<[f32; 3]>,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: f32,
}

/// Where each body is at one moment: `(pos, rot)` in the order the bodies were given.
pub type Frame = Vec<([f32; 3], [f32; 3])>;

/// What the drop came to.
#[derive(Debug)]
pub struct Outcome {
    /// The fall, `fps` frames a second, the first where they started.
    pub frames: Vec<Frame>,
    /// Seconds simulated.
    pub seconds: f32,
    /// Every body came to rest before the time ran out.
    pub settled: bool,
    /// Bodies whose shape could not be made (fewer than three vertices, or all on a line):
    /// left where they were.
    pub skipped: Vec<String>,
}

/// Morrowind's rotation as the viewer builds it (rows), unscaled.
pub fn matrix(rot: [f32; 3]) -> Mat3 {
    let (cx, sx) = ((-rot[0]).cos(), (-rot[0]).sin());
    let (cy, sy) = ((-rot[1]).cos(), (-rot[1]).sin());
    let (cz, sz) = ((-rot[2]).cos(), (-rot[2]).sin());
    // glam's matrices are by columns: these are the viewer's rows, transposed into place.
    Mat3::from_cols(
        Vec3::new(cy * cz, cx * sz + sx * sy * cz, sx * sz - cx * sy * cz),
        Vec3::new(-cy * sz, cx * cz - sx * sy * sz, sx * cz + cx * sy * sz),
        Vec3::new(sy, -sx * cy, cx * cy),
    )
}

/// A rotation from rows (the viewer's 3x3, as its 3x4 matrices hold it).
pub fn glam_mat3_from_rows(r: [[f32; 3]; 3]) -> Mat3 {
    Mat3::from_cols(Vec3::new(r[0][0], r[1][0], r[2][0]), Vec3::new(r[0][1], r[1][1], r[2][1]), Vec3::new(r[0][2], r[1][2], r[2][2]))
}

/// The `rot` a rotation matrix is ([`matrix`]'s inverse).
pub fn angles(m: Mat3) -> [f32; 3] {
    // Rows of the viewer's matrix: m02 = sin(y'), m01 = -cy sz, m00 = cy cz, m12 = -sx cy,
    // m22 = cx cy, with x' = -rot.x and so on.
    let (m00, m01, m02) = (m.x_axis.x, m.y_axis.x, m.z_axis.x);
    let (m12, m22) = (m.z_axis.y, m.z_axis.z);
    let yp = m02.clamp(-1.0, 1.0).asin();
    let (xp, zp) = if m02.abs() < 0.99999 {
        ((-m12).atan2(m22), (-m01).atan2(m00))
    } else {
        // Straight up or down: one angle stands for both.
        let (m10, m11) = (m.x_axis.y, m.y_axis.y);
        (0.0, m10.atan2(m11))
    };
    [-xp, -yp, -zp]
}

/// Drops the bodies onto the statics, until they rest or the time runs out.
pub fn drop(statics: &[Static], bodies: &[Body], s: &Settings) -> Outcome {
    let mut rbs = RigidBodySet::new();
    let mut cols = ColliderSet::new();
    for st in statics {
        if st.tris.is_empty() {
            continue;
        }
        let mut verts = Vec::with_capacity(st.tris.len() * 3);
        let mut idx = Vec::with_capacity(st.tris.len());
        for t in &st.tris {
            let i = verts.len() as u32;
            verts.push(Vec3::new(t[0], t[1], t[2]));
            verts.push(Vec3::new(t[3], t[4], t[5]));
            verts.push(Vec3::new(t[6], t[7], t[8]));
            idx.push([i, i + 1, i + 2]);
        }
        if let Ok(c) = ColliderBuilder::trimesh_with_flags(verts, idx, TriMeshFlags::FIX_INTERNAL_EDGES | TriMeshFlags::MERGE_DUPLICATE_VERTICES) {
            cols.insert(c.friction(s.friction).restitution(s.restitution));
        }
    }
    let mut handles = Vec::new();
    let mut skipped = Vec::new();
    for b in bodies {
        let mut pts: Vec<Vec3> = b.points.iter().map(|p| Vec3::new(p[0], p[1], p[2]) * b.scale).collect();
        // A flat shape (a page, a rug) is given a thin slab, so it has a hull to lie on.
        if pts.len() >= 3 {
            let (lo, hi) = pts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
            let ext = hi - lo;
            let k = if ext.x <= ext.y && ext.x <= ext.z { 0 } else if ext.y <= ext.z { 1 } else { 2 };
            // Thick enough for the hull to have a volume (and so a mass): a hundredth of
            // its size, half on each side, so it still lies where it was drawn.
            let t = (ext.max_element() * 0.01).max(0.5);
            if ext[k] < t {
                let mut off = Vec3::ZERO;
                off[k] = (t - ext[k]) * 0.5;
                let more: Vec<Vec3> = pts.iter().map(|p| *p + off).collect();
                for p in &mut pts {
                    *p -= off;
                }
                pts.extend(more);
            }
        }
        let Some(shape) = (pts.len() >= 4).then(|| ColliderBuilder::convex_hull(&pts)).flatten() else {
            skipped.push(b.key.clone());
            handles.push(None);
            continue;
        };
        let rot = Quat::from_mat3(&matrix(b.rot)).normalize();
        let mut rb = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(Vec3::new(b.pos[0], b.pos[1], b.pos[2]), rot))
            .ccd_enabled(true)
            .linear_damping(0.05)
            .angular_damping(0.2);
        if s.drop_only {
            rb = rb.lock_rotations();
        }
        let h = rbs.insert(rb);
        cols.insert_with_parent(shape.friction(s.friction).restitution(s.restitution).density(1.0), h, &mut rbs);
        handles.push(Some(h));
    }

    let mut params = IntegrationParameters { length_unit: UNITS_PER_METER, ..Default::default() };
    params.dt = 1.0 / 60.0;
    let gravity = Vector::new(0.0, 0.0, -s.gravity.abs());
    let mut pipeline = PhysicsPipeline::new();
    let mut islands = IslandManager::new();
    let mut broad = BroadPhaseBvh::new();
    let mut narrow = NarrowPhase::new();
    let mut joints = ImpulseJointSet::new();
    let mut multi = MultibodyJointSet::new();
    let mut soft = SoftBodySet::new();
    let mut ccd = CCDSolver::new();

    let pose_of = |rbs: &RigidBodySet| -> Frame {
        bodies
            .iter()
            .zip(&handles)
            .map(|(b, h)| match h {
                Some(h) => {
                    let r = &rbs[*h];
                    let t = r.translation();
                    ([t.x, t.y, t.z], angles(Mat3::from_quat(*r.rotation())))
                }
                None => (b.pos, b.rot),
            })
            .collect()
    };
    let mut frames = vec![pose_of(&rbs)];
    let steps_per_frame = ((1.0 / s.fps.max(1.0)) / params.dt).round().max(1.0) as usize;
    let max_steps = (s.max_seconds.max(0.1) / params.dt).ceil() as usize;
    let mut settled = false;
    let mut step = 0usize;
    while step < max_steps {
        pipeline.step(gravity, &params, &mut islands, &mut broad, &mut narrow, &mut rbs, &mut cols, &mut joints, &mut multi, &mut soft, &mut ccd, &(), &());
        step += 1;
        if step.is_multiple_of(steps_per_frame) {
            frames.push(pose_of(&rbs));
        }
        // At rest: every body asleep (Rapier puts a still body to sleep).
        if step > 10 && handles.iter().flatten().all(|h| rbs[*h].is_sleeping()) {
            settled = true;
            if !step.is_multiple_of(steps_per_frame) {
                frames.push(pose_of(&rbs));
            }
            break;
        }
    }
    Outcome { frames, seconds: step as f32 * params.dt, settled, skipped }
}

/// The land of an exterior square as triangles: its 65 x 65 heights (rows from the south,
/// 128 units apart), each quad cut from (x+1, y) to (x, y+1) as the game cuts it.
pub fn land_tris(grid: (i32, i32), heights: &[f32]) -> Vec<[f32; 9]> {
    const N: usize = 65;
    const STEP: f32 = 128.0;
    let (ox, oy) = (grid.0 as f32 * 8192.0, grid.1 as f32 * 8192.0);
    let h = |x: usize, y: usize| heights.get(y * N + x).copied().unwrap_or(-2048.0);
    let p = |x: usize, y: usize| [ox + x as f32 * STEP, oy + y as f32 * STEP, h(x, y)];
    let mut out = Vec::with_capacity((N - 1) * (N - 1) * 2);
    for y in 0..N - 1 {
        for x in 0..N - 1 {
            let (a, b, c, d) = (p(x, y), p(x + 1, y), p(x, y + 1), p(x + 1, y + 1));
            out.push([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
            out.push([b[0], b[1], b[2], d[0], d[1], d[2], c[0], c[1], c[2]]);
        }
    }
    out
}

/// A square with no land: flat at -2048, as OpenMW has it.
pub fn flat_square(grid: (i32, i32)) -> Vec<[f32; 9]> {
    let (x0, y0) = (grid.0 as f32 * 8192.0, grid.1 as f32 * 8192.0);
    let (x1, y1, z) = (x0 + 8192.0, y0 + 8192.0, -2048.0);
    vec![[x0, y0, z, x1, y0, z, x0, y1, z], [x1, y0, z, x1, y1, z, x0, y1, z]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(half: f32) -> Vec<[f32; 3]> {
        let mut v = Vec::new();
        for x in [-half, half] {
            for y in [-half, half] {
                for z in [-half, half] {
                    v.push([x, y, z]);
                }
            }
        }
        v
    }

    #[test]
    fn angles_undo_the_viewer_s_matrix() {
        for rot in [[0.0, 0.0, 0.0], [0.3, -0.2, 1.1], [-1.2, 0.7, -2.9], [0.0, 0.0, 3.0]] {
            let back = angles(matrix(rot));
            let (a, b) = (matrix(rot), matrix(back));
            assert!((a - b).abs_diff_eq(Mat3::ZERO, 1e-4), "{rot:?} -> {back:?}");
        }
        // The viewer's own numbers: rot z of 90 degrees turns +x to -y (its m10 = cx*sz with z' = -z).
        let m = matrix([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        let x = m * Vec3::X;
        assert!((x - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-5, "{x:?}");
    }

    #[test]
    fn a_crate_falls_onto_the_floor_and_rests() {
        // A floor at z = 0 (two triangles, 2000 across) and a 32-unit crate 300 above it.
        let floor = Static { tris: vec![[-1000.0, -1000.0, 0.0, 1000.0, -1000.0, 0.0, -1000.0, 1000.0, 0.0], [1000.0, -1000.0, 0.0, 1000.0, 1000.0, 0.0, -1000.0, 1000.0, 0.0]] };
        let crate_ = Body { key: "c1".into(), points: cube(16.0), pos: [0.0, 0.0, 300.0], rot: [0.0, 0.0, 0.0], scale: 1.0 };
        let out = drop(&[floor], &[crate_], &Settings::default());
        let last = out.frames.last().unwrap()[0];
        assert!(out.settled, "{} s", out.seconds);
        assert!((last.0[2] - 16.0).abs() < 1.5, "rests on the floor: {:?}", last.0);
        assert!(out.frames.len() > 10, "the fall is kept to show: {}", out.frames.len());
    }

    #[test]
    fn a_tilted_crate_tumbles_flat_unless_only_dropped() {
        let floor = Static { tris: land_tris((0, 0), &vec![0.0; 65 * 65]) };
        let b = Body { key: "c1".into(), points: cube(16.0), pos: [4096.0, 4096.0, 200.0], rot: [0.5, 0.3, 0.0], scale: 2.0 };
        // Tumbling: it ends lying on a face (one axis straight up or down).
        let out = drop(std::slice::from_ref(&floor), std::slice::from_ref(&b), &Settings::default());
        let (pos, rot) = out.frames.last().unwrap()[0];
        let m = matrix(rot);
        let up = [m * Vec3::X, m * Vec3::Y, m * Vec3::Z].iter().map(|v| v.z.abs()).fold(0.0f32, f32::max);
        assert!(up > 0.99 && (pos[2] - 32.0).abs() < 3.0, "lies flat at its half height (scaled 2x): {pos:?} {rot:?} up {up}");
        // Only dropped: the turn it had is kept.
        let out = drop(&[floor], &[b], &Settings { drop_only: true, ..Default::default() });
        let (_, rot) = out.frames.last().unwrap()[0];
        assert!((rot[0] - 0.5).abs() < 1e-3 && (rot[1] - 0.3).abs() < 1e-3, "{rot:?}");
    }

    #[test]
    fn a_flat_shape_lies_down_and_a_line_is_left_where_it_is() {
        let floor = Static { tris: land_tris((0, 0), &vec![0.0; 65 * 65]) };
        let page = Body { key: "p".into(), points: vec![[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [0.0, 30.0, 0.0], [20.0, 30.0, 0.0]], pos: [4096.0, 4096.0, 50.0], rot: [0.0; 3], scale: 1.0 };
        let out = drop(std::slice::from_ref(&floor), &[page], &Settings::default());
        let z = out.frames.last().unwrap()[0].0[2];
        assert!(out.skipped.is_empty() && z < 2.0 && z > -0.5, "the page lies on the floor: {z}");
        let line = Body { key: "l".into(), points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], pos: [0.0, 0.0, 50.0], rot: [0.0; 3], scale: 1.0 };
        let out = drop(&[floor], &[line], &Settings { max_seconds: 0.2, ..Default::default() });
        assert_eq!(out.skipped, vec!["l".to_string()]);
        assert_eq!(out.frames.last().unwrap()[0].0, [0.0, 0.0, 50.0]);
    }

    #[test]
    fn a_tall_flat_shape_far_out_falls_too() {
        // A veil (one upright quad, 400 by 1600) far from the middle of the world, over a
        // square with no land: thickened enough to weigh something, it falls to -2048.
        let pts = vec![[0.0, -200.0, 0.0], [0.0, 200.0, 0.0], [0.0, 200.0, 1600.0], [0.0, -200.0, 0.0], [0.0, 200.0, 1600.0], [0.0, -200.0, 1600.0]];
        let b = Body { key: "v".into(), points: pts, pos: [81828.0, 77824.0, 1000.0], rot: [0.0; 3], scale: 1.0 };
        let floor = Static { tris: flat_square((9, 9)) };
        let out = drop(&[floor], &[b], &Settings::default());
        let z = out.frames.last().unwrap()[0].0[2];
        assert!(out.skipped.is_empty() && (z + 2048.0).abs() < 20.0, "on the ground: {z}");
    }
}
