//! Merge to master: greatness7's merge_to_master (public domain) as a library, as 4.x's Tk
//! window had it (through `native/src/merge.rs`, retired with the Python in 5.0).
//!
//! - [`merge_into_master`]: a plugin folded into its master (its last master; one it does
//!   not list is added as its last), the result the merged master.
//! - [`merge_order`]: a whole load order (paths in load order) folded into one master, as
//!   the game would resolve it; the plugins are read in parallel.
//!
//! Each returns the merged master as plugin bytes; [`write_merged`] puts them on disk, the
//! file it replaces kept once as `.premerge.bak`. A source plugin is never written.

use std::path::{Path, PathBuf};

pub use merge_to_master::MergeOptions;

/// The crate's panics, as errors (a malformed plugin can make it give up).
fn guarded(f: impl FnOnce() -> anyhow::Result<merge_to_master::PluginData>) -> Result<Vec<u8>, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f().and_then(|m| Ok(m.into_plugin().save_bytes()?)))) {
        Ok(r) => r.map_err(|e| format!("{e:#}")),
        Err(_) => Err("the merge gave up on these files".into()),
    }
}

/// `plugin` merged into `master`: the merged master's bytes.
pub fn merge_into_master(plugin: &Path, master: &Path, options: MergeOptions) -> Result<Vec<u8>, String> {
    let (p, m) = (plugin.to_path_buf(), master.to_path_buf());
    guarded(move || merge_to_master::merge_plugins(&p, &m, options))
}

/// Every plugin of `order` (in load order) merged into one master: its bytes.
pub fn merge_order(order: &[PathBuf], options: MergeOptions) -> Result<Vec<u8>, String> {
    if order.is_empty() {
        return Err("no plugins to merge".into());
    }
    guarded(move || merge_to_master::par_merge_load_order(order, options))
}

/// `bytes` written to `out` through a `.tmp`; a file already there is kept once as
/// `<name>.premerge.bak` (not overwritten by a later merge). Answers the backup's path.
pub fn write_merged(out: &Path, bytes: &[u8]) -> std::io::Result<Option<PathBuf>> {
    let mut backup = None;
    if out.is_file() {
        let bak = PathBuf::from(format!("{}.premerge.bak", out.display()));
        if !bak.exists() {
            std::fs::copy(out, &bak)?;
        }
        backup = Some(bak);
    }
    let tmp = PathBuf::from(format!("{}.tmp", out.display()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, out)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/tests/fixture/Data Files/");

    #[test]
    fn a_plugin_folds_into_its_master_and_the_master_is_kept() {
        let dir = std::env::temp_dir().join(format!("wg_m2m_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The fixture's plugins are all masters: Lamp.esm folded into LandVanilla.esm (its
        // Morrowind.esm is a stub the crate's strict reader refuses).
        let (plugin, master) = (PathBuf::from(DATA).join("Lamp.esm"), PathBuf::from(DATA).join("LandVanilla.esm"));
        let o = MergeOptions { remove_deleted: false, apply_moved_references: false, preserve_duplicate_references: false };
        let bytes = merge_into_master(&plugin, &master, o).unwrap();
        assert!(bytes.starts_with(b"TES3"));
        let out = dir.join("merged.esm");
        assert!(write_merged(&out, &bytes).unwrap().is_none());
        assert_eq!(write_merged(&out, &bytes).unwrap(), Some(PathBuf::from(format!("{}.premerge.bak", out.display()))));
        let order = vec![master.clone(), plugin.clone()];
        assert!(merge_order(&order, o).unwrap().starts_with(b"TES3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
