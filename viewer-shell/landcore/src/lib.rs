//! The landscape, read one way for the viewer's Tools page and the Python tools (port step
//! 4.4, `viewer-shell/TOOLS-PORT.md` slices 9 and 10):
//!
//! - [`meta`]: Merge Settings - the per-plugin `.mergedlands.toml` read, written and checked
//!   (`wraithguard/land/meta.py`, itself from Merged Lands' `io/meta_schema.rs`, MIT, David
//!   Von Derau).
//! - Merge Lands (`wraithguard/land/`, our port of Merged Lands and its OpenMW fork, with our
//!   own fixes), module for module: [`read`] the terrain out of a plugin, [`layers`] a cell
//!   decoded and diffed, [`textures`] the shared land-texture table, [`landmass`] the
//!   reference and each plugin's changes, [`merge`] one layer of two plugins, [`grid`] and
//!   [`curvature`] what that runs on, [`pipeline`] the whole merge, [`seams`], [`slope`] and
//!   [`cleaning`] its finishing passes, [`heights`] and [`emit`] the records written, and
//!   [`service`] all of it as one call with the progress said line by line; [`preview`] what
//!   each strategy would do to one cell.
//!
//! Written from the Python's behaviour and pinned against its output: the same load order
//! gives the same `Merged Lands.esp`, byte for byte (`tests/merge_cases.rs`). The Python's
//! rounding is kept to the bit (`round` half to even, `int()` toward zero, `//` down).

pub mod cleaning;
pub mod curvature;
pub mod emit;
pub mod grid;
pub mod heights;
pub mod landmass;
pub mod layers;
pub mod merge;
pub mod meta;
pub mod pipeline;
pub mod preview;
pub mod read;
pub mod seams;
pub mod service;
pub mod slope;
pub mod textures;
