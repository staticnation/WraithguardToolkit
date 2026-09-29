//! The profile store: named setups, kept as files beside the program.
//!
//! A profile is a person's setup — obstacle avoidance, placement, export, the
//! viewport — and it applies to whatever grass rules are loaded. There is usually
//! more than one worth keeping: a fast one for looking around, a careful one for
//! the real export, one somebody sent you.
//!
//! So each is its own `*.profile.toml` in the program's folder, and a small
//! `gardenfell.profiles.toml` beside them records which is in use. Loading one no
//! longer overwrites another, which is what happened when there was only ever a
//! single file.
//!
//! Everything here works on a directory passed in, so it is testable without a
//! Tauri window and without touching the real install.

use std::path::{Path, PathBuf};

/// Every profile file ends with this, which is also how they are found.
pub const SUFFIX: &str = ".profile.toml";
/// Records the active profile. Not itself a profile, hence the different suffix.
pub const STATE_FILE: &str = "gardenfell.profiles.toml";

/// The working mesh set, the one file in the store the program rewrites from memory
/// without anybody asking it to (`persist_setup`). That is why it needs a rule of its
/// own when the store moves - see [`setup_move`].
pub const SETUP_FILE: &str = "statics.toml";
/// The state file's name before the program became Gardenfell — read when the new one
/// is absent, so the rename does not forget which profile and rules were in use.
pub const OLD_STATE_FILE: &str = "grassforge.profiles.toml";





fn is_profile(file: &str) -> bool {
    file.len() > SUFFIX.len() && file.to_ascii_lowercase().ends_with(SUFFIX)
}


/// The install Gardenfell last had open.
///
/// Machine-local by nature: a path on this disk, meaningless on anybody else's. It
/// lives in the state file rather than in a profile for exactly that reason —
/// profiles are meant to be sent to other people, and a profile carrying
/// `D:\\Games\\MO2` would be a profile that cannot be shared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Install {
    /// "vanilla", "mo2" or "openmw".
    pub kind: String,
    /// What the user picked: a Data Files folder, an MO2 base folder, an openmw.cfg.
    pub path: String,
    /// The chosen MO2 profile, when the kind is "mo2".
    pub profile: String,
}

/// Everything the state file remembers. Not a profile — this one never leaves the
/// machine.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub active: Option<String>,
    /// Starred entries, by what kind of thing they are: `texture`, `mesh`, `region`.
    ///
    /// Machine-local by nature, like the install path beside it. A favourite is about the
    /// mods *this* install has — a mesh you reach for constantly because it is in your
    /// load order — so it has no business in a profile you send somebody, and it should
    /// survive switching profile or grass rules. Keyed by kind rather than by picker, so
    /// starring a ground texture once puts it at the top of every list that offers
    /// ground textures.
    pub favourites: Vec<(String, Vec<String>)>,
    /// The grass rules that were open. Kept here rather than in a file of its own: it is
    /// half of one answer to one question — what was I working on — and two files would
    /// be two ways for a start-up to disagree with itself.
    pub rules: Option<String>,
    pub install: Option<Install>,
    /// Where the .toml files live, when that is not beside the program (round 9 item 5).
    ///
    /// This one line is why the state file cannot move with the rest: something has to
    /// be findable without being told where to look, and "beside the .exe" is the only
    /// place that qualifies. Absent means the default, which is beside the .exe.
    pub store: Option<String>,
    /// The colour theme the window wore last (round 18cj), by code.
    ///
    /// The profile carries it too, and the profile is the one that *decides* — a shared
    /// profile brings its author's theme with it. This copy exists because of *when*:
    /// Robin asked for "the chosen theme to be saved both to the profile, and the
    /// profiles.toml file, so it can be set directly on program start". The profile is
    /// read a few steps into start-up, after the install is reopened; the state file is
    /// read before anything is on screen, so the window can come up already wearing it
    /// rather than flashing the default and correcting itself.
    ///
    /// Machine-local, like the install path and the favourites beside it: it follows the
    /// person's last choice on *this* machine, and every profile switch rewrites it from
    /// the profile that was opened. Absent means the palette the program ships with.
    pub theme: Option<String>,
    /// How the cell picker was last shown (round 18cp): `"map"`, or absent for the list.
    ///
    /// Robin asked for the choice to be remembered between runs, and this file is the
    /// only place that can do that for a thing that belongs to the machine rather than
    /// to a profile — the same reason the theme's copy is here. Absent means the list,
    /// which is what the picker has always opened as.
    pub cell_picker: Option<String>,
}

