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
    "editDuplicateCell",
    "editPathgrid",
    "editPathgridSet",
    "editSearch",
    "editSearchAll",
    "editReplaceAll",
    "editColumns",
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
/// Build page in place of the Patch Builder). Their Python handlers go with the Python
/// Editor (DROP-TK.md).
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
        let editor = Editor::new(session, queue, journal.clone());
        Links { editor: Arc::new(Mutex::new(editor)), seen: Mutex::new(seen), plugins, journal, ..Links::blank() }
    }

    /// Links over an Editor made elsewhere (the tests' sessions).
    pub fn with_editor(editor: Editor) -> Links {
        let seen = editor.journal.as_deref().and_then(stamp);
        let journal = editor.journal.clone();
        Links { editor: Arc::new(Mutex::new(editor)), seen: Mutex::new(seen), journal, ..Links::blank() }
    }

    fn blank() -> Links {
        let session = Session::new(Vec::new());
        Links {
            editor: Arc::new(Mutex::new(Editor::new(session, PatchQueue::new(), None))),
            seen: Mutex::new(None),
            plugins: Vec::new(),
            journal: None,
            builds: crate::buildjob::Builds::new(),
            folders: Vec::new(),
            test_settings: None,
            scratch: std::env::temp_dir().join("wraithguard-test"),
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
        let mut ed = self.editor.lock().unwrap_or_else(|p| p.into_inner());
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
        out.map(|a| a.text())
    }
}

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
    ed.find(&tag, &rid, plugins.as_deref()).ok_or_else(|| format!("{rid} is not a record any plugin of this load order defines"))
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
            let found = ref_request(ed, &req)?;
            let path = req.get("path").and_then(Value::as_str).filter(|p| !p.is_empty());
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
            let new = new_request(ed, &body_of(body)?)?;
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
            let height = match req.get("height") {
                None => 0.0,
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
            Json(ed.columns(&tag.trim().to_uppercase()))
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
            Json(edit(ed.dialogue_play(
                speaker,
                &text_or_empty(req.get("topic")),
                num(req.get("choice"), 0),
                &text_or_empty(req.get("cell")),
                num(req.get("disposition"), 50),
                strict,
            ))?)
        }
        "editWhoCanSay" => {
            let req = body_of(body)?;
            Json(edit(ed.who_can_say(&text_or_empty(req.get("id")), WHO_CAN_SAY_LIMIT))?)
        }
        "editVerify" => Json(ed.verify(ctx.file_exists)),
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
        let out: Value = serde_json::from_str(&links.call_with("editVerify", "{}", &Ctx { file_exists: Some(&have) }).unwrap()).unwrap();
        assert!(out["findings"].is_array());
        // The build links: where the last patch went comes with the launch.
        let links = links.with_setup(vec!["/data".into()], Some(dir.join("Old.esp")), None);
        let info: Value = serde_json::from_str(&links.call("editBuildInfo", "{}").unwrap()).unwrap();
        assert_eq!((info["last"].clone(), info["folders"].clone()), (json!(dir.join("Old.esp").to_string_lossy()), json!(["/data"])));
        assert!(links.call("editTestSetups", "{}").unwrap_err().starts_with("Test in OpenMW needs Wraithguard"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
