//! A whole sort, as `wraithguard_toolkit.compute_plan` runs it (port step 4.2 slice 2k, part 2):
//! read the inputs, read the cfg, hold grass back, split the curated list off, pull the cfg's
//! orphans, sort, and check the result - the rule warnings, MOMW's file, a list's data paths, the
//! masters, stale merged files - then place the new `data=` folders. Nothing is written but the
//! subset file a mods-folder scan is asked to write.
//!
//! What the Python prints comes back as a [`Report`]: an [`Item`] per section heading, line and
//! log warning, in the Python's English, each warning tagged with what it is about and the
//! plugins it names, so the Load order page can list and filter them and the Python can put them
//! back into the user's language. [`Report::text`] is what the Python prints, line for line.
//!
//! The record-conflict, cell-map, lint and resource scans stay in the Python until 4.3 / 4.4;
//! the report marks where they go ([`ItemKind::Scans`]).
//!
//! The Python is the reference and `tests/fixtures/plan_cases.json` pins both: setups made on
//! disk, the plan and every line printed. Declined ([`PlanError::Unsupported`]): a file that is
//! not UTF-8 where the Python would carry its bytes as `surrogateescape` or stop on them, an
//! input file that cannot be read (the Python raises), a read error other than a missing file
//! where the Python prints the error's own words, and what the parts decline.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::export::{
    decode_cfg, extract_data_path_value, insert_data_paths, read_cfg_text, read_groundcover_names, Append,
    CustomBlock, DataInsert, DataLine, Plan, Replace,
};
use crate::inputs::{self, PluginIndex};
use crate::momw::{self, Entry};
use crate::patterns::Unsupported;
use crate::updates::py_repr;

// ---------------------------------------------------------------- the report

/// What a report item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// A section heading (`_section`): `text` is the title.
    Section,
    /// A sub-heading (`_subsection`): `text` is the title.
    Subsection,
    /// A printed line (may start with a blank line, `\n`).
    Line,
    /// A warning the Python logs rather than prints.
    Log,
    /// Where the Python's own scans (conflicts, cell map, lint, resources) print.
    Scans,
    /// Where the Python's configurator preview prints, in an export whose TOML [`crate::apply`]
    /// declined to preview.
    Preview,
}

/// One thing the report says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What it is.
    pub kind: ItemKind,
    /// The text, in the Python's English.
    pub text: String,
    /// For a warning, what it is about: `rule`, `yml`, `master`, `master_size`, `data_path`,
    /// `stale`, `drift`, `rule_conflict`, `groundcover`, `orphan`, `input`.
    pub warning: Option<&'static str>,
    /// The plugins (or folders) it names, for the page's links.
    pub names: Vec<String>,
}

/// What a run says, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The items.
    pub items: Vec<Item>,
}

impl Report {
    fn push(&mut self, kind: ItemKind, text: impl Into<String>) {
        self.items.push(Item { kind, text: text.into(), warning: None, names: Vec::new() });
    }
    fn section(&mut self, title: impl Into<String>) {
        self.push(ItemKind::Section, title);
    }
    fn subsection(&mut self, title: impl Into<String>) {
        self.push(ItemKind::Subsection, title);
    }
    fn line(&mut self, text: impl Into<String>) {
        self.push(ItemKind::Line, text);
    }
    fn log(&mut self, text: impl Into<String>) {
        self.push(ItemKind::Log, text);
    }
    fn warn(&mut self, what: &'static str, text: impl Into<String>, names: Vec<String>) {
        self.items.push(Item { kind: ItemKind::Line, text: text.into(), warning: Some(what), names });
    }

    /// What the Python prints for these items (the logged ones and the scans' and preview's places
    /// left out), one printed line to an element.
    pub fn text(&self) -> Vec<String> {
        self.text_marked(None, None)
    }

    /// [`Report::text`] with a line standing for what the scans and the preview print.
    pub fn text_marked(&self, scans: Option<&str>, preview: Option<&str>) -> Vec<String> {
        let mut out = Vec::new();
        for item in &self.items {
            let printed = match item.kind {
                ItemKind::Section => {
                    let rule = "=".repeat(70);
                    format!("\n{rule}\n {}\n{rule}", item.text)
                }
                ItemKind::Subsection => format!("\n--- {} ---", item.text),
                ItemKind::Line => item.text.clone(),
                ItemKind::Scans => match scans {
                    Some(s) => s.to_string(),
                    None => continue,
                },
                ItemKind::Preview => match preview {
                    Some(s) => s.to_string(),
                    None => continue,
                },
                ItemKind::Log => continue,
            };
            out.extend(printed.split('\n').map(str::to_string));
        }
        out
    }

    /// The logged warnings' texts.
    pub fn logged(&self) -> Vec<String> {
        self.items.iter().filter(|i| i.kind == ItemKind::Log).map(|i| i.text.clone()).collect()
    }
}

/// English plurals, as `ngettext` gives them untranslated.
fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

// ---------------------------------------------------------------- the arguments

/// What a run is asked to do (the arguments `compute_plan` reads). Paths are as the Python
/// prints them (`str(Path)`).
#[derive(Clone, Debug, Default)]
pub struct PlanArgs {
    /// The `openmw.cfg`.
    pub cfg: String,
    /// `--customizations`.
    pub customizations: Option<String>,
    /// `--subset`.
    pub subset: Vec<String>,
    /// `--subset-file`.
    pub subset_file: Option<String>,
    /// The lines of a scan held in memory (the GUI's).
    pub subset_lines: Option<Vec<String>>,
    /// `--scan-dir` (written to `subset_file`).
    pub scan_dir: Option<String>,
    /// `--subset-from-cfg`.
    pub subset_from_cfg: bool,
    /// Go on with nothing to sort (the GUI shows the cfg's order to edit).
    pub allow_empty_sort: bool,
    /// `--sort-data-paths`.
    pub sort_data_paths: bool,
    /// `--emit-toml` was given (so the source TOML's blocks are kept for it).
    pub emit_toml: bool,
    /// `--groundcover`.
    pub groundcover: Vec<String>,
    /// `--plugin-order-yml`.
    pub plugin_order_yml: Option<String>,
    /// `--list-name`.
    pub list_name: Option<String>,
    /// `--rules`: rule files and folders of `*.txt` rule files, in precedence order.
    pub rules: Vec<String>,
    /// `--no-predicate-warnings`.
    pub no_predicate_warnings: bool,
    /// `--data-paths-cache`.
    pub data_paths_cache: Option<String>,
    /// `-v`: list each of a list's data paths missing from the cfg.
    pub verbose: bool,
}

