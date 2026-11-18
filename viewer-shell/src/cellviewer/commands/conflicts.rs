// The Conflicts page's commands: the record and loose-file scans, a record across its plugins, the
// terrain preview, the texture compare, the reports, the mesh optimizer, and the plugin view.
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- the Conflicts page (Wraithguard; port step 4.3, slice 11) ----------------------

   Wraithguard's conflict window, answered by `conflictcore`: the record scan over the load
   order the launch's openmw.cfg names (`conflicts_scan`, conflicts or either kind of single),
   one record's fields across its plugins with what each field and plugin is doing
   (`conflicts_fields`), the loose-file scan with its mesh findings (`conflicts_resources`), a
   texture pair compared (`conflicts_texture`), the reports and CSVs (`conflicts_report`,
   `conflicts_csv`), and a record or field queued for the patch in the journal the Editor and
   Wraithguard share (`conflicts_patch`). The last scans are kept here between calls. */

/* ---- the scans' share of the machine, and what they keep (2026-10-27) -----------------

   Static: several scans started from the pages (conflicts, files, lint) took every core,
   and a cell load waited minutes behind them at 0%. So the scans get their own budget -
   half the machine (`scan_budget`, which conflictcore's workers keep to as well) - and run
   one at a time (`scan_gate`), leaving the rest to drawing and loading cells. And a
   plugin's conflict keys and cells are kept in an SQLite cache beside the program
   (`conflictcore::keycache`, `wraithguard_scan_cache.sqlite3`), filled in the background
   after the program connects (`scan_cache_warm`), so a scan of a load order already read
   is a lookup, not 600 plugins read again. */

/// The threads a scan may use: half the machine's (at least one, at most eight), set for
/// conflictcore's workers too the first time it is asked.
pub(super) fn scan_budget() -> usize {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let n = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2) / 2).clamp(1, 8);
    ONCE.call_once(|| conflictcore::vfs::set_scan_threads(n));
    n
}

/// One scan at a time: held for the length of a scan, so two pages' scans queue rather than
/// share the machine with the cell load.
pub(super) fn scan_gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: Mutex<()> = Mutex::new(());
    scan_budget();
    GATE.lock().unwrap_or_else(|p| p.into_inner())
}

/// The scans' cache of plugin keys and cells, beside the program; `None` when there is no
/// folder for it (the cache is a shortcut, never needed).
pub(super) fn scan_cache() -> Option<&'static editcore::cache::Cache> {
    static CACHE: std::sync::OnceLock<Option<editcore::cache::Cache>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let dir = app_dir().ok()?;
            Some(editcore::cache::Cache::with_warn(
                dir.join("wraithguard_scan_cache.sqlite3"),
                conflictcore::keycache::PARSER,
                editcore::cache::MAX_BYTES,
                Box::new(|m| eprintln!("{m}")),
            ))
        })
        .as_ref()
}

