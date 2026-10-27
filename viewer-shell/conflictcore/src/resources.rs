//! Loose-file (VFS) conflicts (port step 4.3, slice 5): a path two or more data folders
//! provide (the later folder wins, as OpenMW's VFS orders them), whether the providers hold the
//! same bytes, and the report and CSV (`detect_resource_conflicts`, `format_resource_report`,
//! `write_resource_csv`).
//!
//! Plugin files are left out (`content=` orders them, not the VFS), as are the extensions
//! asked. The mesh analysis (`analyse_mesh_conflicts`, slice 6) is still the Python's: its
//! findings come back in as [`MeshColumns`] for the report and the CSV.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::records::csv_row;
use crate::vfs::{identical, lower, par_map, walk};

/// Extensions OpenMW treats as plugins, left out of the walk.
pub const PLUGIN_EXTS: [&str; 5] = [".esp", ".esm", ".omwaddon", ".omwgame", ".omwscripts"];

/// One loose-file conflict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// The path within each folder, lower case, `/` between names.
    pub path: String,
    /// The folders providing it, in load order.
    pub providers: Vec<String>,
    /// The last of them.
    pub winner: String,
    /// Whether one of them is one of the user's own folders.
    pub involves_subset: bool,
    /// Whether every provider holds the same bytes (`None`: not compared).
    pub identical: Option<bool>,
}

/// The scan's counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceStats {
    pub dirs: usize,
    pub files: usize,
    pub conflicts: usize,
    pub identical: usize,
    pub differing: usize,
}

fn folded(s: &str) -> String {
    lower(s.replace('\\', "/").trim_end_matches('/'))
}

/// The loose-file conflicts across `data_dirs` (load order, winner last). `subset_dirs`: the
/// user's own folders; `exclude_exts`: extensions to leave out (`.dds` or `dds`, any case);
/// `compare`: compare the bytes.
/// Differing files first, then the user's own, then by path.
pub fn detect(
    data_dirs: &[String],
    subset_dirs: &[String],
    exclude_exts: &[String],
    compare: bool,
) -> (Vec<Resource>, ResourceStats) {
    let mine: HashSet<String> = subset_dirs.iter().map(|s| folded(s)).collect();
    let mut skip: HashSet<String> = PLUGIN_EXTS.iter().map(|e| (*e).to_string()).collect();
    for e in exclude_exts {
        let e = lower(e.trim());
        if !e.is_empty() {
            skip.insert(if e.starts_with('.') { e } else { format!(".{e}") });
        }
    }
    let roots: Vec<PathBuf> = data_dirs.iter().map(PathBuf::from).collect();
    let walked = par_map(&roots, |r| walk(r, &skip, true));
    let mut dirs: Vec<String> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut providers: HashMap<String, Vec<usize>> = HashMap::new();
    for (d, rels) in data_dirs.iter().zip(walked) {
        let Some(rels) = rels else { continue };
        dirs.push(d.clone());
        let di = dirs.len() - 1;
        for rel in rels {
            match providers.get_mut(&rel) {
                Some(list) => {
                    if list.last() != Some(&di) {
                        list.push(di);
                    }
                }
                None => {
                    order.push(rel.clone());
                    providers.insert(rel, vec![di]);
                }
            }
        }
    }
    let mut out: Vec<Resource> = order
        .iter()
        .filter_map(|rel| {
            let idxs = &providers[rel];
            (idxs.len() >= 2).then(|| {
                let prov: Vec<String> = idxs.iter().map(|&i| dirs[i].clone()).collect();
                Resource {
                    path: rel.clone(),
                    winner: prov.last().cloned().unwrap_or_default(),
                    involves_subset: prov.iter().any(|p| mine.contains(&folded(p))),
                    providers: prov,
                    identical: None,
                }
            })
        })
        .collect();
    if compare {
        let groups: Vec<Vec<PathBuf>> =
            out.iter().map(|c| c.providers.iter().map(|d| Path::new(d).join(&c.path)).collect()).collect();
        let same = par_map(&groups, |g| identical(g));
        for (c, s) in out.iter_mut().zip(same) {
            c.identical = Some(s);
        }
    }
    out.sort_by(|a, b| {
        (a.identical == Some(true), !a.involves_subset, &a.path).cmp(&(b.identical == Some(true), !b.involves_subset, &b.path))
    });
    let identical_n = out.iter().filter(|c| c.identical == Some(true)).count();
    let stats = ResourceStats {
        dirs: dirs.len(),
        files: providers.len(),
        conflicts: out.len(),
        identical: identical_n,
        differing: out.len() - identical_n,
    };
    (out, stats)
}

