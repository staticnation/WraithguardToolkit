// Headless cell-viewer engine for tests: the command bodies over JSON lines on stdin/stdout.
// From Gardenfell's gdn_cli serve.rs ((c) 2026 Robin Hjelte, MIT).
// The shell's commands, over a pipe.
//
// The app reaches the engine through Tauri. Everything else that wants to drive it —
// the browser test suite, above all — could not, which is why the page grew a second
// implementation of half the engine in the first place, and why the tests that
// exercised it were testing the copy rather than the thing that ships.
//
// This runs the *same command bodies* the shell runs: `wraithguard_viewer_check_commands` is the crate
// that compiles `src-tauri/src/main.rs` against a stub, so calling into it is calling
// the shipping code. Not a mock, not a parallel implementation — one JSON object per
// line in, one out, and the functions in between are the ones the desktop app calls.
//
// ```text
// {"id":1,"cmd":"open_install","args":{"kind":"vanilla","path":"…/Data Files"}}
// {"id":1,"ok":{…}}
// ```
//
// Binary replies (`read_asset`, `mesh_data`) come back as base64 under `b64`, because
// the transport is lines of text and the alternative is a framing protocol nobody
// needs for a mesh.

use viewcore::json::{parse, Val};
use wraithguard_viewer_check_commands::{tauri::State, App};
use std::io::{BufRead, Write};
use std::sync::Mutex;

fn s(a: &Val, k: &str) -> String {
    a.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}
