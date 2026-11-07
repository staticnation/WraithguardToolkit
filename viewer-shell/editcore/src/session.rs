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
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use indexmap::IndexMap;
use ordercore::cells::CellKey;
use ordercore::refs::{origin, winner, Version};
use serde_json::Value;

use crate::cache::Cache;
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
    let t: [u8; 4] = tag.as_bytes().try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a tag is four letters"))?;
    // Mapped, not read into a Vec: a warm-up asks every plugin for twenty-odd tags, and a copy
    // of the whole file for each one (Messages, 2026-11-06: Verify ran for minutes and
    // starved the machine) is what the map avoids - the OS keeps one copy of the file.
    with_file_bytes(path, |bytes| {
        let json = crate::plugin::records_json(bytes, Some(&[t]))?;
        serde_json::from_str(&json).map_err(io::Error::other)
    })
}

/// `f` over a plugin's bytes, mapped (an empty file is an empty slice, which cannot be mapped).
pub(crate) fn with_file_bytes<T>(path: &Path, f: impl FnOnce(&[u8]) -> io::Result<T>) -> io::Result<T> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() == 0 {
        return f(&[]);
    }
    // SAFETY: the plugin is only read, and only while mapped here.
    let map = unsafe { memmap2::Mmap::map(&file) }?;
    f(&map)
}

/// How many plugins a warm-up reads at once: not every core, so the window and the other
/// links keep one, and not more than eight, past which the disk is the limit.
fn readers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(2)).clamp(2, 8)
}