pub fn read_state(dir: &Path) -> State {
    let mut st = State::default();
    let text = match std::fs::read_to_string(dir.join(STATE_FILE)) {
        Ok(t) => t,
        // The rename migration: the next write lands under the new name.
        Err(_) => match std::fs::read_to_string(dir.join(OLD_STATE_FILE)) {
            Ok(t) => t,
            Err(_) => return st,
        },
    };
    let Ok(root) = crate::toml::parse(&text) else { return st };
    st.active = root.get("active").and_then(|v| v.as_str()).map(String::from);
    st.rules = root.get("rules").and_then(|v| v.as_str()).map(String::from);
    st.theme = root
        .get("theme")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    st.cell_picker = root
        .get("cell_picker")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    st.store = root
        .get("store")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    if let Some(t) = root.get("favourites").and_then(|v| v.as_table()) {
        for (kind, v) in t.iter() {
            let Some(a) = v.as_array() else { continue };
            let items: Vec<String> =
                a.iter().filter_map(|x| x.as_str().map(String::from)).collect();
            if !items.is_empty() {
                st.favourites.push((kind.clone(), items));
            }
        }
        st.favourites.sort_by(|a, b| a.0.cmp(&b.0));
    }
    if let Some(t) = root.get("install").and_then(|v| v.as_table()) {
        let g = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let i = Install { kind: g("kind"), path: g("path"), profile: g("profile") };
        if !i.kind.is_empty() && !i.path.is_empty() {
            st.install = Some(i);
        }
    }
    st
}

/// Rewrites the state file whole.
///
/// Read-modify-write, always: the file holds two unrelated things and writing one
/// of them from scratch would silently drop the other. That is how remembering the
/// install would have erased the active profile on the very next switch.
pub fn write_state(dir: &Path, st: &State) -> Result<(), String> {
    let mut o = String::new();
    o.push_str("# Gardenfell's own bookkeeping: which profile and which grass rules are in\n");
    o.push_str("# use, and which install was last open. Machine-local — safe to delete,\n");
    o.push_str("# never shared.\n");
    if let Some(a) = &st.active {
        o.push_str(&format!("active = {}\n", crate::toml::escape(a)));
    }
    if let Some(r) = &st.rules {
        o.push_str(&format!("rules  = {}\n", crate::toml::escape(r)));
    }
    if let Some(t) = &st.theme {
        o.push_str("\n# The window's colour theme, by code. The open profile's own choice is\n");
        o.push_str("# what sets this; it is kept here so the window can wear it at start-up.\n");
        o.push_str(&format!("theme  = {}\n", crate::toml::escape(t)));
    }
    if let Some(m) = &st.cell_picker {
        o.push_str("\n# How the cell picker was last shown: \"map\" or \"list\".\n");
        o.push_str(&format!("cell_picker = {}\n", crate::toml::escape(m)));
    }
    if let Some(d) = &st.store {
        o.push_str("\n# Where your profiles, grass rules and mesh setups are kept. Set from\n");
        o.push_str("# Settings; absent means beside the program.\n");
        o.push_str(&format!("store  = {}\n", crate::toml::escape(d)));
    }
    if !st.favourites.is_empty() {
        o.push_str("\n# Starred entries, shown at the top of every picker that offers that\n");
        o.push_str("# kind of thing. Safe to edit by hand.\n");
        o.push_str("[favourites]\n");
        for (kind, items) in &st.favourites {
            let list: Vec<String> = items.iter().map(|s| crate::toml::escape(s)).collect();
            o.push_str(&format!("{} = [{}]\n", kind, list.join(", ")));
        }
    }
    if let Some(i) = &st.install {
        o.push_str("\n[install]\n");
        o.push_str(&format!("kind    = {}\n", crate::toml::escape(&i.kind)));
        o.push_str(&format!("path    = {}\n", crate::toml::escape(&i.path)));
        if !i.profile.is_empty() {
            o.push_str(&format!("profile = {}\n", crate::toml::escape(&i.profile)));
        }
    }
    std::fs::write(dir.join(STATE_FILE), o).map_err(|e| format!("{}: {}", STATE_FILE, e))
}





