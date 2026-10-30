//! A creature's own visibility and morph controllers follow the `.kf`'s clip (patch 0021).
//! `xtr_cr_bonelord_grt_01.nif` (Tamriel Rebuilt's bonelord) keeps `Hood Node`'s and `Robe
//! Node`'s `NiVisController`s on the animation's timeline: hood on 0.07-18.5 s, robe on
//! 18.4-20.6 s, of a 22.7 s file; its `.kf` puts `Idle` at 0.083-3.333 s and `Death1` at
//! 18.417-20.667 s.

use gardenfell_nif::nif::{parse_draw, parse_draw_lit_kf, parse_kf, DrawPart};

const MESH: &[u8] = include_bytes!("fixtures/xtr_cr_bonelord_grt_01.nif");
const KF: &[u8] = include_bytes!("fixtures/xtr_cr_bonelord_grt_01.kf");

fn part<'a>(parts: &'a [DrawPart], what: &str) -> &'a DrawPart {
    parts
        .iter()
        .find(|p| p.name.to_lowercase().contains(what) && !p.name.to_lowercase().contains("shadow"))
        .unwrap_or_else(|| panic!("no part like {what}: {:?}", parts.iter().map(|p| &p.name).collect::<Vec<_>>()))
}

#[test]
fn the_kf_has_the_clips_the_keys_are_timed_to() {
    let kf = parse_kf(KF).expect("the kf reads");
    let (a, b) = kf.idle().expect("an idle clip");
    assert!((a - 0.0833).abs() < 0.01 && (b - 3.333).abs() < 0.01, "idle {a}..{b}");
}

#[test]
fn the_hood_and_robe_run_on_the_idle_clip_with_the_kf_and_on_their_own_without() {
    let kf = parse_kf(KF).unwrap();
    let (parts, _) = parse_draw_lit_kf(MESH, Some(&kf)).expect("the mesh reads with its kf");
    let (a, b) = kf.idle().unwrap();
    for what in ["hood", "robe"] {
        let p = part(&parts, what);
        let vis = &p.anim.as_ref().unwrap_or_else(|| panic!("{what} has no anim")).vis;
        assert!(!vis.is_empty(), "{what}: a visibility controller");
        for v in vis {
            assert!((v.start - a).abs() < 1e-3 && (v.stop - b).abs() < 1e-3, "{what}: window {}..{} is the idle clip {a}..{b}", v.start, v.stop);
            assert_eq!((v.flags >> 1) & 3, 0, "{what}: looping, like the bones' tracks");
            assert!(v.flags & 8 != 0, "{what}: on");
        }
        // and the morph (the cloth's sway) is on the same clock
        if let Some(m) = &p.morph {
            assert!((m.start - a).abs() < 1e-3 && (m.stop - b).abs() < 1e-3, "{what}: morph window {}..{}", m.start, m.stop);
        }
    }
    // with no .kf nothing is re-timed: the file's own window, as before
    let alone = parse_draw(MESH).expect("the mesh reads alone");
    let hood = part(&alone, "hood");
    let v = &hood.anim.as_ref().unwrap().vis[0];
    assert!(v.start < 0.1 && v.stop > 22.0, "alone: {}..{}", v.start, v.stop);
}
