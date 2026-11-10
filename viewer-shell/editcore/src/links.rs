//! The Editor's links answered in Rust (`wraithguard/gui/editorlink.py`'s handlers): the
//! same JSON in and out, so the page asks the viewer's own engine what it asked
//! Wraithguard over the loopback server. Port step 4.1, slice 7 (the flip): 7a the
//! reading and editing links, 7b the dialogue, scripts, uses and the verifier, 7c the
//! build and the test run. A link not answered here is still Wraithguard's
//! ([`Links::answers`] names the ones that are).
//!
//! The patch queue is shared with Wraithguard's conflict window and Patch Builder, which
//! are still Python, through the journal both write (`wraithguard_patch_journal.json`):
//! every change writes it, and before each link the journal is read again when it
//! changed since this side last saw it ([`Links::sync`]) - another side's change taken
//! as one step, which an undo here takes back.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde_json::{json, Value};

use crate::edit::{EditError, Editor};
use crate::queue::PatchQueue;
use crate::session::{Found, FoundRef, Session};

/// The links this side answers (7a).
pub const ANSWERS: &[&str] = &[
    "editRecord",
    "editSet",
    "editSetMany",
    "editRevert",
    "editInsert",
    "editDuplicate",
    "editRef",
    "editRefSet",
    "editRefRevert",
    "editRefsSet",
    "editPlace",
    "editPlaceMany",
    "editNew",
    "editNewSet",
    "editNewRemove",
    "editNewCell",
    "editNewLand",
    "editLandSet",
    "editLandTextures",
    "editLandColors",
    "editLandPending",
    "editCellRegions",
    "editDuplicateCell",
    "editPathgrid",
    "editPathgridSet",
    "editSearch",
    "editSearchAll",
    "editReplaceAll",
    "editColumns",
    "editActive",
    "editFilterChoices",
    "editUndo",
    "editRedo",
    "editRevision",
    "editPending",
    // 7b
    "editUses",
    "editReplace",
    "editScript",
    "editResult",
    "editScriptWords",
    "editScriptRefs",
    "editLeveledRoll",
    "editTopics",
    "editTopic",
    "editNewResponse",
    "editMoveResponse",
    "editCopyTopic",
    "editDialogueFlow",
    "editDialogueMap",
    "editDialogueFlags",
    "editDialoguePlay",
    "editWhoCanSay",
    "editVerify",
    // 7c
    "editBuildInfo",
    "editBuildCheck",
    "editBuild",
    "editBuildStatus",
    "editTestSetups",
    "editTestRun",
];

/// The links run beside the Editor rather than inside its lock (a build reads the
/// plugins on a thread of its own).
const BUILD_LINKS: &[&str] = &["editBuildInfo", "editBuildCheck", "editBuild", "editBuildStatus", "editTestSetups", "editTestRun"];

/// The links that stay Wraithguard's. None now: the four that opened its windows
/// (`editReview`, `editFlowchart`, `editLuaChart`, `editPreviewPatch`) are the viewer's own
/// commands (`editor_script_flowchart`, `lua_chart`, `editor_preview_patch`, and the Editor's
/// Build page in place of the Patch Builder). Their Python handlers went with the Python
/// Editor (5.0; `CHANGELOG.md`).
pub const WRAITHGUARDS: &[&str] = &[];

/// How many records the Object Window's search lists, and the search across every type;
/// how many scripts naming a word, and NPCs who can say a response.
const SEARCH_LIMIT: usize = 20000;
const SEARCH_ALL_LIMIT: usize = 2000;
const SCRIPT_REFS_LIMIT: usize = 1000;
const WHO_CAN_SAY_LIMIT: usize = 500;

/// What a link may need from the viewer beyond the Editor.
#[derive(Default)]
pub struct Ctx<'a> {
    /// Whether the load order's data folders (loose or archived) have a file (the
    /// verifier's), when the viewer knows its install.
    pub file_exists: Option<&'a dyn Fn(&str) -> bool>,
    /// What is wrong with a sound file the load order has (the verifier's), when known.
    pub sound_problem: Option<crate::verify::SoundProblem<'a>>,
}

/// A link's answer: JSON, or a word (`ok`).
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Json(Value),
    Text(String),
}

impl Answer {
    /// As the page reads it: the JSON's text, or the word.
    pub fn text(&self) -> String {
        match self {
            Answer::Json(v) => v.to_string(),
            Answer::Text(t) => t.clone(),
        }
    }
}

type Res<T> = Result<T, String>;

fn edit<T>(r: Result<T, EditError>) -> Res<T> {
    r.map_err(|e| e.0)
}

/// What a journal file is now: `(size, modified)`, `None` when it is not there.
fn stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), m.modified().ok()?))
}

/// The Editor as the viewer holds it, with the journal it shares.
pub struct Links {
    pub editor: Arc<Mutex<Editor>>,
    /// The Editor's session, held here too: what a link reads from the plugins is read
    /// through it before the Editor's lock is taken (`warm_columns`), so a slow read of a
    /// type never keeps another link (opening a record) waiting.
    session: Arc<Session>,
    /// The journal as this side last read or wrote it.
    seen: Mutex<Option<(u64, SystemTime)>>,
    /// The load order it was opened on, `(name, path)`, and the journal.
    pub plugins: Vec<(String, PathBuf)>,
    pub journal: Option<PathBuf>,
    /// The builds it runs (`editBuild`, `editTestRun`).
    pub builds: crate::buildjob::Builds,
    /// The setup's data folders, the highest priority first: where a patch would go.
    pub folders: Vec<String>,
    /// Where Test in OpenMW keeps the OpenMW chosen and the launch setups (Wraithguard's
    /// `openmw_test.json`); `None`: nowhere, and testing is refused.
    pub test_settings: Option<PathBuf>,
    /// Where the test plugin is written.
    pub scratch: PathBuf,
    /// The use-report index (`with_use_index`): once every plugin is in it, a Use Report is a
    /// query, not a scan of every plugin file. `None`: every report scans.
    use_index: Option<Arc<crate::cache::Cache>>,
    /// Set while the plugins the index lacks are being indexed (one run at a time).
    indexing: Arc<std::sync::atomic::AtomicBool>,
    /// Held across Verify's warm-up: a second Verify (a double click, Check again) waits for the
    /// first's reads and finds them cached, rather than start a second set beside them.
    verify_warm: Mutex<()>,
    /// Where Verify's last warm-up went, in ms: waiting for another's, the ids, the game
    /// settings, the dialogue (told in its answer, `took.parts`).
    verify_parts: Mutex<[u64; 4]>,
}

