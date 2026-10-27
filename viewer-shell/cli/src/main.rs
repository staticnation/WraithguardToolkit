//! `wraithguard`: the command line of `wraithguard_toolkit.py`, in Rust (port step 4.5, slice 1).
//!
//! Sort a load order with the mlox rules (and MOMW's `plugin-order.yml`), write `openmw.cfg` and
//! a `momw-customizations.toml`, and, read-only, scan the result: the record conflicts, the cell
//! map, the lint and the loose-file conflicts. The same flags, the same report on stdout, the
//! same warnings on stderr, the same files; `tests/cli_cases.rs` holds it to the Python's
//! answers (`tests/fixtures/cli_cases.json`).
//!
//! The work is the crates': [`ordercore::plan`] sorts and writes, [`conflictcore`] scans the
//! records and the loose files, [`toolscore::lint`] lints, and [`cellmap`] draws the map. This
//! file is the Python's `run_from_args`: the arguments read, the report printed as it comes,
//! the scans run where the sort's report places them.
//!
//! Not carried over: `--tes3conv` and `--json-dump-dir`, which only the Python's fallback
//! reader used (the reader is built in); they are accepted and said to be unused. The trace
//! (`--trace`) holds the run's steps, not the Python's every line.

mod cellmap;
mod pypath;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::process::ExitCode;

use clap::{Arg, ArgAction, ArgMatches, Command};
use conflictcore::records;
use ordercore::inputs::PluginIndex;
use ordercore::plan::{self, Computed, Counts, ExportArgs, ItemKind, PlanArgs, PlanError, Report};

use pypath::py_path;

/// The run's clock: local time, or `WRAITHGUARD_CLI_NOW` (`YYYY-MM-DDTHH:MM:SS`, for the tests).
fn now() -> chrono::NaiveDateTime {
    std::env::var("WRAITHGUARD_CLI_NOW")
        .ok()
        .and_then(|v| chrono::NaiveDateTime::parse_from_str(&v, "%Y-%m-%dT%H:%M:%S").ok())
        .unwrap_or_else(|| chrono::Local::now().naive_local())
}

// ---------------------------------------------------------------- the arguments

