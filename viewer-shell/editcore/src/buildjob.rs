//! The viewer's Build patch and Test in OpenMW links (`wraithguard/patch/buildjob.py`,
//! and `gui/editorlink.py`'s `editBuildInfo`, `editBuildCheck`, `editBuild`,
//! `editBuildStatus`, `editTestRun`, `editTestSetups` and `editPreviewPatch`): where a
//! patch may go, whether a build may start, the build run on a thread of its own while
//! the page asks how it goes, and the game started on a test build. Port step 4.1,
//! slice 6b; the cases are `tests/fixtures/buildjob_cases.json`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::build::{run_patch_job, written_note, PatchResult, DEFAULT_NAME};
use crate::edit::Editor;
use crate::pyval::{eq, kind, Kind};
use crate::testrun::{self, clean_setup};

/// What a patch may be called.
pub const PATCH_SUFFIXES: [&str; 3] = [".esp", ".esm", ".omwaddon"];
/// How a build treats a file already there.
pub const MODES: [&str; 3] = ["new", "append", "replace"];

/// A refusal: the reason the page shows.
pub type Res<T> = Result<T, String>;

/// A path as Python's `Path` spells it: repeated separators and `.` parts gone, no
/// trailing separator.
fn normal(raw: &str) -> PathBuf {
    Path::new(raw).components().collect()
}

/// Where a request says a patch goes (`{path, ...}`), checked: a plugin's name, a full
/// path, in a folder that is there.
pub fn patch_target(req: &Value) -> Res<PathBuf> {
    let raw = req.get("path").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let Some(raw) = raw else { return Err("Choose where the patch goes".into()) };
    let target = normal(raw);
    if !target.is_absolute() {
        return Err("The patch needs a full path, folder and all".into());
    }
    if !PATCH_SUFFIXES.contains(&testrun::suffix_of(&target).as_str()) {
        return Err("A patch is a plugin: name it .esp (or .omwaddon)".into());
    }
    let parent = target.parent().unwrap_or(Path::new(""));
    if !parent.is_dir() {
        return Err(format!("{} is not a folder", parent.display()));
    }
    Ok(target)
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// `editBuildCheck`'s answer: `{path, exists, inOrder}` (`inOrder`: a plugin of the
/// load order has its name - a patch written before, most likely, or a mod).
pub fn build_check(target: &Path, order: &[String]) -> Value {
    let name = name_of(target).to_lowercase();
    json!({"path": target.to_string_lossy(), "exists": target.exists(), "inOrder": order.iter().any(|n| n.to_lowercase() == name)})
}

/// A build's state, as `editBuildStatus` tells it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildState {
    pub job: i64,
    /// `running`, `done` or `failed`.
    pub state: String,
    pub lines: Vec<String>,
    pub result: Value,
    pub error: Value,
}

impl BuildState {
    /// A build's state as it starts: the next job number, nothing said yet.
    pub fn next(running: Option<&BuildState>) -> BuildState {
        BuildState { job: running.map(|r| r.job).unwrap_or(0) + 1, state: "running".into(), lines: Vec::new(), result: Value::Null, error: Value::Null }
    }

    /// As the case files and the Python side write it.
    pub fn to_json(&self) -> Value {
        json!({"job": self.job, "state": self.state, "lines": self.lines, "result": self.result, "error": self.error})
    }

    /// From that.
    pub fn from_json(v: &Value) -> BuildState {
        BuildState {
            job: v["job"].as_i64().unwrap_or(0),
            state: v["state"].as_str().unwrap_or("").to_string(),
            lines: v["lines"].as_array().into_iter().flatten().map(|l| l.as_str().unwrap_or("").to_string()).collect(),
            result: v["result"].clone(),
            error: v["error"].clone(),
        }
    }
}

/// Refuses a second build while one runs.
pub fn refuse_running(running: Option<&BuildState>) -> Res<()> {
    match running {
        Some(r) if r.state == "running" => Err("A patch is being written already".into()),
        _ => Ok(()),
    }
}