impl Links {
    /// An Editor over the load order `plugins`, its queue the journal's.
    pub fn open(plugins: Vec<(String, PathBuf)>, journal: Option<PathBuf>) -> Links {
        let session = Session::new(plugins.clone());
        let mut queue = PatchQueue::new();
        let seen = journal.as_deref().and_then(stamp);
        if let Some(j) = &journal {
            queue.restore(j);
        }
        let session = Arc::new(session);
        let editor = Editor::new(session.clone(), queue, journal.clone());
        // Which plugins have which cells, read now in the background (the file framing alone, a
        // second or two; nothing at all when the key index on disk has them): placing and every
        // cell lookup are answered from it.
        let warm = session.clone();
        let _ = std::thread::Builder::new().name("cell-keys".into()).spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| warm.warm_ids(&["CELL"])));
        });
        Links { editor: Arc::new(Mutex::new(editor)), session, seen: Mutex::new(seen), plugins, journal, ..Links::blank() }
    }

    /// Links over an Editor made elsewhere (the tests' sessions).
    pub fn with_editor(editor: Editor) -> Links {
        let seen = editor.journal.as_deref().and_then(stamp);
        let journal = editor.journal.clone();
        let session = editor.session.clone();
        Links { editor: Arc::new(Mutex::new(editor)), session, seen: Mutex::new(seen), journal, ..Links::blank() }
    }

    fn blank() -> Links {
        let session = Arc::new(Session::new(Vec::new()));
        Links {
            editor: Arc::new(Mutex::new(Editor::new(session.clone(), PatchQueue::new(), None))),
            session,
            seen: Mutex::new(None),
            plugins: Vec::new(),
            journal: None,
            builds: crate::buildjob::Builds::new(),
            folders: Vec::new(),
            test_settings: None,
            scratch: std::env::temp_dir().join("wraithguard-test"),
            use_index: None,
            indexing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            verify_warm: Mutex::new(()),
            verify_parts: Mutex::new([0; 4]),
        }
    }

    /// What the Editor reads from the plugins kept in the database at `path` between runs (the
    /// Python Editor's record cache, `wraithguard_record_cache.sqlite3`): a plugin's records of a
    /// type and which keys it has, valid while the plugin's size and date are the same.
    pub fn with_record_cache(self, path: PathBuf) -> Links {
        let parser = format!("editcore-records-1-{}", env!("CARGO_PKG_VERSION"));
        self.session.set_disk_cache(Arc::new(crate::cache::Cache::new(path, &parser, crate::cache::RECORD_CACHE_BYTES)));
        self
    }

    /// The Use Report's index, kept in the database at `path`: the first report scans the
    /// plugin files as before and starts indexing the plugins in the background; once all are
    /// in, a report is a query.
    pub fn with_use_index(mut self, path: PathBuf) -> Links {
        self.use_index = Some(Arc::new(crate::cache::Cache::new(path, "editor-use-index", crate::cache::MAX_BYTES)));
        self
    }

    /// Starts indexing the plugins the use index lacks (a plugin new or changed since the last
    /// run), once the world has loaded, so a first Use Report is a query and not a scan of every
    /// file: one background thread, behind the cell index the Editor waits on first. With every
    /// plugin indexed already - the index is kept between runs - it only looks, and ends.
    pub fn start_indexing(self) -> Links {
        let Some(cache) = self.use_index.clone() else { return self };
        if self.indexing.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return self;
        }
        let (session, flag) = (self.session.clone(), self.indexing.clone());
        let spawned = std::thread::Builder::new().name("use-index-start".into()).spawn({
            let flag = flag.clone();
            move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Not before the cell index is read (up to five minutes' wait: it is a
                    // minute at most, and the Editor is unusable without it).
                    for _ in 0..600 {
                        if session.ids_ready("CELL") {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    let paths = session.plugin_paths();
                    if !cache.missing_uses(&paths).is_empty() {
                        crate::uses::index_missing(&cache, &paths, 1);
                    }
                }));
                flag.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        });
        if spawned.is_err() {
            flag.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        self
    }

    /// Indexes the plugins the use index lacks, on two threads of their own, if it is not
    /// being done already.
    fn index_in_background(&self, paths: Vec<PathBuf>) {
        let Some(cache) = self.use_index.clone() else { return };
        if self.indexing.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let flag = self.indexing.clone();
        let spawned = std::thread::Builder::new().name("use-index".into()).spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::uses::index_missing(&cache, &paths, 2)));
            flag.store(false, std::sync::atomic::Ordering::SeqCst);
        });
        if spawned.is_err() {
            self.indexing.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// The setup's data folders, the patch last written, and the Test in OpenMW settings
    /// (what Wraithguard's launch names besides the load order).
    pub fn with_setup(mut self, folders: Vec<String>, last: Option<PathBuf>, test_settings: Option<PathBuf>) -> Links {
        self.folders = folders;
        self.test_settings = test_settings;
        self.editor.lock().unwrap_or_else(|p| p.into_inner()).last_patch = last;
        self
    }

    /// Where the test plugin goes (the tests' own folder).
    pub fn with_scratch(mut self, scratch: PathBuf) -> Links {
        self.scratch = scratch;
        self
    }

    /// What `editColumns` is about to read - every plugin's records of the type (armor's game
    /// settings too) - read now, on several threads, *without* the Editor's lock. The link
    /// itself then finds them in the session's cache and holds the lock only to build the
    /// table. Before this, the read ran under the lock, and an `editRecord` asked for meanwhile
    /// waited for all of it (Static, 2026-11-05: Open said "Reading..." for as long as the tab's
    /// columns took). A request this cannot make sense of is left to the link to refuse; a read
    /// that panics is read again under the lock, where the panic is caught and said.
    fn warm_columns(&self, body: &str) {
        let Ok(req) = body_of(body) else { return };
        let Some(tag) = req.get("tag").and_then(Value::as_str) else { return };
        let tag = crate::search::tag4(tag.trim());
        let tags: Vec<&str> = if tag == "ARMO" { vec![tag.as_str(), "GMST"] } else { vec![tag.as_str()] };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.session.warm(&tags)));
    }

    /// What placing is about to look at - which plugins have the cell, from every plugin's own
    /// cell names and grids - read now, on several threads, without the Editor's lock, as
    /// `warm_columns` does for a type's columns. A request that names the plugins to look in
    /// (`plugins`) reads only those, which the link does itself.
    fn warm_cells(&self, body: &str) {
        let named = body_of(body).ok().and_then(|r| r.get("plugins").and_then(Value::as_array).map(|a| !a.is_empty())).unwrap_or(false);
        if named {
            return;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.session.warm_ids(&["CELL"]);
        }));
    }

    /// What the other slow links are about to read, read now on several threads, without the
    /// Editor's lock (Messages, 2026-11-05: `editTopics` 52 s, `editDialogueFlags` 66 s,
    /// `editDialoguePlay` 71 s, `editLandPending` 40 s, each held the Editor while it read
    /// one plugin after another, and the links asked meanwhile waited).
    fn warm_for(&self, name: &str) {
        if name == "editTopic" {
            // One topic: where each plugin's records of it lie, not every plugin's dialogue read whole.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.session.warm_dialogue_index()));
        } else if DIALOGUE_LINKS.contains(&name) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.session.warm_dialogue()));
        } else if name == "editVerify" {
            // Verify checks the ids its records name against every type's ids over the load
            // order (and each land, and each response's topic): one type after another, and
            // plugin after plugin, with the Editor locked. All of them are read first, on threads.
            let mut tags: Vec<&str> = crate::verify::object_tags();
            tags.extend(VERIFY_TAGS);
            tags.sort_unstable();
            tags.dedup();
            let t_wait = std::time::Instant::now();
            let _one_at_a_time = self.verify_warm.lock().unwrap_or_else(|p| p.into_inner());
            let wait_ms = t_wait.elapsed().as_millis() as u64;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let t = std::time::Instant::now();
                // The ids: from the key index, or the files' framing - no record parsed. Only the
                // game settings are read whole (a few records), and a land is read when a cell asks.
                self.session.warm_ids(&tags);
                let ids_ms = t.elapsed().as_millis() as u64;
                self.session.warm(&["GMST"]);
                let settings_ms = t.elapsed().as_millis() as u64 - ids_ms;
                // Dialogue only when the patch has a response or a topic to check, and then only
                // where each plugin's topics lie: the topics' own records are read when asked.
                let needs = self.editor.lock().unwrap_or_else(|p| p.into_inner()).verify_needs_dialogue();
                if needs {
                    self.session.warm_dialogue_index();
                }
                let dialogue_ms = t.elapsed().as_millis() as u64 - ids_ms - settings_ms;
                *self.verify_parts.lock().unwrap_or_else(|p| p.into_inner()) = [wait_ms, ids_ms, settings_ms, dialogue_ms];
            }));
        } else if name == "editLandPending" {
            // Only when the patch has landscape changes, which are looked up in every plugin's LAND.
            let pending = {
                let ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
                ed.queue.fields().iter().any(|((t, _), _)| t == "Landscape") || ed.queue.new_records().any(|r| r.record_type == "Landscape")
            };
            if pending {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.session.warm(&["LAND"])));
            }
        }
    }

    /// `editSearchAll` over the plugins' own bytes (`search::scan_plugin`): each plugin is
    /// mapped and searched on its own, on a few threads, and only the records the text is in
    /// are parsed. The link's other way, [`Editor::search_all`], reads every record of every
    /// type of every plugin into the session and keeps them: on a big load order that is all
    /// the memory there is (Static, 2026-11-09: the PC frozen, a hard reboot), so the program
    /// never takes it - it stays for the tests' sessions, which have no files (`None` here).
    ///
    /// A match counts when its plugin is the last that has the record: asked of the use index
    /// when it has every plugin, else of the key index for the types a file's framing keys
    /// (a response or a topic then counts from the last plugin the text is in). The records
    /// the patch changes are looked at as it leaves them.
    fn search_all_link(&self, body: &str) -> Option<Res<String>> {
        if !self.session.scans_files() {
            return None;
        }
        let started = std::time::Instant::now();
        let req = match body_of(body) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        let text = req.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let whole = req.get("whole").is_some_and(crate::session::truthy);
        if text.is_empty() {
            return Some(Ok(json!({"hits": [], "count": 0, "capped": false}).to_string()));
        }
        let finder = crate::search::Finder::new(&text, whole);
        let paths = self.session.plugin_paths();
        // Each plugin searched alone; the biggest first, so one of them is not left for last.
        let mut found: Vec<Vec<crate::search::Match>> = (0..paths.len()).map(|_| Vec::new()).collect();
        if let Some(needle) = crate::search::needle(&text) {
            let mut by_size: Vec<usize> = (0..paths.len()).collect();
            by_size.sort_by_key(|&i| std::cmp::Reverse(std::fs::metadata(&paths[i]).map(|m| m.len()).unwrap_or(0)));
            let threads = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(2)).clamp(1, 6).min(paths.len().max(1));
            let next = std::sync::atomic::AtomicUsize::new(0);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    let tx = tx.clone();
                    let (next, by_size, paths, needle, finder) = (&next, &by_size, &paths, &needle, &finder);
                    scope.spawn(move || {
                        while let Some(&i) = by_size.get(next.fetch_add(1, std::sync::atomic::Ordering::SeqCst)) {
                            let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                crate::session::with_file_bytes(&paths[i], |bytes| Ok(crate::search::scan_plugin(bytes, needle, finder))).unwrap_or_default()
                            }))
                            .unwrap_or_default();
                            if tx.send((i, got)).is_err() {
                                break;
                            }
                        }
                    });
                }
                drop(tx);
                for (i, got) in rx {
                    found[i] = got;
                }
            });
        }
        // The last plugin the text is in, for each record.
        let mut last: std::collections::BTreeMap<(String, String, String), (usize, crate::search::Match)> = std::collections::BTreeMap::new();
        for (i, matches) in found.into_iter().enumerate() {
            for m in matches {
                last.insert((m.tag.clone(), m.record_type.clone(), m.key.clone()), (i, m));
            }
        }
        // And whether that plugin is the last that has the record at all.
        let keys: Vec<(String, String)> = last.keys().map(|(_, t, k)| (t.clone(), k.clone())).collect();
        let definers = self.use_index.as_ref().and_then(|c| c.definers(&paths, &keys));
        if definers.is_none() {
            let mut tags: Vec<&str> = last.keys().map(|(g, _, _)| g.as_str()).filter(|g| Session::key_only(g)).collect();
            tags.sort_unstable();
            tags.dedup();
            self.session.warm_ids(&tags);
        }
        let order = self.session.order().to_vec();
        last.retain(|(tag, rtype, key), (at, _)| match &definers {
            Some(d) => d.get(&(rtype.clone(), key.clone())).and_then(|v| v.last()).is_none_or(|w| w == at),
            None if Session::key_only(tag) => self.session.id_holders(tag, key).last().is_none_or(|w| order.get(*at).is_some_and(|n| n == w)),
            None => true,
        });
        // What the patch changes, as it leaves it.
        let mut ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
        self.sync(&mut ed);
        let overlaid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut out = Vec::new();
            for (tag, rtype, key) in ed.search_all_touched() {
                out.push(((tag.clone(), rtype, key.clone()), ed.search_all_one(&tag, &key, &finder)));
            }
            out
        }));
        drop(ed);
        let Ok(overlaid) = overlaid else { return Some(Err("the viewer's Editor failed on editSearchAll".into())) };
        for (k, got) in overlaid {
            last.remove(&k);
            if let Some((id, fields)) = got.filter(|(_, f)| !f.is_empty()) {
                let m = crate::search::Match { tag: k.0.clone(), record_type: k.1.clone(), key: k.2.clone(), id, fields };
                last.insert(k, (usize::MAX, m));
            }
        }
        let mut hits = Vec::new();
        let mut count = 0usize;
        let records = last.len();
        for (_, (_, m)) in last {
            for (path, value) in m.fields {
                count += 1;
                if hits.len() < SEARCH_ALL_LIMIT {
                    hits.push(json!({"tag": m.tag, "id": m.id, "path": path, "value": value}));
                }
            }
        }
        Some(Ok(json!({
            "hits": hits, "count": count, "capped": count > SEARCH_ALL_LIMIT,
            "took": {"ms": started.elapsed().as_millis() as u64, "records": records, "plugins": paths.len()},
        })
        .to_string()))
    }

    /// `editUses`: the Use Report scans every plugin file (seconds on a big load order), so
    /// the Editor is locked to find the record and again to answer, not across the scan -
    /// a record opened meanwhile is not held behind it. `None` when the session reads
    /// through `read_all` (the tests): the usual link answers then.
    fn uses_link(&self, body: &str) -> Option<Res<String>> {
        let prepared = {
            let mut ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
            self.sync(&mut ed);
            if !ed.session.scans_files() {
                return None;
            }
            record_body(body).and_then(|req| record_request(&ed, &req)).map(|found| (found, ed.session.plugin_paths()))
        };
        Some(prepared.and_then(|(found, paths)| {
            // From the index when every plugin is in it (an id of words: the rest are searched
            // in the script source, which this does not do); else the files are scanned, and the
            // plugins the index lacks are indexed behind this answer, for the next.
            let indexed = self.use_index.as_ref().and_then(|cache| {
                if crate::uses::indexable(&found.key) {
                    return crate::uses::from_index(cache, &paths, &found.record_type, &found.key, |_| Vec::new());
                }
                // An id with a space, a dash or another non-word character (`^_l_imp_corner_01`):
                // the index has every string that equals it, and the scripts and responses are
                // searched for the rest, as the Python Editor did - once the index has every
                // plugin, else the files are scanned. Their records come from the disk cache,
                // and only those that mention the id are copied.
                if !cache.missing_uses(&paths).is_empty() {
                    return None;
                }
                self.session.warm(&["SCPT"]);
                self.session.warm_dialogue();
                let order = self.session.order().to_vec();
                crate::uses::from_index(cache, &paths, &found.record_type, &found.key, |i| {
                    let Some(name) = order.get(i) else { return Vec::new() };
                    let scripts = self.session.records(name, "SCPT");
                    let dialogue = self.session.dialogue(name);
                    crate::uses::with_mentions(&found.key, scripts.values().chain(dialogue.iter()))
                })
            });
            let mut index_state = None;
            let hits = match indexed {
                Some(hits) => hits,
                None => {
                    if let Some(cache) = &self.use_index {
                        let missing = cache.missing_uses(&paths).len();
                        index_state = Some(json!({"indexed": paths.len() - missing.min(paths.len()), "total": paths.len(), "building": true}));
                    }
                    self.index_in_background(paths.clone());
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::uses::report(&paths, &found.record_type, &found.key)))
                        .map_err(|_| "the viewer's Editor failed on editUses".to_string())?
                }
            };
            // The answer names the other object types the id is also defined as, which reads
            // every object type of every plugin: on threads, and not with the Editor locked
            // (Messages, 2026-11-05: `editUses` 101 s, `editRevision` 94 s behind it).
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Which plugins have the id as another type: the ids, from the key index or the files'
                // framing, no record parsed (every record of every object type was read here).
                self.session.warm_ids(&crate::verify::object_tags().into_iter().filter(|t| *t != found.tag.as_str()).collect::<Vec<_>>())
            }));
            let ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
            let mut answer = ed.uses_from(&found, &hits);
            if let Some(state) = index_state {
                answer["index"] = state;
            }
            Ok(answer.to_string())
        }))
    }

    /// The queue read again from the journal, under the Editor's lock, and let go.
    fn synced(&self) {
        let mut ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
        self.sync(&mut ed);
    }

    /// A build link (`BUILD_LINKS`): the build runs on a thread of its own, with the
    /// Editor locked only to gather the queue and to spend it.
    fn build_link(&self, name: &str, body: &str) -> Res<String> {
        use crate::buildjob as bj;
        let req = body_of(body)?;
        let out = match name {
            "editBuildInfo" => {
                self.synced();
                bj::build_info(&self.editor.lock().unwrap_or_else(|p| p.into_inner()), &self.folders)
            }
            "editBuildCheck" => {
                let target = bj::patch_target(&req)?;
                bj::build_check(&target, self.editor.lock().unwrap_or_else(|p| p.into_inner()).session.order())
            }
            "editBuild" => {
                self.synced();
                self.builds.build(&self.editor, &req)?
            }
            "editBuildStatus" => self.builds.status(&req)?,
            "editTestSetups" => bj::test_setups(self.test_settings.as_deref().ok_or_else(|| "Test in OpenMW needs Wraithguard: open the viewer from it".to_string())?, &req)?,
            "editTestRun" => {
                self.synced();
                let settings = self.test_settings.clone().ok_or_else(|| "Test in OpenMW needs Wraithguard: open the viewer from it".to_string())?;
                let _ = std::fs::create_dir_all(&self.scratch);
                self.builds.test_run(&self.editor, &req, &settings, &self.scratch)?
            }
            other => return Err(format!("{other} is not a link the viewer answers")),
        };
        Ok(out.to_string())
    }

    /// Whether this side answers `name`.
    pub fn answers(name: &str) -> bool {
        ANSWERS.contains(&name)
    }

    /// The queue read again from the journal when another side changed it since this
    /// one last saw it: as one step, which an undo takes back. The journal's stamp
    /// after it.
    pub fn sync(&self, ed: &mut Editor) -> Option<(u64, SystemTime)> {
        let journal = ed.journal.clone()?;
        let now = stamp(&journal);
        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        if *seen != now {
            ed.queue.clear();
            if now.is_some() {
                ed.queue.restore(&journal);
            }
            ed.history.record(&ed.queue);
            *seen = now;
        }
        now
    }

    /// One link: its answer's text, or the reason it is refused (as Wraithguard's 400
    /// says it). A link this side does not answer is refused as unknown.
    /// A link that panics (a bug) is refused with what it said, and leaves the Editor
    /// usable: the next link finds it as the panic left it.
    pub fn call(&self, name: &str, body: &str) -> Res<String> {
        self.call_with(name, body, &Ctx::default())
    }

    /// [`Links::call`] with what the viewer gives besides (`ctx`).
    pub fn call_with(&self, name: &str, body: &str, ctx: &Ctx) -> Res<String> {
        if BUILD_LINKS.contains(&name) {
            return self.build_link(name, body);
        }
        if name == "editColumns" {
            self.warm_columns(body);
        }
        if CELL_LINKS.contains(&name) {
            self.warm_cells(body);
        }
        let t_read = std::time::Instant::now();
        self.warm_for(name);
        let read_ms = t_read.elapsed().as_millis();
        if name == "editUses"
            && let Some(out) = self.uses_link(body)
        {
            return out;
        }
        if name == "editSearchAll"
            && let Some(out) = self.search_all_link(body)
        {
            return out;
        }
        let t_lock = std::time::Instant::now();
        let mut ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
        let lock_ms = t_lock.elapsed().as_millis();
        let t_check = std::time::Instant::now();
        let before = self.sync(&mut ed);
        let revision = ed.queue.revision;
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| answer(&mut ed, name, body, ctx))).unwrap_or_else(|p| {
            let said = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            Err(format!("the viewer's Editor failed on {name}: {said}"))
        });
        // A change of this side's: the journal as it wrote it is the one it has seen. No
        // change: what it saw before stays, so a write by another side meanwhile is read
        // next time.
        let journal = ed.journal.clone();
        *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = if ed.queue.revision != revision { journal.as_deref().and_then(stamp) } else { before };
        let check_ms = t_check.elapsed().as_millis();
        out.map(|a| {
            let text = a.text();
            // Verify is the one link that takes minutes: where they go is said in its answer
            // (`took`: reading the load order before the lock, waiting for the Editor, checking),
            // when it took 1.5 s or more - a quick one answers exactly as the reference does.
            if name == "editVerify"
                && read_ms + lock_ms + check_ms >= 1500
                && let Ok(mut v) = serde_json::from_str::<Value>(&text)
                && v.is_object()
            {
                let [wait, ids, settings, dialogue] = *self.verify_parts.lock().unwrap_or_else(|p| p.into_inner());
                v["took"] = json!({"read_ms": read_ms as u64, "lock_ms": lock_ms as u64, "check_ms": check_ms as u64,
                    "parts": {"wait_ms": wait, "ids_ms": ids, "settings_ms": settings, "dialogue_ms": dialogue}});
                if let Some((bytes, drops)) = self.session.disk_cache_info() {
                    v["took"]["cache"] = json!({"mb": bytes / (1 << 20), "drops": drops});
                }
                return v.to_string();
            }
            text
        })
    }
}

