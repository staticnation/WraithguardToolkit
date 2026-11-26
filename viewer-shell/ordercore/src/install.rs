//! An install of any kind read as an `openmw.cfg` is ([`crate::cfg::OpenmwCfg`]): the setup
//! every page and the command line take (TODO.md, Next, 2; Static, 2026-11-10).
//!
//! A setup's name is one of:
//!
//! - an `openmw.cfg` (and the cfgs it chains), as OpenMW reads it;
//! - `mo2:<Mod Organizer folder>`, or `mo2:<folder>|<profile>`: the profile's `Morrowind.ini`
//!   for the plugins and archives, its enabled mods by priority for the data folders, the
//!   game's `Data Files` lowest;
//! - a `Data Files` folder, or the `Morrowind.ini` beside it: the game as it is installed.
//!
//! The last two are Robin's readers (`gardenfell-install`'s `layout`), made into the cfg's
//! shape, so what reads a cfg reads an MO2 or Morrowind.ini install the same way.

use std::path::{Path, PathBuf};

use crate::cfg::{OpenmwCfg, Tokens};

/// What kind of install a setup is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An `openmw.cfg`.
    Openmw,
    /// A Mod Organizer 2 install (a profile of it).
    Mo2,
    /// The game's own install: its `Data Files` and `Morrowind.ini`.
    Vanilla,
}

impl Kind {
    /// As the pages name it: `openmw`, `mo2`, `vanilla`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Openmw => "openmw",
            Kind::Mo2 => "mo2",
            Kind::Vanilla => "vanilla",
        }
    }
}

/// A setup's name read: its kind, the path (the cfg, the MO2 folder, the `Data Files`) and an
/// MO2 profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub kind: Kind,
    pub path: String,
    pub profile: Option<String>,
}

impl Spec {
    /// The setup's name for it (what [`spec`] reads back).
    pub fn name(&self) -> String {
        match self.kind {
            Kind::Mo2 => format!("mo2:{}{}", self.path, self.profile.as_deref().filter(|p| !p.is_empty()).map(|p| format!("|{p}")).unwrap_or_default()),
            _ => self.path.clone(),
        }
    }
}

fn is_ini(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("morrowind.ini"))
}

/// Whether a file is an `openmw.cfg`: named `.cfg`, or with `content=` / `data=` lines.
fn is_cfg(p: &Path) -> bool {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cfg")) {
        return true;
    }
    p.is_file()
        && std::fs::read(p).is_ok_and(|b| {
            String::from_utf8_lossy(&b).lines().any(|l| {
                let k = l.split_once('=').map(|(k, _)| k.trim().to_ascii_lowercase()).unwrap_or_default();
                k == "content" || k == "data"
            })
        })
}

/// What a setup's name says it is (a Morrowind.ini named stands for the `Data Files` beside it).
pub fn spec(name: &str) -> Spec {
    let name = name.trim();
    if let Some(rest) = name.strip_prefix("mo2:") {
        let (base, profile) = match rest.rsplit_once('|') {
            Some((b, p)) if !p.trim().is_empty() => (b, Some(p.trim().to_string())),
            _ => (rest.trim_end_matches('|'), None),
        };
        return Spec { kind: Kind::Mo2, path: base.trim().to_string(), profile };
    }
    let p = Path::new(name);
    if is_ini(p) {
        if let Some(game) = p.parent() {
            return Spec { kind: Kind::Vanilla, path: game.join("Data Files").to_string_lossy().into_owned(), profile: None };
        }
    }
    if is_cfg(p) {
        return Spec { kind: Kind::Openmw, path: name.to_string(), profile: None };
    }
    Spec { kind: Kind::Vanilla, path: name.to_string(), profile: None }
}

// ---------------------------------------------------------------------------------------------
// Mod Organizer 2's own settings, read the way MO2 reads them.
//
// MO2's Morrowind support (the `game_morrowind` plugin) keeps which plugins are on in a
// `Morrowind.ini`'s `[Game Files]` and their order in the profile's `loadorder.txt` (it has no
// `plugins.txt` of its own). *Which* `Morrowind.ini` is the profile's setting "use profile-specific
// game INI files" (`LocalSettings` in the profile's `settings.ini`): on, the profile's own copy;
// off, the game folder's, and a copy left in the profile is stale and ignored. Reading "the first
// ini that lists plugins" shows what MO2 has switched off as on whenever such a copy is there.
// ---------------------------------------------------------------------------------------------

