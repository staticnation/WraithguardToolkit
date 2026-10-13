// Cell viewer mode:
//
//   wraithguard-viewer --cell-viewer --openmw-cfg <setup.cfg> [--prefs <viewer profile .toml>]
//                                    [--cell x,y | int:<name>] [--title <t>] [--theme <json>]
//                                    [--extra <json file>] [--mesh-view]
//   --mesh-view: the mesh viewer - the meshes in the extra file's `meshes`
//                ([{path, label}]), on the same setup, instead of a cell.
//   --editor:    open in the Editor mode (50_wg_editor.js): the Construction Set's
//                Object Window and Cell View beside the render window, edits going to
//                Wraithguard's patch pool through the extra file's `links`.
//
// Everything is in this binary: the engine (`viewcore` -- VFS/BSA, load order, the
// ESP/ESM world, NIF meshes, DDS textures, terrain, sky), the command layer the page
// calls (`commands.rs`), and the page itself (ui/, assembled by build.rs into the
// embedded frontend). The page reaches the commands through Tauri's own IPC
// (`withGlobalTauri`), and its file dialogs through tauri-plugin-dialog.
//
// Provenance: the engine, the commands and the page come from Gardenfell by Robin
// Hjelte (MIT). The page also carries shader ports from MGE XE (GPL-2.0), so ui/ is
// licensed GPL-2.0 as a silo; everything else in this crate is MIT. See ui/LICENSE.

mod commands;

use serde_json::{json, Value};
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// `--cell x,y` for an exterior, `--cell int:<name>` for an interior, and (Wraithguard's
/// conflict viewer) `--cell find:<tag>:<id>` for wherever a record can be seen, and
/// `--cell plugin:<name>` for a plugin's cells on the map, reviewed.
fn parse_cell(spec: &str) -> Option<Value> {
    if let Some(name) = spec.strip_prefix("plugin:") {
        return Some(json!({"kind": "plugin", "name": name}));
    }
    if let Some(rest) = spec.strip_prefix("find:") {
        let (tag, id) = rest.split_once(':')?;
        return Some(json!({"kind": "find", "tag": tag, "id": id}));
    }
    if let Some(name) = spec.strip_prefix("int:") {
        return Some(json!({"kind": "int", "name": name, "label": name}));
    }
    let (x, y) = spec.split_once(',')?;
    let (x, y): (i32, i32) = (x.trim().parse().ok()?, y.trim().parse().ok()?);
    Some(json!({"kind": "ext", "x": x, "y": y, "name": "", "label": format!("{x}, {y}")}))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// What the page's viewer_only.html script reads to connect and open a cell.
fn view_request(args: &[String]) -> Value {
    let cfg = flag(args, "--openmw-cfg").filter(|p| std::path::Path::new(p).is_file());
    let cell = flag(args, "--cell").and_then(parse_cell);
    // Wraithguard's current chrome palette (its `DARK` dict), so the viewer wears the
    // theme the rest of the toolkit is wearing. Ignored when it does not parse.
    let theme = flag(args, "--theme").and_then(|t| serde_json::from_str::<Value>(t).ok()).filter(Value::is_object);
    // Anything else Wraithguard hands over that is too long for a command line (its
    // subset, for the cell map's "your mods"), as a JSON file.
    let extra = flag(args, "--extra")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object);
    // The mesh viewer: the meshes in `extra.meshes`, instead of a cell.
    let mesh_view = args.iter().any(|a| a == "--mesh-view");
    // The Editor mode, straight away.
    let editor = args.iter().any(|a| a == "--editor");
    // Run from Steam (a Deck's Game Mode, or a non-Steam-game shortcut): Steam sets
    // SteamGameId for what it launches. The page leaves a pad's cursor mode off there - the
    // trackpad is the cursor (49_wg_gamepad.js).
    let steam = std::env::var_os("SteamGameId").is_some_and(|v| !v.is_empty());
    json!({"cfg": cfg, "cell": cell, "theme": theme, "extra": extra, "meshView": mesh_view, "editor": editor, "steam": steam})
}

