//! Turning a mod manager's own files into an ordered overlay.
//!
//! This used to live in the page, where it depended on File System Access handles
//! a packaged app never obtains. Doing it here means one folder pick is enough:
//! everything else is read off disk.

use std::path::{Path, PathBuf};

/// A resolved install: overlay roots highest priority first, plus the file that
/// states the plugin load order.
#[derive(Debug, Default, Clone)]
pub struct Layout {
    pub roots: Vec<PathBuf>,
    pub order_file: PathBuf,
    pub label: String,
    pub warnings: Vec<String>,
    /// How many mods the manager says are switched **on**, and how many it skipped —
    /// disabled, unmanaged, or a separator line. Reported rather than left to be inferred
    /// from the number of roots, because "am I really only reading my active mods?" is a
    /// question a person with a thousand of them will ask, and the honest answer is a
    /// number the tool can show them. Zero for a plain Data Files folder, which has no
    /// such thing.
    pub mods_on: usize,
    pub mods_off: usize,
    /// The game's own folder (the parent of Data Files), when the layout knows it: where
    /// `mgeXE.toml` and `mge3\MGE.ini` live. None for an OpenMW install, which has no MGE.
    pub game_dir: Option<PathBuf>,
    /// The Mod Organizer folder, when this layout came from one. Round 17m: Root Builder
    /// keeps its build record under `plugins\data\RootBuilder\`, and that record is the
    /// only honest answer to "which d3d8.dll is deployed right now" — see `mge.rs`.
    pub mo2_base: Option<PathBuf>,
}

/// Qt writes `@ByteArray(...)` around values and escapes backslashes; strip both.
fn qt_value(raw: &str) -> String {
    let mut v = raw.trim().to_string();
    if let Some(inner) = v.strip_prefix("@ByteArray(").and_then(|s| s.strip_suffix(')')) {
        v = inner.to_string();
    }
    v = v.trim().trim_matches('"').to_string();
    v.replace("\\\\", "\\")
}

/// Reads `key = value` out of a flat ini, ignoring sections. Keys are matched
/// case-insensitively; the first occurrence wins.
fn ini_lookup(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('[') {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else { continue };
        if k.trim().eq_ignore_ascii_case(key) {
            return Some(qt_value(v));
        }
    }
    None
}

fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned().or_else(|| candidates.iter().find_map(|p| same_name_any_case(p)))
}

/// `p` under whatever case the disk has it. Windows does not care; a Linux filesystem
/// does, and a game folder that came through Proton or a mod manager can hold
/// `morrowind.ini` or `MORROWIND.INI`. Only tried after the exact name has missed.
fn same_name_any_case(p: &Path) -> Option<PathBuf> {
    let want = p.file_name()?.to_str()?.to_ascii_lowercase();
    let rd = std::fs::read_dir(p.parent()?).ok()?;
    rd.flatten().map(|e| e.path()).find(|q| {
        q.file_name().and_then(|n| n.to_str()).map(|n| n.to_ascii_lowercase() == want).unwrap_or(false)
    })
}

/// MO2's `%BASE_DIR%` placeholder, expanded — round 18be (I1).
///
/// Every directory setting in `ModOrganizer.ini` may be written as `%BASE_DIR%/mods`
/// rather than spelled out, and a portable instance moved to another drive is *only*
/// written that way. Qt writes forward slashes and the placeholder is upper case in
/// practice, but neither is guaranteed, so both are forgiven. A relative path with no
/// placeholder is resolved against the same folder, which is what MO2 does with one.
fn expand_base_dir(raw: &str, base_dir: &Path) -> PathBuf {
    let v = raw.trim().replace('\\', "/");
    let rest = if v.len() >= 10 && v[..10].eq_ignore_ascii_case("%BASE_DIR%") {
        Some(v[10..].trim_start_matches('/').to_string())
    } else {
        None
    };
    match rest {
        Some(r) if r.is_empty() => base_dir.to_path_buf(),
        Some(r) => base_dir.join(r),
        None => {
            let p = PathBuf::from(&v);
            if p.is_absolute() || v.len() >= 2 && v.as_bytes()[1] == b':' {
                p
            } else {
                base_dir.join(p)
            }
        }
    }
}

/// One of MO2's own directory settings, and whether the file actually stated it.
///
/// Round 18be (I1). These were never read: `mods\` and `profiles\` were simply assumed
/// to sit inside the folder the person picked. A portable instance keeping its mods on
/// another drive — which `ModOrganizer.ini` says plainly — was refused outright with
/// "does not look like an MO2 folder", and the quieter half was worse: an instance
/// reconfigured to point elsewhere but still holding an old `mods\` beside the ini
/// opened with the **wrong mod set**, silently, and every conflict resolved the way that
/// stale list said.
fn mo2_dir(ini: &str, key: &str, base_dir: &Path, fallback: &str) -> (PathBuf, bool) {
    match ini_lookup(ini, key).filter(|v| !v.trim().is_empty()) {
        Some(raw) => (expand_base_dir(&raw, base_dir), true),
        None => (base_dir.join(fallback), false),
    }
}


