//! Per-plugin merge settings, the `.mergedlands.toml` beside a plugin (`wraithguard/land/meta.py`,
//! ported from Merged Lands' `io/meta_schema.rs`, MIT, David Von Derau).
//!
//! Two knobs per layer: `included = false` drops that plugin's edits to the layer;
//! `conflict_strategy` decides what happens where its edits collide with another mod's. The
//! defaults are permissive on purpose (every layer included, `Auto`), so a plugin with no file
//! merges exactly as if this did not exist. `height_map` governs the vertex normals too: the
//! record stores both under one flag, and a merge that moved a height without its normal would
//! light the old terrain.
//!
//! Example, dropping everything but the world map and forcing that to win:
//!
//! ```toml
//! version = "0"
//! meta_type = "Patch"
//!
//! [height_map]
//! included = false
//!
//! [vertex_colors]
//! included = false
//!
//! [texture_indices]
//! included = false
//!
//! [world_map_data]
//! conflict_strategy = "Overwrite"
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

/// The suffix a sidecar carries after the plugin's own name (`META_SUFFIX`).
pub const META_SUFFIX: &str = ".mergedlands.toml";

/// The only schema version understood; an unknown one is refused, not misread.
pub const SUPPORTED_VERSION: &str = "0";

/// `meta_type` values: `Patch` a hand-written file, `MergedLands` a generated merge, which a
/// re-run ignores instead of merging a merge.
pub const META_AUTO: &str = "Auto";
pub const META_PATCH: &str = "Patch";
pub const META_MERGED_LANDS: &str = "MergedLands";
pub const META_TYPES: [&str; 3] = [META_AUTO, META_PATCH, META_MERGED_LANDS];

/// The layers a landscape record carries (`LandData`), as bits.
pub mod layer {
    pub const NONE: u32 = 0;
    pub const VERTEX_COLORS: u32 = 0b00010;
    pub const TEXTURES: u32 = 0b00100;
    pub const VERTEX_HEIGHTS: u32 = 0b01000;
    pub const VERTEX_NORMALS: u32 = 0b10000;
    pub const WORLD_MAP: u32 = 0b100000;
}

/// The layer names the file uses, in file order, and the layers each governs (`LAYER_NAMES`).
pub const LAYER_NAMES: [(&str, u32); 4] = [
    ("height_map", layer::VERTEX_HEIGHTS | layer::VERTEX_NORMALS),
    ("vertex_colors", layer::VERTEX_COLORS),
    ("texture_indices", layer::TEXTURES),
    ("world_map_data", layer::WORLD_MAP),
];

/// How to settle a vertex two plugins moved (`ConflictStrategy`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Strategy {
    /// Whichever suits the layer (load-order winner today).
    #[default]
    Auto,
    /// Average the two edits, weighted toward the larger.
    Resolve,
    /// The later plugin's edit.
    Overwrite,
    /// The earlier plugin's edit.
    Ignore,
    /// Weighted by the structure each edit introduced; heights only. Ours, not Merged Lands'.
    Curvature,
}

/// The `conflict_strategy` names, in the order a chooser offers them (`STRATEGY_NAMES`).
pub const STRATEGY_NAMES: [&str; 5] = ["Auto", "Resolve", "Overwrite", "Ignore", "Curvature"];

impl Strategy {
    pub const ALL: [Strategy; 5] = [Strategy::Auto, Strategy::Resolve, Strategy::Overwrite, Strategy::Ignore, Strategy::Curvature];

    /// The file's spelling (`strategy_display_name`).
    pub fn name(self) -> &'static str {
        STRATEGY_NAMES[Strategy::ALL.iter().position(|s| *s == self).unwrap_or(0)]
    }

    /// The strategy a name spells (`strategy_from_name`), exactly as written.
    pub fn from_name(name: &str) -> Option<Strategy> {
        STRATEGY_NAMES.iter().position(|n| *n == name).map(|i| Strategy::ALL[i])
    }
}

/// How one layer of one plugin is treated (`MergeSettings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeSettings {
    pub included: bool,
    pub conflict_strategy: Strategy,
}

impl Default for MergeSettings {
    fn default() -> Self {
        MergeSettings { included: true, conflict_strategy: Strategy::Auto }
    }
}

/// Everything a sidecar says about one plugin (`PluginMeta`): layers by name, in the order
/// the file gave them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMeta {
    pub meta_type: String,
    pub layers: Vec<(String, MergeSettings)>,
}

impl Default for PluginMeta {
    fn default() -> Self {
        PluginMeta { meta_type: META_AUTO.into(), layers: Vec::new() }
    }
}

