//! Scratch folders in the shared temp directory that only their user can use (security audit
//! 2026-11-20, F3). A fixed name in `/tmp` is one another local user can create first, fill with
//! plugins and assets, and have a tool read back: the Test in OpenMW's scratch folder is handed to
//! OpenMW as a data folder, and the tes3cmd staging folder holds copies of plugins.
//!
//! [`private_dir`] gives a folder that is the user's own: made with mode `0700`, never an existing
//! one that is a link, someone else's or open to others (those are walked away from, to a new
//! folder with a name of its own). On Windows and macOS the temp directory is already per-user, and
//! the folder is just made.

use std::path::PathBuf;

/// `name` under the temp directory, made if need be, and the user's alone. The same path every run
/// where the folder that is there is safe (what a run leaves is found again), else a fresh one.
pub fn private_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    #[cfg(unix)]
    {
        unix::private_dir_in(&base, name)
    }
    #[cfg(not(unix))]
    {
        let path = base.join(name);
        let _ = std::fs::create_dir_all(&path);
        path
    }
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    use std::path::{Path, PathBuf};

    /// The id of the user running this: the owner of a file made now, which needs no `libc`.
    fn me(base: &Path) -> Option<u32> {
        for n in 0..8u32 {
            let probe = base.join(format!(".wg-owner-{}-{n}", std::process::id()));
            if let Ok(f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
                let uid = f.metadata().ok().map(|m| m.uid());
                let _ = std::fs::remove_file(&probe);
                return uid;
            }
        }
        None
    }

    fn make(path: &Path) -> std::io::Result<()> {
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }

    /// Whether `path` is a real folder (not a link) this user owns that nobody else can enter.
    fn ours(path: &Path, uid: u32) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0)
    }

    pub(super) fn private_dir_in(base: &Path, name: &str) -> PathBuf {
        let Some(uid) = me(base) else {
            // Nothing can be made here: the old path, which the caller's own writes will say.
            return base.join(name);
        };
        let path = base.join(format!("{name}-{uid}"));
        match make(&path) {
            Ok(()) => return path,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && ours(&path, uid) => return path,
            Err(_) => {}
        }
        // Taken by a link, another user, or a folder open to others: a fresh one, never shared.
        for n in 0..16u32 {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
            let fresh = base.join(format!("{name}-{uid}-{}-{nanos}-{n}", std::process::id()));
            if make(&fresh).is_ok() {
                return fresh;
            }
        }
        path
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn scratch(tag: &str) -> PathBuf {
            let d = std::env::temp_dir().join(format!("wg_privdir_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            d
        }

        #[test]
        fn a_new_folder_is_ours_alone_and_found_again() {
            let base = scratch("new");
            let a = private_dir_in(&base, "wg-test");
            assert!(a.is_dir());
            assert_eq!(std::fs::metadata(&a).unwrap().mode() & 0o777, 0o700);
            assert_eq!(private_dir_in(&base, "wg-test"), a, "the same folder next time");
            let _ = std::fs::remove_dir_all(&base);
        }

        #[test]
        fn a_folder_open_to_others_or_a_link_is_not_used() {
            let base = scratch("hostile");
            let uid = me(&base).unwrap();
            // Open to others (as an attacker's mkdir would leave it, were it ours).
            let open = base.join(format!("wg-test-{uid}"));
            std::fs::create_dir(&open).unwrap();
            std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
            let got = private_dir_in(&base, "wg-test");
            assert_ne!(got, open);
            assert_eq!(std::fs::metadata(&got).unwrap().mode() & 0o777, 0o700);
            // A link where the folder should be: not followed.
            let link_name = "wg-link";
            let target = base.join("elsewhere");
            std::fs::create_dir(&target).unwrap();
            std::os::unix::fs::symlink(&target, base.join(format!("{link_name}-{uid}"))).unwrap();
            let got = private_dir_in(&base, link_name);
            assert_ne!(got, base.join(format!("{link_name}-{uid}")));
            assert!(got.is_dir() && !std::fs::symlink_metadata(&got).unwrap().file_type().is_symlink());
            let _ = std::fs::remove_dir_all(&base);
        }
    }
}
