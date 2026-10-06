//! The records a script compile looks names up in (the Python twin is
//! `wraithguard/mwscript/records.py`).
//!
//! The compiler resolves every bare word against the load order: a global, an object
//! ID, a script and its locals (for `id.variable`), a journal topic. `RecordIndex` holds
//! just what it needs of each record, keyed by ASCII-lowercased ID the way MWEdit and the
//! game look IDs up; a later plugin's record replaces an earlier one.
//!
//! Plugins are read with greatness7's tes3 crate (`Plugin::load_bytes_filtered`, only
//! the record types a script can name), as the rest of the backend reads them; cells,
//! which a compile needs only by name, are read from the file's framing instead of being
//! parsed whole. The editor adds the records its patch pool holds (a new global, a new
//! script's locals) with `RecordIndex::add`, so a script compiles against what the patch
//! will be, not only what the plugins are.

use std::collections::HashMap;
use std::path::PathBuf;

use encoding_rs::WINDOWS_1252;
use tes3::esp::{EditorId, Plugin, TES3Object, TypeInfo};

use crate::lint::{records, subrecords};

/// Record types an item can be carried as (MWEdit's `IsESMRecordCarryable`).
pub const CARRYABLE: [&[u8; 4]; 13] =
    [b"ALCH", b"APPA", b"ARMO", b"BOOK", b"CLOT", b"INGR", b"LIGH", b"LOCK", b"MISC", b"PROB", b"REPA", b"WEAP", b"LEVI"];

/// Record types parsed for a compile: everything with an ID a script can name, but not
/// cells (named from the framing), landscape, path grids or responses.
fn wanted(tag: [u8; 4]) -> bool {
    !matches!(&tag, b"TES3" | b"CELL" | b"LAND" | b"PGRD" | b"INFO" | b"LUAL")
}

/// A string as the plugin's bytes (Windows-1252, as the tes3 crate decodes it).
pub fn to_bytes(s: &str) -> Vec<u8> {
    WINDOWS_1252.encode(s).0.into_owned()
}

/// What the compiler needs of one record.
#[derive(Clone, Debug, Default)]
pub struct Record {
    /// The four-letter type (`NPC_`, `SCPT`...).
    pub rtype: [u8; 4],
    /// The ID as the record spells it.
    pub id: Vec<u8>,
    /// For objects, the `SCRI` script attached (empty: none).
    pub script: Vec<u8>,
    /// For `DIAL`, its type (4 is a journal); -1 otherwise.
    pub dial_type: i32,
    /// For `SCPT`, its local names: shorts, longs, floats.
    pub locals: [Vec<Vec<u8>>; 3],
}

impl Record {
    /// A record of `rtype` with `id`.
    pub fn new(rtype: &[u8; 4], id: &[u8]) -> Self {
        Record { rtype: *rtype, id: id.to_vec(), dial_type: -1, ..Default::default() }
    }

    /// A script's local: its 1-based index within its type and the type (`s`, `l`,
    /// `f`); `(-1, 0)` when the script declares none of that name.
    pub fn find_local(&self, name: &[u8]) -> (i32, u8) {
        for (kind, names) in b"slf".iter().copied().zip(&self.locals) {
            if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
                return (i as i32 + 1, kind);
            }
        }
        (-1, 0)
    }
}

/// Records by ID, ASCII case aside.
#[derive(Default)]
pub struct RecordIndex {
    recs: Vec<Record>,
    by_id: HashMap<Vec<u8>, usize>,
    by_type: HashMap<([u8; 4], Vec<u8>), usize>,
}

impl RecordIndex {
    /// Adds (or replaces) a record.
    pub fn add(&mut self, rec: Record) {
        let low = rec.id.to_ascii_lowercase();
        let at = self.recs.len();
        self.by_type.insert((rec.rtype, low.clone()), at);
        self.by_id.insert(low, at);
        self.recs.push(rec);
    }

    /// The record with this ID, whatever its type.
    pub fn find(&self, id: &[u8]) -> Option<&Record> {
        self.by_id.get(&id.to_ascii_lowercase()).map(|&i| &self.recs[i])
    }

