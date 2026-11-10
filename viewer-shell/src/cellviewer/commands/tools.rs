// The Tools page's commands: lint, backups, masters, the save check, the journal and tes3cmd.
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- the Tools page (Wraithguard; port step 4.4) ----------------------------------------

   Wraithguard's tools that work on the whole load order, answered by `toolscore`, over the
   load order the launch's openmw.cfg names: the lint (`tools_lint`), the backups
   (`tools_backups`, `_restore`, `_delete`), Remove / Rename Master (`tools_masters`,
   `tools_remove_master`, `tools_rename_master`), Save Check (`tools_save_check`), the Journal view
   (`tools_journal`), tes3cmd (`tools_tes3cmd`, `_find`, `tools_locate`). */

/// Wraithguard: the load-order lint (`lint_plugins`): `subset` the user's own plugins (their
/// header and script checks, and the `[your mod]` tag). Answers `{warnings, stats: {scanned,
/// unreadable, warnings, interiorCells, pathgrids}}`.
#[tauri::command(async)]
pub(super) fn tools_lint(cfg: String, subset: Vec<String>) -> Result<String, String> {
    let _gate = scan_gate();
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let at: std::collections::HashMap<String, PathBuf> = setup.locate(&order).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let find = move |name: &str| at.get(name).cloned();
    let origins: std::collections::BTreeMap<String, String> = subset.iter().map(|s| (s.to_lowercase(), "your mod".to_string())).collect();
    let threads = scan_budget();
    let (mut warnings, st) = toolscore::lint::lint_order(&order, &find, &subset, &origins, threads);
    // A plugin's .omwscripts twin is OpenMW's: on an MO2 or Morrowind.ini install the game's
    // engine runs no Lua, so a twin left out is nothing to say.
    if !viewcore::ordercore::install::runs_openmw(&cfg) {
        warnings.retain(|w| !w.starts_with("[TWIN]"));
    }
    Ok(serde_json::json!({
        "warnings": warnings,
        "stats": {"scanned": st.scanned, "unreadable": st.unreadable, "warnings": st.warnings, "interiorCells": st.interior_cells, "pathgrids": st.pathgrids},
    })
    .to_string())
}

/// Wraithguard: the backups the tools leave (`scan_backups`, `toolscore::backups`) in the
/// cfg's data folders, the cfg's own folder and `dirs` (the rule files' and plugin-order.yml's
/// folders), four folders deep. Answers `[{path, original, kind, originalExists}]`.
#[tauri::command(async)]
pub(super) fn tools_backups(cfg: String, dirs: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let mut roots: Vec<PathBuf> = setup.data.clone();
    roots.extend(dirs.into_iter().filter(|d| !d.is_empty()).map(PathBuf::from));
    if let Some(parent) = std::path::Path::new(&cfg).parent() {
        roots.push(parent.to_path_buf());
    }
    let found = toolscore::backups::scan(&roots, 4);
    Ok(serde_json::Value::from(
        found
            .iter()
            .map(|b| serde_json::json!({"path": b.path, "original": b.original, "kind": b.kind, "originalExists": b.original.exists()}))
            .collect::<Vec<_>>(),
    )
    .to_string())
}

/// Wraithguard: backups copied over their originals (`[[backup, original]]`). Answers `{restored,
/// failed: [[path, why]]}`.
#[tauri::command(async)]
pub(super) fn tools_backups_restore(pairs: Vec<(String, String)>) -> Result<String, String> {
    let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|(a, b)| (PathBuf::from(a), PathBuf::from(b))).collect();
    let (ok, failed) = toolscore::backups::restore(&pairs);
    Ok(serde_json::json!({"restored": ok, "failed": failed.iter().map(|(p, e)| serde_json::json!([p, e])).collect::<Vec<_>>()}).to_string())
}

/// Wraithguard: backups deleted. Answers how many were.
#[tauri::command(async)]
pub(super) fn tools_backups_delete(paths: Vec<String>) -> Result<usize, String> {
    Ok(toolscore::backups::delete(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>()))
}