impl PluginMeta {
    /// One layer's settings, permissive when the file did not mention it.
    pub fn settings_for(&self, name: &str) -> MergeSettings {
        self.layers.iter().find(|(n, _)| n == name).map(|(_, s)| *s).unwrap_or_default()
    }

    /// Whether this marks a plugin the tool generated itself (`is_previous_merge`).
    pub fn is_previous_merge(&self) -> bool {
        self.meta_type == META_MERGED_LANDS
    }

    /// The layers this plugin may contribute at all (`allowed_layers`).
    pub fn allowed_layers(&self) -> u32 {
        LAYER_NAMES.iter().filter(|(n, _)| self.settings_for(n).included).fold(layer::NONE, |a, (_, f)| a | f)
    }

    /// The strategy governing a layer (`strategy_for`).
    pub fn strategy_for(&self, bits: u32) -> Strategy {
        LAYER_NAMES.iter().find(|(_, f)| bits & f != 0).map(|(n, _)| self.settings_for(n).conflict_strategy).unwrap_or_default()
    }

    /// Every layer spelled out, as the editor starts from it: these settings where the file
    /// gave them, the defaults elsewhere.
    pub fn full(&self) -> Vec<(&'static str, MergeSettings)> {
        LAYER_NAMES.iter().map(|(n, _)| (*n, self.settings_for(n))).collect()
    }
}

/// A sidecar that exists and cannot be trusted (`MetaError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaError(pub String);

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MetaError {}

/// Python's `str()` of a TOML value, for the version check and the strategy lookup.
fn py_str(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Boolean(b) => if *b { "True" } else { "False" }.into(),
        toml::Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        other => format!("{other:?}"),
    }
}

/// Python's `repr()` of a TOML value, for the messages.
fn py_repr(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => {
            if s.contains('\'') && !s.contains('"') {
                format!("\"{s}\"")
            } else {
                format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
            }
        }
        other => py_str(other),
    }
}

/// A parsed sidecar as settings (`parse_meta`). An unknown version, meta_type, layer or
/// strategy is refused rather than defaulted: a typo would otherwise silently give `Auto`,
/// usually the opposite of why the file was written.
pub fn parse_meta(doc: &toml::Table, name: &str) -> Result<PluginMeta, MetaError> {
    if let Some(version) = doc.get("version")
        && py_str(version) != SUPPORTED_VERSION
    {
        return Err(MetaError(format!(
            "{name} declares version {}, and only '{SUPPORTED_VERSION}' is understood. A newer file may mean something different by the same keys.",
            py_repr(version)
        )));
    }
    let meta_type = match doc.get("meta_type") {
        None => META_AUTO.to_string(),
        Some(toml::Value::String(s)) if META_TYPES.contains(&s.as_str()) => s.clone(),
        Some(other) => return Err(MetaError(format!("{name} has an unknown meta_type {}", py_repr(other)))),
    };
    let mut layers = Vec::new();
    for (key, value) in doc {
        if key == "version" || key == "meta_type" {
            continue;
        }
        if !LAYER_NAMES.iter().any(|(n, _)| n == key) {
            let mut known: Vec<&str> = LAYER_NAMES.iter().map(|(n, _)| *n).collect();
            known.sort_unstable();
            return Err(MetaError(format!(
                "{name} configures '{key}', which is not a landscape layer. Expected one of: {}.",
                known.join(", ")
            )));
        }
        let Some(table) = value.as_table() else {
            return Err(MetaError(format!("{name}: [{key}] must be a table")));
        };
        let included = match table.get("included") {
            None => true,
            Some(toml::Value::Boolean(b)) => *b,
            Some(_) => return Err(MetaError(format!("{name}: [{key}] included must be true or false"))),
        };
        let strategy = match table.get("conflict_strategy") {
            None => Strategy::Auto,
            Some(raw) => Strategy::from_name(&py_str(raw)).ok_or_else(|| {
                MetaError(format!(
                    "{name}: [{key}] conflict_strategy {} is not recognised. Expected one of: {}.",
                    py_repr(raw),
                    STRATEGY_NAMES.join(", ")
                ))
            })?,
        };
        layers.push((key.clone(), MergeSettings { included, conflict_strategy: strategy }));
    }
    Ok(PluginMeta { meta_type, layers })
}

/// Text as a sidecar (`_load_toml` then `parse_meta`).
pub fn parse_text(text: &str, name: &str) -> Result<PluginMeta, MetaError> {
    let doc: toml::Table = text.parse().map_err(|e: toml::de::Error| MetaError(format!("{name} is not valid TOML: {}", e.message())))?;
    parse_meta(&doc, name)
}