/// A plugin's topics and responses, from the file, in file order.
pub fn read_dialogue(path: &Path) -> io::Result<Vec<Value>> {
    with_file_bytes(path, |bytes| {
        let json = crate::plugin::records_json(bytes, Some(&[*b"DIAL", *b"INFO"]))?;
        serde_json::from_str(&json).map_err(io::Error::other)
    })
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
    /// Where what is read is kept between runs (`set_disk_cache`): a plugin's records of a tag
    /// as it was last read, and which keys each plugin has of a tag. Only ever a shortcut.
    disk: OnceLock<Arc<Cache>>,
    /// Per plugin and tag, the record keys (lower case): found from the file's framing without
    /// parsing a record (`ids_of`), or from the records when they were read anyway.
    ids: Mutex<HashMap<(String, String), Arc<HashSet<String>>>>,
    /// Whether the plugins are read from the files (`new`), so a key can be found from the
    /// file itself; the tests' sessions read through their own `read`.
    from_files: bool,
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

/// What a plugin's topics and responses are kept under in the disk cache (not a record tag).
const DIALOGUE_TAG: &str = "~DIALOGUE";

impl Session {
    /// A session over the load order `(file name, path)`, reading the files.
    pub fn new(plugins: Vec<(String, PathBuf)>) -> Session {
        let mut s = Session::with_reader(plugins, Box::new(read_tag));
        s.from_files = true;
        s
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
            disk: OnceLock::new(),
            ids: Mutex::default(),
            from_files: false,
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
        self.ids.lock().unwrap().retain(|(p, _), _| *p != name);
        true
    }

    /// Keeps what is read from the plugins in `cache` between runs, as the Python Editor did
    /// (`disk_cache`): a plugin read once is not read again until it changes. Set once.
    pub fn set_disk_cache(&self, cache: Arc<Cache>) {
        let _ = self.disk.set(cache);
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
        let generation = self.generation.load(Ordering::SeqCst);
        let mut found = Vec::new();
        if let Some((_, path)) = self.paths.get(&low) {
            // From the disk cache when the plugin is as it was read (every launch read every
            // plugin's dialogue again, for Verify and the topics), else from the plugin.
            let kept = self.disk.get().and_then(|d| d.get(path, DIALOGUE_TAG)).and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok());
            match kept {
                Some(recs) => found = recs,
                None => match (self.read_dialogue)(path) {
                    Ok(recs) => {
                        if let Some(d) = self.disk.get()
                            && self.generation.load(Ordering::SeqCst) == generation
                            && let Ok(body) = serde_json::to_vec(&recs)
                        {
                            d.put(path, DIALOGUE_TAG, &body, None);
                        }
                        found = recs;
                    }
                    Err(e) => eprintln!("editor: cannot read {}: {e}", path.display()),
                },
            }
        }
        let found = Arc::new(found);
        self.dialogue.lock().unwrap().insert(low, found.clone());
        found
    }

    /// Reads, on several threads, the topics and responses of every plugin not read yet (the
    /// biggest first), as `warm` does the records of a tag; `dialogue` then has them. Needs no
    /// more than a `&self`, so the links that read dialogue can do it before the Editor's lock.
    pub fn warm_dialogue(&self) {
        let names: Vec<&str> = {
            let have = self.dialogue.lock().unwrap();
            self.order.iter().map(String::as_str).filter(|n| !have.contains_key(&n.to_lowercase())).collect()
        };
        let mut todo: Vec<(&str, u64)> = names
            .into_iter()
            .map(|n| (n, self.paths.get(&n.to_lowercase()).and_then(|(_, p)| std::fs::metadata(p).ok()).map_or(0, |m| m.len())))
            .collect();
        todo.sort_by_key(|t| std::cmp::Reverse(t.1));
        let threads = readers().min(MAX_READERS).min(todo.len());
        if threads <= 1 {
            for (name, _) in &todo {
                self.dialogue(name);
            }
            return;
        }
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    while let Some((name, _)) = todo.get(next.fetch_add(1, Ordering::SeqCst)) {
                        self.dialogue(name);
                    }
                });
            }
        });
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
            // From the disk cache when the plugin is as it was read, else from the plugin.
            let kept = self.disk.get().and_then(|d| d.get(path, tag)).and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok());
            let (recs, fresh) = match kept {
                Some(r) => (Some(r), false),
                None => match (self.read)(path, tag) {
                    Ok(r) => (Some(r), true),
                    Err(e) => {
                        eprintln!("editor: cannot read {}: {e}", path.display());
                        (None, false)
                    }
                },
            };
            if let Some(recs) = recs {
                let body = if fresh && self.disk.get().is_some() { serde_json::to_vec(&recs).ok() } else { None };
                for rec in recs {
                    let key = record_key(&rec);
                    if !key.is_empty() {
                        found.insert(key.to_lowercase(), rec);
                    }
                }
                if let (Some(d), Some(body)) = (self.disk.get(), body)
                    && self.generation.load(Ordering::SeqCst) == generation
                {
                    let keys: Vec<String> = found.keys().cloned().collect();
                    d.put(path, tag, &body, Some(&keys));
                }
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
        let threads = readers().min(MAX_READERS).min(todo.len());
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
        let threads = readers().min(MAX_READERS).min(todo.len());
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

    /// Whether every plugin's cell keys are read (`warm_cell_keys` has finished, wherever it ran).
    pub fn cell_keys_ready(&self) -> bool {
        let have = self.cell_keys.lock().unwrap();
        self.order.iter().all(|n| have.contains_key(&n.to_lowercase()))
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

    /// Whether a tag's keys can be found from a plugin's framing alone (`ids_of`), without
    /// parsing its records.
    pub fn key_only(tag: &str) -> bool {
        NAME_KEYED.contains(&tag) || matches!(tag, "SCPT" | "CELL" | "LAND")
    }

    /// One plugin's record keys of a tag (lower case): from its records when they are read
    /// already, else from the file's framing (`ids_of`; a cell or a land is never parsed to
    /// list its key), else from the records, read. Kept in the key index on disk when there is
    /// one.
    fn plugin_ids_of(&self, lower: &str, tag: &str) -> Arc<HashSet<String>> {
        let k = (lower.to_string(), tag.to_string());
        if let Some(hit) = self.ids.lock().unwrap().get(&k) {
            return hit.clone();
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let mut set: Option<HashSet<String>> = self.cache.lock().unwrap().get(&k).map(|r| r.keys().cloned().collect());
        if set.is_none()
            && self.from_files
            && Session::key_only(tag)
            && let Some((_, path)) = self.paths.get(lower)
        {
            set = with_file_bytes(path, |b| Ok(ids_of(b, tag))).ok().flatten();
            if let (Some(s), Some(d)) = (&set, self.disk.get())
                && self.generation.load(Ordering::SeqCst) == generation
            {
                d.put_keys(path, tag, &s.iter().cloned().collect::<Vec<_>>());
            }
        }
        let set = Arc::new(match set {
            Some(s) => s,
            None => self.records(lower, tag).keys().cloned().collect(),
        });
        if self.generation.load(Ordering::SeqCst) == generation {
            self.ids.lock().unwrap().insert(k, set.clone());
        }
        set
    }

    /// Reads the record keys of the `tags` for every plugin that has not been asked yet, on
    /// several threads (`plugin_ids_of`): the files' framing, not a record parsed. A tag the
    /// framing cannot key is left for when it is asked. Needs no more than a `&self`, so a
    /// link can do it before the Editor's lock.
    pub fn warm_ids(&self, tags: &[&str]) {
        let paths = self.plugin_paths();
        let mut todo: Vec<(String, &str, u64)> = Vec::new();
        for tag in tags.iter().filter(|t| !self.from_files || Session::key_only(t)) {
            // Every plugin's keys already kept on disk: the key index answers.
            if self.disk.get().and_then(|d| d.stale_of(&paths, tag)).is_some_and(|s| s.is_empty()) {
                continue;
            }
            let have = self.ids.lock().unwrap();
            for name in &self.order {
                let low = name.to_lowercase();
                if !have.contains_key(&(low.clone(), tag.to_string())) {
                    let size = self.paths.get(&low).and_then(|(_, p)| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
                    todo.push((low, tag, size));
                }
            }
        }
        todo.sort_by_key(|t| std::cmp::Reverse(t.2));
        let threads = readers().min(MAX_READERS).min(todo.len());
        if threads <= 1 {
            for (low, tag, _) in &todo {
                self.plugin_ids_of(low, tag);
            }
            return;
        }
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    while let Some((low, tag, _)) = todo.get(next.fetch_add(1, Ordering::SeqCst)) {
                        self.plugin_ids_of(low, tag);
                    }
                });
            }
        });
    }

    /// Every record key (lower case) a tag has over the load order's plugins: one query when
    /// the key index on disk has every plugin as it now is, else from the plugins - their
    /// framing, no record parsed for a cell, a land or an object - which fills the index.
    pub fn plugin_ids(&self, tag: &str) -> HashSet<String> {
        if let Some(d) = self.disk.get()
            && let Some(keys) = d.all_keys(&self.plugin_paths(), tag)
        {
            return keys;
        }
        self.warm_ids(&[tag]);
        let mut all = HashSet::new();
        for name in &self.order {
            all.extend(self.plugin_ids_of(&name.to_lowercase(), tag).iter().cloned());
        }
        all
    }

    /// The plugins (load order's spelling, in load order) that have a record of a tag with this
    /// key (lower case): from the key index on disk, else from the plugins as `plugin_ids`.
    /// A record a plugin deletes still counts as its.
    pub fn id_holders(&self, tag: &str, key: &str) -> Vec<String> {
        if let Some(d) = self.disk.get()
            && let Some(at) = d.holders(&self.plugin_paths(), tag, key)
        {
            return at.into_iter().filter_map(|i| self.order.get(i).cloned()).collect();
        }
        self.warm_ids(&[tag]);
        self.order.iter().filter(|n| self.plugin_ids_of(&n.to_lowercase(), tag).contains(key)).cloned().collect()
    }

    /// One plugin's landscape record for a square, parsed alone (a plugin has thousands, each
    /// a few hundred kilobytes parsed: the others are not read). The last one in the file when
    /// it has two. `None`: the plugin has none there.
    pub fn land_record(&self, plugin: &str, grid: (i64, i64)) -> Option<Value> {
        let low = plugin.to_lowercase();
        if self.from_files
            && let Some((_, path)) = self.paths.get(&low)
        {
            let got = with_file_bytes(path, |raw| {
                let mut found = None;
                for (tag, body) in crate::frames::records(raw) {
                    if tag == b"LAND" && land_grid(body) == Some(grid) {
                        let at = body.as_ptr() as usize - raw.as_ptr() as usize;
                        found = Some((at.saturating_sub(16), at + body.len()));
                    }
                }
                found.map(|(a, b)| crate::plugin::record_value(&raw[a..b])).transpose()
            });
            if let Ok(found) = got {
                return found;
            }
        }
        let want = format!("({}, {})", grid.0, grid.1);
        self.records(plugin, "LAND").get(&want).cloned()
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
    cell_keys_checked(raw).0
}

