//! Merge Lands, whole (`land/service.py`'s `build_merged_lands`, its native path): a load
//! order's terrain merged into `Merged Lands.esp`, the progress said line by line.
//!
//! Everything it does is additive. The plugins are never modified; the output is one new
//! file beside a `.mergedlands.toml` marking it generated, and deleting it restores the
//! previous behaviour exactly.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use tes3::esp::TES3Object;

use crate::emit::{attach_texture_indices, build_landscape_record_for, build_texture_records, plugin_bytes};
use crate::landmass::{build_reference, Landmass, PluginRecords};
use crate::meta::{self, PluginMeta, Strategy, LAYER_NAMES};
use crate::pipeline::{finish_timed, merge_landmass, DiffModel, MergeOutcome};
use crate::read::{read_terrain_records, Record};
use crate::textures::{compact_textures, KnownTextures};
use crate::grid::Value;

/// The name the community expects.
pub const DEFAULT_NAME: &str = "Merged Lands.esp";

/// What a merge produced (`MergeResult`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeResult {
    pub output: Option<PathBuf>,
    pub cells_written: usize,
    pub textures_written: usize,
    pub contested: usize,
    pub major: usize,
    pub seam_vertices: usize,
    pub slope_vertices: usize,
    pub clamped: usize,
    pub dropped: usize,
    pub borrowed: usize,
    /// The progress report, as the Python says it.
    pub lines: Vec<String>,
    /// What the Python logs rather than reports (records left out, untranslated textures).
    pub notes: Vec<String>,
}

/// The error of a merge stopped through `MergeOptions::cancel`, before it wrote anything.
pub const CANCELLED: &str = "cancelled: nothing was written";

/// The options `build_merged_lands` takes.
#[derive(Clone, Debug)]
pub struct MergeOptions {
    pub output: Option<PathBuf>,
    pub strategy: Strategy,
    pub dry_run: bool,
    pub substitute_unknown_textures: bool,
    pub verbose: bool,
    /// What a plugin's change is measured against (`DiffModel::Masters` by default).
    pub model: DiffModel,
    /// Drop the cells the load order already loads as merged (the OpenMW fork's cleaning);
    /// off, the Python's rule: the cells the masters or a single mod already give.
    pub clean_against_load_order: bool,
    /// Plugins to leave out of the merge altogether, by file name (any case), or every plugin
    /// in a folder when the entry is a path (the fork's `ignore_plugins`,
    /// `ignore_plugins_from_path`). The output's own name is always left out.
    pub ignore: Vec<String>,
    /// Set from elsewhere to stop the merge: it is looked at between its steps (each master,
    /// each batch of mods read, before and after the merge, before the write), and a merge
    /// stopped writes nothing.
    pub cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Write terrain whose steps `VHGT` cannot hold, clamping them in the encoder, and say so
    /// (the Python's way), instead of stopping. On by default. Off, a step the slope limiter
    /// could not repair stops the merge before anything is written: a step clamped in the
    /// encoder is terrain the merge did not choose, and normals worked out beforehand would no
    /// longer describe it. The audit's acceptance list asks for the stop; the default is a
    /// choice to warn.
    pub allow_clamping: bool,
}

/// The fork's starting `ignore_plugins`: generated or non-land plugins, expensive to read and
/// with nothing to merge.
pub const DEFAULT_IGNORE: [&str; 6] = [
    "delta-merged.omwaddon",
    "deleted_groundcover.omwaddon",
    "S3LightFixes.omwaddon",
    "OMWLLFMod.omwaddon",
    "merged.omwaddon",
    "Merged Objects.esp",
];

impl Default for MergeOptions {
    fn default() -> Self {
        MergeOptions {
            output: None,
            strategy: Strategy::Auto,
            dry_run: false,
            substitute_unknown_textures: true,
            verbose: false,
            model: DiffModel::Masters,
            clean_against_load_order: true,
            ignore: DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect(),
            cancel: None,
            allow_clamping: true,
        }
    }
}

fn coords(c: (i64, i64)) -> String {
    format!("({}, {})", c.0, c.1)
}