    /// The record of this type with this ID.
    pub fn find_type(&self, id: &[u8], rtype: &[u8; 4]) -> Option<&Record> {
        let low = id.to_ascii_lowercase();
        if let Some(&i) = self.by_id.get(&low)
            && &self.recs[i].rtype == rtype
        {
            return Some(&self.recs[i]);
        }
        self.by_type.get(&(*rtype, low)).map(|&i| &self.recs[i])
    }

    /// An item record with this ID.
    pub fn find_carryable(&self, id: &[u8]) -> Option<&Record> {
        let low = id.to_ascii_lowercase();
        if let Some(&i) = self.by_id.get(&low)
            && CARRYABLE.contains(&&self.recs[i].rtype)
        {
            return Some(&self.recs[i]);
        }
        CARRYABLE.iter().find_map(|t| self.by_type.get(&(**t, low.clone())).map(|&i| &self.recs[i]))
    }

    /// The `GLOB` record with this ID.
    pub fn get_global(&self, name: &[u8]) -> Option<&Record> {
        self.find_type(name, b"GLOB")
    }

    /// Distinct IDs held.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// The compiler's view of one parsed object (`None`: nothing a script can name).
fn record_of(obj: &TES3Object) -> Option<Record> {
    use TES3Object as O;
    let id = to_bytes(&obj.editor_id());
    if id.is_empty() {
        return None;
    }
    let mut rec = Record::new(obj.tag(), &id);
    let script = match obj {
        O::Activator(o) => &o.script,
        O::Alchemy(o) => &o.script,
        O::Apparatus(o) => &o.script,
        O::Armor(o) => &o.script,
        O::Book(o) => &o.script,
        O::Clothing(o) => &o.script,
        O::Container(o) => &o.script,
        O::Creature(o) => &o.script,
        O::Door(o) => &o.script,
        O::Ingredient(o) => &o.script,
        O::Light(o) => &o.script,
        O::Lockpick(o) => &o.script,
        O::MiscItem(o) => &o.script,
        O::Npc(o) => &o.script,
        O::Probe(o) => &o.script,
        O::RepairItem(o) => &o.script,
        O::Weapon(o) => &o.script,
        O::StartScript(o) => &o.script,
        O::Dialogue(d) => {
            rec.dial_type = d.dialogue_type as i32;
            return Some(rec);
        }
        O::Script(s) => {
            let names: Vec<Vec<u8>> =
                s.variables.split(|&c| c == 0).filter(|n| !n.is_empty()).map(<[u8]>::to_vec).collect();
            let ns = (s.header.num_shorts as usize).min(names.len());
            let nl = (ns + s.header.num_longs as usize).min(names.len());
            rec.locals = [names[..ns].to_vec(), names[ns..nl].to_vec(), names[nl..].to_vec()];
            return Some(rec);
        }
        _ => return Some(rec),
    };
    rec.script = to_bytes(script);
    Some(rec)
}

/// The records of one plugin a compile can refer to.
pub fn records_from_bytes(data: &[u8]) -> std::io::Result<Vec<Record>> {
    let mut plugin = Plugin::new();
    crate::guarded(|| plugin.load_bytes_filtered(data, wanted))?;
    let mut out: Vec<Record> = plugin.objects.iter().filter_map(record_of).collect();
    // Cells by name, from the framing: a compile never needs a cell's contents.
    for (tag, body) in records(data) {
        if tag == b"CELL" {
            let name = subrecords(body).find(|(t, _)| *t == b"NAME").map(|(_, n)| n).unwrap_or_default();
            let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            out.push(Record::new(b"CELL", &name[..end]));
        }
    }
    Ok(out)
}

/// The records of plugins in load order (later ones win). Unreadable files are skipped.
pub fn index_plugins(paths: &[PathBuf]) -> RecordIndex {
    let mut index = RecordIndex::default();
    for path in paths {
        let Ok(data) = std::fs::read(path) else { continue };
        for rec in records_from_bytes(&data).unwrap_or_default() {
            index.add(rec);
        }
    }
    index
}