fn command() -> Command {
    let path = |name: &'static str, help: &'static str| Arg::new(name).long(name).value_name("PATH").help(help);
    let flag = |name: &'static str, help: &'static str| Arg::new(name).long(name).action(ArgAction::SetTrue).help(help);
    Command::new("wraithguard")
        .about(
            "Sort an OpenMW load order with the mlox rules and write openmw.cfg / momw-customizations.toml; \
             optionally scan it for record and loose-file conflicts, lint it, and draw a cell map.",
        )
        .args_override_self(true)
        .arg(path("cfg", "Path to openmw.cfg").required(true))
        .arg(
            Arg::new("rules")
                .long("rules")
                .required(true)
                .num_args(1..)
                .value_name("RULES")
                .help("mlox rule file(s) or directories, in increasing priority order (mlox_base.txt first, mlox_user.txt last)"),
        )
        .arg(path("customizations", "momw-customizations.toml to auto-derive the subset from"))
        .arg(Arg::new("subset").long("subset").num_args(0..).value_name("SUBSET").help("Explicit plugin filenames to sort"))
        .arg(
            Arg::new("groundcover")
                .long("groundcover")
                .num_args(0..)
                .value_name("PLUGIN")
                .help("Plugin filenames to declare as grass (written as groundcover= instead of content=)"),
        )
        .arg(path("subset-file", "Plain text (one plugin per line) or minimal TOML (subset = [...]) listing plugins to sort"))
        .arg(path("scan-dir", "Scan this mods folder for data folders and plugins, write the result to --subset-file, then sort it"))
        .arg(flag("subset-from-cfg", "Sort the cfg's own unmanaged (orphan) content= and data= entries"))
        .arg(flag("dry-run", "Print the plan, write nothing"))
        .arg(flag("no-backup", "Skip the .bak-<timestamp> copy before overwriting openmw.cfg or the --emit-toml target"))
        .arg(path("emit-toml", "Write a momw-customizations.toml here with the sorted inserts"))
        .arg(Arg::new("list-name").long("list-name").value_name("LIST_NAME").help("listName for the emitted TOML"))
        .arg(path("plugin-order-yml", "MOMW's plugin-order.yml: the curated list's plugins, kept out of the sort"))
        .arg(path("data-paths-cache", "A cached MOMW list data-path order (one path tail per line)"))
        .arg(flag("write-cfg", "Patch openmw.cfg in place (a .bak copy first unless --no-backup)"))
        .arg(flag("sort-data-paths", "Also position the data= folders the customizations insert"))
        .arg(flag("no-predicate-warnings", "Skip the [Requires]/[Conflict]/[Note] warnings"))
        .arg(flag("check-conflicts", "After sorting, scan the active plugins for record-level conflicts"))
        .arg(path("conflicts-out", "Write the full conflict list to this CSV (with --check-conflicts)"))
        .arg(path("tes3conv", "Unused: plugins are read by the built-in reader"))
        .arg(path("cell-map", "Write a self-contained HTML cell map here: which mods touch which cells"))
        .arg(flag("resource-conflicts", "Scan the data= folders for loose-file (VFS) conflicts"))
        .arg(flag("lint", "After sorting, run tes3lint-style checks over the active plugins"))
        .arg(path("resources-out", "Write the full resource-conflict list to this CSV (with --resource-conflicts)"))
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .num_args(0..)
                .value_name("EXCLUDE")
                .help("Name patterns (glob) to skip in the conflict, cell-map and lint scans"),
        )
        .arg(flag("conflicts-subset-only", "With --check-conflicts, report only conflicts involving your custom mods"))
        .arg(
            Arg::new("trace")
                .long("trace")
                .num_args(0..=1)
                .default_missing_value("wraithguard_toolkit_trace.log")
                .value_name("LOGFILE")
                .help("Write a trace log of the run (default file wraithguard_toolkit_trace.log)"),
        )
        .arg(path("json-dump-dir", "Unused: only the Python's tes3conv fallback wrote JSON"))
        .arg(Arg::new("verbose").short('v').long("verbose").action(ArgAction::Count).help("Show run diagnostics on stderr (-v, -vv)"))
}

/// What a run is asked, read off the command line.
struct Args {
    plan: PlanArgs,
    cfg: String,
    dry_run: bool,
    no_backup: bool,
    emit_toml: Option<String>,
    write_cfg: bool,
    check_conflicts: bool,
    conflicts_out: Option<String>,
    cell_map: Option<String>,
    resource_conflicts: bool,
    resources_out: Option<String>,
    lint: bool,
    exclude: Vec<String>,
    conflicts_subset_only: bool,
    trace: Option<String>,
    unused: Vec<&'static str>,
    verbose: u8,
}

