//! The Editor's backend (port step 4.1, `viewer-shell/PORT.md`).
//!
//! - [`blank`]: a blank record of a type, as the crate makes it (the Editor's "New").
//! - [`cache`]: the record cache on disk (`wraithguard/patch/recordcache.py`'s tables, its
//!   bodies zstd'd bytes of the caller's rather than pickles).
//! - [`columns`]: the Object Window's columns per type, and armor's weight class.
//! - [`dialogue`]: the order the engine reads a topic in, and the dialogue window
//!   (topics, a topic's responses, a response moved or added, a topic copied).
//! - [`dialogue_graph`]: the dialogue's flow, map, flags and rehearsal, the conditions in
//!   words, and who can say a response.
//! - [`edit`]: the record dialog and the changes made to records (`EditorSession.view`,
//!   `set_field`, `set_many`, `revert`, `insert`, `duplicate`, `pending`, undo).
//! - [`fields`]: what a field is (the generated kind, flag and enum tables).
//! - [`frames`]: a plugin's records and subrecords from its framing, not parsed.
//! - [`key`]: how the Editor keys a record within its type (`wraithguard/patch/records.py`'s
//!   `record_key`).
//! - [`mwscript`]: the MWScript compiler (MWEdit's, ported) and Script Edit's checks.
//! - [`places`]: placed objects, new references, path grids, and the cells and land the
//!   patch makes (`ref_view`, `set_ref_field`, `place`, `pathgrid_view`, `new_cell`, ...).
//! - [`plugin`]: plugins to and from tes3conv-schema JSON (moved from `native/`).
//! - [`pyval`]: values read and shown as the Python Editor does (its messages quote them).
//! - [`queue`]: the patch queue, its journal and the undo history (`queue.py`, the
//!   journal and history of `editor.py`).
//! - [`rename`]: an id renamed inside script source, as the compiler reads one.
//! - [`scripts`]: a script's source saved with its bytecode (the compiler handed in).
//! - [`search`]: the search by any field, the columns' overlay, and text found and
//!   replaced in every record type at once.
//! - [`session`]: a load order's records and placed objects as the Editor finds them
//!   (`EditorSession.find`, `find_ref`).
//! - [`verify`]: the verifier's checks over what the patch carries.
//! - [`uses`]: the Use Report (the files scanned, records read, or the use index) and
//!   Search & Replace over it.

pub mod blank;
pub mod build;
pub mod buildjob;
pub mod cache;
pub mod columns;
pub mod dialogue;
pub mod dialogue_graph;
pub mod edit;
pub mod fields;
pub mod frames;
pub mod key;
pub mod links;
pub mod mwscript;
pub mod places;
pub mod plugin;
pub mod pyval;
pub mod queue;
pub mod rename;
pub mod scripts;
pub mod search;
pub mod session;
pub mod testrun;
pub mod uses;
pub mod verify;

/// Runs a crate call, turning a panic (the crates assert on some malformed input rather
/// than returning an error) into an `InvalidData` error: a bad mod file must be
/// "unreadable", never a crash out of a scan.
pub fn guarded<T>(f: impl FnOnce() -> std::io::Result<T>) -> std::io::Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "the reader gave up on this file".into());
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
        }
    }
}
