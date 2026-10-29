//! A shape under OpenMW's `distortion` shader effect is not drawn as a surface (patch 0020).
//! `tr_water_sqstat512.nif` is Tamriel Rebuilt's animated water square: `Tri Water` (a
//! flipbook of 32 `tr_water00..31` frames), an editor marker, and `Tri Water Distortion2`,
//! whose `omw:data` asks for the `distortion` shader and whose 90 `omw_distortion\water`
//! frames are normal maps.

use gardenfell_nif::nif::parse_draw;

const WATER: &[u8] = include_bytes!("fixtures/tr_water_sqstat512.nif");

#[test]
fn the_distortion_shape_is_not_drawn_but_the_water_is() {
    let parts = parse_draw(WATER).expect("the mesh reads");
    let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(parts.len(), 1, "only the water itself: {names:?}");
    assert!(parts[0].name.to_lowercase().contains("water") && !parts[0].name.to_lowercase().contains("distortion"), "{names:?}");
    // and it is the real water's maps, not a normal map's
    let everything: Vec<String> = parts.iter().flat_map(|p| std::iter::once(p.tex.clone()).chain(p.flip.iter().flat_map(|f| f.frames.clone()))).collect();
    assert!(everything.iter().any(|t| t.to_lowercase().contains("tr_water")), "{everything:?}");
    assert!(!everything.iter().any(|t| t.to_lowercase().contains("omw_distortion")), "{everything:?}");
}
