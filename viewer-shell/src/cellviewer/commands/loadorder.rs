// The Load order page's commands: the sort and the export, Update rules, Sources and the rule maker.
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- the Load order page (Wraithguard; port step 4.2, slice 2k) ---------------------

   Sort and Export, as Wraithguard's main window did them, answered by `ordercore::plan`: the
   page sends what the run reads (`loadorder_compute`), gets the plan's orders and the report
   as items it lists and filters, lets the person reorder and opt out, and exports
   (`loadorder_export`), which also writes the plan file the Tk tools still waiting for 4.3 /
   4.4 read their order from. The last plan is kept here between the two. */

pub(super) static LOADORDER: Mutex<Option<viewcore::ordercore::plan::Computed>> = Mutex::new(None);

pub(super) fn lo_items(report: &viewcore::ordercore::plan::Report) -> serde_json::Value {
    use viewcore::ordercore::plan::ItemKind;
    report
        .items
        .iter()
        .map(|i| {
            let kind = match i.kind {
                ItemKind::Section => "section",
                ItemKind::Subsection => "subsection",
                ItemKind::Line => "line",
                ItemKind::Log => "log",
                ItemKind::Scans => "scans",
                ItemKind::Preview => "preview",
            };
            serde_json::json!({"kind": kind, "text": i.text, "warning": i.warning, "names": i.names})
        })
        .collect()
}

/// Wraithguard: a whole sort (`ordercore::plan::compute_plan`). `args`: JSON with the fields of
/// `PlanArgs` in camelCase. Answers `{refused, report: [{kind, text, warning, names}], plan}`;
/// the plan is kept for `loadorder_export`.
#[tauri::command(async)]
pub(super) fn loadorder_compute(args: String) -> Result<String, String> {
    use viewcore::ordercore::plan::{compute_plan, PlanArgs, PlanError};
    let a: serde_json::Value = serde_json::from_str(&args).map_err(|e| format!("the sort's arguments did not read: {e}"))?;
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_string);
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let list = |k: &str| -> Vec<String> {
        a.get(k).and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    let args = PlanArgs {
        cfg: s("cfg").ok_or("no openmw.cfg given")?,
        customizations: s("customizations"),
        subset: list("subset"),
        subset_file: s("subsetFile"),
        subset_lines: a.get("subsetLines").filter(|v| v.is_array()).map(|_| list("subsetLines")),
        scan_dir: s("scanDir"),
        subset_from_cfg: b("subsetFromCfg"),
        allow_empty_sort: b("allowEmptySort"),
        sort_data_paths: b("sortDataPaths"),
        emit_toml: b("emitToml"),
        groundcover: list("groundcover"),
        plugin_order_yml: s("pluginOrderYml"),
        list_name: s("listName"),
        rules: list("rules"),
        no_predicate_warnings: b("noPredicateWarnings"),
        data_paths_cache: s("dataPathsCache"),
        verbose: b("verbose"),
    };
    match compute_plan(&args) {
        Err(PlanError::Unsupported) => {
            Err("this setup holds something the load order reader does not read (a file that is not UTF-8, or one that could not be opened)".into())
        }
        Err(PlanError::Refused { message, report }) => {
            Ok(serde_json::json!({"refused": message, "report": lo_items(&report), "plan": null}).to_string())
        }
        Ok(got) => {
            let p = &got.plan;
            let plan = serde_json::json!({
                "finalOrder": if got.sorted { serde_json::json!(p.final_order) } else { serde_json::Value::Null },
                "baseOrder": p.base_order_names,
                "subset": p.subset,
                "dataOrder": p.data_order,
                "dataResult": p.data_result.as_ref().map(|r| r.iter().map(|(l, n, v)| serde_json::json!([l, n, v])).collect::<Vec<_>>()),
                "curated": p.curated_set,
                "newGroundcover": p.new_groundcover,
                "orphanData": p.orphan_data_paths,
                "origins": got.subset_origins,
                "masterProblems": got.master_problem_plugins,
                "scanDirs": got.scan_dirs,
            });
            let out = serde_json::json!({"refused": null, "report": lo_items(&got.report), "plan": plan}).to_string();
            *LOADORDER.lock().unwrap_or_else(|p| p.into_inner()) = Some(got);
            Ok(out)
        }
    }
}

