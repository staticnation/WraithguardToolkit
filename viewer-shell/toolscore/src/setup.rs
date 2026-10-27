//! The Setup page's checks (port step 4.5): each path the app runs on - the openmw.cfg, the
//! customizations TOML, the rule files, MOMW's plugin-order.yml, tes3cmd, the folders - read
//! the way the program will read it, and what is wrong said before anything is loaded with it.
//!
//! The point is a malformed install: the page opens on these checks rather than on a load that
//! cannot finish, so a cfg naming folders that are gone, a TOML that does not parse or a rule
//! file that is not there is named, with what to change, while the program is still usable.
//!
//! Nothing here loads plugins or walks the data folders' files: a check is a read of the file
//! itself, fast enough to run as the person types.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// How a check came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Nothing given (and nothing needed).
    Empty,
    Ok,
    /// Usable, with something worth knowing.
    Warn,
    /// The program cannot use it as it is.
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Empty => "empty",
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

/// A check: its level and the lines that say what was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub lines: Vec<String>,
}

impl Check {
    fn new(level: Level, line: impl Into<String>) -> Check {
        Check { level, lines: vec![line.into()] }
    }
    fn push(&mut self, level: Level, line: impl Into<String>) {
        if rank(level) > rank(self.level) {
            self.level = level;
        }
        self.lines.push(line.into());
    }
    pub fn json(&self) -> Value {
        json!({"level": self.level.name(), "lines": self.lines})
    }
}

