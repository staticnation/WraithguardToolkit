//! The alpha maps the ground is drawn from.
//!
//! These were built twice — once in Rust for the scatter, once in JavaScript for
//! the viewport — and the two could not agree, because the page can only see the
//! cells it has loaded and the ring rule needs the neighbour. This is the engine's
//! half of that: the ring, the draw order, and the mask packing the shell ships.

use gardenfell_world::coverage;
use gardenfell_tes3::land::{Land, SUBDIV, VERTS};
use gardenfell_world::world::World;

const GRASS: &str = "t_grass";
const ROAD: &str = "t_road";
const ASH: &str = "t_ash";

fn land_at(grid: (i32, i32), fill: impl Fn(usize, usize) -> u16) -> Land {
    let mut vtex = Box::new([0u16; SUBDIV * SUBDIV]);
    for j in 0..SUBDIV {
        for i in 0..SUBDIV {
            vtex[j * SUBDIV + i] = fill(i, j);
        }
    }
    Land { grid, heights: Box::new([0.0f32; VERTS * VERTS]), normals: None, colours: None, vtex, plugin: 0 }
}

/// vtex values are the LTEX index plus one.
fn world() -> World {
    let mut w = World::default();
    let mut ltex = std::collections::HashMap::new();
    ltex.insert(0u32, GRASS.to_string());
    ltex.insert(1u32, ROAD.to_string());
    ltex.insert(2u32, ASH.to_string());
    w.ltex_by_plugin.push(ltex);
    w
}

fn bit(mask: &[u8], t: usize) -> bool {
    (mask[t >> 3] >> (t & 7)) & 1 == 1
}

/// The mask index of a ring coordinate, gi/gj in -1..=16.
fn at(gi: i32, gj: i32) -> usize {
    ((gj + 1) * coverage::P as i32 + (gi + 1)) as usize
}

fn layer<'a>(ls: &'a [coverage::Layer], id: &str) -> &'a coverage::Layer {
    ls.iter().find(|l| l.id == id).unwrap_or_else(|| panic!("no {} layer", id))
}

#[test]
fn a_lone_cell_holds_its_edge_value() {
    // No neighbours loaded. The ring can only repeat the border, which is what the
    // page did too and what a cell floating in empty space should look like.
    let w = world();
    let land = land_at((0, 0), |_, _| 1);
    let ls = coverage::layers(&w, &land);
    assert_eq!(ls.len(), 1);
    assert_eq!(ls[0].id, GRASS);
    assert!((ls[0].coverage - 1.0).abs() < 1e-6);
    assert!(bit(&ls[0].mask, at(-1, -1)) && bit(&ls[0].mask, at(16, 16)));
}

#[test]
fn the_ring_comes_from_the_neighbouring_landscape() {
    // This is the fact the page could not know. The cell is all grass; the cell to
    // its east is all road. The east ring column must read road.
    let mut w = world();
    let land = land_at((0, 0), |_, _| 1);
    w.lands.insert((1, 0), land_at((1, 0), |_, _| 2));
    w.lands.insert((0, 0), land.clone());

    let ls = coverage::layers(&w, &land);
    let road = layer(&ls, ROAD);
    for gj in 0..SUBDIV as i32 {
        assert!(bit(&road.mask, at(16, gj)), "east ring texel {} is not road", gj);
    }
    // …and the west ring, with nothing loaded there, still holds grass.
    let grass = layer(&ls, GRASS);
    assert!(bit(&grass.mask, at(-1, 0)));
}

#[test]
fn a_texture_that_only_bleeds_in_is_a_layer_but_never_the_base() {
    // Draw order is by how much of the cell's *own* ground a texture paints. A
    // neighbour's texture reaches one texel in; if that counted, a cell beside a
    // large ash flat could be drawn ash-first.
    let mut w = world();
    let land = land_at((0, 0), |_, _| 1);
    w.lands.insert((1, 0), land_at((1, 0), |_, _| 3));
    w.lands.insert((0, 1), land_at((0, 1), |_, _| 3));
    w.lands.insert((1, 1), land_at((1, 1), |_, _| 3));

    let ls = coverage::layers(&w, &land);
    assert_eq!(ls[0].id, GRASS, "the base layer must be the cell's own commonest texture");
    let ash = layer(&ls, ASH);
    assert_eq!(ash.coverage, 0.0, "ash paints none of this cell");
    assert!(bit(&ash.mask, at(16, 0)), "but it is present in the ring");
}

#[test]
fn layers_partition_every_texel_exactly_once() {
    // A texel carries one texture, which is what makes a one-bit mask exact. If two
    // layers ever claimed the same texel the ground would draw double-alpha.
    let mut w = world();
    let land = land_at((0, 0), |i, j| if i < 8 { 1 } else if j < 8 { 2 } else { 3 });
    w.lands.insert((-1, 0), land_at((-1, 0), |_, _| 3));

    let ls = coverage::layers(&w, &land);
    for t in 0..coverage::FIELD {
        let n = ls.iter().filter(|l| bit(&l.mask, t)).count();
        assert_eq!(n, 1, "texel {} is claimed by {} layers", t, n);
    }
    // In-cell coverage is a partition too, so the fractions sum to one.
    let sum: f32 = ls.iter().map(|l| l.coverage).sum();
    assert!((sum - 1.0).abs() < 1e-6, "coverage sums to {}", sum);
}

#[test]
fn draw_order_is_most_painted_first() {
    let w = world();
    // road on 12 columns, grass on 4.
    let land = land_at((0, 0), |i, _| if i < 4 { 1 } else { 2 });
    let ls = coverage::layers(&w, &land);
    assert_eq!(ls[0].id, ROAD);
    assert_eq!(ls[1].id, GRASS);
    assert!((ls[0].coverage - 0.75).abs() < 1e-6);
    assert!((ls[1].coverage - 0.25).abs() < 1e-6);
}

#[test]
fn sampling_is_a_gradient_between_texel_centres() {
    // The whole reason the maps exist: grass reads the same ramp the eye sees. At a
    // texel centre the coverage is flat 1 or 0; halfway between two it is a half.
    let w = world();
    let land = land_at((0, 0), |i, _| if i < 8 { 1 } else { 2 });
    let f = coverage::field_where(&w, &land, |t| t == GRASS).expect("grass is painted");
    const TEXEL: f32 = 512.0;
    assert!((coverage::sample(&f, TEXEL * 3.5, 4096.0) - 1.0).abs() < 1e-4, "deep inside grass");
    // x = 4096 is the join: texel 7 (grass) and texel 8 (road) centres straddle it.
    assert!((coverage::sample(&f, 4096.0, 4096.0) - 0.5).abs() < 1e-4, "on the join");
    assert!(coverage::sample(&f, TEXEL * 12.5, 4096.0) < 1e-4, "deep inside road");
}

#[test]
fn an_unpainted_field_is_dropped_rather_than_returned_empty() {
    // A seventy-entry ban list matches nothing in most cells. Answering None there
    // is what makes a full list free on the rest of the map.
    let w = world();
    let land = land_at((0, 0), |_, _| 1);
    assert!(coverage::field_where(&w, &land, |t| t == ROAD).is_none());
    assert!(coverage::field_where(&w, &land, |t| t == GRASS).is_some());
}