impl Args {
    fn read(m: &ArgMatches) -> Args {
        let path = |k: &str| m.get_one::<String>(k).map(|v| py_path(v));
        let list = |k: &str| m.get_many::<String>(k).map(|v| v.cloned().collect()).unwrap_or_default();
        let on = |k: &str| m.get_flag(k);
        let cfg = path("cfg").unwrap_or_default();
        let verbose = m.get_count("verbose");
        let mut unused = Vec::new();
        for k in ["tes3conv", "json-dump-dir"] {
            if m.contains_id(k) && m.get_one::<String>(k).is_some() {
                unused.push(k);
            }
        }
        Args {
            plan: PlanArgs {
                cfg: cfg.clone(),
                customizations: path("customizations"),
                subset: list("subset"),
                subset_file: path("subset-file"),
                subset_lines: None,
                scan_dir: path("scan-dir"),
                subset_from_cfg: on("subset-from-cfg"),
                allow_empty_sort: false,
                sort_data_paths: on("sort-data-paths"),
                emit_toml: m.get_one::<String>("emit-toml").is_some_and(|v| !v.is_empty()),
                groundcover: list("groundcover"),
                plugin_order_yml: path("plugin-order-yml"),
                list_name: m.get_one::<String>("list-name").filter(|v| !v.is_empty()).cloned(),
                rules: list("rules").iter().map(|r| py_path(r)).collect(),
                no_predicate_warnings: on("no-predicate-warnings"),
                data_paths_cache: path("data-paths-cache"),
                verbose: verbose > 0,
            },
            cfg,
            dry_run: on("dry-run"),
            no_backup: on("no-backup"),
            emit_toml: path("emit-toml"),
            write_cfg: on("write-cfg"),
            check_conflicts: on("check-conflicts"),
            conflicts_out: path("conflicts-out"),
            cell_map: path("cell-map"),
            resource_conflicts: on("resource-conflicts"),
            resources_out: path("resources-out"),
            lint: on("lint"),
            exclude: list("exclude"),
            conflicts_subset_only: on("conflicts-subset-only"),
            trace: m.get_one::<String>("trace").cloned(),
            unused,
            verbose,
        }
    }
}

// ---------------------------------------------------------------- printing

/// stderr as the Python's logging writes it (`%(levelname)-8s %(message)s`), at the level `-v`
/// sets (warnings always, `-v` information, `-vv` detail).
struct Log {
    verbose: u8,
}

impl Log {
    fn emit(&self, level: &str, needs: u8, text: &str) {
        if self.verbose >= needs {
            let _ = writeln!(std::io::stderr(), "{level:<8} {text}");
        }
    }
    fn warning(&self, text: &str) {
        self.emit("WARNING", 0, text);
    }
    fn error(&self, text: &str) {
        self.emit("ERROR", 0, text);
    }
    fn debug(&self, text: &str) {
        self.emit("DEBUG", 2, text);
    }
}

/// The trace file (`--trace`): timestamped lines, each flushed; never the thing that breaks a run.
struct Trace(Option<std::fs::File>);

impl Trace {
    fn open(path: Option<&str>) -> Trace {
        let t = Trace(path.and_then(|p| std::fs::File::create(p).ok()));
        t.line("=== trace start ===");
        t
    }
    fn line(&self, message: &str) {
        if let Some(mut f) = self.0.as_ref() {
            let _ = writeln!(f, "{}  {message}", now().format("%Y-%m-%d %H:%M:%S"));
            let _ = f.flush();
        }
    }
}

fn section(title: &str) {
    let rule = "=".repeat(70);
    println!("\n{rule}\n {title}\n{rule}");
}

/// Print a report's items; at its scans place, run `scans`.
fn say(report: &Report, log: &Log, mut scans: impl FnMut()) {
    for item in &report.items {
        match item.kind {
            ItemKind::Section => section(&item.text),
            ItemKind::Subsection => println!("\n--- {} ---", item.text),
            ItemKind::Line | ItemKind::Preview => println!("{}", item.text),
            ItemKind::Log => log.warning(&item.text),
            ItemKind::Scans => scans(),
        }
    }
}

fn excluded_line(count: usize) {
    if count > 0 {
        println!("  (excluded {count} {} by --exclude)", if count == 1 { "plugin" } else { "plugins" });
    }
}

/// A text file written as Python's `write_text` writes it: `\n` as the platform's line end.
fn write_text(path: &str, text: &str) -> std::io::Result<()> {
    if cfg!(windows) { std::fs::write(path, text.replace('\n', "\r\n")) } else { std::fs::write(path, text) }
}

/// What an `OSError` says, as near as Rust's error gives it.
fn os_error(e: &std::io::Error, path: &str) -> String {
    match e.raw_os_error() {
        Some(code) => format!("[Errno {code}] {e}: '{path}'"),
        None => e.to_string(),
    }
}

// ---------------------------------------------------------------- the scans

