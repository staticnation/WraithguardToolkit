//! Round 18ek: the height offset, floored by the mesh.
//!
//! Robin, after the West Gash and Ascadian Isles audit: "Alright, let's floor the scaled
//! lift by the mesh top then."
//!
//! Round 18eg made the lift travel with the blade, which pins the mesh's own z = -L on
//! the ground at every scale. That assumes the ground line falls inside the mesh. Four of
//! Lush Synthesis's meshes are modelled wholly below their own origin - `luse18`'s highest
//! vertex is 22.7 units under it - so against a lift of 21 the line at -21 is above the
//! whole mesh and nothing shows, at any scale including 1.
//!
//! The numbers in these tests are that mod's own, measured off Robin's install.

use gardenfell_world::spans::{floor_lift, MeshSpans};

/// The four meshes the audit named, with the lift their rule gives them.
/// `[z_lo, z_top]` measured with `gardenfell_nif::nif::parse_draw` over every drawn part.
const LUSE: [(&str, [f32; 2]); 4] = [
    ("luse18", [-34.6, -22.7]),   // wholly below the origin: the one that vanished
    ("luse17", [-62.1, -2.9]),
    ("luse02", [-46.2, -7.1]),
    ("luse01", [-52.6, 8.5]),
];
/// And an ordinary blade from the same set, for the control.
const LUWG03: [f32; 2] = [-46.6, 30.9];
const L: f32 = 21.0;   // GG_LIFT 16 + the rule's height_offset 5

#[test]
fn an_ordinary_blade_is_not_touched() {
    for lift in [16.0_f32, 21.0, 23.0] {
        assert_eq!(floor_lift(lift, Some(LUWG03)), lift, "luwg03 at a lift of {lift}");
    }
    // Nor is one with no span read at all - an empty store is the old behaviour exactly.
    assert_eq!(floor_lift(L, None), L);
}

#[test]
fn a_mesh_under_its_own_origin_is_lifted_until_it_shows() {
    // Only `luse18` is wholly under the ground line; the other three reach over it.
    for (name, span) in LUSE {
        let got = floor_lift(L, Some(span));
        if span[1] < -L {
            assert!(got > L, "{name}: the floor should have raised {L} and gave {got}");
            // The ground now runs through the middle of it, so half of it shows.
            assert!((got - -(span[0] + span[1]) * 0.5).abs() < 1e-4, "{name}: {got}");
        } else {
            assert_eq!(got, L, "{name}: reaches over the line already, leave it alone");
        }
    }
}

#[test]
fn what_it_does_to_luse18_at_every_scale() {
    let span = LUSE[0].1;
    let lift = floor_lift(L, Some(span));
    for s in [0.5_f32, 0.75, 1.0, 1.2] {
        // Where the top of the mesh ends up: the blade stands at ground + lift*s and the
        // mesh is scaled about its own origin.
        let top_before = L + s * span[1];
        let top_18eg = s * (L + span[1]);
        let top_now = s * (lift + span[1]);
        assert!(top_18eg < 0.0, "18eg had it underground at scale {s}: {top_18eg}");
        assert!(top_now > 0.0, "the floor should show it at scale {s}, not {top_now}");
        // Half the mesh, whatever the size - which is the property 18eg was after.
        let half = s * (span[1] - span[0]) * 0.5;
        assert!((top_now - half).abs() < 1e-3, "at {s}: {top_now} is not half of {half}");
        let _ = top_before;
    }
}

#[test]
fn the_floor_only_ever_raises() {
    // Over a wide sweep of shapes and lifts, including meshes that sit high above their
    // origin and ones buried far below it.
    for lo in [-80.0_f32, -50.0, -20.0, -5.0, 0.0] {
        for top in [-30.0_f32, -10.0, -1.0, 0.0, 5.0, 60.0] {
            if top < lo {
                continue;
            }
            for lift in [0.0_f32, 5.0, 16.0, 21.0, 40.0] {
                let got = floor_lift(lift, Some([lo, top]));
                assert!(got >= lift, "[{lo},{top}] at {lift} came back {got}");
                // And whatever it returns, the mesh's top is at or above the ground.
                assert!(got + top >= -1e-4, "[{lo},{top}] at {lift}: top still under by {}", got + top);
            }
        }
    }
}

#[test]
fn a_store_answers_by_mesh_and_ignores_case() {
    let mut sp = MeshSpans::default();
    sp.insert("grass\\az\\LUSE18.nif", LUSE[0].1);
    assert_eq!(sp.len(), 1);
    // The same mesh under either spelling, and the floor with it.
    assert_eq!(sp.span("grass\\az\\luse18.nif"), Some(LUSE[0].1));
    assert!(sp.lift("grass\\AZ\\LuSe18.nif", L) > L);
    // A mesh the store never read is left alone.
    assert_eq!(sp.lift("grass\\az\\luwg03.nif", L), L);
    assert_eq!(sp.span("grass\\az\\luwg03.nif"), None);
}

#[test]
fn a_degenerate_span_changes_nothing() {
    assert_eq!(floor_lift(21.0, Some([f32::NAN, 5.0])), 21.0);
    assert_eq!(floor_lift(21.0, Some([-5.0, f32::INFINITY])), 21.0);
}
