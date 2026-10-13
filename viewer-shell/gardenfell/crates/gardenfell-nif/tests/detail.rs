//! The detail map and translucency — round 17h.
//!
//! Robin: "pc\\f\\pc_flora_scumalpha_03.nif show up as white cutouts, where I think they
//! should be colored somehow with a texture? Maybe [...] there are more texture paths in
//! the .nif?" There are: its NiTexturingProperty names a *base* map that is a white shape
//! with an alpha cutout and a *detail* map, `tx_bc_scum.dds`, on the second UV set, which
//! is where all of its colour lives. The game's second texture stage multiplies the two
//! and doubles the result; OpenMW copies that with a TexEnvCombine.
//!
//! And: "OAAB\\e\\water_rectf256_01.nif and other water meshes [...] are supposed to be
//! translucent. Maybe there is a common thread?" There is — a NiAlphaProperty asking for
//! SRC_ALPHA / ONE_MINUS_SRC_ALPHA blending over a NiMaterialProperty alpha under 1.
//! Both are read here; the viewport draws them (`06_gl.js`).
//!
//! `fixture/detail.nif` (`gf/mkdetail.js`) carries both.

use gardenfell_nif::nif::parse_draw;
use std::path::PathBuf;

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixture").join(name);
    std::fs::read(&p).unwrap_or_else(|_| panic!("{} — run `node mkdetail.js` in gf/", p.display()))
}

#[test]
fn the_detail_map_and_its_own_uv_set_are_read() {
    let parts = parse_draw(&fixture("detail.nif")).expect("parses");
    assert_eq!(parts.len(), 1);
    let p = &parts[0];
    assert_eq!(p.tex, "shape_alpha.dds", "the base map is still the base map");
    assert_eq!(p.detail_tex, "the_colour.dds", "and the detail map is read beside it");
    // Four vertices, and the detail's UVs are the *second* set — half the first's.
    assert_eq!(p.uv2.len(), 8, "one pair per vertex");
    assert_eq!(p.uv.len(), 8);
    for (a, b) in p.uv.iter().zip(p.uv2.iter()) {
        assert!((a * 0.5 - b).abs() < 1e-6, "set 1, not set 0: {a} vs {b}");
    }
}

#[test]
fn a_translucent_shape_keeps_its_blend_flags_and_material_alpha() {
    let parts = parse_draw(&fixture("detail.nif")).expect("parses");
    let p = &parts[0];
    assert_eq!(p.alpha_flags & 1, 1, "blending enabled");
    assert_eq!((p.alpha_flags >> 1) & 15, 6, "source factor SRC_ALPHA");
    assert_eq!((p.alpha_flags >> 5) & 15, 7, "destination ONE_MINUS_SRC_ALPHA");
    assert!(p.alpha_threshold.is_none(), "no alpha test on this one");
    assert!((p.mat_alpha - 0.7).abs() < 1e-6, "the material's own alpha: {}", p.mat_alpha);
}

#[test]
fn a_mesh_without_either_says_so() {
    // The plain fixture: one texture, no detail, opaque.
    let parts = parse_draw(&fixture("static.nif")).expect("parses");
    let p = &parts[0];
    assert!(p.detail_tex.is_empty() && p.uv2.is_empty(), "no detail map, no second set");
    assert!((p.mat_alpha - 1.0).abs() < 1e-6);
}