/// The links that look for the plugins holding a cell (`Editor::cell_plugins`,
/// `Editor::in_load_order`): the first of them reads every plugin's CELL records, which is
/// seconds over a big load order (`warm_cells` does it before the Editor is locked).
const CELL_LINKS: &[&str] = &["editPlace", "editPlaceMany", "editNewCell", "editDuplicateCell"];

/// The record types Verify looks ids up in besides the object types (`verify.rs`).
const VERIFY_TAGS: [&str; 14] = ["CELL", "CLAS", "ENCH", "FACT", "GLOB", "GMST", "LAND", "PGRD", "RACE", "REGN", "SCPT", "SOUN", "SPEL", "BODY"];

/// The links that read the topics and responses of the plugins (`Session::dialogue`).
const DIALOGUE_LINKS: &[&str] = &[
    "editTopics",
    "editNewResponse",
    "editMoveResponse",
    "editCopyTopic",
    "editDialogueFlow",
    "editDialogueMap",
    "editDialogueFlags",
    "editDialoguePlay",
    "editWhoCanSay",
];

/// A request's JSON object (`_body`).
fn body_of(body: &str) -> Res<Value> {
    match serde_json::from_str::<Value>(body) {
        Ok(v @ Value::Object(_)) => Ok(v),
        _ => Err("bad request".into()),
    }
}