/// As `mo2`, but with the profile chosen explicitly rather than taken from
/// `ModOrganizer.ini`. Profiles can enable different mods and a different plugin
/// order, so which one is active changes the answer entirely.
pub fn mo2_profile(base: &Path, want: Option<&str>) -> Result<Layout, String> {
    let mut out = Layout { label: "MO2".into(), mo2_base: Some(base.to_path_buf()), ..Default::default() };

    /* Round 18be (I1): the file first, the folder layout second. `ModOrganizer.ini`
       states where this instance keeps its mods, its profiles and its overwrite, and
       until now none of it was read — see `mo2_dir`. Read before the check below,
       because on a portable instance the check is exactly what the file answers. */
    let org_ini = first_existing(&[base.join("ModOrganizer.ini")])
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    /* What `%BASE_DIR%` stands for. MO2 puts the instance's own folder here, which for
       the folder a person picks is the folder they picked; an instance that states
       something else and means it gets that instead. A stated base that is not there is
       not worth failing on by itself — the mods and profiles below are the real test. */
    let base_dir = match ini_lookup(&org_ini, "base_directory").filter(|v| !v.trim().is_empty()) {
        Some(v) => {
            let p = expand_base_dir(&v, base);
            if p.is_dir() {
                p
            } else {
                base.to_path_buf()
            }
        }
        None => base.to_path_buf(),
    };
    let (mods_dir, mods_stated) = mo2_dir(&org_ini, "mod_directory", &base_dir, "mods");
    let (profiles_dir, profiles_stated) = mo2_dir(&org_ini, "profiles_directory", &base_dir, "profiles");
    for (dir, stated, key) in [
        (&mods_dir, mods_stated, "mod_directory"),
        (&profiles_dir, profiles_stated, "profiles_directory"),
    ] {
        if dir.is_dir() {
            continue;
        }
        /* A folder the file *named* gets its own answer. Falling back to a `mods\` that
           happens to sit beside the ini is the wrong-mod-set failure above, so a stated
           directory that is not there is a reason to stop and say which one. */
        return Err(if stated {
            crate::msg!("eng.mo2_dir_missing", key = key, path = dir.display())
        } else {
            crate::msg!("eng.not_mo2_folder", path = base.display())
        });
    }

    // Which profile is active, and where the game itself lives.
    let selected = match want {
        Some(w) if !w.is_empty() => w.to_string(),
        _ => ini_lookup(&org_ini, "selected_profile").unwrap_or_default(),
    };
    let game_path = ini_lookup(&org_ini, "gamePath").unwrap_or_default();

    let profile = if !selected.is_empty() && profiles_dir.join(&selected).is_dir() {
        selected
    } else {
        // Fall back to the only profile, or the first alphabetically.
        let mut names: Vec<String> = std::fs::read_dir(&profiles_dir)
            .map_err(|e| crate::msg!("eng.mo2_profiles_unreadable", err = e))?
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        let picked = names.first().cloned().ok_or_else(|| crate::msg!("eng.mo2_no_profiles"))?;
        if !selected.is_empty() {
            out.warnings.push(crate::msg!("eng.mo2_profile_missing", wanted = selected, used = picked));
        }
        picked
    };
    let profile_dir = profiles_dir.join(&profile);

    // The game's own Data Files sits at the bottom of the stack.
    let data_files = if game_path.is_empty() {
        None
    } else {
        first_existing(&[
            PathBuf::from(&game_path).join("Data Files"),
            PathBuf::from(&game_path),
        ])
    };
    if data_files.is_none() {
        out.warnings.push(crate::msg!("eng.mo2_no_game_path"));
    }

    // modlist.txt, top line first. `+` enabled, `-` disabled, `*` unmanaged
    // (it already lives in Data Files), and separators are just UI furniture.
    let modlist = std::fs::read_to_string(profile_dir.join("modlist.txt"))
        .map_err(|e| crate::msg!("eng.mo2_modlist_unreadable", path = profile_dir.display(), err = e))?;
    let mut missing = 0usize;
    let mut enabled = 0usize;
    /* MO2's `overwrite` folder sits above every mod: it is where the game's own writes
       land (MWSE configs saved in play, generated files), and it wins over all of them.
       Round 13: Weather Adjuster's settings, when changed in game, are written there. */
    // Round 18be (I1): and from wherever this instance keeps it.
    let (overwrite, _) = mo2_dir(&org_ini, "overwrite_directory", &base_dir, "overwrite");
    if overwrite.is_dir() {
        out.roots.push(overwrite);
    }
    /* A BOM is three bytes Notepad adds to any file it saves as UTF-8, and editing
       `modlist.txt` by hand is a normal MO2 habit. `trim()` does not remove U+FEFF -
       it is a format character, not whitespace - so it stayed on the front of the
       first line and the byte-index split below landed inside it (round 18bb). */
    let modlist = modlist.strip_prefix('\u{feff}').unwrap_or(&modlist);
    for line in modlist.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        /* By character, not by byte. `split_at(1)` on a `&str` is a byte index, so any
           line beginning with a multi-byte character panicked rather than being read -
           and MO2 mod names routinely carry one. */
        let mut cs = t.chars();
        let flag = cs.next().map(|c| c.to_string()).unwrap_or_default();
        let flag = flag.as_str();
        let name = cs.as_str().trim();
        if name.is_empty() || name.ends_with("_separator") {
            continue;
        }
        match flag {
            "+" => {}
            "-" => {
                out.mods_off += 1;  // disabled
                continue;
            }
            "*" => {
                out.mods_off += 1;  // unmanaged: served by the Data Files root
                continue;
            }
            _ => continue,
        }
        enabled += 1;
        out.mods_on += 1;
        let dir = mods_dir.join(name);
        if dir.is_dir() {
            out.roots.push(dir);
        } else {
            missing += 1;
        }
    }
    if missing > 0 {
        out.warnings.push(crate::msg!("eng.mo2_mods_missing", n = missing));
    }
    if let Some(df) = data_files.clone() {
        out.game_dir = df.parent().map(|p| p.to_path_buf());
        out.roots.push(df);
    }

    // The plugin order lives with the profile when it keeps local ini files,
    // otherwise the game's own Morrowind.ini is authoritative.
    let mut order_candidates = vec![profile_dir.join("Morrowind.ini")];
    if let Some(df) = &data_files {
        order_candidates.push(df.join("Morrowind.ini"));
        if let Some(parent) = df.parent() {
            order_candidates.push(parent.join("Morrowind.ini"));
        }
    }
    let order = order_candidates.into_iter().find(|p| {
        std::fs::read_to_string(p).map(|t| t.contains("GameFile")).unwrap_or(false)
    });
    match order {
        Some(p) => out.order_file = p,
        None => {
            return Err(crate::msg!("eng.mo2_no_order", path = profile_dir.display()))
        }
    }

    out.label = format!("MO2 · {} · {} mods", profile, enabled);
    Ok(out)
}

