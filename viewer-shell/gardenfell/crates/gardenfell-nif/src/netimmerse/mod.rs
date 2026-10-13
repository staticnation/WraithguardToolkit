//! The NetImmerse file format (NIF 4.0.0.2), as data.
//!
//! Round 18gg (`backend-plan.md` part 3). Every block of the file as a struct, read and
//! written back. Since round 18gh this is the only reader of a mesh's bytes: the engine's
//! [`crate::nif`] parses a file into a [`stream::Stream`] and builds what the viewport and
//! the scatter need from the typed blocks, so a file either reads whole or not at all. Two
//! layers:
//!
//! - [`stream`]: the file as a header line, a version, the blocks in file order and the
//!   roots footer, with links as the block indices the file holds. Parsing is strict:
//!   a block type the model does not have, a version other than 4.0.0.2, or bytes left
//!   over after the footer are errors that name the block and the byte.
//! - [`types`]: one struct per block type, all 127 the format has, each with its parent
//!   as a `base` field the way the format nests them (`NiNode` is an `NiAVObject` is an
//!   `NiObjectNET` is an `NiObject`). The layouts are Greatness7's `tes3`
//!   (`libs/nif/src/types/*.rs`), with the particle controller read in nif.xml's order
//!   (`backend-audit.md` D14, where `tes3` has the active count after the particles).
//!
//! A NIF has no subrecord sizes, so there is no lenient layer under this one: a file
//! either reads to the end or it does not, and [`check`] reports which, and whether the
//! bytes come back the same. `GardenfellCLI roundtrip --meshes` runs it over every mesh
//! of an install.

pub mod check;
pub mod stream;
pub mod types;