/// Wraithguard: fills the scans' cache for the cfg's load order in the background, on two
/// threads (the page asks for it once the program has connected). Answers at once.
#[tauri::command(async)]
pub(super) fn scan_cache_warm(cfg: String) -> Result<(), String> {
    let setup = cf_setup(&cfg)?;
    let files: Vec<PathBuf> = setup.locate(&setup.plugins()).into_iter().filter_map(|(_, p)| p).collect();
    std::thread::Builder::new()
        .name("scan-cache-warm".into())
        .spawn(move || {
            if let Some(c) = scan_cache() {
                let _ = conflictcore::keycache::warm(c, &files, 2);
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(super) struct ConflictScan {
    /// Each scanned plugin's file, as the scan located it: a record's detail uses these and
    /// does not list the (1000+) data folders again for every click.
    paths: std::collections::HashMap<String, PathBuf>,
    list: Vec<conflictcore::records::Conflict>,
    stats: conflictcore::records::Stats,
    mode: String,
    /// Which scan this is (`SCAN_IDS`): the verdicts judged of it (`JUDGED`) are its own.
    id: u64,
}

/// Each record scan's number, for the verdicts kept of it (`JUDGED`).
static SCAN_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) struct ResourceScan {
    list: Vec<conflictcore::resources::Resource>,
    stats: conflictcore::resources::ResourceStats,
    mesh: Vec<Option<conflictcore::mesh::Finding>>,
}

pub(super) static CONFLICTS: Mutex<Option<ConflictScan>> = Mutex::new(None);
pub(super) static RESOURCES: Mutex<Option<ResourceScan>> = Mutex::new(None);

/// The setup a page names (its `cfg`), read: the plugins in order, the data folders by
/// priority, the archives. `cfg` is one of three (`setup_spec`):
///
/// - an `openmw.cfg` (and the cfgs it chains), as OpenMW reads it;
/// - `mo2:<Mod Organizer folder>`, or `mo2:<folder>|<profile>`: the profile's `Morrowind.ini`
///   for the plugins and archives, its enabled mods by priority for the data folders, the
///   game's `Data Files` lowest;
/// - a `Data Files` folder, or the `Morrowind.ini` beside it: the game as it is installed.
///
/// The last two are Robin's readers (`viewcore::layout`, Gardenfell's), made into the same
/// shape as a cfg's, so every page reads an MO2 or a Morrowind.ini install as it reads an
/// OpenMW one (Static, 2026-11-10).
pub(super) fn cf_setup(cfg: &str) -> Result<viewcore::ordercore::cfg::OpenmwCfg, String> {
    viewcore::ordercore::install::read(cfg)
}

/// What a setup's name says it is: `(kind, path, profile)` - `openmw`, `mo2` or `vanilla`
/// (a Morrowind.ini named stands for the `Data Files` beside it; `ordercore::install::spec`).
pub(super) fn setup_spec(spec: &str) -> (String, String, Option<String>) {
    let s = viewcore::ordercore::install::spec(spec);
    (s.kind.name().to_string(), s.path, s.profile)
}

/// A Morrowind.ini or MO2 install in a cfg's shape (`ordercore::install::from_layout`).
pub(super) fn setup_from_layout(l: &layout::Layout) -> viewcore::ordercore::cfg::OpenmwCfg {
    viewcore::ordercore::install::from_layout(l)
}

/// A folder's path as compared here: trailing separators off, case folded.
pub(super) fn mo_folded(p: &str) -> String {
    p.trim_end_matches(['/', '\\']).to_lowercase()
}

/// The meshes of the load order that the optimizer looks at: the winning copy of each,
/// not those in the output folder (what it wrote before is not an input).
pub(super) fn mo_candidates(cfg: &str, opts: &conflictcore::nif_batch::Options, out_dir: &str) -> Result<Vec<conflictcore::nif_batch::Candidate>, String> {
    use conflictcore::nif_batch as nb;
    let setup = cf_setup(cfg)?;
    let skip = mo_folded(out_dir);
    let data: Vec<std::path::PathBuf> =
        setup.data.iter().filter(|d| skip.is_empty() || mo_folded(&d.to_string_lossy()) != skip).cloned().collect();
    Ok(nb::filter(nb::collect(&data), opts))
}

/// Wraithguard: the mesh optimizer's measurement over a load order (Tools). `opts`:
/// `{minGain: percent, minTriangles, folder}`; `out_dir`: the folder the copies will go to
/// (left out of the inputs); `limit`: the most rows sent, best gain first. Answers
/// `{stats: {meshes, unreadable, improvable, qualifying, qualifyingTriangles,
/// qualifyingBytes}, rows: [{path, dir, bytes, triangles, shapes, improvable, gain, acmr,
/// overdraw, overfetch, qualifies, note}], shown}`. What was measured is kept in the scan
/// cache, so a second run measures only the meshes that changed.
#[tauri::command(async)]
pub(super) fn meshopt_analyse(cfg: String, opts: serde_json::Value, out_dir: String, limit: usize) -> Result<String, String> {
    use conflictcore::nif_batch as nb;
    let _gate = scan_gate();
    let o = nb::Options::from_json(&opts);
    let cands = mo_candidates(&cfg, &o, &out_dir)?;
    let an = nb::analyse_all(scan_cache(), &cands);
    let mut stats = (0usize, 0usize, 0usize, 0u64, 0u64);
    for a in &an {
        if !a.unreadable.is_empty() {
            stats.0 += 1;
        }
        if a.improvable > 0 {
            stats.1 += 1;
        }
        if nb::qualifies(a, &o) {
            stats.2 += 1;
            stats.3 += a.triangles;
            stats.4 += a.bytes;
        }
    }
    let mut order: Vec<usize> = (0..cands.len()).collect();
    order.sort_by(|&x, &y| {
        let (qx, qy) = (nb::qualifies(&an[x], &o), nb::qualifies(&an[y], &o));
        qy.cmp(&qx).then(an[y].gain.partial_cmp(&an[x].gain).unwrap_or(std::cmp::Ordering::Equal))
    });
    let shown = order.len().min(if limit == 0 { 5000 } else { limit });
    let rows: Vec<serde_json::Value> = order.iter().take(shown).map(|&i| nb::row_json(&cands[i], &an[i], &o)).collect();
    let out = serde_json::json!({
        "stats": {"meshes": cands.len(), "unreadable": stats.0, "improvable": stats.1, "qualifying": stats.2,
                  "qualifyingTriangles": stats.3, "qualifyingBytes": stats.4},
        "rows": rows, "shown": shown,
    });
    serde_json::to_string(&out).map_err(|e| e.to_string())
}

/// Wraithguard: writes the optimized copies (Tools). The meshes that qualify under `opts`
/// (and, when `paths` is not empty, are in it: the rows the person left ticked), each
/// optimized, read back and checked to be the same mesh, at its own path under `out_dir`,
/// and `wraithguard-mesh-optimiser.json` beside them. Nothing in any mod is touched.
/// Answers `{written, failed, bytesBefore, bytesAfter, failures: [{path, why}]}`.
#[tauri::command(async)]
pub(super) fn meshopt_write(cfg: String, opts: serde_json::Value, out_dir: String, paths: Vec<String>) -> Result<String, String> {
    use conflictcore::nif_batch as nb;
    if out_dir.trim().is_empty() {
        return Err("name the folder the optimized meshes go to".into());
    }
    let _gate = scan_gate();
    let o = nb::Options::from_json(&opts);
    let mut cands = mo_candidates(&cfg, &o, &out_dir)?;
    if !paths.is_empty() {
        let want: std::collections::HashSet<String> = paths.iter().map(|p| p.to_lowercase()).collect();
        cands.retain(|c| want.contains(&c.rel.to_lowercase()));
    }
    let an = nb::analyse_all(scan_cache(), &cands);
    let picked: Vec<&nb::Candidate> = cands.iter().zip(&an).filter(|(_, a)| nb::qualifies(a, &o)).map(|(c, _)| c).collect();
    let out = std::path::Path::new(&out_dir);
    std::fs::create_dir_all(out).map_err(|e| format!("{out_dir} could not be made: {e}"))?;
    let done = nb::write_all(&picked, out);
    let manifest = nb::manifest(&done, &o);
    std::fs::write(out.join("wraithguard-mesh-optimiser.json"), serde_json::to_string_pretty(&manifest).unwrap_or_default())
        .map_err(|e| format!("the record could not be written: {e}"))?;
    let ok: Vec<&nb::Written> = done.iter().filter(|w| w.error.is_empty()).collect();
    let res = serde_json::json!({
        "written": ok.len(), "failed": done.len() - ok.len(),
        "bytesBefore": ok.iter().map(|w| w.before).sum::<u64>(), "bytesAfter": ok.iter().map(|w| w.after).sum::<u64>(),
        "failures": done.iter().filter(|w| !w.error.is_empty()).take(200).map(|w| serde_json::json!({"path": w.rel, "why": w.error})).collect::<Vec<_>>(),
    });
    serde_json::to_string(&res).map_err(|e| e.to_string())
}

/// Wraithguard: how far the optimizer's run has got, `[done, total]`.
#[tauri::command(async)]
pub(super) fn meshopt_progress() -> [usize; 2] {
    use std::sync::atomic::Ordering;
    [conflictcore::nif_batch::DONE.load(Ordering::Relaxed), conflictcore::nif_batch::TOTAL.load(Ordering::Relaxed)]
}

/// Wraithguard: the record scan (`detect_conflicts` / `list_subset_singles` /
/// `list_other_singles`). `mode`: `conflicts`, `mine` or `other`; `subset`: the user's own
/// plugins; `exclude`: patterns of plugins left out (`filter_plugins`). Answers `{list,
/// stats: {scanned, unreadable, records, found, excluded}, cells: [{x, y, total, mine, types,
/// plugins, byPlugin, winners}]}` as bytes (`cf_scan_tables`).
#[tauri::command(async)]
pub(super) fn conflicts_scan(
    cfg: String,
    subset: Vec<String>,
    mode: String,
    exclude: Vec<String>,
) -> Result<tauri::ipc::Response, String> {
    use conflictcore::records;
    let setup = cf_setup(&cfg)?;
    let (order, excluded) = records::filter_plugins(&setup.plugins(), &exclude);
    let at: std::collections::HashMap<String, PathBuf> =
        setup.locate(&order).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let paths = at.clone();
    let find = move |name: &str| at.get(name).cloned();
    let _gate = scan_gate();
    let scan = records::scan_touch(&order, &find, &conflictcore::keycache::cached_keys(scan_cache()));
    let (list, stats) = match mode.as_str() {
        "conflicts" => records::conflicts(&scan, &subset),
        "mine" => records::singles(&scan, &subset, true),
        "other" => records::singles(&scan, &subset, false),
        _ => return Err(format!("no scan {mode:?}")),
    };
    let cells = if mode == "conflicts" { conflictcore::map::group_by_cell(&list) } else { Default::default() };
    let out = cf_scan_tables(&list, &stats, &excluded, &cells);
    *CONFLICTS.lock().unwrap_or_else(|p| p.into_inner()) = Some(ConflictScan { paths, list, stats, mode, id: SCAN_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) });
    Ok(tauri::ipc::Response::new(out))
}

/* Wraithguard: `conflicts_scan`'s answer as bytes (`WGTB`, Static 2026-11-17: JSON only where
   it is needed; a row a conflicting record, tens of thousands on a large load order). Every
   plugin is named once, in `plugins`, and the other tables give its index there:

   - `stats` `uuu` (scanned, records, found); `unreadable` `s`; `excluded` `s`.
   - `list` `ssuu` (type, id, winner, involves the user's own plugins 0/1), with `list_plugins`
     `uu` and `list_deleted` `uu` (its row, a plugin), in order.
   - `cells` `iiuu` (x, y, total, mine), with `cell_types` `usu` (its row, type, count),
     `cell_plugins` `uu`, `cell_by` `uusu` (its row, plugin, type, count) and `cell_winners`
     `uuu` (its row, plugin, count).

   Read by `WgConflicts.readScan` (60_wg_conflicts.js), back into the JSON's object. */
fn cf_scan_tables(
    list: &[conflictcore::records::Conflict],
    stats: &conflictcore::records::Stats,
    excluded: &[String],
    cells: &std::collections::BTreeMap<(i64, i64), conflictcore::map::CellConflicts>,
) -> Vec<u8> {
    let mut names: Vec<&str> = Vec::new();
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for c in list {
        for n in c.plugins.iter().chain(c.deleted_by.iter()).chain(std::iter::once(&c.winner)) {
            if !at.contains_key(n.as_str()) {
                at.insert(n.as_str(), names.len());
                names.push(n.as_str());
            }
        }
    }
    for c in cells.values() {
        for n in c.plugins.iter().chain(c.by_plugin.keys()).chain(c.winners.keys()) {
            if !at.contains_key(n.as_str()) {
                at.insert(n.as_str(), names.len());
                names.push(n.as_str());
            }
        }
    }
    let p = |n: &str| at[n] as u64;
    let mut tb = Tables::new();
    tb.table("stats", "uuu", 1);
    tb.u(stats.scanned as u64).u(stats.records as u64).u(stats.found as u64);
    tb.table("unreadable", "s", stats.unreadable.len());
    for n in &stats.unreadable {
        tb.s(n);
    }
    tb.table("excluded", "s", excluded.len());
    for n in excluded {
        tb.s(n);
    }
    tb.table("plugins", "s", names.len());
    for n in &names {
        tb.s(n);
    }
    tb.table("list", "ssuu", list.len());
    for c in list {
        tb.s(&c.rtype).s(&c.id).u(p(&c.winner)).u(c.involves_subset as u64);
    }
    tb.table("list_plugins", "uu", list.iter().map(|c| c.plugins.len()).sum());
    for (row, c) in list.iter().enumerate() {
        for n in &c.plugins {
            tb.u(row as u64).u(p(n));
        }
    }
    tb.table("list_deleted", "uu", list.iter().map(|c| c.deleted_by.len()).sum());
    for (row, c) in list.iter().enumerate() {
        for n in &c.deleted_by {
            tb.u(row as u64).u(p(n));
        }
    }
    tb.table("cells", "iiuu", cells.len());
    for c in cells.values() {
        tb.i(c.x).i(c.y).u(c.total as u64).u(c.mine as u64);
    }
    tb.table("cell_types", "usu", cells.values().map(|c| c.types.len()).sum());
    for (row, c) in cells.values().enumerate() {
        for (k, n) in &c.types {
            tb.u(row as u64).s(k).u(*n as u64);
        }
    }
    tb.table("cell_plugins", "uu", cells.values().map(|c| c.plugins.len()).sum());
    for (row, c) in cells.values().enumerate() {
        for n in &c.plugins {
            tb.u(row as u64).u(p(n));
        }
    }
    tb.table("cell_by", "uusu", cells.values().map(|c| c.by_plugin.values().map(|m| m.len()).sum::<usize>()).sum());
    for (row, c) in cells.values().enumerate() {
        for (n, types) in &c.by_plugin {
            for (k, count) in types {
                tb.u(row as u64).u(p(n)).s(k).u(*count as u64);
            }
        }
    }
    tb.table("cell_winners", "uuu", cells.values().map(|c| c.winners.len()).sum());
    for (row, c) in cells.values().enumerate() {
        for (n, count) in &c.winners {
            tb.u(row as u64).u(p(n)).u(*count as u64);
        }
    }
    tb.done()
}

/// Wraithguard: one record of the last scan across its plugins (`diff_record_fields`, with
/// `wraithguard.patch.status`). Answers `{plugins, fields: [{name, values: [value | null],
/// present: [bool], differs, all, this: [status]}], all, this}` - `all` a field's or the
/// record's `ConflictAll` (0-4), `this` each plugin's `ConflictThis` name.
#[tauri::command(async)]
pub(super) fn conflicts_fields(cfg: String, rtype: String, id: String) -> Result<String, String> {
    use conflictcore::fields::{conflict_all, conflict_this, diff, worst_all, worst_this};
    let (plugins, located): (Vec<String>, Vec<Option<PathBuf>>) = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        let names = scan.list.iter().find(|c| c.rtype == rtype && c.id == id).map(|c| c.plugins.clone()).ok_or("that record is not in the last scan")?;
        let at: Vec<Option<PathBuf>> = names.iter().map(|n| scan.paths.get(n).cloned()).collect();
        (names, at)
    };
    // The files the scan found; only a plugin it has no path for (the program was handed a
    // different cfg since) is looked for in the data folders.
    let located: Vec<Option<PathBuf>> = if located.iter().all(Option::is_some) {
        located
    } else {
        let setup = cf_setup(&cfg)?;
        setup.locate(&plugins).into_iter().map(|(_, p)| p).collect()
    };
    // One thread a plugin (a record is in a handful). Each asks the scan cache where the
    // record lies in its file and reads just those bytes; a plugin not indexed yet is read
    // once and indexed (`keycache::record_at`), so clicking through records is a seek each.
    let records: Vec<Option<serde_json::Value>> = std::thread::scope(|s| {
        let jobs: Vec<_> = located
            .iter()
            .map(|p| {
                let (rtype, id) = (&rtype, &id);
                s.spawn(move || p.as_ref().and_then(|p| conflictcore::keycache::record_at(scan_cache(), p, rtype, id)))
            })
            .collect();
        jobs.into_iter().map(|j| j.join().ok().flatten()).collect()
    });
    let versions: Vec<(String, Option<&serde_json::Value>)> = plugins.iter().cloned().zip(records.iter().map(Option::as_ref)).collect();
    let d = diff(&versions);
    let mut field_all = Vec::new();
    let mut field_this: Vec<Vec<conflictcore::fields::ConflictThis>> = Vec::new();
    let fields: Vec<serde_json::Value> = d
        .ordered
        .iter()
        .map(|name| {
            let values = d.values(name);
            let all = conflict_all(&values, false);
            let this = conflict_this(&values, false);
            field_all.push(all);
            field_this.push(this.clone());
            serde_json::json!({
                "name": name,
                "values": values.iter().map(|v| v.cloned().unwrap_or(serde_json::Value::Null)).collect::<Vec<_>>(),
                "present": values.iter().map(Option::is_some).collect::<Vec<_>>(),
                "differs": d.differing.contains(name),
                "all": all.code(),
                "this": this.iter().map(|t| t.name()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let per_plugin: Vec<&str> =
        (0..plugins.len()).map(|i| worst_this(field_this.iter().map(|t| t[i])).name()).collect();
    Ok(serde_json::json!({
        "plugins": plugins,
        "read": records.iter().map(Option::is_some).collect::<Vec<_>>(),
        "fields": fields,
        "all": worst_all(field_all).code(),
        "this": per_plugin,
    })
    .to_string())
}

/// Wraithguard: a conflicting `LAND` record's terrain in each plugin, and what each conflict
/// strategy would make of it (`_compare_merge_strategies`, `landcore::preview`): the first
/// plugin with heights the base, the rest merged in load order. `id` is the scan's `(x, y)`.
/// Answers `{plugins: [{name, heights}], strategies: [{name, heights}]}` (65x65, world units).
#[tauri::command(async)]
pub(super) fn conflicts_land_surfaces(cfg: String, id: String) -> Result<String, String> {
    let plugins: Vec<String> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        scan.list.iter().find(|c| c.rtype == "Landscape" && c.id == id).map(|c| c.plugins.clone()).ok_or("that cell is not in the last scan")?
    };
    let nums: Vec<i64> = id.trim_matches(|c| c == '(' || c == ')').split(',').filter_map(|n| n.trim().parse().ok()).collect();
    let [gx, gy] = nums[..] else { return Err(format!("{id} is not a cell")) };
    let located: Vec<(String, Option<PathBuf>)> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let known: Option<Vec<(String, Option<PathBuf>)>> = slot
            .as_ref()
            .map(|scan| plugins.iter().map(|n| (n.clone(), scan.paths.get(n).cloned())).collect())
            .filter(|v: &Vec<(String, Option<PathBuf>)>| v.iter().all(|(_, p)| p.is_some()));
        drop(slot);
        match known {
            Some(v) => v,
            None => cf_setup(&cfg)?.locate(&plugins),
        }
    };
    let mut surfaces: Vec<(String, Vec<landcore::grid::Value>)> = Vec::new();
    for (name, path) in located {
        let Some(path) = path else { continue };
        let Ok(records) = landcore::read::read_landscape_records(&path) else { continue };
        let found = records.iter().rev().find(|r| matches!(r, landcore::read::Record::Land { grid, .. } if *grid == (gx, gy)));
        let layers = found.and_then(|r| landcore::layers::LandscapeLayers::from_record(r).ok().flatten());
        if let Some(h) = layers.and_then(|l| l.heights) {
            surfaces.push((name, h.to_vec()));
        }
    }
    if surfaces.len() < 2 {
        return Err("Only one plugin edits this cell's terrain, so every strategy gives the same result. Strategy choice only matters where two or more plugins change the same ground.".into());
    }
    let grids: Vec<Vec<landcore::grid::Value>> = surfaces.iter().map(|(_, h)| h.clone()).collect();
    let strategies: Vec<serde_json::Value> = landcore::preview::PREVIEW_STRATEGIES
        .iter()
        .filter_map(|s| landcore::preview::merge_preview(&grids, *s).ok().map(|h| serde_json::json!({"name": s.name(), "heights": h})))
        .collect();
    Ok(serde_json::json!({
        "plugins": surfaces.iter().map(|(n, h)| serde_json::json!({"name": n, "heights": h})).collect::<Vec<_>>(),
        "strategies": strategies,
    })
    .to_string())
}

/// Wraithguard: the loose-file scan (`detect_resource_conflicts`) over the cfg's data
/// folders, and the mesh analysis of those that differ (`analyse_mesh_conflicts`; `meshes`
/// false: none; `meshLimit` 0: all). The user's own folders are `subsetDirs` and those the
/// Load order page's plan file names (`planFile`, its `customDataDirs`; `""`: none). Answers
/// `{list: [{path, providers, winner, involvesSubset, identical, mesh, meshColumns}], stats,
/// meshStats}`.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub(super) fn conflicts_resources(cfg: String, subset_dirs: Vec<String>, exclude_exts: Vec<String>, compare: bool, meshes: bool, mesh_limit: usize, plan_file: String) -> Result<tauri::ipc::Response, String> {
    let _gate = scan_gate();
    let mut subset_dirs = subset_dirs;
    let plan: Option<serde_json::Value> =
        if plan_file.is_empty() { None } else { std::fs::read_to_string(&plan_file).ok().and_then(|t| serde_json::from_str(&t).ok()) };
    if let Some(plan) = plan {
        subset_dirs.extend(plan["customDataDirs"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)));
    }
    let setup = cf_setup(&cfg)?;
    let dirs: Vec<String> = setup.data.iter().map(|d| d.to_string_lossy().into_owned()).collect();
    let (list, stats) = conflictcore::resources::detect(&dirs, &subset_dirs, &exclude_exts, compare);
    let (mesh, mstats) = if meshes { conflictcore::mesh::analyse(&list, mesh_limit) } else { (vec![None; list.len()], conflictcore::mesh::MeshStats::default()) };
    let out = cf_files_tables(&list, &stats, &mesh, &mstats);
    *RESOURCES.lock().unwrap_or_else(|p| p.into_inner()) = Some(ResourceScan { list, stats, mesh });
    Ok(tauri::ipc::Response::new(out))
}

/* Wraithguard: `conflicts_resources`'s answer as bytes (`WGTB`, Static 2026-11-17: JSON only
   where it is needed; a row a file that more than one folder supplies). Each data folder is
   named once, in `dirs`:

   - `stats` `uuuuu` (folders, files, conflicts, identical, differing); `mesh_stats` `uuu`
     (analysed, findings, unreadable).
   - `list` `suuu` (path, winner, involves the user's own folders 0/1, identical: 0 no, 1 yes,
     2 not compared), with `list_providers` `uu` (its row, a folder), in order.
   - `mesh` `usssss` (its row, what the mesh loses, and the four columns) for a file whose
     mesh was read.

   Read by `WgConflicts.readFiles` (60_wg_conflicts.js), back into the JSON's object. */
fn cf_files_tables(
    list: &[conflictcore::resources::Resource],
    stats: &conflictcore::resources::ResourceStats,
    mesh: &[Option<conflictcore::mesh::Finding>],
    mstats: &conflictcore::mesh::MeshStats,
) -> Vec<u8> {
    let mut names: Vec<&str> = Vec::new();
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for c in list {
        for n in c.providers.iter().chain(std::iter::once(&c.winner)) {
            if !at.contains_key(n.as_str()) {
                at.insert(n.as_str(), names.len());
                names.push(n.as_str());
            }
        }
    }
    let mut tb = Tables::new();
    tb.table("stats", "uuuuu", 1);
    tb.u(stats.dirs as u64).u(stats.files as u64).u(stats.conflicts as u64).u(stats.identical as u64).u(stats.differing as u64);
    tb.table("mesh_stats", "uuu", 1);
    tb.u(mstats.analysed as u64).u(mstats.findings as u64).u(mstats.unreadable as u64);
    tb.table("dirs", "s", names.len());
    for n in &names {
        tb.s(n);
    }
    tb.table("list", "suuu", list.len());
    for c in list {
        tb.s(&c.path).u(at[c.winner.as_str()] as u64).u(c.involves_subset as u64).u(match c.identical {
            Some(false) => 0,
            Some(true) => 1,
            None => 2,
        });
    }
    tb.table("list_providers", "uu", list.iter().map(|c| c.providers.len()).sum());
    for (row, c) in list.iter().enumerate() {
        for n in &c.providers {
            tb.u(row as u64).u(at[n.as_str()] as u64);
        }
    }
    tb.table("mesh", "usssss", mesh.iter().flatten().count());
    for (row, m) in mesh.iter().enumerate() {
        if let Some(f) = m {
            let (a, b, c, d) = f.columns();
            tb.u(row as u64).s(&f.describe()).s(&a).s(&b).s(&c).s(&d);
        }
    }
    tb.done()
}

/// Bytes as standard base64, for the page's `atob`.
pub(super) fn cf_b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

/// RGBA decoded for comparing: what the viewer's texture reader gives as straight RGBA or
/// blocks (the first level), else the reason.
pub(super) fn cf_rgba(bytes: &[u8], path: &str) -> Result<conflictcore::textures::Rgba, String> {
    use viewcore::img::Kind;
    let t = viewcore::img::read(bytes, path, false)?;
    let (w, h, px) = t.levels.into_iter().next().ok_or("no image in the file")?;
    let pixels = match t.kind {
        Kind::Rgba => px,
        Kind::File | Kind::Volume => return Err("this format is not decoded here".into()),
        k => viewcore::img::decode_blocks(&px, w, h, k),
    };
    Ok(conflictcore::textures::Rgba { width: w as usize, height: h as usize, pixels })
}

/// Wraithguard: a texture conflict's pair compared (`_texture_compare_payload`): the file the
/// winner replaced and the winner. Answers `{left, right: {label, w, h, rgba (base64) | null},
/// verdict, detail, changedShare, worstChannel, meanChannel, diff: {w, h, rgba} | null}`, the
/// images at most `max` on their long side for the page.
#[tauri::command(async)]
pub(super) fn conflicts_texture(path: String, providers: Vec<String>, max: u32) -> Result<String, String> {
    use conflictcore::textures::{compare_bytes, difference, pair, Verdict};
    let (lo, wi) = pair(&providers).ok_or("one provider: nothing to compare")?;
    let read = |dir: &str| std::fs::read(std::path::Path::new(dir).join(&path)).map_err(|e| format!("{dir}: {e}"));
    let (lb, wb) = (read(lo)?, read(wi)?);
    let c = compare_bytes(&lb, &wb, |b| cf_rgba(b, &path));
    let b64 = cf_b64;
    let side = |dir: &str, bytes: &[u8]| -> serde_json::Value {
        let label = format!("{} / {path}", std::path::Path::new(dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        match cf_rgba(bytes, &path) {
            Ok(i) => {
                let (w, h, px) = viewcore::img::shrink_rgba(i.width as u32, i.height as u32, i.pixels, max);
                serde_json::json!({"label": label, "w": w, "h": h, "rgba": b64(&px)})
            }
            Err(e) => serde_json::json!({"label": label, "w": 0, "h": 0, "rgba": null, "error": e}),
        }
    };
    let diff = if c.verdict == Verdict::Different && c.worst_channel > 0 {
        match (cf_rgba(&lb, &path), cf_rgba(&wb, &path)) {
            (Ok(a), Ok(b)) if a.width == b.width && a.height == b.height => {
                let d = difference(&a.pixels, &b.pixels, 4);
                let (w, h, px) = viewcore::img::shrink_rgba(a.width as u32, a.height as u32, d, max);
                serde_json::json!({"w": w, "h": h, "rgba": b64(&px)})
            }
            _ => serde_json::Value::Null,
        }
    } else {
        serde_json::Value::Null
    };
    Ok(serde_json::json!({
        "left": side(lo, &lb), "right": side(wi, &wb), "verdict": c.verdict.name(), "detail": c.detail,
        "changedShare": c.changed_share, "worstChannel": c.worst_channel, "meanChannel": c.mean_channel, "diff": diff,
    })
    .to_string())
}

/// Wraithguard: the text report of the last scan (`kind`: `records` or `files`), the user's
/// own only with `subsetOnly`, at most `limit` listed (0: all).
#[tauri::command(async)]
pub(super) fn conflicts_report(kind: String, subset_only: bool, limit: usize) -> Result<String, String> {
    if kind == "files" {
        let slot = RESOURCES.lock().unwrap_or_else(|p| p.into_inner());
        let r = slot.as_ref().ok_or("scan the files first")?;
        let mesh: Vec<Option<conflictcore::resources::MeshColumns>> = r
            .mesh
            .iter()
            .map(|m| m.as_ref().map(|f| conflictcore::resources::MeshColumns { report_note: f.describe(), ..Default::default() }))
            .collect();
        return Ok(serde_json::Value::String(conflictcore::resources::report(&r.list, &r.stats, &mesh, subset_only, limit)).to_string());
    }
    let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
    let c = slot.as_ref().ok_or("scan first")?;
    if c.mode != "conflicts" {
        return Err("the report is of conflicts: scan for conflicts first".into());
    }
    Ok(serde_json::Value::String(conflictcore::records::report(&c.list, &c.stats, subset_only, limit)).to_string())
}

/// Wraithguard: the last scan's CSV (`write_conflict_csv` / `write_resource_csv`) written to
/// `path`.
#[tauri::command(async)]
pub(super) fn conflicts_csv(kind: String, path: String) -> Result<(), String> {
    let text = if kind == "files" {
        let slot = RESOURCES.lock().unwrap_or_else(|p| p.into_inner());
        let r = slot.as_ref().ok_or("scan the files first")?;
        let mesh: Vec<Option<conflictcore::resources::MeshColumns>> = r
            .mesh
            .iter()
            .map(|m| {
                m.as_ref().map(|f| {
                    let (note, lost_collision, lost_animation, triangle_ratio) = f.columns();
                    conflictcore::resources::MeshColumns { note, lost_collision, lost_animation, triangle_ratio, report_note: String::new() }
                })
            })
            .collect();
        conflictcore::resources::csv(&r.list, &mesh)
    } else {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        conflictcore::records::conflict_csv(&slot.as_ref().ok_or("scan first")?.list)
    };
    write_path_whole(&path, text.as_bytes())
}

/// Wraithguard: queue a decision for the patch in the shared journal (`queue_whole_record`,
/// `queue_field`): `op` `record` (the record from `plugin`), `field` (`field` from `plugin`)
/// or `value` (`field` set to `value`, JSON). Answers how many records the queue holds.
#[tauri::command(async)]
pub(super) fn conflicts_patch(journal: String, op: String, rtype: String, id: String, plugin: String, field: String, value: String) -> Result<usize, String> {
    use editcore::queue::{Choice, PatchQueue, Selection};
    if journal.is_empty() {
        return Err("no patch journal: open this from Wraithguard".into());
    }
    let path = std::path::Path::new(&journal);
    let mut q = PatchQueue::new();
    q.restore(path);
    match op.as_str() {
        "record" => q.add_whole(Selection { plugin, record_type: rtype, key: id }),
        "field" => q.add_field(&rtype, &id, Choice::Plugin { path: field, plugin }),
        "value" => {
            conflictcore::patch::can_define(&field).map_err(|r| r.message())?;
            let value: serde_json::Value = serde_json::from_str(&value).map_err(|e| format!("the value did not read: {e}"))?;
            q.add_field(&rtype, &id, Choice::Value { path: field, value });
        }
        _ => return Err(format!("no patch step {op:?}")),
    }
    q.save(path).map_err(|e| format!("the journal could not be written: {e}"))?;
    Ok(q.len())
}

/* ---- the plugin view (Wraithguard's `pluginview.py`; port step 4.4) ----------------------

   The last record scan as plugin -> record type -> records (`conflicts_by_plugin`), a type
   judged for one plugin on demand (`conflicts_plugin_group`), and a plugin's fields taken
   across every record it shares (`conflicts_plugin_bulk`, `_apply`). */

/// The scan's records each plugin of `plugins` defines, read once per plugin file: `(plugin,
/// type, id)` to the record.
pub(super) fn cf_read_many(
    setup: &viewcore::ordercore::cfg::OpenmwCfg,
    list: &[&conflictcore::records::Conflict],
) -> std::collections::HashMap<(String, String, String), serde_json::Value> {
    cf_read_many_at(setup, list).0
}

/// `cf_read_many`, and the files it read, by plugin.
fn cf_read_many_at(
    setup: &viewcore::ordercore::cfg::OpenmwCfg,
    list: &[&conflictcore::records::Conflict],
) -> (std::collections::HashMap<(String, String, String), serde_json::Value>, std::collections::HashMap<String, PathBuf>) {
    use std::collections::{HashMap, HashSet};
    let mut wanted: Vec<(String, HashSet<(String, String)>)> = Vec::new();
    for c in list {
        for p in &c.plugins {
            let key = (c.rtype.clone(), c.id.clone());
            match wanted.iter_mut().find(|(n, _)| n == p) {
                Some((_, w)) => {
                    w.insert(key);
                }
                None => wanted.push((p.clone(), std::iter::once(key).collect())),
            }
        }
    }
    let names: Vec<String> = wanted.iter().map(|(n, _)| n.clone()).collect();
    let at: HashMap<String, PathBuf> = setup.locate(&names).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let read = |(name, keys): &(String, HashSet<(String, String)>)| -> Vec<((String, String, String), serde_json::Value)> {
        let Some(bytes) = at.get(name).and_then(|p| std::fs::read(p).ok()) else { return Vec::new() };
        conflictcore::fields::records_of(&bytes, keys)
            .map(|m| m.into_iter().map(|((t, i), v)| ((name.clone(), t, i), v)).collect())
            .unwrap_or_default()
    };
    let threads = scan_budget();
    let chunk = wanted.len().div_ceil(threads).max(1);
    let read = std::thread::scope(|s| {
        let jobs: Vec<_> = wanted.chunks(chunk).map(|part| s.spawn(move || part.iter().flat_map(read).collect::<Vec<_>>())).collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    });
    (read, at)
}

/// One record of the scan judged across its plugins (`judge`): its `ConflictAll` code, and
/// each plugin's `ConflictThis` in the record's plugin order.
type Verdict = (u8, Vec<conflictcore::fields::ConflictThis>);

/// The verdicts judged of the last record scan (Static 2026-11-18): `conflicts_verdicts` and
/// `conflicts_plugin_group` (Judge all calls it once per plugin) used to read every plugin a
/// record has again on each call, so a plugin was read once for each plugin it shares a record
/// with. A record is judged once now and kept, by its place in the scan's list, until another
/// scan or until a plugin it was judged from changes on disk (size or date; checked on each use).
struct Judged {
    scan: u64,
    stamps: std::collections::HashMap<PathBuf, (u64, Option<std::time::SystemTime>)>,
    per: Vec<Option<Verdict>>,
}

static JUDGED: Mutex<Option<Judged>> = Mutex::new(None);

fn stamp(p: &std::path::Path) -> (u64, Option<std::time::SystemTime>) {
    std::fs::metadata(p).map(|m| (m.len(), m.modified().ok())).unwrap_or((u64::MAX, None))
}

/// The verdicts of `list`'s records at `wanted` (scan `scan`), judging the ones not kept yet.
fn verdicts_of(setup: &viewcore::ordercore::cfg::OpenmwCfg, scan: u64, list: &[conflictcore::records::Conflict], wanted: &[usize]) -> Vec<Verdict> {
    use conflictcore::summary::judge;
    let mut slot = JUDGED.lock().unwrap_or_else(|p| p.into_inner());
    let fresh = slot.as_ref().is_some_and(|j| j.scan == scan && j.per.len() == list.len() && j.stamps.iter().all(|(p, s)| stamp(p) == *s));
    if !fresh {
        *slot = Some(Judged { scan, stamps: std::collections::HashMap::new(), per: vec![None; list.len()] });
    }
    let j = slot.as_mut().expect("just set");
    let missing: Vec<usize> = wanted.iter().copied().filter(|&i| j.per[i].is_none()).collect();
    if !missing.is_empty() {
        let refs: Vec<&conflictcore::records::Conflict> = missing.iter().map(|&i| &list[i]).collect();
        let (read, files) = cf_read_many_at(setup, &refs);
        for p in files.into_values() {
            let s = stamp(&p);
            j.stamps.entry(p).or_insert(s);
        }
        for &i in &missing {
            let c = &list[i];
            let versions: Vec<(String, Option<&serde_json::Value>)> =
                c.plugins.iter().map(|p| (p.clone(), read.get(&(p.clone(), c.rtype.clone(), c.id.clone())))).collect();
            let (all, this) = judge(&versions);
            j.per[i] = Some((all.code(), this));
        }
    }
    wanted.iter().map(|&i| j.per[i].clone().expect("judged")).collect()
}

/// Wraithguard: the last record scan as plugin -> type (`group_by_plugin`), plugins in the
/// cfg's load order. Answers `[{plugin, records, mine, groups: [{type, count}]}]`.
#[tauri::command(async)]
pub(super) fn conflicts_by_plugin(cfg: String, subset: Vec<String>) -> Result<tauri::ipc::Response, String> {
    let setup = cf_setup(&cfg)?;
    let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
    let scan = slot.as_ref().ok_or("scan first")?;
    let mine: std::collections::HashSet<String> = subset.iter().map(|s| s.to_lowercase()).collect();
    /* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed):
       `branches` `suu` (plugin, records, one of the user's own 0/1) and `groups` `usu` (its
       row, record type, count). Read by `WgConflicts.readBranches` (60_wg_conflicts.js). */
    let branches = conflictcore::summary::group_by_plugin(&scan.list, &setup.plugins());
    let mut tb = Tables::new();
    tb.table("branches", "suu", branches.len());
    for b in &branches {
        tb.s(&b.plugin).u(b.records() as u64).u(mine.contains(&b.plugin.to_lowercase()) as u64);
    }
    tb.table("groups", "usu", branches.iter().map(|b| b.groups.len()).sum());
    for (row, b) in branches.iter().enumerate() {
        for (t, m) in &b.groups {
            tb.u(row as u64).s(t).u(m.len() as u64);
        }
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: one plugin's records of one type in the last scan (`rtype` `""`: every
/// type), each judged across its plugins (`_judge_group`, `record_status`,
/// `record_plugin_statuses`). Answers `{records: [{type, id, plugins, winner, all, this}],
/// tally: {status: count}, worst}` - `all` the record's `ConflictAll` (0-4), `this` and
/// `worst` the plugin's `ConflictThis` names.
#[tauri::command(async)]
pub(super) fn conflicts_plugin_group(cfg: String, plugin: String, rtype: String) -> Result<tauri::ipc::Response, String> {
    use conflictcore::summary::tally;
    let setup = cf_setup(&cfg)?;
    let (whole, scan_id) = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        (scan.list.clone(), scan.id)
    };
    let at_list: Vec<usize> =
        whole.iter().enumerate().filter(|(_, c)| (rtype.is_empty() || c.rtype == rtype) && c.plugins.contains(&plugin)).map(|(i, _)| i).collect();
    let verdicts = verdicts_of(&setup, scan_id, &whole, &at_list);
    let list: Vec<&conflictcore::records::Conflict> = at_list.iter().map(|&i| &whole[i]).collect();
    let mut statuses = Vec::new();
    let mut judged = Vec::new();
    for (c, (all, this)) in list.iter().zip(&verdicts) {
        let mine = c.plugins.iter().position(|p| *p == plugin).map(|i| this[i]).unwrap_or(conflictcore::fields::ConflictThis::Unknown);
        statuses.push(mine);
        judged.push((*all, mine.name()));
    }
    let mut counts = Vec::new();
    tally(statuses.iter().copied(), &mut counts);
    let worst = conflictcore::fields::worst_this(statuses.iter().copied());
    /* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed): `head`
       `s` (the worst), `plugins` `s` (each named once), `records` `ssuus` (type, id, winner,
       all's code, this plugin's status), `record_plugins` `uu` (its row, a plugin) and `tally`
       `su` (status, count, in order). Read by `WgConflicts.readGroup` (60_wg_conflicts.js). */
    let mut names: Vec<&str> = Vec::new();
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for c in list.iter().copied() {
        for n in c.plugins.iter().chain(std::iter::once(&c.winner)) {
            if !at.contains_key(n.as_str()) {
                at.insert(n.as_str(), names.len());
                names.push(n.as_str());
            }
        }
    }
    let mut tb = Tables::new();
    tb.table("head", "s", 1);
    tb.s(worst.name());
    tb.table("plugins", "s", names.len());
    for n in &names {
        tb.s(n);
    }
    tb.table("records", "ssuus", list.len());
    for (c, (all, this)) in list.iter().zip(&judged) {
        tb.s(&c.rtype).s(&c.id).u(at[c.winner.as_str()] as u64).u(u64::from(*all)).s(this);
    }
    tb.table("record_plugins", "uu", list.iter().map(|c| c.plugins.len()).sum());
    for (row, c) in list.iter().enumerate() {
        for n in &c.plugins {
            tb.u(row as u64).u(at[n.as_str()] as u64);
        }
    }
    tb.table("tally", "su", counts.len());
    for (s, n) in &counts {
        tb.s(s.name()).u(*n as u64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: every record of the last conflict scan judged (`judge`, as the Tk record list's
/// plugin summary coloured it): the `ConflictAll` code of each record in the scan's order
/// (0 unknown, 1 only one, 2 no conflict, 3 override, 4 conflict), one byte each - the answer
/// is the codes themselves (Static 2026-11-17: JSON only where it is needed).
#[tauri::command(async)]
pub(super) fn conflicts_verdicts(cfg: String) -> Result<tauri::ipc::Response, String> {
    let _gate = scan_gate();
    let setup = cf_setup(&cfg)?;
    let (list, scan_id) = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        (scan.list.clone(), scan.id)
    };
    let all: Vec<usize> = (0..list.len()).collect();
    let codes: Vec<u8> = verdicts_of(&setup, scan_id, &list, &all).into_iter().map(|v| v.0).collect();
    Ok(tauri::ipc::Response::new(codes))
}

/// The records `conflicts_plugin_bulk` read, kept for `_apply`: the plugin and its records.
pub(super) static BULK: Mutex<Option<(String, Vec<conflictcore::summary::BulkRecord>)>> = Mutex::new(None);

/// Wraithguard: Merge fields from a plugin (`_bulk_merge_selected_plugin`): reads every record
/// of the last scan the plugin shares with another, and answers the fields taking would
/// change, `[{path, count, value}]` (`count` the records it would change, `value` the
/// plugin's from the first of them).
#[tauri::command(async)]
pub(super) fn conflicts_plugin_bulk(cfg: String, plugin: String) -> Result<String, String> {
    use conflictcore::summary::{bulk_candidates, BulkRecord};
    let setup = cf_setup(&cfg)?;
    let list: Vec<conflictcore::records::Conflict> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        scan.list.iter().filter(|c| c.plugins.len() >= 2 && c.plugins.contains(&plugin)).cloned().collect()
    };
    if list.is_empty() {
        return Err(format!("{plugin} shares no record with another plugin, so there is nothing to take."));
    }
    let refs: Vec<&conflictcore::records::Conflict> = list.iter().collect();
    let mut read = cf_read_many(&setup, &refs);
    let records: Vec<BulkRecord> = list
        .iter()
        .map(|c| BulkRecord {
            rtype: c.rtype.clone(),
            key: c.id.clone(),
            plugins: c.plugins.clone(),
            values: c
                .plugins
                .iter()
                .filter_map(|p| read.remove(&(p.clone(), c.rtype.clone(), c.id.clone())).map(|v| (p.clone(), conflictcore::fields::flatten(&v))))
                .collect(),
        })
        .collect();
    let out: Vec<serde_json::Value> = bulk_candidates(&records, &plugin)
        .into_iter()
        .map(|(path, count, value)| serde_json::json!({"path": path, "count": count, "value": value}))
        .collect();
    *BULK.lock().unwrap_or_else(|p| p.into_inner()) = Some((plugin, records));
    Ok(serde_json::Value::Array(out).to_string())
}

/// Wraithguard: queue the chosen fields from the plugin `conflicts_plugin_bulk` read
/// (`apply_choices`, `bulk_field_choices`) into the patch journal; `dry` only counts. Answers
/// how many field takes that is.
#[tauri::command(async)]
pub(super) fn conflicts_plugin_bulk_apply(journal: String, plugin: String, paths: Vec<String>, dry: bool) -> Result<usize, String> {
    use editcore::queue::{Choice, PatchQueue};
    let slot = BULK.lock().unwrap_or_else(|p| p.into_inner());
    let (read_for, records) = slot.as_ref().ok_or("read the plugin first")?;
    if *read_for != plugin {
        return Err("read the plugin first".into());
    }
    let takes = conflictcore::summary::bulk_takes(records, &plugin, &paths);
    if dry {
        return Ok(takes.len());
    }
    if journal.is_empty() {
        return Err("no patch journal: open this from Wraithguard".into());
    }
    let path = std::path::Path::new(&journal);
    let mut q = PatchQueue::new();
    q.restore(path);
    for t in &takes {
        q.add_field(&t.rtype, &t.key, Choice::Plugin { path: t.path.clone(), plugin: plugin.clone() });
    }
    q.save(path).map_err(|e| format!("the journal could not be written: {e}"))?;
    Ok(takes.len())
}
