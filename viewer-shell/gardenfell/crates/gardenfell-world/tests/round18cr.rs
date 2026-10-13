//! Round 18cr: the collision meshes read beside the merge give the world the merge would
//! have given after — the same `geom`, the same counts, the same failure lines. Robin:
//! start-up "feels a bit slow", and the mesh read was a second of sixteen idle threads
//! waiting on one.

use gardenfell_install::vfs::Vfs;
use gardenfell_world::world::{LoadOpts, World};
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixture")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(name)).unwrap()
}

/// The same world twice: one read after the merge, one beside it.
fn both(vfs: &Vfs, order: &[PathBuf], want_land: bool) -> (World, World) {
    let after =
        World::load_with(order, vfs, want_land, &LoadOpts { overlap_geometry: false, ..Default::default() });
    let beside =
        World::load_with(order, vfs, want_land, &LoadOpts { overlap_geometry: true, ..Default::default() });
    (after, beside)
}

fn same_geometry(a: &World, b: &World) {
    let mut ka: Vec<&String> = a.geom.keys().collect();
    let mut kb: Vec<&String> = b.geom.keys().collect();
    ka.sort();
    kb.sort();
    assert_eq!(ka, kb, "the meshes with geometry");
    for k in ka {
        let (ga, gb) = (&a.geom[k], &b.geom[k]);
        assert_eq!(ga.tris.len(), gb.tris.len(), "{k}: triangle count");
        assert_eq!(ga.tris, gb.tris, "{k}: the triangles themselves");
        assert_eq!(ga.aabb_min, gb.aabb_min, "{k}: bounds");
        assert_eq!(ga.aabb_max, gb.aabb_max, "{k}: bounds");
        assert_eq!(ga.shape, gb.shape, "{k}: shape");
        assert_eq!(ga.radius, gb.radius, "{k}: radius");
        assert_eq!(ga.accel.is_some(), gb.accel.is_some(), "{k}: hierarchy");
    }
    let sa = &a.stats;
    let sb = &b.stats;
    assert_eq!(
        (sa.meshes, sa.mesh_wanted, sa.mesh_collision, sa.mesh_unresolved, sa.mesh_unparsed),
        (sb.meshes, sb.mesh_wanted, sb.mesh_collision, sb.mesh_unresolved, sb.mesh_unparsed),
        "the connect log's counts"
    );
    /* As sets: `used` is walked out of a HashMap of cells, so the lines' order was never
    the same twice even before this round. */
    let mut la = a.mesh_failures.clone();
    let mut lb = b.mesh_failures.clone();
    la.sort();
    lb.sort();
    assert_eq!(la, lb, "the failure lines");
    let mut fa: Vec<(&String, &u8)> = a.mesh_failed.iter().collect();
    let mut fb: Vec<(&String, &u8)> = b.mesh_failed.iter().collect();
    fa.sort();
    fb.sort();
    assert_eq!(fa, fb, "why each missing mesh is missing");
}

#[test]
fn the_fixture_install_loads_the_same_geometry_either_way() {
    let df = fixture_dir().join("Data Files");
    let mut vfs = Vfs::with_roots(&[("data".to_string(), df.clone())]);
    vfs.add_archives_named_by(&df.join("Morrowind.ini"));
    let order: Vec<PathBuf> = [
        "Morrowind.esm",
        "Tamriel_Data.esp",
        "Rocks_Redone.esp",
        "Tree_Replacer.esp",
        "Flora_Expanded.esp",
        "Town_Overhaul.esp",
        "Patch_Collection.esp",
        "Landmass_Fixes.esp",
        "Distant_Land_Fix.esp",
    ]
    .iter()
    .map(|n| df.join(n))
    .filter(|p| p.exists())
    .collect();
    assert!(order.len() >= 2, "the fixture install has its plugins");
    let (after, beside) = both(&vfs, &order, true);
    assert!(after.stats.mesh_wanted > 0, "the fixture places meshes");
    assert!(!after.geom.is_empty(), "and some of them read");
    same_geometry(&after, &beside);
    // And the rest of the world is untouched by where the read happened.
    assert_eq!(after.cells.len(), beside.cells.len());
    assert_eq!(after.stats.refs_merged, beside.stats.refs_merged);
    assert_eq!(after.models.len(), beside.models.len());
}

/// The twin path: an animated class whose merge picks the `x` file is read from the
/// superset (which named both), and a mesh nothing references is not read at all.
#[test]
fn twins_and_corpses_come_out_the_same_and_the_unreferenced_stay_unread() {
    let dir = std::env::temp_dir().join(format!("gdn-18cr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Meshes/x")).unwrap();
    std::fs::create_dir_all(dir.join("Meshes/f")).unwrap();
    let flap = fixture("flap.nif");
    std::fs::write(dir.join("Meshes/x/flap.nif"), &flap).unwrap();
    std::fs::write(dir.join("Meshes/x/xflap.nif"), &flap).unwrap();
    std::fs::write(dir.join("Meshes/x/xflap.kf"), fixture("flap.kf")).unwrap();
    std::fs::write(dir.join("Meshes/f/plain.nif"), &flap).unwrap();
    std::fs::write(dir.join("Meshes/x/flapshop.nif"), &flap).unwrap();
    std::fs::write(dir.join("Meshes/x/xflapshop.nif"), &flap).unwrap();
    std::fs::write(dir.join("Meshes/x/xflapshop.kf"), fixture("flapshop.kf")).unwrap();
    std::fs::write(dir.join("top.nif"), &flap).unwrap();
    std::fs::write(dir.join("xtop.nif"), &flap).unwrap();
    let vfs = Vfs::with_roots(&[("data".to_string(), dir.clone())]);
    let esm = fixture_dir().join("Lamp.esm");
    let (after, beside) = both(&vfs, &[esm], false);
    assert!(after.models.values().any(|m| m.starts_with("x\\x")), "the fixture has a twin in play");
    same_geometry(&after, &beside);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The superset is a superset: everything the merged world wants was named before the
/// merge, so the read after it has nothing left to do — which is the whole saving.
#[test]
fn nothing_is_left_to_read_after_the_merge() {
    let df = fixture_dir().join("Data Files");
    let mut vfs = Vfs::with_roots(&[("data".to_string(), df.clone())]);
    vfs.add_archives_named_by(&df.join("Morrowind.ini"));
    let order = vec![df.join("Morrowind.esm")];
    let w = World::load_with(&order, &vfs, true, &LoadOpts::default());
    let w2 =
        World::load_with(&order, &vfs, true, &LoadOpts { overlap_geometry: false, ..Default::default() });
    assert!(w.stats.meshes == w2.stats.meshes && w.stats.meshes > 0);
    // `mesh_late` counts what `load_geometry` had to read itself after the merge.
    assert_eq!(w.stats.mesh_late, 0, "after the overlap the mesh step read nothing itself");
    assert!(
        w2.stats.mesh_late > 0 && w2.stats.mesh_late <= w2.stats.mesh_wanted,
        "without it, all of it: {}",
        w2.stats.mesh_late
    );
}
