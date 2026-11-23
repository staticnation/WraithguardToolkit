//! Security F5 (the second audit, 2026-11-23): the file dialogs, opened from here.
//!
//! The page used to open them itself (`window.__TAURI__.dialog`) and hand the answer back
//! as an argument, so the program could not tell a path the person picked from one the page
//! made up. Opened here, the answer is recorded (`commands::granted`) before the page sees
//! it, and the writers take only what was recorded. The page's dialog permission is down to
//! ask / confirm / message (capabilities/cellviewer.json): it cannot open these itself.
//!
//! Not in `commands.rs`: check-commands compiles that file against a stand-in for Tauri
//! with no dialogs in it. These two are only ever the desktop's.

use super::commands::granted;
use std::path::{Path, PathBuf};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// What the page asks for (`Engine.pick`'s options, as the JS dialog API had them).
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Opts {
    default_path: Option<String>,
    directory: bool,
    multiple: bool,
    title: Option<String>,
    filters: Vec<Filter>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Filter {
    name: String,
    extensions: Vec<String>,
}

/// A file or folder dialog (`kind` `save` or `open`; `opts` as above, JSON). Answers the
/// path, a list of paths (`multiple`), or null when the person backed out - each one
/// recorded as theirs to write first.
#[tauri::command(async)]
pub(crate) fn path_pick(kind: String, opts: String, window: tauri::Window) -> Result<serde_json::Value, String> {
    let o: Opts = serde_json::from_str(&opts).unwrap_or_default();
    let mut d = window.dialog().file().set_parent(&window);
    if let Some(t) = o.title.as_deref().filter(|t| !t.is_empty()) {
        d = d.set_title(t);
    }
    for f in &o.filters {
        let ext: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
        d = d.add_filter(&f.name, &ext);
    }
    if let Some(p) = o.default_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        let p = Path::new(p);
        // A folder starts the dialog there; a file's name is offered, in its folder if it has one.
        if p.is_dir() {
            d = d.set_directory(p);
        } else {
            if let Some(dir) = p.parent().filter(|d| d.is_dir()) {
                d = d.set_directory(dir);
            }
            if let Some(n) = p.file_name() {
                d = d.set_file_name(n.to_string_lossy());
            }
        }
    }
    let to_path = |f: tauri_plugin_dialog::FilePath| f.into_path().ok();
    let one = |p: PathBuf, folder: bool| {
        if folder {
            granted::folder(&p)
        } else {
            granted::file(&p)
        }
        serde_json::Value::from(p.to_string_lossy().into_owned())
    };
    Ok(if kind == "save" {
        d.blocking_save_file().and_then(to_path).map(|p| one(p, false)).unwrap_or(serde_json::Value::Null)
    } else if o.directory && o.multiple {
        d.blocking_pick_folders().map(|v| v.into_iter().filter_map(to_path).map(|p| one(p, true)).collect()).unwrap_or(serde_json::Value::Null)
    } else if o.directory {
        d.blocking_pick_folder().and_then(to_path).map(|p| one(p, true)).unwrap_or(serde_json::Value::Null)
    } else if o.multiple {
        d.blocking_pick_files().map(|v| v.into_iter().filter_map(to_path).map(|p| one(p, false)).collect()).unwrap_or(serde_json::Value::Null)
    } else {
        d.blocking_pick_file().and_then(to_path).map(|p| one(p, false)).unwrap_or(serde_json::Value::Null)
    })
}

/// A path the page names that no dialog gave (a typed output folder, one kept from an
/// earlier run): the program's own message box, saying the path in full, and recorded as
/// the person's when they allow it. Answers whether they did.
#[tauri::command(async)]
pub(crate) fn path_allow(path: String, run: Option<bool>, window: tauri::Window) -> Result<bool, String> {
    let p = path.trim();
    if p.is_empty() {
        return Ok(false);
    }
    let pb = PathBuf::from(p);
    let folder = pb.is_dir();
    let run = run.unwrap_or(false) && !folder;
    let (what, title) = if run {
        ("run the program", "Wraithguard: allow this program?")
    } else if folder {
        ("write into the folder", "Wraithguard: allow this write?")
    } else {
        ("write the file", "Wraithguard: allow this write?")
    };
    let yes = window
        .dialog()
        .message(format!("Wraithguard is about to {what}\n\n{p}\n\nAllow it? Choose No if you did not just ask for this."))
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::YesNo)
        .parent(&window)
        .blocking_show();
    if yes {
        if folder {
            granted::folder(&pb)
        } else {
            granted::file(&pb)
        }
    }
    Ok(yes)
}
