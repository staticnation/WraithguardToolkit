//! The cell viewer's engine: the install's files (VFS, archives), its load order merged
//! into a world, meshes and textures read for drawing, the sky and weather.
//!
//! Adopted from Gardenfell's engine (c) 2026 Robin Hjelte, MIT, with its grass
//! generation, scatter, painting and export removed: the viewer displays a setup, it
//! does not generate anything.

pub mod audio;
pub mod bcx;
pub mod bsa;
pub mod coverage;
pub mod distant;
pub mod esp;
pub mod rawrec;
pub mod img;
pub mod inspect;
pub mod json;
pub mod land;
pub mod lang;
pub mod lod;
pub mod layout;
pub mod luascan;
pub mod mge;
pub mod markers;
pub mod mland;
pub mod navmesh;
pub mod nif;
pub mod nifx;
pub mod npc;
pub mod objects;
pub mod plugins;
pub mod pool;
pub mod preview;
pub mod profiles;
pub mod refkey;
pub mod renderer;
pub mod review;
pub mod toml;
pub mod usage;
pub mod vfs;
pub mod vfsindex;
pub mod weather;
pub mod world;
pub mod worldmap;

/// OpenMW Lua scripts in plain Rust (viewer-shell/luacore), shared with the Python backend.
pub use luacore;
/// One spelling of a cell for the engine, the page and the Python pool (`ordercore::cells`).
pub use ::ordercore::cells;
/// The load order (rules, sort, checks, export): the Load order page's commands use it whole.
pub use ::ordercore;

/// Make Greatness7's `tes3` crates the engine's plugin and mesh readers (through
/// `gardenfell-g7`). Call once at startup, before the first read: a mesh opened on its
/// own reads no plugin first, so `World::load_with` setting it is not enough.
pub fn init() {
    let _ = gardenfell_g7::install();
}