/// `args` are everything after `--cell-viewer`; `context` is the binary's one
/// `tauri::generate_context!()` (see `main`). Returns the process exit code.
pub fn run(args: &[String], context: tauri::Context<tauri::Wry>) -> i32 {
    viewcore::init();
    // Where the viewer profile lives (commands::viewer_profile_*). Set before any
    // thread exists.
    if let Some(p) = flag(args, "--prefs") {
        std::env::set_var("WRAITHGUARD_VIEWER_PREFS", p);
    }
    let init = format!("window.__WG_VIEW__ = {};", view_request(args));
    let title = flag(args, "--title").unwrap_or("Cell Preview").to_string();

    let result = commands::builder()
        .setup(move |app| {
            let win = WebviewWindowBuilder::new(app, "cellviewer", WebviewUrl::App("index.html".into()))
                .title(&title)
                .inner_size(1600.0, 1000.0)
                // The page draws its own title bar (drag region, min/max/close) when
                // `__TAURI__.window` is there, as it is here.
                .decorations(false)
                .initialization_script(&init)
                // Built hidden, put back where it was, then shown: no jump on screen.
                .visible(false)
                .build()?;
            restore_window(&win);
            let _ = win.show();
            Ok(())
        })
        .run(context);

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("wraithguard-viewer: fatal error: {e}");
            1
        }
    }
}

/// Puts the window back where it was (`commands::window_box`, written by the page through
/// `window_set`), before it is on screen. From Gardenfell's shell (Robin Hjelte, MIT;
/// rounds 18el and 18fi), on Wraithguard's own file.
///
/// A remembered rectangle is used only when it is *reachable*: one that overlaps none of
/// the monitors there now (a second screen unplugged) is dropped for the default, since this
/// window has no system title bar to drag it back by. Overlap rather than containment: a
/// window hanging half off a screen is somewhere a person may have left it.
fn restore_window(win: &tauri::WebviewWindow) {
    let Some(b) = commands::window_box() else { return };
    let (x0, y0) = (b.x as i64, b.y as i64);
    let (x1, y1) = (x0 + b.w as i64, y0 + b.h as i64);
    let seen = win.available_monitors().unwrap_or_default();
    let reachable = seen.is_empty()
        || seen.iter().any(|m| {
            let (p, s) = (m.position(), m.size());
            let (mx0, my0) = (p.x as i64, p.y as i64);
            let (mx1, my1) = (mx0 + s.width as i64, my0 + s.height as i64);
            x0 < mx1 && x1 > mx0 && y0 < my1 && y1 > my0
        });
    if !reachable {
        return;
    }
    /* Gardenfell 18fi: the page writes the *outer* size (the window on the desk, frame and
       all) and `set_size` sets the inner one, so the frame - on Windows a frameless window
       still has an invisible resize border - is taken off, or every start grows the window
       by it (Robin, under PowerToys FancyZones). */
    let (dw, dh) = match (win.outer_size(), win.inner_size()) {
        (Ok(o), Ok(i)) => (o.width.saturating_sub(i.width), o.height.saturating_sub(i.height)),
        _ => (0, 0),
    };
    let _ = win.set_size(tauri::PhysicalSize::new(b.w.saturating_sub(dw).max(1), b.h.saturating_sub(dh).max(1)));
    let _ = win.set_position(tauri::PhysicalPosition::new(b.x, b.y));
    if b.max {
        let _ = win.maximize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_specs() {
        assert_eq!(parse_cell("-3, 7").unwrap()["y"], 7);
        assert_eq!(parse_cell("int:Balmora, Guild").unwrap()["kind"], "int");
        assert!(parse_cell("x").is_none());
        assert_eq!(parse_cell("plugin:My Mod.esp").unwrap()["name"], "My Mod.esp");
    }

    #[test]
    fn a_missing_cfg_is_not_passed_to_the_page() {
        let args: Vec<String> = ["--openmw-cfg", "/no/such/openmw.cfg", "--cell", "1,2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let v = view_request(&args);
        assert!(v["cfg"].is_null());
        assert_eq!(v["cell"]["x"], 1);
        assert!(v["theme"].is_null());
    }

    #[test]
    fn the_theme_is_passed_when_it_parses() {
        let args: Vec<String> = ["--theme", r##"{"accent":"#3794ff"}"##].iter().map(|s| s.to_string()).collect();
        assert_eq!(view_request(&args)["theme"]["accent"], "#3794ff");
        let bad: Vec<String> = ["--theme", "not json"].iter().map(|s| s.to_string()).collect();
        assert!(view_request(&bad)["theme"].is_null());
    }
}