/// Wraithguard: a plugin's masters, `[[name, size]]` (Remove / Rename Master's list).
#[tauri::command(async)]
pub(super) fn tools_masters(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let list = toolscore::masters::masters(&bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(serde_json::json!(list).to_string())
}

/// Wraithguard: a master removed from a plugin, its references renumbered
/// (`remove_master_from_bytes`), the original kept once as `.premaster.bak`; `dryRun` reports
/// without writing. Answers `{master, removed, dropped, droppedByCell, remapped, remaining,
/// backup}`.
#[tauri::command(async)]
pub(super) fn tools_remove_master(path: String, master: String, dry_run: bool) -> Result<String, String> {
    let (r, backup) = toolscore::masters::remove_master_file(std::path::Path::new(&path), &master, dry_run).map_err(|e| format!("{path}: {e}"))?;
    Ok(serde_json::json!({"master": r.master, "removed": r.removed, "dropped": r.references_dropped, "droppedByCell": r.dropped_by_cell,
                          "remapped": r.references_remapped, "remaining": r.remaining_masters, "backup": backup})
    .to_string())
}

/// Wraithguard: Merge to master (`toolscore::mergemaster`, greatness7's merge_to_master).
/// `mode` `"plugin"`: `plugin` folded into `master`; `"order"`: `plugins` (in load order)
/// into one master. The result goes to `out` (a file there kept once as `.premerge.bak`); the
/// sources are never written. Answers `{out, bytes, backup}`.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub(super) fn tools_merge_master(mode: String, plugin: String, master: String, plugins: Vec<String>, out: String, remove_deleted: bool, apply_moved: bool, keep_duplicates: bool) -> Result<String, String> {
    use toolscore::mergemaster::{merge_into_master, merge_order, write_merged, MergeOptions};
    let _gate = scan_gate();
    let o = MergeOptions { remove_deleted, apply_moved_references: apply_moved, preserve_duplicate_references: keep_duplicates };
    let out_path = std::path::Path::new(&out);
    for src in plugins.iter().chain([&plugin, &master]) {
        if !src.is_empty() && std::path::Path::new(src) == out_path {
            return Err(format!("{out} is one of the files being merged: write the merge somewhere else"));
        }
    }
    let bytes = if mode == "order" {
        merge_order(&plugins.iter().map(PathBuf::from).collect::<Vec<_>>(), o)?
    } else {
        merge_into_master(std::path::Path::new(&plugin), std::path::Path::new(&master), o)?
    };
    let backup = write_merged(out_path, &bytes).map_err(|e| format!("{out}: {e}"))?;
    Ok(serde_json::json!({"out": out, "bytes": bytes.len(), "backup": backup}).to_string())
}

/// Wraithguard: a plugin's master `old` pointed at the file `newPath` (its name and size), every
/// reference kept (`rename_master_in_bytes`), the original kept once as `.premaster.bak`.
/// Answers `{renamed, remaining, backup}`.
#[tauri::command(async)]
pub(super) fn tools_rename_master(path: String, old: String, new_path: String) -> Result<String, String> {
    let (r, backup) = toolscore::masters::rename_master_file(std::path::Path::new(&path), &old, std::path::Path::new(&new_path)).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"renamed": r.renamed, "remaining": r.remaining_masters, "backup": backup}).to_string())
}

/// Wraithguard: Save Check (`toolscore::save`): each save's content files against the load
/// order the cfg names, and `openmw-save-stats`' report - one save, or two compared (older
/// first). `details`: the largest mods detailed; `mods`: those mods instead; `all`: the tables
/// of the whole save too; `dump`: the saved data of the scripts whose path holds these. Answers
/// `{saves: [{name, files, missing, error}], report}`.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub(super) fn tools_save_check(cfg: String, saves: Vec<String>, details: usize, mods: Vec<String>, all: bool, dump: Vec<String>) -> Result<String, String> {
    use toolscore::save;
    if saves.is_empty() || saves.len() > 2 {
        return Err("one save, or two to compare".into());
    }
    let setup = cf_setup(&cfg)?;
    // Every content= line, not only the plugins: a save lists the .omwscripts it ran with
    // too, and OpenMW loads builtin.omwscripts from its own resources without a line.
    // (Static, 2026-10-27: measured against plugins() a save "missed" 89 .omwscripts the
    // cfg loads.)
    let mut order = setup.content.clone();
    order.push("builtin.omwscripts".into());
    let dirs = save::DataDirs::from_cfg(std::path::Path::new(&cfg)).ok();
    let mut checks = Vec::new();
    let mut stats = Vec::new();
    for path in &saves {
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
        let data = std::fs::read(path).map_err(|e| format!("can't read save: {e}"))?;
        match save::content_files(&data) {
            Ok(files) => {
                let missing = save::missing_from(&files, &order);
                checks.push(serde_json::json!({"name": name, "files": files, "missing": missing, "error": null}));
            }
            Err(e) => checks.push(serde_json::json!({"name": name, "files": [], "missing": [], "error": e})),
        }
        stats.push(save::analyse(&name, data, dirs.as_ref(), 4));
    }
    let report = if stats.len() == 2 {
        save::report_diff(&stats[0], &stats[1], 30, &mods)
    } else {
        let opt = save::ReportOptions { details, patterns: mods, full: all, dump, ..Default::default() };
        save::report(&stats[0], &opt)
    };
    Ok(serde_json::json!({"saves": checks, "report": report}).to_string())
}

