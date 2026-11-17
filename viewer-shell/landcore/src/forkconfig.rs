//! OpenMWMergedLands' own settings, `merged_lands.toml`, read so that someone coming from it keeps
//! what they set there (Static, 2026-11-16). Only read: Wraithguard's settings stay its own (the
//! Tools page, the command line), and this fills them in when asked.
//!
//! The fork keeps the file in the folder its `openmw.cfg` is in, or OpenMW's own config folder
//! (`MergedLandsConfig::resolve_location`), or beside its program. What is taken from it: the
//! plugins it leaves out (`ignore_plugins`), the folders it leaves out (`ignore_plugins_from_path`,
//! relative to the file's folder), the cells it leaves out for every plugin (`cell_ignore`) and for
//! one (`cell_ignore_by_plugin`), and where it writes (`output_file_dir`). The rest of the file is
//! the fork's own bookkeeping (`generated_output_*`, `openmw_cfg`) and is not used.

use std::path::{Path, PathBuf};

use crate::landmass::{Coords, IgnoredCells};

/// The file's name.
pub const FILE_NAME: &str = "merged_lands.toml";

/// What was read, and from where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForkConfig {
    pub path: PathBuf,
    /// Plugins by name, and folders (absolute) left out, in the file's order.
    pub ignore: Vec<String>,
    pub ignored_cells: IgnoredCells,
    /// The cells as Wraithguard writes them (`Plugin.esp=X,Y;X,Y | X,Y`), the plugins' names
    /// spelled as the file spells them.
    pub cells_text: String,
    pub output_dir: Option<PathBuf>,
}

/// The first of `folders` that holds the file.
pub fn find(folders: &[PathBuf]) -> Option<PathBuf> {
    folders.iter().map(|d| d.join(FILE_NAME)).find(|p| p.is_file())
}