/// Masters and mods, each in load order; `.omwscripts` in neither (`_split_order`).
///
/// The masters are the reference terrain: the run of `.esm` / `.omwgame` files the order
/// starts with. A master that comes after a mod is a mod to the merge, taken in its place in
/// the order, because OpenMW's precedence is the order and not the file's extension (WG-A05):
/// splitting every `.esm` off to the reference put a late master under the mods before it.
/// An order that starts with a mod has no such run, and then every `.esm` is a master as
/// before, so the merge has a reference to work from.
pub fn split_order(names: &[String]) -> (Vec<String>, Vec<String>) {
    let plugins: Vec<&String> = names.iter().filter(|n| !n.to_lowercase().ends_with(".omwscripts")).collect();
    let is_master = |n: &str| {
        let l = n.to_lowercase();
        l.ends_with(".esm") || l.ends_with(".omwgame")
    };
    let leading = plugins.iter().take_while(|n| is_master(n)).count();
    if leading == 0 {
        let masters = plugins.iter().filter(|n| is_master(n)).map(|n| (*n).clone()).collect();
        let mods = plugins.iter().filter(|n| !is_master(n)).map(|n| (*n).clone()).collect();
        return (masters, mods);
    }
    (plugins[..leading].iter().map(|n| (*n).clone()).collect(), plugins[leading..].iter().map(|n| (*n).clone()).collect())
}

/// The data folder holding a plugin, the *last* that does, as OpenMW resolves it, matching
/// the name case-insensitively (`resolve_plugin`).
/// Every file of the data folders by its lowercase name, the last folder holding one
/// winning: [`resolve_plugin`]'s answer for any name, with each folder listed once.
/// (Static, 2026-10-27: resolving each of 621 plugins listed all 1233 folders again -
/// 44 s for the masters, 85 s before the first mod was read, 15 s to size the masters.)
pub struct PluginIndex(HashMap<String, PathBuf>);

impl PluginIndex {
    pub fn new(dirs: &[PathBuf]) -> PluginIndex {
        let mut map = HashMap::new();
        for d in dirs {
            let Ok(entries) = std::fs::read_dir(d) else { continue };
            for e in entries.flatten() {
                if e.file_type().is_ok_and(|t| t.is_file() || (t.is_symlink() && e.path().is_file())) {
                    map.insert(e.file_name().to_string_lossy().to_lowercase(), e.path());
                }
            }
        }
        PluginIndex(map)
    }
    pub fn get(&self, name: &str) -> Option<PathBuf> {
        self.0.get(&name.to_lowercase()).cloned()
    }
}

/// Where Merge Lands writes by default: the data folder holding the most of the load order's
/// plugins (the game's own `Data Files` on most setups), the first such in the cfg's order;
/// `None` when no folder holds any. The Tools page offers it, and the command line uses it.
pub fn home_folder(dirs: &[PathBuf], load_order: &[String]) -> Option<PathBuf> {
    let mut best: Option<&PathBuf> = None;
    let mut best_count = 0;
    for d in dirs {
        let count = load_order.iter().filter(|n| d.join(n).is_file()).count();
        if count > best_count {
            best = Some(d);
            best_count = count;
        }
    }
    best.cloned()
}

pub fn resolve_plugin(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let wanted = name.to_lowercase();
    let mut found = None;
    for d in dirs {
        let candidate = d.join(name);
        if candidate.is_file() {
            found = Some(candidate);
            continue;
        }
        let Ok(entries) = std::fs::read_dir(d) else { continue };
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().to_lowercase() == wanted && e.path().is_file() {
                found = Some(e.path());
                break;
            }
        }
    }
    found
}

/// One plugin's terrain records (`_records_via` then `_records_natively`, the native path):
/// none, without a failure, for a plugin with no terrain.
fn records_of(path: &Path) -> (Vec<Record>, String) {
    // The file read once (it was read twice, after a scan of every byte for the two tags).
    match read_terrain_records(path) {
        Ok(r) => (r, String::new()),
        Err(first) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let why = format!("reading {name} directly failed: {first}");
            match read_terrain_records(path) {
                Ok(r) => (r, String::new()),
                Err(again) => (Vec::new(), format!("{why}; reading it directly also failed: {again}")),
            }
        }
    }
}

/// A plugin's settings in one line (`_describe_meta`).
fn describe_meta(m: &PluginMeta) -> String {
    let layers: Vec<String> = LAYER_NAMES
        .iter()
        .map(|(n, _)| {
            let s = m.settings_for(n);
            format!("{n}: {}/{}", if s.included { "on" } else { "OFF" }, s.conflict_strategy.name())
        })
        .collect();
    format!("meta_type={}; {}", m.meta_type, layers.join("; "))
}

