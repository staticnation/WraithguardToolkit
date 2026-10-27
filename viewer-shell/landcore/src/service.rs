//! Merge Lands, whole (`land/service.py`'s `build_merged_lands`, its native path): a load
//! order's terrain merged into `Merged Lands.esp`, the progress said line by line.
//!
//! Everything it does is additive. The plugins are never modified; the output is one new
//! file beside a `.mergedlands.toml` marking it generated, and deleting it restores the
//! previous behaviour exactly.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use tes3::esp::TES3Object;

use crate::emit::{attach_texture_indices, build_landscape_record, build_texture_records, plugin_bytes};
use crate::landmass::{build_reference, Landmass, PluginRecords};
use crate::meta::{self, PluginMeta, Strategy, LAYER_NAMES};
use crate::pipeline::{finish, merge_landmass, DiffModel, MergeOutcome};
use crate::read::{has_landscape, read_landscape_records, Record};
use crate::textures::{compact_textures, KnownTextures};

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
        }
    }
}

fn coords(c: (i64, i64)) -> String {
    format!("({}, {})", c.0, c.1)
}

/// Masters and mods, each in load order; `.omwscripts` in neither (`_split_order`).
pub fn split_order(names: &[String]) -> (Vec<String>, Vec<String>) {
    let plugins: Vec<&String> = names.iter().filter(|n| !n.to_lowercase().ends_with(".omwscripts")).collect();
    let is_master = |n: &str| {
        let l = n.to_lowercase();
        l.ends_with(".esm") || l.ends_with(".omwgame")
    };
    let masters = plugins.iter().filter(|n| is_master(n)).map(|n| (*n).clone()).collect();
    let mods = plugins.iter().filter(|n| !is_master(n)).map(|n| (*n).clone()).collect();
    (masters, mods)
}

/// The data folder holding a plugin, the *last* that does, as OpenMW resolves it, matching
/// the name case-insensitively (`resolve_plugin`).
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
    if !has_landscape(path) {
        return (Vec::new(), String::new());
    }
    match read_landscape_records(path) {
        Ok(r) => (r, String::new()),
        Err(first) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let why = format!("reading {name} directly failed: {first}");
            match read_landscape_records(path) {
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
    if dirs.is_empty() {
        fail!("no data folder was given, so no plugin can be found");
    }
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
    for name in &master_names {
        let Some(path) = resolve_plugin(name, dirs) else {
            fail!(
                "master {name} is not in any of the {} data folder(s) searched. Every master has to be readable: they are the reference terrain the whole merge is measured against.",
                dirs.len()
            );
        };
        match meta::load_meta(&path) {
            Ok(m) => metas.push((name.clone(), m)),
            Err(e) => fail!("{e}"),
        }
        let (records, failure) = records_of(&path);
        if !failure.is_empty() {
            fail!("could not read master {name}: {failure}");
        }
        masters.push(PluginRecords { name: name.clone(), records });
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
    // Read a few at a time, results in load order (later plugins win).
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4);
    let mut reads: Vec<(Vec<Record>, String)> = Vec::with_capacity(resolved.len());
    for chunk in resolved.chunks(threads.max(1) * 8) {
        let part: Vec<(Vec<Record>, String)> = std::thread::scope(|s| {
            let per = chunk.len().div_ceil(threads).max(1);
            let jobs: Vec<_> = chunk.chunks(per).map(|c| s.spawn(move || c.iter().map(|(_, p)| records_of(p)).collect::<Vec<_>>())).collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
        });
        reads.extend(part);
    }
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

    say!("merging ...");
    let mut outcome = match merge_landmass(&reference, &mods, &mut known, &meta_map, opts.strategy, opts.model, opts.clean_against_load_order.then_some(&mut loaded)) {
        Ok(o) => o,
        Err(e) => fail!("{e}"),
    };
    notes.append(&mut outcome.notes);
    for (name, why) in &outcome.skipped_plugins {
        say!("  skipped {name}: {why}");
    }
    say!("  {} modified cell(s)", outcome.cells.len());
    if let Err(e) = finish(&mut outcome, &reference, opts.clean_against_load_order.then_some(&loaded)) {
        fail!("{e}");
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
    let mut pending: Vec<(usize, Vec<i64>)> = Vec::new();
    let mut used: BTreeSet<i64> = BTreeSet::new();
    let mut clamped = 0;
    for (c, cell) in &outcome.cells {
        match build_landscape_record(*c, cell.heights.as_deref(), cell.normals.as_deref(), cell.world_map.as_deref(), cell.colors.as_deref()) {
            Ok((record, clamps)) => {
                clamped += clamps.len();
                if let Some(t) = &cell.textures {
                    used.extend(t.iter().copied());
                    pending.push((lands.len(), t.clone()));
                }
                lands.push(record);
            }
            Err(e) => say!("  cell {} could not be written: {e}", coords(*c)),
        }
    }
    if clamped > 0 {
        say!("  {clamped} vertex/vertices could not be expressed in VHGT and were clamped");
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
            let written: Vec<i64> = rows.iter().map(|v| mapping.get(v).copied().unwrap_or(*v)).collect();
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
    let bytes = match plugin_bytes(objects, &masters_sized) {
        Ok(b) => b,
        Err(e) => fail!("could not encode the merged plugin natively: {e}"),
    };
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&target, &bytes) {
        fail!("could not encode the merged plugin natively: {e}");
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