/// A text file as its bytes say: UTF-8 when it is (Qt writes `ModOrganizer.ini` so), else Windows-1252.
fn read_any(p: &Path) -> String {
    std::fs::read(p).map(|b| String::from_utf8(b.clone()).unwrap_or_else(|_| crate::inputs::cp1252(&b))).unwrap_or_default()
}

/// Qt's `@ByteArray(...)`, quotes and doubled backslashes off a value.
fn qt_text(raw: &str) -> String {
    let mut v = raw.trim().to_string();
    if let Some(inner) = v.strip_prefix("@ByteArray(").and_then(|x| x.strip_suffix(')')) {
        v = inner.to_string();
    }
    v.trim().trim_matches('"').replace("\\\\", "\\")
}

/// `key=value` out of a Qt ini, ignoring sections; the key matched without regard to case, with
/// Qt's `%20` and `/` taken as a space and a backslash (`OpenMW%20Exporter/config%20path`).
fn ini_get(text: &str, key: &str) -> Option<String> {
    let want = key.replace('/', "\\").to_lowercase();
    text.lines().find_map(|l| {
        let t = l.trim();
        if t.starts_with(';') || t.starts_with('[') {
            return None;
        }
        let (k, v) = t.split_once('=')?;
        (k.trim().replace("%20", " ").replace('/', "\\").to_lowercase() == want).then(|| qt_text(v))
    })
}

fn flag(v: &str) -> bool {
    v.eq_ignore_ascii_case("true") || v == "1"
}

/// `%BASE_DIR%` (MO2's own folder for the instance) and relative paths, resolved.
fn expand_base(raw: &str, base_dir: &Path) -> PathBuf {
    let v = raw.trim().replace('\\', "/");
    if v.len() >= 10 && v[..10].eq_ignore_ascii_case("%BASE_DIR%") {
        let rest = v[10..].trim_start_matches('/');
        return if rest.is_empty() { base_dir.to_path_buf() } else { base_dir.join(rest) };
    }
    let p = PathBuf::from(&v);
    if p.is_absolute() || (v.len() >= 2 && v.as_bytes()[1] == b':') { p } else { base_dir.join(p) }
}

/// An entry of `dir` by name, whatever case the disk has it in.
fn any_case(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// One MO2 profile: its folder, and whether it keeps its own game INI files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mo2Profile {
    pub name: String,
    pub dir: PathBuf,
    /// "Use profile-specific game INI files" (`LocalSettings`): the profile's `Morrowind.ini` is the
    /// one MO2 reads and writes. Off: the game folder's.
    pub local_ini: bool,
}

/// The profile an MO2 folder has open (`want`, or the one `ModOrganizer.ini` selects; as the
/// layout reader falls back: the first by name when that one is not there).
pub fn mo2_profile(base: &Path, want: Option<&str>) -> Option<Mo2Profile> {
    let org = read_any(&base.join("ModOrganizer.ini"));
    let base_dir = ini_get(&org, "base_directory")
        .filter(|v| !v.trim().is_empty())
        .map(|v| expand_base(&v, base))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| base.to_path_buf());
    let profiles = ini_get(&org, "profiles_directory").filter(|v| !v.trim().is_empty()).map(|v| expand_base(&v, &base_dir)).unwrap_or_else(|| base_dir.join("profiles"));
    let selected = want.filter(|w| !w.is_empty()).map(str::to_string).or_else(|| ini_get(&org, "selected_profile")).unwrap_or_default();
    let name = if !selected.is_empty() && profiles.join(&selected).is_dir() {
        selected
    } else {
        let mut names: Vec<String> = std::fs::read_dir(&profiles).ok()?.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names.into_iter().next()?
    };
    let dir = profiles.join(&name);
    // The profile's own setting, else MO2's global default (on when nothing says).
    let local_ini = ini_get(&read_any(&dir.join("settings.ini")), "LocalSettings").or_else(|| ini_get(&org, "profile_local_inis")).is_none_or(|v| flag(&v));
    Some(Mo2Profile { name, dir, local_ini })
}

/// The profile folder of an MO2 setup's name (`mo2:<folder>|<profile>`), where MO2 keeps
/// `loadorder.txt`, `modlist.txt` and the profile's settings.
pub fn profile_dir_of(name: &str) -> Option<PathBuf> {
    let s = spec(name);
    (s.kind == Kind::Mo2).then(|| mo2_profile(Path::new(&s.path), s.profile.as_deref())).flatten().map(|p| p.dir)
}

