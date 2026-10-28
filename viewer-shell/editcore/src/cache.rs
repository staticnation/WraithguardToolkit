//! A cache of plugin records on disk, so the Editor reads a load order once: the Rust
//! side of `wraithguard/patch/recordcache.py` (port step 4.1, slice 2), the same tables
//! and rules, with every stored body zstd-compressed bytes the caller gives (the Python
//! side gives tagged JSON, `native/src/cache.rs`) - no pickles.
//!
//! Reading one tag (the cells, say) out of hundreds of plugins is minutes of file reading
//! the first time and nothing the second. Each entry is one plugin's records of one tag,
//! valid while that plugin's size and modification time are the ones it was read at; a
//! plugin that changed is simply read again. The cache is only ever a shortcut: any
//! failure of it (a locked, corrupt or missing database) is a miss, never an error.
//!
//! Beside the records is a key index: which record keys (a cell's name or `(x, y)`, an
//! id) each plugin has of a tag, with the same size-and-time stamp, so "is this cell name
//! free?" and "which plugins have this cell?" are one indexed query over the whole load
//! order. And the use-report index: every string each plugin's records hold, by record.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::{params, params_from_iter, Connection, ErrorCode, OptionalExtension};

/// Bumped when what is stored changes shape; an older database is emptied, not read.
/// 4: bodies are zstd'd bytes of the caller's (tagged JSON from Python), not pickles.
/// 5: the use index has the words of dialogue responses' results too.
pub const SCHEMA: i64 = 5;

/// How much of the file the record bodies may take before the largest tag's go.
pub const MAX_BYTES: u64 = 1 << 30;

/// Reports kept for the load order as it is (the oldest go first).
pub const REPORTS_KEPT: i64 = 2000;

/// The pseudo-tag the use-report index is stamped under (in `key_stamps`).
pub const USES_TAG: &str = "~USE";

/// The tables that hold what was read from the plugins (emptied together).
const DERIVED: [&str; 9] = ["records", "keys", "key_stamps", "reports", "uses", "use_records", "use_plugins", "spans2", "span_stamps2"];

/// Times to open the file again when another connection is setting it up.
const OPEN_TRIES: u32 = 12;

/// Plugins asked about in one SQL statement.
const CHUNK: usize = 400;

/// zstd's level for the bodies: fast, and still most of the gain.
const LEVEL: i32 = 3;

/// A plugin's cache key and the size and time it is valid for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub key: String,
    pub size: i64,
    pub mtime_ns: i64,
}

/// `os.path.normcase`: on Windows, lower case with backslashes; elsewhere as it is.
fn normcase(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) { s.replace('/', "\\").to_lowercase() } else { s.into_owned() }
}

/// A plugin's stamp, or `None` when the file cannot be looked at.
pub fn stamp(plugin: &Path) -> Option<Stamp> {
    let md = std::fs::metadata(plugin).ok()?;
    let mtime = md.modified().ok()?;
    let ns = match mtime.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i64,
        Err(e) => -(e.duration().as_nanos() as i64),
    };
    Some(Stamp { key: normcase(plugin), size: md.len() as i64, mtime_ns: ns })
}

/// One string a plugin's record holds, for the use-report index: `(value, record type,
/// record key, paths as JSON, count)`, record by record (`usesindex.index_rows`).
pub type UseRow = (String, String, String, String, i64);

/// One use the index answers with: `(plugin index, record type, record key, paths, count)`.
pub type UseHit = (usize, String, String, Vec<String>, i64);

struct State {
    conn: Option<Connection>,
    dead: bool,
}

/// What to do with a warning (the Python side sends it to its log).
pub type Warn = dyn Fn(&str) + Send + Sync;

/// Plugin records of one tag, kept in a SQLite file. One connection shared by every
/// thread, under a lock.
pub struct Cache {
    pub path: PathBuf,
    pub parser: String,
    pub max_bytes: u64,
    state: Mutex<State>,
    warn: Box<Warn>,
}

fn locked(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error_code(), Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked))
}

fn marks(n: usize) -> String {
    vec!["?"; n].join(",")
}

impl Cache {
    /// Remembers where the database is; it is opened on first use. `parser` names the
    /// build of the reader the records are made by: a database made by another is emptied.
    pub fn new(path: PathBuf, parser: &str, max_bytes: u64) -> Cache {
        Cache::with_warn(path, parser, max_bytes, Box::new(|m| eprintln!("{m}")))
    }