/// The record-conflict and cell-map scans, the lint and the loose-file scan, as the Python runs
/// them at the report's scans place.
fn scans(args: &Args, got: &Computed, log: &Log, trace: &Trace) {
    let subset = &got.plan.subset;
    let active: &[String] =
        if got.sorted && !got.plan.final_order.is_empty() { &got.plan.final_order } else { &got.plan.base_order_names };
    let dirs = &got.scan_dirs;
    if args.check_conflicts || args.cell_map.is_some() {
        let (order, excluded) = records::filter_plugins(active, &args.exclude);
        excluded_line(excluded.len());
        let index = PluginIndex::build(dirs);
        let find = records::in_index(&index);
        if args.check_conflicts {
            section("TES3 RECORD CONFLICTS (read-only)");
            println!("  Engine: native esp reader (field-level)");
            let scan = records::scan_touch(&order, &find, &records::plugin_keys);
            let (list, stats) = records::conflicts(&scan, subset);
            trace.line(&format!("conflicts: {} of {} records", list.len(), stats.records));
            println!("{}", records::report(&list, &stats, args.conflicts_subset_only, 200));
            if let Some(out) = args.conflicts_out.as_deref().filter(|_| !list.is_empty()) {
                // written as the Python writes it (newline=""): the CSV's own line ends
                match std::fs::write(out, records::conflict_csv(&list)) {
                    Ok(()) => println!("\n  Wrote conflict report: {out}"),
                    Err(e) => log.error(&format!("could not write conflict CSV: {}", os_error(&e, out))),
                }
            }
        }
        if let Some(out) = args.cell_map.as_deref() {
            section("CELL MAP (which mods touch which cells)");
            let cov = cellmap::coverage(&order, &find, subset);
            let html = cellmap::html(&cov, cellmap::TITLE, now());
            trace.line(&format!("cell map: rendered {} bytes", html.len()));
            match write_text(out, &html) {
                Ok(()) => {
                    println!(
                        "  Scanned {} plugin(s): {} exterior + {} interior cell(s) touched.",
                        cov.scanned,
                        cov.exterior.len(),
                        cov.interior.len()
                    );
                    println!("  Wrote cell map: {out}  (open it in a browser)");
                }
                Err(e) => log.error(&format!("could not write cell map: {}", os_error(&e, out))),
            }
        }
    }
    if args.lint {
        section("LINT (tes3lint-style checks, read-only)");
        let (order, excluded) = records::filter_plugins(active, &args.exclude);
        excluded_line(excluded.len());
        let index = PluginIndex::build(dirs);
        let find = records::in_index(&index);
        let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
        let origins: BTreeMap<String, String> = got.subset_origins.clone();
        let (warnings, st) = toolscore::lint::lint_order(&order, &find, subset, &origins, threads);
        println!(
            "  Scanned {} plugin(s); {} interior cell(s), {} interior pathgrid(s).",
            st.scanned, st.interior_cells, st.pathgrids
        );
        for w in &warnings {
            println!("\n{w}");
        }
        if warnings.is_empty() {
            println!("\n  No lint findings.");
        }
    }
    if args.resource_conflicts {
        use conflictcore::resources::{self, MeshColumns};
        section("DATA-PATH RESOURCE (VFS) CONFLICTS (read-only)");
        let (list, stats) = resources::detect(dirs, &got.custom_data_dirs, &[], true);
        let (mesh, ms) = conflictcore::mesh::analyse(&list, 0);
        log.debug(&format!("analysed {} mesh conflict(s): {} finding(s), {} unreadable", ms.analysed, ms.findings, ms.unreadable));
        let notes: Vec<Option<MeshColumns>> = mesh
            .iter()
            .map(|m| Some(MeshColumns { report_note: m.as_ref().map(conflictcore::mesh::Finding::describe).unwrap_or_default(), ..Default::default() }))
            .collect();
        println!("{}", resources::report(&list, &stats, &notes, false, 200));
        if let Some(out) = args.resources_out.as_deref().filter(|_| !list.is_empty()) {
            let columns: Vec<Option<MeshColumns>> = mesh
                .iter()
                .map(|m| {
                    let (note, lost_collision, lost_animation, triangle_ratio) =
                        m.as_ref().map(conflictcore::mesh::Finding::columns).unwrap_or_default();
                    Some(MeshColumns { note, lost_collision, lost_animation, triangle_ratio, report_note: String::new() })
                })
                .collect();
            match std::fs::write(out, resources::csv(&list, &columns)) {
                Ok(()) => println!("\n  Wrote resource report: {out}"),
                Err(e) => log.error(&format!("could not write resource CSV: {}", os_error(&e, out))),
            }
        }
    }
}

