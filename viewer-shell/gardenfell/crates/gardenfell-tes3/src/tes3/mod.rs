//! The TES3 plugin format, as data.
//!
//! Round 18gg (`backend-plan.md` part 3). Three layers, each usable without the ones
//! above it:
//!
//! - [`bytes`]: a cursor over a byte slice and a growing buffer, little-endian numbers,
//!   Windows-1252 text. Nothing here knows what a record is.
//! - [`raw`]: a plugin as records of subrecords, exactly as the file holds them. This
//!   layer round-trips any file byte for byte, including subrecords no reader knows.
//! - [`types`]: one struct per record type (all 43 the format has, `TES3` to `INFO`),
//!   each read from a [`raw::Record`] and written back to one. The layouts follow
//!   Greatness7's `tes3` (`libs/esp/src/types/*.rs`), which is the most complete public
//!   description of the format and the one the Construction Set's own output agrees
//!   with; where the two differ, the note says so.
//!
//! Round 18gh: the engine reads plugins through these layers and nowhere else.
//! `world::parse_plugin` takes `raw::parse_plugin`'s records, types the kinds the world
//! keeps with [`types::Object::from_raw`] and fills its views (`esp.rs`, `land.rs`) from
//! the typed values; `records.rs`, `esppatch.rs` and `plugins.rs` read raw records, and
//! `espwrite.rs` writes them. The `roundtrip` command in `GardenfellCLI` runs the layers
//! over an install and reports what each one gives back, which is how the typed layer was
//! proven on real files before anything depended on it.

pub mod bytes;
pub mod check;
pub mod raw;
pub mod types;