/// How `editBuild` writes (`{mode?}`: `new`, the default, `append` or `replace`), or
/// why it will not start.
pub fn build_mode(req: &Value, target: &Path, running: Option<&BuildState>) -> Res<String> {
    let mode = match req.get("mode") {
        Some(m) if crate::session::truthy(m) => m.clone(),
        _ => json!("new"),
    };
    let Some(mode) = mode.as_str().filter(|m| MODES.contains(m)) else { return Err("bad mode".into()) };
    if mode == "new" && target.exists() {
        return Err(format!("{} exists: choose Append or Replace", name_of(target)));
    }
    refuse_running(running)?;
    Ok(mode.to_string())
}

/// `editBuildStatus`'s answer to `{job, from}`: `{job, state, lines, total, result,
/// error}`, `lines` from `from` on. Refused for a job that is not the current one.
pub fn build_status(state: Option<&BuildState>, req: &Value) -> Res<Value> {
    let Some(state) = state.filter(|s| eq(req.get("job").unwrap_or(&Value::Null), &json!(s.job))) else {
        return Err("No such patch build".into());
    };
    // Python's `isinstance(start, int)`: a bool is one.
    let start = match req.get("from") {
        Some(Value::Bool(true)) => 1,
        Some(v) if kind(v) == Kind::Int => v.as_i64().filter(|n| *n >= 0).unwrap_or(0) as usize,
        _ => 0,
    };
    Ok(json!({
        "job": state.job,
        "state": state.state,
        "lines": state.lines.get(start..).unwrap_or(&[]),
        "total": state.lines.len(),
        "result": state.result,
        "error": state.error,
    }))
}

/// `editTestSetups`: the launch setups replaced (`setups`) and the one used chosen
/// (`use`), in `settings`; whether there is anything to keep.
pub fn update_setups(settings: &mut Value, req: &Value) -> Res<bool> {
    if let Some(raw) = req.get("setups") {
        let Value::Object(raw) = raw else { return Err("setups is an object".into()) };
        let mut kept = serde_json::Map::new();
        for (k, v) in raw {
            let k = k.trim();
            if k.is_empty() {
                continue;
            }
            let clean = clean_setup(v)?;
            kept.insert(k.to_string(), clean);
        }
        settings["setups"] = Value::Object(kept);
    }
    let has = |name: &str, settings: &Value| settings["setups"].get(name).is_some();
    if let Some(u) = req.get("use") {
        let u = u.as_str().unwrap_or("");
        settings["use"] = json!(if has(u, settings) { u } else { "" });
    } else if !has(settings["use"].as_str().unwrap_or(""), settings) {
        settings["use"] = json!("");
    }
    Ok(req.get("setups").is_some() || req.get("use").is_some())
}

/// What `editTestRun` starts the game with (`{cell?, exe?, setup?}`): the OpenMW found
/// (`exe`), the cell to start in (`None`: the game's own start) and the launch setup (the
/// one asked for, else the one last chosen). Refused, `needExe: ...`, with no OpenMW,
/// or for a setup that is not kept.
pub fn test_request(req: &Value, exe_given: Option<&str>, exe: Option<PathBuf>, settings: &Value) -> Res<(PathBuf, Option<String>, Option<Value>)> {
    let Some(exe) = exe else {
        return Err(match exe_given.filter(|g| !g.is_empty()) {
            Some(g) => format!("needExe: {g} is not OpenMW"),
            None => "needExe: Where is OpenMW? Choose openmw.exe (or the folder it is in)".into(),
        });
    };
    let cell = req.get("cell").and_then(Value::as_str).map(str::to_string);
    let name = match req.get("setup") {
        Some(Value::String(s)) => s.clone(),
        _ => settings["use"].as_str().unwrap_or("").to_string(),
    };
    let setup = if name.is_empty() { None } else { settings["setups"].get(&name).cloned() };
    if !name.is_empty() && setup.is_none() {
        return Err(format!("No launch setup is called {name}"));
    }
    Ok((exe, cell, setup))
}

/// `editPreviewPatch`'s plugin: one [`patch_target`] allows that is written.
pub fn preview_target(req: &Value) -> Res<PathBuf> {
    let target = patch_target(req)?;
    if !target.is_file() {
        return Err(format!("{} is not written yet", name_of(&target)));
    }
    Ok(target)
}