fn rank(l: Level) -> u8 {
    match l {
        Level::Empty => 0,
        Level::Ok => 1,
        Level::Warn => 2,
        Level::Error => 3,
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A file that must be there: `Err` with the check to give when it is not.
fn file(path: &str) -> Result<PathBuf, Check> {
    let p = PathBuf::from(path.trim());
    if p.is_dir() {
        return Err(Check::new(Level::Error, "This is a folder; a file is wanted here."));
    }
    if !p.is_file() {
        return Err(Check::new(Level::Error, "No file is there."));
    }
    Ok(p)
}

/// The openmw.cfg: its data folders (how many are missing), its content and grass, the cfgs it
/// chains, and the lines it could not follow.
pub fn cfg(path: &str) -> Check {
    if path.trim().is_empty() {
        return Check::new(Level::Error, "Every page needs the openmw.cfg: choose it, or let Find look where OpenMW keeps it.");
    }
    let p = match file(path) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let got = match ordercore::cfg::read(&p, &ordercore::cfg::Tokens::for_cfg(&p)) {
        Ok(g) => g,
        Err(e) => return Check::new(Level::Error, format!("It could not be read: {e}")),
    };
    let mut c = Check::new(
        Level::Ok,
        format!(
            "{}, {}{}.",
            plural(got.data.len(), "data folder", "data folders"),
            plural(got.content.len(), "content file", "content files"),
            if got.groundcover.is_empty() { String::new() } else { format!(", {} of grass", got.groundcover.len()) }
        ),
    );
    if got.files.len() > 1 {
        c.push(Level::Ok, format!("Read with {}.", plural(got.files.len() - 1, "cfg it chains", "cfgs it chains")));
    }
    if got.content.is_empty() {
        c.push(Level::Warn, "It loads no content files: is this the cfg OpenMW uses?");
    }
    if !got.missing_data.is_empty() {
        let shown: Vec<String> = got.missing_data.iter().take(5).map(|d| d.display().to_string()).collect();
        c.push(
            Level::Warn,
            format!(
                "{} not there: {}{}. Their files will be missing until the folders are back or the lines go.",
                plural(got.missing_data.len(), "data folder is", "data folders are"),
                shown.join("; "),
                if got.missing_data.len() > 5 { format!(" and {} more", got.missing_data.len() - 5) } else { String::new() }
            ),
        );
    }
    for w in got.warnings.iter().take(5) {
        c.push(Level::Warn, w.clone());
    }
    if got.data.is_empty() {
        c.push(Level::Error, "No data folder it names exists, so nothing can be loaded from it.");
    }
    c
}

/// A momw-customizations TOML: that it parses, and its blocks and lists.
pub fn customizations(path: &str) -> Check {
    if path.trim().is_empty() {
        return Check { level: Level::Empty, lines: Vec::new() };
    }
    let p = match file(path) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let text = match std::fs::read(&p).map(String::from_utf8) {
        Ok(Ok(t)) => t,
        Ok(Err(_)) => return Check::new(Level::Error, "It is not UTF-8 text."),
        Err(e) => return Check::new(Level::Error, format!("It could not be read: {e}")),
    };
    let table = match text.trim_start_matches('\u{feff}').parse::<toml::Table>() {
        Ok(t) => t,
        Err(e) => return Check::new(Level::Error, format!("It is not valid TOML: {}", e.message())),
    };
    let blocks = table.get("Customizations").and_then(toml::Value::as_array).map_or(0, Vec::len);
    if blocks == 0 {
        return Check::new(Level::Warn, "It has no [[Customizations]] block, so it adds nothing to the sort.");
    }
    let lists: Vec<String> = table["Customizations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b.get("listName").and_then(toml::Value::as_str).map(str::to_string))
        .collect();
    let mut c = Check::new(Level::Ok, format!("{}.", plural(blocks, "block", "blocks")));
    if !lists.is_empty() {
        c.push(Level::Ok, format!("For the list{} {}.", if lists.len() == 1 { "" } else { "s" }, lists.join(", ")));
    }
    if ordercore::plan::customization_blocks(&text).is_err() {
        c.push(Level::Warn, "Something in it is not of the shape the sort reads; the sort will say what when it runs.");
    }
    c
}

/// The rule files, one a line (files, or folders of `*.txt` rule files): each there, and its
/// ordering rules counted.
pub fn rules(lines: &str) -> Check {
    let entries: Vec<&str> = lines.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if entries.is_empty() {
        return Check::new(
            Level::Warn,
            "No rule files: the sort keeps the cfg's order. Add mlox_base.txt (Update rules on the Load order page downloads it), then your own.",
        );
    }
    let mut c = Check { level: Level::Ok, lines: Vec::new() };
    for e in entries {
        let p = Path::new(e);
        let name = p.file_name().map_or_else(|| e.to_string(), |n| n.to_string_lossy().into_owned());
        if p.is_dir() {
            let n = std::fs::read_dir(p)
                .map(|rd| rd.flatten().filter(|f| f.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("txt"))).count())
                .unwrap_or(0);
            let level = if n == 0 { Level::Warn } else { Level::Ok };
            c.push(level, format!("{name}: a folder of {}.", plural(n, "rule file", "rule files")));
            continue;
        }
        match std::fs::read(p) {
            Ok(b) => {
                let text = String::from_utf8_lossy(b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&b)).into_owned();
                let parsed = ordercore::rules::parse_text(&text);
                let level = if parsed.blocks.is_empty() { Level::Warn } else { Level::Ok };
                c.push(level, format!("{name}: {}.", plural(parsed.blocks.len(), "ordering rule", "ordering rules")));
            }
            Err(_) if p.exists() => c.push(Level::Error, format!("{name}: could not be read.")),
            Err(_) => c.push(Level::Error, format!("{name}: no file is there.")),
        }
    }
    c
}

/// MOMW's plugin-order.yml: that it reads, and how many plugins and lists it names.
pub fn plugin_order_yml(path: &str) -> Check {
    if path.trim().is_empty() {
        return Check { level: Level::Empty, lines: Vec::new() };
    }
    let p = match file(path) {
        Ok(p) => p,
        Err(c) => {
            return Check::new(Level::Warn, format!("{} Update plugin-order.yml on the Load order page downloads it here.", c.lines[0]));
        }
    };
    match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| ordercore::momw::parse_bytes(&b).map_err(|e| e.to_string())) {
        Ok(entries) => {
            let mut lists: Vec<&str> = entries.iter().flat_map(|e| e.on_lists.iter().map(String::as_str)).collect();
            lists.sort_unstable();
            lists.dedup();
            Check::new(Level::Ok, format!("{} on {}.", plural(entries.len(), "plugin", "plugins"), plural(lists.len(), "list", "lists")))
        }
        Err(e) => Check::new(Level::Error, format!("It does not read: {e}")),
    }
}

/// A file that only has to be there (a data-paths cache, a subset file, tes3cmd).
pub fn existing_file(path: &str, absent: Level) -> Check {
    if path.trim().is_empty() {
        return Check { level: Level::Empty, lines: Vec::new() };
    }
    match file(path) {
        Ok(_) => Check::new(Level::Ok, "Found."),
        Err(mut c) => {
            c.level = absent;
            c
        }
    }
}

/// A folder that has to be there.
pub fn existing_folder(path: &str) -> Check {
    if path.trim().is_empty() {
        return Check { level: Level::Empty, lines: Vec::new() };
    }
    let p = Path::new(path.trim());
    if p.is_dir() {
        Check::new(Level::Ok, "Found.")
    } else if p.exists() {
        Check::new(Level::Error, "This is a file; a folder is wanted here.")
    } else {
        Check::new(Level::Error, "No folder is there.")
    }
}

/// A file the program writes (the TOML Export writes, your rule file): its folder must be there.
pub fn writable_file(path: &str) -> Check {
    if path.trim().is_empty() {
        return Check { level: Level::Empty, lines: Vec::new() };
    }
    let p = Path::new(path.trim());
    if p.is_dir() {
        return Check::new(Level::Error, "This is a folder; a file name is wanted here.");
    }
    match p.parent().filter(|d| !d.as_os_str().is_empty()) {
        Some(d) if !d.is_dir() => Check::new(Level::Error, "Its folder is not there."),
        _ if p.is_file() => Check::new(Level::Ok, "There now; it is written over (a backup first, unless that is turned off)."),
        _ => Check::new(Level::Ok, "It will be made."),
    }
}

