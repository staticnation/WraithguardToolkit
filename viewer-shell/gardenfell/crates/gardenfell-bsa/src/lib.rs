//! Bethesda archives: reading an archive's table, hashing a name, and the archive builder.
//!
//! One of the `gardenfell-*` library crates cut from `gdn_core` in round 18gj; since round 18gm the
//! front ends and the tests name these crates directly, and `gdn_core` is gone.

pub mod bsa;

// The lower crates' modules this crate's files name as `crate::x`, resolved here so the files
// read as they did before the split. Private: none of these is this crate's to export.
use gardenfell_bytes::{bytes, openmwcfg, paths};
