//! Merging a generated groundcover plugin into a **copy** of an existing one.
//!
//! Robin, of the Advanced export scope: "I don't want to write to the existing .esp, but
//! instead make a copy of it, with the rules and cells I have chosen in the dialogue to be
//! added statics to the .esp file. [...] one could see it as you create a new, merged .esp
//! with the existing records in that .esp file and the added scattered statics."
//!
//! So nothing here writes anywhere. It takes the bytes of a plugin somebody already has,
//! the bytes of a plugin this program just generated, and returns a third set of bytes.
//! The source file is opened read-only and is never the output path — the caller asks
//! where the copy goes.
//!
//! **The source's header is kept whole**, masters and all (Robin's choice). A patched copy
//! is somebody else's plugin with grass added, not a Gardenfell plugin that happens to
//! contain their records: rewriting the author or dropping a master would either mislabel
//! it or break it outright. The one field that has to move is HEDR's record count, because
//! records are being added and a wrong count is what makes some tools call a file corrupt.
//!
//! Three rules decide the merge, and all three are about not disturbing what is there:
//!
//! - **A cell the source already has takes the new references appended to its own record.**
//!   Not a second CELL record for the same place — a plugin holding two of those is
//!   malformed, and which one the game believed would be a coin toss. The source's own
//!   header subrecords (its lighting, its water, its region) are untouched: only the
//!   reference block grows.
//! - **New references are numbered above every number the source uses itself.** FRMR's top
//!   byte is a 1-based index into the writing plugin's master list, so a reference the
//!   plugin created has a top byte of zero; those are the only ones that can collide with
//!   ours. Numbering from one — what a fresh plugin does — would give two objects in the
//!   same cell the same number, and the game keys references by that number.
//! - **A STAT id the source already defines is left alone**, and the references simply
//!   point at it. Two records with one id is the same conflict at object scale; and if the
//!   source is a plugin this tool patched last week, its `GRS_` records are already the
//!   ones we would be writing.
//!
//! Round 18gh: both plugins are read with `tes3::raw::parse_plugin` into records of
//! subrecords, the cells are keyed through the typed `Cell`, the grass is spliced in as
//! subrecords, and the whole thing is written back with `raw::write_plugin`. The raw layer
//! is byte-exact, so every record this does not touch comes out as it went in, and a
//! source record this program does not understand still travels whole.

use crate::tes3::bytes::{decode, Reader};
use crate::tes3::raw::{self, Record, Sub};
use crate::tes3::types::{Cell, Typed};

/// What a patch did, for the dialogue to say out loud.
#[derive(Default, Debug, Clone)]
pub struct PatchReport {
    /// Cells the source already had, which took new references.
    pub cells_extended: usize,
    /// Cells the source did not have, appended whole.
    pub cells_added: usize,
    /// STAT records appended.
    pub statics_added: usize,
    /// STAT ids the source already defined, so the references use its own.
    pub statics_shared: usize,
    /// References added, over every cell.
    pub refs: usize,
    /// The first number given to a new reference. One past the highest the source used.
    pub first_frmr: u32,
}

/// Where a cell lives, as a key both files can be looked up by.
///
/// An exterior is its grid; an interior is its name, lowercased because a load order is
/// full of files that disagree about capitalisation and Morrowind does not care.
fn cell_key(c: &Cell) -> String {
    if c.is_interior() {
        format!("i:{}", c.name.to_lowercase())
    } else {
        format!("e:{},{}", c.data.grid.0, c.data.grid.1)
    }
}

/// The records of a plugin, or why it is not one. The first has to be the TES3 header.
fn records(buf: &[u8]) -> Result<Vec<Record<'_>>, String> {
    let recs = raw::parse_plugin(buf).map_err(|e| format!("{e} — it is truncated or not a TES3 plugin"))?;
    if recs.first().map(|r| &r.tag) != Some(b"TES3") {
        return Err("that file does not start with a TES3 header, so it is not a Morrowind plugin".into());
    }
    Ok(recs)
}

/// A CELL record as the typed model reads it, or why it would not.
///
/// A cell the model refuses is a malformed record, and the merge cannot say which place it
/// is without reading it, so the answer is the model's error, naming the subrecord and the
/// byte, rather than a guess that might double the grass.
fn cell_of(rec: &Record) -> Result<Cell, String> {
    Cell::from_raw(rec).map_err(|e| format!("a CELL record in the plugin could not be read: {e}"))
}

/// The NAME of a record, as its id.
fn record_name(rec: &Record) -> String {
    rec.sub(b"NAME").map(|s| decode(&s.data)).unwrap_or_default()
}

