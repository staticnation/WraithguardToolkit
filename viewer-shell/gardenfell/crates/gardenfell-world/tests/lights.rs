//! Lamps and moving textures — round 17m.
//!
//! Two of Robin's queued asks, and both turned out to be things already written in the
//! files:
//!
//! 1. "For all .nif files, if they have a light source, I would want to lit the scene
//!    with them, but ONLY when it's dark [...] Lights is it's own form of record in
//!    Morrowind engine, and in that record there is information about how far they light
//!    things up. For light falloff etc. look at OpenMW." The record is LIGH and its LHDT
//!    subrecord is 24 bytes — weight f32, then value, time, radius, colour and flags as
//!    int32. Where the light hangs is a node in the mesh called `AttachLight`, which
//!    OpenMW looks for by name (`SceneUtil::addLight`) and falls back to the origin
//!    without. And: "Some assets like light_de_paper_lantern_01_128 has a brighter
//!    exterior [...] when they are on" — that is the NiMaterialProperty's *emissive*
//!    colour, and it is the entire difference between Morrowind's own
//!    `light_paper_lantern_01.nif` and `light_paper_lantern_off.nif`.
//!
//! 2. "Ex_Vivec_waterfall_03, ex_vivec_waterfall_05_q [...] All have different kinds of
//!    translucency and movement in the meshes." The movement is a NiUVController and its
//!    NiUVData: four key curves that make a texture matrix, which the game applies to the
//!    texture rather than to the geometry.
//!
//! The vanilla assets are the test wherever `GARDENFELL_BSA` names Morrowind.bsa; the
//! parts that do not need one run everywhere.

use gardenfell_nif::nif::parse_draw_lit;

fn bsa() -> Option<gardenfell_bsa::bsa::Bsa> {
    let p = std::env::var("GARDENFELL_BSA").ok()?;
    gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)).ok()
}

/// LHDT, byte for byte. Built by hand rather than read from a plugin, so the layout is
/// asserted rather than assumed: weight, value, time, radius, colour, flags.
#[test]
fn a_light_records_radius_and_colour_come_off_lhdt() {
    let mut body: Vec<u8> = Vec::new();
    let sub = |tag: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(tag);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    };
    sub(b"NAME", b"light_test\0", &mut body);
    sub(b"MODL", b"l\\light_test.nif\0", &mut body);
    let mut lhdt = Vec::new();
    lhdt.extend_from_slice(&2.5f32.to_le_bytes()); // weight
    lhdt.extend_from_slice(&30i32.to_le_bytes()); // value
    lhdt.extend_from_slice(&0i32.to_le_bytes()); // time
    lhdt.extend_from_slice(&384i32.to_le_bytes()); // radius
    lhdt.extend_from_slice(&[255, 180, 90, 0]); // colour: r, g, b, ignored
    lhdt.extend_from_slice(&0x18i32.to_le_bytes()); // Flicker | Fire
    sub(b"LHDT", &lhdt, &mut body);

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"TES3");
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(b"LIGH");
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&body);

    let d = gardenfell_world::world::parse_plugin(&buf, 0, &[], false);
    assert_eq!(d.lights.len(), 1, "one LIGH record");
    let l = &d.lights[0];
    assert_eq!(l.id, "light_test");
    assert_eq!(l.radius, 384);
    assert_eq!(l.colour, [255, 180, 90]);
    assert!(!l.negative(), "flags {:#x}", l.flags);
    assert!(!l.off_default());
    // And it is still an ordinary model record, so the mesh reaches the preview.
    assert!(d.models.iter().any(|m| m.id == "light_test" && m.mesh == "l\\light_test.nif"));
}

/// The two flags that change what a placed light does: Negative (0x004) is darkness,
/// OffDefault (0x020) does not burn where it stands.
#[test]
fn the_two_flags_that_matter_are_read_off_the_number() {
    let mk = |flags: i32| gardenfell_world::world::LightDef { flags, ..Default::default() };
    assert!(mk(0x004).negative());
    assert!(!mk(0x004).off_default());
    assert!(mk(0x020).off_default());
    assert!(!mk(0x020).negative());
    assert!(mk(0x024).negative() && mk(0x024).off_default());
    assert!(!mk(0x018).negative() && !mk(0x018).off_default(), "Flicker and Fire change nothing here");
}