/// Resolves an OpenMW install from its `openmw.cfg`.
///
/// OpenMW reads `data=` lines lowest priority first, so the overlay is that list
/// reversed.
pub fn openmw(cfg: &Path) -> Result<Layout, String> {
    let text = std::fs::read_to_string(cfg).map_err(|e| format!("{}: {}", cfg.display(), e))?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut missing = 0usize;

    // Relative paths in openmw.cfg are relative to the config's own folder.
    let anchor = cfg.parent().map(Path::to_path_buf).unwrap_or_default();

    for line in text.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("data=").or_else(|| t.strip_prefix("data-local=")) else {
            continue;
        };
        let raw = rest.trim().trim_matches('"');
        // OpenMW escapes embedded quotes and ampersands with '&'.
        let cleaned = raw.replace("&\"", "\"").replace("&&", "&");
        let p = PathBuf::from(&cleaned);
        let p = if p.is_absolute() { p } else { anchor.join(p) };
        if p.is_dir() {
            dirs.push(p);
        } else {
            missing += 1;
        }
    }
    if dirs.is_empty() {
        return Err(crate::msg!("eng.openmw_no_data", path = cfg.display()));
    }
    dirs.reverse(); // last listed wins, so it goes first in the overlay

    let mut out = Layout {
        label: format!("OpenMW · {} data dirs", dirs.len()),
        roots: dirs,
        order_file: cfg.to_path_buf(),
        warnings: Vec::new(),
        mods_on: 0,
        mods_off: 0,
        game_dir: None,
        mo2_base: None,
    };
    if missing > 0 {
        out.warnings.push(crate::msg!("eng.openmw_data_missing", n = missing));
    }
    Ok(out)
}



/// A plain install: one folder, and its own Morrowind.ini.
pub fn vanilla(data_files: &Path) -> Result<Layout, String> {
    if !data_files.is_dir() {
        return Err(crate::msg!("eng.not_a_folder", path = data_files.display()));
    }
    let order = first_existing(&[
        data_files.join("Morrowind.ini"),
        data_files.parent().map(|p| p.join("Morrowind.ini")).unwrap_or_default(),
    ])
    .ok_or_else(|| {
        format!(
            "no Morrowind.ini beside {} — that file lists the plugin load order under [Game Files]",
            data_files.display()
        )
    })?;

    let mut out = Layout {
        label: data_files.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        roots: vec![data_files.to_path_buf()],
        order_file: order,
        warnings: Vec::new(),
        mods_on: 0,
        mods_off: 0,
        game_dir: data_files.parent().map(|p| p.to_path_buf()),
        mo2_base: None,
    };
    let has = |n: &str| data_files.join(n).is_dir();
    if !has("Meshes") && !has("meshes") {
        out.warnings.push(crate::msg!("eng.no_meshes_folder"));
    }
    Ok(out)
}