/// Where the references begin in a CELL record: the first `FRMR` or `MVRF`.
fn refs_at(rec: &Record) -> usize {
    rec.subs.iter().position(|s| matches!(&s.tag, b"FRMR" | b"MVRF")).unwrap_or(rec.subs.len())
}

/// The reference block of a CELL record: every subrecord from the first `FRMR` or `MVRF`.
fn ref_subs<'a, 'b>(rec: &'a Record<'b>) -> &'a [Sub<'b>] {
    rec.subs.get(refs_at(rec)..).unwrap_or(&[])
}

/// How many references a reference block holds, by counting its `FRMR`s.
fn count_frmr(refs: &[Sub]) -> u32 {
    refs.iter().filter(|s| &s.tag == b"FRMR").count() as u32
}

/// Copies a reference block, giving every FRMR a fresh number from `next`.
///
/// Everything that is not an FRMR is copied as it is — a reference carries subrecords this
/// program never writes (ownership, a lock level, a script) and a merge that only understood
/// its own output would quietly drop them.
fn renumber<'a>(refs: &[Sub<'a>], next: &mut u32, out: &mut Vec<Sub<'a>>) -> usize {
    let mut n = 0usize;
    for s in refs {
        if &s.tag == b"FRMR" && s.data.len() == 4 {
            out.push(Sub::new(b"FRMR", next.to_le_bytes().to_vec()));
            *next += 1;
            n += 1;
        } else {
            out.push(s.clone());
        }
    }
    n
}

/// One little-endian dword out of a subrecord, where there is one.
fn dword(s: &Sub) -> Option<u32> {
    Reader::new(&s.data).u32().ok()
}

/// HEDR's record count: `version, file type, author[32], description[256], record count`,
/// the last four of its 300 bytes. Found by walking to the subrecord rather than by
/// measuring from the end of the record, because masters follow it and "four bytes before
/// the end" would patch the last master's file size instead. That exact mistake is why
/// [`crate::espwrite::Writer::header`] hands its own offset back.
const HEDR_COUNT_AT: usize = 296;

fn hedr_count(head: &Record) -> Option<u32> {
    let s = head.sub(b"HEDR")?;
    let bytes = s.data.get(HEDR_COUNT_AT..HEDR_COUNT_AT + 4)?;
    Reader::new(bytes).u32().ok()
}

/// Writes the count back into HEDR, and nothing else of the header.
fn set_hedr_count(head: &mut Record, n: u32) {
    for s in head.subs.iter_mut() {
        if &s.tag == b"HEDR" {
            if let Some(slot) = s.data.to_mut().get_mut(HEDR_COUNT_AT..HEDR_COUNT_AT + 4) {
                slot.copy_from_slice(&n.to_le_bytes());
            }
            return;
        }
    }
}

