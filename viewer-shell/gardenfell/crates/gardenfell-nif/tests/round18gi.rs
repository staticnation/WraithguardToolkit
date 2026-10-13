//! Round 18gi: a level-of-detail node shows the level the preview's distance falls in.
//!
//! Robin, of `light_sconce10.NIF` in his install: "a large billboarded particle that is
//! static in front of the lamp. It shouldn't be there." and, with the particles turned
//! off and the sprite still there, "It might be a lod step to simulate fire for distant
//! land (very far away). So up close in the game it wouldn't render". It is: Improved
//! Lights for All Shaders replaces the sconce with one that hangs a billboarded fire
//! sprite under a NiLODNode whose one level is 2500 to 4000 units, and Memento Mori's
//! `gg_light_sconce10.nif` is built the same way. The walks took a LOD node's child 0 as
//! "the nearest level", which for a far-only node is the far one. Now the levels' ranges
//! are read and the children whose range holds distance 0 are the ones shown.

use gardenfell_nif::netimmerse::stream::{Link, Stream};
use gardenfell_nif::netimmerse::types::*;
use gardenfell_nif::nif;

/// A one-triangle shape at `z`, as a (shape, data) pair of blocks.
fn shape_at(z: f32, data: i32) -> (NiType, NiType) {
    let mut shape = NiTriShape::default();
    shape.base.base.base.base.name = format!("shape at {z}");
    shape.base.base.geometry_data = Link::new(data);
    let mut d = NiTriShapeData::default();
    d.base.base.vertices = vec![[0.0, 0.0, z], [10.0, 0.0, z], [0.0, 10.0, z]];
    d.base.base.normals = vec![[0.0, 0.0, 1.0]; 3];
    d.triangles = vec![[0, 1, 2]];
    (NiType::NiTriShape(shape), NiType::NiTriShapeData(d))
}

/// A root with one LOD node, whose children are one shape per level given.
fn lod_file(levels: &[[f32; 2]]) -> Vec<u8> {
    let mut s = Stream::new();
    let mut root = NiNode::default();
    root.base.base.name = "root".to_string();
    root.children = vec![Link::new(1)];
    s.blocks.push(NiType::NiNode(root));
    let mut lod = NiLODNode::default();
    lod.base.base.base.base.name = "lod".to_string();
    lod.lod_levels = levels.to_vec();
    let first_child = 2;
    lod.base.base.children = (0..levels.len()).map(|k| Link::new(first_child + 2 * k as i32)).collect();
    s.blocks.push(NiType::NiLODNode(lod));
    for (k, level) in levels.iter().enumerate() {
        let data = first_child + 2 * k as i32 + 1;
        let (shape, d) = shape_at(level[1], data);
        s.blocks.push(shape);
        s.blocks.push(d);
    }
    s.roots = vec![Link::new(0)];
    s.write().expect("a file")
}

#[test]
fn a_lod_node_shows_the_level_the_preview_stands_in() {
    // Two levels: near up to 500 units, far from 500 to 5000. The preview stands at 0.
    let f = lod_file(&[[0.0, 500.0], [500.0, 5000.0]]);
    let parts = nif::parse_draw(&f).expect("the mesh draws");
    assert_eq!(parts.len(), 1, "one level is shown: {:?}", parts.iter().map(|p| &p.name).collect::<Vec<_>>());
    assert_eq!(parts[0].name, "shape at 500");
    // And the hull is the near level's too: at z 500 and not 5000.
    let g = nif::parse(&f).expect("a hull");
    assert!(!g.tris.is_empty());
    assert!((g.aabb_max[2] - 500.0).abs() < 1e-3, "{:?}", g.aabb_max);
}

#[test]
fn a_far_only_lod_level_is_drawn_by_nothing_up_close() {
    // The Improved Lights sconce's shape: one level, 2500 to 4000 units, and nothing
    // nearer. Up close the game shows nothing of it, and neither does the preview.
    let f = lod_file(&[[2500.0, 4000.0]]);
    assert!(nif::parse_draw(&f).unwrap_or_default().is_empty(), "the far sprite is not drawn");
    // No geometry at all is no hull at all: the same answer an empty mesh gets.
    assert!(nif::parse(&f).map_or(true, |g| g.tris.is_empty()), "and it blocks no grass");
}

#[test]
fn the_levels_order_is_the_childrens_order_not_the_index_saved() {
    // The far level first, the near one second: the near one is still the one shown.
    let f = lod_file(&[[500.0, 5000.0], [0.0, 500.0]]);
    let parts = nif::parse_draw(&f).expect("the mesh draws");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].name, "shape at 500");
}
