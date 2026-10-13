//! NetImmerse 4.0.0.2 meshes: the block stream and the typed model of every block (`netimmerse`), and the
//! engine's views of a mesh (`nif`: hull, draw parts, lights, particles, animation) with the mesh scanner.
//!
//! One of the `gardenfell-*` library crates cut from `gdn_core` in round 18gj; since round 18gm the
//! front ends and the tests name these crates directly, and `gdn_core` is gone.

pub mod meshscan;
pub mod netimmerse;
pub mod nif;
pub mod triorder;

// The lower crates' modules this crate's files name as `crate::x`, resolved here so the files
// read as they did before the split. Private: none of these is this crate's to export.
use gardenfell_bytes::{bytes, paths, toml, triaccel};