/// Wraithguard: export the last sort (`ordercore::plan::export`). `args`: JSON with `cfg`,
/// `writeCfg`, `emitToml`, `dryRun`, `noBackup`, `sortDataPaths`, `listName`, `stamp`,
/// `finalOrder` / `dataOrder` (the panels' enabled entries; null: the plan's),
/// `disabledPlugins`, `disabledData`, and `planFile` (where the Tk tools read the order from;
/// empty: not written). Answers `{report, wroteCfg, wroteToml, toml}`.
#[tauri::command(async)]
pub(super) fn loadorder_export(args: String) -> Result<String, String> {
    use viewcore::ordercore::plan::{export, Counts, ExportArgs};
    let a: serde_json::Value = serde_json::from_str(&args).map_err(|e| format!("the export's arguments did not read: {e}"))?;
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_string);
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let list = |k: &str| -> Option<Vec<String>> {
        a.get(k).and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
    };
    let slot = LOADORDER.lock().unwrap_or_else(|p| p.into_inner());
    let got = slot.as_ref().ok_or("sort first: there is no plan to export")?;
    let ex = ExportArgs {
        write_cfg: b("writeCfg"),
        cfg: s("cfg").ok_or("no openmw.cfg given")?,
        emit_toml: s("emitToml"),
        dry_run: b("dryRun"),
        no_backup: b("noBackup"),
        sort_data_paths: b("sortDataPaths"),
        list_name: s("listName"),
        stamp: s("stamp").unwrap_or_else(|| "backup".into()),
        toml_line_ending: None,
    };
    let counts = Counts { rule_warnings: got.predicate_warnings.len(), yml_warnings: got.yml_warnings.len() };
    let final_order = list("finalOrder");
    let data_order = list("dataOrder");
    let done = export(
        &ex,
        &got.plan,
        counts,
        final_order.as_deref(),
        data_order.as_deref(),
        &list("disabledPlugins").unwrap_or_default(),
        &list("disabledData").unwrap_or_default(),
    )
    .map_err(|e| match e {
        viewcore::ordercore::export::ExportError::Unsupported => "the export holds something the writer does not write".to_string(),
        viewcore::ordercore::export::ExportError::Io(e) => format!("the export could not write: {e}"),
    })?;
    if let Some(path) = s("planFile") {
        let order = final_order.clone().unwrap_or_else(|| got.plan.final_order.clone());
        let data = data_order.clone().unwrap_or_else(|| {
            got.plan.data_result.as_ref().map(|r| r.iter().map(|(l, _, _)| l.clone()).collect()).unwrap_or_else(|| got.plan.data_order.clone())
        });
        let file = serde_json::json!({
            "version": 1,
            "cfg": ex.cfg,
            "finalOrder": order,
            "dataLines": data,
            "subset": got.plan.subset,
            "scanDirs": got.scan_dirs,
            "customDataDirs": got.custom_data_dirs,
            "newDataLines": got.plan.data_result.iter().flatten().filter(|(_, n, _)| *n).map(|(l, _, _)| l.clone()).collect::<Vec<_>>(),
            "masterProblems": got.master_problem_plugins,
            "orphanData": got.plan.orphan_data_paths,
            "newGroundcover": got.plan.new_groundcover,
        });
        write_path_whole(&path, file.to_string().as_bytes())?;
    }
    Ok(serde_json::json!({
        "report": lo_items(&done.report),
        "wroteCfg": done.written.wrote_cfg,
        "wroteToml": done.written.wrote_toml,
        "toml": done.written.toml_text,
    })
    .to_string())
}

/* ---- the Load order page: Update rules, Sources and the rule maker ------------------

   The downloads (`ordercore::updates`, as Wraithguard's Update rules and Sources do them) over
   `ureq`: HTTPS in plain Rust, the size capped as the Python's `_get` caps it; and the rule
   maker (`ordercore::authoring`) on its plain JSON form. */

/// The network for `ordercore::updates`: one GET, at most `max_bytes` of the body.
pub(super) struct Net;

impl viewcore::ordercore::updates::Transport for Net {
    fn get(&self, url: &str, timeout_secs: u64, max_bytes: usize) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(timeout_secs)))
            .build()
            .into();
        let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(max_bytes as u64)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// How long a download may take, as the Python's default.
