//! What a setup loads, read once and the same way for the viewer's engine (viewcore) and
//! the Python backend (native/): so the Editor and the viewer cannot disagree.
//!
//! - [`cfg`]: an OpenMW `openmw.cfg` - its data folders, content and groundcover plugins,
//!   archives and chained cfgs, by the rules OpenMW documents.
//! - [`records`]: which version of a record wins, and what type a placed object is.
//!
//! Written from OpenMW's documentation ("Paths", openmw.readthedocs.io) and its observed
//! behaviour, not its code: OpenMW is GPL-3 and this crate is MIT.

pub mod cfg;
pub mod records;