/// `cell_keys_of`, and whether a name was left out for not being ASCII.
fn cell_keys_checked(raw: &[u8]) -> (HashSet<String>, bool) {
    let mut keys = HashSet::new();
    let mut left_out = false;
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
            } else if !n.is_empty() {
                left_out = true;
            }
        } else {
            let x = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
            let y = i32::from_le_bytes([d[8], d[9], d[10], d[11]]);
            keys.insert(format!("({x}, {y})"));
        }
    }
    (keys, left_out)
}

/// The tags whose records are keyed by their first `NAME` subrecord alone (the id, as
/// `record_key` reads `id`).
const NAME_KEYED: [&str; 32] = [
    "ACTI", "ALCH", "APPA", "ARMO", "BODY", "BOOK", "BSGN", "CLAS", "CLOT", "CONT", "CREA", "DIAL", "DOOR", "ENCH", "FACT", "GLOB", "GMST", "INGR", "LEVC", "LEVI", "LIGH",
    "LOCK", "MISC", "NPC_", "PROB", "RACE", "REGN", "REPA", "SOUN", "SPEL", "STAT", "WEAP",
];

/// A land square from a `LAND` body's `INTV`.
fn land_grid(body: &[u8]) -> Option<(i64, i64)> {
    let (_, d) = crate::frames::subrecords(body).find(|(t, d)| *t == b"INTV" && d.len() >= 8)?;
    Some((i32::from_le_bytes([d[0], d[1], d[2], d[3]]) as i64, i32::from_le_bytes([d[4], d[5], d[6], d[7]]) as i64))
}