/// Every starred entry, by kind.
pub fn favourites(dir: &Path) -> Vec<(String, Vec<String>)> {
    read_state(dir).favourites
}

/// Stars or unstars one entry. Everything else in the state file is left alone.
///
/// Case-insensitive, because these are Morrowind asset names: the same mesh is spelled
/// three ways across a load order and a favourite that only matches one of them is a
/// favourite that looks broken.
pub fn set_favourite(dir: &Path, kind: &str, item: &str, on: bool) -> Result<(), String> {
    let mut st = read_state(dir);
    let slot = match st.favourites.iter().position(|(k, _)| k == kind) {
        Some(i) => i,
        None => {
            st.favourites.push((kind.to_string(), Vec::new()));
            st.favourites.len() - 1
        }
    };
    let list = &mut st.favourites[slot].1;
    let at = list.iter().position(|x| x.eq_ignore_ascii_case(item));
    match (on, at) {
        (true, None) => list.push(item.to_string()),
        (false, Some(i)) => {
            list.remove(i);
        }
        _ => {}
    }
    st.favourites.retain(|(_, v)| !v.is_empty());
    st.favourites.sort_by(|a, b| a.0.cmp(&b.0));
    write_state(dir, &st)
}



/// Remembers the window's colour theme for the next start-up (round 18cj).
///
/// Empty means the palette the program ships with, and writes no line at all — the same
/// shape the profile's own `theme` has. Unchanged is not rewritten: this is called on
/// every profile switch, and a file rewritten to say what it already said is a disk write
/// somebody's backup tool will notice.
pub fn set_theme(dir: &Path, code: &str) -> Result<(), String> {
    let code = code.trim();
    let mut st = read_state(dir);
    let want = if code.is_empty() { None } else { Some(code.to_string()) };
    if st.theme == want {
        return Ok(());
    }
    st.theme = want;
    write_state(dir, &st)
}

/// The theme the state file remembers, for the window to wear before the profile lands.
pub fn theme(dir: &Path) -> Option<String> {
    read_state(dir).theme
}

/// Remembers how the cell picker is shown (round 18cp). `"list"` and empty both mean the
/// list, and write no line — the default needs no bookkeeping. Unchanged is not rewritten.
pub fn set_cell_picker(dir: &Path, mode: &str) -> Result<(), String> {
    let mode = mode.trim().to_ascii_lowercase();
    let mut st = read_state(dir);
    let want = if mode.is_empty() || mode == "list" { None } else { Some(mode) };
    if st.cell_picker == want {
        return Ok(());
    }
    st.cell_picker = want;
    write_state(dir, &st)
}

/// How the cell picker was last shown: `"map"`, or `"list"` when nothing is remembered.
pub fn cell_picker(dir: &Path) -> String {
    read_state(dir).cell_picker.unwrap_or_else(|| "list".to_string())
}

/// The remembered install, if its path is still there.
///
/// A folder on a drive that is not plugged in is not an error to report at startup
/// — it is a reason to come up unconnected and let the user pick. Only a path that
/// still exists is worth trying to open.
pub fn install(dir: &Path) -> Option<Install> {
    let i = read_state(dir).install?;
    if Path::new(&i.path).exists() {
        Some(i)
    } else {
        None
    }
}