/// Merges `added` into a copy of `source`.
///
/// `added` is a plugin this program generated: a TES3 header, STAT records, and CELL
/// records carrying nothing but a name, the grid or the interior flag, and references.
/// Its header is discarded — the copy keeps the source's.
pub fn patch(source: &[u8], added: &[u8]) -> Result<(Vec<u8>, PatchReport), String> {
    let mut src = records(source)?;
    let add = records(added)?;
    let mut rep = PatchReport::default();

    // Where each of the source's cells is, what it already defines, and the highest
    // reference number it gave to a reference of its **own**. Only those can collide: a
    // top byte above zero names one of the plugin's masters, and a reference that
    // overrides a master's object is that object rather than a new one.
    let mut src_cell: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut src_stat: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut max_own = 0u32;
    for (i, r) in src.iter().enumerate() {
        match &r.tag {
            b"CELL" => {
                let c = cell_of(r)?;
                // First wins: a source holding two records for one cell is already
                // malformed, and appending to both would double the grass.
                src_cell.entry(cell_key(&c)).or_insert(i);
                for reference in &c.references {
                    if reference.mast_index == 0 && reference.refr_index > max_own {
                        max_own = reference.refr_index;
                    }
                }
            }
            b"STAT" => {
                src_stat.insert(record_name(r).to_ascii_lowercase());
            }
            _ => {}
        }
    }

    let mut next = max_own.saturating_add(1);
    rep.first_frmr = next;

    /* The added plugin, split into what extends a cell the source has and what is new.
    Built before anything is written, because the record count in the header has to be
    right and it is only knowable once every record is accounted for. */
    let mut extend: std::collections::HashMap<usize, Vec<&[Sub]>> = std::collections::HashMap::new();
    let mut new_cells: Vec<&Record> = Vec::new();
    let mut new_stats: Vec<&Record> = Vec::new();
    for r in &add {
        match &r.tag {
            b"CELL" => {
                let c = cell_of(r)?;
                match src_cell.get(&cell_key(&c)) {
                    Some(&i) => extend.entry(i).or_default().push(ref_subs(r)),
                    None => new_cells.push(r),
                }
            }
            b"STAT" => {
                if src_stat.contains(&record_name(r).to_ascii_lowercase()) {
                    rep.statics_shared += 1;
                } else {
                    new_stats.push(r);
                }
            }
            // The generated plugin has no other record kinds; anything else that turns up
            // travels rather than being silently dropped.
            b"TES3" => {}
            _ => new_stats.push(r),
        }
    }
    rep.cells_extended = extend.len();
    rep.cells_added = new_cells.len();
    rep.statics_added = new_stats.len();

    /* Round 18bc (A7): whether the numbers exist at all, asked before a byte is written.
       A new reference is numbered above every number the source uses itself, and a
       plugin's own references have to stay below 0x0100_0000 because FRMR's top byte is
       a 1-based index into its master list. Past that the game reads each one as an
       override of an object in the source's first master. A generated plugin spills into
       a second file when it runs out of numbers; a merge cannot — the answer is one copy
       of somebody else's plugin — so this is reported instead of written. */
    let adding: u64 = extend
        .values()
        .flatten()
        .map(|b| count_frmr(b) as u64)
        .chain(new_cells.iter().map(|r| count_frmr(ref_subs(r)) as u64))
        .sum();
    if next as u64 + adding > crate::espwrite::FRMR_LIMIT as u64 {
        return Err(format!(
            "the merged plugin would need reference numbers past {} and a Morrowind plugin \
             cannot hold more than that. The source already numbers up to {}, and this run \
             adds {}. Export without merging — a generated plugin splits itself across as \
             many files as it needs — or merge into a plugin with fewer references in it.",
            crate::espwrite::FRMR_LIMIT - 1,
            next.saturating_sub(1),
            adding
        ));
    }

    // The source's header, whole. Only the record count moves, once everything is counted.
    let old_count = src.first().and_then(hedr_count);

    for (i, r) in src.iter_mut().enumerate().skip(1) {
        let Some(blocks) = extend.get(&i) else { continue };
        /* A cell that grows. The record's own header and existing references stay as they
        are and the new block follows, so the source's data keeps the order it had.

        Round 18bc (A8): and NAM0, which says how many references the record carries.
        Round 18gf (backend-plan item 2): NAM0 is positional. Everything before it is
        persistent, everything after it is temporary, and its value is the count of
        what follows (`tes3` cell.rs:40, 74-78, 177-182). So the source's own NAM0 is
        found wherever it sits - in the header block, or after the persistent
        references that a room of NPCs keeps in front of it - and grown by the block
        appended behind it; and a source with no NAM0 at all (every reference
        persistent) gets one *here*, between its last reference and the grass, so its
        references stay persistent to every reader. The first version put the new NAM0
        before the source's first FRMR, which made a room of all-persistent references
        read as all-temporary, and missed a NAM0 sitting among the references, so a
        second one was written ahead of them with the same effect. */
        let nam0 = match r.subs.iter().position(|s| &s.tag == b"NAM0" && s.data.len() >= 4) {
            Some(at) => at,
            None => {
                r.push(b"NAM0", 0u32.to_le_bytes().to_vec());
                r.subs.len() - 1
            }
        };
        let before = rep.refs;
        for refs in blocks {
            rep.refs += renumber(refs, &mut next, &mut r.subs);
        }
        if let Some(s) = r.subs.get_mut(nam0) {
            let n = dword(s).unwrap_or(0).saturating_add((rep.refs - before) as u32);
            s.data = std::borrow::Cow::Owned(n.to_le_bytes().to_vec());
        }
    }

    let mut out: Vec<Record> = src;
    for r in new_stats {
        out.push(r.clone());
    }
    for r in new_cells {
        /* A cell the source lacks, appended whole — with its references renumbered, since
        they were numbered for a plugin of their own and this one has its own count. */
        let at = refs_at(r);
        let mut cell = Record::new(&r.tag);
        cell.unknown = r.unknown;
        cell.flags = r.flags;
        cell.subs.extend(r.subs.iter().take(at).cloned());
        rep.refs += renumber(ref_subs(r), &mut next, &mut cell.subs);
        out.push(cell);
    }

    if let (Some(old), Some(head)) = (old_count, out.first_mut()) {
        set_hedr_count(head, old.saturating_add((rep.cells_added + rep.statics_added) as u32));
    }
    Ok((raw::write_plugin(&out), rep))
}
