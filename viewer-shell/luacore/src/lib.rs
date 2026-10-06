//! OpenMW Lua scripts, read and checked in plain Rust: shared by the Python backend
//! (`native/`, which binds it for `wraithguard/lua/`) and the viewer's engine (`viewcore`,
//! for the Editor), so neither needs the other to check a load order's scripts.
//!
//! - [`lexer`], [`parser`]: Lua 5.1/LuaJIT source (Teal's types read and set aside).
//! - [`analysis`]: what one script declares, and what it does every frame.
//! - [`omwscripts`], [`scan`]: a load order's scripts found, read and checked together.
//! - [`api`]: what each OpenMW release allows where.
//! - [`ldt`], [`teal`]: an install's API documentation read, Teal declarations written.
//! - [`findings`]: Teal's diagnostics as findings, and where a setup's Teal pieces are.
//! - [`report`], [`mermaid`]: the report text, flowcharts and call graphs.
//!
//! The Teal checker itself (an embedded Lua running tl) stays in `native/`: it is the one
//! heavy dependency, and only the Python side runs it so far.
//!
//! Each module was a Python one in `wraithguard/lua/`, which stays as the fallback and
//! the reference this is held to (tests on both sides).

pub mod analysis;
pub mod api;
pub mod findings;
pub mod ldt;
pub mod lexer;
pub mod mermaid;
pub mod omwscripts;
pub mod parser;
pub mod report;
pub mod scan;
pub mod teal;

use std::io;
use std::path::Path;

/// Runs a crate call, turning a panic (tes3 asserts on some malformed input rather than
/// returning an error) into an `InvalidData` error: a bad mod file is "unreadable",
/// never a crash out of a scan.
pub fn guarded<T>(f: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "the reader gave up on this file".into());
            Err(io::Error::new(io::ErrorKind::InvalidData, msg))
        }
    }
}

/// The scripts a content file's bytes register in its LUAL records (OpenMW's
/// `LuaScriptsCfg`), read with the tes3 crate's `ScriptConfigList`: `[(vfs path, flags
/// word, record types, per object)]` in record order - `per object` when it is attached
/// to particular records or references (LUAR/LUAI). Only LUAL records are read.
pub fn lual_scripts_of(bytes: &[u8]) -> io::Result<Vec<(String, u32, Vec<String>, bool)>> {
    use tes3::esp::{Plugin, TES3Object};
    let mut plugin = Plugin::new();
    guarded(|| plugin.load_bytes_filtered(bytes, |t| &t == b"LUAL"))?;
    let mut out = Vec::new();
    for obj in &plugin.objects {
        if let TES3Object::ScriptConfigList(list) = obj {
            for sc in &list.scripts {
                let per_object = !sc.records.is_empty() || !sc.instances.is_empty();
                out.push((sc.path.clone(), sc.flags.bits(), sc.types.clone(), per_object));
            }
        }
    }
    Ok(out)
}

/// [`lual_scripts_of`] for a file.
pub fn read_lual_scripts(path: &Path) -> io::Result<Vec<(String, u32, Vec<String>, bool)>> {
    lual_scripts_of(&std::fs::read(path)?)
}

/// Whether a LUAL read failed on the file (OSError to Python) rather than its data.
pub fn lual_error_is_os(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied)
}
