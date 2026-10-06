//! The cell viewer's engine: the install's files (VFS, archives), its load order merged
//! into a world, meshes and textures read for drawing, the sky and weather.
//!
//! Adopted from Gardenfell's engine (c) 2026 Robin Hjelte, MIT, with its grass
//! generation, scatter, painting and export removed: the viewer displays a setup, it
//! does not generate anything.

pub mod bcx;
pub mod bsa;
pub mod coverage;
pub mod esp;
pub mod img;
pub mod inspect;
pub mod json;
pub mod land;
pub mod lang;
pub mod layout;
pub mod luascan;
pub mod mge;
pub mod markers;
pub mod mland;
pub mod navmesh;
pub mod nif;
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
pub mod weather;
pub mod world;
pub mod worldmap;

/// OpenMW Lua scripts in plain Rust (viewer-shell/luacore), shared with the Python backend.
pub use luacore;