/// A record's tag as the record link checks it: four letters, digits or `_`.
fn good_tag(tag: &str) -> bool {
    tag.len() == 4 && tag.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Python's `str(req.get(k) or "")`.
fn text_or_empty(v: Option<&Value>) -> String {
    match v {
        Some(v) if crate::session::truthy(v) => crate::pyval::str_of(v),
        _ => String::new(),
    }
}

/// A list of plugin names from a request, `None` when absent or empty (`_names`).
fn names(v: Option<&Value>, what: &str) -> Res<Option<Vec<String>>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.iter().all(Value::is_string) => {
            let out: Vec<String> = a.iter().map(|s| s.as_str().unwrap().to_string()).collect();
            Ok((!out.is_empty()).then_some(out))
        }
        _ => Err(format!("bad {what}")),
    }
}

/// A request naming a record (`{tag, id, plugins?}`), and the record (`_edit_request`).
fn record_request(ed: &Editor, req: &Value) -> Res<Found> {
    let tag = text_or_empty(req.get("tag"));
    let rid = text_or_empty(req.get("id")).trim().to_string();
    if !tag.is_empty() && !good_tag(&tag) {
        return Err("bad tag".into());
    }
    if rid.is_empty() || rid.chars().count() > 256 || rid.contains(['\r', '\n', '\0']) {
        return Err("bad id".into());
    }
    let plugins = names(req.get("plugins"), "plugins")?;
    ed.find(&tag, &rid, plugins.as_deref()).ok_or_else(|| {
        if ed.queued_without_record(&tag, &rid) {
            format!("{rid}: the patch changes it, but no plugin of this load order has it now. The plugin it was changed in has left the load order, or was written again without it (a patch written over itself, most likely). Delete these changes in Build patch, or put that plugin back in the load order.")
        } else {
            format!("{rid} is not a record any plugin of this load order defines")
        }
    })
}

