//! Morrowind plugins: the raw record layer and the typed model of every record kind (`tes3`), the
//! engine's views of cells and landscape (`esp`, `land`), the record index, and the plugin writer and patcher.
//!
//! One of the `gardenfell-*` library crates cut from `gdn_core` in round 18gj; since round 18gm the
//! front ends and the tests name these crates directly, and `gdn_core` is gone.

pub mod esp;
pub mod esppatch;
pub mod espwrite;
pub mod land;
pub mod records;
pub mod tes3;

// The lower crates' modules this crate's files name as `crate::x`, resolved here so the files
// read as they did before the split. Private: none of these is this crate's to export.
use gardenfell_bytes::bytes;