/// MO2's layout (Robin's reader) with the plugin list taken from the `Morrowind.ini` MO2 itself
/// reads: the profile's when it keeps its own, else the game folder's. A copy in the profile that
/// MO2 does not use lists other plugins; the layout says so rather than showing them.
///
/// # Errors
/// As the layout reader: no MO2 folder, profile, or plugin list.
pub fn mo2_layout(base: &Path, want: Option<&str>) -> Result<gardenfell_install::layout::Layout, String> {
    let mut l = gardenfell_install::layout::mo2_profile(base, want)?;
    let Some(p) = mo2_profile(base, want) else { return Ok(l) };
    let ini = if p.local_ini { any_case(&p.dir, "Morrowind.ini") } else { l.game_dir.as_deref().and_then(|g| any_case(g, "Morrowind.ini")) };
    let Some(ini) = ini.filter(|i| !game_files(i).is_empty()) else { return Ok(l) };
    if ini == l.order_file {
        return Ok(l);
    }
    let (mine, theirs) = (game_files(&ini), game_files(&l.order_file));
    let have: std::collections::HashSet<String> = mine.iter().map(|n| n.to_lowercase()).collect();
    let extra = theirs.iter().filter(|n| !have.contains(&n.to_lowercase())).count();
    if extra > 0 || mine.len() != theirs.len() {
        l.warnings.push(format!(
            "MO2 reads this profile's plugins from {} (profile-specific game INI files are {}), not from {}, which lists {extra} plugin(s) the profile has off",
            ini.display(),
            if p.local_ini { "on" } else { "off" },
            l.order_file.display()
        ));
    }
    l.order_file = ini;
    Ok(l)
}

/// OpenMW Player's dummy plugins: a `.omwaddon` or `.omwscripts` it makes an empty `.esp` for, so
/// MO2's plugin list can switch it on and sort it. MO2 lists `X.omwaddon.esp`; OpenMW is given
/// `X.omwaddon`.
const DUMMY_SUFFIXES: [&str; 2] = [".omwaddon.esp", ".omwscripts.esp"];

/// Whether Kezyma's OpenMW Player is among an MO2 folder's plugins.
pub fn player_installed(base: &Path) -> bool {
    any_case(&base.join("plugins"), "openmwplayer").is_some_and(|p| p.is_dir())
        || read_any(&base.join("ModOrganizer.ini")).lines().any(|l| l.trim().to_lowercase().starts_with("openmwplayer\\"))
}

/// The name OpenMW has for a plugin MO2 lists: a dummy `.esp` is its `.omwaddon` or `.omwscripts`.
pub fn real_name(n: &str) -> String {
    let low = n.to_lowercase();
    if DUMMY_SUFFIXES.iter().any(|s| low.ends_with(s)) { n[..n.len() - ".esp".len()].to_string() } else { n.to_string() }
}

/// The names as MO2 lists them: each of `names` that `listed` (MO2's own list) has as a dummy
/// `.esp` goes back to that name. For writing an order into MO2's files.
pub fn as_listed(listed: &[String], names: &[String]) -> Vec<String> {
    let dummy: std::collections::HashMap<String, &String> = listed.iter().filter(|n| real_name(n) != **n).map(|n| (real_name(n).to_lowercase(), n)).collect();
    names.iter().map(|n| dummy.get(&n.to_lowercase()).map_or_else(|| n.clone(), |d| (*d).clone())).collect()
}

/// The `openmw.cfg` that MO2 feeds OpenMW with for the profile `want` (the open one when `None`),
/// when it does. Two plugins do:
///
/// - Kezyma's *OpenMW Player* keeps a cfg per profile (`openmw.cfg` in the profile folder when it
///   keeps its own INIs, else under `plugins/data/OpenMWPlayer`), and writes the one set as its
///   `openmwcfgpath` (default `Documents/My Games/OpenMW/openmw.cfg`) when OpenMW is run from MO2.
///   The profile's is the one OpenMW is given, so it comes first;
/// - AnyOldName3's *Export to OpenMW* (`OpenMWExport.py`) rewrites one cfg (`OpenMW Exporter\config
///   path`, else the default).
///
/// `None`: the install runs the game itself (Static, 2026-11-10: "openmw can use mo2").
pub fn mo2_openmw_cfg_for(base: &Path, want: Option<&str>) -> Option<PathBuf> {
    let ini = read_any(&base.join("ModOrganizer.ini"));
    let home_cfg = || {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
        Some(PathBuf::from(home).join("Documents").join("My Games").join("OpenMW").join("openmw.cfg")).filter(|p| p.is_file())
    };
    if player_installed(base) {
        if let Some(p) = mo2_profile(base, want) {
            let own = if p.local_ini { p.dir.join("openmw.cfg") } else { base.join("plugins").join("data").join("OpenMWPlayer").join("openmw.cfg") };
            if own.is_file() {
                return Some(own);
            }
        }
        let kept = ini_get(&ini, "OpenMWPlayer\\openmwcfgpath").map(PathBuf::from).filter(|p| p.is_file());
        if kept.is_some() {
            return kept;
        }
        if let Some(h) = home_cfg() {
            return Some(h);
        }
    }
    if let Some(p) = ini_get(&ini, "OpenMW Exporter\\config path").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    if !base.join("plugins").join("OpenMWExport.py").is_file() {
        return None;
    }
    home_cfg()
}

