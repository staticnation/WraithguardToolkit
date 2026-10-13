//! Gardenfell's `nif`: Robin Hjelte's, from `gardenfell-nif` (viewer-shell/gardenfell,
//! vendored; gloss maps, embedded texture names, bone names and the skeleton module are
//! carried patch 0008), re-exported so viewcore's callers keep their paths, with
//! Wraithguard's additions ([`crate::nifx`]).

pub use crate::nifx::*;
pub use gardenfell_nif::nif::skeleton::*;
pub use gardenfell_nif::nif::*;
