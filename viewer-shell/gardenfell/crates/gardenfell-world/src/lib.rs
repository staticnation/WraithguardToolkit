//! The merged load order as a value: `World`, with its cells, references, landscape, rooms, coverage, map
//! tiles and mesh spans, built from an install through the plugin model.
//!
//! One of the `gardenfell-*` library crates cut from `gdn_core` in round 18gj; since round 18gm the
//! front ends and the tests name these crates directly, and `gdn_core` is gone.

pub mod coverage;
pub mod objects;
pub mod spans;
pub mod terrain;
pub mod world;
pub mod worldmap;

// The lower crates' modules this crate's files name as `crate::x`, resolved here so the files
// read as they did before the split. Private: none of these is this crate's to export.
use gardenfell_bytes::{img, json, openmwcfg, paths, pool, triaccel};
use gardenfell_install::vfs;
use gardenfell_nif::nif;
use gardenfell_tes3::{esp, land, records, tes3};