/// The mesh analysis's columns for one conflict (the Python's `_mesh_columns`; blank: not
/// established), and its note for the report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MeshColumns {
    pub note: String,
    pub lost_collision: String,
    pub lost_animation: String,
    pub triangle_ratio: String,
    /// The line the report marks the conflict with (`_mesh_note`; empty: none).
    pub report_note: String,
}

/// The report (`format_resource_report`): the user's own only with `subset_only`, at most
/// `limit` listed (0: all). `mesh`: the analysis's columns by conflict, where it ran.
pub fn report(conflicts: &[Resource], stats: &ResourceStats, mesh: &[Option<MeshColumns>], subset_only: bool, limit: usize) -> String {
    let note = |i: usize| mesh.get(i).and_then(Option::as_ref).map(|m| m.report_note.as_str()).filter(|n| !n.is_empty());
    let shown: Vec<usize> = (0..conflicts.len()).filter(|&i| conflicts[i].involves_subset || !subset_only).collect();
    let n_sub = conflicts.iter().filter(|c| c.involves_subset).count();
    let mut lines = vec![format!(
        "Scanned {} data folder(s), {} loose file(s): {} conflicting file(s), {} involving your custom data paths.",
        stats.dirs, stats.files, stats.conflicts, n_sub
    )];
    if stats.identical > 0 {
        lines.push(format!(
            "  {} differ; {} are byte-identical across every provider and need no decision.",
            stats.differing, stats.identical
        ));
    }
    let flagged = (0..conflicts.len()).filter(|&i| note(i).is_some()).count();
    if flagged > 0 {
        lines.push(if flagged == 1 {
            format!("  {flagged} mesh conflict changes what the asset does; those lines are marked below.")
        } else {
            format!("  {flagged} mesh conflicts change what the asset does; those lines are marked below.")
        });
    }
    let capped = if limit == 0 { &shown[..] } else { &shown[..shown.len().min(limit)] };
    for &i in capped {
        let c = &conflicts[i];
        let star = if c.involves_subset { "* " } else { "  " };
        let same = if c.identical == Some(true) { "  [identical]" } else { "" };
        lines.push(format!("{star}{}   ({} providers, wins: {}){same}", c.path, c.providers.len(), c.winner));
        if let Some(n) = note(i) {
            lines.push(format!("      -> {n}"));
        }
    }
    if limit != 0 && shown.len() > limit {
        lines.push(format!("  ... and {} more (save the full report).", shown.len() - limit));
    }
    lines.join("\n")
}

/// The CSV (`write_resource_csv`).
pub fn csv(conflicts: &[Resource], mesh: &[Option<MeshColumns>]) -> String {
    let mut out = csv_row(&[
        "file_path",
        "providers",
        "winner",
        "involves_custom",
        "identical",
        "provider_folders",
        "mesh_note",
        "mesh_lost_collision",
        "mesh_lost_animation",
        "mesh_triangle_ratio",
    ]);
    let blank = MeshColumns::default();
    for (i, c) in conflicts.iter().enumerate() {
        let m = mesh.get(i).and_then(Option::as_ref).unwrap_or(&blank);
        let n = c.providers.len().to_string();
        let identical = match c.identical {
            None => "",
            Some(true) => "yes",
            Some(false) => "no",
        };
        out.push_str(&csv_row(&[
            &c.path,
            &n,
            &c.winner,
            if c.involves_subset { "yes" } else { "no" },
            identical,
            &c.providers.join(" -> "),
            &m.note,
            &m.lost_collision,
            &m.lost_animation,
            &m.triangle_ratio,
        ]));
    }
    out
}