/// Why there is no plan.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The run stops with this message (the Python's `SystemExit`), after saying `report`.
    Refused { message: String, report: Report },
    /// Not read here; the caller keeps the Python answer.
    Unsupported,
}

fn refused(message: &str) -> PlanError {
    PlanError::Refused { message: message.to_string(), report: Report::default() }
}

impl From<Unsupported> for PlanError {
    fn from(_: Unsupported) -> Self {
        PlanError::Unsupported
    }
}

/// A computed plan: what export needs ([`Plan`]) and what the run found.
#[derive(Clone, Debug, Default)]
pub struct Computed {
    /// The part export reads. Its `final_order` is empty when nothing was sorted.
    pub plan: Plan,
    /// Whether a sort ran (the Python's `final_order` is `None` when not).
    pub sorted: bool,
    /// The predicate rules' warnings.
    pub predicate_warnings: Vec<String>,
    /// MOMW's file's warnings.
    pub yml_warnings: Vec<String>,
    /// The master check's missing and out-of-order lines.
    pub master_warnings: Vec<String>,
    /// The plugins behind those.
    pub master_problem_plugins: Vec<String>,
    /// `{plugin in lower case: where it came from}`.
    pub subset_origins: BTreeMap<String, String>,
    /// Every folder this run's plugins may be in (cfg, then pending).
    pub scan_dirs: Vec<String>,
    /// The pending custom folders.
    pub custom_data_dirs: Vec<String>,
    /// What the run said.
    pub report: Report,
}

// ---------------------------------------------------------------- reading files as Python does

/// A file's text as `read_text(encoding="utf-8")` gives it (strict: other bytes are declined).
fn read_utf8(path: &str) -> Result<String, PlanError> {
    let bytes = fs::read(path).map_err(|_| PlanError::Unsupported)?;
    String::from_utf8(bytes).map_err(|_| PlanError::Unsupported)
}

/// The file name of a path as `Path(...).name` gives it.
fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// What `str(OSError)` says for a missing file; other errors are declined (their words are the
/// system's).
fn missing_file_error(e: &io::Error, path: &str) -> Result<String, PlanError> {
    if e.kind() == io::ErrorKind::NotFound {
        Ok(format!("[Errno 2] No such file or directory: {}", py_repr(path)?))
    } else {
        Err(PlanError::Unsupported)
    }
}

/// The rule files a `--rules` entry names: itself if a file, else its `*.txt` files, sorted as
/// `sorted(path.glob("*.txt"))` sorts them on this system.
fn rule_files(entry: &str) -> Vec<PathBuf> {
    let path = PathBuf::from(entry);
    if path.is_file() {
        return vec![path];
    }
    let Ok(rd) = fs::read_dir(&path) else { return Vec::new() };
    let windows = cfg!(windows);
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if windows { name.to_lowercase().ends_with(".txt") } else { name.ends_with(".txt") }
        })
        .collect();
    if windows {
        files.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    } else {
        files.sort();
    }
    files
}