    /// As [`Cache::new`], with warnings going to `warn`.
    pub fn with_warn(path: PathBuf, parser: &str, max_bytes: u64, warn: Box<Warn>) -> Cache {
        Cache { path, parser: parser.to_string(), max_bytes, state: Mutex::new(State { conn: None, dead: false }), warn }
    }

    /// Runs `f` on the open connection; `None` when the cache cannot be had.
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Option<T>) -> Option<T> {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if st.dead {
            return None;
        }
        if st.conn.is_none() {
            let mut delay = Duration::from_millis(50);
            for attempt in 0..OPEN_TRIES {
                match self.open() {
                    Ok(c) => {
                        st.conn = Some(c);
                        break;
                    }
                    Err(e) if locked(&e) && attempt + 1 < OPEN_TRIES => {
                        std::thread::sleep(delay);
                        delay = (delay * 2).min(Duration::from_secs(1));
                    }
                    Err(e) => {
                        (self.warn)(&format!("editor: record cache unavailable ({}): {e}", self.path.display()));
                        st.dead = !locked(&e); // a lock is for now
                        return None;
                    }
                }
            }
        }
        f(st.conn.as_mut()?)
    }

    /// Opens the file and makes it ready.
    fn open(&self) -> rusqlite::Result<Connection> {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut conn = Connection::open(&self.path)?;
        conn.busy_timeout(Duration::from_secs(30))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // The one that sets the file up takes the write lock first (and waits for the others
        // to finish theirs), then looks at what is there.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v)", [])?;
        let schema: Option<i64> = tx.query_row("SELECT v FROM meta WHERE k='schema'", [], |r| r.get(0)).optional()?.flatten();
        let made: Option<String> = tx
            .query_row("SELECT v FROM meta WHERE k='parser'", [], |r| r.get::<_, rusqlite::types::Value>(0))
            .optional()?
            .and_then(|v| match v {
                rusqlite::types::Value::Text(s) => Some(s),
                _ => None,
            });
        let want = format!("p:{}", self.parser);
        if schema != Some(SCHEMA) || made.as_deref() != Some(want.as_str()) {
            // Another shape, or made by another build of the reader: none of it is to be
            // trusted, the key index with the records.
            for t in DERIVED {
                tx.execute(&format!("DROP TABLE IF EXISTS {t}"), [])?;
            }
            tx.execute("INSERT OR REPLACE INTO meta VALUES ('schema', ?)", [SCHEMA])?;
            tx.execute("INSERT OR REPLACE INTO meta VALUES ('parser', ?)", [&want])?;
        }
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS records (plugin TEXT NOT NULL, tag TEXT NOT NULL,
               size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, data BLOB NOT NULL, PRIMARY KEY (plugin, tag));
             CREATE TABLE IF NOT EXISTS keys (tag TEXT NOT NULL, key TEXT NOT NULL,
               plugin TEXT NOT NULL, PRIMARY KEY (tag, key, plugin)) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS keys_plugin ON keys (plugin, tag);
             CREATE TABLE IF NOT EXISTS key_stamps (plugin TEXT NOT NULL, tag TEXT NOT NULL,
               size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, PRIMARY KEY (plugin, tag));
             CREATE TABLE IF NOT EXISTS use_plugins (pid INTEGER PRIMARY KEY, plugin TEXT NOT NULL UNIQUE);
             CREATE TABLE IF NOT EXISTS use_records (rid INTEGER PRIMARY KEY,
               pid INTEGER NOT NULL, rtype TEXT NOT NULL, key TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS use_records_pid ON use_records (pid);
             CREATE INDEX IF NOT EXISTS use_records_key ON use_records (lower(key));
             CREATE TABLE IF NOT EXISTS uses (value TEXT NOT NULL, rid INTEGER NOT NULL,
               paths TEXT NOT NULL, n INTEGER NOT NULL, PRIMARY KEY (value, rid)) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS reports (sig TEXT NOT NULL, kind TEXT NOT NULL,
               key TEXT NOT NULL, data BLOB NOT NULL, PRIMARY KEY (sig, kind, key));
             DROP TABLE IF EXISTS spans;
             DROP TABLE IF EXISTS span_stamps;
             CREATE TABLE IF NOT EXISTS spans2 (plugin TEXT NOT NULL, rtype TEXT NOT NULL, id TEXT NOT NULL,
               off INTEGER NOT NULL, len INTEGER NOT NULL, PRIMARY KEY (plugin, rtype, id)) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS span_stamps2 (plugin TEXT PRIMARY KEY,
               size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL);",
        )?;
        Cache::forget_missing(&tx)?;
        tx.commit()?;
        Ok(conn)
    }

    /// Drops what was kept for plugins that are no longer on disk (a mod removed).
    fn forget_missing(conn: &Connection) -> rusqlite::Result<()> {
        let mut gone: Vec<String> = Vec::new();
        for q in ["SELECT DISTINCT plugin FROM records", "SELECT DISTINCT plugin FROM key_stamps", "SELECT plugin FROM use_plugins", "SELECT plugin FROM span_stamps2"] {
            let mut st = conn.prepare(q)?;
            let names: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            for n in names {
                if !gone.contains(&n) && !Path::new(&n).exists() {
                    gone.push(n);
                }
            }
        }
        for g in &gone {
            for t in ["records", "keys", "key_stamps", "spans2", "span_stamps2"] {
                conn.execute(&format!("DELETE FROM {t} WHERE plugin=?"), [g])?;
            }
            Cache::drop_uses(conn, g)?;
            conn.execute("DELETE FROM use_plugins WHERE plugin=?", [g])?;
        }
        Ok(())
    }

    /// Removes what the use index holds of one plugin (the caller commits).
    fn drop_uses(conn: &Connection, plugin: &str) -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM uses WHERE rid IN (SELECT r.rid FROM use_records r JOIN use_plugins p ON p.pid = r.pid WHERE p.plugin=?)",
            [plugin],
        )?;
        conn.execute("DELETE FROM use_records WHERE pid IN (SELECT pid FROM use_plugins WHERE plugin=?)", [plugin])?;
        Ok(())
    }

    /// One plugin's cached body of a tag, when the plugin is unchanged since; else `None`.
    pub fn get(&self, plugin: &Path, tag: &str) -> Option<Vec<u8>> {
        let s = stamp(plugin)?;
        self.with(|c| {
            let row: Option<(i64, i64, Vec<u8>)> = c
                .query_row("SELECT size, mtime_ns, data FROM records WHERE plugin=? AND tag=?", params![s.key, tag], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .optional()
                .ok()?;
            let (size, mtime, data) = row?;
            if size != s.size || mtime != s.mtime_ns {
                return None;
            }
            zstd::decode_all(data.as_slice()).ok()
        })
    }

    /// Keeps a plugin's body of a tag, as the plugin is now; `keys` (lower case) indexes
    /// its records' keys too, `None` leaves the index as it is.
    pub fn put(&self, plugin: &Path, tag: &str, data: &[u8], keys: Option<&[String]>) {
        let Some(s) = stamp(plugin) else { return };
        let Ok(blob) = zstd::encode_all(data, LEVEL) else { return };
        self.with(|c| {
            let r = (|| -> rusqlite::Result<()> {
                let tx = c.transaction()?;
                tx.execute("INSERT OR REPLACE INTO records VALUES (?, ?, ?, ?, ?)", params![s.key, tag, s.size, s.mtime_ns, blob])?;
                if let Some(k) = keys {
                    Cache::write_keys(&tx, &s, tag, k)?;
                }
                tx.commit()
            })();
            match r {
                Ok(()) => self.trim(c, tag),
                Err(e) => (self.warn)(&format!("editor: record cache write failed: {e}")),
            }
            Some(())
        });
    }

    /// The bytes of the database in use (free pages not counted).
    fn used(c: &Connection) -> rusqlite::Result<i64> {
        let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let free: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        let size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok((pages - free) * size)
    }

    /// Past the size limit, drops the record bodies of the tag that takes most. The key
    /// index stays; the tag just written is spared while another can go. Still over with
    /// the bodies gone, the use-report index goes last.
    fn trim(&self, c: &Connection, keep: &str) {
        let over = |c: &Connection| Cache::used(c).map(|u| u as u64 > self.max_bytes).unwrap_or(false);
        let mut dropped = 0;
        while over(c) {
            let order: Vec<String> = match c
                .prepare("SELECT tag FROM records GROUP BY tag ORDER BY SUM(LENGTH(data)) DESC")
                .and_then(|mut st| st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>())
            {
                Ok(o) => o,
                Err(_) => break,
            };
            let others: Vec<&String> = order.iter().filter(|t| *t != keep).collect();
            let next = others.first().copied().or(order.first());
            let Some(next) = next.cloned() else { break };
            if dropped > 64 || c.execute("DELETE FROM records WHERE tag=?", [&next]).is_err() {
                break;
            }
            dropped += 1;
            (self.warn)(&format!("editor: record cache over {} bytes: dropped the {next} records", self.max_bytes));
        }
        if over(c) {
            let _ = c.execute_batch("DELETE FROM uses; DELETE FROM use_records; DELETE FROM use_plugins;");
            let _ = c.execute("DELETE FROM key_stamps WHERE tag=?", [USES_TAG]);
            (self.warn)(&format!("editor: record cache over {} bytes: dropped the use index", self.max_bytes));
            dropped += 1;
        }
        if dropped > 0 {
            self.shrink(c);
        }
    }

    /// Gives the disk back when more than half the file is free pages (a VACUUM).
    fn shrink(&self, c: &Connection) {
        let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(0);
        let free: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0)).unwrap_or(0);
        if pages < 1024 || free * 2 <= pages {
            return;
        }
        if let Err(e) = c.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);") {
            (self.warn)(&format!("editor: record cache could not be compacted: {e}"));
        }
    }

    /// The bytes of the database in use (0 when the cache is out).
    pub fn size_bytes(&self) -> i64 {
        self.with(|c| Cache::used(c).ok()).unwrap_or(0)
    }

    /// Replaces one plugin's indexed keys of a tag (the caller commits).
    fn write_keys(c: &Connection, s: &Stamp, tag: &str, keys: &[String]) -> rusqlite::Result<()> {
        c.execute("DELETE FROM keys WHERE plugin=? AND tag=?", params![s.key, tag])?;
        let unique: HashSet<&String> = keys.iter().collect();
        let mut ins = c.prepare("INSERT OR IGNORE INTO keys VALUES (?, ?, ?)")?;
        for k in unique {
            ins.execute(params![tag, k, s.key])?;
        }
        c.execute("INSERT OR REPLACE INTO key_stamps VALUES (?, ?, ?, ?)", params![s.key, tag, s.size, s.mtime_ns])?;
        Ok(())
    }

    /// Indexes a plugin's keys of a tag (lower case) without keeping its records.
    pub fn put_keys(&self, plugin: &Path, tag: &str, keys: &[String]) {
        let Some(s) = stamp(plugin) else { return };
        self.with(|c| {
            let r = (|| -> rusqlite::Result<()> {
                let tx = c.transaction()?;
                Cache::write_keys(&tx, &s, tag, keys)?;
                tx.commit()
            })();
            if let Err(e) = r {
                (self.warn)(&format!("editor: key index write failed: {e}"));
            }
            Some(())
        });
    }

    /// The plugins (indices into `plugins`) whose keys of a tag are not indexed as they
    /// now are; `None` when the cache is out.
    pub fn stale_of(&self, plugins: &[PathBuf], tag: &str) -> Option<Vec<usize>> {
        let stamps: Vec<Option<Stamp>> = plugins.iter().map(|p| stamp(p)).collect();
        let keyed: Vec<&str> = stamps.iter().flatten().map(|s| s.key.as_str()).collect();
        let have = self.with(|c| {
            let mut have: HashMap<String, (i64, i64)> = HashMap::new();
            for part in keyed.chunks(CHUNK) {
                let q = format!("SELECT plugin, size, mtime_ns FROM key_stamps WHERE tag=? AND plugin IN ({})", marks(part.len()));
                let mut st = c.prepare(&q).ok()?;
                let args = std::iter::once(tag).chain(part.iter().copied());
                let rows = st
                    .query_map(params_from_iter(args), |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))
                    .ok()?;
                for row in rows {
                    let (k, v) = row.ok()?;
                    have.insert(k, v);
                }
            }
            Some(have)
        })?;
        Some(
            stamps
                .iter()
                .enumerate()
                .filter(|(_, s)| match s {
                    None => true,
                    Some(s) => have.get(&s.key) != Some(&(s.size, s.mtime_ns)),
                })
                .map(|(i, _)| i)
                .collect(),
        )
    }

    /// Whether a plugin's keys of a tag are indexed and the plugin is unchanged since.
    pub fn keys_current(&self, plugin: &Path, tag: &str) -> bool {
        self.stale_of(&[plugin.to_path_buf()], tag).is_some_and(|s| s.is_empty())
    }

    /// The plugins whose keys of a tag still have to be indexed (all, if the cache is out).
    pub fn missing_keys(&self, plugins: &[PathBuf], tag: &str) -> Vec<usize> {
        self.stale_of(plugins, tag).unwrap_or_else(|| (0..plugins.len()).collect())
    }

    /// Stamps of the plugins that can be looked at, with their indices.
    fn stamped(plugins: &[PathBuf]) -> Vec<(usize, Stamp)> {
        plugins.iter().enumerate().filter_map(|(i, p)| stamp(p).map(|s| (i, s))).collect()
    }

    /// Which of these plugins (indices, in the order given) have a key, from the index;
    /// `None` when the index cannot answer (a plugin not indexed as it now is, or the
    /// cache out).
    pub fn holders(&self, plugins: &[PathBuf], tag: &str, key: &str) -> Option<Vec<usize>> {
        if !self.stale_of(plugins, tag)?.is_empty() {
            return None;
        }
        let stamped = Cache::stamped(plugins);
        let names: Vec<&str> = stamped.iter().map(|(_, s)| s.key.as_str()).collect();
        let found = self.with(|c| {
            let mut found: HashSet<String> = HashSet::new();
            for part in names.chunks(CHUNK) {
                let q = format!("SELECT plugin FROM keys WHERE tag=? AND key=? AND plugin IN ({})", marks(part.len()));
                let mut st = c.prepare(&q).ok()?;
                let args = [tag, key].into_iter().chain(part.iter().copied());
                for row in st.query_map(params_from_iter(args), |r| r.get::<_, String>(0)).ok()? {
                    found.insert(row.ok()?);
                }
            }
            Some(found)
        })?;
        Some(stamped.iter().filter(|(_, s)| found.contains(&s.key)).map(|(i, _)| *i).collect())
    }

    /// Every key of a tag over these plugins, from the index; `None` when it cannot answer.
    pub fn all_keys(&self, plugins: &[PathBuf], tag: &str) -> Option<HashSet<String>> {
        if !self.stale_of(plugins, tag)?.is_empty() {
            return None;
        }
        let stamped = Cache::stamped(plugins);
        let names: Vec<&str> = stamped.iter().map(|(_, s)| s.key.as_str()).collect();
        self.with(|c| {
            let mut out = HashSet::new();
            for part in names.chunks(CHUNK) {
                let q = format!("SELECT DISTINCT key FROM keys WHERE tag=? AND plugin IN ({})", marks(part.len()));
                let mut st = c.prepare(&q).ok()?;
                let args = std::iter::once(tag).chain(part.iter().copied());
                for row in st.query_map(params_from_iter(args), |r| r.get::<_, String>(0)).ok()? {
                    out.insert(row.ok()?);
                }
            }
            Some(out)
        })
    }

    /// One plugin's rows as records and (string, record) pairs: a record is a run of rows
    /// with the same type and key; its definition row (count 0) only says it exists. A
    /// string a record holds twice is one pair.
    pub fn pack_uses(rows: &[UseRow], last_rid: i64) -> (Vec<(i64, String, String)>, Vec<(String, i64, String, i64)>) {
        let mut records = Vec::new();
        let mut merged: BTreeMap<(String, i64), (Vec<serde_json::Value>, i64)> = BTreeMap::new();
        let mut rid = last_rid;
        let mut current: Option<(&str, &str)> = None;
        for (value, rtype, key, paths, n) in rows {
            if current != Some((rtype.as_str(), key.as_str())) {
                rid += 1;
                current = Some((rtype, key));
                records.push((rid, rtype.clone(), key.clone()));
            }
            if *n == 0 {
                continue;
            }
            let pair = merged.entry((value.clone(), rid)).or_default();
            if let Ok(serde_json::Value::Array(p)) = serde_json::from_str(paths) {
                pair.0.extend(p);
            }
            pair.1 += n;
        }
        let refs = [serde_json::Value::from("references")];
        let strings = merged
            .into_iter()
            .map(|((value, rid), (p, n))| {
                let paths = if p == refs { "r".to_string() } else { serde_json::Value::Array(p).to_string() };
                (value, rid, paths, n)
            })
            .collect();
        (records, strings)
    }

    /// Indexes every string a plugin's records hold (the use-report index).
    pub fn put_uses(&self, plugin: &Path, rows: &[UseRow]) {
        let Some(s) = stamp(plugin) else { return };
        self.with(|c| {
            let r = (|| -> rusqlite::Result<()> {
                let tx = c.transaction()?;
                tx.execute("INSERT OR IGNORE INTO use_plugins (plugin) VALUES (?)", [&s.key])?;
                let pid: i64 = tx.query_row("SELECT pid FROM use_plugins WHERE plugin=?", [&s.key], |r| r.get(0))?;
                Cache::drop_uses(&tx, &s.key)?;
                let last: i64 = tx.query_row("SELECT COALESCE(MAX(rid), 0) FROM use_records", [], |r| r.get(0))?;
                let (records, strings) = Cache::pack_uses(rows, last);
                {
                    let mut ins = tx.prepare("INSERT INTO use_records VALUES (?, ?, ?, ?)")?;
                    for (rid, t, k) in &records {
                        ins.execute(params![rid, pid, t, k])?;
                    }
                    let mut ins = tx.prepare("INSERT INTO uses VALUES (?, ?, ?, ?)")?;
                    for (v, rid, p, n) in &strings {
                        ins.execute(params![v, rid, p, n])?;
                    }
                }
                tx.execute("INSERT OR REPLACE INTO key_stamps VALUES (?, ?, ?, ?)", params![s.key, USES_TAG, s.size, s.mtime_ns])?;
                tx.commit()
            })();
            match r {
                Ok(()) => self.trim(c, ""),
                Err(e) => (self.warn)(&format!("editor: use index write failed: {e}")),
            }
            Some(())
        });
    }

    /// The plugins whose strings are not indexed as they now are (all if the cache is out).
    pub fn missing_uses(&self, plugins: &[PathBuf]) -> Vec<usize> {
        self.missing_keys(plugins, USES_TAG)
    }

    /// Every record that holds a string (lower case), from the use-report index, in load
    /// order and, within a plugin, file order; definition rows left out. `None` when the
    /// index cannot answer.
    pub fn uses_for(&self, plugins: &[PathBuf], value: &str) -> Option<Vec<UseHit>> {
        if !self.stale_of(plugins, USES_TAG)?.is_empty() {
            return None;
        }
        let order: HashMap<String, usize> = Cache::stamped(plugins).into_iter().map(|(i, s)| (s.key, i)).collect();
        let mut out: Vec<(usize, i64, UseHit)> = self.with(|c| {
            let mut st = c
                .prepare(
                    "SELECT p.plugin, r.rtype, r.key, u.paths, u.n, r.rid FROM uses u
                     JOIN use_records r ON r.rid = u.rid JOIN use_plugins p ON p.pid = r.pid WHERE u.value=?",
                )
                .ok()?;
            let rows = st
                .query_map([value], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))
                })
                .ok()?;
            let mut out = Vec::new();
            for row in rows {
                let (plug, rtype, key, paths, n, rid) = row.ok()?;
                let Some(&i) = order.get(&plug) else { continue };
                let paths: Vec<String> = if paths == "r" {
                    vec!["references".to_string()]
                } else {
                    let v: Vec<serde_json::Value> = serde_json::from_str(&paths).ok()?;
                    v.into_iter().map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string())).collect()
                };
                out.push((i, rid, (i, rtype, key, paths, n)));
            }
            Some(out)
        })?;
        out.sort_by_key(|(i, rid, _)| (*i, *rid));
        Some(out.into_iter().map(|(_, _, h)| h).collect())
    }

    /// Which plugins (indices, load order) define each `(record type, lower-case key)`,
    /// from the use-report index; `None` when it cannot answer.
    pub fn definers(&self, plugins: &[PathBuf], records: &[(String, String)]) -> Option<HashMap<(String, String), Vec<usize>>> {
        if !self.stale_of(plugins, USES_TAG)?.is_empty() {
            return None;
        }
        let order: HashMap<String, usize> = Cache::stamped(plugins).into_iter().map(|(i, s)| (s.key, i)).collect();
        let wanted: HashSet<&(String, String)> = records.iter().collect();
        let mut values: Vec<&str> = records.iter().map(|(_, k)| k.as_str()).collect::<HashSet<_>>().into_iter().collect();
        values.sort();
        let found = self.with(|c| {
            let mut found: HashMap<(String, String), HashSet<usize>> = HashMap::new();
            for part in values.chunks(CHUNK) {
                let q = format!(
                    "SELECT p.plugin, r.rtype, r.key FROM use_records r JOIN use_plugins p ON p.pid = r.pid WHERE lower(r.key) IN ({})",
                    marks(part.len())
                );
                let mut st = c.prepare(&q).ok()?;
                let rows = st
                    .query_map(params_from_iter(part.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
                    .ok()?;
                for row in rows {
                    let (plug, rtype, key) = row.ok()?;
                    let pair = (rtype, key.to_lowercase());
                    if let (true, Some(&i)) = (wanted.contains(&pair), order.get(&plug)) {
                        found.entry(pair).or_default().insert(i);
                    }
                }
            }
            Some(found)
        })?;
        Some(
            found
                .into_iter()
                .map(|(pair, set)| {
                    let mut v: Vec<usize> = set.into_iter().collect();
                    v.sort();
                    (pair, v)
                })
                .collect(),
        )
    }

    /// One text for a load order: which plugins, in what order, as they now are (a hash
    /// of every plugin's path, size and time); `None` when one cannot be looked at.
    pub fn load_order_signature(plugins: &[PathBuf]) -> Option<String> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for p in plugins {
            let s = stamp(p)?;
            h.update(format!("{}\0{}\0{}\n", s.key, s.size, s.mtime_ns).as_bytes());
        }
        Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// A report kept for a load order ([`Cache::load_order_signature`]).
    pub fn get_report(&self, sig: &str, kind: &str, key: &str) -> Option<Vec<u8>> {
        self.with(|c| {
            let data: Vec<u8> = c
                .query_row("SELECT data FROM reports WHERE sig=? AND kind=? AND key=?", params![sig, kind, key], |r| r.get(0))
                .optional()
                .ok()??;
            zstd::decode_all(data.as_slice()).ok()
        })
    }

    /// Keeps a report for a load order; reports of any other load order go, and the
    /// oldest past [`REPORTS_KEPT`].
    pub fn put_report(&self, sig: &str, kind: &str, key: &str, data: &[u8]) {
        let Ok(blob) = zstd::encode_all(data, LEVEL) else { return };
        self.with(|c| {
            let r = (|| -> rusqlite::Result<()> {
                let tx = c.transaction()?;
                tx.execute("INSERT OR REPLACE INTO reports VALUES (?, ?, ?, ?)", params![sig, kind, key, blob])?;
                tx.execute("DELETE FROM reports WHERE sig != ?", [sig])?;
                tx.execute(
                    "DELETE FROM reports WHERE rowid IN (SELECT rowid FROM reports ORDER BY rowid DESC LIMIT -1 OFFSET ?)",
                    [REPORTS_KEPT],
                )?;
                tx.commit()
            })();
            if let Err(e) = r {
                (self.warn)(&format!("editor: report cache write failed: {e}"));
            }
            Some(())
        });
    }

    /// Keeps where each record key of a plugin lies in its file - `(type, id, offset,
    /// length)`, the whole record - as the plugin is now, so one record can be read by
    /// seeking to it rather than reading the plugin through. Replaces what was kept of it.
    /// Ids are kept and looked up without case (ASCII): the first record of an id in any
    /// spelling is the one kept.
    pub fn put_spans(&self, plugin: &Path, spans: &[(String, String, u64, u32)]) {
        let Some(s) = stamp(plugin) else { return };
        self.with(|c| {
            let r = (|| -> rusqlite::Result<()> {
                let tx = c.transaction()?;
                tx.execute("DELETE FROM spans2 WHERE plugin=?", [&s.key])?;
                {
                    // The first record of an id (in any case) stands, as the keys count it.
                    let mut ins = tx.prepare("INSERT OR IGNORE INTO spans2 VALUES (?, ?, ?, ?, ?)")?;
                    for (t, id, off, len) in spans {
                        ins.execute(params![s.key, t, id.to_ascii_lowercase(), *off as i64, *len as i64])?;
                    }
                }
                tx.execute("INSERT OR REPLACE INTO span_stamps2 VALUES (?, ?, ?)", params![s.key, s.size, s.mtime_ns])?;
                tx.commit()
            })();
            if let Err(e) = r {
                (self.warn)(&format!("editor: record span write failed: {e}"));
            }
            Some(())
        });
    }

    /// Whether a plugin's record spans are kept and the plugin is unchanged since (so a
    /// key [`Cache::span`] does not find is not in the plugin).
    pub fn spans_current(&self, plugin: &Path) -> bool {
        let Some(s) = stamp(plugin) else { return false };
        self.with(|c| {
            let row: Option<(i64, i64)> = c
                .query_row("SELECT size, mtime_ns FROM span_stamps2 WHERE plugin=?", [&s.key], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .ok()?;
            Some(row == Some((s.size, s.mtime_ns)))
        })
        .unwrap_or(false)
    }

    /// Where a record lies in a plugin's file, `(offset, length)`: one indexed lookup; `None`
    /// when the spans are not kept for the plugin as it is, or it has no such record.
    pub fn span(&self, plugin: &Path, rtype: &str, id: &str) -> Option<(u64, u32)> {
        let s = stamp(plugin)?;
        self.with(|c| {
            c.query_row(
                "SELECT p.off, p.len FROM spans2 p JOIN span_stamps2 t ON t.plugin = p.plugin \
                 WHERE p.plugin=?1 AND p.rtype=?2 AND p.id=?3 AND t.size=?4 AND t.mtime_ns=?5",
                params![s.key, rtype, id.to_ascii_lowercase(), s.size, s.mtime_ns],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u32)),
            )
            .optional()
            .ok()
        })
        .flatten()
    }

    /// The plugins (indices into `plugins`) whose record spans are not kept as they now
    /// are; `None` when the cache is out.
    pub fn stale_spans(&self, plugins: &[PathBuf]) -> Option<Vec<usize>> {
        let stamps: Vec<Option<Stamp>> = plugins.iter().map(|p| stamp(p)).collect();
        let keyed: Vec<&str> = stamps.iter().flatten().map(|s| s.key.as_str()).collect();
        let have = self.with(|c| {
            let mut have: HashMap<String, (i64, i64)> = HashMap::new();
            for part in keyed.chunks(CHUNK) {
                let q = format!("SELECT plugin, size, mtime_ns FROM span_stamps2 WHERE plugin IN ({})", marks(part.len()));
                let mut st = c.prepare(&q).ok()?;
                let rows = st
                    .query_map(params_from_iter(part.iter().copied()), |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))
                    .ok()?;
                for row in rows {
                    let (k, v) = row.ok()?;
                    have.insert(k, v);
                }
            }
            Some(have)
        })?;
        Some(
            stamps
                .iter()
                .enumerate()
                .filter(|(_, s)| match s {
                    None => true,
                    Some(s) => have.get(&s.key) != Some(&(s.size, s.mtime_ns)),
                })
                .map(|(i, _)| i)
                .collect(),
        )
    }

    /// Forgets everything cached (the use index aside, as before).
    pub fn clear(&self) {
        self.with(|c| c.execute_batch("DELETE FROM records; DELETE FROM keys; DELETE FROM key_stamps; DELETE FROM reports; DELETE FROM spans2; DELETE FROM span_stamps2;").ok());
    }

    /// What the cache holds: `pages`, `free`, `page_size`, `used`, and `rows_<table>`.
    pub fn stats(&self) -> Vec<(String, i64)> {
        self.with(|c| {
            let q = |s: &str| c.query_row(s, [], |r| r.get::<_, i64>(0)).ok();
            let (pages, free, size) = (q("PRAGMA page_count")?, q("PRAGMA freelist_count")?, q("PRAGMA page_size")?);
            let mut out = vec![("pages".into(), pages), ("free".into(), free), ("page_size".into(), size), ("used".into(), (pages - free) * size)];
            for t in ["records", "keys", "key_stamps", "uses", "use_records", "use_plugins", "reports", "spans2"] {
                out.push((format!("rows_{t}"), q(&format!("SELECT COUNT(*) FROM {t}"))?));
            }
            Some(out)
        })
        .unwrap_or_default()
    }

    /// Gives the disk back when more than half the file is free pages; whether it did.
    pub fn compact(&self) -> bool {
        self.with(|c| {
            let before: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0)).ok()?;
            self.shrink(c);
            let after: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0)).ok()?;
            Some(after < before)
        })
        .unwrap_or(false)
    }

    /// Closes the database (it reopens on next use), compacting it first if it is mostly
    /// free pages.
    pub fn close(&self) {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(c) = st.conn.take() {
            self.shrink(&c);
            let _ = c.close();
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.close();
    }
}
