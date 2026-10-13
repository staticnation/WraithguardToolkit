//! Gardenfell's `vfs`: Robin Hjelte's, from `gardenfell-install` (viewer-shell/gardenfell,
//! vendored; carried patches in its `patches/`), re-exported so viewcore's callers keep
//! their paths. Below it, what Wraithguard adds.

pub use gardenfell_install::vfs::*;

/// A path as the overlay keys it (lower case, forward slashes): Robin's, from
/// `gardenfell_bytes::paths`. `Vfs::providers` is carried patch 0003 in his crate.
pub use gardenfell_bytes::paths::norm;
