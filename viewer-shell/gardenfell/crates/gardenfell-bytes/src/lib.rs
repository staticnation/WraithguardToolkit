//! The bytes under every Gardenfell file format: the cursor and buffer the typed TES3 and NIF models
//! share, the code pages, JSON and TOML readers, the thread pool, and the small geometry and image helpers.
//!
//! One of the `gardenfell-*` library crates cut from `gdn_core` in round 18gj; since round 18gm the
//! front ends and the tests name these crates directly, and `gdn_core` is gone.

pub mod b64;
pub mod bytes;
pub mod codepage;
pub mod geom;
pub mod img;
pub mod ini;
pub mod json;
pub mod openmwcfg;
pub mod paths;
pub mod pool;
pub mod segdist;
pub mod toml;
pub mod triaccel;