/// The folders the fork looks in for a setup whose `openmw.cfg` is `cfg` (or the folder holding
/// it): that folder, then OpenMW's own config folder for this user.
pub fn folders_for(cfg: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(c) = cfg {
        let dir = if c.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("openmw.cfg")) { c.parent().map(Path::to_path_buf) } else if c.is_dir() { Some(c.to_path_buf()) } else { None };
        out.extend(dir);
    }
    if let Some(h) = std::env::var_os("USERPROFILE") {
        out.push(PathBuf::from(h).join("Documents").join("My Games").join("OpenMW"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        let h = PathBuf::from(h);
        out.push(h.join(".config").join("openmw"));
        out.push(h.join("Library").join("Preferences").join("openmw"));
    }
    out.dedup();
    out
}

/// The file at `path`, read. A field of the wrong shape is an error that names it (the fork stops
/// on one too); a field missing is the fork's empty default.
pub fn read(path: &Path) -> Result<ForkConfig, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{} could not be read: {e}", path.display()))?;
    let table: toml::Table = text.parse().map_err(|e| format!("{} is not TOML the fork could read: {e}", path.display()))?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let resolve = |s: &str| {
        let p = PathBuf::from(s);
        if p.is_absolute() { p } else { dir.join(p) }
    };
    let strings = |key: &str| -> Result<Vec<String>, String> {
        match table.get(key) {
            None => Ok(Vec::new()),
            Some(toml::Value::Array(a)) => a.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(|| format!("{key}: every entry is to be a string"))).collect(),
            Some(_) => Err(format!("{key} is to be a list of strings")),
        }
    };
    let cells = |key: &str, v: &toml::Value| -> Result<Vec<Coords>, String> {
        let toml::Value::Array(a) = v else { return Err(format!("{key} is to be a list of [x, y]")) };
        a.iter()
            .map(|c| match c.as_array().map(|p| p.iter().map(toml::Value::as_integer).collect::<Vec<_>>()) {
                Some(p) if p.len() == 2 && p.iter().all(Option::is_some) => Ok((p[0].unwrap(), p[1].unwrap())),
                _ => Err(format!("{key}: every cell is to be [x, y], two whole numbers")),
            })
            .collect()
    };
    let mut ignore = strings("ignore_plugins")?;
    // The fork's other spellings of it, as its reader takes them.
    for key in ["ignore_plugins_from_path", "ignore_plugins_from_paths", "ignore_paths"] {
        ignore.extend(strings(key)?.iter().map(|f| resolve(f).display().to_string()));
    }
    let mut out = ForkConfig { path: path.to_path_buf(), ignore, ..Default::default() };
    let mut text_parts: Vec<String> = Vec::new();
    if let Some(v) = table.get("cell_ignore") {
        let got = cells("cell_ignore", v)?;
        if !got.is_empty() {
            text_parts.push(got.iter().map(|(x, y)| format!("{x},{y}")).collect::<Vec<_>>().join("; "));
        }
        out.ignored_cells.everywhere.extend(got);
    }
    match table.get("cell_ignore_by_plugin") {
        None => {}
        Some(toml::Value::Table(t)) => {
            for (plugin, v) in t {
                let got = cells(&format!("cell_ignore_by_plugin.{plugin}"), v)?;
                if got.is_empty() {
                    continue;
                }
                text_parts.push(format!("{plugin}={}", got.iter().map(|(x, y)| format!("{x},{y}")).collect::<Vec<_>>().join(";")));
                out.ignored_cells.by_plugin.entry(plugin.to_ascii_lowercase()).or_default().extend(got);
            }
        }
        Some(_) => return Err("cell_ignore_by_plugin is to be a table of plugin = [[x, y], ...]".into()),
    }
    out.cells_text = text_parts.join(" | ");
    for key in ["output_file_dir", "default_output_file_dir"] {
        if let Some(v) = table.get(key) {
            out.output_dir = Some(resolve(v.as_str().ok_or_else(|| format!("{key} is to be a folder"))?));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fork's README example, as it writes it.
    #[test]
    fn the_forks_settings_are_read_as_it_reads_them() {
        let d = std::env::temp_dir().join(format!("wg_forkcfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join(FILE_NAME);
        std::fs::write(
            &file,
            "ignore_plugins = [\"Some Generated Plugin.omwaddon\"]\n\
             ignore_plugins_from_path = [\"generated\"]\n\
             cell_ignore = [[-2, 3], [0, 0]]\n\
             output_file_dir = \"Merged Output\"\n\
             generated_output_dir = \"default_data_local\"\n\
             generated_output_files = [\"Merged Lands.omwaddon\"]\n\
             [cell_ignore_by_plugin]\n\
             \"TR_Mainland.esm\" = [[-9, -46], [-9, -47]]\n\
             \"Empty.esp\" = []\n",
        )
        .unwrap();
        assert_eq!(find(&[d.join("nothing"), d.clone()]), Some(file.clone()));
        let got = read(&file).unwrap();
        assert_eq!(got.ignore, vec!["Some Generated Plugin.omwaddon".to_string(), d.join("generated").display().to_string()]);
        assert_eq!(got.ignored_cells.everywhere.iter().copied().collect::<Vec<_>>(), vec![(-2, 3), (0, 0)]);
        assert_eq!(got.ignored_cells.by_plugin["tr_mainland.esm"].len(), 2);
        assert_eq!(got.cells_text, "-2,3; 0,0 | TR_Mainland.esm=-9,-46;-9,-47");
        // What the page shows reads back as the same cells.
        assert_eq!(crate::pipeline::parse_ignored_cells(&got.cells_text).unwrap(), got.ignored_cells);
        assert_eq!(got.output_dir, Some(d.join("Merged Output")));
        std::fs::write(&file, "cell_ignore = [[1, 2, 3]]\n").unwrap();
        assert!(read(&file).unwrap_err().contains("cell_ignore: every cell is to be [x, y]"));
        std::fs::write(&file, "").unwrap();
        assert_eq!(read(&file).unwrap().ignore, Vec::<String>::new());
        assert_eq!(folders_for(Some(&d.join("openmw.cfg")))[0], d);
        let _ = std::fs::remove_dir_all(&d);
    }
}
