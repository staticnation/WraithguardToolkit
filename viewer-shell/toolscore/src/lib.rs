//! The Tools, for the viewer's Tools page and the Python tools (port step 4.4,
//! `viewer-shell/TOOLS-PORT.md`):
//!
//! - [`backups`]: the copies the tools leave before changing a file, found, restored or
//!   deleted (`scan_backups`).
//! - [`docs`]: the program's Markdown documents as HTML, for the Help page
//!   (`wraithguard/viz/docs.py`).
//! - [`masters`]: a plugin's master removed (its references renumbered) or renamed.
//! - [`save`]: Save Check - a save's content files against the load order, and the space each
//!   mod takes in it (`openmw-save-stats`, MIT, ported whole).
//! - [`tes3cmd`]: tes3cmd launched on a plugin staged with its masters, and master sizes
//!   synced in-app.
//! - [`lint`]: the load-order lint (`wraithguard_toolkit.lint_plugins`): evil GMSTs, the fog
//!   bug, interiors without a path grid, blank headers, expansion-only script calls, missing
//!   `.omwscripts` twins.
//!
//! Written from the Python's behaviour (the reference until it retires).

pub mod backups;
pub mod docs;
pub mod lint;
pub mod lint_tables;
pub mod masters;
pub mod save;
pub mod settings;
pub mod setup;
pub mod tes3cmd;
