//! A load order's records and placed objects, as the Editor finds them: the reading half
//! of `wraithguard/patch/editor.py`'s `EditorSession` (`_records`, `find`, `_masters_of`,
//! `find_ref`, `_home_cell`), on the same tes3conv-schema records and the same rules.
//!
//! [`Session::find_in`] also finds the records the patch makes itself (the queue's new
//! records, `crate::queue`), as the Python `find` does. Not here yet (later slices of
//! port step 4.1, `viewer-shell/PORT.md`): the disk cache in the session, and every edit.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use ordercore::cells::CellKey;
use ordercore::refs::{origin, winner, Version};
use serde_json::Value;

use crate::key::{deletes, record_key, type_name, type_of};
use crate::queue::PatchQueue;

/// The name a version the patch makes itself goes by (`editor.PATCH`).
pub const PATCH: &str = "(this patch)";

/// Reads one plugin's records of one tag (the seam the tests fill, as the Python tests
/// fill `EditorSession(read=...)`).
pub type Reader = dyn Fn(&Path, &str) -> io::Result<Vec<Value>> + Send + Sync;

/// Reads all of one plugin's records, in file order (the Use Report's seam, as the Python
/// tests fill `EditorSession(read_all=...)`; without it the report scans the files).
pub type ReadAll = dyn Fn(&Path) -> io::Result<Vec<Value>> + Send + Sync;

/// A plugin's records of one tag, from the file (`plugin::records_json` with that tag).
pub fn read_tag(path: &Path, tag: &str) -> io::Result<Vec<Value>> {
    let bytes = std::fs::read(path)?;
    let t: [u8; 4] = tag.as_bytes().try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a tag is four letters"))?;
    let json = crate::plugin::records_json(&bytes, Some(&[t]))?;
    serde_json::from_str(&json).map_err(io::Error::other)
}

/// A plugin's topics and responses, from the file, in file order.
pub fn read_dialogue(path: &Path) -> io::Result<Vec<Value>> {
    let bytes = std::fs::read(path)?;
    let json = crate::plugin::records_json(&bytes, Some(&[*b"DIAL", *b"INFO"]))?;
    serde_json::from_str(&json).map_err(io::Error::other)
}

/// One record, as every plugin that defines it has it.
#[derive(Clone, Debug)]
pub struct Found {
    /// Its tag (`NPC_`).
    pub tag: String,
    /// Its tes3conv `type` (`Npc`).
    pub record_type: String,
    /// Its key as the winning version spells it.
    pub key: String,
    /// `(plugin, record)` for each plugin whose version counts, in load order; the last
    /// wins. A deletion clears the ones before it.
    pub versions: Vec<(String, Value)>,
    /// The patch makes it: no plugin defines it, and its one version is the patch's.
    pub new: bool,
}

impl Found {
    /// The plugin whose version the game uses.
    pub fn winner(&self) -> &str {
        &self.versions.last().expect("a found record has a version").0
    }
    /// The winning version.
    pub fn record(&self) -> &Value {
        &self.versions.last().expect("a found record has a version").1
    }
}

/// One placed object, as the load order resolves it.
#[derive(Clone, Debug)]
pub struct FoundRef {
    /// Its cell's key (an interior's name, an exterior's `(x, y)`): where its record is,
    /// which for one moved out of its exterior cell is the cell it left.
    pub cell: String,
    /// The plugin that created it, spelled as the load order spells it.
    pub origin: String,
    /// Its index there.
    pub refr_index: i64,
    /// The plugins with a CELL record for its cell, in load order.
    pub plugins: Vec<String>,
    /// The plugin whose version of the reference stands.
    pub winner: String,
    /// That version (deleted, when a deletion stands: `ordercore::refs`).
    pub reference: Value,
}

/// A load order read the Editor's way. Each plugin's records of a tag are read once.
pub struct Session {
    order: Vec<String>,
    paths: HashMap<String, (String, PathBuf)>,
    rank: HashMap<String, usize>,
    read: Box<Reader>,
    read_all: Option<Box<ReadAll>>,
    read_dialogue: Box<ReadAll>,
    dialogue: Mutex<HashMap<String, Arc<Vec<Value>>>>,
    cache: Mutex<HashMap<(String, String), Arc<IndexMap<String, Value>>>>,
    masters: Mutex<HashMap<String, Arc<Vec<String>>>>,
    /// Per plugin: its exterior references by index, `index -> [(origin, cell key)]`.
    home: Mutex<HashMap<String, Arc<HashMap<i64, Vec<(String, String)>>>>>,
}

