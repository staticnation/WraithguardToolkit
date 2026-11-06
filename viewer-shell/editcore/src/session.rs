//! A load order's records and placed objects, as the Editor finds them: the reading half
//! of `wraithguard/patch/editor.py`'s `EditorSession` (`_records`, `find`, `_masters_of`,
//! `find_ref`, `_home_cell`), on the same tes3conv-schema records and the same rules.
//!
//! [`Session::find_in`] also finds the records the patch makes itself (the queue's new
//! records, `crate::queue`), as the Python `find` did. The edits are `crate::edit`'s and
//! `crate::places`'; the disk cache is `crate::cache`.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

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
    /// Per plugin: the keys (lower case) of its CELL records, read from the file's record and
    /// subrecord framing alone (`cell_keys_of`): which plugins have a cell is known without
    /// parsing a cell, and a cell's references are most of a plugin.
    cell_keys: Mutex<HashMap<String, Arc<HashSet<String>>>>,
    /// Bumped when a plugin is forgotten: a read begun before it may be of the file as it was,
    /// and is not kept.
    generation: AtomicU64,
    /// The reads under way, `(plugin, tag)`: a second asker waits for the first's read rather
    /// than make its own (the Object Window asks for a type's columns more than once).
    reading: Mutex<HashSet<(String, String)>>,
    reading_done: Condvar,
}

/// A read under way, let go when it ends (or panics), so that those waiting for it go on.
struct Claim<'a> {
    session: &'a Session,
    key: (String, String),
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.session.reading.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.key);
        self.session.reading_done.notify_all();
    }
}

