//! Wraithguard's Rust backend: Python bindings over greatness7's tes3 crates.
//!
//! One submodule per piece of the toolkit it replaces; each keeps the Python API
//! of the port it stands in for (the thin wrappers in `wraithguard/`), so a piece
//! can move here without its callers changing.
//!
//! - `bsa`: `.bsa` archives (was `wraithguard/nif/bsa.py`).
//! - `esp`: plugins to tes3conv-schema JSON (was `wraithguard.esp` + `record_to_json`
//!   in the toolkit's native session), and plugins read and written for wraithguard/esp.
//! - `land`: the numeric core of Merged Lands (`wraithguard/land/`): the relative
//!   grids, the per-vertex merge, the slope limiter, normals and height decoding.
//! - `lua`: OpenMW Lua scripts: the API read from the setup's OpenMW install, Teal
//!   declarations written from it, and scripts checked by the Teal compiler (htl).
//! - `merge`: whole-plugin merging, greatness7's merge_to_master (vendored) called as a
//!   library (was the merge in `wraithguard/merge/`).
//! - `nif`: mesh summaries, the mesh viewer's block panel and field edits (was
//!   wraithguard/nif's reader, geometry and editor).
//! - `uses`: the editor's Use Report, every record of a load order naming an id (was
//!   `wraithguard/patch/uses.py`'s scan).

use pyo3::prelude::*;

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

/// Quiets the default panic message for the panics `guarded` turns into errors.
pub fn quiet_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::panic::set_hook(Box::new(|_| {})));
}

pub mod bsa;
pub mod esp;
pub mod img;
pub mod land;
pub mod lint;
pub mod lua;
pub mod merge;
pub mod nif;
pub mod uses;

/// The extension module. `gil_used = false`: nothing here relies on the GIL
/// (every class is immutable after construction), so free-threaded Python keeps
/// its threads.
#[pymodule(gil_used = false)]
fn wraithguard_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    quiet_panics();
    bsa::register(m)?;
    esp::register(m)?;
    nif::register(m)?;
    land::register(m)?;
    img::register(m)?;
    lint::register(m)?;
    lua::register(m)?;
    merge::register(m)?;
    uses::register(m)?;
    Ok(())
}
