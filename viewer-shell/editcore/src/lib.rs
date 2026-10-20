//! The Editor's backend (port step 4.1, `viewer-shell/PORT.md`).
//!
//! - [`plugin`]: plugins to and from tes3conv-schema JSON (moved from `native/`).
//! - [`key`]: how the Editor keys a record within its type (`wraithguard/patch/records.py`'s
//!   `record_key`).
//! - [`session`]: a load order's records and placed objects as the Editor finds them
//!   (`EditorSession.find`, `find_ref`).

pub mod key;
pub mod plugin;
pub mod session;

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