pub fn remember_install(dir: &Path, i: &Install) -> Result<(), String> {
    let mut st = read_state(dir);
    st.install = Some(i.clone());
    write_state(dir, &st)
}

/// Where openmw.cfg lives, which is the same place on every install of a given
/// platform. Windows puts it under Documents; the others follow their own
/// conventions.
///
/// Returned whether or not it exists, so a file picker can start there even on a
/// machine that has never run OpenMW.
pub fn openmw_cfg() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let home = std::env::var_os("USERPROFILE")?;
        Some(PathBuf::from(home).join("Documents").join("My Games").join("OpenMW").join("openmw.cfg"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library").join("Preferences").join("openmw").join("openmw.cfg"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        /* Two homes on Linux: the XDG one, and the sandboxed one a Flatpak OpenMW keeps
           under ~/.var. The first that exists wins; when neither does, the XDG one is
           where a picker should start. */
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(x) => PathBuf::from(x).join("openmw").join("openmw.cfg"),
            None => home.as_ref()?.join(".config").join("openmw").join("openmw.cfg"),
        };
        if xdg.is_file() {
            return Some(xdg);
        }
        if let Some(h) = &home {
            let flatpak = h
                .join(".var")
                .join("app")
                .join("org.openmw.OpenMW")
                .join("config")
                .join("openmw")
                .join("openmw.cfg");
            if flatpak.is_file() {
                return Some(flatpak);
            }
        }
        Some(xdg)
    }
}







/// Where the store lives: beside the executable.
///
/// Not `%APPDATA%` — findable, hand-editable, and it travels when the folder is
/// copied. Nothing is written anywhere else on the machine.
/// Where the `.toml` files live: profiles, grass rules, the working mesh setups.
///
/// Round 9 item 5. Beside the program by default, which is where every one of these
/// files has always been and where they stay for anybody who never opens Settings.
/// `app` is the program's own folder — the state file that records this choice lives
/// there and only there, along with the two caches nobody keeps by hand (the mesh index
/// and the wording file), because a pointer that lives at the end of itself is no
/// pointer at all.
///
/// A folder that has since gone — an unplugged drive, a renamed directory — falls back
/// to `app` rather than failing: the program has to open, and opening onto an empty
/// store is a state it already knows how to be in. Creating it if it is merely absent is
/// deliberate; the person named this folder once and should not have to name it again
/// because something tidied it away.
pub fn store_dir(app: &Path) -> PathBuf {
    let Some(d) = read_state(app).store else { return app.to_path_buf() };
    let p = PathBuf::from(&d);
    if p.is_dir() {
        return p;
    }
    if std::fs::create_dir_all(&p).is_ok() && p.is_dir() {
        return p;
    }
    app.to_path_buf()
}

/// The language packs' place in a move: `languages\gardenfell.sv.toml`, relative to the
/// store, with the platform's own separator since the name is shown to a person and
/// describes their disk. `None` for a name in the store root.
pub fn pack_leaf(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(crate::lang::LANG_DIR)?;
    let leaf = rest.strip_prefix(std::path::MAIN_SEPARATOR).or_else(|| rest.strip_prefix('/'))?;
    if crate::lang::is_pack_file(leaf) { Some(leaf) } else { None }
}





/* ---------------------------------------------------------------------------------
   Backups — round 18v.

   Every file this program keeps is rewritten as you work: the profile on every control
   you touch, the grass rules on every edit, the working mesh set on every stroke. That is
   what makes them safe to leave alone, and it is also what makes one bad write final. The
   working set has had a rotation beside it since round 17z (an undo that took every
   mesh's rules with it); round 18s spent one of its three slots recovering from a store
   move that wrote an empty set over a full one, which is the argument for more of them and
   for the other two kinds having them too.

   Robin: "I want the back up files to be for both the Grass Rules and the Profile too.
   The same way as we do with the Statics file. Make it 10 steps for statics, grass rules
   and profiles respectively, but keep the backup files in a subfolder, called 'backups',
   and then subfolders for 'profiles', 'grass rules' and 'statics'."

   So: `<store>/backups/<kind>/<filename>.bak1` is the version before the current one,
   `.bak10` the oldest kept, and the rotation happens only when the bytes actually change —
   saving the same file twice never costs a slot. Out of the way of the folder somebody
   browses, which is the other half of what he asked for.
   --------------------------------------------------------------------------------- */

