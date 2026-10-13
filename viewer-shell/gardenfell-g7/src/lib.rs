//! Greatness7's `tes3` crates reading for Gardenfell's engine.
//!
//! Robin Hjelte's engine (`viewer-shell/gardenfell`) reads plugins through its own typed
//! model, whose layouts are G7's `tes3` ported to std-only Rust. This crate lets G7's crate
//! do the reading instead and converts what it read into Robin's types, so the engine runs
//! unchanged on G7's parse: `install()` sets the engine's record hook
//! (`gardenfell_world::world::set_record_typer`, patch 0001) and its mesh hook
//! (`gardenfell_nif::nif::set_stream_reader`, patch 0002).
//!
//! Where the two differ the conversion takes the raw record's word for what G7's types do
//! not keep (see [`records`]): which optional subrecords were there at all, the order of a
//! cell's references, and an `MVRF` that names another reference than the one after it.
//! The code page Robin's engine reads in (Russian, Polish installs) is applied to G7's text,
//! which it always decodes as Windows-1252.

pub mod meshes;
pub mod records;

/// Make G7's crate the engine's record reader. Call once, before the first plugin is read.
/// False when a reader was already set.
pub fn install() -> bool {
    let recs = gardenfell_world::world::set_record_typer(records::type_record).is_ok();
    let meshes = gardenfell_nif::nif::set_stream_reader(meshes::read_stream).is_ok();
    recs && meshes
}
