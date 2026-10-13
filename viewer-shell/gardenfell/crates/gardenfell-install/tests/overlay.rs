//! Which mod answers when several spell the same texture differently.
//!
//! Robin: "Not sure the ground textures are actually the latest ones that win in my MO2
//! load order. Many look more like vanilla morrowind textures."
//!
//! They were vanilla. A Morrowind LTEX record names a `.tga` — `Tx_AC_rock_01.tga` —
//! and almost every land retexture ships `.dds` instead, because that is what the game
//! prefers. Resolving the family extension-first asks the *whole load order* for the
//! `.tga` before it asks anyone for the `.dds`, so vanilla's loose copy at the bottom
//! of the order answered and the eleven retextures above it were never consulted.
//!
//! The order that gets it right is root-major: the first mod holding any member of the
//! family wins, and inside one mod `.dds` wins. These tests are built to fail under the
//! old order rather than merely to pass under the new one — every case puts the two
//! rules in direct conflict.

use gardenfell_install::vfs::Vfs;
use std::path::{Path, PathBuf};

fn tmp(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gf-ovl-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `mod/relative/path` -> a file with that name holding its own path, so a resolution
/// can be checked by reading it rather than by comparing platform paths.
fn make(root: &Path, rel: &str) {
    let p = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, rel.as_bytes()).unwrap();
}

fn vfs(dirs: &[(&str, PathBuf)]) -> Vfs {
    let owned: Vec<(String, PathBuf)> =
        dirs.iter().map(|(l, d)| ((*l).to_string(), d.clone())).collect();
    Vfs::with_roots(&owned)
}

fn found(v: &Vfs, rel: &str) -> String {
    let p = v.resolve_texture(rel).expect("nothing resolved");
    String::from_utf8(v.load(&p).unwrap()).unwrap()
}

/// The headline case, and the one that was wrong.
#[test]
fn a_retexture_above_vanilla_wins_even_when_it_spells_the_name_differently() {
    let (top, bottom) = (tmp("retex-top"), tmp("retex-bottom"));
    make(&top, "Textures/tx_ac_rock_01.dds");
    make(&bottom, "Textures/Tx_AC_rock_01.tga");

    let v = vfs(&[("retexture", top), ("Data Files", bottom)]);
    assert_eq!(
        found(&v, "Tx_AC_rock_01.tga"),
        "Textures/tx_ac_rock_01.dds",
        "the record names the .tga vanilla ships; the mod on top of it ships only .dds"
    );
}

/// And the converse, so the rule is not "always prefer .dds" — a mod that ships exactly
/// what the record asked for must still beat a lower mod's .dds.
#[test]
fn a_mod_that_ships_the_stated_spelling_still_beats_a_lower_mods_dds() {
    let (top, bottom) = (tmp("stated-top"), tmp("stated-bottom"));
    make(&top, "Textures/tx_ac_rock_01.tga");
    make(&bottom, "Textures/tx_ac_rock_01.dds");

    let v = vfs(&[("upper", top), ("lower", bottom)]);
    assert_eq!(found(&v, "Tx_AC_rock_01.tga"), "Textures/tx_ac_rock_01.tga");
}

/// Within one mod the game prefers `.dds`, and so does this.
#[test]
fn one_mod_shipping_both_is_read_as_the_dds() {
    let d = tmp("both");
    make(&d, "Textures/tx_ac_rock_01.tga");
    make(&d, "Textures/tx_ac_rock_01.dds");

    let v = vfs(&[("only", d)]);
    assert_eq!(found(&v, "Tx_AC_rock_01.tga"), "Textures/tx_ac_rock_01.dds");
}

/// A NIF names its textures without the folder as often as with it.
#[test]
fn the_textures_prefix_is_optional_in_the_name_and_supplied_in_the_lookup() {
    let d = tmp("prefix");
    make(&d, "Textures/grass/az_bc.dds");
    let v = vfs(&[("mod", d)]);
    assert_eq!(found(&v, "grass\\az_bc.tga"), "Textures/grass/az_bc.dds", "backslashes too");
}

/// Mod Organizer hides a file by renaming it, and a hidden file is not in the overlay
/// the game sees. Resolving one would put a texture the user switched off back on the
/// ground — and worse, would hide the mod underneath that was meant to take over.
#[test]
fn a_file_mod_organizer_has_hidden_is_not_in_the_overlay() {
    let (top, bottom) = (tmp("hidden-top"), tmp("hidden-bottom"));
    make(&top, "Textures/tx_ac_rock_01.dds.mohidden");
    make(&bottom, "Textures/tx_ac_rock_01.dds");

    let v = vfs(&[("hiding it", top), ("providing it", bottom)]);
    assert_eq!(found(&v, "Tx_AC_rock_01.tga"), "Textures/tx_ac_rock_01.dds");

    let listed = v.list("textures/", Vfs::TEX_EXTS);
    assert_eq!(listed.len(), 1, "the hidden file must not be offered in a browser either");
}

/// Nothing at all is a normal answer — a config naming a texture the install does not
/// have is the case the tool exists to report — and it must not be an error or a panic.
#[test]
fn a_texture_no_mod_supplies_resolves_to_nothing() {
    let d = tmp("missing");
    make(&d, "Textures/something_else.dds");
    let v = vfs(&[("mod", d)]);
    assert!(v.resolve_texture("Tx_AC_rock_01.tga").is_none());
}