/// The folder the backups live in, under the store.
pub const BACKUP_DIR: &str = "backups";
/// How many versions of each file are kept.
pub const BACKUP_DEPTH: usize = 10;

/// Which backup subfolder a store file belongs in, or `None` for a file that is not the
/// store's own.
pub fn backup_kind(name: &str) -> Option<&'static str> {
    if name == SETUP_FILE {
        Some("statics")
    } else if is_profile(name) {
        Some("profiles")
    } else if pack_leaf(name).is_some() {
        /* A language pack (`languages\gardenfell.sv.toml`, round 18as): somebody's
           translation, kept before a refresh rewrites it or a move replaces it. */
        Some("languages")
    } else {
        None
    }
}

/// Where `<store>/backups/<kind>` is for this file.
pub fn backup_dir(store: &Path, name: &str) -> Option<PathBuf> {
    backup_kind(name).map(|k| store.join(BACKUP_DIR).join(k))
}

/// Files this program should never have to explain: a rotation is best-effort, and a
/// backup that could not be written is not a reason to refuse somebody's save.
fn rotate(dir: &Path, name: &str) {
    for k in (1..BACKUP_DEPTH).rev() {
        let from = dir.join(format!("{name}.bak{k}"));
        if from.exists() {
            let _ = std::fs::rename(&from, dir.join(format!("{name}.bak{}", k + 1)));
        }
    }
}

/// Keeps the version of `name` that is about to be replaced, if it differs from `new`.
///
/// Called with the bytes already on disk; `None` means there is no file there yet, which
/// is nothing to keep. Answers whether a copy was made, which is only of interest to the
/// tests — every caller treats a failure here as no reason to stop.
pub fn keep_backup(store: &Path, name: &str, new: &[u8]) -> bool {
    let Some(dir) = backup_dir(store, name) else { return false };
    let Ok(old) = std::fs::read(store.join(name)) else { return false };
    if old == new {
        return false;                       // an identical save never costs a slot
    }
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    // A pack's name carries its folder; its backups are called by the file alone.
    let name = pack_leaf(name).unwrap_or(name);
    migrate_old_backups(store, &dir, name);
    rotate(&dir, name);
    std::fs::write(dir.join(format!("{name}.bak1")), &old).is_ok()
}

/// Where the last kept version of `name` is: `<store>/backups/<kind>/<file>.bak1`.
pub fn backup_path(store: &Path, name: &str) -> Option<PathBuf> {
    let dir = backup_dir(store, name)?;
    Some(dir.join(format!("{}.bak1", pack_leaf(name).unwrap_or(name))))
}

/// The three that used to sit beside the file itself, moved in under the new folder the
/// first time this file is backed up there.
///
/// Round 17z put `statics.toml.bak1..bak3` in the store root, and one of those is what a
/// person's mesh setups came back from after round 18s. Leaving them where they are would
/// keep the safety net but hide it from the place that now holds the rest; overwriting
/// them would be the same mistake this whole section is about. So they move, keeping their
/// numbers, and only into a slot nothing has taken.
fn migrate_old_backups(store: &Path, dir: &Path, name: &str) {
    for k in 1..=3 {
        let old = store.join(format!("{name}.bak{k}"));
        let to = dir.join(format!("{name}.bak{k}"));
        if old.is_file() && !to.exists() {
            let _ = std::fs::rename(&old, &to);
        }
    }
}