/// Where a plugin's sidecar lives, whether or not it exists (`meta_path_for`).
pub fn meta_path_for(plugin: &Path) -> PathBuf {
    let stem = plugin.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    plugin.with_file_name(format!("{stem}{META_SUFFIX}"))
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A plugin's sidecar, or the permissive defaults when it has none (`load_meta`).
pub fn load_meta(plugin: &Path) -> Result<PluginMeta, MetaError> {
    let path = meta_path_for(plugin);
    if !path.is_file() {
        return Ok(PluginMeta::default());
    }
    let name = file_name(&path);
    let bytes = std::fs::read(&path).map_err(|e| MetaError(format!("could not read {name}: {e}")))?;
    let text = String::from_utf8(bytes).map_err(|_| MetaError(format!("{name} is not valid TOML: it is not UTF-8")))?;
    parse_text(&text, &name)
}

/// Settings as `.mergedlands.toml` text (`render_meta`), the inverse of [`parse_meta`].
/// `explicit` spells every layer out, defaults too; otherwise a layer at its default is left
/// out, as Merged Lands writes it.
pub fn render_meta(meta: &PluginMeta, explicit: bool) -> String {
    let mut lines = vec![format!("version = \"{SUPPORTED_VERSION}\""), format!("meta_type = \"{}\"", meta.meta_type), String::new()];
    for (name, _) in LAYER_NAMES {
        let s = meta.settings_for(name);
        if !explicit && s == MergeSettings::default() {
            continue;
        }
        lines.push(format!("[{name}]"));
        lines.push(if s.included { "included = true" } else { "included = false" }.into());
        lines.push(format!("conflict_strategy = \"{}\"", s.conflict_strategy.name()));
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n").trim_end_matches('\n'))
}

/// The comment block above a hand-editable file (`_template_header`).
pub fn template_header() -> String {
    let layers: Vec<&str> = LAYER_NAMES.iter().map(|(n, _)| *n).collect();
    format!(
        "# Merged Lands patch for the plugin beside this file.\n\
         # It controls how this plugin's landscape edits are merged.\n\
         #\n\
         # meta_type        = {META_AUTO} | {META_PATCH} | {META_MERGED_LANDS}  (use {META_PATCH} for a hand-written patch)\n\
         # Layers: {}\n\
         #   (height_map governs vertex normals too.)\n\
         # Per layer:\n\
         #   included          = true | false   (false drops this plugin's edits to the layer)\n\
         #   conflict_strategy = {}\n\
         # Delete a section to accept its defaults (included = true, Auto).\n",
        layers.join(", "),
        STRATEGY_NAMES.join(" | ")
    )
}

/// A plugin's sidecar written (`write_meta`); answers its path.
pub fn write_meta(plugin: &Path, meta: &PluginMeta, explicit: bool, header: &str) -> Result<PathBuf, MetaError> {
    let path = meta_path_for(plugin);
    let body = format!("{header}{}", render_meta(meta, explicit));
    std::fs::write(&path, body).map_err(|e| MetaError(format!("could not write {}: {e}", file_name(&path))))?;
    Ok(path)
}

/// The editor's write (`write_settings`): every layer spelled out, under the header.
pub fn write_settings(plugin: &Path, meta: &PluginMeta) -> Result<PathBuf, MetaError> {
    write_meta(plugin, meta, true, &template_header())
}

/// A `Patch` file with every layer at its default, to edit by hand (`write_patch_template`).
pub fn write_patch_template(plugin: &Path) -> Result<PathBuf, MetaError> {
    let meta = PluginMeta { meta_type: META_PATCH.into(), layers: LAYER_NAMES.iter().map(|(n, _)| (n.to_string(), MergeSettings::default())).collect() };
    write_settings(plugin, &meta)
}

/// The marker beside a plugin the merge generated, so a later run ignores it instead of
/// merging a merge (`write_merged_marker`).
pub fn write_merged_marker(plugin: &Path) -> Result<PathBuf, MetaError> {
    let header = "# Written by Wraithguard Toolkit. Do not delete.\n\
                  #\n\
                  # It marks the plugin beside it as generated, so that merging again\n\
                  # ignores it instead of merging a merge. The format is Merged Lands'\n\
                  # .mergedlands.toml. Every layer is written out and set to\n\
                  # included = false as a belt-and-braces: meta_type = MergedLands\n\
                  # already keeps this plugin out of a re-merge, and if that were ever\n\
                  # missed the layers contribute nothing regardless.\n";
    let marker = PluginMeta {
        meta_type: META_MERGED_LANDS.into(),
        layers: LAYER_NAMES.iter().map(|(n, _)| (n.to_string(), MergeSettings { included: false, conflict_strategy: Strategy::Auto })).collect(),
    };
    write_meta(plugin, &marker, true, header).map_err(|e| {
        let p = file_name(plugin);
        MetaError(format!(
            "could not write {}, which marks {p} as generated. Without it a later merge would treat this plugin as a mod and merge its terrain back into itself: {e}",
            file_name(&meta_path_for(plugin))
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_example() {
        let m = parse_text(
            "version = \"0\"\nmeta_type = \"Patch\"\n[height_map]\nincluded = false\n[vertex_colors]\nincluded = false\n\
             [texture_indices]\nincluded = false\n[world_map_data]\nconflict_strategy = \"Overwrite\"\n",
            "x",
        )
        .unwrap();
        assert_eq!(m.meta_type, "Patch");
        assert_eq!(m.allowed_layers(), layer::WORLD_MAP);
        assert_eq!(m.strategy_for(layer::WORLD_MAP), Strategy::Overwrite);
        assert_eq!(m.strategy_for(layer::VERTEX_NORMALS), Strategy::Auto);
        assert!(!m.is_previous_merge());
    }

    #[test]
    fn refuses_what_it_cannot_trust() {
        let e = |t: &str| parse_text(t, "a.mergedlands.toml").unwrap_err().0;
        assert_eq!(e("version = \"1\""), "a.mergedlands.toml declares version '1', and only '0' is understood. A newer file may mean something different by the same keys.");
        assert_eq!(e("meta_type = \"Odd\""), "a.mergedlands.toml has an unknown meta_type 'Odd'");
        assert_eq!(
            e("[heights]\nincluded = true"),
            "a.mergedlands.toml configures 'heights', which is not a landscape layer. Expected one of: height_map, texture_indices, vertex_colors, world_map_data."
        );
        assert_eq!(e("height_map = 1"), "a.mergedlands.toml: [height_map] must be a table");
        assert_eq!(e("[height_map]\nincluded = \"no\""), "a.mergedlands.toml: [height_map] included must be true or false");
        assert_eq!(
            e("[height_map]\nconflict_strategy = \"auto\""),
            "a.mergedlands.toml: [height_map] conflict_strategy 'auto' is not recognised. Expected one of: Auto, Resolve, Overwrite, Ignore, Curvature."
        );
        assert!(e("version = ").starts_with("a.mergedlands.toml is not valid TOML: "));
        // An integer version is read as Python's str() of it.
        assert!(parse_text("version = 0", "a").is_ok());
    }

    #[test]
    fn renders_and_reads_back() {
        let m = PluginMeta {
            meta_type: "Patch".into(),
            layers: vec![("vertex_colors".into(), MergeSettings { included: false, conflict_strategy: Strategy::Auto }), ("height_map".into(), MergeSettings { included: true, conflict_strategy: Strategy::Curvature })],
        };
        let short = render_meta(&m, false);
        assert_eq!(
            short,
            "version = \"0\"\nmeta_type = \"Patch\"\n\n[height_map]\nincluded = true\nconflict_strategy = \"Curvature\"\n\n[vertex_colors]\nincluded = false\nconflict_strategy = \"Auto\"\n"
        );
        let back = parse_text(&short, "x").unwrap();
        assert_eq!(back.full(), m.full());
        assert_eq!(render_meta(&PluginMeta::default(), false), "version = \"0\"\nmeta_type = \"Auto\"\n");
        assert_eq!(
            template_header(),
            "# Merged Lands patch for the plugin beside this file.\n# It controls how this plugin's landscape edits are merged.\n#\n\
             # meta_type        = Auto | Patch | MergedLands  (use Patch for a hand-written patch)\n\
             # Layers: height_map, vertex_colors, texture_indices, world_map_data\n#   (height_map governs vertex normals too.)\n# Per layer:\n\
             #   included          = true | false   (false drops this plugin's edits to the layer)\n\
             #   conflict_strategy = Auto | Resolve | Overwrite | Ignore | Curvature\n\
             # Delete a section to accept its defaults (included = true, Auto).\n"
        );
        let full = format!("{}{}", template_header(), render_meta(&m, true));
        assert_eq!(parse_text(&full, "x").unwrap().full(), m.full());
    }

    #[test]
    fn writes_beside_the_plugin() {
        let dir = std::env::temp_dir().join(format!("landcore_meta_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Some.Mod v1.1.esp");
        assert_eq!(meta_path_for(&plugin), dir.join("Some.Mod v1.1.mergedlands.toml"));
        assert_eq!(load_meta(&plugin).unwrap(), PluginMeta::default());
        write_patch_template(&plugin).unwrap();
        assert_eq!(load_meta(&plugin).unwrap().meta_type, META_PATCH);
        write_merged_marker(&plugin).unwrap();
        let m = load_meta(&plugin).unwrap();
        assert!(m.is_previous_merge());
        assert_eq!(m.allowed_layers(), layer::NONE);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