/// The plugins the merged file depends on, in load order (`_contributors`).
fn contributors(outcome: &MergeOutcome, texture_sources: &[String], reference: &Landmass, load_order: &[String]) -> Vec<String> {
    let mut set: BTreeSet<String> = texture_sources.iter().cloned().collect();
    for (c, cell) in &outcome.cells {
        set.extend(cell.editors.iter().cloned());
        if let Some(origin) = reference.sources.get(c)
            && !origin.is_empty()
        {
            set.insert(origin.clone());
        }
    }
    let seen: BTreeSet<String> = load_order.iter().map(|n| n.to_lowercase()).collect();
    let mut ordered: Vec<String> = load_order.iter().filter(|n| set.contains(*n)).cloned().collect();
    ordered.extend(set.iter().filter(|n| !seen.contains(&n.to_lowercase())).cloned());
    ordered
}

/// Reads the plugins' terrain records on several threads and answers them in the order given.
/// The biggest are taken first and each thread takes the next as it finishes, so a big plugin
/// at the end of the load order is not left to one thread after the rest are done (a batch of
/// thirty-two waited on its slowest, and the last batch took 1.8 s of 14.5 s on a 619-plugin
/// order). `progress(done, total)` is called on this thread, every thirty-second plugin and at
/// the end. Once `cancel` is set no more are started: the ones not read answer as nothing, and
/// the caller stops.
fn read_all(paths: &[&Path], cancel: Option<&std::sync::atomic::AtomicBool>, progress: &mut dyn FnMut(usize, usize)) -> Vec<(Vec<Record>, String)> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let total = paths.len();
    if total == 0 {
        return Vec::new();
    }
    let mut by_size: Vec<usize> = (0..total).collect();
    by_size.sort_by_key(|&i| std::cmp::Reverse(std::fs::metadata(paths[i]).map(|m| m.len()).unwrap_or(0)));
    // Not every core: the window and the Editor's own reads keep two.
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(2)).clamp(2, 8).min(total);
    let next = AtomicUsize::new(0);
    let mut out: Vec<Option<(Vec<Record>, String)>> = (0..total).map(|_| None).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        for _ in 0..threads {
            let tx = tx.clone();
            let (next, by_size) = (&next, &by_size);
            s.spawn(move || {
                loop {
                    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                        break;
                    }
                    let Some(&i) = by_size.get(next.fetch_add(1, Ordering::Relaxed)) else { break };
                    let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| records_of(paths[i])))
                        .unwrap_or_else(|_| (Vec::new(), "the reader failed on it".to_string()));
                    if tx.send((i, read)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut done = 0;
        for (i, read) in rx {
            out[i] = Some(read);
            done += 1;
            if done % 32 == 0 || done == total {
                progress(done, total);
            }
        }
    });
    out.into_iter().map(Option::unwrap_or_default).collect()
}