/// [`mo2_openmw_cfg_for`] for the profile MO2 has open.
pub fn mo2_openmw_cfg(base: &Path) -> Option<PathBuf> {
    mo2_openmw_cfg_for(base, None)
}

/// The `openmw.cfg` the setup named feeds OpenMW through, when it is an MO2 install that does
/// ([`mo2_openmw_cfg_for`]).
pub fn feeds_openmw(name: &str) -> Option<PathBuf> {
    let s = spec(name);
    (s.kind == Kind::Mo2).then(|| mo2_openmw_cfg_for(Path::new(&s.path), s.profile.as_deref())).flatten()
}

/// Whether the setup named runs on OpenMW: an `openmw.cfg`, or an MO2 install that feeds one.
pub fn runs_openmw(name: &str) -> bool {
    spec(name).kind == Kind::Openmw || feeds_openmw(name).is_some()
}

/// Whether a plugin is a file only OpenMW loads.
pub fn is_openmw_only(name: &str) -> bool {
    let low = name.to_lowercase();
    low.ends_with(".omwaddon") || low.ends_with(".omwscripts") || low.ends_with(".omwgame")
}

/// A Morrowind.ini's `[Game Files]`: the `GameFileN=` names, by `N`.
pub fn game_files(ini: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(ini) else { return Vec::new() };
    let text = crate::inputs::cp1252(&bytes);
    let mut indexed: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.len() > 8 && t[..8].eq_ignore_ascii_case("gamefile") {
            if let Some((n, v)) = t[8..].split_once('=') {
                if let Ok(n) = n.trim().parse::<usize>() {
                    let v = v.trim();
                    if !v.is_empty() {
                        indexed.push((n, v.to_string()));
                    }
                }
            }
        }
    }
    indexed.sort_by_key(|(n, _)| *n);
    indexed.into_iter().map(|(_, s)| s).collect()
}

/// A plain Morrowind install's plugins in the order the game loads them. `Morrowind.ini` says
/// *which* plugins load (`[Game Files]`), not in what order: the game sorts them by the dates of
/// the files, the masters (`.esm`) first and then the plugins, and the ini lists them as the
/// launcher last wrote them. So the ini's order is only the tie-break here, for files with one
/// date. A master is told by its extension, here as everywhere in the tools; a file whose header
/// flag disagrees is reported by the master check (`inputs::check_missing_masters`). A file is
/// looked for in the data folders (the last holding it, as a cfg's are read); one that is not there
/// cannot be dated, and goes last of its kind.
pub fn by_date(names: &[String], data: &[PathBuf]) -> Vec<String> {
    let mut found: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
    for d in data {
        let Ok(entries) = std::fs::read_dir(d) else { continue };
        for e in entries.flatten() {
            found.insert(e.file_name().to_string_lossy().to_lowercase(), e.path());
        }
    }
    let master = |n: &str| {
        let l = n.to_lowercase();
        l.ends_with(".esm") || l.ends_with(".omwgame")
    };
    let mut keyed: Vec<(bool, bool, Option<std::time::SystemTime>, usize)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let at = found.get(&n.to_lowercase()).and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
            (!master(n), at.is_none(), at, i)
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(_, _, _, i)| names[i].clone()).collect()
}

/// An MO2 or Morrowind.ini install in a cfg's shape: its `[Game Files]` as the content, its
/// overlay's roots as the data folders (lowest priority first, as a cfg lists them), the
/// archives the engine loads (`Morrowind.bsa` first, which it always loads).
pub fn from_layout(l: &gardenfell_install::layout::Layout) -> OpenmwCfg {
    from_layout_for(l, None)
}