// ---------------------------------------------------------------- the run

fn run(args: &Args) -> Result<(), String> {
    let log = Log { verbose: args.verbose };
    let trace = Trace::open(args.trace.as_deref());
    trace.line("CLI started");
    for k in &args.unused {
        log.warning(&format!("--{k} is not used: plugins are read by the built-in reader"));
    }
    let got = match plan::compute_plan(&args.plan) {
        Ok(got) => got,
        Err(PlanError::Refused { message, report }) => {
            say(&report, &log, || {});
            return Err(message);
        }
        Err(PlanError::Unsupported) => {
            return Err("this setup holds something the sort does not read: a file that is not UTF-8, or one that could not be opened".into());
        }
    };
    trace.line(&format!("sorted: {} plugin(s)", got.plan.subset.len()));
    say(&got.report, &log, || scans(args, &got, &log, &trace));
    let ex = ExportArgs {
        write_cfg: args.write_cfg,
        cfg: args.cfg.clone(),
        emit_toml: args.emit_toml.clone(),
        dry_run: args.dry_run,
        no_backup: args.no_backup,
        sort_data_paths: args.plan.sort_data_paths,
        list_name: args.plan.list_name.clone(),
        stamp: now().format("%Y%m%d-%H%M%S").to_string(),
        toml_line_ending: None,
    };
    let counts = Counts { rule_warnings: got.predicate_warnings.len(), yml_warnings: got.yml_warnings.len() };
    let done = plan::export(&ex, &got.plan, counts, None, None, &[], &[]).map_err(|e| match e {
        ordercore::export::ExportError::Unsupported => "the export holds something the writer does not write".to_string(),
        ordercore::export::ExportError::Io(e) => format!("the export could not write: {e}"),
    })?;
    say(&done.report, &log, || {});
    trace.line("done");
    Ok(())
}

fn main() -> ExitCode {
    let matches = command().get_matches();
    let args = Args::read(&matches);
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let mut err = String::new();
            let _ = writeln!(err, "{message}");
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().write_all(err.as_bytes());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_read_as_argparse_reads_them() {
        let m = command()
            .try_get_matches_from([
                "wraithguard", "--cfg", "a//openmw.cfg", "--rules", "r1", "r2/", "--subset", "A.esp", "B.esp", "--lint", "-vv",
                "--rules", "r3", "--trace",
            ])
            .unwrap();
        let a = Args::read(&m);
        assert_eq!(a.cfg, py_path("a/openmw.cfg"));
        assert_eq!(a.plan.rules, vec![py_path("r3")]);
        assert_eq!(a.plan.subset, vec!["A.esp", "B.esp"]);
        assert!(a.lint && a.verbose == 2 && a.trace.as_deref() == Some("wraithguard_toolkit_trace.log"));
        assert!(command().try_get_matches_from(["wraithguard", "--cfg", "x"]).is_err());
    }

    #[test]
    fn unused_paths_are_named() {
        let m = command().try_get_matches_from(["wraithguard", "--cfg", "x", "--rules", "r", "--tes3conv", "t"]).unwrap();
        assert_eq!(Args::read(&m).unused, vec!["tes3conv"]);
    }
}