/// The most plugins read at once.
const MAX_READERS: usize = 16;

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
            cell_keys: Mutex::default(),
            generation: AtomicU64::new(0),
            reading: Mutex::default(),
            reading_done: Condvar::new(),
        }
    }

    /// Forgets what was read of the plugin at `path` (a patch the Editor just wrote over one
    /// of the load order's files): its records are read again from the file as it is now.
    /// Without it, the Editor kept answering from the file as it was when first read - land
    /// the new patch carries was "not there", and changes queued against the old copy
    /// pointed at records the file no longer has. `true` when `path` is one of its plugins.
    pub fn forget_file(&self, path: &Path) -> bool {
        let Some(name) = self.paths.iter().find(|(_, (_, p))| same_file(p, path)).map(|(k, _)| k.clone()) else { return false };
        // First, so that a read already under way (these run without the Editor's lock) does
        // not put the file as it was back in the cache after it is cleared.
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.cache.lock().unwrap().retain(|(p, _), _| *p != name);
        self.masters.lock().unwrap().remove(&name);
        self.home.lock().unwrap().remove(&name);
        self.dialogue.lock().unwrap().remove(&name);
        self.cell_keys.lock().unwrap().remove(&name);
        true
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

    /// Whether the Use Report scans the plugin files (no `read_all`): then
    /// [`crate::uses::report`] over [`Session::plugin_paths`] is the report, and can be
    /// run without the Editor.
    pub fn scans_files(&self) -> bool {
        self.read_all.is_none()
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
        // Take the read, or wait for whoever has it and use what they read. The cache is looked
        // at again once the claim list is held: a read puts its result in the cache before it
        // lets go of its claim, so a claim that is gone has left its result there.
        loop {
            if let Some(hit) = self.cache.lock().unwrap().get(&k) {
                return hit.clone();
            }
            let mut busy = self.reading.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(hit) = self.cache.lock().unwrap().get(&k) {
                return hit.clone();
            }
            if busy.insert(k.clone()) {
                break;
            }
            while busy.contains(&k) {
                busy = self.reading_done.wait(busy).unwrap_or_else(|p| p.into_inner());
            }
        }
        let _claim = Claim { session: self, key: k.clone() };
        let generation = self.generation.load(Ordering::SeqCst);
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
        if self.generation.load(Ordering::SeqCst) == generation {
            self.cache.lock().unwrap().insert(k, found.clone());
        }
        found
    }

    /// Reads, on several threads at once, what of the `tags` each plugin has not been read
    /// for yet; `records` then has them. The biggest plugins go first, so that one of them
    /// is not left for last. Needs no more of the Session than a `&self`: the Editor's
    /// links call it before taking the Editor's lock, so a cold read holds nothing up.
    pub fn warm(&self, tags: &[&str]) {
        let mut todo: Vec<(&str, &str, u64)> = Vec::new();
        {
            let cache = self.cache.lock().unwrap();
            for tag in tags {
                for name in &self.order {
                    if !cache.contains_key(&(name.to_lowercase(), tag.to_string())) {
                        todo.push((name, tag, 0));
                    }
                }
            }
        }
        for t in &mut todo {
            t.2 = self.paths.get(&t.0.to_lowercase()).and_then(|(_, p)| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        }
        todo.sort_by_key(|t| std::cmp::Reverse(t.2));
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_READERS).min(todo.len());
        if threads <= 1 {
            for (name, tag, _) in &todo {
                self.records(name, tag);
            }
            return;
        }
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    while let Some((name, tag, _)) = todo.get(next.fetch_add(1, Ordering::SeqCst)) {
                        self.records(name, tag);
                    }
                });
            }
        });
    }

    /// A plugin's cell keys (lower case; `cell_keys_of`), read once; `None` when the file
    /// cannot be read.
    fn plugin_cell_keys(&self, lower: &str) -> Option<Arc<HashSet<String>>> {
        if let Some(k) = self.cell_keys.lock().unwrap().get(lower) {
            return Some(k.clone());
        }
        let (_, path) = self.paths.get(lower)?;
        let generation = self.generation.load(Ordering::SeqCst);
        let file = std::fs::File::open(path).ok()?;
        // SAFETY: the plugin is only read, and only while mapped here.
        let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
        let keys = Arc::new(cell_keys_of(&map));
        // A plugin forgotten while it was read is not kept as it was read.
        if self.generation.load(Ordering::SeqCst) == generation {
            self.cell_keys.lock().unwrap().insert(lower.to_string(), keys.clone());
        }
        Some(keys)
    }

    /// Reads every plugin's cell keys that are not read yet, on several threads. Needs no more
    /// than a `&self`, so a link can do it before the Editor's lock is taken.
    pub fn warm_cell_keys(&self) {
        let todo: Vec<String> = {
            let have = self.cell_keys.lock().unwrap();
            self.order.iter().map(|n| n.to_lowercase()).filter(|n| !have.contains_key(n)).collect()
        };
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_READERS).min(todo.len());
        if threads <= 1 {
            for name in &todo {
                let _ = self.plugin_cell_keys(name);
            }
            return;
        }
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    while let Some(name) = todo.get(next.fetch_add(1, Ordering::SeqCst)) {
                        let _ = self.plugin_cell_keys(name);
                    }
                });
            }
        });
    }

    /// The plugins with a CELL record for a cell (`want`: an interior's name or an exterior's
    /// `(x, y)`, lower case), in load order, as the load order spells them - from the plugins'
    /// own cell names and grids, no cell parsed. `None` when this cannot say: a name that is
    /// not ASCII (the framing cannot say what the plugin's text decodes to), or no plugin file
    /// that can be read (the tests' sessions): the caller reads the cells then.
    pub fn cell_holders(&self, want: &str) -> Option<Vec<String>> {
        if want.is_empty() || !want.is_ascii() {
            return None;
        }
        self.warm_cell_keys();
        let (mut out, mut readable) = (Vec::new(), 0usize);
        for name in &self.order {
            let Some(keys) = self.plugin_cell_keys(&name.to_lowercase()) else { continue };
            readable += 1;
            if keys.contains(want) {
                out.push(name.clone());
            }
        }
        (readable > 0).then_some(out)
    }

    /// Each plugin's records of a tag, in load order (`records` for all of them, the unread
    /// ones read on several threads: `warm`).
    pub fn records_all(&self, tag: &str) -> Vec<Arc<IndexMap<String, Value>>> {
        self.warm(&[tag]);
        self.order.iter().map(|n| self.records(n, tag)).collect()
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

    /// Whether a plugin of the load order has the record, from what has been read already
    /// and nothing more: `None` when a plugin's records of that type have not been read yet.
    /// For what must never wait on reading every plugin (the pool's list, asked for again
    /// and again: a CELL read through 600 plugins held the Editor for minutes).
    pub fn has_read(&self, tag: &str, rid: &str) -> Option<bool> {
        let record_type = type_of(tag)?;
        let tag = crate::key::tag_of(record_type)?;
        let want = rid.trim().to_lowercase();
        let cache = self.cache.lock().unwrap();
        let mut found = false;
        for name in &self.order {
            let recs = cache.get(&(name.to_lowercase(), tag.to_string()))?;
            if let Some(rec) = recs.get(&want).filter(|r| type_name(r) == record_type) {
                found = !deletes(rec);
            }
        }
        Some(found)
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
        // A cell is read from the plugins that have it (their own cell names and grids say
        // which), not from every plugin: that parsed every cell of the load order.
        let holders = if plugins.is_none() && record_type == "Cell" { self.cell_holders(&want) } else { None };
        let plugins = holders.as_deref().or(plugins);
        if plugins.is_none() {
            self.warm(&[tag.as_str()]);
        }
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

/// Two paths naming one file: the same path, or (Windows ignores case) the same path in
/// any case, or the same file once resolved.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b || a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase() {
        return true;
    }
    matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// The keys (lower case) of the CELL records in a plugin's bytes, from the record and
/// subrecord framing alone: an interior's name (its `IS_INTERIOR` flag, the first bit of
/// `DATA`'s flags), an exterior's `(x, y)` from `DATA`'s grid - the key `record_key` gives the
/// parsed record. A name that is not ASCII is left out (nothing ASCII asks for it).
pub(crate) fn cell_keys_of(raw: &[u8]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for (tag, body) in crate::frames::records(raw) {
        if tag != b"CELL" {
            continue;
        }
        let (mut name, mut data): (Option<&[u8]>, Option<&[u8]>) = (None, None);
        for (sub, bytes) in crate::frames::subrecords(body) {
            match sub {
                b"NAME" if name.is_none() => name = Some(bytes),
                b"DATA" if data.is_none() => data = Some(bytes),
                _ => {}
            }
        }
        let Some(d) = data.filter(|d| d.len() >= 12) else { continue };
        if u32::from_le_bytes([d[0], d[1], d[2], d[3]]) & 1 != 0 {
            let n = name.unwrap_or(&[]);
            let n = &n[..n.iter().position(|&b| b == 0).unwrap_or(n.len())];
            if !n.is_empty() && n.is_ascii() {
                keys.insert(String::from_utf8_lossy(n).to_lowercase());
            }
        } else {
            let x = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
            let y = i32::from_le_bytes([d[8], d[9], d[10], d[11]]);
            keys.insert(format!("({x}, {y})"));
        }
    }
    keys
}

#[cfg(test)]
mod cell_keys_tests {
    use super::cell_keys_of;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn record(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend([0u8; 8]);
        v.extend_from_slice(body);
        v
    }

    fn cell(name: &str, flags: u32, x: i32, y: i32) -> Vec<u8> {
        let mut body = sub(b"NAME", &[name.as_bytes(), &[0]].concat());
        let mut d = flags.to_le_bytes().to_vec();
        d.extend(x.to_le_bytes());
        d.extend(y.to_le_bytes());
        body.extend(sub(b"DATA", &d));
        record(b"CELL", &body)
    }

    #[test]
    fn cells_are_keyed_from_the_framing_as_the_editor_keys_them() {
        let mut raw = record(b"STAT", &sub(b"NAME", b"x\0"));
        raw.extend(cell("Vivec, Arena", 1, 0, 0));
        raw.extend(cell("Balmora", 2, -3, -2));
        raw.extend(cell("Seyda Neen", 0, -2, -9));
        raw.extend(cell("Caf\u{e9}", 1, 0, 0));
        let keys = cell_keys_of(&raw);
        let mut got: Vec<&str> = keys.iter().map(String::as_str).collect();
        got.sort();
        assert_eq!(got, vec!["(-2, -9)", "(-3, -2)", "vivec, arena"]);
    }
}
