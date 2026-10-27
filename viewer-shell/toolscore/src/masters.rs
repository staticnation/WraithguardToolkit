//! Remove or rename a plugin's master (port step 4.4, slice 4): `wraithguard/esp/masters.py`
//! (`remove_master_from_bytes`, `rename_master_in_bytes`), the rules behind the Remove /
//! Rename Master window.
//!
//! OpenMW refuses a plugin whose header lists a master it cannot find. Removing the entry is
//! the fix when the master is no longer needed, but a placed reference records which master
//! defined it by the master's 1-based position: drop one and every later master shifts down,
//! so those references are renumbered, and a reference into the removed master no longer
//! resolves and is dropped (Wrye Mash's master remap). Renaming keeps every position: only the
//! header's name and recorded size change.
//!
//! Only the header and the CELL records are read and written again (by the tes3 crate, as the
//! Editor writes them); every other record is copied byte for byte, so record types the crate
//! does not model and the record order survive.

use std::io;

use tes3::esp::TES3Object;

/// What removing a master did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoveReport {
    /// The master as the header spelled it (as asked, when it is not there).
    pub master: String,
    pub removed: bool,
    /// References into the removed master, dropped.
    pub references_dropped: usize,
    /// Those per cell (its name, or an exterior's `(x, y)`), in record order.
    pub dropped_by_cell: Vec<(String, usize)>,
    /// References into a later master, renumbered.
    pub references_remapped: usize,
    /// The master list after.
    pub remaining_masters: Vec<String>,
}

/// What renaming a master did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenameReport {
    pub old_name: String,
    pub new_name: String,
    pub renamed: bool,
    pub remaining_masters: Vec<String>,
}

fn bad(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// The records of a plugin, `(start, end, tag)` of each in the bytes.
fn spans(bytes: &[u8]) -> io::Result<Vec<(usize, usize, [u8; 4])>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes.len() - pos < 16 {
            return Err(bad(format!("a record header at {pos} runs past the end of the file")));
        }
        let tag: [u8; 4] = bytes[pos..pos + 4].try_into().expect("4 bytes");
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().expect("4 bytes")) as usize;
        let end = pos.checked_add(16 + size).filter(|&e| e <= bytes.len()).ok_or_else(|| {
            bad(format!("the {} record at {pos} runs past the end of the file", String::from_utf8_lossy(&tag)))
        })?;
        out.push((pos, end, tag));
        pos = end;
    }
    Ok(out)
}

fn load(bytes: &[u8]) -> io::Result<TES3Object> {
    editcore::guarded(|| bytes_io::Reader::new(bytes).load())
}

fn save(obj: &TES3Object, out: &mut Vec<u8>) -> io::Result<()> {
    let mut w = bytes_io::Writer::new(Vec::new());
    editcore::guarded(|| w.save(obj))?;
    out.extend(w.cursor.into_inner());
    Ok(())
}

/// The plugin with its header and its CELL records passed through `edit`; the rest copied.
fn rewrite(bytes: &[u8], mut edit: impl FnMut(&mut TES3Object)) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    for (start, end, tag) in spans(bytes)? {
        if &tag == b"TES3" || &tag == b"CELL" {
            let mut obj = load(&bytes[start..end])?;
            edit(&mut obj);
            save(&obj, &mut out)?;
        } else {
            out.extend_from_slice(&bytes[start..end]);
        }
    }
    Ok(out)
}

/// The header's masters (`(name, size)`).
pub fn masters(bytes: &[u8]) -> io::Result<Vec<(String, u64)>> {
    let Some(&(start, end, tag)) = spans(bytes)?.first() else { return Ok(Vec::new()) };
    if &tag != b"TES3" {
        return Ok(Vec::new());
    }
    match load(&bytes[start..end])? {
        TES3Object::Header(h) => Ok(h.masters),
        _ => Ok(Vec::new()),
    }
}

fn position(masters: &[(String, u64)], name: &str) -> Option<usize> {
    let want = name.to_lowercase();
    masters.iter().position(|(n, _)| n.to_lowercase() == want).map(|i| i + 1)
}

/// `master` removed from the plugin's header and its references renumbered
/// (`remove_master_from_bytes`). When the master is not there the bytes come back as they were.
pub fn remove_master(bytes: &[u8], master: &str) -> io::Result<(Vec<u8>, RemoveReport)> {
    let list = masters(bytes)?;
    let Some(pos) = position(&list, master) else {
        let report = RemoveReport { master: master.to_string(), remaining_masters: list.into_iter().map(|(n, _)| n).collect(), ..Default::default() };
        return Ok((bytes.to_vec(), report));
    };
    let mut report = RemoveReport { master: list[pos - 1].0.clone(), removed: true, ..Default::default() };
    let pos32 = u32::try_from(pos).map_err(|_| bad("too many masters"))?;
    let out = rewrite(bytes, |obj| match obj {
        TES3Object::Header(h) => {
            h.masters.remove(pos - 1);
            report.remaining_masters = h.masters.iter().map(|(n, _)| n.clone()).collect();
        }
        TES3Object::Cell(c) => {
            let old = std::mem::take(&mut c.references);
            let mut dropped = 0usize;
            // In the order the cell wrote them, so a rebuilt cell keeps it.
            let mut refs: Vec<((u32, u32), tes3::esp::Reference)> = old.into_iter().collect();
            refs.sort_by_key(|(k, _)| *k);
            for ((mast, refr), mut r) in refs {
                if mast == pos32 {
                    dropped += 1;
                    continue;
                }
                let mast = if mast > pos32 {
                    report.references_remapped += 1;
                    r.mast_index = mast - 1;
                    mast - 1
                } else {
                    mast
                };
                c.references.insert((mast, refr), r);
            }
            if dropped > 0 {
                let label = if c.name.is_empty() { format!("({}, {})", c.data.grid.0, c.data.grid.1) } else { c.name.clone() };
                report.references_dropped += dropped;
                report.dropped_by_cell.push((label, dropped));
            }
        }
        _ => {}
    })?;
    Ok((out, report))
}

