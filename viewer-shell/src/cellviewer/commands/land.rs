// Merge Settings and Merge Lands: the `.mergedlands.toml` beside a plugin, and the merge itself.
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- Merge Settings (Wraithguard's `.mergedlands.toml` editor; port step 4.4) -----------

   The per-plugin Merge Lands settings file beside a plugin (`landcore::meta`): read
   (`land_meta_read`), previewed (`land_meta_preview`) and written (`land_meta_write`). */

/// The layers as the page sends them, `[{name, included, strategy}]`.
pub(super) fn land_meta_of(meta_type: &str, layers: &[serde_json::Value]) -> Result<landcore::meta::PluginMeta, String> {
    use landcore::meta::{MergeSettings, PluginMeta, Strategy, META_TYPES};
    if !META_TYPES.contains(&meta_type) {
        return Err(format!("unknown meta_type {meta_type:?}"));
    }
    let layers = layers
        .iter()
        .map(|l| {
            let name = l["name"].as_str().unwrap_or_default().to_string();
            let strategy = l["strategy"].as_str().unwrap_or("Auto");
            let conflict_strategy = Strategy::from_name(strategy).ok_or_else(|| format!("unknown strategy {strategy:?}"))?;
            Ok((name, MergeSettings { included: l["included"].as_bool().unwrap_or(true), conflict_strategy }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PluginMeta { meta_type: meta_type.to_string(), layers })
}

pub(super) fn land_meta_json(meta: &landcore::meta::PluginMeta) -> serde_json::Value {
    serde_json::json!({
        "metaType": meta.meta_type,
        "layers": meta.full().iter().map(|(n, s)| serde_json::json!({"name": n, "included": s.included, "strategy": s.conflict_strategy.name()})).collect::<Vec<_>>(),
    })
}

/// Wraithguard: a plugin's `.mergedlands.toml` (`_open_merge_settings_editor`): `{path, exists,
/// error, metaType, layers: [{name, included, strategy}], metaTypes, strategies}`. No file, or
/// one that will not read (`error` says why), starts from a `Patch` at the defaults.
#[tauri::command(async)]
pub(super) fn land_meta_read(plugin: String) -> Result<String, String> {
    use landcore::meta;
    let plugin = std::path::Path::new(&plugin);
    let path = meta::meta_path_for(plugin);
    let exists = path.is_file();
    let fresh = meta::PluginMeta { meta_type: meta::META_PATCH.into(), layers: Vec::new() };
    let (m, error) = if exists {
        match meta::load_meta(plugin) {
            Ok(m) => (m, String::new()),
            Err(e) => (fresh, e.0),
        }
    } else {
        (fresh, String::new())
    };
    let mut out = land_meta_json(&m);
    out["path"] = path.display().to_string().into();
    out["exists"] = exists.into();
    out["error"] = error.into();
    out["metaTypes"] = serde_json::json!(meta::META_TYPES);
    out["strategies"] = serde_json::json!(meta::STRATEGY_NAMES);
    Ok(out.to_string())
}

/// Wraithguard: the file the editor would write (`_ms_refresh_preview`), every layer spelled out
/// (a JSON string: the page's `Engine.call` reads every answer as JSON).
#[tauri::command(async)]
pub(super) fn land_meta_preview(meta_type: String, layers: Vec<serde_json::Value>) -> Result<String, String> {
    Ok(serde_json::Value::String(landcore::meta::render_meta(&land_meta_of(&meta_type, &layers)?, true)).to_string())
}

/// Wraithguard: the `.mergedlands.toml` written beside `plugin` (`_ms_write`, `write_settings`),
/// header and every layer; answers its path.
#[tauri::command(async)]
pub(super) fn land_meta_write(plugin: String, meta_type: String, layers: Vec<serde_json::Value>) -> Result<String, String> {
    let m = land_meta_of(&meta_type, &layers)?;
    landcore::meta::write_settings(std::path::Path::new(&plugin), &m).map(|p| serde_json::Value::String(p.display().to_string()).to_string()).map_err(|e| e.0)
}

/// Wraithguard: where Merge Lands writes by default (`_merged_lands_home`): the data folder
/// holding most of the load order. Answers `{folders, home, name, exists}` (`exists`: a
/// `Merged Lands.esp` is already there).
#[tauri::command(async)]
pub(super) fn land_merge_home(cfg: String) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let folders: Vec<PathBuf> = setup.data.iter().filter(|d| d.is_dir()).cloned().collect();
    let best = landcore::service::home_folder(&folders, &order);
    let name = landcore::service::DEFAULT_NAME;
    Ok(serde_json::json!({
        "folders": folders.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "home": best.as_ref().map(|b| b.display().to_string()),
        "name": name,
        "exists": best.is_some_and(|b| b.join(name).is_file()),
        "ignore": landcore::service::DEFAULT_IGNORE,
    })
    .to_string())
}

/// Wraithguard: Merge Lands (`build_merged_lands`, landcore::service) over the cfg's load
/// order and data folders, `Merged Lands.esp` written into `folder` (`dryRun`: nothing
/// written). `model` `"masters"` measures each plugin against the masters, `"loadorder"`
/// against the load order before it (the OpenMW fork); `ignore` the plugins left out by name
/// or folder (`None`: the fork's defaults). Answers `{error, cancelled, lines, notes, output, result: {cells, textures, contested, major,
/// seams, slopes, clamped, dropped, borrowed}}` - `error` the reason it stopped, the lines said
/// before it kept. `log` is every line it said, as [`LAND_LOG`] has them (the time it took
/// before each), for the page to keep showing when the run ends.
///
/// While it runs, each line it says is kept in [`LAND_LOG`] for `land_merge_progress`, so the
/// page shows the merge as it goes (Static, 2026-10-27: a 621-plugin order sat on "Reading
/// the load order's terrain..." with nothing to say whether it was moving).
#[tauri::command(async)]
pub(super) fn land_merge(cfg: String, folder: String, dry_run: bool, verbose: bool, model: String, ignore: Option<Vec<String>>) -> Result<String, String> {
    use landcore::pipeline::DiffModel;
    use landcore::service::{build_merged_lands, MergeOptions, DEFAULT_NAME};
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let dirs: Vec<PathBuf> = setup.data.iter().filter(|d| d.is_dir()).cloned().collect();
    let output = (!folder.is_empty()).then(|| PathBuf::from(&folder).join(DEFAULT_NAME));
    let model = if model == "loadorder" { DiffModel::LoadOrder } else { DiffModel::Masters };
    // A new switch each run, so a Cancel meant for the last one cannot stop this one.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    *LAND_STOP.lock().unwrap_or_else(|p| p.into_inner()) = Some(stop.clone());
    let mut opts = MergeOptions { output, dry_run, verbose, model, cancel: Some(stop), ..Default::default() };
    if let Some(ignore) = ignore {
        opts.ignore = ignore;
    }
    let started = std::time::Instant::now();
    {
        let mut log = LAND_LOG.lock().unwrap_or_else(|p| p.into_inner());
        log.clear();
        log.push(format!("{} plugin(s) in the load order, {} data folder(s)", order.len(), dirs.len()));
    }
    let mut say = |line: &str| {
        LAND_LOG.lock().unwrap_or_else(|p| p.into_inner()).push(format!("[{:>5.1}s] {line}", started.elapsed().as_secs_f32()));
    };
    let mut out = match build_merged_lands(&dirs, &order, &opts, &mut say) {
        Ok(r) => serde_json::json!({
            "error": null, "lines": r.lines, "notes": r.notes,
            "output": r.output.map(|p| p.display().to_string()),
            "result": {"cells": r.cells_written, "textures": r.textures_written, "contested": r.contested, "major": r.major,
                       "seams": r.seam_vertices, "slopes": r.slope_vertices, "clamped": r.clamped, "dropped": r.dropped, "borrowed": r.borrowed},
        }),
        Err((error, lines)) => serde_json::json!({"error": error, "cancelled": error == landcore::service::CANCELLED, "lines": lines, "notes": [], "output": null, "result": null}),
    };
    // Every line the run said, with the time it took to say it: the page showed them while the
    // merge ran, then replaced them with `lines` (the report, without the timings and without
    // the progress-only lines), so what the verbose log had told was gone as it finished.
    out["log"] = serde_json::json!(LAND_LOG.lock().unwrap_or_else(|p| p.into_inner()).clone());
    *LAND_STOP.lock().unwrap_or_else(|p| p.into_inner()) = None;
    Ok(out.to_string())
}

/// The running Merge Lands' stop switch (`land_merge_cancel`), while one runs.
pub(super) static LAND_STOP: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = Mutex::new(None);

/// Stops the running Merge Lands at its next step; it writes nothing. `true` when one was
/// running.
#[tauri::command(async)]
pub(super) fn land_merge_cancel() -> Result<bool, String> {
    let stop = LAND_STOP.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(s) = &stop {
        s.store(true, std::sync::atomic::Ordering::Relaxed);
        LAND_LOG.lock().unwrap_or_else(|p| p.into_inner()).push("cancel asked: stopping at the next step".into());
    }
    Ok(stop.is_some())
}

/// The running (or last) Merge Lands' lines, as it says them.
pub(super) static LAND_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The Merge Lands lines from `from` on: `{lines, next}` - the page asks every half second
/// while `land_merge` runs and shows them as they come.
#[tauri::command(async)]
pub(super) fn land_merge_progress(from: usize) -> Result<String, String> {
    let log = LAND_LOG.lock().unwrap_or_else(|p| p.into_inner());
    let lines: Vec<&String> = log.iter().skip(from).collect();
    Ok(serde_json::json!({"lines": lines, "next": log.len()}).to_string())
}