/// Merge a load order's terrain and write `Merged Lands.esp` (`build_merged_lands`).
/// `say` is called with each progress line as it happens. `Err` is the Python's
/// `MergeServiceError`, with the lines said before it in `lines`.
pub fn build_merged_lands(dirs: &[PathBuf], load_order: &[String], opts: &MergeOptions, say: &mut dyn FnMut(&str)) -> Result<MergeResult, (String, Vec<String>)> {
    let mut lines: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    macro_rules! say {
        ($($t:tt)*) => {{
            let line = format!($($t)*);
            say(&line);
            lines.push(line);
        }};
    }
    macro_rules! fail {
        ($($t:tt)*) => {
            return Err((format!($($t)*), lines))
        };
    }
    macro_rules! stop {
        () => {
            if opts.cancel.as_ref().is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
                fail!("{CANCELLED}");
            }
        };
    }
    if dirs.is_empty() {
        fail!("no data folder was given, so no plugin can be found");
    }
    let index = PluginIndex::new(dirs);
    let resolve_plugin = |name: &str, _dirs: &[PathBuf]| index.get(name);
    // The fork's filter: the output itself (a merge whose marker was lost would otherwise be
    // merged as a mod), and the ignore list, by name or by folder.
    let own = opts.output.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_else(|| DEFAULT_NAME.to_lowercase());
    let names: Vec<String> = opts.ignore.iter().filter(|e| !e.contains(['/', '\\'])).map(|e| e.trim().to_lowercase()).collect();
    let folders: Vec<String> = opts.ignore.iter().filter(|e| e.contains(['/', '\\'])).map(|e| e.trim().replace('\\', "/").trim_end_matches('/').to_lowercase()).collect();
    let mut left_out: Vec<String> = Vec::new();
    let order: Vec<String> = load_order
        .iter()
        .filter(|n| {
            let low = n.to_lowercase();
            let by_folder = !folders.is_empty()
                && resolve_plugin(n, dirs).is_some_and(|p| {
                    let p = p.to_string_lossy().replace('\\', "/").to_lowercase();
                    folders.iter().any(|f| p.starts_with(&format!("{f}/")))
                });
            let out = low == own || names.contains(&low) || by_folder;
            if out {
                left_out.push((*n).clone());
            }
            !out
        })
        .cloned()
        .collect();
    let load_order = &order[..];
    let (master_names, mod_names) = split_order(load_order);
    if master_names.is_empty() {
        fail!("no masters in the load order. Without them there is no reference terrain to merge against, and every mod would look as though it had rewritten the world.");
    }
    say!("masters: {}", master_names.join(", "));
    say!("mods:    {}", mod_names.len());
    if !left_out.is_empty() {
        say!("  left out {} plugin(s): {}", left_out.len(), left_out.join(", "));
    }
    if opts.model == DiffModel::LoadOrder {
        say!("  each plugin measured against the load order before it");
    }

    let mut known = KnownTextures::default();
    say!("reading masters ...");
    let mut metas: Vec<(String, PluginMeta)> = Vec::new();
    let mut masters: Vec<PluginRecords> = Vec::new();
    // Found and measured one by one, in order; read together (they were read one at a time, the
    // biggest of the load order among them). What stopped the walk is told after the reads of
    // the masters before it, so the first failure in load order is the one said, as it was.
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    let mut early: Option<String> = None;
    for name in &master_names {
        stop!();
        let Some(path) = resolve_plugin(name, dirs) else {
            early = Some(format!(
                "master {name} is not in any of the {} data folder(s) searched. Every master has to be readable: they are the reference terrain the whole merge is measured against.",
                dirs.len()
            ));
            break;
        };
        match meta::load_meta(&path) {
            Ok(m) => metas.push((name.clone(), m)),
            Err(e) => {
                early = Some(format!("{e}"));
                break;
            }
        }
        found.push((name.clone(), path));
    }
    let master_paths: Vec<&Path> = found.iter().map(|(_, p)| p.as_path()).collect();
    let master_reads = read_all(&master_paths, opts.cancel.as_deref(), &mut |done, total| {
        if done == total {
            // Worded as the mods' progress is ("  read n/m plugin(s)"): the parity cases leave out
            // the lines of that shape, which are for the page and not the Python's.
            say(&format!("  read {total} master plugin(s)"));
        }
    });
    stop!();
    for ((name, _), (records, failure)) in found.iter().zip(master_reads) {
        if !failure.is_empty() {
            fail!("could not read master {name}: {failure}");
        }
        masters.push(PluginRecords { name: name.clone(), records });
    }
    if let Some(e) = early {
        fail!("{e}");
    }
    let meta_map: HashMap<String, PluginMeta> = metas.iter().cloned().collect();
    let (reference, mut loaded) = match build_reference(&masters, &mut known, &meta_map, &mut notes) {
        Ok(r) => r,
        Err(e) => fail!("{e}"),
    };
    say!("  reference landmass: {} cell(s), {} land texture(s)", reference.len(), known.len());

    say!("reading mods ...");
    let mut unreadable: Vec<String> = Vec::new();
    let mut resolved: Vec<(String, PathBuf)> = Vec::new();
    for name in &mod_names {
        let Some(path) = resolve_plugin(name, dirs) else {
            unreadable.push(format!("{name} (not found)"));
            continue;
        };
        match meta::load_meta(&path) {
            Ok(m) => metas.push((name.clone(), m)),
            Err(e) => fail!("{e}"),
        }
        resolved.push((name.clone(), path));
    }
    // Read together, the biggest first, results in load order (later plugins win).
    let mod_paths: Vec<&Path> = resolved.iter().map(|(_, p)| p.as_path()).collect();
    // Progress only (not a report line, so the report stays the Python's): the page shows it
    // while a big order is read.
    let reads = read_all(&mod_paths, opts.cancel.as_deref(), &mut |done, total| say(&format!("  read {done}/{total} plugin(s)")));
    stop!();
    let mut mods: Vec<PluginRecords> = Vec::new();
    for (index, ((name, _), (records, failure))) in resolved.iter().zip(reads).enumerate() {
        if !records.is_empty() {
            mods.push(PluginRecords { name: name.clone(), records });
        } else if !failure.is_empty() {
            unreadable.push(format!("{name} ({failure})"));
        }
        if (index + 1) % 50 == 0 {
            say!("  {}/{}", index + 1, resolved.len());
        }
    }
    if !unreadable.is_empty() {
        if opts.verbose {
            say!("  {} plugin(s) could not be read:", unreadable.len());
            for e in &unreadable {
                say!("    {e}");
            }
        } else {
            say!("  {} plugin(s) could not be read: {}", unreadable.len(), unreadable.iter().take(5).cloned().collect::<Vec<_>>().join(", "));
        }
    }
    // The Python's dict: a name read twice keeps its first place and its last settings.
    let mut first_seen: Vec<String> = Vec::new();
    for (n, _) in &metas {
        if !first_seen.contains(n) {
            first_seen.push(n.clone());
        }
    }
    let meta_map: HashMap<String, PluginMeta> = metas.iter().cloned().collect();
    let patched: Vec<&String> = first_seen.iter().filter(|n| !meta_map[*n].layers.is_empty()).collect();
    if !patched.is_empty() {
        let listed = patched.iter().take(12).map(|s| s.as_str()).collect::<Vec<_>>().join(", ") + if patched.len() > 12 { ", ..." } else { "" };
        say!("  {} plugin(s) carry .mergedlands.toml settings: {listed}", patched.len());
        if opts.verbose {
            for n in &patched {
                say!("    {n}: {}", describe_meta(&meta_map[*n]));
            }
        }
    }

    stop!();
    say!("merging ...");
    let mut outcome = match merge_landmass(&reference, &mods, &mut known, &meta_map, opts.strategy, opts.model, opts.clean_against_load_order.then_some(&mut loaded)) {
        Ok(o) => o,
        Err(e) => fail!("{e}"),
    };
    stop!();
    notes.append(&mut outcome.notes);
    for (name, why) in &outcome.skipped_plugins {
        say!("  skipped {name}: {why}");
    }
    say!("  {} modified cell(s)", outcome.cells.len());
    let times = match finish_timed(&mut outcome, &reference, opts.clean_against_load_order.then_some(&loaded)) {
        Ok(t) => t,
        Err(e) => fail!("{e}"),
    };
    stop!();
    // The steps' times with Verbose (the Tools page's box, the command line's -v): the log
    // is otherwise the Python's, line for line.
    if opts.verbose {
        say!("  finished in {}", times.iter().map(|(n, d)| format!("{n} {:.2} s", d.as_secs_f64())).collect::<Vec<_>>().join(", "));
    }
    say!("  seam repair moved {} vertex/vertices (widest gap {} units)", outcome.seams.total(), outcome.seams.largest_gap);
    if outcome.borrowed > 0 {
        say!("  brought in {} untouched cell(s) to reconcile their borders", outcome.borrowed);
    }
    if outcome.slopes.adjusted > 0 {
        say!("  slope limiter moved {} vertex/vertices in {} pass(es)", outcome.slopes.adjusted, outcome.slopes.passes);
    }
    if outcome.slopes.pinned > 0 {
        say!("  {} adjustment(s) refused: the vertex is shared with terrain outside the merge", outcome.slopes.pinned);
    }
    say!("  cleaning dropped {} redundant cell(s)", outcome.cleaning.dropped());
    if let Some(worst) = outcome.seams.tears.first() {
        let neighbour = worst.right.map(coords).unwrap_or_else(|| "terrain outside the merge".into());
        fail!(
            "{} cell border(s) still disagree after seam repair -- worst between {} and {neighbour}, {} vertex/vertices apart by up to {} units. Nothing was written: this would be a visible tear in the landscape, and it is a defect in the merge rather than in your load order. Please report it.",
            outcome.seams.tears.len(),
            coords(worst.left),
            worst.vertices,
            worst.worst
        );
    }

    let mut result = MergeResult {
        contested: outcome.cells.values().map(|c| c.contested).sum(),
        major: outcome.cells.values().map(|c| c.major).sum(),
        seam_vertices: outcome.seams.total(),
        slope_vertices: outcome.slopes.adjusted,
        dropped: outcome.cleaning.dropped(),
        borrowed: outcome.borrowed,
        ..Default::default()
    };

    // _build_records
    let mut lands = Vec::new();
    let mut pending: Vec<(usize, Vec<Value>)> = Vec::new();
    let mut used: BTreeSet<Value> = BTreeSet::new();
    let mut clamped = 0;
    let mut clamped_cells: Vec<(i64, i64)> = Vec::new();
    for (c, cell) in &outcome.cells {
        match build_landscape_record_for(*c, cell.heights.as_deref(), cell.normals.as_deref(), cell.world_map.as_deref(), cell.colors.as_deref(), cell.textures.is_some()) {
            Ok((record, clamps)) => {
                clamped += clamps.len();
                if !clamps.is_empty() {
                    clamped_cells.push(*c);
                }
                if let Some(t) = &cell.textures {
                    used.extend(t.iter().copied());
                    pending.push((lands.len(), t.clone()));
                }
                lands.push(record);
            }
            // A cell the merge kept and cannot write is a merge that did not finish: carrying on
            // would save a plugin missing terrain the load order needs, and say it had worked.
            Err(e) => fail!("cell {} could not be written: {e} Nothing was written.", coords(*c)),
        }
    }
    if clamped > 0 && !opts.allow_clamping {
        // The heights were finalized and the slopes limited to what VHGT holds before this, so a
        // step left over is one the merge could not move (against terrain outside it): saved,
        // the encoder would change heights the normals were already worked out from.
        let first = clamped_cells.iter().take(5).map(|c| coords(*c)).collect::<Vec<_>>().join(", ");
        fail!(
            "{clamped} vertex/vertices in {} cell(s) (first: {first}) have a step VHGT cannot store and the slope limiter could not repair it. Nothing was written: saving it would make the encoder clamp the terrain after its normals were worked out. Please report it.",
            clamped_cells.len()
        );
    }
    if clamped > 0 {
        let first = clamped_cells.iter().take(5).map(|c| coords(*c)).collect::<Vec<_>>().join(", ");
        say!("  {clamped} vertex/vertices in {} cell(s) (first: {first}) have a step VHGT cannot store that the slope limiter could not repair, and were clamped; their normals were worked out before it", clamped_cells.len());
    }
    result.clamped = clamped;
    if lands.is_empty() {
        say!("nothing to merge; no plugin written");
        result.lines = lines;
        result.notes = notes;
        return Ok(result);
    }

    // _finish_textures
    let mut texture_records = Vec::new();
    let mut texture_sources: Vec<String> = Vec::new();
    if !pending.is_empty() {
        let (mapping, kept, unresolved) = compact_textures(&known, &used, opts.substitute_unknown_textures);
        for (i, rows) in &pending {
            let written: Vec<Value> = rows.iter().map(|v| mapping.get(v).copied().unwrap_or(*v)).collect();
            attach_texture_indices(&mut lands[*i], &written);
        }
        say!("  land textures: {} known, {} painted by the merge", known.len(), kept.len());
        if !unresolved.is_empty() {
            let count = unresolved.len();
            if opts.substitute_unknown_textures {
                say!("  {count} texture index/indices no LTEX defines: substituted with the fallback (smallest painted texture) so the plugin loads -- a missing master");
            } else {
                say!("  {count} texture index/indices no LTEX defines: passed through unchanged, so the plugin carries a dangling index -- a missing master");
            }
        }
        let sources: BTreeSet<String> = kept.iter().filter(|t| !t.source.is_empty()).map(|t| t.source.clone()).collect();
        texture_sources = sources.into_iter().collect();
        texture_records = build_texture_records(&kept);
    }
    result.cells_written = lands.len();
    result.textures_written = texture_records.len();
    let total = lands.len() + texture_records.len();

    if opts.dry_run {
        say!("dry run: nothing was written");
        result.lines = lines;
        result.notes = notes;
        return Ok(result);
    }
    let mut declared = contributors(&outcome, &texture_sources, &reference, load_order);
    if declared.is_empty() {
        declared = master_names.clone();
    }
    stop!();
    say!("  declaring {} master(s): the plugins that actually contributed", declared.len());
    let target = opts.output.clone().unwrap_or_else(|| dirs[0].join(DEFAULT_NAME));

    // _write
    let mut masters_sized = Vec::new();
    for name in &declared {
        let Some(path) = resolve_plugin(name, dirs) else { fail!("cannot find master {name} to measure it") };
        match std::fs::metadata(&path) {
            Ok(m) => masters_sized.push((name.clone(), m.len())),
            Err(e) => fail!("cannot measure master {name}: {e}"),
        }
    }
    let mut objects: Vec<TES3Object> = texture_records.into_iter().map(TES3Object::LandscapeTexture).collect();
    objects.extend(lands.into_iter().map(TES3Object::Landscape));
    let encoding = std::time::Instant::now();
    let bytes = match plugin_bytes(objects, &masters_sized) {
        Ok(b) => b,
        Err(e) => fail!("could not encode the merged plugin natively: {e}"),
    };
    let encoded = encoding.elapsed();
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let writing = std::time::Instant::now();
    if let Err(e) = std::fs::write(&target, &bytes) {
        fail!("could not encode the merged plugin natively: {e}");
    }
    // Verbose: the encoding and the disk apart (a real order's write took 4.4 s, 2026-11-10).
    if opts.verbose {
        say!("  encoded in {:.2} s, written to disk in {:.2} s", encoded.as_secs_f64(), writing.elapsed().as_secs_f64());
    }
    say!("wrote {} ({} bytes, {} records)", target.display(), bytes.len(), total + 1);
    match meta::write_merged_marker(&target) {
        Ok(marker) => say!("wrote {} so a later merge ignores this file", marker.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
        Err(e) => fail!("{e}"),
    }
    result.output = Some(target);
    result.lines = lines;
    result.notes = notes;
    Ok(result)
}