fn i(a: &Val, k: &str) -> i32 {
    a.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as i32
}
fn b(a: &Val, k: &str) -> bool {
    a.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn strs(a: &Val, k: &str) -> Vec<String> {
    a.get(k)
        .and_then(|v| v.as_arr())
        .map(|xs| xs.iter().filter_map(|x| x.as_str()).map(String::from).collect())
        .unwrap_or_default()
}


/// An optional string: absent and empty mean the same thing to every caller here.
fn opt(a: &Val, k: &str) -> Option<String> {
    let v = s(a, k);
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | (*c.get(2).unwrap_or(&0) as u32);
        out.push(B64[(n >> 18 & 63) as usize] as char);
        out.push(B64[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 { B64[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// Runs one command. `Ok(Some(json))` is a JSON reply, `Ok(None)` a base64 one already
/// wrapped by the caller.
fn dispatch(cmd: &str, a: &Val, app: &Mutex<App>) -> Result<Reply, String> {
    use wraithguard_viewer_check_commands as t;
    let st = || State::new(app);
    Ok(match cmd {
        /* Round 18bi: `plugins` is which of the load order to parse — absent means all of
           it, and an empty array really does mean none, the same distinction `opt_ix`
           draws for the export's rules. */
        "open_install" => Reply::Json(t::open_install(
            s(a, "kind"),
            s(a, "path"),
            opt(a, "profile"),
            a.get("plugins").and_then(|v| v.as_arr()).map(|xs| {
                xs.iter().filter_map(|x| x.as_str()).map(String::from).collect::<Vec<String>>()
            }),
            st(),
        )?),
        "cell_coverage" => Reply::Json(t::cell_coverage(st())?),
        "ori" => Reply::Json(t::ori(s(a, "key"), opt(a, "model"), st())?),
        "viewer_profile_read" => Reply::Text(t::viewer_profile_read()?),
        "viewer_profile_write" => Reply::Text(t::viewer_profile_write(s(a, "text"))?),
        "wg_save_edited" => Reply::Text(t::wg_save_edited(s(a, "url"), s(a, "body"), s(a, "out"))?),
        "wg_open_record" => Reply::Text(t::wg_open_record(s(a, "url"), s(a, "body"))?),
        "wg_post" => Reply::Text(t::wg_post(s(a, "url"), s(a, "body"))?),
        "find_record" => Reply::Json(t::find_record(s(a, "tag"), s(a, "id"), st())?),
        "editor_tags" => Reply::Json(t::editor_tags(st())?),
        "editor_records" => Reply::Json(t::editor_records(s(a, "tag"), st())?),
        "editor_cell_refs" => Reply::Json(t::editor_cell_refs(s(a, "cell"), st())?),
        "ori_dialogue" => Reply::Json(t::ori_dialogue(s(a, "id"), st())?),
        "startup_install" => Reply::Json(t::startup_install()?),
        // Round 18cj: the colour theme, remembered for the next start-up.
        "theme_set" => Reply::Json(t::theme_set(s(a, "code"))?),
        "picker_set" => Reply::Json(t::picker_set(s(a, "mode"))?),
        "world_map" => Reply::Json(t::world_map(st())?),
        "world_map_tiles" => Reply::Bytes(t::world_map_tiles(i(a, "from").max(0) as u32, i(a, "count").max(0) as u32, st())?.0),
        "textures" => Reply::Json(t::textures(st())?),
        "scopes" => Reply::Json(t::scopes(st())?),
        "cells" => Reply::Json(t::cells(st())?),
        "texture_usage" => Reply::Json(t::texture_usage(st())?),
        "assets" => Reply::Json(t::assets(st())?),
        "cell_data" => Reply::Json(t::cell_data(i(a, "gx"), i(a, "gy"), opt(a, "review"), Some(b(a, "without")), opt(a, "land"), st())?),
        "interior_data" => Reply::Json(t::interior_data(s(a, "name"), opt(a, "review"), Some(b(a, "without")), st())?),
        // Wraithguard: the review, overlay and link tools.
        "pathgrid" => Reply::Json(t::pathgrid(strs(a, "cells"), st())?),
        "collision_bundle" => Reply::Bytes(t::collision_bundle(strs(a, "meshes"), st())?.0),
        "where_used" => Reply::Json(t::where_used(s(a, "kind"), s(a, "name"), st())?),
        "asset_providers" => Reply::Json(t::asset_providers(s(a, "path"), s(a, "kind"), st())?),
        "game_settings" => Reply::Json(t::game_settings(st())?),
        "write_b64" => {
            t::write_b64(s(a, "path"), s(a, "data"))?;
            Reply::Raw("true".into())
        }
        // Text, not JSON: the caller wants the file's bytes.
        "texture_data" => Reply::Bytes(t::texture_data(s(a, "path"), Some(b(a, "bc")), a.get("maxSize").and_then(|v| v.as_i64()).map(|v| v.max(0) as u32), Some(strs(a, "bcx")), st())?.0),
        "assets_bundle" => Reply::Bytes(t::assets_bundle(strs(a, "meshes"), strs(a, "have"), Some(b(a, "bc")), a.get("maxSize").and_then(|v| v.as_i64()).map(|v| v.max(0) as u32), Some(strs(a, "bcx")), Some(strs(a, "maps")), st())?.0),
        // Spelled by hand here, as every argument in this file is; renaming one is a
        // two-file edit and the symptom of forgetting is the engine insisting a value is
        // missing while the page plainly sent it.
        "atmosphere" => Reply::Json(t::atmosphere(st())?),
        "favourites_list" => Reply::Json(t::favourites_list()?),
        "lang_list" => Reply::Json(t::lang_list()?),
        "lang_get" => Reply::Json(t::lang_get(s(a, "code"))?),
        "lang_create" => Reply::Json(t::lang_create(s(a, "code"), s(a, "name"))?),
        "lang_update" => Reply::Json(t::lang_update(s(a, "code"))?),
        "open_folder" => Reply::Json(t::open_folder(s(a, "what"))?),
        "favourite_set" => {
            t::favourite_set(s(a, "kind"), s(a, "item"), b(a, "on"))?;
            Reply::Raw("true".into())
        }
        "write_text" => {
            t::write_text(s(a, "path"), s(a, "text"))?;
            Reply::Raw("true".into())
        }
        /* Round 18r: there is no pointer to move out here. The command is dispatched all
           the same rather than left out, so the page can call it without asking which
           shell it is in - the browser suites reach the same branch the desktop does and
           get the same "done". */
        "cursor_park" => {
            t::cursor_park(
                a.get("show").and_then(|v| v.as_bool()),
                a.get("x").and_then(|v| v.as_f64()),
                a.get("y").and_then(|v| v.as_f64()),
                t::tauri::Window::default(),
            )?;
            Reply::Raw("true".into())
        }
        "read_asset" => Reply::Bytes(t::read_asset(s(a, "path"), st())?.0),
        "mesh_data" => Reply::Bytes(t::mesh_data(s(a, "path"), st())?.0),
        "mesh_collision" => Reply::Bytes(t::mesh_collision(s(a, "path"), st())?.0),
        "cpu_threads" => Reply::Raw(t::cpu_threads().to_string()),
        other => return Err(format!("no such command: {other}")),
    })
}

enum Reply {
    /// Already JSON; passed through as-is.
    Json(String),
    /// A string the caller wants as a JSON string.
    Text(String),
    /// A number or literal.
    Raw(String),
    Bytes(Vec<u8>),
}

/// How many requests may be in flight at once — round 18bf (I8).
///
/// It used to be one: the loop read a line, ran the command on the main thread, and wrote
/// the reply before reading the next. So nothing the caller sent could ever arrive *while*
/// a command was running, and round 18n's supersede path — a scatter that stops because a
/// newer ask has come in — was not merely untested over the pipe, it was unreachable. The
/// browser suites drive the engine through here, so the mechanism that makes a burst of
/// edits cost one wait rather than one per edit had no coverage from the tests that
/// exercise it.
///
/// Nothing reorders for a caller that awaits each reply before sending the next, which is
/// what the page does for everything but the preview: concurrency here is the *caller's*
/// to use. Replies carry the `id` they answer and `engine.js` matches on it, so they may
/// come back in any order.
const SERVE_MAX: usize = 4;

/// One request, answered. Whatever happens, a line comes back.
fn serve_one(line: &str, app: &Mutex<App>) -> String {
    let (id, reply) = {
        match parse(line) {
            Err(e) => (0.0, Err(format!("unreadable request: {e}"))),
            Ok(req) => {
                let id = req.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let args = req.get("args").cloned().unwrap_or(Val::Obj(vec![]));
                /* A command that panics takes the whole session with it otherwise, and
                   a test suite that dies mid-run tells you far less than one that
                   reports which call went wrong. */
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    dispatch(&cmd, &args, app)
                }));
                (
                    id,
                    match caught {
                        Ok(r) => r,
                        Err(_) => Err(format!("{cmd} panicked")),
                    },
                )
            }
        }
    };

    {
        let mut s = format!("{{\"id\":{id},");
        match reply {
            Ok(Reply::Json(j)) => {
                s.push_str("\"ok\":");
                s.push_str(&j);
            }
            Ok(Reply::Raw(v)) => {
                s.push_str("\"ok\":");
                s.push_str(&v);
            }
            Ok(Reply::Text(t)) => {
                s.push_str("\"ok\":\"");
                viewcore::json::escape_into(&t, &mut s);
                s.push('"');
            }
            Ok(Reply::Bytes(bytes)) => {
                s.push_str("\"b64\":\"");
                s.push_str(&base64(&bytes));
                s.push('"');
            }
            Err(e) => {
                s.push_str("\"err\":\"");
                viewcore::json::escape_into(&e, &mut s);
                s.push('"');
            }
        }
        s.push('}');
        s
    }
}

fn run() -> i32 {
    /* Leaked on purpose. The workers below borrow the state for as long as they live, and
       they live for as long as the session does — a `serve` session that is over is a
       process that is over, so there is nothing for a drop to do. */
    let app: &'static Mutex<App> = Box::leak(Box::new(Mutex::new(App::default())));

    /* One writer, so two replies cannot interleave on a line. It announces readiness
       itself rather than the main thread doing it, which keeps "ready" first in the
       stream by construction instead of by timing. */
    let (done_tx, done_rx) = std::sync::mpsc::channel::<String>();
    let writer = std::thread::spawn(move || {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{{\"ready\":true}}");
        let _ = out.flush();
        for line in done_rx {
            if writeln!(out, "{line}").is_err() || out.flush().is_err() {
                break;
            }
        }
    });

    let (job_tx, job_rx) = std::sync::mpsc::channel::<String>();
    let job_rx = std::sync::Arc::new(Mutex::new(job_rx));
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(2, SERVE_MAX);
    let mut workers = Vec::with_capacity(n);
    for _ in 0..n {
        let job_rx = job_rx.clone();
        let done = done_tx.clone();
        workers.push(std::thread::spawn(move || loop {
            // The guard is dropped before the work, so handing a job out is serialised
            // and doing it is not.
            let job = {
                let Ok(rx) = job_rx.lock() else { break };
                rx.recv()
            };
            let Ok(line) = job else { break };
            if done.send(serve_one(&line, app)).is_err() {
                break;
            }
        }));
    }
    drop(done_tx);

    /* Round 18bf (I7): bytes, not lines.

       `stdin.lock().lines()` yields `Err` for a line that is not UTF-8, and the loop
       ended on it — returning 0, which is exactly what a clean shutdown returns. A test
       harness whose pipe picked up one stray byte got a session that simply stopped
       answering, with no message and a success code. Read as bytes and decoded lossily, a
       bad byte is one request's problem: the JSON fails to parse and the caller gets
       "unreadable request" against the right id. A real I/O error on the pipe is a
       different thing — said on stderr, and answered with a non-zero code. */
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buf: Vec<u8> = Vec::new();
    let mut code = 0;
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) => break, // end of input: the caller is done with us
            Ok(_) => {}
            Err(e) => {
                eprintln!("GardenfellCLI serve: stdin: {e}");
                code = 2;
                break;
            }
        }
        let line = String::from_utf8_lossy(&buf).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if job_tx.send(line).is_err() {
            break; // every worker is gone
        }
    }
    drop(job_tx);
    for w in workers {
        let _ = w.join();
    }
    let _ = writer.join();
    code
}

fn main() { std::process::exit(run()) }

/* Round 18bk — the two registries a command has to be in, checked against each other.
   
   `plugins_probe` shipped in round 18bi defined, dispatched here, and covered by 41 tests,
   and the desktop app still answered "Command plugins_probe not found". It was never added
   to `generate_handler!`, and nothing could have caught that: `wraithguard_viewer_check_commands` rewrites
   `main.rs` by cutting everything from `fn main()` onward, which is precisely where the
   registration lives, and every browser suite talks to `serve` rather than to Tauri. So the
   one list that decides what the shipped program can do was the one list under no test at
   all. A whole round of green gates said nothing.

   These read the two files as text, which is the only way to see a macro list that never
   reaches this crate's compiler. */
#[cfg(test)]
mod registry {
    const MAIN: &str = include_str!("../../../src/cellviewer/commands.rs");
    const SERVE: &str = include_str!("wg-view-serve.rs");

    /// Every `#[tauri::command]` function in the shell, by name.
    fn commands() -> Vec<String> {
        let mut out = Vec::new();
        let mut lines = MAIN.lines().peekable();
        while let Some(l) = lines.next() {
            if !l.trim_start().starts_with("#[tauri::command") {
                continue;
            }
            // The `fn` line, possibly after further attributes.
            for next in lines.by_ref() {
                let t = next.trim_start();
                if t.starts_with('#') {
                    continue;
                }
                let t = t.strip_prefix("pub ").unwrap_or(t);
                if let Some(rest) = t.strip_prefix("fn ") {
                    let name: String =
                        rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    if !name.is_empty() {
                        out.push(name);
                    }
                }
                break;
            }
        }
        out
    }

    /// The names inside `invoke_handler(tauri::generate_handler![ … ])`.
    fn registered() -> Vec<String> {
        let at = MAIN
            .find("invoke_handler(tauri::generate_handler![")
            .expect("no generate_handler! in commands.rs — has the shell been restructured?");
        let rest = &MAIN[at..];
        let end = rest.find("])").expect("unterminated generate_handler!");
        rest[..end]
            .lines()
            .skip(1)
            .filter_map(|l| {
                let t = l.trim().trim_end_matches(',');
                (!t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| t.to_string())
            })
            .collect()
    }

    #[test]
    fn every_command_the_shell_defines_is_one_the_shell_registers() {
        /* The check that would have caught round 18bi's miss before it reached Robin.
           An unregistered command compiles, tests clean through `serve`, and then answers
           "Command <name> not found" the first time somebody uses the program. */
        let reg = registered();
        let missing: Vec<String> = commands().into_iter().filter(|c| !reg.contains(c)).collect();
        assert!(
            missing.is_empty(),
            "these are #[tauri::command] but not in generate_handler!, so the desktop app \
             cannot call them: {missing:?} — add each to the list in `builder()`"
        );
    }

    #[test]
    fn every_command_the_shell_registers_is_one_it_defines() {
        // The other way round: a name left in the list after its function was renamed or
        // removed fails the Tauri build with a macro error that names no useful cause.
        let cmds = commands();
        let stale: Vec<String> = registered().into_iter().filter(|r| !cmds.contains(r)).collect();
        assert!(stale.is_empty(), "named in generate_handler! but no such command: {stale:?}");
    }

    #[test]
    fn every_command_is_reachable_through_serve_as_well() {
        /* Every browser suite talks to `serve`, so a command the dispatch does not reach is
           a command no test can exercise however many are written. Reached either under its
           own name (`"foo" => … t::foo(…)`) or under another — `paint_layer_overrides` is
           the page's name for `paint_layer_set` — so what is checked is that the function
           is called, not what the arm is spelled. */
        let unreached: Vec<String> = commands()
            .into_iter()
            .filter(|c| !SERVE.contains(&format!("t::{c}(")))
            .collect();
        assert!(
            unreached.is_empty(),
            "defined in the shell but never called from serve's dispatch, so no browser \
             test can reach them: {unreached:?}"
        );
    }
}
