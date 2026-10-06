//! What OpenMW's Lua API allows where, per release (the tables of `wraithguard/lua/api.py`,
//! which the Python side still passes in itself; these are for callers with no Python -
//! the viewer). Taken from each release's engine handlers and built-in events pages:
//! [`api_051`] is 0.51.0 (Lua API revision 129), [`api_050`] 0.50.0 (97), [`api_049`]
//! 0.49.0 (76). An install's own documentation carries its packages and interfaces
//! ([`from_docs`]), but not its handlers or built-in events, which come from the release.

use std::collections::{HashMap, HashSet};

use crate::analysis::Rules;
use crate::ldt;
use crate::scan::ScanRules;

/// One release's rules.
#[derive(Clone)]
pub struct ApiVersion {
    /// The OpenMW release, e.g. `0.51.0`.
    pub openmw: String,
    /// Its `core.API_REVISION` (0 when not known).
    pub revision: i64,
    /// Handlers, per-frame handlers, packages, built-in interfaces and events.
    pub rules: ScanRules,
}

const ANY: &[&str] = &["global", "load", "local", "menu", "player"];
const NON_MENU: &[&str] = &["global", "local", "player"];
const LOCAL: &[&str] = &["local", "player"];
const MENU_PLAYER: &[&str] = &["menu", "player"];
const GLOBAL: &[&str] = &["global"];

const HANDLERS: &[(&str, &[&str])] = &[
    ("onInterfaceOverride", ANY),
    ("onInit", NON_MENU),
    ("onUpdate", NON_MENU),
    ("onSave", NON_MENU),
    ("onLoad", NON_MENU),
    ("onNewGame", GLOBAL),
    ("onPlayerAdded", GLOBAL),
    ("onObjectActive", GLOBAL),
    ("onActorActive", GLOBAL),
    ("onItemActive", GLOBAL),
    ("onActivate", GLOBAL),
    ("onNewExterior", GLOBAL),
    ("onActive", LOCAL),
    ("onInactive", LOCAL),
    ("onTeleported", LOCAL),
    ("onActivated", LOCAL),
    ("onConsume", LOCAL),
    ("onFrame", MENU_PLAYER),
    ("onKeyPress", MENU_PLAYER),
    ("onKeyRelease", MENU_PLAYER),
    ("onControllerButtonPress", MENU_PLAYER),
    ("onControllerButtonRelease", MENU_PLAYER),
    ("onInputAction", MENU_PLAYER),
    ("onTouchPress", MENU_PLAYER),
    ("onTouchRelease", MENU_PLAYER),
    ("onTouchMove", MENU_PLAYER),
    ("onMouseButtonPress", MENU_PLAYER),
    ("onMouseButtonRelease", MENU_PLAYER),
    ("onMouseWheel", MENU_PLAYER),
    ("onConsoleCommand", MENU_PLAYER),
    ("onQuestUpdate", &["player"]),
    ("onStateChanged", &["menu"]),
    ("onContentFilesLoaded", &["load"]),
];

const PACKAGES: &[(&str, &[&str])] = &[
    ("openmw.world", GLOBAL),
    ("openmw.self", LOCAL),
    ("openmw.nearby", LOCAL),
    ("openmw.ui", MENU_PLAYER),
    ("openmw.input", MENU_PLAYER),
    ("openmw.camera", &["player"]),
    ("openmw.postprocessing", &["player"]),
    ("openmw.ambient", MENU_PLAYER),
    ("openmw.menu", &["menu"]),
    ("openmw.content", &["load"]),
];

const EVENTS_051: &[&str] = &[
    "DialogueResponse", "Died", "StartAIPackage", "RemoveAIPackages", "UseItem", "ModifyStat", "AddVfx",
    "PlaySound3d", "BreakInvisibility", "Unequip", "Hit", "ModifyItemCondition", "ShowMessage", "UiModeChanged",
    "AddUiMode", "SetUiMode", "Pause", "Unpause", "SetGameTimeScale", "SetSimulationTimeScale", "SpawnVfx",
    "ConsumeItem", "Lock", "Unlock",
];

const EVENTS_049: &[&str] = &[
    "Died", "StartAIPackage", "RemoveAIPackages", "UseItem", "UiModeChanged", "AddUiMode", "SetUiMode", "Pause",
    "Unpause", "SetGameTimeScale", "SetSimulationTimeScale",
];

const INTERFACES: &[&str] = &[
    "Activation", "AI", "AnimationController", "Camera", "Combat", "Controls", "Crimes", "GamepadControls",
    "ItemUsage", "MWUI", "Settings", "SkillProgression", "UI",
];

fn table(rows: &[(&str, &[&str])], drop_load: bool) -> HashMap<String, Vec<String>> {
    rows.iter()
        .filter_map(|(k, v)| {
            let mut ctx: Vec<String> = v.iter().filter(|c| !(drop_load && **c == "load")).map(|c| c.to_string()).collect();
            ctx.sort();
            (!ctx.is_empty()).then(|| (k.to_string(), ctx))
        })
        .collect()
}

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn version(openmw: &str, revision: i64, handlers: HashMap<String, Vec<String>>, packages: HashMap<String, Vec<String>>, events: &[&str], interfaces: HashSet<String>) -> ApiVersion {
    ApiVersion {
        openmw: openmw.into(),
        revision,
        rules: ScanRules {
            rules: Rules { handlers, per_frame: set(&["onUpdate", "onFrame"]), packages },
            builtin_interfaces: interfaces,
            builtin_events: set(events),
        },
    }
}

