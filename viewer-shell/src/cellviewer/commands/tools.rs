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
pub(super) fn tools_lint(cfg: String, subset: Vec<String>) -> Result<tauri::ipc::Response, String> {
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
    /* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed; a line
       per finding across the load order): `stats` `uuuuu` (scanned, unreadable, warnings,
       interior cells, path grids) and `warnings` `s`. Read by 61_wg_tools.js. */
    let mut tb = Tables::new();
    tb.table("stats", "uuuuu", 1);
    tb.u(st.scanned as u64).u(st.unreadable as u64).u(st.warnings as u64).u(st.interior_cells as u64).u(st.pathgrids as u64);
    tb.table("warnings", "s", warnings.len());
    for w in &warnings {
        tb.s(w);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
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
///
/// Wraithguard: as bytes (WGTB, Static 2026-11-18: the results the page lists, filters and undoes
/// in place of tes3cmd's text alone): `head` `ss` (the status, the run's id), `lines` `s` (the
/// log), `files` `su` (each plugin the run changed, how many records) and `changes` `usssus`
/// (its file's row, `removed`/`added`/`changed`, type, id, which of that key, the subrecords
/// that differ). `tools_tes3cmd_undo` puts records back. Read by `61_wg_tools.js` (`t3Read`).
///
/// Wraithguard: the tes3cmd built in (`Engine::Builtin`, tes3cmd-rs) unless `external` is set
/// and a tes3cmd is found (`exe`, else beside the setup), or the command runs Perl code
/// (`needs_perl`: `modify --run` and the like), which goes to an external one when there is
/// one (Static 2026-11-17: built in by default, the external an opt-in fallback).
#[tauri::command(async)]
pub(super) fn tools_tes3cmd(
    cfg: String,
    exe: String,
    cmd: String,
    extra: Vec<String>,
    files: Vec<String>,
    external: Option<bool>,
) -> Result<tauri::ipc::Response, String> {
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
        // Only the headers' master sizes change (kept as `.masterfix.bak`): no records to list.
        let mut tb = Tables::new();
        tb.table("head", "ss", 1);
        tb.s(&status).s("");
        tb.table("lines", "s", lines.len());
        for l in &lines {
            tb.s(l);
        }
        t3_tables(&mut tb, &[]);
        return Ok(tauri::ipc::Response::new(tb.done()));
    }
    if cmd == "custom" && extra.is_empty() {
        return Err("Type a tes3cmd command first, e.g. 'dump --type CELL' or 'clean'.".into());
    }
    let perl = cmd == "custom" && toolscore::tes3cmd::needs_perl(&extra);
    let engine = if external.unwrap_or(false) || perl {
        // The external one: the path given (a wrong one is said, not looked past), else one
        // on PATH or beside the setup.
        if !exe.is_empty() && !std::path::Path::new(&exe).is_file() {
            return Err(format!("'{exe}' does not exist."));
        }
        let dirs: Vec<PathBuf> = std::path::Path::new(&cfg).parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
        match toolscore::tes3cmd::find(Some(&exe), &dirs) {
            Some(p) => toolscore::tes3cmd::Engine::External(toolscore::tes3cmd::invocation(&p)?),
            // Perl code with no external tes3cmd: the built-in one says it needs Perl.
            None if perl => toolscore::tes3cmd::Engine::Builtin,
            None => {
                return Err("No external tes3cmd found. Browse to tes3cmd (the MOMW Tools Pack's tes3cmd.exe, or the Perl script), or untick it to use the built-in one.".into());
            }
        }
    } else {
        toolscore::tes3cmd::Engine::Builtin
    };
    let staging = std::env::temp_dir().join("wraithguard_tes3cmd_staging");
    let r = toolscore::tes3cmd::run(&engine, &cmd, &extra, &files, &find, &staging, &setup.plugins());
    let held: Vec<HeldFile> = r
        .files
        .iter()
        .map(|f| HeldFile { path: f.path.clone(), before: f.before.clone(), expect: std::fs::read(&f.path).map(|b| digest(&b)).unwrap_or(0), changes: f.changes.clone() })
        .collect();
    let mut tb = Tables::new();
    tb.table("head", "ss", 1);
    tb.s(&r.status).s(&r.id);
    tb.table("lines", "s", r.lines.len());
    for l in &r.lines {
        tb.s(l);
    }
    t3_tables(&mut tb, &held);
    let mut runs = T3_RUNS.lock().unwrap_or_else(|p| p.into_inner());
    runs.retain(|(id, _)| std::path::Path::new(&staging).join("results").join(id).is_dir());
    runs.push((r.id, held));
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// A plugin a tes3cmd run changed, held for undo: its file, its copy from before the run, a
/// digest of the file as the run (or the last undo) left it, and what differs from before.
struct HeldFile {
    path: PathBuf,
    before: PathBuf,
    expect: u64,
    changes: Vec<toolscore::recdiff::Change>,
}

/// The tes3cmd runs of this session whose plugins can be undone, by id (their "before" copies
/// are in the staging folder's `results`, the newest `KEEP_RUNS` kept).
static T3_RUNS: Mutex<Vec<(String, Vec<HeldFile>)>> = Mutex::new(Vec::new());

fn digest(b: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

/// `files` `su` and `changes` `usssus` of `held` (see `tools_tes3cmd`).
fn t3_tables(tb: &mut Tables, held: &[HeldFile]) {
    tb.table("files", "su", held.len());
    for f in held {
        tb.s(&f.path.to_string_lossy()).u(f.changes.len() as u64);
    }
    tb.table("changes", "usssus", held.iter().map(|f| f.changes.len()).sum());
    for (i, f) in held.iter().enumerate() {
        for c in &f.changes {
            tb.u(i as u64).s(c.kind).s(&c.key.tag).s(&c.key.id).u(u64::from(c.key.nth)).s(&c.detail);
        }
    }
}

/// Wraithguard: puts records a tes3cmd run changed back (`toolscore::recdiff::undo`): of run
/// `run`'s plugin `file` (its row in `files`), the changes at `rows` (their places in that
/// file's list, as the last answer gave it) - a removed record restored where it was, a changed
/// one as it was, an added one taken out, the header's count made right. Refused when the plugin
/// changed since (another tool, or the game's editor): its file is not the run's any more.
/// Answers the run's `files` and `changes` again (WGTB, as `tools_tes3cmd`), with `head` `s`
/// saying what was done.
#[tauri::command(async)]
pub(super) fn tools_tes3cmd_undo(run: String, file: usize, rows: Vec<usize>) -> Result<tauri::ipc::Response, String> {
    let mut runs = T3_RUNS.lock().unwrap_or_else(|p| p.into_inner());
    let held = runs.iter_mut().find(|(id, _)| *id == run).map(|(_, h)| h).ok_or("That run is no longer kept: undo is for the last ten runs of this session.")?;
    let f = held.get_mut(file).ok_or("no such plugin in that run")?;
    let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let current = std::fs::read(&f.path).map_err(|e| format!("{name}: {e}"))?;
    if digest(&current) != f.expect {
        return Err(format!("{name} has changed since this run (another tool wrote it): undo would lose that, so it is left as it is."));
    }
    let before = std::fs::read(&f.before).map_err(|e| format!("{name}'s copy from before the run: {e}"))?;
    let keys: Vec<toolscore::recdiff::Key> = rows.iter().filter_map(|&i| f.changes.get(i)).map(|c| c.key.clone()).collect();
    if keys.is_empty() {
        return Err("Choose the changes to undo first.".into());
    }
    let new = toolscore::recdiff::undo(&before, &current, &keys);
    let tmp = f.path.with_extension("wgundo.tmp");
    std::fs::write(&tmp, &new).and_then(|()| std::fs::rename(&tmp, &f.path)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{name}: {e}")
    })?;
    f.expect = digest(&new);
    f.changes = toolscore::recdiff::diff(&before, &new);
    let said = format!(
        "{name}: {} put back{}.",
        keys.len(),
        if f.changes.iter().all(|c| c.key.tag == "TES3") { " - the plugin is as it was before the run" } else { "" }
    );
    let mut tb = Tables::new();
    tb.table("head", "s", 1);
    tb.s(&said);
    t3_tables(&mut tb, held);
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: Merged Objects (`editcore::mergeobjects`, a port of NullCascade's TES3Merge,
/// Static 2026-11-18): the setup's load order merged record by record into `Merged Objects.esp`
/// (`lists_only`: only the leveled lists, as TES3Merge's `multipatch`, into `multipatch.esp`),
/// written into `folder` (empty: the data folder holding most of the load order, as Merge Lands
/// picks) unless `dry_run`. `types` the record tags merged (empty: TES3Merge's default list),
/// `patches` any of `fogbug`, `summons`, `cellnames`, `ignore` plugins left out by name, `filters`
/// lines of `regex = true|false` over ids in lower case (TES3Merge.ini's `[ObjectFilters]`).
///
/// Answers bytes (WGTB, as `tools_tes3cmd`): `head` `ssu` (the status, the file written, how
/// many records), `lines` `s` (the log) and `types` `su` (records merged per type). Read by
/// `61_wg_tools.js` (`moRun`).
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)] // a command's arguments are the page's keys, one each
pub(super) fn tools_merge_objects(
    cfg: String,
    folder: String,
    types: Vec<String>,
    inclusive: bool,
    patches: Vec<String>,
    ignore: Vec<String>,
    filters: String,
    lists_only: bool,
    dry_run: bool,
) -> Result<tauri::ipc::Response, String> {
    use editcore::mergeobjects::{lists_only as lists, run, Options, DEFAULT_TYPES};
    let mut opts = Options::default();
    if !types.is_empty() {
        opts.types = types.iter().map(|t| t.trim().to_uppercase()).filter(|t| DEFAULT_TYPES.contains(&t.as_str()) || t == "FACT").collect();
    }
    if lists_only {
        opts = lists(opts);
    }
    opts.inclusive = inclusive;
    let has = |p: &str| patches.iter().any(|x| x.eq_ignore_ascii_case(p) || x.eq_ignore_ascii_case("all"));
    (opts.fogbug, opts.summons, opts.cellnames) = (has("fogbug"), has("summons"), has("cellnames"));
    opts.ignore = ignore.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let folder = folder.trim();
    let r = run(&cfg, (!folder.is_empty()).then(|| std::path::Path::new(folder)), opts, &filters, dry_run)?;
    let (status, lines, n) = (r.status, r.lines, r.outcome.records.len());
    let written = r.written.map(|p| p.display().to_string()).unwrap_or_default();
    let mut tb = Tables::new();
    tb.table("head", "ssu", 1);
    tb.s(&status).s(&written).u(n as u64);
    tb.table("lines", "s", lines.len());
    for l in &lines {
        tb.s(l);
    }
    tb.table("types", "su", r.outcome.by_type.len());
    for (t, n) in &r.outcome.by_type {
        tb.s(t).u(*n as u64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}