/// Every field's check, by the page's field names (`settings::all_fields`'s).
pub fn check_all(f: &serde_json::Map<String, Value>) -> Value {
    let t = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    json!({
        "cfg": cfg(&t("cfg")).json(),
        "customizations": customizations(&t("customizations")).json(),
        "subsetFile": existing_file(&t("subsetFile"), Level::Error).json(),
        "rules": rules(&t("rules")).json(),
        "userRules": writable_file(&t("userRules")).json(),
        "pluginOrderYml": plugin_order_yml(&t("pluginOrderYml")).json(),
        "dataPathsCache": existing_file(&t("dataPathsCache"), Level::Warn).json(),
        "emitToml": writable_file(&t("emitToml")).json(),
        "scanDir": existing_folder(&t("scanDir")).json(),
        "tes3cmd": existing_file(&t("tes3cmd"), Level::Error).json(),
        "mergeLandsFolder": existing_folder(&t("mergeLandsFolder")).json(),
        "openmwInstall": existing_folder(&t("openmwInstall")).json(),
    })
}

/// Where OpenMW keeps its openmw.cfg on this machine, those that exist: the user's config
/// folder (Windows' Documents\My Games\OpenMW, Linux's ~/.config/openmw and the Flatpak's,
/// macOS's Library/Preferences/openmw). `home` and `documents` are the user's folders.
pub fn find_cfgs(home: Option<&Path>, documents: Option<&Path>) -> Vec<PathBuf> {
    let mut at: Vec<PathBuf> = Vec::new();
    if let Some(d) = documents {
        at.push(d.join("My Games").join("OpenMW").join("openmw.cfg"));
    }
    if let Some(h) = home {
        at.push(h.join("Documents").join("My Games").join("OpenMW").join("openmw.cfg"));
        at.push(h.join(".config").join("openmw").join("openmw.cfg"));
        at.push(h.join(".var/app/org.openmw.OpenMW/config/openmw/openmw.cfg"));
        at.push(h.join("Library/Preferences/openmw/openmw.cfg"));
    }
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        at.insert(0, PathBuf::from(x).join("openmw").join("openmw.cfg"));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for p in at {
        if p.is_file() && !out.iter().any(|q| q == &p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wg_setup_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_cfg_with_a_missing_folder_says_so() {
        let d = tmp("cfg");
        std::fs::create_dir_all(d.join("Data Files")).unwrap();
        let cfg = d.join("openmw.cfg");
        std::fs::write(&cfg, format!("data=\"{}\"\ndata=\"{}\"\ncontent=Morrowind.esm\n", d.join("Data Files").display(), d.join("Gone").display())).unwrap();
        let c = super::cfg(&cfg.to_string_lossy());
        assert_eq!(c.level, Level::Warn);
        assert_eq!(c.lines[0], "1 data folder, 1 content file.");
        assert!(c.lines[1].starts_with("1 data folder is not there:"));
        assert_eq!(super::cfg("").level, Level::Error);
        assert_eq!(super::cfg(&d.to_string_lossy()).lines[0], "This is a folder; a file is wanted here.");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_broken_toml_and_a_missing_rule_file() {
        let d = tmp("toml");
        let t = d.join("c.toml");
        std::fs::write(&t, "[[Customizations]]\nlistName = \"x\"\n").unwrap();
        let c = customizations(&t.to_string_lossy());
        assert_eq!((c.level, c.lines.clone()), (Level::Ok, vec!["1 block.".to_string(), "For the list x.".to_string()]));
        std::fs::write(&t, "[[Customizations]\n").unwrap();
        let c = customizations(&t.to_string_lossy());
        assert_eq!(c.level, Level::Error);
        assert!(c.lines[0].starts_with("It is not valid TOML"));
        let r = d.join("mlox_base.txt");
        std::fs::write(&r, "[Order]\nA.esp\nB.esp\n").unwrap();
        let c = rules(&format!("{}\n{}", r.display(), d.join("none.txt").display()));
        assert_eq!(c.level, Level::Error);
        assert_eq!(c.lines, vec!["mlox_base.txt: 1 ordering rule.".to_string(), "none.txt: no file is there.".to_string()]);
        assert_eq!(rules("").level, Level::Warn);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn find_looks_in_openmws_folders() {
        let d = tmp("find");
        let at = d.join(".config/openmw");
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join("openmw.cfg"), "").unwrap();
        assert!(find_cfgs(Some(&d), None).contains(&at.join("openmw.cfg")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