/// OpenMW 0.51.0 (API 129).
pub fn api_051() -> ApiVersion {
    version("0.51.0", 129, table(HANDLERS, false), table(PACKAGES, false), EVENTS_051, set(INTERFACES))
}

/// OpenMW 0.50.0 (API 97): 0.51's handlers and events without load scripts
/// (`onContentFilesLoaded`, `openmw.content`).
pub fn api_050() -> ApiVersion {
    version("0.50.0", 97, table(HANDLERS, true), table(PACKAGES, true), EVENTS_051, set(INTERFACES))
}

/// OpenMW 0.49.0 (API 76): 0.50's handlers; fewer built-in events, and no `Combat`.
pub fn api_049() -> ApiVersion {
    let mut interfaces = set(INTERFACES);
    interfaces.remove("Combat");
    version("0.49.0", 76, table(HANDLERS, true), table(PACKAGES, true), EVENTS_049, interfaces)
}

/// `"0.50.0"` -> `[0, 50, 0]`; anything unreadable sorts as newest.
fn version_key(text: &str) -> Vec<u32> {
    let parts: Result<Vec<u32>, _> = text.trim().split('.').take(3).map(|p| p.parse::<u32>()).collect();
    parts.unwrap_or_else(|_| vec![99])
}

/// The table for an OpenMW release: the newest one not newer than it (the newest for an
/// unknown or newer one, the oldest for an older one).
pub fn for_version(version: Option<&str>) -> ApiVersion {
    let Some(v) = version.filter(|v| !v.is_empty()) else { return api_051() };
    let want = version_key(v);
    let mut best = api_049();
    for rel in [api_049(), api_050(), api_051()] {
        if version_key(&rel.openmw) <= want {
            best = rel;
        }
    }
    best
}

/// The documentation's context names -> the rules' (its `local` is any local script, the
/// player's among them).
fn doc_contexts(c: &str) -> &'static [&'static str] {
    match c {
        "global" => &["global"],
        "local" => &["local", "player"],
        "player" => &["player"],
        "menu" => &["menu"],
        "load" => &["load"],
        _ => &[],
    }
}

/// The release's rules brought to an install's documentation (the Python
/// `openmw_api.api_from_docs` with its default base): its packages and where each may be
/// required, its built-in interfaces added.
pub fn from_docs(docs: &ldt::Api) -> ApiVersion {
    let base = for_version(docs.version.as_deref());
    let mut packages: HashMap<String, Vec<String>> = HashMap::new();
    for m in &docs.modules {
        if m.interface.as_deref().is_some_and(|i| !i.is_empty()) || !m.require.starts_with("openmw.") {
            continue;
        }
        let mut ctx: Vec<String> = Vec::new();
        for c in &m.contexts {
            for x in doc_contexts(c) {
                if !ctx.iter().any(|y| y == x) {
                    ctx.push(x.to_string());
                }
            }
        }
        ctx.sort();
        if !ctx.is_empty() && ctx.len() != ANY.len() {
            packages.insert(m.require.clone(), ctx);
        }
    }
    let version = docs.version.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| base.openmw.clone());
    let mut out = base.clone();
    out.revision = if version == base.openmw { base.revision } else { 0 };
    out.openmw = version;
    if !packages.is_empty() {
        out.rules.rules.packages = packages;
    }
    for m in &docs.modules {
        if let Some(i) = m.interface.as_deref().filter(|i| !i.is_empty()) {
            out.rules.builtin_interfaces.insert(i.to_string());
        }
    }
    out
}

/// The rules for a setup: its install's documentation when `resources` is one
/// ([`crate::findings::find_resources`] finds it), the newest release's otherwise.
pub fn for_install(resources: Option<&std::path::Path>) -> ApiVersion {
    match resources.and_then(ldt::read_resources) {
        Some(docs) => from_docs(&docs),
        None => api_051(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_as_python_has_them() {
        let a = api_051();
        assert_eq!(a.rules.rules.handlers["onUpdate"], ["global", "local", "player"]);
        assert_eq!(a.rules.rules.handlers["onInterfaceOverride"].len(), 5);
        let b = api_050();
        assert!(!b.rules.rules.handlers.contains_key("onContentFilesLoaded"));
        assert_eq!(b.rules.rules.handlers["onInterfaceOverride"], ["global", "local", "menu", "player"]);
        assert!(!b.rules.rules.packages.contains_key("openmw.content"));
        let c = api_049();
        assert!(!c.rules.builtin_interfaces.contains("Combat"));
        assert!(!c.rules.builtin_events.contains("ModifyStat"));
        assert_eq!(for_version(Some("0.50.2")).openmw, "0.50.0");
        assert_eq!(for_version(Some("0.48.0")).openmw, "0.49.0");
        assert_eq!(for_version(Some("0.52.0")).openmw, "0.51.0");
        assert_eq!(for_version(None).revision, 129);
    }
}