/// The program's own folder — beside the executable — when that folder takes writes.
///
/// An installed program often lives somewhere it may not write: `Program Files` from the
/// Windows installer, `/usr/bin` from the `.deb`, or the read-only image an AppImage
/// mounts for itself, which is gone the moment it exits. Writing beside the program there
/// would fail on Windows and Linux alike, and silently lose everything on the AppImage.
/// So the folder is probed once per run, and when it will not take a file the per-user
/// data folder stands in: `%APPDATA%\Gardenfell`, `~/Library/Application Support/Gardenfell`,
/// `$XDG_DATA_HOME/gardenfell` (`~/.local/share/gardenfell`). Somebody who unpacks the
/// program into a folder of their own keeps the old behaviour exactly: files beside it.
pub fn dir_beside_exe() -> Result<PathBuf, String> {
    /* Overridable so a test run gets its own folder. Without it every suite shares the
       profiles next to the binary, one test's active profile leaks into the next, and
       the order they happen to run in decides whether they pass. Not a product
       feature: nothing sets this but a harness. */
    /* The old spelling is still read, and read *second*: somebody with both set means
       the new one. Every sidecar this program has ever written keeps its old name as a
       fallback for the same reason — a rename must not lose the settings somebody spent
       a month making. */
    if let Some(p) = override_dir() {
        std::fs::create_dir_all(&p).map_err(|e| format!("{}: {}", p.display(), e))?;
        return Ok(p);
    }
    /* Decided once: the answer does not change while the program runs, and the probe
       writes a file. */
    static DIR: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| Ok(app_dir_for(&beside_exe()?))).clone()
}

/// Where language packs live: `languages\` under the store — beside the program until
/// Settings says otherwise, and then in the folder chosen there, so a pack travels with
/// the rest of a person's files when they move. Round 18as, Robin: "put them in my
/// selected folder instead under a languages folder". (Round 18aq had them beside the
/// program only.) The harness override stands for the program's folder here as it does
/// everywhere else, so a suite gets a folder of its own.
pub fn languages_dir() -> Result<PathBuf, String> {
    Ok(store_dir(&dir_beside_exe()?).join(crate::lang::LANG_DIR))
}

/// Opens a folder in the system's file browser — Explorer, Finder, whatever `xdg-open`
/// finds — and comes back at once; the window is the person's from then on. Round 18as,
/// the "Open folder" and "Open languages folder" buttons in Settings.
pub fn show_folder(dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| crate::msg!("eng.open_failed", dir = dir.display().to_string(), err = e.to_string()))
}

fn override_dir() -> Option<PathBuf> {
    let d = std::env::var("GARDENFELL_PROFILE_DIR").or_else(|_| std::env::var("GRASSFORGE_PROFILE_DIR")).ok()?;
    if d.is_empty() {
        return None;
    }
    Some(PathBuf::from(d))
}

fn beside_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate the app: {}", e))?;
    Ok(exe.parent().ok_or("the app has no parent directory")?.to_path_buf())
}


/// `beside` when it takes writes, else the per-user data folder, else `beside` after all
/// — so that a machine with neither fails where it always failed, on the write, with the
/// path in the message.
pub fn app_dir_for(beside: &Path) -> PathBuf {
    if takes_writes(beside) {
        return beside.to_path_buf();
    }
    match user_data_dir() {
        Some(d) if std::fs::create_dir_all(&d).is_ok() && takes_writes(&d) => d,
        _ => beside.to_path_buf(),
    }
}

/// Whether a file can be made in `dir`, found out the only reliable way: by making one.
/// Permission bits say what the owner may do, not what this user may; a read-only mount
/// says nothing at all until it refuses.
fn takes_writes(dir: &Path) -> bool {
    let probe = dir.join(".gardenfell-write-probe");
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        // Left by a run that ended between making it and removing it. Removable means
        // writable.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => std::fs::remove_file(&probe).is_ok(),
        Err(_) => false,
    }
}

/// The platform's per-user data folder, with the program's name on it.
pub fn user_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let a = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(a).join("Gardenfell"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library").join("Application Support").join("Gardenfell"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
            if !x.is_empty() {
                return Some(PathBuf::from(x).join("gardenfell"));
            }
        }
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".local").join("share").join("gardenfell"))
    }
}
