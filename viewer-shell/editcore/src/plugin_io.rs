//! Wraithguard: plugins mapped and plugins written, kept apart (security audit F7).
//!
//! A plugin read for a scan is memory-mapped (`session::with_file_bytes`, `uses::report`): the
//! OS keeps one copy of a file of hundreds of MB. Reading a map whose file was cut short under
//! it is not an error but a crash (SIGBUS on Linux and macOS), so a plugin is never truncated in
//! place while one could be mapped:
//!
//! - every write of a plugin is whole ([`write_whole`]): a file beside it, then a rename over it.
//!   On Unix the rename leaves a mapped file's old contents where the map reads them; on Windows
//!   a rename onto a mapped file fails, which is an error, not a crash;
//! - in this process a write waits for the scans and a scan for the writes ([`reading`],
//!   [`writing`]): a tes3cmd run, which edits plugins in place, takes [`writing`] for its run.
//!
//! Another program changing a plugin while it is mapped (an editor, a mod manager) is not
//! covered: nothing in this process can hold it off.

use std::path::Path;
use std::sync::RwLock;

static IO: RwLock<()> = RwLock::new(());

/// `f` while no plugin is being written by this process (many may read at once).
pub fn reading<T>(f: impl FnOnce() -> T) -> T {
    let _g = IO.read().unwrap_or_else(|e| e.into_inner());
    f()
}

/// `f` while no plugin is mapped or written by anything else in this process.
pub fn writing<T>(f: impl FnOnce() -> T) -> T {
    let _g = IO.write().unwrap_or_else(|e| e.into_inner());
    f()
}

/// `bytes` put at `path` whole, under [`writing`]: written beside it (`.<name>.wg-new`), then
/// renamed over it, so the file is never cut short in place and a write stopped part way leaves
/// the old one. The temporary file is removed when anything fails.
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    writing(|| {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "plugin".into());
        let tmp = path.with_file_name(format!(".{name}.wg-new"));
        let r = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
        if r.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        r
    })
}

#[cfg(test)]
mod tests {
    /// A mapped plugin rewritten whole: the map still reads what it was mapped over (no SIGBUS),
    /// and the file is the new one. A write waits for a scan, under the lock.
    #[test]
    fn a_whole_write_leaves_a_live_map_readable() {
        let d = std::env::temp_dir().join(format!("wg-plugin-io-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("Mod.esp");
        std::fs::write(&p, vec![7u8; 1 << 20]).unwrap();
        let f = std::fs::File::open(&p).unwrap();
        // SAFETY: the test's own file, read only.
        let map = unsafe { memmap2::Mmap::map(&f) }.unwrap();
        #[cfg(unix)]
        {
            super::write_whole(&p, b"short").unwrap();
            assert_eq!(std::fs::read(&p).unwrap(), b"short");
            assert!(map.iter().all(|b| *b == 7), "the map reads what it was mapped over");
        }
        #[cfg(windows)]
        {
            // A rename onto a mapped file is refused: an error, not a crash.
            let _ = super::write_whole(&p, b"short");
            assert!(map.iter().all(|b| *b == 7));
        }
        drop(map);
        // The lock: a write waits while a scan holds it.
        let started = std::time::Instant::now();
        std::thread::scope(|s| {
            s.spawn(|| super::reading(|| std::thread::sleep(std::time::Duration::from_millis(300))));
            std::thread::sleep(std::time::Duration::from_millis(50));
            super::writing(|| ());
        });
        assert!(started.elapsed() >= std::time::Duration::from_millis(250));
        let _ = std::fs::remove_dir_all(&d);
    }
}
