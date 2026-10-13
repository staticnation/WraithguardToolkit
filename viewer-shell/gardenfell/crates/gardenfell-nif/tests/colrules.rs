//! What the engine treats as solid, one rule per test.
//!
//! Every one of these decides whether a placed object clears grass out of the way
//! or is walked straight through, and each has been seen to go the wrong way on
//! real content: a beam lying in the grass with blades growing through it, or a
//! whole region stripped bare because a flag was read as a blanket "ignore this".

use gardenfell_nif::nif;

fn fixture(name: &str) -> nif::MeshGeom {
    let p = format!("{}/../../fixture/{}", env!("CARGO_MANIFEST_DIR"), name);
    let buf = std::fs::read(&p)
        .unwrap_or_else(|e| panic!("{}: {} — run `node mkcolmesh.js` in the page source tree", p, e));
    nif::parse(&buf).unwrap_or_else(|| panic!("{} did not parse", p))
}

fn height(g: &nif::MeshGeom) -> f32 {
    g.aabb_max[2] - g.aabb_min[2]
}

/// Collision hulls are normally authored with the hidden flag set — they exist to
/// be collided with, not drawn. Applying that flag inside the hull throws the hull
/// away and silently substitutes the visible mesh, which is a different shape.
#[test]
fn a_hidden_collision_hull_is_still_the_collision_hull() {
    let g = fixture("plank.nif");
    assert_eq!(g.shape, nif::ColShape::Hull, "the hull is marked hidden, and was dropped because of it");
    assert!(
        (height(&g) - 20.0).abs() < 0.01,
        "the shape tested against is {} tall; the hull is 20 and the visible slab is 12",
        height(&g)
    );
    assert!(g.radius > 100.0, "a 240-unit beam is not clutter, radius {}", g.radius);
}

/// A NiCollisionSwitch is not "the node that switches collision off". It collides
/// normally; only a clear propagate bit stops it — and then what is left is a thing
/// with no collision, which gets a stand-in box rather than being ignored.
#[test]
fn a_collision_switch_collides_unless_its_flag_says_otherwise() {
    let on = fixture("colswitch_on.nif");
    assert_eq!(on.shape, nif::ColShape::Visible);
    assert!((height(&on) - 200.0).abs() < 0.01, "propagating: the geometry is still solid");

    let off = fixture("colswitch_off.nif");
    assert_eq!(off.shape, nif::ColShape::Bounds, "nothing in it collides");
    assert!((height(&off) - 100.0).abs() < 0.01, "so it stands in a box half its size");
}

/// A particle node is an effect only when it is set to follow. A static one is
/// ordinary geometry and blocks like it.
#[test]
fn a_particle_node_is_only_an_effect_when_it_follows() {
    let still = fixture("particle_still.nif");
    assert_eq!(still.shape, nif::ColShape::Visible);
    assert!((height(&still) - 200.0).abs() < 0.01, "a still particle node is solid geometry");

    // A following particle node is not drawn as geometry either, so unlike a
    // non-propagating collision switch there is nothing left to stand a box around.
    let p = format!("{}/../../fixture/particle_move.nif", env!("CARGO_MANIFEST_DIR"));
    assert!(
        nif::parse(&std::fs::read(p).unwrap()).is_none(),
        "a following one is an effect, with no shape at all"
    );
}

/// One child of a switch node is live at a time. Taking them all would put the
/// dead alternative's geometry into the world alongside the live one.
#[test]
fn only_the_live_child_of_a_switch_node_counts() {
    let g = fixture("switchnode.nif");
    // Live child: 60 tall. Dead child: 800 tall. Tail shape outside the switch: 100.
    assert!(
        (height(&g) - 100.0).abs() < 0.01,
        "expected the live child and the tail, got a {}-unit shape",
        height(&g)
    );
    assert!(g.aabb_max[0] < 60.0, "the dead child is 800 wide and must not be in the bounds");
}

/// A level-of-detail node holds the same thing at several detail levels. Level 0 is the
/// nearest and so the most detailed, which is the one to draw; taking them all stacks a
/// tree on top of itself, and taking the wrong one draws it at the wrong fidelity.
///
/// This is the record that shipped broken. `NiLODNode` was not in the reader's list at
/// all, so it fell through to the resync, which scanned past the node *and its children*
/// — and since every visible shape in a West Gash tree hangs off its LOD node, the tree
/// drew nothing at all. Its collision hull is a sibling of the LOD node rather than a
/// child, so avoidance went on working perfectly and nothing anywhere said a word.
#[test]
fn only_the_nearest_level_of_a_lod_node_counts() {
    let g = fixture("lodnode.nif");
    // Level 0: 60 tall. Level 1: 800 tall. Tail shape outside the LOD node: 100.
    assert!(
        (height(&g) - 100.0).abs() < 0.01,
        "expected the near level and the tail, got a {}-unit shape",
        height(&g)
    );
    assert!(g.aabb_max[0] < 60.0, "the far level is 800 wide and must not be in the bounds");
}

/// Records in this format carry no length, so a node read one field short does not
/// spoil that node — it spoils the whole rest of the file, because the next type
/// name is then read out of the middle of it. Round 18gh: the typed model reads each
/// of these nodes at its length or refuses the file, so the file reading to its end is
/// what `scanned == 0` used to say.
#[test]
fn nodes_with_trailing_fields_do_not_desync_the_file() {
    for name in ["sortadjust.nif", "switchnode.nif", "lodnode.nif"] {
        let p = format!("{}/../../fixture/{}", env!("CARGO_MANIFEST_DIR"), name);
        let buf = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {}", p, e));
        assert!(
            gardenfell_nif::netimmerse::stream::Stream::parse(&buf).is_ok(),
            "{} did not read to its end",
            name
        );
        let g = fixture(name);
        assert!(!g.tris.is_empty(), "{} lost its geometry", name);
    }
}
