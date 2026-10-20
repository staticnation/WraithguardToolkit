//! The Object Window's columns, record type by record type, as the Construction Set has
//! them, and an armor piece's weight class: `wraithguard/patch/columns.py` and `armor.py`,
//! column for column (port step 4.1, slice 5a). The engine lists every record's id, name,
//! model and count; these are each type's own (a weapon's damage, an NPC's race, class and
//! faction, a leveled list's entries).

use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::pyval;

/// How a column reads a record.
#[derive(Clone, Copy)]
enum Get {
    /// A dotted path, the value as it is (the column can be edited in place).
    At(&'static str),
    /// One flag of a flags field (`A | B` text, or a list).
    Flag(&'static str, &'static str),
    /// Not that flag.
    NotFlag(&'static str, &'static str),
    /// A list field's ids, the first four, comma separated (`-NONE-` for none).
    Names(&'static str),
    /// An ingredient's nth effect.
    Effect(usize),
    /// A spell's or enchantment's first effect's field, in words.
    FirstEffect(&'static str),
    /// An enum field, in words.
    Enum(&'static str),
    /// A book that is a scroll.
    Scroll,
    /// The skill a book teaches.
    Teaches,
    /// `min-max` of two fields.
    Span(&'static str, &'static str),
    /// A creature's ways of moving.
    Movement,
    /// Whether a field is set at all.
    Truthy(&'static str),
    /// An armor piece's weight class.
    ArmorClass,
}

type Column = (&'static str, Get, &'static str, &'static str);

const NAME: Column = ("Name", Get::At("name"), "text", "The name the game shows");
const SCRIPT: Column = ("Script", Get::At("script"), "text", "The script it runs");
const WEIGHT: Column = ("Weight", Get::At("data.weight"), "num", "");
const VALUE: Column = ("Value", Get::At("data.value"), "num", "");
const ICON: Column = ("Inventory", Get::At("icon"), "text", "Its inventory icon");
const PERSISTS: Column = ("Persists", Get::Flag("flags", "PERSISTENT"), "bool", "References persist (always loaded)");
const BLOCKED: Column = ("Blocked", Get::Flag("flags", "BLOCKED"), "bool", "Blocked: the Construction Set will not change it");
const ENCHANTING: Column = ("Enchanting", Get::At("enchanting"), "text", "Its enchantment");
const ENCH_PTS: Column = ("Ench. pts", Get::At("data.enchantment"), "num", "Enchantment capacity");
const USE_ITEM: [Column; 7] = [NAME, SCRIPT, WEIGHT, VALUE, ("Uses", Get::At("data.uses"), "num", ""), ("Quality", Get::At("data.quality"), "num", ""), ICON];

/// Each tag's own columns, between the engine's id/count and its model and flags.
fn columns(tag: &str) -> Option<Vec<Column>> {
    Some(match tag {
        "ACTI" => vec![NAME, SCRIPT],
        "ALCH" => vec![NAME, SCRIPT, WEIGHT, VALUE, ("Autocalc", Get::Flag("data.flags", "AUTO_CALCULATE"), "bool", ""), ICON],
        "APPA" => vec![("Type", Get::Enum("data.apparatus_type"), "text", ""), NAME, SCRIPT, WEIGHT, VALUE, ("Quality", Get::At("data.quality"), "num", ""), ICON],
        "ARMO" => vec![
            ("Type", Get::Enum("data.armor_type"), "text", ""),
            NAME,
            SCRIPT,
            WEIGHT,
            VALUE,
            ("Health", Get::At("data.health"), "num", ""),
            ENCHANTING,
            ENCH_PTS,
            ("Rating", Get::At("data.armor_rating"), "num", "Armor rating"),
            ICON,
        ],
        "BODY" => vec![
            ("Type", Get::Enum("data.bodypart_type"), "text", ""),
            ("Race", Get::At("race"), "text", ""),
            ("Part", Get::Enum("data.part"), "text", ""),
            ("Female", Get::Flag("data.flags", "FEMALE"), "bool", ""),
            ("Playable", Get::NotFlag("data.flags", "NOT_PLAYABLE"), "bool", ""),
        ],
        "BOOK" => vec![
            NAME,
            SCRIPT,
            WEIGHT,
            VALUE,
            ("Scroll", Get::Scroll, "bool", ""),
            ("Teaches", Get::Teaches, "text", "The skill it raises when read"),
            ENCHANTING,
            ENCH_PTS,
            ICON,
        ],
        "CLOT" => vec![("Type", Get::Enum("data.clothing_type"), "text", ""), NAME, SCRIPT, WEIGHT, VALUE, ENCHANTING, ENCH_PTS, ICON],
        "CONT" => vec![
            NAME,
            SCRIPT,
            ("Weight", Get::At("encumbrance"), "num", "How much it holds"),
            ("Organic", Get::Flag("container_flags", "ORGANIC"), "bool", ""),
            ("Respawns", Get::Flag("container_flags", "RESPAWNS"), "bool", ""),
            ("Item list", Get::Names("inventory"), "text", ""),
        ],
        "DOOR" => vec![NAME, SCRIPT, ("Open sound", Get::At("open_sound"), "text", ""), ("Close sound", Get::At("close_sound"), "text", "")],
        "INGR" => vec![
            NAME,
            SCRIPT,
            WEIGHT,
            VALUE,
            ("Effect 1", Get::Effect(0), "text", ""),
            ("Effect 2", Get::Effect(1), "text", ""),
            ("Effect 3", Get::Effect(2), "text", ""),
            ("Effect 4", Get::Effect(3), "text", ""),
            ICON,
        ],
        "LIGH" => vec![
            NAME,
            SCRIPT,
            ("Sound", Get::At("sound"), "text", ""),
            WEIGHT,
            VALUE,
            ("Time", Get::At("data.time"), "num", "How long it burns (-1: forever)"),
            ("Radius", Get::At("data.radius"), "num", ""),
            ICON,
        ],
        "LOCK" | "PROB" | "REPA" => USE_ITEM.to_vec(),
        "MISC" => vec![NAME, SCRIPT, WEIGHT, VALUE, ("Key", Get::Flag("data.flags", "KEY"), "bool", ""), ICON],
        "STAT" => vec![],
        "WEAP" => vec![
            ("Type", Get::Enum("data.weapon_type"), "text", ""),
            NAME,
            SCRIPT,
            WEIGHT,
            ("Health", Get::At("data.health"), "num", ""),
            VALUE,
            ENCHANTING,
            ENCH_PTS,
            ("Speed", Get::At("data.speed"), "num", ""),
            ("Reach", Get::At("data.reach"), "num", ""),
            ("Chop", Get::Span("data.chop_min", "data.chop_max"), "text", "Chop damage, min-max"),
            ("Slash", Get::Span("data.slash_min", "data.slash_max"), "text", "Slash damage, min-max"),
            ("Thrust", Get::Span("data.thrust_min", "data.thrust_max"), "text", "Thrust damage, min-max"),
            ICON,
            ("Ignores resist", Get::Flag("data.flags", "IGNORES_NORMAL_WEAPON_RESISTANCE"), "bool", "Ignores normal weapon resistance"),
            ("Silver", Get::Flag("data.flags", "SILVER"), "bool", ""),
        ],
        "NPC_" => vec![
            NAME,
            SCRIPT,
            ("Level", Get::At("data.level"), "num", ""),
            ("Race", Get::At("race"), "text", ""),
            ("Female", Get::Flag("npc_flags", "FEMALE"), "bool", ""),
            ("Class", Get::At("class"), "text", ""),
            ("Faction", Get::At("faction"), "text", ""),
            ("Rank", Get::At("data.rank"), "num", ""),
            ("Autocalc", Get::Flag("npc_flags", "AUTO_CALCULATE"), "bool", ""),
            ("Essential", Get::Flag("npc_flags", "ESSENTIAL"), "bool", ""),
            ("Respawns", Get::Flag("npc_flags", "RESPAWN"), "bool", ""),
            ("Fight", Get::At("ai_data.fight"), "num", ""),
        ],
        "CREA" => vec![
            NAME,
            ("Type", Get::Enum("data.creature_type"), "text", ""),
            SCRIPT,
            ("Sound", Get::At("sound"), "text", "Its sound generator"),
            ("Level", Get::At("data.level"), "num", ""),
            ("Essential", Get::Flag("creature_flags", "ESSENTIAL"), "bool", ""),
            ("Respawns", Get::Flag("creature_flags", "RESPAWN"), "bool", ""),
            ("Movement", Get::Movement, "text", ""),
            ("Weapon & shield", Get::Flag("creature_flags", "WEAPON_AND_SHIELD"), "bool", ""),
            ("Biped", Get::Flag("creature_flags", "BIPED"), "bool", ""),
            ("Soul", Get::At("data.soul"), "num", ""),
        ],
        "LEVC" => vec![
            ("All levels", Get::Flag("leveled_creature_flags", "CALCULATE_FROM_ALL_LEVELS"), "bool", "Calculate from all levels <= the player's"),
            ("Chance none", Get::At("chance_none"), "num", ""),
            ("Creature list", Get::Names("creatures"), "text", ""),
        ],
        "LEVI" => vec![
            ("All levels", Get::Flag("leveled_item_flags", "CALCULATE_FROM_ALL_LEVELS"), "bool", "Calculate from all levels <= the player's"),
            ("Each item", Get::Flag("leveled_item_flags", "CALCULATE_FOR_EACH_ITEM"), "bool", "Calculate for each item"),
            ("Chance none", Get::At("chance_none"), "num", ""),
            ("Item list", Get::Names("items"), "text", ""),
        ],
        "SPEL" => vec![
            NAME,
            ("Type", Get::Enum("data.spell_type"), "text", ""),
            ("Cost", Get::At("data.cost"), "num", ""),
            ("Range", Get::FirstEffect("range"), "text", "Its first effect's range"),
            ("Autocalc", Get::Flag("data.flags", "AUTO_CALCULATE"), "bool", ""),
            ("PC start", Get::Flag("data.flags", "PC_START_SPELL"), "bool", ""),
        ],
        "ENCH" => vec![
            ("Charge", Get::At("data.max_charge"), "num", ""),
            ("Type", Get::Enum("data.enchant_type"), "text", ""),
            ("Cost", Get::At("data.cost"), "num", ""),
            ("Autocalc", Get::Truthy("data.flags"), "bool", ""),
        ],
        _ => return None,
    })
}

/// The record type each tag reads as.
fn type_for(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "ACTI" => "Activator",
        "ALCH" => "Alchemy",
        "APPA" => "Apparatus",
        "ARMO" => "Armor",
        "BODY" => "Bodypart",
        "BOOK" => "Book",
        "CLOT" => "Clothing",
        "CONT" => "Container",
        "DOOR" => "Door",
        "INGR" => "Ingredient",
        "LIGH" => "Light",
        "LOCK" => "Lockpick",
        "PROB" => "Probe",
        "REPA" => "RepairItem",
        "MISC" => "MiscItem",
        "STAT" => "Static",
        "WEAP" => "Weapon",
        "NPC_" => "Npc",
        "CREA" => "Creature",
        "LEVC" => "LeveledCreature",
        "LEVI" => "LeveledItem",
        "SPEL" => "Spell",
        "ENCH" => "Enchanting",
        _ => return None,
    })
}

/// Python's truth of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// A dotted path's value; null where the path runs out (`.get` all the way down).
fn at<'a>(rec: &'a Value, path: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    let mut cur = rec;
    for part in path.split('.') {
        match cur {
            Value::Object(m) => cur = m.get(part).unwrap_or(&NULL),
            _ => return &NULL,
        }
    }
    cur
}

/// `str(value or "")`.
fn text_or_empty(v: &Value) -> String {
    if truthy(v) { pyval::str_of(v) } else { String::new() }
}

/// `ShortBladeOneHand` -> `Short Blade One Hand`; `ON_TOUCH` -> `On Touch`.
pub fn words(text: &str) -> String {
    let cased: Vec<char> = text.chars().filter(|c| c.is_uppercase() || c.is_lowercase()).collect();
    let is_upper = !cased.is_empty() && cased.iter().all(|c| !c.is_lowercase());
    if text.contains('_') || is_upper {
        return text
            .split('_')
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && ch.is_uppercase() && !chars[i - 1].is_uppercase() {
            out.push(' ');
        }
        out.push(*ch);
    }
    out
}

fn flag(rec: &Value, path: &str, name: &str) -> bool {
    match at(rec, path) {
        Value::Array(a) => a.iter().any(|x| pyval::str_of(x).to_uppercase() == name),
        raw => text_or_empty(raw).split('|').any(|p| p.trim().to_uppercase() == name),
    }
}

/// A column's value for one record, as the Python's reader gives it (`None` where it
/// would raise: the cell shows empty).
fn read(get: Get, rec: &Value, settings: &Settings) -> Option<Value> {
    Some(match get {
        Get::At(p) => at(rec, p).clone(),
        Get::Flag(p, n) => json!(flag(rec, p, n)),
        Get::NotFlag(p, n) => json!(!flag(rec, p, n)),
        Get::Names(p) => {
            let Some(list) = at(rec, p).as_array().filter(|a| !a.is_empty()) else { return Some(json!("-NONE-")) };
            let mut ids: Vec<String> = Vec::new();
            for entry in list {
                let val = match entry {
                    Value::Object(m) => m.iter().find(|(k, v)| v.is_string() && ["id", "item", "creature", "name"].contains(&k.as_str())).map(|(_, v)| v.as_str().unwrap().to_string()),
                    Value::Array(a) => a.iter().find_map(|v| v.as_str().map(str::to_string)),
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                };
                if let Some(v) = val.filter(|v| !v.is_empty()) {
                    ids.push(v);
                }
            }
            if ids.is_empty() {
                json!("-NONE-")
            } else {
                let more = if ids.len() > 4 { format!(" +{}", ids.len() - 4) } else { String::new() };
                json!(ids[..ids.len().min(4)].join(", ") + &more)
            }
        }
        Get::Effect(i) => {
            let Some(effects) = at(rec, "data.effects").as_array().filter(|e| i < e.len()) else { return Some(json!("")) };
            let v = &effects[i];
            let empty = match v {
                Value::Null => true,
                Value::String(s) => s == "None" || s == "-1",
                Value::Number(n) => n.as_i64().is_some_and(|x| x < 0) || n.as_f64() == Some(-1.0),
                _ => false,
            };
            if empty { json!("") } else { json!(words(&pyval::str_of(v))) }
        }
        Get::FirstEffect(field) => match rec.get("effects").and_then(Value::as_array).and_then(|e| e.first()) {
            Some(Value::Object(e)) => json!(words(&text_or_empty(e.get(field).unwrap_or(&Value::Null)))),
            _ => json!(""),
        },
        Get::Enum(p) => json!(words(&text_or_empty(at(rec, p)))),
        Get::Scroll => json!(text_or_empty(at(rec, "data.book_type")).to_lowercase() == "scroll"),
        Get::Teaches => {
            let s = at(rec, "data.skill");
            let none = s.is_null() || s.as_f64() == Some(-1.0) || s.as_str() == Some("None");
            if none { json!("") } else { json!(words(&pyval::str_of(s))) }
        }
        Get::Span(a, b) => json!(format!("{}-{}", pyval::str_of(at(rec, a)), pyval::str_of(at(rec, b)))),
        Get::Movement => json!(
            [("WALKS", "Walk"), ("SWIMS", "Swim"), ("FLIES", "Fly")].iter().filter(|(f, _)| flag(rec, "creature_flags", f)).map(|(_, w)| *w).collect::<Vec<_>>().join(", ")
        ),
        Get::Truthy(p) => json!(truthy(at(rec, p))),
        Get::ArmorClass => {
            let data = match rec.get("data") {
                Some(d) if truthy(d) => d,
                _ => &json!({}),
            };
            let Value::Object(d) = data else { return None };
            let weight = match d.get("weight") {
                Some(w) if truthy(w) => match w {
                    Value::Number(n) => n.as_f64()?,
                    Value::Bool(b) => *b as i32 as f64,
                    Value::String(s) => pyval::float_of(s)?,
                    _ => return None,
                },
                _ => 0.0,
            };
            json!(weight_class(&text_or_empty(d.get("armor_type").unwrap_or(&Value::Null)), weight, settings))
        }
    })
}

/// A value as the table shows it: yes/no, numbers kept, floats to two places.
fn show(value: Value, kind: &str) -> Value {
    if kind == "bool" {
        return json!(if truthy(&value) { "yes" } else { "no" });
    }
    match &value {
        Value::Number(n) if n.is_f64() => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            pyval::float_value(format!("{x:.2}").parse().unwrap_or(x))
        }
        Value::Null => json!(""),
        _ => value,
    }
}

/// The type's own columns for every record, read off the winning versions (`versions`:
/// per plugin in load order, its records of the tag by lower-case id; the last wins):
/// `{columns: [{label, kind, title, path}], rows: {id: [value, ...]}}`, persists and
/// blocked last; empty for a type with no table. `path` is the field a column shows as
/// it is (editable in place), null for one worked out.
pub fn table(tag: &str, versions: &[IndexMap<String, Value>], settings: &Settings) -> Value {
    let mut t = tag.to_uppercase();
    while t.len() < 4 {
        t.push('_');
    }
    let Some(mut cols) = columns(&t) else { return json!({"columns": [], "rows": {}}) };
    if t == "ARMO" {
        let class = ("Class", Get::ArmorClass, "text", "Weight class: the slot's base weight (iHelmWeight, iCuirassWeight...) times fLightMaxMod or fMedMaxMod, from the load order's game settings");
        cols.insert(1, class);
    }
    cols.push(PERSISTS);
    cols.push(BLOCKED);
    let want = type_for(&t);
    let mut winners: IndexMap<&String, &Value> = IndexMap::new();
    for records in versions {
        for (key, rec) in records {
            if want.is_none_or(|w| rec.get("type").and_then(Value::as_str) == Some(w)) {
                winners.insert(key, rec);
            }
        }
    }
    let mut rows = serde_json::Map::new();
    for (key, rec) in winners {
        let row: Vec<Value> = cols.iter().map(|(_, get, kind, _)| show(read(*get, rec, settings).unwrap_or(json!("")), kind)).collect();
        rows.insert(key.clone(), Value::Array(row));
    }
    let columns: Vec<Value> = cols
        .iter()
        .map(|(label, get, kind, title)| {
            let path = match get {
                Get::At(p) => json!(p),
                _ => Value::Null,
            };
            json!({"label": label, "kind": kind, "title": title, "path": path})
        })
        .collect();
    json!({"columns": columns, "rows": rows})
}

// -- armor's weight class -------------------------------------------------------------

/// The game settings the weight class is worked out from, by name.
pub type Settings = IndexMap<String, f64>;

/// The game setting holding each slot's base weight.
pub const SLOT_SETTING: [(&str, &str); 11] = [
    ("Helmet", "iHelmWeight"),
    ("Cuirass", "iCuirassWeight"),
    ("LeftPauldron", "iPauldronWeight"),
    ("RightPauldron", "iPauldronWeight"),
    ("Greaves", "iGreavesWeight"),
    ("Boots", "iBootsWeight"),
    ("LeftGauntlet", "iGauntletWeight"),
    ("RightGauntlet", "iGauntletWeight"),
    ("LeftBracer", "iGauntletWeight"),
    ("RightBracer", "iGauntletWeight"),
    ("Shield", "iShieldWeight"),
];

/// Morrowind.esm's values, used where the load order has no GMST of that name.
pub const DEFAULTS: [(&str, f64); 9] = [
    ("iHelmWeight", 5.0),
    ("iPauldronWeight", 10.0),
    ("iCuirassWeight", 30.0),
    ("iGauntletWeight", 5.0),
    ("iGreavesWeight", 15.0),
    ("iBootsWeight", 20.0),
    ("iShieldWeight", 15.0),
    ("fLightMaxMod", 0.6),
    ("fMedMaxMod", 0.9),
];

/// Weights are 32-bit floats: a piece exactly at a limit must not fall over it.
const SLACK: f64 = 5e-4;

/// A GMST record's number (`value` is `{type, data}`), or `None`.
pub fn setting_value(record: &Value) -> Option<f64> {
    let mut v = record.get("value")?;
    if let Value::Object(m) = v {
        v = m.get("data")?;
    }
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// The settings the weight class needs, from GMST records (load order; last wins).
pub fn settings_from<'a>(versions: impl IntoIterator<Item = &'a IndexMap<String, Value>>) -> Settings {
    let mut out: Settings = DEFAULTS.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    for records in versions {
        for (key, rec) in records {
            if let Some((name, _)) = DEFAULTS.iter().find(|(k, _)| k.to_lowercase() == key.to_lowercase())
                && let Some(v) = setting_value(rec)
            {
                out.insert(name.to_string(), v);
            }
        }
    }
    out
}

fn setting(s: &Settings, name: &str) -> f64 {
    s.get(name).copied().or_else(|| DEFAULTS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)).unwrap_or(0.0)
}

/// The heaviest a piece of this slot can be and still be Light, and Medium.
pub fn limits(armor_type: &str, s: &Settings) -> Option<(f64, f64)> {
    let name = SLOT_SETTING.iter().find(|(t, _)| *t == armor_type)?.1;
    let base = setting(s, name);
    Some((base * setting(s, "fLightMaxMod"), base * setting(s, "fMedMaxMod")))
}

/// `Light`, `Medium` or `Heavy`; empty for a slot it cannot tell.
pub fn weight_class(armor_type: &str, weight: f64, s: &Settings) -> String {
    match limits(armor_type, s) {
        None => String::new(),
        Some((light, _)) if weight <= light + SLACK => "Light".into(),
        Some((_, medium)) if weight <= medium + SLACK => "Medium".into(),
        Some(_) => "Heavy".into(),
    }
}

/// What the page needs to work the class out itself as a weight is typed:
/// `{slots: {armor type: base weight}, light, medium}`.
pub fn browser_table(s: &Settings) -> Value {
    let slots: serde_json::Map<String, Value> = SLOT_SETTING.iter().map(|(t, n)| (t.to_string(), pyval::float_value(setting(s, n)))).collect();
    json!({"slots": slots, "light": pyval::float_value(setting(s, "fLightMaxMod")), "medium": pyval::float_value(setting(s, "fMedMaxMod"))})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_classes_read_as_the_python_ones() {
        assert_eq!(words("ShortBladeOneHand"), "Short Blade One Hand");
        assert_eq!(words("ON_TOUCH"), "On Touch");
        assert_eq!(words("AI"), "Ai");
        let s = settings_from(std::iter::empty());
        assert_eq!(weight_class("Helmet", 3.0, &s), "Light");
        assert_eq!(weight_class("Helmet", 4.5, &s), "Medium");
        assert_eq!(weight_class("Helmet", 9.0, &s), "Heavy");
        assert_eq!(weight_class("Hat", 1.0, &s), "");
    }
}
