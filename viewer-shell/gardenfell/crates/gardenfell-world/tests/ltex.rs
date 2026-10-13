//! VTEX index 0 is the engine's own land texture, not an absence of one. Whole
//! stretches of vanilla landscape were never painted — riverbeds especially — and
//! treating 0 as nothing leaves them bare in the tool and textured in the game.

use std::collections::HashMap;

use gardenfell_world::world::{World, DEFAULT_LTEX, DEFAULT_LTEX_FILE};

fn world_with_two_plugins() -> World {
    let mut w = World::default();
    let mut base = HashMap::new();
    base.insert(0u32, "Tx_BC_grass".to_string());
    base.insert(1, "Tx_sand".to_string());
    let mut mod_ = HashMap::new();
    mod_.insert(7, "AI_Clover".to_string());
    w.ltex_by_plugin.push(base);
    w.ltex_by_plugin.push(mod_);
    w.ltex_file.insert(DEFAULT_LTEX.to_string(), DEFAULT_LTEX_FILE.to_string());
    w
}

#[test]
fn unpainted_ground_resolves_to_the_engine_default() {
    let w = world_with_two_plugins();
    assert_eq!(w.ltex_for(0, 0), Some(DEFAULT_LTEX));
    assert_eq!(w.ltex_for(0, 1), Some(DEFAULT_LTEX), "the default does not depend on the plugin");
    assert_eq!(w.ltex_file.get(DEFAULT_LTEX).map(String::as_str), Some(DEFAULT_LTEX_FILE));
}

#[test]
fn painted_indices_are_still_one_based_and_plugin_local() {
    let w = world_with_two_plugins();
    assert_eq!(w.ltex_for(1, 0), Some("Tx_BC_grass"));
    assert_eq!(w.ltex_for(2, 0), Some("Tx_sand"));
    // Index 7 exists only in the second plugin, so land written by the first cannot
    // see it, and land written by the second can.
    assert_eq!(w.ltex_for(8, 0), None);
    assert_eq!(w.ltex_for(8, 1), Some("AI_Clover"));
    // A plugin that edits land without shipping its own LTEX falls back down the order.
    assert_eq!(w.ltex_for(1, 1), Some("Tx_BC_grass"));
}
