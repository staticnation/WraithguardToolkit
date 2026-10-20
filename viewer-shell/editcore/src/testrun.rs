//! Test in OpenMW: the patch pool written to a scratch plugin and the game started on it
//! (`wraithguard/patch/testrun.py`). Port step 4.1, slice 6b; the cases are
//! `tests/fixtures/buildjob_cases.json`.
//!
//! The pool is written as a plugin of its own in a scratch folder (nothing is spent), and
//! OpenMW is started with the player's own configuration and that folder and plugin added
//! last, past the main menu, in the cell the Editor shows. Only OpenMW's documented
//! command line is used: `--data` and `--content` add to what `openmw.cfg` gives,
//! `--skip-menu` goes straight into a new game, `--start <cell>` names the cell, and
//! `--script-run <file>` runs console commands once the game is up (`coe x y` for an
//! exterior known only by its grid).

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Map, Value};

use crate::pyval::str_of;
use crate::session::truthy;

/// The scratch plugin's name: never one a load order has.
pub const TEST_PLUGIN: &str = "Wraithguard Test.esp";
/// What a content file of a launch setup may be.
pub const PLUGIN_SUFFIXES: [&str; 4] = [".esp", ".esm", ".omwaddon", ".omwgame"];
/// Where the OpenMW chosen and the launch setups are kept, in the app's folder.
pub const SETTINGS_FILE: &str = "openmw_test.json";

static GRID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(?\s*(-?\d+)\s*,\s*(-?\d+)\s*\)?$").expect("a pattern"));
static GRID_AT_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\((-?\d+)\s*,\s*(-?\d+)\)\s*$").expect("a pattern"));

/// The folder the test plugin is written to (made when missing).
pub fn scratch_dir() -> PathBuf {
    let path = std::env::temp_dir().join("wraithguard-test");
    let _ = std::fs::create_dir_all(&path);
    path
}

/// What is kept for testing, from `file`: `{exe?, setups: {name: setup}, use}` (anything
/// else the file holds kept too). A file that is not there or does not read: none.
pub fn load_settings(file: &Path) -> Value {
    let got: Option<Value> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok());
    let Some(Value::Object(mut got)) = got else { return json!({"setups": {}, "use": ""}) };
    let setups = match got.get("setups") {
        Some(Value::Object(s)) => Value::Object(s.clone()),
        _ => json!({}),
    };
    let wanted = got.get("use").and_then(Value::as_str).unwrap_or("").to_string();
    let used = if setups.get(&wanted).is_some() { wanted } else { String::new() };
    got.insert("setups".into(), setups);
    got.insert("use".into(), json!(used));
    Value::Object(got)
}

/// The settings kept in `file` (quietly not, in a read-only app folder: asked again
/// next time).
pub fn save_settings(file: &Path, settings: &Value) {
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b" "));
    if serde::Serialize::serialize(settings, &mut ser).is_ok() {
        let _ = std::fs::write(file, out);
    }
}

/// A launch setup as given, checked: `{content: [plugin names], script}`, the content
/// files' names trimmed and the empty ones left out.
pub fn clean_setup(raw: &Value) -> Result<Value, String> {
    let Value::Object(raw) = raw else { return Err("a launch setup is an object".into()) };
    // Python iterates what it is given: a list's items, a text's letters, a dict's keys.
    let items: Vec<String> = match raw.get("content") {
        Some(v) if !truthy(v) => Vec::new(),
        None => Vec::new(),
        Some(Value::Array(a)) => a.iter().map(str_of).collect(),
        Some(Value::String(s)) => s.chars().map(String::from).collect(),
        Some(Value::Object(m)) => m.keys().cloned().collect(),
        Some(other) => return Err(format!("'{}' object is not iterable", crate::pyval::type_name(other))),
    };
    let content: Vec<String> = items.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
    for name in &content {
        let p = Path::new(name);
        let own = p.file_name().map(|n| n.to_string_lossy() == *name).unwrap_or(false);
        let suffix = suffix_of(p);
        if !own || !PLUGIN_SUFFIXES.contains(&suffix.as_str()) {
            return Err(format!("{name} is not a plugin's name (a .esp, .esm or .omwaddon)"));
        }
    }
    let script = raw.get("script").and_then(Value::as_str).unwrap_or("");
    Ok(json!({"content": content, "script": script}))
}