/// The master `old` pointed at `new` (recording `new_size`), every reference left as it is
/// (`rename_master_in_bytes`). Refused when `new` is already another master of the plugin.
pub fn rename_master(bytes: &[u8], old: &str, new: &str, new_size: u64) -> io::Result<(Vec<u8>, RenameReport)> {
    let list = masters(bytes)?;
    let mut report = RenameReport { old_name: old.to_string(), new_name: new.to_string(), ..Default::default() };
    let Some(pos) = position(&list, old) else {
        report.remaining_masters = list.into_iter().map(|(n, _)| n).collect();
        return Ok((bytes.to_vec(), report));
    };
    if position(&list, new).is_some_and(|c| c != pos) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("'{new}' is already a master of this plugin.")));
    }
    let out = rewrite(bytes, |obj| {
        if let TES3Object::Header(h) = obj {
            h.masters[pos - 1] = (new.to_string(), new_size);
            report.remaining_masters = h.masters.iter().map(|(n, _)| n.clone()).collect();
        }
    })?;
    report.renamed = true;
    Ok((out, report))
}

/// The one-time backup a master change keeps beside the plugin (`name.premaster.bak`).
pub fn backup_path(plugin: &std::path::Path) -> std::path::PathBuf {
    let mut name = plugin.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".premaster.bak");
    plugin.with_file_name(name)
}

/// Writes `bytes` over `plugin`, after copying the original to its one-time backup when there is
/// none yet. Answers the backup's path.
fn write_with_backup(plugin: &std::path::Path, bytes: &[u8]) -> io::Result<std::path::PathBuf> {
    let backup = backup_path(plugin);
    if !backup.exists() {
        std::fs::copy(plugin, &backup)?;
    }
    std::fs::write(plugin, bytes)?;
    Ok(backup)
}

/// [`remove_master`] on a plugin file: written (with its backup) unless `dry_run` or nothing
/// changed. Answers the report and the backup, when one was written.
pub fn remove_master_file(plugin: &std::path::Path, master: &str, dry_run: bool) -> io::Result<(RemoveReport, Option<std::path::PathBuf>)> {
    let (out, report) = remove_master(&std::fs::read(plugin)?, master)?;
    let backup = if report.removed && !dry_run { Some(write_with_backup(plugin, &out)?) } else { None };
    Ok((report, backup))
}

/// [`rename_master`] on a plugin file, `old` pointed at the file `new` (its name and its size on
/// disk). Answers the report and the backup, when one was written.
pub fn rename_master_file(plugin: &std::path::Path, old: &str, new: &std::path::Path) -> io::Result<(RenameReport, Option<std::path::PathBuf>)> {
    let name = new.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| bad("no file name"))?;
    let size = std::fs::metadata(new)?.len();
    let (out, report) = rename_master(&std::fs::read(plugin)?, old, &name, size)?;
    let backup = if report.renamed { Some(write_with_backup(plugin, &out)?) } else { None };
    Ok((report, backup))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tes3::esp::{Cell, Header, Reference};

    fn plugin() -> Vec<u8> {
        let mut out = Vec::new();
        let header = Header { masters: vec![("A.esm".into(), 1), ("B.esm".into(), 2), ("C.esp".into(), 3)], ..Default::default() };
        save(&TES3Object::Header(header), &mut out).unwrap();
        let mut cell = Cell { name: "Room".into(), ..Default::default() };
        for (m, r) in [(0u32, 1u32), (1, 5), (2, 6), (2, 7), (3, 8)] {
            cell.references.insert((m, r), Reference { mast_index: m, refr_index: r, id: format!("x{m}{r}"), ..Default::default() });
        }
        save(&TES3Object::Cell(cell), &mut out).unwrap();
        // A record the rewrite does not read, copied as it is.
        out.extend(b"ZZZZ\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00abc");
        out
    }

    #[test]
    fn removing_a_master_drops_its_references_and_renumbers_the_later_ones() {
        let (out, r) = remove_master(&plugin(), "b.ESM").unwrap();
        assert!(r.removed);
        assert_eq!(r.master, "B.esm");
        assert_eq!(r.remaining_masters, ["A.esm", "C.esp"]);
        assert_eq!((r.references_dropped, r.references_remapped), (2, 1));
        assert_eq!(r.dropped_by_cell, [("Room".to_string(), 2)]);
        assert!(out.ends_with(b"ZZZZ\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00abc"));
        let spans = spans(&out).unwrap();
        let TES3Object::Cell(c) = load(&out[spans[1].0..spans[1].1]).unwrap() else { panic!("not a cell") };
        let mut keys: Vec<(u32, u32)> = c.references.keys().copied().collect();
        keys.sort();
        assert_eq!(keys, [(0, 1), (1, 5), (2, 8)]);
        assert_eq!(c.references[&(2, 8)].mast_index, 2);
        let (same, r) = remove_master(&plugin(), "Nope.esm").unwrap();
        assert!(!r.removed);
        assert_eq!(same, plugin());
    }

    #[test]
    fn renaming_keeps_the_position_and_refuses_a_duplicate() {
        let (out, r) = rename_master(&plugin(), "C.esp", "D.esp", 9).unwrap();
        assert!(r.renamed);
        assert_eq!(masters(&out).unwrap()[2], ("D.esp".to_string(), 9));
        assert!(rename_master(&plugin(), "C.esp", "a.esm", 9).is_err());
    }
}