#[cfg(test)]
mod read_all_tests {
    use super::*;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn record(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend([0u8; 8]);
        v.extend_from_slice(body);
        v
    }

    fn plugin(lands: i32) -> Vec<u8> {
        let mut raw = record(b"TES3", &sub(b"HEDR", &[0u8; 300]));
        for g in 0..lands {
            let mut grid = g.to_le_bytes().to_vec();
            grid.extend(7i32.to_le_bytes());
            let mut heights = 1.5f32.to_le_bytes().to_vec();
            heights.extend((0..4232 - 4).map(|i| (i % 251) as u8));
            let mut body = sub(b"INTV", &grid);
            body.extend(sub(b"DATA", &9u32.to_le_bytes()));
            body.extend(sub(b"VHGT", &heights));
            raw.extend(record(b"LAND", &body));
        }
        raw
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wg-readall-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn answers_in_the_order_given_what_reading_one_at_a_time_gives() {
        let dir = scratch("order");
        let mut paths: Vec<PathBuf> = Vec::new();
        for i in 0..70 {
            let p = dir.join(format!("p{i:02}.esp"));
            // Sizes that do not follow the order: the biggest first is not load order.
            match i % 7 {
                5 => std::fs::write(&p, b"TES4 not a morrowind plugin").unwrap(),
                6 => {} // missing: not written
                k => std::fs::write(&p, plugin(1 + ((i * 3 + k) % 9))).unwrap(),
            }
            paths.push(p);
        }
        let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let mut seen: Vec<(usize, usize)> = Vec::new();
        let got = read_all(&refs, None, &mut |d, t| seen.push((d, t)));
        let want: Vec<(Vec<Record>, String)> = refs.iter().map(|p| records_of(p)).collect();
        assert_eq!(got, want, "the same records, failures and order");
        assert!(got.iter().any(|(r, _)| !r.is_empty()) && got.iter().any(|(_, f)| !f.is_empty()), "the sample has terrain and failures");
        assert_eq!(seen, vec![(32, 70), (64, 70), (70, 70)]);
        assert!(read_all(&[], None, &mut |_, _| panic!("nothing to say")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancel_starts_no_more_reads() {
        let dir = scratch("cancel");
        let p = dir.join("a.esp");
        std::fs::write(&p, plugin(2)).unwrap();
        let stop = std::sync::atomic::AtomicBool::new(true);
        let got = read_all(&[p.as_path(), p.as_path()], Some(&stop), &mut |_, _| {});
        assert_eq!(got, vec![(Vec::new(), String::new()), (Vec::new(), String::new())]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