/// The record request's own refusal of a body that is not a JSON object.
fn record_body(body: &str) -> Res<Value> {
    body_of(body).map_err(|_| "not a record request".into())
}

/// A non-empty text field of a request, or the refusal `bad <what>`.
fn need_text(req: &Value, key: &str, what: &str) -> Res<String> {
    match req.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(format!("bad {what}")),
    }
}

/// A text field with something besides spaces in it (`not x.strip()` refused).
fn need_words(req: &Value, key: &str, what: &str) -> Res<String> {
    match req.get(key).and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => Ok(s.to_string()),
        _ => Err(format!("bad {what}")),
    }
}

/// A request naming a placed object (`{cell, origin, refr, plugins?}`, `_ref_request`).
fn ref_request(ed: &Editor, req: &Value) -> Res<FoundRef> {
    let cell = req.get("cell").and_then(Value::as_str).filter(|s| !s.is_empty());
    let origin = req.get("origin").and_then(Value::as_str).filter(|s| !s.is_empty());
    let (Some(cell), Some(origin)) = (cell, origin) else { return Err("bad request".into()) };
    let refr = match req.get("refr") {
        Some(v) if crate::pyval::kind(v) == crate::pyval::Kind::Int => v.as_i64().filter(|n| *n >= 0),
        _ => None,
    };
    let Some(refr) = refr else { return Err("bad refr".into()) };
    let plugins = names(req.get("plugins"), "plugins")?;
    ed.session.find_ref(cell, origin, refr, plugins.as_deref()).ok_or_else(|| format!("No plugin of this load order has reference {origin}:{refr} in {cell}"))
}

/// A request naming a reference the patch adds (`{cell, uid}`, `_new_request`).
fn new_request(ed: &Editor, req: &Value) -> Res<crate::queue::NewRef> {
    let cell = req.get("cell").and_then(Value::as_str).filter(|s| !s.is_empty());
    let uid = req.get("uid").and_then(Value::as_str).filter(|s| !s.is_empty());
    let (Some(_), Some(uid)) = (cell, uid) else { return Err("bad request".into()) };
    ed.find_new(uid).ok_or_else(|| "That new reference is no longer in the patch".into())
}

/// Python's `int(v) if isinstance(v, (int, float)) else default` (a bool is an int).
fn num(v: Option<&Value>, default: i64) -> i64 {
    match v {
        Some(Value::Bool(b)) => i64::from(*b),
        Some(Value::Number(n)) => n.as_i64().unwrap_or_else(|| n.as_f64().map_or(default, |f| f.trunc() as i64)),
        _ => default,
    }
}

/// An optional text field of a request: absent or null is `None`, text is itself, else
/// `bad <what>`.
fn opt_text(req: &Value, key: &str, what: &str) -> Res<Option<String>> {
    match req.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(format!("bad {what}")),
    }
}

