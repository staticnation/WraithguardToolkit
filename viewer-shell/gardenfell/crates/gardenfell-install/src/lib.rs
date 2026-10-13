//! A Morrowind install as the game sees it: where it is (MO2, OpenMW, plain), what it loads and in which
//! order, the overlay file system over its folders and archives, and the engine settings it runs with.
//!
//! One of the `gardenfell-*` library crates cut from `gdn_core` in round 18gj; since round 18gm the
//! front ends and the tests name these crates directly, and `gdn_core` is gone.

pub mod layout;
pub mod mge;
pub mod plugins;
pub mod renderer;
pub mod vfs;
pub mod weather;

// The lower crates' modules this crate's files name as `crate::x`, resolved here so the files
// read as they did before the split. Private: none of these is this crate's to export.
use gardenfell_bsa::bsa;
use gardenfell_bytes::{codepage, ini, json, openmwcfg, paths, pool, toml};
use gardenfell_tes3::esp;
use gardenfell_text::msg;