/// `editBuildInfo`'s answer: what the Build patch panel starts from - the queue's
/// summary, the patch last written, where to suggest (that, else the default name in the
/// first data folder; `folders` the setup's, the highest priority first) and the load
/// order.
pub fn build_info(ed: &Editor, folders: &[String]) -> Value {
    let last = ed.last_patch.as_ref().map(|p| p.to_string_lossy().to_string());
    let suggested = match (&last, folders.first()) {
        (Some(l), _) => l.clone(),
        (None, Some(f)) => Path::new(f).join(DEFAULT_NAME).to_string_lossy().to_string(),
        (None, None) => String::new(),
    };
    json!({"summary": ed.patch_summary(), "last": last, "suggested": suggested, "folders": folders, "defaultName": DEFAULT_NAME, "order": ed.session.order()})
}

/// What a written patch tells the page.
pub fn written_answer(result: &PatchResult) -> Value {
    json!({
        "output": result.output.as_ref().map(|p| p.to_string_lossy().to_string()),
        "records": result.records,
        "carried": result.carried,
        "masters": result.masters,
        "remapped": result.remapped,
        "note": written_note(result),
    })
}

/// The builds the viewer runs, one at a time, each on a thread of its own: the last
/// one's state, for `editBuildStatus`.
#[derive(Clone, Default)]
pub struct Builds {
    state: Arc<Mutex<Option<BuildState>>>,
}

impl Builds {
    pub fn new() -> Builds {
        Builds::default()
    }

    /// The last build's state.
    pub fn current(&self) -> Option<BuildState> {
        self.state.lock().unwrap().clone()
    }

    /// `editBuildStatus`.
    pub fn status(&self, req: &Value) -> Res<Value> {
        build_status(self.state.lock().unwrap().as_ref(), req)
    }

    /// A new build's state put in place (refused while one runs): its job number.
    fn begin(&self) -> Res<i64> {
        let mut st = self.state.lock().unwrap();
        refuse_running(st.as_ref())?;
        let next = BuildState::next(st.as_ref());
        let job = next.job;
        *st = Some(next);
        Ok(job)
    }

    fn with(&self, f: impl FnOnce(&mut BuildState)) {
        if let Some(st) = self.state.lock().unwrap().as_mut() {
            f(st);
        }
    }

    fn say(&self, line: String) {
        self.with(|st| st.lines.push(line));
    }

    fn failed(&self, error: String) {
        eprintln!("editor: the viewer's patch build failed: {error}");
        self.with(|st| {
            st.error = json!(error);
            st.state = "failed".into();
        });
    }