impl Session {
    /// A session over the load order `(file name, path)`, reading the files.
    pub fn new(plugins: Vec<(String, PathBuf)>) -> Session {
        Session::with_reader(plugins, Box::new(read_tag))
    }

    /// A session reading through `read`.
    pub fn with_reader(plugins: Vec<(String, PathBuf)>, read: Box<Reader>) -> Session {
        let order: Vec<String> = plugins.iter().map(|(n, _)| n.clone()).collect();
        let rank = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
        let paths = plugins.into_iter().map(|(n, p)| (n.to_lowercase(), (n, p))).collect();
        Session {
            order,
            paths,
            rank,
            read,
            read_all: None,
            read_dialogue: Box::new(read_dialogue),
            dialogue: Mutex::default(),
            cache: Mutex::default(),
            masters: Mutex::default(),
            home: Mutex::default(),
        }
    }

    /// The session, reading a plugin whole through `read_all` for the Use Report.
    pub fn with_read_all(mut self, read_all: Box<ReadAll>) -> Session {
        self.read_all = Some(read_all);
        self
    }

    /// The session, reading a plugin's topics and responses through `read_dialogue`.
    pub fn with_read_dialogue(mut self, read_dialogue: Box<ReadAll>) -> Session {
        self.read_dialogue = read_dialogue;
        self
    }

    /// One plugin's topics and responses, in file order (a response belongs to the topic
    /// before it), read once. A plugin that cannot be read has none.
    pub fn dialogue(&self, plugin: &str) -> Arc<Vec<Value>> {
        let low = plugin.to_lowercase();
        if let Some(hit) = self.dialogue.lock().unwrap().get(&low) {
            return hit.clone();
        }
        let mut found = Vec::new();
        if let Some((_, path)) = self.paths.get(&low) {
            match (self.read_dialogue)(path) {
                Ok(recs) => found = recs,
                Err(e) => eprintln!("editor: cannot read {}: {e}", path.display()),
            }
        }
        let found = Arc::new(found);
        self.dialogue.lock().unwrap().insert(low, found.clone());
        found
    }

    /// The load order's plugin names, in order.
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// The load order's plugin files, in order.
    pub fn plugin_paths(&self) -> Vec<PathBuf> {
        self.order.iter().map(|n| self.paths[&n.to_lowercase()].1.clone()).collect()
    }

    /// Every use of a record over the plugins (`crate::uses`): the files scanned, or the
    /// records `read_all` gives.
    pub fn use_report(&self, record_type: &str, rid: &str) -> Vec<crate::uses::Use> {
        let paths = self.plugin_paths();
        match &self.read_all {
            Some(read) => crate::uses::report_with(paths.len(), |i| read(&paths[i]), record_type, rid),
            None => crate::uses::report(&paths, record_type, rid),
        }
    }

