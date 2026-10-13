//! Skinned meshes — round 12 item 3.
//!
//! Robin: "Most flags are rotated wrong. They should be vertical but are horizontal."
//! Every vanilla banner is a skinned NiTriShape: its vertices are stored in skin space
//! and placed by the bones. `skinned.nif` (`gf/mkskin.js`) is the smallest such mesh —
//! a flat quad held by one bone turned 90 degrees about x and raised 50 units — so the
//! right answer is known exactly: the quad stands up between z = 40 and z = 60.

use gardenfell_nif::nif::{parse, parse_draw};
use std::path::PathBuf;

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixture").join(name);
    std::fs::read(&p).unwrap_or_else(|_| panic!("{} — run `node mkskin.js` in gf/", p.display()))
}

fn span(pos: &[f32], k: usize) -> (f32, f32) {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for v in pos.chunks(3) {
        lo = lo.min(v[k]);
        hi = hi.max(v[k]);
    }
    (lo, hi)
}

#[test]
fn a_skinned_shape_is_drawn_where_its_bones_hold_it() {
    let parts = parse_draw(&fixture("skinned.nif")).expect("parses");
    assert_eq!(parts.len(), 1);
    let p = &parts[0];
    let (x0, x1) = span(&p.pos, 0);
    let (y0, y1) = span(&p.pos, 1);
    let (z0, z1) = span(&p.pos, 2);
    assert!((x0 + 10.0).abs() < 1e-3 && (x1 - 10.0).abs() < 1e-3, "x untouched: {x0}..{x1}");
    assert!(y0.abs() < 1e-3 && y1.abs() < 1e-3, "flat in y once it stands up: {y0}..{y1}");
    assert!((z0 - 40.0).abs() < 1e-3 && (z1 - 60.0).abs() < 1e-3, "standing between 40 and 60: {z0}..{z1}");
    // The normal turned with it: it pointed +z, it points -y now.
    let n = &p.nrm[..3];
    assert!(n[0].abs() < 1e-3 && (n[1] + 1.0).abs() < 1e-3 && n[2].abs() < 1e-3, "normal {n:?}");
}

#[test]
fn the_skins_own_overall_transform_is_ignored_as_openmw_ignores_it() {
    /* Round 17h. `NiSkinData` carries an overall transform before the per-bone ones;
       the fixture states (7, 0, -13). OpenMW's RigGeometry builds its bone matrices from
       `boneWorld * bones[i].trafo` alone and never reads it, and the game agrees: Project
       Atlas's Vivec banner states (32, 0, -128) there and hangs level with the vanilla
       one in both. So the quad stands exactly where the bone holds it, unmoved. */
    let parts = parse_draw(&fixture("skinned.nif")).expect("parses");
    let (z0, z1) = span(&parts[0].pos, 2);
    let (x0, x1) = span(&parts[0].pos, 0);
    assert!((z0 - 40.0).abs() < 1e-3 && (z1 - 60.0).abs() < 1e-3, "not dropped by the skin transform: {z0}..{z1}");
    assert!((x0 + 10.0).abs() < 1e-3 && (x1 - 10.0).abs() < 1e-3, "nor shifted along x: {x0}..{x1}");
}

#[test]
fn the_obstacle_walk_sees_the_same_standing_shape() {
    /* Robin's choice: both drawing and avoidance, so the grass keeps clear of where the
       flag actually hangs. No collision node in the fixture, so the visible geometry is
       what the box is stood on. */
    let g = parse(&fixture("skinned.nif")).expect("geometry");
    assert!((g.aabb_min[2] - 40.0).abs() < 1.0 && (g.aabb_max[2] - 60.0).abs() < 1.0, "z {:?}..{:?}", g.aabb_min, g.aabb_max);
    assert!(g.aabb_min[1].abs() < 1.0 && g.aabb_max[1].abs() < 1.0, "y {:?}..{:?}", g.aabb_min, g.aabb_max);
}

#[test]
fn a_real_banner_stands_up_when_the_archive_is_at_hand() {
    /* The vanilla banner, when `GARDENFELL_BSA` names Morrowind.bsa: 80 wide, 128
       tall, no depth — a sheet hanging from its pole, not lying on the ground. */
    let Ok(p) = std::env::var("GARDENFELL_BSA") else { return };
    let a = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)).expect("opens");
    let Some(b) = a.get("meshes/f/furn_banner_hlaalu_01.nif") else { return };
    let parts = parse_draw(b).expect("parses");
    let (x0, x1) = span(&parts[0].pos, 0);
    let (y0, y1) = span(&parts[0].pos, 1);
    let (z0, z1) = span(&parts[0].pos, 2);
    assert!((x1 - x0 - 80.0).abs() < 1.0, "width {}", x1 - x0);
    assert!((y1 - y0).abs() < 1.0, "a sheet: depth {}", y1 - y0);
    assert!((z1 - z0 - 128.0).abs() < 1.0, "height {}", z1 - z0);
}
