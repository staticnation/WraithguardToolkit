//! What a texture's alpha channel is *for* — round 17n.
//!
//! Robin: "There is weird artefacts in the waterfall meshes (Ex_Vivec_waterfall_03.NIF as
//! an example mesh) [...] They look like a geometric pattern of darker and brighter areas
//! where it shouldn't be." And, separately: "The Ghostfence (x\\Ex_GG_fence_s_02.NIF) is
//! also translucent."
//!
//! One cause. A NiAlphaProperty says *how* to draw a shape but not what its texture
//! holds, and round 17h's rule — blend when the material alpha is under 1, otherwise cut
//! at 0.34 — is right for a stencil and wrong for a gradient. Cutting `Tx_waterfall_01`,
//! whose alpha runs smoothly across the whole range, turns two soft sheets into two hard
//! overlapping stencils: the pattern Robin saw. So the alpha channel is classified at
//! full resolution and the flags are read in the light of it.
//!
//! Measured over every shape in Morrowind.bsa: 22,577 shapes, of which 2,069 say "blend"
//! with no alpha test. Of those, 1,533 are gradients (which now blend) and 506 are
//! stencils (which still cut). Nothing else changes.

use gardenfell_bytes::img::{classify_alpha, classify_alpha_blocks, Alpha, Kind};

/// `n` texels of the given alpha values, cycled.
fn rgba(alphas: &[u8], n: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(n * 4);
    for i in 0..n {
        v.extend_from_slice(&[128, 128, 128, alphas[i % alphas.len()]]);
    }
    v
}

#[test]
fn a_channel_of_nothing_but_255_is_opaque() {
    assert_eq!(classify_alpha(&rgba(&[255], 4096)), Alpha::Opaque);
    // And near enough to it: a stencil's ends are bands, not exact values.
    assert_eq!(classify_alpha(&rgba(&[255, 250, 245], 4096)), Alpha::Opaque);
    assert_eq!(classify_alpha(&[]), Alpha::Opaque);
}

#[test]
fn two_clusters_at_the_ends_is_a_stencil() {
    assert_eq!(classify_alpha(&rgba(&[0, 255], 4096)), Alpha::Binary);
    /* `tx_dwrv_parch20` in the flesh: 12,931 texels at the bottom, 50,829 at the top and
       1,776 — 2.7% — in between. A cutout with soft edges is still a cutout. */
    let mut v = rgba(&[0], 12931);
    v.extend(rgba(&[64, 96, 128, 160, 192], 1776));
    v.extend(rgba(&[255], 50829));
    assert_eq!(classify_alpha(&v), Alpha::Binary);
}

#[test]
fn a_real_gradient_is_soft() {
    // Tx_waterfall_01: mass in every band.
    let mut v = rgba(&[0, 8, 20], 3147);
    v.extend(rgba(&[40, 60, 80, 100, 120, 140, 160, 180, 200], 59783));
    assert_eq!(classify_alpha(&v), Alpha::Soft);
    /* And a flat value that simply is not 1 — `Tx_window_pane` is every one of its 4,096
       texels at about 0.4. A uniformly translucent pane, which a cutout at 0.34 would
       have drawn as solid glass. */
    assert_eq!(classify_alpha(&rgba(&[110], 4096)), Alpha::Soft);
}

/// The threshold is not near anything real: the measured meshes sit at 2.7% (a stencil)
/// and 95%+ (a gradient), so there is a wide margin either side of the 5% line.
#[test]
fn the_line_between_them_has_room_on_both_sides() {
    let n = 10000;
    let soft_share = |pct: usize| {
        let mut v = rgba(&[128], n * pct / 100);
        v.extend(rgba(&[0], n * (100 - pct) / 200));
        v.extend(rgba(&[255], n * (100 - pct) / 200));
        classify_alpha(&v)
    };
    assert_eq!(soft_share(3), Alpha::Binary);
    assert_eq!(soft_share(20), Alpha::Soft);
    assert_eq!(soft_share(95), Alpha::Soft);
}

/// BC blocks are read without decoding their colours: BC3 keeps eight-bit alpha in the
/// first eight bytes of the block, BC2 four bits, and BC1 one bit that can only ever be
/// a stencil.
#[test]
fn compressed_blocks_are_classified_from_their_alpha_alone() {
    // One BC3 block, alpha endpoints 0 and 255 with the indices all picking a middle
    // interpolated value: a gradient.
    let mut b = vec![0u8; 16];
    b[0] = 255;
    b[1] = 0;
    for i in 2..8 {
        b[i] = 0b10010010; // index 2 everywhere: 6/7 of the way between the endpoints
    }
    assert_eq!(classify_alpha_blocks(&b, 4, 4, Kind::Bc3), Alpha::Soft);
    // The same block with every index 0 — all at the first endpoint, 255.
    let mut o = vec![0u8; 16];
    o[0] = 255;
    o[1] = 0;
    assert_eq!(classify_alpha_blocks(&o, 4, 4, Kind::Bc3), Alpha::Opaque);
    /* BC1 with c0 > c1 has no alpha at all; with c0 <= c1 index 3 is the clear texel, and
       there is nothing in between either way. */
    let mut o1 = vec![0u8; 8];
    o1[0] = 0xff; o1[1] = 0xff;   // c0 = 0xffff > c1 = 0
    assert_eq!(classify_alpha_blocks(&o1, 4, 4, Kind::Bc1), Alpha::Opaque);
    let mut c1 = vec![0u8; 8];
    c1[2] = 0xff; c1[3] = 0xff;   // c0 = 0 <= c1
    c1[4] = 0b11000000;           // one texel on index 3
    assert_eq!(classify_alpha_blocks(&c1, 4, 4, Kind::Bc1), Alpha::Binary);
}

/// Against the game's own textures, when `GARDENFELL_BSA` names Morrowind.bsa. These are
/// the five the classification was designed from, and the reason the thresholds are where
/// they are.
#[test]
fn the_games_own_textures_land_where_they_should() {
    let Ok(p) = std::env::var("GARDENFELL_BSA") else { return };
    let Ok(a) = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)) else { return };
    for (name, want) in [
        ("tx_waterfall_01", Alpha::Soft),      // the waterfall Robin reported
        ("tx_gg_fence_01", Alpha::Soft),       // the Ghostfence
        ("tx_window_pane", Alpha::Soft),       // every texel at 0.4 — a pane of glass
        ("tx_dwrv_parch20", Alpha::Binary),    // a stencil with soft edges
        ("tx_bc_grass", Alpha::Opaque),        // no alpha worth the name
    ] {
        let Some(b) = a.get(&format!("textures/{name}.dds")) else { continue };
        let t = gardenfell_bytes::img::read(b, name, false).expect("decodes");
        assert_eq!(t.alpha, want, "{name}");
    }
}
