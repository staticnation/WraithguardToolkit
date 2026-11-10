// The Help page's commands and the program's own path.
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- the Help page (Wraithguard's `viz/docs.py`; port step 4.4) -------------------------

   The program's documents (`help_docs`), one rendered (`help_doc`, `toolscore::docs`), and a
   web link in one opened in the browser (`help_open_link`). `dirs` is where Wraithguard
   says the documents are (`extra.docsDir`); the program's own folder and those above it
   follow. */

pub(super) fn help_dirs(dirs: &[String]) -> Vec<PathBuf> {
    let mut out = toolscore::docs::search_dirs(dirs, std::env::current_exe().ok().as_deref());
    // An AppImage: the documents it carries (Tauri puts resources in usr/lib/<product>),
    // then any beside the .AppImage file.
    if let Some(app) = std::env::var_os("APPDIR") {
        out.push(PathBuf::from(app).join("usr").join("lib").join("wraithguard-toolkit"));
    }
    if let Some(dir) = std::env::var_os("APPIMAGE").map(PathBuf::from).and_then(|p| p.parent().map(std::path::Path::to_path_buf)) {
        out.push(dir);
    }
    out
}

/// Wraithguard: the program to start again (a new Cell Preview, a restart): the .AppImage
/// file when it runs as one - its own binary lives in a mount that ends with it - else the
/// executable itself.
pub(super) fn self_exe() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()) {
        return Ok(p);
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Wraithguard: the documents the Help offers (`HELP_DOCUMENTS`), `[{name, title, found}]`.
#[tauri::command(async)]
pub(super) fn help_docs(dirs: Vec<String>) -> Result<String, String> {
    let search = help_dirs(&dirs);
    let out: Vec<serde_json::Value> = toolscore::docs::DOCUMENTS
        .iter()
        .map(|(name, title)| serde_json::json!({"name": name, "title": title, "found": toolscore::docs::locate(name, &search).is_some()}))
        .collect();
    Ok(serde_json::Value::Array(out).to_string())
}

/// Wraithguard: one document rendered (`_open_document`, `render_markdown`): `name` a `.md`
/// file name, no folders. Answers `{name, title, path, body, headings: [[level, text, id]]}`.
#[tauri::command(async)]
pub(super) fn help_doc(name: String, dirs: Vec<String>) -> Result<String, String> {
    if name.contains(['/', '\\']) || name.starts_with('.') || !name.to_lowercase().ends_with(".md") {
        return Err(format!("{name} is not one of the program's documents"));
    }
    let path = toolscore::docs::locate(&name, &help_dirs(&dirs))
        .ok_or_else(|| format!("{name} was not found next to the program. It ships alongside the app; in a checkout it is in the project folder."))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{name} could not be read: {e}"))?;
    let r = toolscore::docs::render(&String::from_utf8_lossy(&bytes));
    let title = toolscore::docs::DOCUMENTS.iter().find(|(n, _)| *n == name).map(|(_, t)| t.to_string()).unwrap_or_else(|| name.clone());
    Ok(serde_json::json!({
        "name": name, "title": title, "path": path.display().to_string(), "body": r.body,
        "headings": r.headings.iter().map(|(l, t, i)| serde_json::json!([l, t, i])).collect::<Vec<_>>(),
    })
    .to_string())
}

/// Wraithguard: a document's web link, in the system's browser (http, https and mailto only).
#[tauri::command(async)]
pub(super) fn help_open_link(url: String) -> Result<(), String> {
    let low = url.to_lowercase();
    if !(low.starts_with("http://") || low.starts_with("https://") || low.starts_with("mailto:")) {
        return Err(format!("{url} is not a web link"));
    }
    if std::env::var_os("GARDENFELL_NO_OPEN").is_some() {
        return Ok(());
    }
    viewcore::profiles::show_folder(std::path::Path::new(&url))
}