    /// `editBuild` (`{path, mode?}`): the queue written as a patch on a thread of its
    /// own - gathered under the Editor's lock (refused there, with the reason), the
    /// plugins read and the patch written outside it, and the queue spent under it once
    /// written. `{job}`, for [`Builds::status`].
    pub fn build(&self, editor: &Arc<Mutex<Editor>>, req: &Value) -> Res<Value> {
        let target = patch_target(req)?;
        let mode = build_mode(req, &target, self.current().as_ref())?;
        let job = editor.lock().unwrap().prepare_patch(&target, mode == "append").map_err(|e| e.0)?;
        let number = self.begin()?;
        let (me, editor) = (self.clone(), editor.clone());
        std::thread::Builder::new()
            .name("wg-patch-build".into())
            .spawn(move || {
                let said = me.clone();
                match run_patch_job(&job, false, &mut |l| said.say(l.to_string())) {
                    Ok(result) => {
                        editor.lock().unwrap().finish_patch(&job, &result);
                        let answer = written_answer(&result);
                        me.with(|st| {
                            st.result = answer;
                            st.state = "done".into();
                        });
                    }
                    Err(e) => me.failed(e.0),
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(json!({"job": number}))
    }

    /// `editTestRun` (`{cell?, exe?, setup?}`): the queue written to
    /// [`testrun::TEST_PLUGIN`] in `scratch` (kept: nothing is spent) and OpenMW started
    /// on it past its menu in the cell; what it prints comes back as the job's lines, and
    /// the job is done when the game is closed. `settings`: the file the OpenMW chosen
    /// and the launch setups are kept in.
    pub fn test_run(&self, editor: &Arc<Mutex<Editor>>, req: &Value, settings: &Path, scratch: &Path) -> Res<Value> {
        let exe_given = req.get("exe").and_then(Value::as_str);
        let found = testrun::find_openmw(exe_given, settings);
        let (exe, cell, setup) = test_request(req, exe_given, found, &testrun::load_settings(settings))?;
        refuse_running(self.current().as_ref())?;
        let target = scratch.join(testrun::TEST_PLUGIN);
        let _ = std::fs::remove_file(&target);
        let job = editor.lock().unwrap().prepare_patch(&target, false).map_err(|e| e.0)?;
        let number = self.begin()?;
        let me = self.clone();
        let scratch = scratch.to_path_buf();
        std::thread::Builder::new()
            .name("wg-test-run".into())
            .spawn(move || {
                let said = me.clone();
                let run = || -> Res<()> {
                    let result = run_patch_job(&job, false, &mut |l| said.say(l.to_string())).map_err(|e| e.0)?;
                    let (mut argv, script) = testrun::launch_args(&exe, &target, cell.as_deref(), setup.as_ref());
                    if let Some(script) = script {
                        let start = scratch.join("start.txt");
                        std::fs::write(&start, script).map_err(|e| e.to_string())?;
                        argv.extend(["--script-run".into(), start.to_string_lossy().to_string()]);
                    }
                    me.say(testrun::shown(&argv));
                    let cwd = exe.parent().map(Path::to_path_buf).unwrap_or_default();
                    let note = written_note(&result);
                    let output = result.output.as_ref().map(|p| p.to_string_lossy().to_string());
                    let code = testrun::run_game(
                        &argv,
                        &cwd,
                        &mut |pid| me.with(|st| st.result = json!({"output": output, "records": result.records, "pid": pid, "note": note})),
                        &mut |line| me.say(line),
                    )
                    .map_err(|e| e.to_string())?;
                    me.with(|st| {
                        st.result["exit"] = json!(code);
                        st.state = "done".into();
                    });
                    Ok(())
                };
                if let Err(e) = run() {
                    me.failed(e);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(json!({"job": number}))
    }
}

/// `editTestSetups` against the settings kept in `file`: `{setups, use}` as kept.
pub fn test_setups(file: &Path, req: &Value) -> Res<Value> {
    let mut settings = testrun::load_settings(file);
    if update_setups(&mut settings, req)? {
        testrun::save_settings(file, &settings);
    }
    Ok(testrun::setups_answer(&settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::{PatchQueue, Selection};

    fn npc(id: &str, level: i64) -> Value {
        let mut r = crate::blank::blank("Npc").unwrap();
        r["id"] = json!(id);
        r["data"]["level"] = json!(level);
        r
    }

    fn header(masters: &[&str]) -> Value {
        let m: Vec<Value> = masters.iter().map(|m| json!([m, 100])).collect();
        json!({"type": "Header", "flags": "", "version": 1.3, "file_type": "Esp", "author": "", "description": "", "num_objects": 0, "masters": m})
    }

    /// A load order of two plugins in `dir`, an Editor over it with one record queued.
    fn editor(dir: &Path) -> Arc<Mutex<Editor>> {
        let (mw, md) = (dir.join("Morrowind.esm"), dir.join("Mod.esp"));
        std::fs::write(&mw, crate::build::patch_bytes(&[header(&[]), npc("fargoth", 2)]).unwrap()).unwrap();
        std::fs::write(&md, crate::build::patch_bytes(&[header(&["Morrowind.esm"]), npc("fargoth", 10)]).unwrap()).unwrap();
        let session = crate::session::Session::new(vec![("Morrowind.esm".into(), mw), ("Mod.esp".into(), md)]);
        let mut ed = Editor::new(session, PatchQueue::new(), None);
        ed.queue.add_whole(Selection { plugin: "Mod.esp".into(), record_type: "Npc".into(), key: "fargoth".into() });
        Arc::new(Mutex::new(ed))
    }

    fn finished(builds: &Builds, job: &Value) -> Value {
        for _ in 0..400 {
            let st = builds.status(&json!({"job": job["job"], "from": 0})).unwrap();
            if st["state"] != "running" {
                return st;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("the build did not finish");
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wg-buildjob-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_build_runs_on_its_own_and_spends_the_queue() {
        let dir = folder("build");
        let ed = editor(&dir);
        let builds = Builds::new();
        let target = dir.join("Patch.esp");
        let job = builds.build(&ed, &json!({"path": target.to_string_lossy()})).unwrap();
        let st = finished(&builds, &job);
        assert_eq!(st["state"], "done", "{st}");
        assert_eq!((st["result"]["records"].clone(), st["result"]["output"].clone()), (json!(1), json!(target.to_string_lossy())));
        assert!(st["lines"].as_array().unwrap().last().unwrap().as_str().unwrap().starts_with("wrote "));
        assert!(ed.lock().unwrap().queue.is_empty());
        let info = build_info(&ed.lock().unwrap(), &["/data".into()]);
        assert_eq!((info["last"].clone(), info["suggested"].clone()), (json!(target.to_string_lossy()), json!(target.to_string_lossy())));
        // Written: new is refused, and with the queue spent nothing is to be written.
        assert!(builds.build(&ed, &json!({"path": target.to_string_lossy()})).unwrap_err().ends_with("exists: choose Append or Replace"));
        assert_eq!(builds.build(&ed, &json!({"path": target.to_string_lossy(), "mode": "append"})).unwrap_err(), "Nothing is queued: there is no patch to write.");
        assert_eq!(builds.status(&json!({"job": 9})).unwrap_err(), "No such patch build");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_test_run_writes_the_plugin_keeps_the_queue_and_starts_the_game() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("test");
        let ed = editor(&dir);
        // The "game": prints what it was given, and how it would start, and fails.
        let game = dir.join("openmw");
        std::fs::write(&game, "#!/bin/sh\necho \"args: $*\"\necho oops >&2\nexit 2\n").unwrap();
        std::fs::set_permissions(&game, std::fs::Permissions::from_mode(0o755)).unwrap();
        let settings = dir.join(testrun::SETTINGS_FILE);
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let builds = Builds::new();
        let job = builds.test_run(&ed, &json!({"cell": "(1, 2)", "exe": game.to_string_lossy()}), &settings, &scratch).unwrap();
        let st = finished(&builds, &job);
        assert_eq!(st["state"], "done", "{st}");
        assert!(scratch.join(testrun::TEST_PLUGIN).is_file());
        assert_eq!(std::fs::read_to_string(scratch.join("start.txt")).unwrap(), "coe 1 2\n");
        let lines: Vec<&str> = st["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
        assert!(lines.iter().any(|l| l.starts_with("$ ") && l.contains("--script-run")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("args: --data")) && lines.contains(&"oops"), "{lines:?}");
        assert_eq!(st["result"]["exit"], json!(2));
        assert!(st["result"]["pid"].as_u64().unwrap() > 0);
        assert_eq!(ed.lock().unwrap().queue.len(), 1, "the queue is kept");
        assert_eq!(testrun::load_settings(&settings)["exe"], json!(game.to_string_lossy()), "the OpenMW chosen is kept");
        let missing = builds.test_run(&ed, &json!({"exe": dir.join("nope").to_string_lossy()}), &settings, &scratch).unwrap_err();
        assert!(missing.starts_with("needExe: ") && missing.ends_with("is not OpenMW"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn setups_are_kept_in_their_file() {
        let dir = folder("setups");
        let file = dir.join(testrun::SETTINGS_FILE);
        assert_eq!(test_setups(&file, &json!({})).unwrap(), json!({"setups": {}, "use": ""}));
        let got = test_setups(&file, &json!({"setups": {"Mine": {"content": ["A.esp"], "script": "tgm"}}, "use": "Mine"})).unwrap();
        assert_eq!(got, json!({"setups": {"Mine": {"content": ["A.esp"], "script": "tgm"}}, "use": "Mine"}));
        testrun::remember_exe(&file, Path::new("/g/openmw"));
        assert_eq!(testrun::load_settings(&file)["use"], json!("Mine"), "the setups survive the exe being kept");
        assert_eq!(test_setups(&file, &json!({"setups": {}})).unwrap()["use"], json!(""));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