/// A path's suffix as Python's `PurePath.suffix` gives it, lower-cased: `""` for none (a
/// name starting with its only dot has none, nor does one ending in a dot).
pub fn suffix_of(p: &Path) -> String {
    match p.extension() {
        Some(e) if !e.is_empty() => format!(".{}", e.to_string_lossy().to_lowercase()),
        _ => String::new(),
    }
}

/// The OpenMW executable chosen before (`settings`'s `exe`), when it is still there.
pub fn remembered_exe(file: &Path) -> Option<PathBuf> {
    let raw = load_settings(file).get("exe").and_then(Value::as_str).filter(|s| !s.is_empty()).map(PathBuf::from)?;
    raw.is_file().then_some(raw)
}

/// The OpenMW executable chosen, kept for the next test.
pub fn remember_exe(file: &Path, exe: &Path) {
    let mut settings = load_settings(file);
    settings["exe"] = json!(exe.to_string_lossy());
    save_settings(file, &settings);
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "openmw.exe" } else { "openmw" }
}

/// `openmw` on the PATH, as Python's `shutil.which` finds it (on Windows, with each of
/// `PATHEXT`'s endings).
fn on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let ends: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into()).split(';').filter(|e| !e.is_empty()).map(str::to_string).collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for end in &ends {
            let found = dir.join(format!("openmw{end}"));
            if found.is_file() {
                return Some(found);
            }
        }
    }
    None
}

/// The OpenMW executable to start: `given` (the executable, or the folder holding it;
/// made the remembered one), else the remembered one, else `openmw` on the PATH, else the
/// usual install folders. `None` when none is found.
pub fn find_openmw(given: Option<&str>, settings: &Path) -> Option<PathBuf> {
    if let Some(given) = given.filter(|g| !g.is_empty()) {
        let mut path = PathBuf::from(given.trim().trim_matches('"'));
        if path.is_dir() {
            path = path.join(exe_name());
        }
        if path.is_file() {
            remember_exe(settings, &path);
            return Some(path);
        }
        return None;
    }
    if let Some(found) = remembered_exe(settings) {
        return Some(found);
    }
    if let Some(found) = on_path() {
        return Some(found);
    }
    let mut places: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["PROGRAMFILES", "PROGRAMFILES(X86)"] {
            let Some(root) = std::env::var_os(var) else { continue };
            let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("OpenMW"))
                .map(|e| e.path().join("openmw.exe"))
                .filter(|p| p.exists())
                .collect();
            found.sort();
            found.reverse();
            places.extend(found);
        }
    } else if cfg!(target_os = "macos") {
        places.push("/Applications/OpenMW.app/Contents/MacOS/openmw".into());
    } else {
        places.extend(["/usr/games/openmw".into(), "/usr/local/bin/openmw".into()]);
    }
    places.into_iter().find(|p| p.is_file())
}

/// A grid number as Python's `int()` reads it (leading zeros and a sign).
fn int_text(s: &str) -> String {
    match s.parse::<i128>() {
        Ok(n) => n.to_string(),
        Err(_) => s.to_string(),
    }
}

