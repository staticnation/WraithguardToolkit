// Cell viewer mode:
//
//   wraithguard-viewer --cell-viewer --openmw-cfg <setup.cfg> [--prefs <viewer profile .toml>]
//                                    [--cell x,y | int:<name>] [--title <t>] [--theme <json>]
//                                    [--extra <json file>] [--mesh-view]
//   --mesh-view: the mesh viewer - the meshes in the extra file's `meshes`
//                ([{path, label}]), on the same setup, instead of a cell.
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

/// `--cell x,y` for an exterior, `--cell int:<name>` for an interior.
fn parse_cell(spec: &str) -> Option<Value> {
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
    json!({"cfg": cfg, "cell": cell, "theme": theme, "extra": extra, "meshView": mesh_view})
}

/// `args` are everything after `--cell-viewer`. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    // Where the viewer profile lives (commands::viewer_profile_*). Set before any
    // thread exists.
    if let Some(p) = flag(args, "--prefs") {
        std::env::set_var("WRAITHGUARD_VIEWER_PREFS", p);
    }
    let init = format!("window.__WG_VIEW__ = {};", view_request(args));
    let title = flag(args, "--title").unwrap_or("Cell Preview").to_string();

    let result = commands::builder()
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "cellviewer", WebviewUrl::App("index.html".into()))
                .title(&title)
                .inner_size(1600.0, 1000.0)
                // The page draws its own title bar (drag region, min/max/close) when
                // `__TAURI__.window` is there, as it is here.
                .decorations(false)
                .initialization_script(&init)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!());

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("wraithguard-viewer: fatal error: {e}");
            1
        }
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