pub(super) const LO_TIMEOUT: u64 = 30;

pub(super) fn lo_env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

pub(super) fn lo_update_err(e: viewcore::ordercore::updates::UpdateError) -> String {
    match e {
        viewcore::ordercore::updates::UpdateError::Unsupported => "the download holds something the updater does not read".into(),
        viewcore::ordercore::updates::UpdateError::Io(e) => format!("could not write the download: {e}"),
    }
}

/// Runs `f` with the update flows' outside world: the `.bak` stamp and the three variables.
pub(super) fn lo_with_env<T>(stamp: &str, f: impl FnOnce(&viewcore::ordercore::updates::Env<'_>) -> T) -> T {
    let (po, rt, host) = (lo_env_var("MLOX_PLUGIN_ORDER_URL"), lo_env_var("MLOX_RULES_URL_TEMPLATE"), lo_env_var("MOMW_API_HOST"));
    let env = viewcore::ordercore::updates::Env {
        stamp,
        plugin_order_url: po.as_deref(),
        rules_url_template: rt.as_deref(),
        momw_api_host: host.as_deref(),
    };
    f(&env)
}

/// Wraithguard: Update rules - each managed rule file downloaded and replaced (a `.bak` of the
/// old). `rules`: the rule files and folders; `stamp`: the person's clock. The report's lines.
#[tauri::command(async)]
pub(super) fn loadorder_update_rules(rules: Vec<String>, stamp: String) -> Result<String, String> {
    let paths: Vec<PathBuf> = rules.iter().map(PathBuf::from).collect();
    let template = lo_env_var("MLOX_RULES_URL_TEMPLATE");
    let lines = lo_with_env(&stamp, |env| viewcore::ordercore::updates::update_rule_files(&paths, template.as_deref(), LO_TIMEOUT, env, &Net))
        .map_err(lo_update_err)?;
    Ok(serde_json::Value::from(lines).to_string())
}

/// Wraithguard: Sources - MOMW's plugin-order.yml downloaded to `path` (checked as MOMW's before
/// it replaces the file). The report's lines.
#[tauri::command(async)]
pub(super) fn loadorder_update_yml(path: String, stamp: String) -> Result<String, String> {
    let count = |data: &[u8]| viewcore::ordercore::momw::parse_bytes(data).map(|e| e.len()).map_err(|e| e.0);
    let lines = lo_with_env(&stamp, |env| {
        viewcore::ordercore::updates::update_plugin_order_yml(std::path::Path::new(&path), None, LO_TIMEOUT, env, &Net, &count)
    })
    .map_err(lo_update_err)?;
    Ok(serde_json::Value::from(lines).to_string())
}

/// Wraithguard: Sources - a curated list's data paths, from MOMW's cfg generator, cached at
/// `path`. The report's lines.
#[tauri::command(async)]
pub(super) fn loadorder_fetch_data_paths(path: String, list_name: String, stamp: String) -> Result<String, String> {
    let host = lo_env_var("MOMW_API_HOST");
    let lines = lo_with_env(&stamp, |env| {
        viewcore::ordercore::updates::fetch_list_data_paths(std::path::Path::new(&path), &list_name, host.as_deref(), LO_TIMEOUT, env, &Net)
    })
    .map_err(lo_update_err)?;
    Ok(serde_json::Value::from(lines).to_string())
}

/// Wraithguard: the rule maker. `rule`: the plain JSON form (`kind`, `plugins`, `expressions`,
/// `message`, `ref`, ...). `op`: `check` - `{text, problems: [[severity, message, remedy]]}`;
/// `append` - written to the rule file `path`, `{written}`, or the problems as the error.
#[tauri::command(async)]
pub(super) fn loadorder_author(op: String, rule: String, path: Option<String>) -> Result<String, String> {
    use viewcore::ordercore::authoring;
    let v: serde_json::Value = serde_json::from_str(&rule).map_err(|e| format!("the rule did not read: {e}"))?;
    let mut rule = authoring::rule_from_json(&v).ok_or("the rule is not in the rule maker's form")?;
    // The page's warning rules come as rule text, one expression a line (`[DESC]`, `[SIZE]`,
    // `[VER]`, `[ANY]`/`[ALL]`/`[NOT]` groups or a plain name), read here as the sort reads them.
    if let Some(lines) = v.get("lines").and_then(|l| l.as_array()) {
        rule.expressions = lines
            .iter()
            .filter_map(|l| l.as_str())
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(authoring::expr_from_text)
            .collect::<Result<_, _>>()?;
    }
    let declined = |_| "the rule holds something the rule maker does not write".to_string();
    match op.as_str() {
        "check" => {
            let problems = authoring::validate(&rule).map_err(declined)?;
            let text = authoring::render_rule(&rule).unwrap_or_default();
            let problems: Vec<serde_json::Value> =
                problems.into_iter().map(|p| serde_json::json!([p.severity.as_str(), p.message, p.remedy])).collect();
            Ok(serde_json::json!({"text": text, "problems": problems}).to_string())
        }
        "append" => {
            let path = path.filter(|p| !p.is_empty()).ok_or("no rule file to write to")?;
            let written = authoring::append_authored_rule(
                std::path::Path::new(&path),
                &rule,
                viewcore::ordercore::export::LineEnding::platform(),
            )
            .map_err(|e| match e {
                authoring::AppendError::Refused(m) => m,
                authoring::AppendError::Unsupported => "the rule file holds something the rule maker does not write".into(),
                authoring::AppendError::Io(e) => format!("could not write the rule file: {e}"),
            })?;
            Ok(serde_json::json!({"written": written}).to_string())
        }
        other => Err(format!("no such rule maker step: {other}")),
    }
}

/* ---- the plain sort: an MO2 or Morrowind.ini install (TODO.md, Next, 3) ------------------

   The mlox rules over the whole load order, no MOMW list: `ordercore::plain`. The page sends
   the setup's name and its rule files; it gets the order back to look over, then writes it. */

fn plain_index(c: &viewcore::ordercore::cfg::OpenmwCfg) -> viewcore::ordercore::inputs::PluginIndex {
    let dirs: Vec<String> = c.data.iter().map(|d| d.to_string_lossy().into_owned()).collect();
    viewcore::ordercore::inputs::PluginIndex::build(&dirs)
}

/// Wraithguard: the plain mlox sort of a load order, no MOMW list (`cfg`: the setup's name -
/// an MO2 or Morrowind.ini install, or an `openmw.cfg` when the page's "Plain mlox" is ticked;
/// `rules`: the rule files, lowest priority first). An `openmw.cfg`'s data folders come too,
/// in the order they are (`sortData`: in the order their plugins load). Answers `{kind, was,
/// order, moved: [{name, from, to}], conflicts: [[before, after]], notes, data, dataMoved}`.
#[tauri::command(async)]
pub(super) fn loadorder_plain_sort(cfg: String, rules: Vec<String>, sort_data: bool) -> Result<String, String> {
    let (kind, _, _) = setup_spec(&cfg);
    let c = cf_setup(&cfg)?;
    let index = plain_index(&c);
    let find = |n: &str| index.find(n).map(std::path::Path::to_path_buf);
    let got = viewcore::ordercore::plain::sort(&c.content, &rules, &find)?;
    let moved: Vec<serde_json::Value> = got.moved.iter().map(|(n, f, t)| serde_json::json!({"name": n, "from": f, "to": t})).collect();
    let (data, data_moved) = if kind != "openmw" {
        (Vec::new(), Vec::new())
    } else if sort_data {
        viewcore::ordercore::plain::data_order(&c.data, &got.order, &find)
    } else {
        (c.data.clone(), Vec::new())
    };
    let data: Vec<String> = data.iter().map(|d| d.to_string_lossy().into_owned()).collect();
    let data_moved: Vec<serde_json::Value> = data_moved.iter().map(|(n, f, t)| serde_json::json!({"name": n, "from": f, "to": t})).collect();
    Ok(serde_json::json!({"kind": kind, "was": got.was, "order": got.order, "moved": moved, "conflicts": got.conflicts, "notes": got.notes,
        "data": data, "dataMoved": data_moved}).to_string())
}

/// Wraithguard: writes `order` (every plugin of the load order, `off` the ones switched off)
/// into the install (`cfg`: the setup's name): an `openmw.cfg`'s `content=` lines (`off` as
/// `#content=`), its `data=` lines in the order of `data` when given (`dataOff` as `#data=`),
/// and the customizations TOML when `tomlTo` is named (from `tomlFrom`, `listName`; MOMW's
/// configurator then builds the same order); a Morrowind.ini's `[Game Files]` (`off` left
/// out), an MO2 profile's `loadorder.txt` and `plugins.txt` (`off` switched off), a
/// Morrowind.ini install's plugin dates. Each file copied first (`stamp` names the copies);
/// `dryRun`: nothing written; `writeCfg` false: the cfg not written (the TOML may be).
/// Answers `{lines}`, what was (or would be) done.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub(super) fn loadorder_plain_write(
    cfg: String,
    order: Vec<String>,
    off: Vec<String>,
    data: Option<Vec<String>>,
    data_off: Option<Vec<String>>,
    toml_from: Option<String>,
    toml_to: Option<String>,
    list_name: Option<String>,
    write_cfg: Option<bool>,
    dry_run: bool,
    stamp: String,
) -> Result<String, String> {
    use std::path::{Path, PathBuf};
    let (kind, _, _) = setup_spec(&cfg);
    let c = cf_setup(&cfg)?;
    // Only the plugins the install loads, each once: the order is the page's, after the person's moves.
    let have: std::collections::HashSet<String> = c.content.iter().map(|n| n.to_lowercase()).collect();
    let mut seen = std::collections::HashSet::new();
    let order: Vec<String> = order.into_iter().filter(|n| have.contains(&n.to_lowercase()) && seen.insert(n.to_lowercase())).collect();
    if order.len() != have.len() {
        return Err(format!("the order names {} of the install's {} plugins: sort again", order.len(), have.len()));
    }
    let stamp = if stamp.trim().is_empty() { "backup".to_string() } else { stamp };
    let index = plain_index(&c);
    let find = |n: &str| index.find(n).map(Path::to_path_buf);
    let lines = if kind == "openmw" {
        let file = viewcore::ordercore::plain::cfg_holding_content(&c.files)?;
        let data: Option<Vec<PathBuf>> = data.map(|d| d.into_iter().map(PathBuf::from).collect());
        let data_off: Vec<PathBuf> = data_off.unwrap_or_default().into_iter().map(PathBuf::from).collect();
        // The cfg is written only when asked (the page's Write openmw.cfg); the TOML when named.
        let cfg_dry = dry_run || !write_cfg.unwrap_or(true);
        let mut lines = viewcore::ordercore::plain::write_cfg(&file, &order, &off, data.as_deref().map(|d| (d, data_off.as_slice())), cfg_dry, &stamp)?;
        let to = toml_to.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        if let Some(to) = to {
            let from = toml_from.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
            let list = list_name.as_deref().map(str::trim).filter(|l| !l.is_empty());
            lines.extend(viewcore::ordercore::plain::write_toml(from.as_deref().map(Path::new), Path::new(&to), &c.content, &order, &off, list, dry_run, &stamp)?);
        }
        lines
    } else {
        let ini = c.files.first().cloned().ok_or("the install has no Morrowind.ini")?;
        let profile = if kind == "mo2" { ini.parent().map(Path::to_path_buf) } else { None };
        viewcore::ordercore::plain::write(&ini, &order, &off, profile.as_deref(), &find, dry_run, &stamp)?
    };
    Ok(serde_json::json!({"lines": lines}).to_string())
}

/// Wraithguard: puts back the plugin dates the last Export set on a Morrowind.ini install
/// (`cfg`: the setup's name), from the `Morrowind.ini.dates-<stamp>.json` it kept. `dryRun`:
/// nothing set. Answers `{lines}`.
#[tauri::command(async)]
pub(super) fn loadorder_plain_restore_dates(cfg: String, dry_run: bool) -> Result<String, String> {
    let (kind, _, _) = setup_spec(&cfg);
    if kind != "vanilla" {
        return Err("only the game's own install (a Morrowind.ini) has its plugin dates set".into());
    }
    let c = cf_setup(&cfg)?;
    let ini = c.files.first().cloned().ok_or("the install has no Morrowind.ini")?;
    let lines = viewcore::ordercore::plain::restore_dates(&ini, dry_run)?;
    Ok(serde_json::json!({"lines": lines}).to_string())
}