/// A lit lantern glows; the same lantern's "off" mesh does not. Nothing else about the
/// two files differs — same shapes, same textures, same material alpha.
#[test]
fn the_lit_lantern_carries_an_emissive_and_the_unlit_one_does_not() {
    let Some(a) = bsa() else { return };
    let Some(on) = a.get("meshes/l/light_paper_lantern_01.nif") else { return };
    let Some(off) = a.get("meshes/l/light_paper_lantern_off.nif") else { return };
    let (on, _) = parse_draw_lit(on).expect("parses");
    let (off, _) = parse_draw_lit(off).expect("parses");
    let lit = |v: &[gardenfell_nif::nif::DrawPart]| v.iter().filter(|p| p.emissive != [0.0; 3]).count();
    assert!(lit(&on) > 0, "the lit lantern has a glowing shape");
    assert_eq!(lit(&off), 0, "the unlit one has none");
    assert_eq!(on.len(), off.len(), "otherwise the same mesh");
}

/// Where the flame is. A hanging lantern's light belongs at the lamp, not at the point
/// the mesh is placed from.
#[test]
fn a_mesh_says_where_its_light_hangs() {
    let Some(a) = bsa() else { return };
    let Some(b) = a.get("meshes/l/light_de_lantern_01.nif") else { return };
    let (_, at) = parse_draw_lit(b).expect("parses");
    let at = at.expect("light_de_lantern_01 has an AttachLight node");
    // Below the origin and a little to one side: the lamp under the bracket.
    assert!(at[2] < -10.0, "hangs below the anchor: {:?}", at);
    assert!(at[0].abs() < 20.0 && at[1].abs() < 20.0, "roughly under it: {:?}", at);

    // And a mesh without one says so, rather than inventing a position.
    let Some(p) = a.get("meshes/l/light_paper_lantern_01.nif") else { return };
    assert!(parse_draw_lit(p).unwrap().1.is_none());
}

/// The waterfall. Two sheets, each scrolling its texture down at its own rate and out of
/// phase with the other — which is what stops the pair reading as one sliding image.
#[test]
fn the_vivec_waterfall_scrolls_its_texture_down() {
    let Some(a) = bsa() else { return };
    let Some(b) = a.get("meshes/x/ex_vivec_waterfall_01.nif") else { return };
    let (parts, _) = parse_draw_lit(b).expect("parses");
    assert_eq!(parts.len(), 2, "two sheets");
    let mut phases = Vec::new();
    for p in &parts {
        let an = p.uv_anim.as_ref().expect("every sheet moves");
        assert!(an.u_trans.is_empty(), "it does not slide sideways");
        assert_eq!(an.v_trans.len(), 2, "a ramp with two keys");
        assert_eq!(an.v_trans[0], (0.0, 0.0));
        // Down, by whole texture repeats, so the wrap is seamless.
        assert!(an.v_trans[1].1 < 0.0, "downwards: {:?}", an.v_trans);
        assert_eq!(an.v_trans[1].1, an.v_trans[1].1.round(), "whole repeats: {:?}", an.v_trans);
        assert_eq!(an.stop, 1.0);
        phases.push(an.phase);
        // And it is translucent, which round 17h reads: a blend flag and a threshold.
        assert!(p.alpha_flags & 1 == 1, "blended: {:#06x}", p.alpha_flags);
    }
    assert!(phases[0] != phases[1], "and the two sheets are out of step: {:?}", phases);
}

/// The Gnisis fence's banner sways: its U offset goes out one way, back through zero,
/// out the other and back, over eight seconds, while the V offset runs steadily on.
#[test]
fn the_gnisis_fence_banner_sways_rather_than_scrolls() {
    let Some(a) = bsa() else { return };
    let Some(b) = a.get("meshes/x/ex_gg_fence_s_03.nif") else { return };
    let (parts, _) = parse_draw_lit(b).expect("parses");
    let moving: Vec<_> = parts.iter().filter_map(|p| p.uv_anim.as_ref()).collect();
    assert!(!moving.is_empty(), "something on it moves");
    let an = moving[0];
    assert_eq!(an.stop, 8.0, "an eight-second cycle");
    assert_eq!(an.u_trans.len(), 5, "out, back, out the other way, back");
    assert_eq!(an.u_trans[0], (0.0, 0.0));
    assert_eq!(an.u_trans[4], (8.0, 0.0), "and it ends where it began, so it loops");
    let peak = an.u_trans.iter().map(|(_, v)| v.abs()).fold(0.0f32, f32::max);
    assert!(peak > 0.0 && peak < 0.5, "a sway, not a scroll: {:?}", an.u_trans);
    assert!(an.u_scale.is_empty() && an.v_scale.is_empty(), "the tiling is left alone");
}

/// A still mesh carries no animation and no glow, so nothing about this costs anything
/// on the overwhelming majority of what a cell holds.
#[test]
fn an_ordinary_rock_has_neither() {
    let Some(a) = bsa() else { return };
    let Some(b) = a.get("meshes/f/furn_banner_hlaalu_01.nif") else { return };
    let (parts, at) = parse_draw_lit(b).expect("parses");
    assert!(at.is_none());
    for p in &parts {
        assert!(p.uv_anim.is_none(), "{} moves?", p.name);
    }
}