/// Wraithguard: the Journal view (`editcore::journal`): every quest of the load order the cfg
/// names, its stages by index, and the calls that set each (where, under what conditions, what
/// else they do). Only the topics, responses and scripts are read, the plugins side by side.
#[tauri::command(async)]
pub(super) fn tools_journal(cfg: String) -> Result<String, String> {
    let _gate = scan_gate();
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let found: Vec<(String, PathBuf)> = setup.locate(&order).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let keep: [[u8; 4]; 3] = [*b"DIAL", *b"INFO", *b"SCPT"];
    let read = |path: &PathBuf| -> Vec<serde_json::Value> {
        let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
        editcore::plugin::objects(&bytes, Some(&keep), false)
            .map(|objs| objs.iter().filter_map(|o| serde_json::to_value(o).ok()).collect())
            .unwrap_or_default()
    };
    let threads = scan_budget();
    let chunk = found.len().div_ceil(threads).max(1);
    let mut sources: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
    std::thread::scope(|sc| {
        let handles: Vec<_> = found.chunks(chunk).map(|part| sc.spawn(move || part.iter().map(|(n, p)| (n.clone(), read(p))).collect::<Vec<_>>())).collect();
        for h in handles {
            if let Ok(part) = h.join() {
                sources.extend(part);
            }
        }
    });
    Ok(editcore::journal::view(&sources, &order).to_string())
}

/// Wraithguard: tes3cmd's file (`find_tes3cmd`): `explicit` when it is one, else `$MLOX_TES3CMD`,
/// `PATH`, the cfg's folder. Answers its path, or `""`.
#[tauri::command(async)]
pub(super) fn tools_tes3cmd_find(cfg: String, explicit: String) -> String {
    let dirs: Vec<PathBuf> = std::path::Path::new(&cfg).parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    serde_json::Value::String(toolscore::tes3cmd::find(Some(&explicit), &dirs).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()).to_string()
}

/// Wraithguard: plugins of the load order the cfg names, located (`[[name, path]]`; the path
/// `null` when no data folder has it) - the tes3cmd list's "Add your mods".
#[tauri::command(async)]
pub(super) fn tools_locate(cfg: String, names: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    Ok(serde_json::json!(setup.locate(&names)).to_string())
}

/// Wraithguard: tes3cmd over `files` (`toolscore::tes3cmd`): `cmd` `clean`, `header`, `custom`
/// (`extra` the whole command) or `sync` (master sizes, done here). `clean` and `custom` run on
/// a copy staged with the plugin's masters and copy back what changed, a backup kept. Answers
/// `{lines, status}`.
#[tauri::command(async)]
pub(super) fn tools_tes3cmd(cfg: String, exe: String, cmd: String, extra: Vec<String>, files: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    let mut dirs = setup.data.clone();
    dirs.extend(files.iter().filter_map(|f| f.parent().map(std::path::Path::to_path_buf)));
    let index = viewcore::ordercore::inputs::PluginIndex::build(&dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>());
    let find = |n: &str| index.find(n).map(std::path::Path::to_path_buf);
    if cmd == "sync" {
        let mut lines = vec!["=".repeat(70), " MASTER-SIZE RESYNC (in-app, VFS-aware)".to_string(), "=".repeat(70)];
        let (mut ok, mut fixed, mut fail) = (0, 0, 0);
        for f in &files {
            let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match toolscore::tes3cmd::sync_master_sizes(f, &find, true) {
                Err(e) => {
                    fail += 1;
                    lines.push(format!("  {name}: ERROR: {e}"));
                }
                Ok((updated, unresolved)) => {
                    ok += 1;
                    if updated.is_empty() {
                        lines.push(format!("  {name}: already in sync"));
                    } else {
                        fixed += 1;
                        lines.extend(updated.iter().map(|(m, a, b)| format!("  {name}: '{m}' {a} -> {b}")));
                    }
                    lines.extend(unresolved.iter().map(|m| format!("  {name}: WARNING: master '{m}' not found in any data folder -- its size field left untouched")));
                }
            }
        }
        let status = format!("Resync: {fixed} plugin(s) updated, {} already in sync, {fail} error(s).", ok - fixed);
        return Ok(serde_json::json!({"lines": lines, "status": status}).to_string());
    }
    let exe_path = PathBuf::from(&exe);
    if !exe_path.is_file() {
        return Err(if exe.is_empty() { "tes3cmd not found. Browse to the compiled tes3cmd.exe (MOMW Tools Pack).".into() } else { format!("'{exe}' does not exist.") });
    }
    if cmd == "custom" && extra.is_empty() {
        return Err("Type a tes3cmd command first, e.g. 'dump --type CELL' or 'fixcells'.".into());
    }
    let argv = toolscore::tes3cmd::invocation(&exe_path)?;
    let staging = std::env::temp_dir().join("wraithguard_tes3cmd_staging");
    let r = toolscore::tes3cmd::run(&argv, &cmd, &extra, &files, &find, &staging, &setup.plugins());
    Ok(serde_json::json!({"lines": r.lines, "status": r.status}).to_string())
}