    /// One plugin's every record (header included), in file order: through `read_all`
    /// when the session has one, else from the file. `None` when it cannot be read.
    pub fn read_whole(&self, plugin: &str) -> Option<Vec<Value>> {
        let (_, path) = self.paths.get(&plugin.to_lowercase())?;
        let got = match &self.read_all {
            Some(read) => read(path),
            None => std::fs::read(path).and_then(|b| crate::plugin::records_json(&b, None)).and_then(|j| serde_json::from_str(&j).map_err(io::Error::other)),
        };
        match got {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("editor: cannot read {}: {e}", path.display());
                None
            }
        }
    }

    /// One plugin's records of a tag by lower-case key, in file order (a second record
    /// with a key takes the first's place). A plugin that cannot be read has none (the
    /// Python logs it and carries on, as here).
    pub fn records(&self, plugin: &str, tag: &str) -> Arc<IndexMap<String, Value>> {
        let k = (plugin.to_lowercase(), tag.to_string());
        if let Some(hit) = self.cache.lock().unwrap().get(&k) {
            return hit.clone();
        }
        let mut found = IndexMap::new();
        if let Some((_, path)) = self.paths.get(&k.0) {
            match (self.read)(path, tag) {
                Ok(recs) => {
                    for rec in recs {
                        let key = record_key(&rec);
                        if !key.is_empty() {
                            found.insert(key.to_lowercase(), rec);
                        }
                    }
                }
                Err(e) => eprintln!("editor: cannot read {}: {e}", path.display()),
            }
        }
        let found = Arc::new(found);
        self.cache.lock().unwrap().insert(k, found.clone());
        found
    }

    /// `names` (or the whole order) known to the load order, sorted into it.
    pub(crate) fn known(&self, names: Option<&[String]>) -> Vec<String> {
        let mut v: Vec<String> = match names {
            Some(n) => n.iter().filter(|p| self.rank.contains_key(&p.to_lowercase())).cloned().collect(),
            None => self.order.clone(),
        };
        v.sort_by_key(|p| self.rank[&p.to_lowercase()]);
        v
    }

    /// The plugin's name as the load order spells it.
    pub(crate) fn spelled<'a>(&'a self, name: &'a str) -> &'a str {
        self.paths.get(&name.to_lowercase()).map(|(n, _)| n.as_str()).unwrap_or(name)
    }

    /// A record, from every plugin that defines it (`plugins`, when the caller knows them;
    /// else every plugin). `rid` is its id, or `(x, y)` for an exterior cell. `None` when
    /// no plugin has it, or the last that does deletes it.
    pub fn find(&self, tag: &str, rid: &str, plugins: Option<&[String]>) -> Option<Found> {
        self.find_in(tag, rid, plugins, None)
    }

    /// [`Session::find`], with the records `queue` makes: one the patch makes is in no
    /// plugin (its id was checked free when made), so none is read for it - for a CELL
    /// that would be every plugin of the load order - and its one version is the patch's.
    pub fn find_in(&self, tag: &str, rid: &str, plugins: Option<&[String]>, queue: Option<&PatchQueue>) -> Option<Found> {
        let record_type = type_of(tag)?;
        if let Some(made) = queue.and_then(|q| q.new_record(record_type, rid.trim())) {
            let tag = crate::key::tag_of(record_type)?.to_string();
            return Some(Found {
                tag,
                record_type: record_type.to_string(),
                key: made.key.clone(),
                versions: vec![(PATCH.to_string(), made.record.clone())],
                new: true,
            });
        }
        let tag = crate::key::tag_of(record_type)?.to_string();
        let want = rid.trim().to_lowercase();
        let mut versions: Vec<(String, Value)> = Vec::new();
        for name in self.known(plugins) {
            let recs = self.records(&name, &tag);
            if let Some(rec) = recs.get(&want).filter(|r| type_name(r) == record_type) {
                if deletes(rec) {
                    // A deletion takes the record out (as OpenMW does): the versions before
                    // it no longer count, and a later plugin may define it again.
                    versions.clear();
                    continue;
                }
                versions.push((self.spelled(&name).to_string(), rec.clone()));
            }
        }
        let key = record_key(&versions.last()?.1);
        Some(Found { tag, record_type: record_type.to_string(), key, versions, new: false })
    }

    /// A plugin's master list, from its header (read once).
    pub fn masters_of(&self, plugin: &str) -> Arc<Vec<String>> {
        let low = plugin.to_lowercase();
        if let Some(hit) = self.masters.lock().unwrap().get(&low) {
            return hit.clone();
        }
        let mut out = Vec::new();
        if let Some((_, path)) = self.paths.get(&low) {
            match (self.read)(path, "TES3") {
                Ok(recs) => {
                    if let Some(h) = recs.iter().find(|r| type_name(r) == "Header") {
                        for m in h.get("masters").and_then(Value::as_array).into_iter().flatten() {
                            if let Some(name) = m.as_array().and_then(|e| e.first()).and_then(Value::as_str) {
                                out.push(name.to_string());
                            }
                        }
                    }
                }
                Err(e) => eprintln!("editor: cannot read {}: {e}", path.display()),
            }
        }
        let out = Arc::new(out);
        self.masters.lock().unwrap().insert(low, out.clone());
        out
    }

    /// The key a cell's records are found by: an exterior's `(x, y)` from any spelling
    /// (`ordercore::cells`), an interior's name, both lower case.
    fn cell_want(cell: &str) -> String {
        match CellKey::parse(cell) {
            Some(k @ CellKey::Exterior(..)) => k.pool_key(),
            _ => cell.trim().to_lowercase(),
        }
    }

    /// A placed object, from every plugin with a CELL record for its cell (`plugins`, when
    /// the caller knows them). `cell` is an interior's name or an exterior in any spelling.
    /// One moved here from another exterior cell (MVRF) is found in the cell it left,
    /// where its record is and where it is changed. `None` when no version has it.
    pub fn find_ref(&self, cell: &str, origin_name: &str, refr_index: i64, plugins: Option<&[String]>) -> Option<FoundRef> {
        let want = Session::cell_want(cell);
        let mut cells: Vec<(String, Value)> = Vec::new();
        for name in self.known(plugins) {
            if let Some(rec) = self.records(&name, "CELL").get(&want).filter(|r| type_name(r) == "Cell") {
                cells.push((self.spelled(&name).to_string(), rec.clone()));
            }
        }
        let won = if cells.is_empty() { None } else { self.winning_reference(&cells, origin_name, refr_index) };
        let Some((winner_plugin, reference)) = won else {
            CellKey::parse(cell)?.grid()?;
            let home = self.home_cell(origin_name, refr_index)?;
            if home.to_lowercase() == want {
                return None;
            }
            return self.find_ref(&home, origin_name, refr_index, None);
        };
        Some(FoundRef {
            cell: record_key(&cells.last()?.1),
            origin: self.spelled(origin_name).to_string(),
            refr_index,
            plugins: cells.iter().map(|(p, _)| p.clone()).collect(),
            winner: winner_plugin,
            reference,
        })
    }

    /// The reference `(origin, refr_index)` as the load order resolves it over a cell's
    /// versions: the last version stands, but after a deletion only another deletion takes
    /// over (`ordercore::refs`, `refedit.winning_reference`).
    pub fn winning_reference(&self, cells: &[(String, Value)], origin_name: &str, refr_index: i64) -> Option<(String, Value)> {
        let mut mine: Vec<(String, Value)> = Vec::new();
        for (plugin, rec) in cells {
            let masters = self.masters_of(plugin);
            for r in rec.get("references").and_then(Value::as_array).into_iter().flatten() {
                if !r.is_object() || r.get("refr_index").and_then(Value::as_i64) != Some(refr_index) {
                    continue;
                }
                let mast = r.get("mast_index").and_then(Value::as_i64).unwrap_or(-1);
                if origin(plugin, &masters, mast).is_some_and(|o| o.to_lowercase() == origin_name.to_lowercase()) {
                    mine.push((plugin.clone(), r.clone()));
                }
            }
        }
        let versions: Vec<Version> = mine
            .iter()
            .enumerate()
            .map(|(i, (_, r))| Version { plugin: i, deleted: r.get("deleted").is_some_and(truthy) })
            .collect();
        let w = winner(&versions)?;
        Some(mine.swap_remove(w.at))
    }

    /// The exterior cell whose records hold a reference: the first plugin in load order
    /// with it in one of its exterior cells. One moved out of a cell keeps its key there.
    pub fn home_cell(&self, origin_name: &str, refr_index: i64) -> Option<String> {
        let want = origin_name.to_lowercase();
        for plugin in &self.order {
            if let Some(list) = self.home_index(plugin).get(&refr_index)
                && let Some((_, key)) = list.iter().find(|(at, _)| *at == want)
            {
                return Some(key.clone());
            }
        }
        None
    }

    /// One plugin's exterior references by index (walked once).
    fn home_index(&self, plugin: &str) -> Arc<HashMap<i64, Vec<(String, String)>>> {
        let low = plugin.to_lowercase();
        if let Some(hit) = self.home.lock().unwrap().get(&low) {
            return hit.clone();
        }
        let masters = self.masters_of(plugin);
        let name = self.spelled(plugin).to_string();
        let mut index: HashMap<i64, Vec<(String, String)>> = HashMap::new();
        for (key, rec) in self.records(plugin, "CELL").iter() {
            if CellKey::parse(key).and_then(|k| k.grid()).is_none() {
                continue;
            }
            let cell = record_key(rec);
            for r in rec.get("references").and_then(Value::as_array).into_iter().flatten() {
                let Some(idx) = r.get("refr_index").and_then(Value::as_i64) else { continue };
                let mast = r.get("mast_index").and_then(Value::as_i64).unwrap_or(-1);
                if let Some(at) = origin(&name, &masters, mast) {
                    index.entry(idx).or_default().push((at.to_lowercase(), cell.clone()));
                }
            }
        }
        let index = Arc::new(index);
        self.home.lock().unwrap().insert(low, index.clone());
        index
    }
}

/// Python's `bool()` of a JSON value.
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