/// One link answered against the Editor.
pub fn answer(ed: &mut Editor, name: &str, body: &str, ctx: &Ctx) -> Res<Answer> {
    use Answer::Json;
    Ok(match name {
        "editRecord" => {
            let req = record_body(body)?;
            let found = record_request(ed, &req)?;
            Json(ed.view(&found))
        }
        "editSet" => {
            let req = record_body(body)?;
            let found = record_request(ed, &req)?;
            let path = need_text(&req, "path", "path")?;
            let value = req.get("value").cloned().unwrap_or(Value::Null);
            if found.record_type == "Script" && path == "text" && value.is_string() {
                let compiler = ed.script_compiler();
                let compiled = edit(ed.set_script_text(&found, value.as_str().unwrap(), compiler.as_ref()))?;
                let mut out = ed.view(&found);
                out["compile"] = compiled;
                Json(out)
            } else {
                edit(ed.set_field(&found, &path, &value))?;
                Json(ed.view(&found))
            }
        }
        "editSetMany" => {
            let req = body_of(body)?;
            let tag = need_words(&req, "tag", "tag")?;
            let ids: Vec<String> = match req.get("ids").and_then(Value::as_array) {
                Some(a) if !a.is_empty() && a.iter().all(Value::is_string) => a.iter().map(|i| i.as_str().unwrap().to_string()).collect(),
                _ => return Err("ids is a list of record ids".into()),
            };
            let path = need_text(&req, "path", "path")?;
            // Python's dict of id -> found: an id given twice is looked up once.
            let mut seen = indexmap::IndexMap::new();
            for rid in &ids {
                if !seen.contains_key(rid) {
                    seen.insert(rid.clone(), ed.find(&tag, rid, None));
                }
            }
            let founds: Vec<Found> = seen.values().flatten().cloned().collect();
            let missing: Vec<&String> = seen.iter().filter(|(_, f)| f.is_none()).map(|(k, _)| k).collect();
            let value = req.get("value").cloned().unwrap_or(Value::Null);
            let mut out = ed.set_many(&founds, &path, &value);
            out["missing"] = json!(missing);
            Json(out)
        }
        "editRevert" => {
            let req = record_body(body)?;
            // Changes to a record no plugin has now cannot be opened, but can be taken out.
            if req.get("path").and_then(Value::as_str).is_none_or(str::is_empty)
                && ed.drop_orphan(&text_or_empty(req.get("tag")), text_or_empty(req.get("id")).trim())
            {
                return Ok(Json(json!({"dropped": true})));
            }
            let found = record_request(ed, &req)?;
            let path = req.get("path").and_then(Value::as_str).filter(|p| !p.is_empty());
            edit(ed.revert(&found, path))?;
            Json(ed.view(&found))
        }
        "editInsert" => {
            let req: Value = serde_json::from_str(body).map_err(|_| "bad request".to_string())?;
            let (tag, new_id) = (req.get("tag").and_then(Value::as_str), req.get("newId").and_then(Value::as_str));
            let (Some(tag), Some(new_id)) = (tag, new_id) else { return Err("bad request".into()) };
            let made = edit(ed.insert(tag, new_id))?;
            Json(ed.view(&made))
        }
        "editDuplicate" => {
            let req = record_body(body)?;
            let found = record_request(ed, &req)?;
            let Some(new_id) = req.get("newId").and_then(Value::as_str) else { return Err("bad newId".into()) };
            let copy = edit(ed.duplicate(&found, new_id))?;
            Json(ed.view(&copy))
        }
        "editRef" => {
            let found = ref_request(ed, &body_of(body)?)?;
            Json(ed.ref_view(&found))
        }
        "editRefSet" => {
            let req = body_of(body)?;
            let found = ref_request(ed, &req)?;
            let path = need_text(&req, "path", "path")?;
            edit(ed.set_ref_field(&found, &path, req.get("value").unwrap_or(&Value::Null)))?;
            // The reference as it is now (a move puts it in another cell's record).
            Json(ed.ref_view(&found))
        }
        "editRefRevert" => {
            let req = body_of(body)?;
            let path = req.get("path").and_then(Value::as_str).filter(|p| !p.is_empty());
            let found = match ref_request(ed, &req) {
                Ok(f) => f,
                // A placed object no plugin has now: its queued changes are still taken out.
                Err(e) => {
                    let s = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    let refr = req.get("refr").and_then(Value::as_i64).unwrap_or(-1);
                    if path.is_none() && ed.drop_orphan_ref(&s("cell"), &s("origin"), refr) {
                        return Ok(Json(json!({"dropped": true})));
                    }
                    return Err(e);
                }
            };
            ed.revert_ref(&found, path);
            Json(ed.ref_view(&found))
        }
        "editRefsSet" => {
            let req = body_of(body)?;
            let changes = match req.get("changes").and_then(Value::as_array) {
                Some(c) if !c.is_empty() => c.clone(),
                _ => return Err("changes is a list".into()),
            };
            enum Target {
                Placed(FoundRef),
                New(crate::queue::NewRef),
            }
            let mut steps = Vec::new();
            for item in &changes {
                let Some(path) = item.get("path").and_then(Value::as_str).filter(|_| item.is_object()) else {
                    return Err("each change is {reference, path, value}".into());
                };
                let target = if item.get("uid").is_some_and(crate::session::truthy) { Target::New(new_request(ed, item)?) } else { Target::Placed(ref_request(ed, item)?) };
                steps.push((target, path.to_string(), item.get("value").cloned().unwrap_or(Value::Null)));
            }
            let n = steps.len();
            edit(ed.batch(|ed| {
                for (target, path, value) in &steps {
                    match target {
                        // A new reference changed twice is found again, as it is now.
                        Target::New(new) => {
                            let now = ed.find_new(&new.uid).unwrap_or_else(|| new.clone());
                            ed.set_new_field(&now, path, value)?;
                        }
                        Target::Placed(found) => ed.set_ref_field(found, path, value)?,
                    }
                }
                Ok(())
            }))?;
            Json(json!({"changed": n}))
        }
        "editPlace" => {
            let place = body_of(body)?;
            let mut rec = json!({"tag": place.get("tag").cloned().unwrap_or(Value::Null), "id": place.get("id").cloned().unwrap_or(Value::Null)});
            if place.get("defined").is_some_and(|d| !d.is_null()) {
                rec["plugins"] = place["defined"].clone();
            }
            let found = record_request(ed, &rec)?;
            let Some(cell) = place.get("cell").and_then(Value::as_str).filter(|c| !c.is_empty()) else { return Err("bad cell".into()) };
            let cell_plugins = names(place.get("plugins"), "plugins")?;
            let rotation = place.get("rotation").filter(|r| !r.is_null());
            let new = edit(ed.place(cell, &found, place.get("translation").unwrap_or(&Value::Null), rotation, cell_plugins.as_deref()))?;
            Json(ed.new_view(&new))
        }
        "editPlaceMany" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            let items = match req.get("items").and_then(Value::as_array) {
                Some(i) if !i.is_empty() => i.clone(),
                _ => return Err("items is a list".into()),
            };
            let mut todo = Vec::new();
            for it in &items {
                if !it.is_object() {
                    return Err("each item is {tag, id, translation}".into());
                }
                let found = ed.find(&text_or_empty(it.get("tag")), &text_or_empty(it.get("id")), None);
                let Some(found) = found else {
                    return Err(format!("{} is not a record of this load order", crate::pyval::str_of(it.get("id").unwrap_or(&Value::Null))));
                };
                // Python's `isinstance(scale, (int, float))`: a bool is one.
                let scale = match it.get("scale") {
                    Some(Value::Bool(b)) => f64::from(u8::from(*b)),
                    Some(v) if v.is_number() => v.as_f64().unwrap_or(1.0),
                    _ => 1.0,
                };
                let rotation = it.get("rotation").filter(|r| !r.is_null()).cloned();
                todo.push((found, it.get("translation").cloned().unwrap_or(Value::Null), rotation, scale));
            }
            let placed = edit(ed.place_many(&cell, &todo))?;
            Json(json!({"placed": placed.iter().map(|n| json!({"cell": n.cell, "uid": n.uid})).collect::<Vec<_>>()}))
        }
        "editNew" => {
            let new = new_request(ed, &body_of(body)?)?;
            Json(ed.new_view(&new))
        }
        "editNewSet" => {
            let req = body_of(body)?;
            let new = new_request(ed, &req)?;
            let path = need_text(&req, "path", "path")?;
            let now = edit(ed.set_new_field(&new, &path, req.get("value").unwrap_or(&Value::Null)))?;
            Json(ed.new_view(&now))
        }
        "editNewRemove" => {
            // One no longer in the patch is out of it already: nothing to refuse.
            let new = match new_request(ed, &body_of(body)?) {
                Ok(n) => n,
                Err(e) if e.contains("no longer in the patch") => return Ok(Answer::Text("ok".into())),
                Err(e) => return Err(e),
            };
            ed.remove_new(&new);
            Answer::Text("ok".into())
        }
        "editNewCell" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            let absent = req.get("absent") == Some(&Value::Bool(true));
            Json(edit(ed.new_cell(&cell, absent))?)
        }
        "editNewLand" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            // The ground an empty square shows (the CS's and OpenMW's), unless a height is given.
            let height = match req.get("height") {
                None => crate::places::DEFAULT_LAND_HEIGHT,
                Some(v) if v.is_number() => v.as_f64().unwrap_or(0.0),
                _ => return Err("bad height".into()),
            };
            let texture = match req.get("texture") {
                None => String::new(),
                Some(Value::String(s)) => s.clone(),
                _ => return Err("bad texture".into()),
            };
            let no_land = req.get("noLand") == Some(&Value::Bool(true));
            let has_cell = match req.get("hasCell") {
                None | Some(Value::Null) => None,
                Some(Value::Bool(b)) => Some(*b),
                _ => return Err("bad hasCell".into()),
            };
            Json(edit(ed.new_land(&cell, height, &texture, no_land, has_cell))?)
        }
        "editLandSet" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            let Some(heights) = req.get("heights").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<f64>>()) else {
                return Err("bad heights".into());
            };
            Json(edit(ed.set_land(&cell, &heights))?)
        }
        "editLandTextures" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            let Some(ids) = req.get("ids").and_then(Value::as_array).map(|a| a.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect::<Vec<String>>()) else {
                return Err("bad ids".into());
            };
            Json(edit(ed.set_land_textures(&cell, &ids))?)
        }
        "editLandColors" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            let Some(colors) = req.get("colors").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<f64>>()) else {
                return Err("bad colors".into());
            };
            Json(edit(ed.set_land_colors(&cell, &colors))?)
        }
        "editCellRegions" => {
            let req = body_of(body)?;
            let Some(cells) = req.get("cells").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<String>>())
            else {
                return Err("bad cells".into());
            };
            let region = req.get("region").and_then(Value::as_str).unwrap_or("").to_string();
            let make = req.get("make") == Some(&Value::Bool(true));
            Json(edit(ed.set_cell_regions(&cells, &region, make))?)
        }
        "editDuplicateCell" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            let Some(new_name) = req.get("newName").and_then(Value::as_str) else { return Err("bad newName".into()) };
            Json(edit(ed.duplicate_cell(&cell, new_name))?)
        }
        "editPathgrid" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            Json(ed.pathgrid_view(&cell))
        }
        "editPathgridSet" => {
            let req = body_of(body)?;
            let cell = need_words(&req, "cell", "cell")?;
            if req.get("revert").is_some_and(crate::session::truthy) {
                return Ok(Json(ed.revert_pathgrid(&cell)));
            }
            let (Some(points), Some(edges)) = (req.get("points").and_then(Value::as_array), req.get("edges").and_then(Value::as_array)) else {
                return Err("points and edges are lists".into());
            };
            Json(edit(ed.set_pathgrid(&cell, points, edges))?)
        }
        "editSearch" => {
            let req = body_of(body)?;
            let tag = req.get("tag").and_then(Value::as_str).filter(|t| !t.trim().is_empty());
            let (Some(tag), Some(query)) = (tag, req.get("query").and_then(Value::as_str)) else { return Err("bad request".into()) };
            Json(edit(ed.search(tag, query, SEARCH_LIMIT))?)
        }
        "editSearchAll" => {
            let req = body_of(body)?;
            let text = need_words(&req, "text", "text")?;
            let whole = req.get("whole").is_some_and(crate::session::truthy);
            Json(ed.search_all(&text, whole, SEARCH_ALL_LIMIT))
        }
        "editReplaceAll" => {
            let req = body_of(body)?;
            let hits = req.get("hits").and_then(Value::as_array);
            let text = req.get("text").and_then(Value::as_str).filter(|t| !t.is_empty());
            let (Some(hits), Some(text)) = (hits, text) else { return Err("bad request".into()) };
            let Some(by) = req.get("by").and_then(Value::as_str) else { return Err("by is text".into()) };
            let whole = req.get("whole").is_some_and(crate::session::truthy);
            let (hits, text, by) = (hits.clone(), text.to_string(), by.to_string());
            Json(ed.replace_all(&hits, &text, &by, whole))
        }
        "editColumns" => {
            let req = body_of(body)?;
            let tag = need_words(&req, "tag", "tag")?;
            let mut out = ed.columns(&tag.trim().to_uppercase());
            // The records the plugin being edited has itself (`own`: its name), to mark.
            if let Some(own) = req.get("own").and_then(Value::as_str).filter(|o| !o.is_empty())
                && ed.session.order().iter().any(|n| n.eq_ignore_ascii_case(own))
            {
                let keys: Vec<String> = ed.session.records(own, &tag.trim().to_uppercase()).keys().cloned().collect();
                out["own"] = json!(keys);
            }
            Json(out)
        }
        // The plugin the pool is for (`queue.active`): `{}` asks; `{name}` names it - at once
        // with an empty pool or `force`, else only told (`differs`), for the page to ask
        // first. `{active, records, differs}`.
        "editActive" => {
            let req = body_of(body)?;
            let mut differs = false;
            if let Some(name) = req.get("name").and_then(Value::as_str) {
                let name = name.trim();
                if !name.eq_ignore_ascii_case(&ed.queue.active) {
                    if ed.queue.is_empty() || req.get("force").is_some_and(crate::session::truthy) {
                        ed.queue.active = name.to_string();
                        ed.save_journal();
                    } else {
                        differs = true;
                    }
                }
            }
            Json(json!({"active": ed.queue.active, "records": ed.queue.len(), "differs": differs}))
        }
        "editFilterChoices" => Json(crate::fields::filter_choices()),
        "editUndo" => Json(ed.undo()),
        "editRedo" => Json(ed.redo()),
        "editRevision" => {
            let mut out = json!({"rev": ed.queue.revision});
            if let Value::Object(m) = ed.can_undo() {
                for (k, v) in m {
                    out[k] = v;
                }
            }
            Json(out)
        }
        "editPending" => Json(Value::Array(ed.pending())),
        "editLandPending" => Json(ed.land_pending()),
        // -- 7b ----------------------------------------------------------------------
        "editUses" => {
            let found = record_request(ed, &record_body(body)?)?;
            Json(ed.uses(&found))
        }
        "editReplace" => {
            let req = record_body(body)?;
            let found = record_request(ed, &req)?;
            let new_id = need_words(&req, "newId", "newId")?;
            let cell = match req.get("cell") {
                None | Some(Value::Null) => None,
                Some(Value::String(c)) if !c.trim().is_empty() => Some(c.clone()),
                _ => return Err("bad cell".into()),
            };
            let (plan, _report) = edit(ed.replace_plan(&found, new_id.trim(), cell.as_deref()))?;
            let compiler = ed.script_compiler();
            let changed = edit(ed.replace_uses(&plan, compiler.as_ref()))?;
            Json(crate::uses::replace_summary(&plan, changed))
        }
        "editScript" => {
            let req = record_body(body)?;
            let found = record_request(ed, &req)?;
            let text = opt_text(&req, "text", "text")?;
            Json(edit(ed.edit_script(&found, text.as_deref()))?)
        }
        "editResult" => {
            let req = record_body(body)?;
            let found = record_request(ed, &req)?;
            let text = opt_text(&req, "text", "text")?;
            Json(ed.edit_result(&found, text.as_deref()))
        }
        "editScriptWords" => Json(ed.script_words()),
        "editScriptRefs" => {
            let req = body_of(body)?;
            let word = need_words(&req, "word", "word")?;
            Json(ed.script_refs(&word, SCRIPT_REFS_LIMIT))
        }
        "editLeveledRoll" => {
            let req = body_of(body)?;
            Json(edit(ed.edit_leveled_roll(&text_or_empty(req.get("tag")), &text_or_empty(req.get("id")), req.get("level").unwrap_or(&Value::Null)))?)
        }
        "editTopics" => Json(Value::Array(ed.topics())),
        "editTopic" => {
            let req: Value = serde_json::from_str(body).map_err(|_| "bad request".to_string())?;
            let topic = need_words(&req, "topic", "topic")?;
            Json(edit(ed.topic(&topic))?)
        }
        "editNewResponse" => {
            let req = body_of(body)?;
            let topic = req.get("topic").and_then(Value::as_str).filter(|t| !t.trim().is_empty());
            let after = match req.get("after") {
                None => Some(""),
                Some(v) => v.as_str(),
            };
            let (Some(topic), Some(after)) = (topic, after) else { return Err("bad request".into()) };
            let (topic, after) = (topic.to_string(), after.to_string());
            let made = edit(ed.new_response(&topic, &after))?;
            Json(ed.view(&made))
        }
        "editMoveResponse" => {
            let req = body_of(body)?;
            let after = match req.get("after") {
                None => Some(""),
                Some(v) => v.as_str(),
            };
            let (topic, rid) = (req.get("topic").and_then(Value::as_str), req.get("id").and_then(Value::as_str));
            let (Some(topic), Some(rid), Some(after)) = (topic, rid, after) else { return Err("bad request".into()) };
            if topic.trim().is_empty() {
                return Err("bad request".into());
            }
            let (topic, rid, after) = (topic.to_string(), rid.to_string(), after.to_string());
            Json(edit(ed.move_response(&topic, &rid, &after))?)
        }
        "editCopyTopic" => {
            let req = body_of(body)?;
            let (topic, new_id) = (req.get("topic").and_then(Value::as_str), req.get("newId").and_then(Value::as_str));
            let (Some(topic), Some(new_id)) = (topic, new_id) else { return Err("bad request".into()) };
            let (topic, new_id) = (topic.to_string(), new_id.to_string());
            Json(edit(ed.copy_topic(&topic, &new_id))?)
        }
        "editDialogueFlow" => {
            let topic = need_words(&body_of(body)?, "topic", "topic")?;
            Json(edit(ed.dialogue_index().flow(&topic))?)
        }
        "editDialogueMap" => {
            let req = body_of(body)?;
            let topic = need_words(&req, "topic", "topic")?;
            // Python's `isinstance(depth, int)`: a bool is one.
            let depth = match req.get("depth") {
                Some(Value::Bool(b)) => i64::from(*b),
                Some(v) if crate::pyval::kind(v) == crate::pyval::Kind::Int => v.as_i64().unwrap_or(1),
                _ => 1,
            };
            Json(edit(ed.dialogue_index().neighbourhood(&topic, depth))?)
        }
        "editDialogueFlags" => {
            let req = body_of(body)?;
            let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            Json(edit(ed.dialogue_index().flags(&text("name"), &text("kind")))?)
        }
        "editDialoguePlay" => {
            let req = body_of(body)?;
            let Some(speaker) = req.get("speaker").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) else {
                return Err("Choose who to talk to (an NPC's id)".into());
            };
            let strict = req.get("strict").is_some_and(crate::session::truthy);
            let st = crate::dialogue_graph::PlayState::from_request(&text_or_empty(req.get("cell")), num(req.get("disposition"), 50), strict, req.get("state"));
            Json(edit(ed.dialogue_play(speaker, &text_or_empty(req.get("topic")), num(req.get("choice"), 0), &st))?)
        }
        "editWhoCanSay" => {
            let req = body_of(body)?;
            Json(edit(ed.who_can_say(&text_or_empty(req.get("id")), WHO_CAN_SAY_LIMIT))?)
        }
        "editVerify" => Json(ed.verify_with(ctx.file_exists, ctx.sound_problem)),
        other => return Err(format!("{other} is not a link the viewer answers")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npc(id: &str, level: i64) -> Value {
        let mut r = crate::blank::blank("Npc").unwrap();
        r["id"] = json!(id);
        r["data"]["level"] = json!(level);
        r
    }

    #[test]
    fn the_queue_is_the_journals_and_the_journal_the_queues() {
        let dir = std::env::temp_dir().join(format!("wg-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mw = dir.join("Morrowind.esm");
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 0, "masters": []});
        std::fs::write(&mw, crate::build::patch_bytes(&[header, npc("fargoth", 2), npc("caius", 3)]).unwrap()).unwrap();
        let journal = dir.join("wraithguard_patch_journal.json");
        // A journal Wraithguard left: the Editor opens with it.
        std::fs::write(&journal, r#"{"version": 1, "whole": [{"type": "Npc", "key": "caius", "plugin": "Morrowind.esm"}], "fields": [], "refs": [], "made": [], "new": []}"#).unwrap();
        let links = Links::open(vec![("Morrowind.esm".into(), mw)], Some(journal.clone()));
        let pending: Value = serde_json::from_str(&links.call("editPending", "{}").unwrap()).unwrap();
        assert_eq!(pending[0]["id"], "caius");
        // A change here is in the journal, for Wraithguard to read.
        links.call("editSet", r#"{"tag": "NPC_", "id": "fargoth", "path": "data.level", "value": 9}"#).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&journal).unwrap()).unwrap();
        assert_eq!(doc["fields"][0]["key"], "fargoth");
        // Wraithguard writing it (its Patch Builder wrote the patch and spent the queue).
        std::fs::remove_file(&journal).unwrap();
        let pending: Value = serde_json::from_str(&links.call("editPending", "{}").unwrap()).unwrap();
        assert_eq!(pending, json!([]));
        let undo: Value = serde_json::from_str(&links.call("editUndo", "{}").unwrap()).unwrap();
        assert_eq!(undo["done"], json!(true), "the other side's change is a step: {undo}");
        assert!(journal.exists(), "and undoing it writes the journal back");
        assert_eq!(links.call("editReview", "{}").unwrap_err(), "editReview is not a link the viewer answers");
        assert!(!Links::answers("editReview") && Links::answers("editRecord") && Links::answers("editBuild"));
        // The verifier, with the files the install has (none here but one).
        let have = |p: &str| p == "meshes/x.nif";
        let out: Value = serde_json::from_str(&links.call_with("editVerify", "{}", &Ctx { file_exists: Some(&have), sound_problem: None }).unwrap()).unwrap();
        assert!(out["findings"].is_array());
        // The build links: where the last patch went comes with the launch.
        let links = links.with_setup(vec!["/data".into()], Some(dir.join("Old.esp")), None);
        let info: Value = serde_json::from_str(&links.call("editBuildInfo", "{}").unwrap()).unwrap();
        assert_eq!((info["last"].clone(), info["folders"].clone()), (json!(dir.join("Old.esp").to_string_lossy()), json!(["/data"])));
        assert!(links.call("editTestSetups", "{}").unwrap_err().starts_with("Test in OpenMW needs Wraithguard"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Static, 2026-11-03: two land squares in Build patch, "1 change" each, whose pencil said
    /// "(-69, 23) is not a record any plugin of this load order defines". Changes queued
    /// against a plugin's record that the plugin no longer has are said as that, in the
    /// list and when opened; and a plugin written over (the patch, loaded) is read again.
    #[test]
    fn changes_to_a_record_no_plugin_has_now_are_named_and_a_rewritten_plugin_is_read_again() {
        let dir = std::env::temp_dir().join(format!("wg-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esp", "author": "", "description": "", "num_objects": 0, "masters": []});
        let patch = dir.join("Patch.esp");
        std::fs::write(&patch, crate::build::patch_bytes(&[header.clone(), npc("fargoth", 2)]).unwrap()).unwrap();
        let journal = dir.join("journal.json");
        let links = Links::open(vec![("Patch.esp".into(), patch.clone())], Some(journal));
        links.call("editSet", r#"{"tag": "NPC_", "id": "fargoth", "path": "data.level", "value": 9}"#).unwrap();
        // The plugin written again without the record, and the Editor told (as a build does).
        std::fs::write(&patch, crate::build::patch_bytes(&[header, npc("caius", 3)]).unwrap()).unwrap();
        assert!(links.editor.lock().unwrap_or_else(|p| p.into_inner()).session.forget_file(&patch));
        // The pool's list reads nothing itself (a CELL change would read every plugin, each time
        // it is asked): before the plugin is read again, it cannot say.
        let pending: Value = serde_json::from_str(&links.call("editPending", "{}").unwrap()).unwrap();
        assert!(pending[0].get("missing").is_none(), "nothing read, nothing said: {pending}");
        let e = links.call("editRecord", r#"{"tag": "NPC_", "id": "fargoth"}"#).unwrap_err();
        assert!(e.contains("the patch changes it, but no plugin of this load order has it now"), "{e}");
        let pending: Value = serde_json::from_str(&links.call("editPending", "{}").unwrap()).unwrap();
        assert_eq!(pending[0]["missing"], json!(true), "once read, it is said: {pending}");
        // And they can be taken out of the patch, the record not being there to open.
        let out: Value = serde_json::from_str(&links.call("editRevert", r#"{"tag": "NPC_", "id": "Fargoth"}"#).unwrap()).unwrap();
        assert_eq!(out["dropped"], json!(true));
        let pending: Value = serde_json::from_str(&links.call("editPending", "{}").unwrap()).unwrap();
        assert_eq!(pending, json!([]), "the pool is empty");
        // A placed object's changes and a new object, when what they name is gone: taken out
        // (or already out), not refused.
        {
            let mut ed = links.editor.lock().unwrap_or_else(|p| p.into_inner());
            ed.queue.add_ref_edit(crate::queue::RefEdit { cell: "Gone Cell".into(), origin: "Gone.esp".into(), refr_index: 7, changes: Default::default(), plugins: vec!["Gone.esp".into()] });
        }
        let out: Value = serde_json::from_str(&links.call("editRefRevert", r#"{"cell": "gone cell", "origin": "Gone.esp", "refr": 7}"#).unwrap()).unwrap();
        assert_eq!(out["dropped"], json!(true));
        assert_eq!(links.call("editNewRemove", r#"{"cell": "Gone Cell", "uid": "nope"}"#).unwrap(), "ok");
        assert!(links.call("editNewRemove", r#"{"cell": "Gone Cell"}"#).is_err(), "a bad request is still refused");
        // What the file has now is found.
        let v: Value = serde_json::from_str(&links.call("editRecord", r#"{"tag": "NPC_", "id": "caius"}"#).unwrap()).unwrap();
        assert_eq!(v["id"], "caius");
        // A record nobody ever had is still the plain refusal.
        let e = links.call("editRecord", r#"{"tag": "NPC_", "id": "nobody"}"#).unwrap_err();
        assert!(e.ends_with("is not a record any plugin of this load order defines"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_id_defined_as_two_object_types_is_said_in_verify_and_the_use_report() {
        let dir = std::env::temp_dir().join(format!("wg-twins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mw = dir.join("Morrowind.esm");
        let header = json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esm", "author": "", "description": "", "num_objects": 0, "masters": []});
        let mut rock = crate::blank::blank("Static").unwrap();
        rock["id"] = json!("Fargoth");
        rock["mesh"] = json!("x.nif");
        std::fs::write(&mw, crate::build::patch_bytes(&[header, npc("fargoth", 2), npc("caius", 3), rock]).unwrap()).unwrap();
        let links = Links::open(vec![("Morrowind.esm".into(), mw)], None);
        links.call("editSet", r#"{"tag": "NPC_", "id": "fargoth", "path": "data.level", "value": 9}"#).unwrap();
        links.call("editSet", r#"{"tag": "NPC_", "id": "caius", "path": "data.level", "value": 9}"#).unwrap();
        let out: Value = serde_json::from_str(&links.call("editVerify", "{}").unwrap()).unwrap();
        let said: Vec<String> = out["findings"].as_array().unwrap().iter().filter(|f| f["message"].as_str().unwrap_or("").contains("Matching IDs")).map(|f| format!("{} {}", f["id"].as_str().unwrap(), f["message"].as_str().unwrap())).collect();
        assert_eq!(said.len(), 1, "{said:#?}");
        assert!(said[0].starts_with("fargoth fargoth is also a Static (Morrowind.esm)"), "{}", said[0]);
        let uses: Value = serde_json::from_str(&links.call("editUses", r#"{"tag": "NPC_", "id": "fargoth"}"#).unwrap()).unwrap();
        assert_eq!(uses["twins"], json!([{"tag": "STAT", "plugin": "Morrowind.esm"}]));
        let uses: Value = serde_json::from_str(&links.call("editUses", r#"{"tag": "NPC_", "id": "caius"}"#).unwrap()).unwrap();
        assert!(uses.get("twins").is_none(), "only when there are any");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
