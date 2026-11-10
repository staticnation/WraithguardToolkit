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

/// An MO2 or Morrowind.ini install in a cfg's shape: its `[Game Files]` as the content, its
/// overlay's roots as the data folders (lowest priority first, as a cfg lists them), the
/// archives the engine loads (`Morrowind.bsa` first, which it always loads).
pub fn from_layout(l: &gardenfell_install::layout::Layout) -> OpenmwCfg {
    let content = game_files(&l.order_file);
    let mut warnings = l.warnings.clone();
    // OpenMW's own files: the game's engine does not load them, whatever the ini lists.
    for n in content.iter().filter(|n| is_openmw_only(n)) {
        warnings.push(format!("{n} is OpenMW's (.omwaddon, .omwscripts, .omwgame): the game's engine does not load it"));
    }
    OpenmwCfg {
        files: vec![l.order_file.clone()],
        data: l.roots.iter().rev().filter(|d| d.is_dir()).cloned().collect(),
        missing_data: l.roots.iter().rev().filter(|d| !d.is_dir()).cloned().collect(),
        content,
        archives: gardenfell_bsa::bsa::archives_loaded(&l.order_file).names,
        warnings,
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
        Kind::Mo2 => Ok(from_layout(&gardenfell_install::layout::mo2_profile(&p, s.profile.as_deref())?)),
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
    fn game_files_read_by_number() {
        let d = std::env::temp_dir().join(format!("wg_install_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let ini = d.join("Morrowind.ini");
        std::fs::write(&ini, b"[Game Files]\r\nGameFile1=B.esp\r\nGameFile0=Morrowind.esm\r\ngamefile2 = C\xe9.esp\r\nGameFile3=\r\n").unwrap();
        assert_eq!(game_files(&ini), ["Morrowind.esm", "B.esp", "C\u{e9}.esp"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