/// [`from_layout`] for an install that feeds OpenMW through `openmw` (an MO2 install's
/// exported cfg): its grass from the cfg, OpenMW's own files not warned of, and a cfg that
/// no longer has the profile's plugins said (export again, or write both from here).
pub fn from_layout_for(l: &gardenfell_install::layout::Layout, openmw: Option<&OpenmwCfg>) -> OpenmwCfg {
    let mut listed = game_files(&l.order_file);
    // MO2 with OpenMW Player lists a dummy .esp for each .omwaddon/.omwscripts: OpenMW's names.
    if l.mo2_base.as_deref().is_some_and(player_installed) {
        listed = listed.iter().map(|n| real_name(n)).collect();
    }
    let mut warnings = l.warnings.clone();
    let data: Vec<PathBuf> = l.roots.iter().rev().filter(|d| d.is_dir()).cloned().collect();
    // A plain install (not MO2, whose list is its own, and not an OpenMW cfg) loads by the dates
    // of its plugins, whatever order the ini names them in.
    let plain = l.mo2_base.is_none() && openmw.is_none() && !l.order_file.extension().is_some_and(|e| e.eq_ignore_ascii_case("cfg"));
    let content = if plain { by_date(&listed, &data) } else { listed.clone() };
    if plain && content != listed {
        let moved = content.iter().zip(&listed).filter(|(a, b)| a != b).count();
        warnings.push(format!(
            "Morrowind.ini lists {moved} plugin(s) in another order than the game loads them: it loads by the dates of the files, masters first, and the ini only says which load"
        ));
    }
    match openmw {
        None => {
            // OpenMW's own files: the game's engine does not load them, whatever the ini lists.
            for n in content.iter().filter(|n| is_openmw_only(n)) {
                warnings.push(format!("{n} is OpenMW's (.omwaddon, .omwscripts, .omwgame): the game's engine does not load it"));
            }
        }
        Some(c) => {
            let have: std::collections::HashSet<String> = c.content.iter().map(|n| n.to_lowercase()).collect();
            let gone: Vec<&String> = content.iter().filter(|n| !have.contains(&n.to_lowercase())).collect();
            if !gone.is_empty() {
                warnings.push(format!(
                    "{} of the profile's plugins are not in the openmw.cfg MO2 feeds OpenMW with ({}): run OpenMW from MO2 (or Export to OpenMW) to bring it up to date",
                    gone.len(),
                    gone.iter().take(5).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    OpenmwCfg {
        files: vec![l.order_file.clone()],
        data,
        missing_data: l.roots.iter().rev().filter(|d| !d.is_dir()).cloned().collect(),
        content,
        archives: gardenfell_bsa::bsa::archives_loaded(&l.order_file).names,
        warnings,
        groundcover: openmw.map(|c| c.groundcover.clone()).unwrap_or_default(),
        // The edition's code page as the layout found it (the ini's own bytes, or the cfg MO2 feeds).
        encoding: openmw.and_then(|c| c.encoding.clone()).or_else(|| Some(l.code_page.openmw_name().to_string())),
        ..Default::default()
    }
}

/// The setup named `name` read (see the module's note).
///
/// # Errors
/// The cfg, the MO2 folder or profile, or the game install cannot be read.
pub fn read(name: &str) -> Result<OpenmwCfg, String> {
    let s = spec(name);
    let p = PathBuf::from(&s.path);
    match s.kind {
        Kind::Openmw => crate::cfg::read(&p, &Tokens::for_cfg(&p)).map_err(|e| format!("{name} could not be read: {e}")),
        Kind::Mo2 => {
            let l = mo2_layout(&p, s.profile.as_deref())?;
            let feeds = mo2_openmw_cfg_for(&p, s.profile.as_deref()).and_then(|c| crate::cfg::read(&c, &Tokens::for_cfg(&c)).ok());
            Ok(from_layout_for(&l, feeds.as_ref()))
        }
        Kind::Vanilla => Ok(from_layout(&gardenfell_install::layout::vanilla(&p)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setups_name_says_its_kind() {
        assert_eq!(spec("mo2:C:/MO2|Main"), Spec { kind: Kind::Mo2, path: "C:/MO2".into(), profile: Some("Main".into()) });
        assert_eq!(spec("mo2:C:/MO2|").profile, None);
        assert_eq!(spec("mo2:C:/MO2|Main").name(), "mo2:C:/MO2|Main");
        assert_eq!(spec("/x/openmw.cfg").kind, Kind::Openmw);
        let ini = spec("/g/Morrowind.ini");
        assert_eq!((ini.kind, Path::new(&ini.path)), (Kind::Vanilla, Path::new("/g/Data Files")));
        assert_eq!(spec("/g/Data Files").kind, Kind::Vanilla);
    }

    #[test]
    fn an_mo2_install_that_feeds_openmw() {
        let d = std::env::temp_dir().join(format!("wg_mo2omw_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(mo2_openmw_cfg(&d), None);
        let cfg = d.join("openmw.cfg");
        std::fs::write(&cfg, "content=Morrowind.esm\n").unwrap();
        std::fs::write(d.join("ModOrganizer.ini"), format!("[General]\r\n[Plugins]\r\nOpenMW%20Exporter\\always%20use%20this%20config%20path=true\r\nOpenMW%20Exporter\\config%20path={}\r\n", cfg.display())).unwrap();
        assert_eq!(mo2_openmw_cfg(&d), Some(cfg.clone()));
        assert!(runs_openmw(&format!("mo2:{}", d.display())) && runs_openmw("/x/openmw.cfg") && !runs_openmw("/g/Data Files"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn game_files_read_by_number() {
        let d = std::env::temp_dir().join(format!("wg_install_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let ini = d.join("Morrowind.ini");
        std::fs::write(&ini, b"[Game Files]\r\nGameFile1=B.esp\r\nGameFile0=Morrowind.esm\r\ngamefile2 = C\xe9.esp\r\nGameFile3=\r\n").unwrap();
        assert_eq!(game_files(&ini), ["Morrowind.esm", "B.esp", "C\u{e9}.esp"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A plain install loads by file date, masters first; the ini names what loads. MO2's list and
    /// an OpenMW cfg are the order as they are listed.
    #[test]
    fn a_plain_installs_order_is_the_dates_not_the_inis() {
        let d = std::env::temp_dir().join(format!("wg_bydate_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let data = d.join("Data Files");
        std::fs::create_dir_all(&data).unwrap();
        let at = |s: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(s);
        // Listed: Morrowind, A, B, Late.esm - the dates say Late.esm is a master, B older than A.
        for (n, s) in [("Morrowind.esm", 100), ("A.esp", 500), ("B.esp", 400), ("Late.esm", 200)] {
            std::fs::File::create(data.join(n)).unwrap().set_modified(at(s)).unwrap();
        }
        std::fs::write(d.join("Morrowind.ini"), b"[Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=A.esp\r\nGameFile2=B.esp\r\nGameFile3=Late.esm\r\nGameFile4=Gone.esp\r\n").unwrap();
        let cfg = read(&d.join("Morrowind.ini").to_string_lossy()).unwrap();
        assert_eq!(cfg.content, ["Morrowind.esm", "Late.esm", "B.esp", "A.esp", "Gone.esp"]);
        assert!(cfg.warnings.iter().any(|w| w.contains("loads by the dates of the files")), "{:?}", cfg.warnings);
        // As the ini lists them when the dates agree: no warning, the same list.
        for (n, s) in [("Morrowind.esm", 100), ("Late.esm", 150), ("A.esp", 300), ("B.esp", 400)] {
            std::fs::File::options().write(true).open(data.join(n)).unwrap().set_modified(at(s)).unwrap();
        }
        std::fs::write(d.join("Morrowind.ini"), b"[Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=Late.esm\r\nGameFile2=A.esp\r\nGameFile3=B.esp\r\n").unwrap();
        let cfg = read(&d.join("Morrowind.ini").to_string_lossy()).unwrap();
        assert_eq!(cfg.content, ["Morrowind.esm", "Late.esm", "A.esp", "B.esp"]);
        assert!(!cfg.warnings.iter().any(|w| w.contains("loads by the dates")), "{:?}", cfg.warnings);
        let _ = std::fs::remove_dir_all(&d);
    }

    // ---- MO2 and OpenMW Player ----

    /// An MO2 folder as MO2 lays it out: one profile `Main`, its `modlist.txt`, `settings.ini`
    /// (`LocalSettings` as given; none when `None`), a profile `Morrowind.ini` listing `profile_ini`
    /// and the game folder's listing `game_ini`.
    fn fake_mo2(tag: &str, local: Option<bool>, profile_ini: &[&str], game_ini: &[&str]) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("wg_mo2_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (base, game) = (root.join("MO2"), root.join("Morrowind"));
        let profile = base.join("profiles").join("Main");
        for d in [&profile, &base.join("mods").join("ModA"), &game.join("Data Files")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(
            base.join("ModOrganizer.ini"),
            format!("[General]\r\ngamePath=@ByteArray({})\r\nselected_profile=@ByteArray(Main)\r\n", game.display()),
        )
        .unwrap();
        std::fs::write(profile.join("modlist.txt"), "# This file was automatically generated by Mod Organizer.\r\n+ModA\r\n").unwrap();
        if let Some(l) = local {
            std::fs::write(profile.join("settings.ini"), format!("[General]\r\nLocalSettings={l}\r\n")).unwrap();
        }
        let ini = |names: &[&str]| {
            let mut t = String::from("[Game Files]\r\n");
            for (i, n) in names.iter().enumerate() {
                t += &format!("GameFile{i}={n}\r\n");
            }
            t
        };
        if !profile_ini.is_empty() {
            std::fs::write(profile.join("Morrowind.ini"), ini(profile_ini)).unwrap();
        }
        std::fs::write(game.join("Morrowind.ini"), ini(game_ini)).unwrap();
        (base, game)
    }

    fn setup(base: &Path) -> String {
        format!("mo2:{}|Main", base.display())
    }

    #[test]
    fn with_profile_inis_off_mo2_uses_the_games_ini_not_a_stale_copy_in_the_profile() {
        // The profile's copy was made when the profile was and lists everything; MO2 keeps the
        // plugins that are on in the game folder's ini once the setting is off.
        let (base, _) = fake_mo2("off", Some(false), &["Morrowind.esm", "A.esp", "B.esp", "C.esp"], &["Morrowind.esm", "A.esp"]);
        let cfg = read(&setup(&base)).unwrap();
        assert_eq!(cfg.content, ["Morrowind.esm", "A.esp"]);
        assert!(cfg.warnings.iter().any(|w| w.contains("profile-specific game INI files are off") && w.contains("2 plugin(s)")), "{:?}", cfg.warnings);
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn with_profile_inis_on_mo2_uses_the_profiles_ini() {
        let (base, _) = fake_mo2("on", Some(true), &["Morrowind.esm", "A.esp"], &["Morrowind.esm", "A.esp", "Old.esp"]);
        let cfg = read(&setup(&base)).unwrap();
        assert_eq!(cfg.content, ["Morrowind.esm", "A.esp"]);
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn a_profile_that_says_nothing_follows_mo2s_default_which_is_profile_inis_on() {
        let (base, _) = fake_mo2("default", None, &["Morrowind.esm", "A.esp"], &["Morrowind.esm", "A.esp", "Old.esp"]);
        assert!(mo2_profile(&base, None).unwrap().local_ini);
        std::fs::write(base.join("ModOrganizer.ini"), format!("{}[Settings]\r\nprofile_local_inis=false\r\n", read_any(&base.join("ModOrganizer.ini")))).unwrap();
        assert!(!mo2_profile(&base, None).unwrap().local_ini);
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn the_profile_folder_is_the_profiles_not_the_inis() {
        // Profile-specific INIs off: the ini in use is the game's, and its folder is not the profile.
        let (base, game) = fake_mo2("dir", Some(false), &[], &["Morrowind.esm", "A.esp"]);
        let name = setup(&base);
        assert_eq!(read(&name).unwrap().files, [game.join("Morrowind.ini")]);
        assert_eq!(profile_dir_of(&name), Some(base.join("profiles").join("Main")));
        assert_eq!(profile_dir_of("/x/openmw.cfg"), None);
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn kezymas_openmw_player_is_found_by_its_profile_cfg_or_its_setting() {
        let (base, _) = fake_mo2("player", Some(true), &["Morrowind.esm"], &["Morrowind.esm"]);
        // Not installed: nothing feeds OpenMW.
        assert_eq!(mo2_openmw_cfg_for(&base, None), None);
        std::fs::create_dir_all(base.join("plugins").join("openmwplayer")).unwrap();
        // Installed, and the profile has its cfg: that one.
        let own = base.join("profiles").join("Main").join("openmw.cfg");
        std::fs::write(&own, "content=Morrowind.esm\n").unwrap();
        assert_eq!(mo2_openmw_cfg_for(&base, None), Some(own.clone()));
        assert_eq!(feeds_openmw(&setup(&base)), Some(own.clone()));
        assert!(runs_openmw(&setup(&base)));
        // No profile cfg yet: the path set in the plugin's options.
        std::fs::remove_file(&own).unwrap();
        let kept = base.join("kept.cfg");
        std::fs::write(&kept, "content=Morrowind.esm\n").unwrap();
        let ini = read_any(&base.join("ModOrganizer.ini"));
        std::fs::write(base.join("ModOrganizer.ini"), format!("{ini}[Plugins]\r\nOpenMWPlayer\\openmwcfgpath={}\r\n", kept.display().to_string().replace('\\', "\\\\"))).unwrap();
        assert_eq!(mo2_openmw_cfg_for(&base, None), Some(kept));
        // Profile-specific INIs off: its cfg is under plugins/data.
        std::fs::write(base.join("profiles").join("Main").join("settings.ini"), "[General]\r\nLocalSettings=false\r\n").unwrap();
        let shared = base.join("plugins").join("data").join("OpenMWPlayer");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("openmw.cfg"), "content=Morrowind.esm\n").unwrap();
        assert_eq!(mo2_openmw_cfg_for(&base, None), Some(shared.join("openmw.cfg")));
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn the_older_export_plugin_is_still_found() {
        let (base, _) = fake_mo2("exporter", Some(true), &["Morrowind.esm"], &["Morrowind.esm"]);
        let cfg = base.join("exported.cfg");
        std::fs::write(&cfg, "content=Morrowind.esm\n").unwrap();
        let ini = read_any(&base.join("ModOrganizer.ini"));
        std::fs::write(base.join("ModOrganizer.ini"), format!("{ini}[Plugins]\r\nOpenMW%20Exporter\\config%20path={}\r\n", cfg.display().to_string().replace('\\', "\\\\"))).unwrap();
        assert_eq!(mo2_openmw_cfg_for(&base, None), Some(cfg));
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn open_mw_players_dummy_plugins_are_read_as_the_files_they_stand_for() {
        let names = ["Morrowind.esm", "Foo.omwaddon.esp", "Bar.OMWSCRIPTS.esp", "Plain.esp"];
        let (base, _) = fake_mo2("dummy", Some(true), &names, &names);
        // Without the plugin, they are plugins like any other.
        assert_eq!(read(&setup(&base)).unwrap().content, names);
        std::fs::create_dir_all(base.join("plugins").join("openmwplayer")).unwrap();
        let cfg = read(&setup(&base)).unwrap();
        assert_eq!(cfg.content, ["Morrowind.esm", "Foo.omwaddon", "Bar.OMWSCRIPTS", "Plain.esp"]);
        // And an order is written back under the names MO2 lists.
        let listed: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        let order: Vec<String> = cfg.content.iter().rev().cloned().collect();
        assert_eq!(as_listed(&listed, &order), ["Plain.esp", "Bar.OMWSCRIPTS.esp", "Foo.omwaddon.esp", "Morrowind.esm"]);
        assert_eq!(real_name("x.omwaddon.esp"), "x.omwaddon");
        assert_eq!(real_name("x.esp"), "x.esp");
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }

    #[test]
    fn a_sort_reaches_the_profiles_load_order_whichever_ini_is_in_use() {
        use crate::plain;
        let (base, game) = fake_mo2("write", Some(false), &[], &["Morrowind.esm", "A.esp", "B.esp"]);
        let profile = base.join("profiles").join("Main");
        std::fs::write(profile.join("loadorder.txt"), "# This file was automatically generated by Mod Organizer.\r\nMorrowind.esm\r\nA.esp\r\nB.esp\r\n").unwrap();
        let name = setup(&base);
        let cfg = read(&name).unwrap();
        let ini = cfg.files[0].clone();
        assert_eq!(ini, game.join("Morrowind.ini"));
        let order: Vec<String> = ["Morrowind.esm", "B.esp", "A.esp"].iter().map(|s| s.to_string()).collect();
        let dir = profile_dir_of(&name).unwrap();
        let said = plain::write(&ini, &order, &[], Some(&dir), &|_| None, false, "t").unwrap();
        // The game ini (MO2's, with INIs off) and the profile's loadorder.txt are both in the new order.
        assert_eq!(game_files(&ini), ["Morrowind.esm", "B.esp", "A.esp"]);
        let lo = std::fs::read_to_string(profile.join("loadorder.txt")).unwrap();
        assert!(lo.find("B.esp").unwrap() < lo.find("A.esp").unwrap(), "{lo}");
        // The game folder was not given an order file of its own.
        assert!(!game.join("loadorder.txt").exists() && !game.join("plugins.txt").exists());
        assert!(said.iter().all(|l| !l.contains("has no loadorder.txt")), "{said:?}");
        // A profile with none is told so rather than skipped in silence.
        std::fs::remove_file(profile.join("loadorder.txt")).unwrap();
        let said = plain::write(&ini, &order, &[], Some(&dir), &|_| None, true, "t").unwrap();
        assert!(said.iter().any(|l| l.contains("has no loadorder.txt")), "{said:?}");
        let _ = std::fs::remove_dir_all(base.parent().unwrap());
    }
}