/// A record's id from its bytes: plain printable ASCII with nothing to trim, else `None`
/// (the caller parses, for what the plugin's text decodes to).
fn plain_id(bytes: &[u8]) -> Option<String> {
    let n = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
    if n.is_empty() || !n.iter().all(|b| (0x20..0x7f).contains(b)) || n.first() == Some(&b' ') || n.last() == Some(&b' ') {
        return None;
    }
    Some(String::from_utf8_lossy(n).to_lowercase())
}

/// The keys (lower case) of a tag's records in a plugin's bytes, from the record and
/// subrecord framing alone - what `record_key` gives each record parsed, without parsing
/// one (every cell's references, every land's vertices): `NAME` for the object types and
/// most others, a script's `SCHD`, a cell's name or grid, a land's `INTV`. `None` when this
/// cannot say for the tag (`Session::key_only`) or for a record in this plugin (a name that is
/// not plain ASCII, or an unreadable one): the caller parses the plugin's records instead.
pub(crate) fn ids_of(raw: &[u8], tag: &str) -> Option<HashSet<String>> {
    match tag {
        "CELL" => {
            let (keys, left_out) = cell_keys_checked(raw);
            (!left_out).then_some(keys)
        }
        "LAND" => Some(crate::frames::records(raw).filter(|(t, _)| *t == b"LAND").filter_map(|(_, body)| land_grid(body)).map(|(x, y)| format!("({x}, {y})")).collect()),
        "SCPT" => {
            let mut keys = HashSet::new();
            for (_, body) in crate::frames::records(raw).filter(|(t, _)| *t == b"SCPT") {
                let (_, d) = crate::frames::subrecords(body).find(|(t, _)| *t == b"SCHD")?;
                keys.insert(plain_id(&d[..d.len().min(32)])?);
            }
            Some(keys)
        }
        t if NAME_KEYED.contains(&t) => {
            let mut keys = HashSet::new();
            for (_, body) in crate::frames::records(raw).filter(|(rt, _)| *rt == t.as_bytes()) {
                let (_, d) = crate::frames::subrecords(body).find(|(st, _)| *st == b"NAME")?;
                keys.insert(plain_id(d)?);
            }
            Some(keys)
        }
        _ => None,
    }
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

#[cfg(test)]
mod key_only_tests {
    use super::{ids_of, Session};
    use crate::key::record_key;
    use serde_json::Value;
    use std::collections::HashSet;

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

    fn named(tag: &[u8; 4], id: &str) -> Vec<u8> {
        record(tag, &sub(b"NAME", &[id.as_bytes(), &[0]].concat()))
    }

    fn script(id: &str) -> Vec<u8> {
        let mut head = [id.as_bytes(), &[0u8; 32][..32 - id.len()]].concat();
        head.extend([0u8; 20]);
        let mut body = sub(b"SCHD", &head);
        body.extend(sub(b"SCTX", b"Begin x\nEnd\0"));
        record(b"SCPT", &body)
    }

    fn cell(name: &str, interior: bool, x: i32, y: i32) -> Vec<u8> {
        let mut body = sub(b"NAME", &[name.as_bytes(), &[0]].concat());
        let mut d = (interior as u32).to_le_bytes().to_vec();
        d.extend(x.to_le_bytes());
        d.extend(y.to_le_bytes());
        body.extend(sub(b"DATA", &d));
        record(b"CELL", &body)
    }

    fn land(x: i32, y: i32) -> Vec<u8> {
        let mut d = x.to_le_bytes().to_vec();
        d.extend(y.to_le_bytes());
        record(b"LAND", &sub(b"INTV", &d))
    }

    /// A plugin with a couple of records of each tag the framing keys, ids with the awkward
    /// cases (case, spaces inside, punctuation).
    fn plugin() -> Vec<u8> {
        let mut raw = Vec::new();
        for tag in [
            "STAT", "ACTI", "WEAP", "ARMO", "BOOK", "NPC_", "CREA", "CONT", "DOOR", "FACT", "CLAS", "RACE", "SOUN", "SPEL", "REGN", "GLOB", "ALCH", "APPA", "BODY", "BSGN", "CLOT", "DIAL", "ENCH", "GMST",
            "INGR", "LEVC", "LEVI", "LIGH", "LOCK", "MISC", "PROB", "REPA",
        ] {
            let t: [u8; 4] = tag.as_bytes().try_into().unwrap();
            raw.extend(named(&t, &format!("{tag}_Mixed Case-1")));
            raw.extend(named(&t, &format!("{tag}, second")));
        }
        raw.extend(script("Fargoth_Script"));
        raw.extend(script("a b"));
        raw.extend(cell("Seyda Neen, Census and Excise Office", true, 0, 0));
        raw.extend(cell("", false, -2, -9));
        raw.extend(cell("Vivec", false, 3, -11));
        raw.extend(land(-2, -9));
        raw.extend(land(3, -11));
        raw
    }

    fn parsed_keys(raw: &[u8], tag: &str) -> HashSet<String> {
        let t: [u8; 4] = tag.as_bytes().try_into().unwrap();
        let recs: Vec<Value> = serde_json::from_str(&crate::plugin::records_json(raw, Some(&[t])).unwrap_or_else(|e| panic!("{tag}: {e}"))).unwrap();
        recs.iter().map(record_key).filter(|k| !k.is_empty()).map(|k| k.to_lowercase()).collect()
    }

    #[test]
    fn framing_keys_are_the_keys_of_the_parsed_records() {
        let raw = plugin();
        let mut tags: Vec<&str> = super::NAME_KEYED.to_vec();
        tags.extend(["SCPT", "CELL", "LAND"]);
        for tag in tags {
            assert!(Session::key_only(tag), "{tag}");
            let want = parsed_keys(&raw, tag);
            assert!(!want.is_empty(), "{tag}: the sample has records of it");
            assert_eq!(ids_of(&raw, tag).unwrap_or_else(|| panic!("{tag}: the framing keys it")), want, "{tag}");
        }
        assert!(ids_of(&raw, "PGRD").is_none(), "a tag the framing cannot key is parsed");
    }

    #[test]
    fn a_name_that_is_not_plain_is_left_to_the_parse() {
        let mut raw = named(b"STAT", "Caf\u{e9}");
        raw.extend(named(b"STAT", "plain"));
        assert!(ids_of(&raw, "STAT").is_none());
        assert!(ids_of(&named(b"STAT", " padded"), "STAT").is_none());
        assert!(ids_of(&cell("Caf\u{e9}", true, 0, 0), "CELL").is_none(), "a cell whose name is not ASCII is not keyed from the framing");
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wg-keyonly-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ids_and_holders_come_from_the_files_and_then_from_the_key_index() {
        use crate::cache::Cache;
        use std::sync::Arc;
        let dir = scratch("holders");
        let (a, b) = (dir.join("a.esp"), dir.join("b.esp"));
        std::fs::write(&a, [named(b"STAT", "Rock_A"), cell("Vivec", false, 3, -11), land(3, -11)].concat()).unwrap();
        std::fs::write(&b, [named(b"STAT", "rock_b"), named(b"STAT", "Rock_A"), land(3, -11), land(0, 0)].concat()).unwrap();
        let order = || vec![("a.esp".to_string(), a.clone()), ("b.esp".to_string(), b.clone())];
        let cache = Arc::new(Cache::new(dir.join("c.sqlite3"), "t", 1 << 24));

        let first = Session::new(order());
        first.set_disk_cache(cache.clone());
        let want: HashSet<String> = ["rock_a", "rock_b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(first.plugin_ids("STAT"), want);
        assert_eq!(first.id_holders("STAT", "rock_a"), vec!["a.esp", "b.esp"]);
        assert_eq!(first.id_holders("LAND", "(3, -11)"), vec!["a.esp", "b.esp"]);
        assert_eq!(first.id_holders("LAND", "(0, 0)"), vec!["b.esp"]);
        assert!(first.plugin_ids("CELL").contains("(3, -11)"));
        assert!(cache.keys_current(&a, "STAT") && cache.keys_current(&b, "LAND"), "what was keyed is in the index");

        // A second run: the index has it, and nothing is read for it.
        let second = Session::new(order());
        second.set_disk_cache(cache.clone());
        assert_eq!(second.plugin_ids("STAT"), want);
        assert_eq!(second.id_holders("STAT", "rock_b"), vec!["b.esp"]);
        assert!(second.ids.lock().unwrap().is_empty(), "no plugin was keyed again");

        // A plugin that changed is keyed again.
        std::fs::write(&b, [named(b"STAT", "Rock_C"), land(0, 0)].concat()).unwrap();
        let third = Session::new(order());
        third.set_disk_cache(cache.clone());
        let now: HashSet<String> = ["rock_a", "rock_c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(third.plugin_ids("STAT"), now);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_land_is_parsed_alone_by_its_square() {
        let dir = scratch("land");
        let p = dir.join("l.esp");
        std::fs::write(&p, [land(1, 2), land(3, -11), land(5, 5)].concat()).unwrap();
        let s = Session::new(vec![("l.esp".into(), p.clone())]);
        let got = s.land_record("l.esp", (3, -11)).expect("the square is there");
        assert_eq!(record_key(&got), "(3, -11)");
        assert!(s.land_record("l.esp", (9, 9)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_read_once_come_from_the_disk_cache_until_the_plugin_changes() {
        use crate::cache::Cache;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let dir = scratch("records");
        let p = dir.join("r.esp");
        std::fs::write(&p, named(b"STAT", "Rock_A")).unwrap();
        let cache = Arc::new(Cache::new(dir.join("c.sqlite3"), "t", 1 << 24));
        let reads = Arc::new(AtomicUsize::new(0));
        let session = |reads: Arc<AtomicUsize>| {
            let s = Session::with_reader(
                vec![("r.esp".into(), p.clone())],
                Box::new(move |path, tag| {
                    reads.fetch_add(1, Ordering::SeqCst);
                    super::read_tag(path, tag)
                }),
            );
            s.set_disk_cache(cache.clone());
            s
        };
        let first = session(reads.clone()).records("r.esp", "STAT");
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let again = session(reads.clone()).records("r.esp", "STAT");
        assert_eq!(reads.load(Ordering::SeqCst), 1, "the second run read nothing");
        assert_eq!(*first, *again, "and got the same records");
        std::fs::write(&p, [named(b"STAT", "Rock_A"), named(b"STAT", "Rock_B")].concat()).unwrap();
        assert_eq!(session(reads.clone()).records("r.esp", "STAT").len(), 2);
        assert_eq!(reads.load(Ordering::SeqCst), 2, "a changed plugin is read again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dialogue_read_once_comes_from_the_disk_cache_until_the_plugin_changes() {
        use crate::cache::Cache;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let dir = scratch("dialogue");
        let p = dir.join("d.esp");
        std::fs::write(&p, named(b"DIAL", "greeting 0")).unwrap();
        let cache = Arc::new(Cache::new(dir.join("c.sqlite3"), "t", 1 << 24));
        let reads = Arc::new(AtomicUsize::new(0));
        let session = |reads: Arc<AtomicUsize>| {
            let s = Session::new(vec![("d.esp".into(), p.clone())]).with_read_dialogue(Box::new(move |path| {
                reads.fetch_add(1, Ordering::SeqCst);
                super::read_dialogue(path)
            }));
            s.set_disk_cache(cache.clone());
            s
        };
        let first = session(reads.clone()).dialogue("d.esp");
        assert_eq!(first.len(), 1);
        let again = session(reads.clone()).dialogue("d.esp");
        assert_eq!(reads.load(Ordering::SeqCst), 1, "the second run read nothing");
        assert_eq!(*first, *again);
        std::fs::write(&p, [named(b"DIAL", "greeting 0"), named(b"DIAL", "greeting 1")].concat()).unwrap();
        assert_eq!(session(reads.clone()).dialogue("d.esp").len(), 2);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