/// A rule file's text as `read_text(encoding="utf-8-sig", errors="replace")` gives it.
fn rule_text(path: &Path) -> Result<String, PlanError> {
    let bytes = fs::read(path).map_err(|_| PlanError::Unsupported)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// The ordering blocks of every rule file (`load_rule_blocks`): `(chains, nearstart, nearend)`,
/// a chain's priority its entry's place in `rules`, and a note per file with bridged lines.
fn load_rule_blocks(rules: &[String], report: &mut Report) -> Result<RuleBlocks, PlanError> {
    let mut out = RuleBlocks::default();
    for (priority, entry) in rules.iter().enumerate() {
        for file in rule_files(entry) {
            let parsed = crate::rules::parse_text(&rule_text(&file)?);
            if parsed.skipped > 0 {
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let lines = plural(parsed.skipped, "line", "lines");
                report.line(format!(
                    "NOTE: {name}: {} conditional/unrecognized {lines} inside ordering rules treated as \
                     not-installed and bridged over (mlox does the same).",
                    parsed.skipped
                ));
            }
            for block in parsed.blocks {
                match block.kind {
                    crate::rules::Kind::Order => out.chains.push((block.names, i64::try_from(priority).unwrap_or(0))),
                    crate::rules::Kind::NearStart => out.nearstart.extend(block.names),
                    crate::rules::Kind::NearEnd => out.nearend.extend(block.names),
                }
            }
        }
    }
    Ok(out)
}

#[derive(Default)]
struct RuleBlocks {
    chains: Vec<(Vec<String>, i64)>,
    nearstart: Vec<String>,
    nearend: Vec<String>,
}

/// Every rule file's text joined by newlines (`load_rules_raw_text`).
fn load_rules_raw_text(rules: &[String]) -> Result<String, PlanError> {
    let mut chunks = Vec::new();
    for entry in rules {
        for file in rule_files(entry) {
            chunks.push(rule_text(&file)?);
        }
    }
    Ok(chunks.join("\n"))
}

// ---------------------------------------------------------------- the source TOML's blocks

/// The `[[Customizations]]` blocks of a TOML as export carries them through (what the Python
/// reads off `tomllib`'s dict for `generate_customizations_toml`).
///
/// # Errors
/// [`Unsupported`] for a file tomllib refuses, or a value not of the shape export reads.
pub fn customization_blocks(text: &str) -> Result<Vec<CustomBlock>, Unsupported> {
    if text.starts_with('\u{feff}') {
        return Err(Unsupported);
    }
    let table = text.parse::<toml::Table>().map_err(|_| Unsupported)?;
    let Some(list) = table.get("Customizations") else { return Ok(Vec::new()) };
    let list = list.as_array().ok_or(Unsupported)?;
    let opt_str = |t: &toml::Table, k: &str| -> Result<Option<String>, Unsupported> {
        match t.get(k) {
            None => Ok(None),
            Some(v) => v.as_str().map(|s| Some(s.to_string())).ok_or(Unsupported),
        }
    };
    let strs = |t: &toml::Table, k: &str| -> Result<Vec<String>, Unsupported> {
        match t.get(k) {
            None => Ok(Vec::new()),
            Some(v) => v
                .as_array()
                .ok_or(Unsupported)?
                .iter()
                .map(|x| x.as_str().map(str::to_string).ok_or(Unsupported))
                .collect(),
        }
    };
    let tables = |t: &toml::Table, k: &str| -> Result<Vec<toml::Table>, Unsupported> {
        match t.get(k) {
            None => Ok(Vec::new()),
            Some(v) => v.as_array().ok_or(Unsupported)?.iter().map(|x| x.as_table().cloned().ok_or(Unsupported)).collect(),
        }
    };
    list.iter()
        .map(|b| {
            let b = b.as_table().ok_or(Unsupported)?;
            Ok(CustomBlock {
                list_name: opt_str(b, "listName")?,
                remove_data: strs(b, "removeData")?,
                remove_content: strs(b, "removeContent")?,
                remove_fallback: strs(b, "removeFallback")?,
                remove_groundcover: strs(b, "removeGroundcover")?,
                replace: tables(b, "replace")?
                    .iter()
                    .map(|r| Ok(Replace { source: opt_str(r, "source")?, dest: opt_str(r, "dest")? }))
                    .collect::<Result<_, Unsupported>>()?,
                append: tables(b, "append")?
                    .iter()
                    .map(|r| Ok(Append { append: opt_str(r, "append")?, append_block: opt_str(r, "appendBlock")? }))
                    .collect::<Result<_, Unsupported>>()?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- the stages

fn insert(value: String) -> DataInsert {
    DataInsert { value, after: None, before: None }
}

fn values(inserts: &[DataInsert]) -> Vec<String> {
    inserts.iter().map(|d| d.value.clone()).filter(|v| !v.is_empty()).collect()
}

fn dedupe_lower(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|s| seen.insert(s.to_lowercase())).collect()
}

/// What the input stage read (`_read_subset_inputs`).
#[derive(Default)]
struct Inputs {
    subset: Vec<String>,
    data_inserts: Vec<DataInsert>,
    raw_toml_data_inserts: Vec<DataInsert>,
    original_content_values: BTreeMap<String, String>,
    original_toml_blocks: Vec<CustomBlock>,
    replace_dest_names: HashSet<String>,
    subset_origins: BTreeMap<String, String>,
}

fn not_sorted_note(report: &mut Report, inserts: &[DataInsert]) {
    let paths: Vec<&str> = inserts.iter().map(|d| d.value.as_str()).collect();
    report.line(format!(
        "  NOTE: data path insertions found but not sorted (pass --sort-data-paths to include them): {}",
        paths.join(", ")
    ));
}

fn skipped_warnings(report: &mut Report, skipped: &[String], source: &str) {
    for entry in skipped {
        report.log(format!(
            "'{entry}' from {source} doesn't look like a plugin filename or a data folder path (no recognized \
             extension, no slash) -- skipping."
        ));
    }
}

/// The subset file read as `extract_subset_from_subset_file` reads it: `(plugins, data)`.
fn read_subset_file(path: &str, report: &mut Report) -> Result<(Vec<String>, Vec<DataInsert>), PlanError> {
    let text = read_utf8(path)?;
    let entries = if path.to_lowercase().ends_with(".toml") {
        inputs::subset_from_toml(&text)?.entries
    } else {
        inputs::subset_from_lines(&inputs::text_lines(&text))?
    };
    skipped_warnings(report, &entries.skipped, path);
    Ok((entries.plugins, entries.data.into_iter().map(insert).collect()))
}

fn read_inputs(args: &PlanArgs, report: &mut Report) -> Result<Inputs, PlanError> {
    if let Some(dir) = &args.scan_dir {
        let Some(out) = &args.subset_file else {
            return Err(refused("--scan-dir requires --subset-file (where to write the scanned list)."));
        };
        report.section("SCANNING MODS FOLDER");
        let scan = inputs::scan_mod_directories(dir);
        fs::write(out, scan.text()).map_err(|_| PlanError::Unsupported)?;
        report.line(format!(
            "Scanned '{}': {} mod folder(s), {} plugin(s).",
            scan.start, scan.folders, scan.plugins
        ));
        report.line(format!("Wrote subset file: {out}"));
    }
    let has_lines = args.subset_lines.as_ref().is_some_and(|l| !l.is_empty());
    if args.customizations.is_none()
        && args.subset.is_empty()
        && args.subset_file.is_none()
        && !has_lines
        && !args.subset_from_cfg
        && !args.allow_empty_sort
    {
        return Err(refused(
            "Provide --customizations, --subset, --subset-file, --scan-dir, or --subset-from-cfg so there's \
             something to sort.",
        ));
    }
    report.section("READING INPUT");
    let mut inp = Inputs { subset: args.subset.clone(), ..Inputs::default() };

    if let Some(path) = &args.subset_file {
        let (plugins, data) = read_subset_file(path, report)?;
        inp.subset.extend(plugins.iter().cloned());
        inp.raw_toml_data_inserts.extend(data.iter().cloned());
        report.line(format!("  {path}: {} plugin(s), {} data path(s)", plugins.len(), data.len()));
        if args.sort_data_paths {
            inp.data_inserts.extend(data);
        } else if !data.is_empty() {
            not_sorted_note(report, &data);
        }
        let origin = format!("subset file ({})", file_name(path));
        for name in plugins {
            inp.original_content_values.entry(name.clone()).or_insert_with(|| name.clone());
            inp.subset_origins.entry(name.to_lowercase()).or_insert_with(|| origin.clone());
        }
    }

    if let Some(lines) = args.subset_lines.as_ref().filter(|l| !l.is_empty()) {
        let entries = inputs::subset_from_lines(lines)?;
        skipped_warnings(report, &entries.skipped, "scanned subset");
        let data: Vec<DataInsert> = entries.data.into_iter().map(insert).collect();
        inp.subset.extend(entries.plugins.iter().cloned());
        inp.raw_toml_data_inserts.extend(data.iter().cloned());
        report.line(format!(
            "  in-memory scan: {} plugin(s), {} data path(s)",
            entries.plugins.len(),
            data.len()
        ));
        if args.sort_data_paths {
            inp.data_inserts.extend(data);
        } else if !data.is_empty() {
            let n = data.len();
            report.line(format!(
                "  NOTE: data path {} found but not sorted (pass --sort-data-paths to include {}): {n} {}",
                plural(n, "insertion", "insertions"),
                plural(n, "it", "them"),
                plural(n, "path", "paths")
            ));
        }
        for name in entries.plugins {
            inp.original_content_values.entry(name.clone()).or_insert_with(|| name.clone());
            inp.subset_origins.entry(name.to_lowercase()).or_insert_with(|| "scanned subset (in memory)".into());
        }
    }

    if let Some(path) = &args.customizations {
        let text = read_utf8(path)?;
        let custom = inputs::customizations(&text)?;
        inp.subset.extend(custom.subset.iter().cloned());
        inp.raw_toml_data_inserts.extend(custom.data.iter().cloned());
        report.line(format!(
            "  {path}: {} content plugin(s), {} data path(s)",
            custom.subset.len(),
            custom.data.len()
        ));
        if args.sort_data_paths {
            inp.data_inserts.extend(custom.data.iter().cloned());
        } else if !custom.data.is_empty() {
            not_sorted_note(report, &custom.data);
        }
        for name in &custom.subset {
            inp.original_content_values.insert(name.clone(), name.clone());
            let origin = match custom.list_names.get(name) {
                Some(list) => format!("customizations.toml -> '{list}'"),
                None => "customizations.toml".into(),
            };
            inp.subset_origins.insert(name.to_lowercase(), origin);
        }
        inp.replace_dest_names = custom.replace_dest.into_iter().collect();
        if args.emit_toml {
            inp.original_toml_blocks = customization_blocks(&text)?;
        }
    }

    inp.subset = dedupe_lower(std::mem::take(&mut inp.subset));
    if inp.subset.is_empty()
        && inp.data_inserts.is_empty()
        && inp.raw_toml_data_inserts.is_empty()
        && !args.subset_from_cfg
        && !args.allow_empty_sort
    {
        return Err(refused("No subset plugins or data paths found -- nothing to do."));
    }
    Ok(inp)
}

/// Every plugin this run declares as grass (`declared_groundcover`), and the not-plugin
/// declarations it warns about on the way.
fn declared_groundcover(args: &PlanArgs, report: &mut Report) -> Result<Vec<String>, PlanError> {
    let mut found = args.groundcover.clone();
    if let Some(path) = &args.subset_file {
        let text = fs::read(path).ok().map(String::from_utf8);
        let text = match text {
            None => String::new(), // the subset reader reports this
            Some(Ok(t)) => t,
            Some(Err(_)) => return Err(PlanError::Unsupported),
        };
        if path.to_lowercase().ends_with(".toml") {
            match inputs::subset_from_toml(&text) {
                Ok(t) => found.extend(t.groundcover),
                Err(_) => return Err(PlanError::Unsupported),
            }
        } else {
            let (names, not_plugins) = inputs::groundcover_declarations(&inputs::text_lines(&text))?;
            not_grass_warnings(report, &not_plugins, path);
            found.extend(names);
        }
    }
    if let Some(lines) = &args.subset_lines {
        let (names, not_plugins) = inputs::groundcover_declarations(lines)?;
        not_grass_warnings(report, &not_plugins, "scanned subset");
        found.extend(names);
    }
    Ok(dedupe_lower(found))
}

fn not_grass_warnings(report: &mut Report, names: &[String], source: &str) {
    for name in names {
        report.log(format!("'{name}' from {source} is declared as groundcover but is not a plugin filename -- skipping."));
    }
}

/// MOMW's file read and the curated list split off (`_apply_plugin_order_yml`).
#[derive(Default)]
struct Yml {
    entries: Vec<Entry>,
    curated_set: HashSet<String>,
    curated_order: Vec<String>,
    warnings: Vec<String>,
    declared_lower: HashSet<String>,
}

fn apply_plugin_order_yml(args: &PlanArgs, subset: &mut Vec<String>, report: &mut Report) -> Result<Yml, PlanError> {
    let mut yml = Yml { declared_lower: subset.iter().map(|s| s.to_lowercase()).collect(), ..Yml::default() };
    let list_name = args.list_name.as_deref().filter(|n| !n.is_empty());
    let Some(path) = &args.plugin_order_yml else { return Ok(yml) };
    report.section("PLUGIN-ORDER.YML (MOMW source of truth)");
    let read = match fs::read(path) {
        Ok(bytes) => momw::parse_bytes(&bytes).map_err(|e| e.0),
        Err(e) => Err(missing_file_error(&e, path)?),
    };
    match read {
        Ok(entries) => {
            let n = entries.len();
            report.line(format!("  Loaded {n} plugin {} from {}", plural(n, "entry", "entries"), file_name(path)));
            yml.entries = entries;
        }
        Err(error) => {
            report.line(format!("  WARNING: could not read plugin-order.yml ({error}) -- skipping yml checks."));
        }
    }
    if !yml.entries.is_empty() && list_name.is_none() {
        report.line(
            "  NOTE: no list name given -- can't separate curated-list plugins from your custom ones, so \
             curated/redundant/orphan/order checks are skipped (needs-cleaning notes still work).",
        );
    }
    if let (false, Some(list)) = (yml.entries.is_empty(), list_name) {
        let (set, order) = momw::curated_for_list(&yml.entries, list);
        yml.curated_set = set;
        yml.curated_order = order;
        let n = yml.curated_set.len();
        report.line(format!("  '{list}': {n} curated {} on this list", plural(n, "plugin", "plugins")));
        if yml.curated_set.is_empty() {
            report.line(format!(
                "  WARNING: no plugins found for list '{list}' in the yml -- check the list name spelling. \
                 Curated-set checks skipped."
            ));
        }
        let redundant: Vec<String> =
            subset.iter().filter(|s| yml.curated_set.contains(&s.to_lowercase())).cloned().collect();
        if !redundant.is_empty() {
            subset.retain(|s| !yml.curated_set.contains(&s.to_lowercase()));
            yml.warnings.extend(redundant.iter().map(|r| {
                format!(
                    "[REDUNDANT] '{r}' is already part of the '{list}' list -- not sorting it (leaving it to the \
                     curated list / configurator)."
                )
            }));
            let n = redundant.len();
            report.line(format!(
                "  Excluded {n} curated {} from the sort: {}",
                plural(n, "plugin", "plugins"),
                redundant.join(", ")
            ));
        }
    }
    Ok(yml)
}

/// A data-paths cache's tails, or the error the Python prints (`Err`) when it cannot be read.
fn read_cache(path: &str) -> Result<Result<Vec<String>, String>, PlanError> {
    match fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8(bytes).map_err(|_| PlanError::Unsupported)?;
            Ok(Ok(inputs::parse_data_paths_cache(&text)))
        }
        Err(e) => Ok(Err(missing_file_error(&e, path)?)),
    }
}

#[allow(clippy::too_many_arguments)]
fn pull_cfg_orphans(
    args: &PlanArgs,
    inp: &mut Inputs,
    base_order_names: &[String],
    data_order: &[String],
    yml: &Yml,
    cfg_groundcover_lower: &HashSet<String>,
    report: &mut Report,
) -> Result<Vec<String>, PlanError> {
    if !args.subset_from_cfg {
        return Ok(Vec::new());
    }
    report.section("UNMANAGED (ORPHAN) ENTRIES FROM openmw.cfg");
    let tails = match &args.data_paths_cache {
        Some(cache) => Some(read_cache(cache)?.unwrap_or_default()),
        None => None,
    };
    let declared_data: Vec<String> =
        inp.data_inserts.iter().chain(&inp.raw_toml_data_inserts).map(|d| d.value.clone()).collect();
    let q = inputs::OrphanQuery {
        subset: &inp.subset,
        base_order_names,
        data_order,
        curated_lower: yml.curated_set.clone(),
        needs_cleaning_lower: momw::needs_cleaning_set(&yml.entries),
        declared_lower: yml.declared_lower.clone(),
        groundcover_lower: cfg_groundcover_lower.clone(),
        declared_data: &declared_data,
        tails: tails.as_deref(),
    };
    let (new_plugins, orphan_data) = inputs::cfg_orphans(&q)?;
    for name in &new_plugins {
        inp.subset.push(name.clone());
        inp.original_content_values.entry(name.clone()).or_insert_with(|| name.clone());
        inp.subset_origins.entry(name.to_lowercase()).or_insert_with(|| "openmw.cfg (orphan)".into());
    }
    let n = new_plugins.len();
    report.line(format!("  {n} orphan {} pulled from openmw.cfg", plural(n, "plugin", "plugins")));
    for name in &new_plugins {
        report.warn("orphan", format!("    content={name}"), vec![name.clone()]);
    }
    if new_plugins.is_empty() {
        report.line(
            "  No unmanaged plugins found -- every content= plugin in the cfg is on the list or in your \
             customizations.",
        );
    }
    match orphan_data {
        Some(data) => {
            let n = data.len();
            report.line(format!("  {n} orphan data= {} found in openmw.cfg", plural(n, "path", "paths")));
            for value in &data {
                report.warn("orphan", format!("    data={value}"), vec![value.clone()]);
            }
            if data.is_empty() {
                report.line(
                    "  No unmanaged data= paths found -- every data= path in the cfg is on the list's data-path \
                     order or in your customizations.",
                );
            }
            Ok(data)
        }
        None => {
            report.line(
                "  data= paths not classified -- fetch a data-paths cache for this list to surface unmanaged \
                 (orphan) data= paths too.",
            );
            Ok(Vec::new())
        }
    }
}

/// `{custom in lower case: (how, anchor)}`.
type Anchors = BTreeMap<String, (String, Option<String>)>;

#[allow(clippy::too_many_arguments)]
fn sort_subset(
    args: &PlanArgs,
    subset: &[String],
    base_order_names: &[String],
    data_order: &[String],
    scan_dirs: &[String],
    origins: &BTreeMap<String, String>,
    anchors: &mut Anchors,
    report: &mut Report,
) -> Result<(Option<Vec<String>>, Vec<String>), PlanError> {
    if subset.is_empty() {
        if args.allow_empty_sort {
            report.section("NOTHING TO SORT");
            report.line(
                "  No plugins to sort. The current openmw.cfg load order is shown above for editing -- remove \
                 entries and Export, or add plugins and Sort again.",
            );
        }
        return Ok((None, Vec::new()));
    }
    report.section(format!("SORTING {} PLUGIN(S)", subset.len()));
    report.line(format!("  {}", subset.join(", ")));
    let base_lower: HashSet<String> = base_order_names.iter().map(|n| n.to_lowercase()).collect();
    let (present, new): (Vec<&String>, Vec<&String>) = subset.iter().partition(|s| base_lower.contains(&s.to_lowercase()));
    let join = |v: &[&String]| v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
    if !present.is_empty() {
        report.line(format!("\n  Already in cfg, will be repositioned within it: {}", join(&present)));
    }
    if !new.is_empty() {
        report.line(format!("  Not in cfg yet, will be inserted: {}", join(&new)));
    }
    report.subsection("loading mlox rules");
    let rules = load_rule_blocks(&args.rules, report)?;

    let index = PluginIndex::build(scan_dirs);
    let mut masters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in subset {
        if let Some(p) = index.find(name) {
            let ms = inputs::read_masters(p);
            if !ms.is_empty() {
                masters.insert(name.to_lowercase(), ms);
            }
        }
    }
    if masters.is_empty() {
        report.line(
            "  (No header masters read -- mod files not reachable from the cfg's data= folders, so ordering uses \
             mlox rules + ESM-first only.)",
        );
    } else {
        report.line(format!("  Read header masters for {} of {} custom plugin(s).", masters.len(), subset.len()));
    }

    let sorted =
        crate::order::build_and_sort(base_order_names, subset, &rules.chains, &masters, &rules.nearstart, &rules.nearend)?;
    let mut seen = HashSet::new();
    let unique: Vec<&(String, String)> =
        sorted.conflicts.iter().filter(|(a, b)| seen.insert((a.to_lowercase(), b.to_lowercase()))).collect();
    if !unique.is_empty() {
        let n = unique.len();
        report.warn(
            "rule_conflict",
            format!(
                "\n  {n} mlox ordering {} not applied -- your openmw.cfg already orders these the other way, so \
                 your (curated) cfg order is kept:",
                plural(n, "rule", "rules")
            ),
            Vec::new(),
        );
        for (a, b) in unique {
            report.warn(
                "rule_conflict",
                format!("    - mlox wanted '{a}' before '{b}', but your load order already has '{b}' before '{a}'"),
                vec![a.clone(), b.clone()],
            );
        }
    }
    anchors.extend(sorted.anchors.into_iter().map(|(k, (how, a))| (k, (how.as_str().to_string(), a))));
    let final_order = sorted.order;

    let subset_lower: HashSet<String> = subset.iter().map(|s| s.to_lowercase()).collect();
    let frozen_before: Vec<&String> = base_order_names.iter().filter(|n| !subset_lower.contains(&n.to_lowercase())).collect();
    let base_set: HashSet<&String> = base_order_names.iter().collect();
    let frozen_after: Vec<&String> =
        final_order.iter().filter(|n| base_set.contains(n) && !subset_lower.contains(&n.to_lowercase())).collect();
    if frozen_before != frozen_after {
        report.warn(
            "drift",
            "\n  INTERNAL WARNING: curated (frozen) order drifted -- this shouldn't happen. Please double check the \
             output before using it.",
            Vec::new(),
        );
    }
    report.subsection("final content= order");
    for n in &final_order {
        let tag = if subset_lower.contains(&n.to_lowercase()) { "  <-- inserted/moved" } else { "" };
        report.line(format!("  content={n}{tag}"));
    }

    let mut predicate_warnings = Vec::new();
    if !args.no_predicate_warnings {
        let text = load_rules_raw_text(&args.rules)?;
        let mut pred_dirs = Vec::new();
        for line in data_order {
            if let Some(v) = extract_data_path_value(line)?.filter(|v| !v.is_empty()) {
                pred_dirs.push(v);
            }
        }
        let facts = PluginIndex::build(&pred_dirs);
        predicate_warnings = crate::predicates::check_predicates(&text, &final_order, origins, &facts)?;
        if predicate_warnings.is_empty() {
            report.line("\n  No [Conflict]/[Requires]/[Note] warnings triggered.");
        } else {
            report.section(format!("{} MLOX RULE WARNING(S) -- read-only, not enforced", predicate_warnings.len()));
            for w in &predicate_warnings {
                let names = final_order.iter().filter(|n| w.contains(n.as_str())).cloned().collect();
                report.warn("rule", format!("\n{w}"), names);
            }
        }
    }
    Ok((Some(final_order), predicate_warnings))
}

/// The plugins of `order` a warning's text names.
fn named_in(text: &str, order: &[String]) -> Vec<String> {
    order.iter().filter(|n| text.contains(&format!("'{n}'"))).cloned().collect()
}

fn data_path_order_warnings(args: &PlanArgs, data_order: &[String], report: &mut Report) -> Result<(), PlanError> {
    let Some(cache) = &args.data_paths_cache else { return Ok(()) };
    let tails = match read_cache(cache)? {
        Ok(t) => t,
        Err(error) => {
            report.section("1 DATA PATH WARNING(S) -- read-only, not enforced");
            report.warn("data_path", format!("\n[DATA PATH] could not read {}: {error}", file_name(cache)), Vec::new());
            return Ok(());
        }
    };
    let mut cfg_paths = Vec::new();
    for line in data_order {
        if let Some(v) = extract_data_path_value(line)?.filter(|v| !v.is_empty()) {
            cfg_paths.push(v);
        }
    }
    let warnings = inputs::reconcile_data_paths(&tails, &cfg_paths, args.verbose);
    if warnings.is_empty() {
        report.line("\n  No data path warnings.");
    } else {
        report.section(format!("{} DATA PATH WARNING(S) -- read-only, not enforced", warnings.len()));
        for w in warnings {
            report.warn("data_path", format!("\n{w}"), Vec::new());
        }
    }
    Ok(())
}

fn check_masters(
    active: &[String],
    scan_dirs: &[String],
    origins: &BTreeMap<String, String>,
    report: &mut Report,
) -> (Vec<String>, Vec<String>) {
    let origins: HashMap<String, String> = origins.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let got = inputs::check_missing_masters(active, &PluginIndex::build(scan_dirs), &origins);
    if got.checked == 0 {
        report.section("MASTER CHECK -- skipped");
        report.line("  (plugin files not reachable from the data folders; can't read headers)");
        return (Vec::new(), got.problem_plugins);
    }
    let issues = got.missing.len() + got.order_problems.len();
    report.section(format!(
        "MASTER CHECK -- {} plugin(s) read, {issues} problem(s), {} size note(s)",
        got.checked,
        got.size_notes.len()
    ));
    for w in got.missing.iter().chain(&got.order_problems) {
        report.warn("master", format!("\n{w}"), named_in(w, active));
    }
    if !got.size_notes.is_empty() {
        report.subsection(format!("{} master size mismatch note(s) (usually benign)", got.size_notes.len()));
        for w in &got.size_notes {
            report.warn("master_size", w.clone(), named_in(w, active));
        }
    }
    if issues == 0 && got.size_notes.is_empty() {
        report.line("  All masters present and correctly ordered.");
    }
    let mut warnings = got.missing;
    warnings.extend(got.order_problems);
    (warnings, got.problem_plugins)
}

/// The merged files a curated list builds, which go stale when a plugin is newer.
const MERGED_ARTIFACTS: [&str; 3] = ["delta-merged.omwaddon", "deleted_groundcover.omwaddon", "S3LightFixes.esp"];

fn mtime(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn staleness_watchdog(active: &[String], scan_dirs: &[String], report: &mut Report) {
    let index = PluginIndex::build(scan_dirs);
    let active_lower: HashSet<String> = active.iter().map(|n| n.to_lowercase()).collect();
    for artifact in MERGED_ARTIFACTS {
        let Some(path) = index.find(artifact) else { continue };
        if !active_lower.contains(&artifact.to_lowercase()) {
            continue;
        }
        let Some(made) = mtime(path) else { return }; // the Python's guard ends the watch here
        let newer: Vec<String> = active
            .iter()
            .filter(|n| n.to_lowercase() != artifact.to_lowercase() && !n.to_lowercase().ends_with(".omwscripts"))
            .filter(|n| index.find(n).and_then(mtime).is_some_and(|t| t > made))
            .cloned()
            .collect();
        if !newer.is_empty() {
            let n = newer.len();
            let examples = newer.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let more = if n > 3 { ", ..." } else { "" };
            let mut names = vec![artifact.to_string()];
            names.extend(newer.iter().cloned());
            report.warn(
                "stale",
                format!(
                    "\n[STALE] '{artifact}' is older than {n} active {} (e.g. {examples}{more}) -- re-run \
                     momw-configurator so it gets rebuilt against the current load order.",
                    plural(n, "plugin", "plugins")
                ),
                names,
            );
        }
    }
}

fn plan_data_paths(
    args: &PlanArgs,
    data_inserts: &mut [DataInsert],
    data_order: &[String],
    final_order: &[String],
    report: &mut Report,
) -> Result<Option<Vec<DataLine>>, PlanError> {
    if data_inserts.is_empty() || !args.sort_data_paths {
        return Ok(None);
    }
    report.section(format!("SORTING {} DATA PATH(S)", data_inserts.len()));
    let base_dir = Path::new(&args.cfg).parent().map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p });
    let notes = inputs::infer_data_path_anchors(data_inserts, data_order, final_order, base_dir)?;
    for n in notes {
        report.line(format!("  Inferred anchor for '{}': {} '{}' (via plugin {})", n.value, n.mode, n.anchor, n.via));
    }
    for d in data_inserts.iter() {
        let set = |s: &Option<String>| s.clone().filter(|v| !v.is_empty());
        let anchor = match (set(&d.after), set(&d.before)) {
            (Some(a), _) => format!("after '{a}'"),
            (None, Some(b)) => format!("before '{b}'"),
            (None, None) => "no anchor".into(),
        };
        report.line(format!("  {}  ({anchor})", d.value));
    }
    let (result, notes) = insert_data_paths(data_order, data_inserts)?;
    for note in notes {
        report.line(note);
    }
    report.subsection("final data= order");
    for (line, is_new, _) in &result {
        report.line(format!("  {line}{}", if *is_new { "  <-- inserted" } else { "" }));
    }
    Ok(Some(result))
}

// ---------------------------------------------------------------- the run

/// Compute a plan (`compute_plan`): see the module. Writes only the subset file a scan is asked
/// to write.
///
/// # Errors
/// [`PlanError`].
pub fn compute_plan(args: &PlanArgs) -> Result<Computed, PlanError> {
    let mut report = Report::default();
    match run(args, &mut report) {
        Err(PlanError::Refused { message, .. }) => Err(PlanError::Refused { message, report }),
        other => other,
    }
}

fn run(args: &PlanArgs, report: &mut Report) -> Result<Computed, PlanError> {
    let mut inp = read_inputs(args, report)?;

    let cfg_bytes = fs::read(&args.cfg).map_err(|_| PlanError::Unsupported)?;
    let cfg = read_cfg_text(&decode_cfg(&cfg_bytes)?)?;
    let base_order_names: Vec<String> = cfg.content_order.iter().map(|(n, _)| n.clone()).collect();

    let cfg_groundcover = read_groundcover_names(&cfg.lines)?;
    let user_groundcover = declared_groundcover(args, report)?;
    let known: HashSet<String> = cfg_groundcover.iter().map(|n| n.to_lowercase()).collect();
    let new_groundcover: Vec<String> =
        user_groundcover.iter().filter(|n| !known.contains(&n.to_lowercase())).cloned().collect();
    let all_grass: Vec<String> = cfg_groundcover.iter().chain(&user_groundcover).cloned().collect();
    let (kept, held_back) = inputs::hold_back_groundcover(&inp.subset, &all_grass);
    inp.subset = kept;
    if !new_groundcover.is_empty() {
        report.subsection("groundcover declared by this run");
        for name in &new_groundcover {
            report.warn("groundcover", format!("  groundcover={name}"), vec![name.clone()]);
        }
        report.line(
            "  These are written as groundcover= rather than content=. Their data= folders are inserted \
             normally -- OpenMW has to find the file.",
        );
    }
    if !held_back.is_empty() {
        report.subsection("groundcover plugins held back");
        let n = held_back.len();
        report.line(if n == 1 {
            format!(
                "{n} plugin in your subset is already declared as groundcover in openmw.cfg, so it was NOT added \
                 to content=:"
            )
        } else {
            format!(
                "{n} plugins in your subset are already declared as groundcover in openmw.cfg, so they were NOT \
                 added to content=:"
            )
        });
        for name in &held_back {
            report.warn("groundcover", format!("  {name}"), vec![name.clone()]);
        }
        report.line(
            "  Their groundcover= lines are untouched and their data= paths are still written, so the grass stays \
             enabled -- as grass. Loading one as content= too would spawn every blade as a real object.",
        );
    }

    let mut subset = std::mem::take(&mut inp.subset);
    let yml = apply_plugin_order_yml(args, &mut subset, report)?;
    inp.subset = subset;

    let orphan_data_paths =
        pull_cfg_orphans(args, &mut inp, &base_order_names, &cfg.data_order, &yml, &known, report)?;

    let scan_dirs = inputs::all_scan_dirs(&cfg.data_order, &values(&inp.raw_toml_data_inserts), &values(&inp.data_inserts))?;
    let custom_data_dirs = inputs::pending_custom_dirs(&values(&inp.raw_toml_data_inserts), &values(&inp.data_inserts));
    let mut anchors = Anchors::new();
    let (final_order, predicate_warnings) = sort_subset(
        args,
        &inp.subset,
        &base_order_names,
        &cfg.data_order,
        &scan_dirs,
        &inp.subset_origins,
        &mut anchors,
        report,
    )?;

    let mut yml_warnings = yml.warnings.clone();
    if !yml.entries.is_empty() {
        yml_warnings.extend(momw::post_sort_findings(
            final_order.as_deref(),
            &base_order_names,
            &yml.entries,
            &yml.curated_set,
            &yml.curated_order,
            &yml.declared_lower,
            args.list_name.as_deref().filter(|n| !n.is_empty()),
        ));
        if yml_warnings.is_empty() {
            report.line("\n  No plugin-order.yml warnings.");
        } else {
            report.section(format!("{} PLUGIN-ORDER.YML WARNING(S) -- read-only, not enforced", yml_warnings.len()));
            let order = final_order.as_deref().unwrap_or(&base_order_names);
            for w in &yml_warnings {
                report.warn("yml", format!("\n{w}"), named_in(w, order));
            }
        }
    }

    data_path_order_warnings(args, &cfg.data_order, report)?;

    let active: &[String] = match &final_order {
        Some(order) if !order.is_empty() => order,
        _ => &base_order_names,
    };
    let (master_warnings, master_problem_plugins) = check_masters(active, &scan_dirs, &inp.subset_origins, report);
    staleness_watchdog(active, &scan_dirs, report);
    report.push(ItemKind::Scans, "");

    let empty = Vec::new();
    let data_result = plan_data_paths(
        args,
        &mut inp.data_inserts,
        &cfg.data_order,
        final_order.as_deref().unwrap_or(&empty),
        report,
    )?;

    if args.sort_data_paths {
        // the Python's two lists hold the same insert objects then, so the anchors just inferred
        // show in both
        inp.raw_toml_data_inserts.clone_from(&inp.data_inserts);
    }

    let plan = Plan {
        lines: cfg.lines,
        content_positions: cfg.content_positions,
        data_positions: cfg.data_positions,
        final_order: final_order.clone().unwrap_or_default(),
        data_result,
        new_groundcover,
        subset: inp.subset,
        curated_set: {
            let mut c: Vec<String> = yml.curated_set.iter().cloned().collect();
            c.sort();
            c
        },
        base_order_names,
        data_order: cfg.data_order,
        data_inserts: inp.data_inserts,
        raw_toml_data_inserts: inp.raw_toml_data_inserts,
        orphan_data_paths,
        original_toml_blocks: inp.original_toml_blocks,
        original_content_values: inp.original_content_values,
        replace_dest_names: inp.replace_dest_names,
        custom_anchors: anchors,
    };
    Ok(Computed {
        plan,
        sorted: final_order.is_some(),
        predicate_warnings,
        yml_warnings,
        master_warnings,
        master_problem_plugins,
        subset_origins: inp.subset_origins,
        scan_dirs,
        custom_data_dirs,
        report: std::mem::take(report),
    })
}

// ---------------------------------------------------------------- the export

/// What an export is told besides the plan (the Python's `write_plan` arguments).
#[derive(Clone, Debug, Default)]
pub struct ExportArgs {
    /// `--write-cfg`: the cfg (the plan's own) is patched.
    pub write_cfg: bool,
    /// The cfg's path, as the Python prints it.
    pub cfg: String,
    /// `--emit-toml`, as the Python prints it.
    pub emit_toml: Option<String>,
    /// `--dry-run`.
    pub dry_run: bool,
    /// `--no-backup`.
    pub no_backup: bool,
    /// `--sort-data-paths`.
    pub sort_data_paths: bool,
    /// `--list-name`.
    pub list_name: Option<String>,
    /// The `.bak` stamp, `YYYYMMDD-HHMMSS` in the user's local time.
    pub stamp: String,
    /// The TOML's line endings.
    pub toml_line_ending: Option<crate::export::LineEnding>,
}

/// What the summary counts that the plan does not hold.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    /// The predicate rules' warnings.
    pub rule_warnings: usize,
    /// MOMW's file's warnings.
    pub yml_warnings: usize,
}

/// An export and what it said.
#[derive(Clone, Debug)]
pub struct Exported {
    /// What was written.
    pub written: crate::export::Written,
    /// What the Python prints for it (the preview's place marked).
    pub report: Report,
}

/// Write out a plan as `write_plan` does, with its report: the opted-out removals, the orders
/// the person changed, what was written (and backed up), and the summary. `final_order` and
/// `data_order` override the plan's, `disabled_*` are what the person opted out of.
///
/// # Errors
/// [`crate::export::ExportError`].
pub fn export(
    args: &ExportArgs,
    plan: &Plan,
    counts: Counts,
    final_order: Option<&[String]>,
    data_order: Option<&[String]>,
    disabled_plugins: &[String],
    disabled_data: &[String],
) -> Result<Exported, crate::export::ExportError> {
    let cfg_path = PathBuf::from(&args.cfg);
    let toml_path = args.emit_toml.as_ref().map(PathBuf::from);
    let targets = crate::export::Targets {
        write_cfg: args.write_cfg.then_some(cfg_path.as_path()),
        emit_toml: toml_path.as_deref(),
        dry_run: args.dry_run,
        no_backup: args.no_backup,
        sort_data_paths: args.sort_data_paths,
        list_name: args.list_name.as_deref(),
        stamp: &args.stamp,
        toml_line_ending: args.toml_line_ending.unwrap_or_else(crate::export::LineEnding::platform),
    };
    let written = crate::export::write_plan(&targets, plan, final_order, data_order, disabled_plugins, disabled_data)?;
    let mut report = Report::default();
    if !written.remove_content.is_empty() || !written.remove_data.is_empty() {
        report.subsection("opted-out items already in cfg -> removeContent/removeData");
        for p in &written.remove_content {
            report.line(format!("  removeContent: {p}"));
        }
        for d in &written.remove_data {
            report.line(format!("  removeData: {d}"));
        }
    }
    let final_order_v: &[String] = final_order.unwrap_or(&plan.final_order);
    if !final_order_v.is_empty() && final_order_v != plan.final_order.as_slice() {
        report.subsection("content= order being exported (manually adjusted)");
        for n in final_order_v {
            report.line(format!("  content={n}"));
        }
    }
    if let Some(result) = &written.data_result {
        let lines: Vec<&String> = result.iter().map(|(l, _, _)| l).collect();
        let planned: Vec<&String> = plan.data_result.iter().flatten().map(|(l, _, _)| l).collect();
        if lines != planned {
            report.subsection("data= order being exported (manually adjusted)");
            for l in lines {
                report.line(format!("  {l}"));
            }
        }
    }
    report.section("WRITING OUTPUT");
    if args.write_cfg {
        if args.dry_run {
            report.line("\n--- DRY RUN: no files written ---");
        } else {
            if let Some(b) = &written.cfg_backup {
                report.line(format!("Backup written: {b}"));
            }
            report.line(format!("Wrote updated: {}", args.cfg));
        }
    } else {
        report.line("  openmw.cfg left untouched (pass --write-cfg to patch it directly)");
    }
    if let Some(path) = &args.emit_toml {
        for m in &written.messages {
            report.line(m.clone());
        }
        report.subsection("configurator preview (simulated apply)");
        let user_norms: Vec<String> =
            written.user_data_values.iter().filter(|v| !v.is_empty()).map(|v| crate::export::normalize_data_path(v)).collect();
        let preview = crate::apply::preview_configurator_result(
            &written.cfg_lines,
            written.toml_text.as_deref().unwrap_or_default(),
            final_order_v,
            &plan.subset,
            &user_norms,
            args.list_name.as_deref(),
        );
        match preview {
            Ok((_, lines)) => {
                for l in lines {
                    report.line(l);
                }
            }
            // the Python's preview prints here instead (the files are already written)
            Err(_) => report.push(ItemKind::Preview, ""),
        }
        if args.dry_run {
            report.line(format!(
                "\n  DRY RUN: would write {path}\n{}",
                written.toml_text.as_deref().unwrap_or_default()
            ));
        } else {
            if let Some(b) = &written.toml_backup {
                report.line(format!("Backup written: {b}"));
            }
            report.line(format!("  Wrote corrected customizations: {path}"));
        }
    }
    if !args.write_cfg && args.emit_toml.is_none() {
        report.line(
            "\n  NOTE: nothing was written -- this was a preview. Pass --write-cfg and/or --emit-toml to save the \
             result.",
        );
    }
    report.section("SUMMARY");
    report.line(format!("  Plugins sorted:        {}", plan.subset.len()));
    report.line(format!(
        "  Data paths inserted:   {}",
        if args.sort_data_paths { plan.data_inserts.len() } else { 0 }
    ));
    report.line(format!("  Rule warnings raised:  {}", counts.rule_warnings));
    report.line(format!("  plugin-order.yml warnings: {}", counts.yml_warnings));
    let yes = |b: bool| if b { "yes" } else { "no" };
    report.line(format!("  openmw.cfg written:    {}", yes(written.wrote_cfg)));
    report.line(format!("  customizations.toml written: {}", yes(written.wrote_toml)));
    Ok(Exported { written, report })
}