/// OpenMW's command line for a test (`argv`) and the console commands it runs at start
/// (the text of a `--script-run` file, or `None`). `cell`: an interior's name, `(x, y)`
/// for an exterior (or a name with it, `Balmora (-3, -2)`), or `None` for the game's own
/// start; a launch setup's content files load before the test plugin, and its script
/// runs after the move to the cell.
pub fn launch_args(exe: &Path, plugin: &Path, cell: Option<&str>, setup: Option<&Value>) -> (Vec<String>, Option<String>) {
    let parent = plugin.parent().map(|p| p.to_string_lossy().to_string()).filter(|p| !p.is_empty()).unwrap_or_else(|| ".".into());
    let mut argv = vec![exe.to_string_lossy().to_string(), "--data".into(), parent];
    for name in setup.and_then(|s| s.get("content")).and_then(Value::as_array).into_iter().flatten() {
        argv.extend(["--content".into(), str_of(name)]);
    }
    let own_name = plugin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    argv.extend(["--content".into(), own_name, "--skip-menu".into()]);
    let mut script: Option<String> = None;
    let name = cell.unwrap_or("").trim();
    let grid = GRID.captures(name).or_else(|| GRID_AT_END.captures(name));
    if let Some(g) = grid {
        script = Some(format!("coe {} {}\n", int_text(&g[1]), int_text(&g[2])));
    } else if !name.is_empty() {
        argv.extend(["--start".into(), name.to_string()]);
    }
    let own = setup.and_then(|s| s.get("script")).filter(|v| truthy(v)).map(str_of).unwrap_or_default();
    let own = own.trim();
    if !own.is_empty() {
        script = Some(format!("{}{own}\n", script.unwrap_or_default()));
    }
    (argv, script)
}

/// The command line as the report shows it (`$ ` and each word, quoted when it has a
/// space).
pub fn shown(argv: &[String]) -> String {
    let words: Vec<String> = argv.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect();
    format!("$ {}", words.join(" "))
}

/// OpenMW started on `argv` in its own folder, each line it prints (as text, a bad byte
/// replaced) given to `say`: its process id at the start (`started`), and how it ended.
pub fn run_game(argv: &[String], cwd: &Path, started: &mut dyn FnMut(u32), say: &mut dyn FnMut(String)) -> std::io::Result<i32> {
    let mut child = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .spawn()?;
    started(child.id());
    // The game's two streams as one, as Python's `stderr=STDOUT` reads them.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    let streams: Vec<Box<dyn std::io::Read + Send>> =
        [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
            .into_iter()
            .flatten()
            .collect();
    for stream in streams {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut r = std::io::BufReader::new(stream);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match r.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf);
                        let line = line.strip_suffix('\n').unwrap_or(&line);
                        let line = line.strip_suffix('\r').unwrap_or(line);
                        if tx.send(line.to_string()).is_err() {
                            break;
                        }
                    }
                }
            }
        }));
    }
    drop(tx);
    for line in rx {
        say(line);
    }
    for r in readers {
        let _ = r.join();
    }
    let status = child.wait()?;
    Ok(status.code().unwrap_or(-1))
}

/// The settings' launch setups, as `editTestSetups` answers: `{setups, use}`.
pub fn setups_answer(settings: &Value) -> Value {
    let mut out = Map::new();
    out.insert("setups".into(), settings.get("setups").cloned().unwrap_or_else(|| json!({})));
    out.insert("use".into(), settings.get("use").cloned().unwrap_or_else(|| json!("")));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openmw_given_as_its_folder_is_found_and_remembered() {
        let dir = std::env::temp_dir().join(format!("wg-testrun-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join(SETTINGS_FILE);
        let exe = dir.join(exe_name());
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(find_openmw(Some(&dir.to_string_lossy()), &settings), Some(exe.clone()));
        assert_eq!(load_settings(&settings)["exe"], json!(exe.to_string_lossy()));
        assert_eq!(find_openmw(None, &settings), Some(exe.clone()));
        assert_eq!(find_openmw(Some(&dir.join("nope").to_string_lossy()), &settings), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_games_lines_are_heard_and_its_end() {
        let argv: Vec<String> = ["sh", "-c", "echo one; echo two >&2; printf 'three\\r\\n'; exit 3"].iter().map(|s| s.to_string()).collect();
        let mut lines = Vec::new();
        let mut pid = 0;
        let code = run_game(&argv, Path::new("/"), &mut |p| pid = p, &mut |l| lines.push(l)).unwrap();
        lines.sort();
        assert_eq!((code, lines, pid > 0), (3, vec!["one".to_string(), "three".into(), "two".into()], true));
    }
}
