//! Turning a mod manager's own files into an ordered overlay.
//!
//! This used to live in the page, where it depended on File System Access handles
//! a packaged app never obtains. Doing it here means one folder pick is enough:
//! everything else is read off disk.

use crate::codepage::{self, CodePage};
use crate::paths;
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
    /// The code page the install's text is in (round 18gq): what `Morrowind.ini` is written
    /// in for a plain or MO2 install, since the game writes that file in the system's page
    /// and reads its plugins in the same one; `encoding=` for an OpenMW install;
    /// Windows-1252 when nothing says otherwise. The connect makes it the page in force
    /// (`codepage::set`); the layout only answers the question.
    pub code_page: CodePage,
}

/// How the layout found the install's code page, for the connect log.
fn code_page_line(page: CodePage, how: &str) -> Option<String> {
    if page == CodePage::Win1252 {
        return None;
    }
    Some(match how {
        "openmw" => crate::msg!("eng.code_page_openmw", page = page.name(), name = page.openmw_name()),
        _ => crate::msg!("eng.code_page_ini", page = page.name()),
    })
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

/// Resolves a Mod Organizer 2 install from its base folder — the one holding
/// `mods/` and `profiles/`.
///
/// Priority runs the way MO2's own left pane shows it: the first line of
/// `modlist.txt` is the conflict winner, so it becomes the first overlay root.
/// The game's Data Files goes last, because anything MO2 manages overrides it.
pub fn mo2(base: &Path) -> Result<Layout, String> {
    mo2_profile(base, None)
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
        .and_then(|p| codepage::read_text(&p).ok())
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
    let modlist = codepage::read_text(&profile_dir.join("modlist.txt"))
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
    /* Round 18gq: read as text whatever page it is in. `read_to_string` refused the Russian
       edition's ini outright, so a Russian MO2 install had "no plugin load order". */
    let order = order_candidates.into_iter().find(|p| {
        codepage::read_text(p).map(|t| t.contains("GameFile")).unwrap_or(false)
    });
    match order {
        Some(p) => out.order_file = p,
        None => {
            return Err(crate::msg!("eng.mo2_no_order", path = profile_dir.display()))
        }
    }
    out.code_page = codepage::sniff_file(&out.order_file);
    out.warnings.extend(code_page_line(out.code_page, "ini"));

    out.label = format!("MO2 · {} · {} mods", profile, enabled);
    Ok(out)
}

/// Resolves an OpenMW install from its `openmw.cfg`.
///
/// Round 18gn: the file is read as the first or last link of OpenMW's config chain
/// (`openmwcfg::read`), so the install's own `openmw.cfg` and every `config=` folder count,
/// `data-local=` comes last whatever line it stood on, and a `replace=` is honoured. OpenMW
/// reads `data=` lines lowest priority first, so the overlay is that list reversed. When
/// more than one file was read, the connect log names them in order.
pub fn openmw(cfg: &Path) -> Result<Layout, String> {
    if !cfg.is_file() {
        return Err(format!("{}: not a file", cfg.display()));
    }
    let chain = crate::openmwcfg::read(cfg);
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut missing = 0usize;
    for p in &chain.data {
        if p.is_dir() {
            dirs.push(p.clone());
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
        // Round 18gq: `encoding=` is OpenMW's own word for it, and the launcher writes it.
        code_page: chain.code_page().unwrap_or_default(),
    };
    if chain.files.len() > 1 {
        let files = chain.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>().join(", ");
        out.warnings.push(crate::msg!("eng.openmw_chain", n = chain.files.len(), files = files));
    }
    out.warnings.extend(code_page_line(out.code_page, "openmw"));
    if missing > 0 {
        out.warnings.push(crate::msg!("eng.openmw_data_missing", n = missing));
    }
    Ok(out)
}

/// Works out what kind of install a path is, and opens it — round 18bh.
///
/// One flag instead of three. `--data`, `--mo2` and `--openmw` each say *what kind of thing
/// you are pointing at*, which is a question the path itself can nearly always answer: an
/// `openmw.cfg` is a file and says so in its name; a folder holding `ModOrganizer.ini` is an
/// MO2 base; a folder called `Data Files`, or one with a `Morrowind.ini` beside it, is a
/// plain install. The three stay as overrides for the day this guesses wrong — a
/// hand-assembled folder, an MO2 base that has lost its ini — and the answer says which kind
/// it decided on, so a wrong guess is visible rather than mysterious.
///
/// `mo2_profile` is only consulted for an MO2 install, where it names the profile whose load
/// order to read; elsewhere it is ignored, because there is nothing for it to name.
pub fn detect(path: &Path, mo2_profile: Option<&str>) -> Result<Layout, String> {
    // A file can only be one thing, and the name is how openmw itself finds it.
    if path.is_file() {
        let name = path.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.ends_with(".cfg") {
            return openmw(path);
        }
        return Err(crate::msg!("eng.not_a_folder", path = path.display()));
    }
    if !path.is_dir() {
        return Err(crate::msg!("eng.not_a_folder", path = path.display()));
    }
    // MO2 first: its base folder also holds a `mods` folder, and a `Data Files` beneath a
    // game folder never holds a ModOrganizer.ini.
    if path.join("ModOrganizer.ini").is_file() || path.join("mods").is_dir() {
        return mo2_profile_or_base(path, mo2_profile);
    }
    // An openmw.cfg sitting in the folder somebody pointed at.
    let cfg = path.join("openmw.cfg");
    if cfg.is_file() {
        return openmw(&cfg);
    }
    /* A game folder rather than its Data Files: the mistake is easy to make and the fix is
       one join, so make it rather than refusing. */
    let data = path.join("Data Files");
    if data.is_dir() {
        return vanilla(&data);
    }
    vanilla(path)
}

fn mo2_profile_or_base(base: &Path, want: Option<&str>) -> Result<Layout, String> {
    match want {
        Some(_) => mo2_profile(base, want),
        None => mo2(base),
    }
}

/// Whether a folder holds a plugin at its top level, which is what makes it a Data Files.
fn has_plugins(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().any(|e| paths::is_plugin(&e.file_name().to_string_lossy())))
        .unwrap_or(false)
}

/// A plain install: one folder, and its own Morrowind.ini.
pub fn vanilla(data_files: &Path) -> Result<Layout, String> {
    if !data_files.is_dir() {
        return Err(crate::msg!("eng.not_a_folder", path = data_files.display()));
    }
    /* Round 18gu: the game folder picked instead of its Data Files. The window's Connect
       Data Files button takes whatever folder is picked as the overlay root, and a game
       folder has `Morrowind.ini` right there, so the load order read fine and every plugin
       in it was "not found": they sit one folder down, keyed `data files/...` in the index,
       where no lookup by plain name reaches. The Russian tester's report (every plugin not
       found, on his edition and on the English Steam one alike, with a plain ASCII path)
       is that shape exactly. `detect` already steps into a game folder's Data Files; this
       does the same when the picked folder holds no plugin of its own and has one, and the
       connect log says so. */
    let mut stepped: Option<PathBuf> = None;
    let data_files: PathBuf = if !has_plugins(data_files) && data_files.join("Data Files").is_dir() {
        stepped = Some(data_files.to_path_buf());
        data_files.join("Data Files")
    } else {
        data_files.to_path_buf()
    };
    let data_files = data_files.as_path();
    /* Round 18hk: the game's own ini is the one in the Morrowind folder, beside Data Files;
       that is the file the launcher writes the load order into. A `Morrowind.ini` inside
       Data Files is somebody's copy (a mod's sample, a tool's settings, a backup), and
       until now it was tried first and won: abot's connect read such a file, found no
       [Game Files] in it, and was told his ini "lists no plugins" while the real one sat
       one folder up, full of them. The folder's ini comes first now; the one inside Data
       Files is only read when there is nothing above it (the harness builds its fixtures
       that way), and when both are there the connect log says which was passed over. */
    let in_game_dir = data_files.parent().and_then(|p| first_existing(&[p.join("Morrowind.ini")]));
    let in_data_files = first_existing(&[data_files.join("Morrowind.ini")]);
    let passed_over = if in_game_dir.is_some() { in_data_files.clone() } else { None };
    let order = in_game_dir.or(in_data_files).ok_or_else(|| {
        format!(
            "no Morrowind.ini beside {} — that file lists the plugin load order under [Game Files]",
            data_files.display()
        )
    })?;

    // Round 18gq: the ini's own bytes say which page this edition of the game writes.
    let code_page = codepage::sniff_file(&order);
    let mut out = Layout {
        label: data_files.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        roots: vec![data_files.to_path_buf()],
        order_file: order,
        warnings: Vec::new(),
        mods_on: 0,
        mods_off: 0,
        game_dir: data_files.parent().map(|p| p.to_path_buf()),
        mo2_base: None,
        code_page,
    };
    if let Some(picked) = stepped {
        out.warnings.push(crate::msg!("eng.data_files_stepped_in", picked = picked.display()));
    }
    if let Some(shadow) = passed_over {
        out.warnings.push(crate::msg!("eng.ini_in_data_files", file = shadow.display()));
    }
    out.warnings.extend(code_page_line(code_page, "ini"));
    let has = |n: &str| data_files.join(n).is_dir();
    if !has("Meshes") && !has("meshes") {
        out.warnings.push(crate::msg!("eng.no_meshes_folder"));
    }
    Ok(out)
}
