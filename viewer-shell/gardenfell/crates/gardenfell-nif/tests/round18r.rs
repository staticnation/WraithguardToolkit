//! Round 18r: two NIF records the reader could not measure, and the mesh they hid.
//!
//! `MWSE - Nocturnal Moths` ships `meshes\R0\e\moths_lntrn.NIF`, and Gardenfell read it as
//! a mesh with no geometry at all - so the moths round 18q added never appeared, at any
//! hour, on any lamp. Robin: "Moths are not visible in the scene no matter the time of
//! day."
//!
//! Neither record was the moths' own idea; both are ordinary Morrowind:
//!
//! - **`NiGeomMorpherController` is 31 bytes, not 30.** The time controller's 26, the data
//!   reference, and a trailing byte - nif.xml's `Always Update`, which it dates later than
//!   4.0.0.2 and which this 4.0.0.2 file plainly carries. Reading 30 left the parse one
//!   byte short of the next record's type name, and it stopped there: every record after
//!   it was lost, the shape data with them.
//! - **`NiMorphData` had no length in the reader at all**, so it fell through to the
//!   scan-for-the-next-type-name recovery, which is a guess.
//!
//! `fixture/morph.nif` (see `page-source/mkmorph.js`) is the smallest file with both, in
//! the mod's own nesting: an animation node, a level-of-detail node, a collision switch,
//! and two morph-animated shapes under them.

use std::path::PathBuf;

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixture").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn a_morph_animated_mesh_keeps_its_geometry() {
    let buf = fixture("morph.nif");
    let parts = gardenfell_nif::nif::parse_draw(&buf).expect("the mesh reads at all");
    assert_eq!(parts.len(), 2, "both moths, not one and not none");
    /* Where they are matters as much as that they are there: a reader that lost its place
    could still hand back a shape built from the wrong record. */
    let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Moth1") && names.contains(&"Moth2"), "{names:?}");
    for p in &parts {
        assert_eq!(p.idx.len(), 6, "two triangles, as three indices each");
        assert_eq!(p.pos.len(), 12, "four vertices");
    }
    /* And each one flies its own path. A shape under an animated node is baked in *that
    node's* frame - the geometry sits at the origin and the node's resting transform
    travels in the link, which is what the page composes per frame - so the ten units
    either side of the origin are in the link, not in the vertices. Both halves are
    checked: the geometry centred, and the two links a good way apart. Round 18r retimes
    exactly these links per lantern, so a fixture without them would not stand in for
    the mod's file at all. */
    let mid = |p: &gardenfell_nif::nif::DrawPart, k: usize| -> f32 {
        p.pos.iter().skip(k).step_by(3).sum::<f32>() / (p.pos.len() as f32 / 3.0)
    };
    for p in &parts {
        assert!(mid(p, 0).abs() < 0.01 && mid(p, 1).abs() < 0.01, "baked off its own node");
        let a = p.anim.as_ref().expect("the moth's keyframe controller");
        assert_eq!(a.nodes.len(), 1, "one animated node above the shape");
        assert!(a.nodes[0].ctrl.stop > 1.0, "a clip with a length: {}", a.nodes[0].ctrl.stop);
    }
    let mut xs: Vec<f32> = parts
        .iter()
        // `[3x3 row-major, translation]`, so x is index 9.
        .map(|p| p.anim.as_ref().unwrap().nodes[0].local.0[9])
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((xs[0] + 10.0).abs() < 0.01 && (xs[1] - 10.0).abs() < 0.01, "{xs:?}");
}

#[test]
fn every_record_is_measured_rather_than_scanned() {
    /* The whole file, read exactly: a reader that guessed its way past either record could
    still arrive at two shapes by luck. `scanned` was the count of records the byte
    reader could not measure; round 18gh reads through the typed model, which measures
    every record or refuses the file, so the file reading to its end, with the morph
    controller and its data both listed, is the claim. */
    let buf = fixture("morph.nif");
    let _g = gardenfell_nif::nif::parse(&buf).expect("the collision walk reads it too");
    assert!(gardenfell_nif::netimmerse::stream::Stream::parse(&buf).is_ok(), "the file reads to its end");
    let types = gardenfell_nif::nif::record_types(&buf).expect("a NIF");
    assert!(types.iter().any(|t| t == "NiGeomMorpherController"), "{types:?}");
    assert!(types.iter().any(|t| t == "NiMorphData"), "{types:?}");
}

#[test]
fn the_node_tree_reaches_both_shapes_through_all_four_wrappers() {
    let buf = fixture("morph.nif");
    let tree = gardenfell_nif::nif::node_tree(&buf).join("\n");
    for want in [
        "MothsAnimNode",
        "MothsLODNode",
        "MothsCollisionSwitchNode",
        "MothsRootNode",
        "MothNode1",
        "Moth1",
        "MothNode2",
        "Moth2",
    ] {
        assert!(tree.contains(want), "missing {want} in\n{tree}");
    }
}
