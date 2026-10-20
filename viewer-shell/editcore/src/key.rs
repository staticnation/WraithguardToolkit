//! How the Editor keys a record within its type, from its tes3conv-schema form: the rules
//! of `wraithguard/patch/records.py` (`record_key`, `is_interior`) and the tag table of
//! `wraithguard/tes3fields/naming.py`, line for line. (The conflict scan's key, which
//! names a cell by its name, is another thing: [`crate::plugin::conflict_key`].)

use serde_json::Value;

/// tes3conv's `"type"` and the record's four-letter tag (`NPC_` padded, as files have it).
pub const TYPES: [(&str, &str); 43] = [
    ("Activator", "ACTI"),
    ("Alchemy", "ALCH"),
    ("Apparatus", "APPA"),
    ("Armor", "ARMO"),
    ("Birthsign", "BSGN"),
    ("Bodypart", "BODY"),
    ("Book", "BOOK"),
    ("Cell", "CELL"),
    ("Class", "CLAS"),
    ("Clothing", "CLOT"),
    ("Container", "CONT"),
    ("Creature", "CREA"),
    ("Dialogue", "DIAL"),
    ("DialogueInfo", "INFO"),
    ("Door", "DOOR"),
    ("Enchanting", "ENCH"),
    ("Faction", "FACT"),
    ("GameSetting", "GMST"),
    ("GlobalVariable", "GLOB"),
    ("Header", "TES3"),
    ("Ingredient", "INGR"),
    ("Landscape", "LAND"),
    ("LandscapeTexture", "LTEX"),
    ("LeveledCreature", "LEVC"),
    ("LeveledItem", "LEVI"),
    ("Light", "LIGH"),
    ("Lockpick", "LOCK"),
    ("MagicEffect", "MGEF"),
    ("MiscItem", "MISC"),
    ("Npc", "NPC_"),
    ("PathGrid", "PGRD"),
    ("Probe", "PROB"),
    ("Race", "RACE"),
    ("Region", "REGN"),
    ("RepairItem", "REPA"),
    ("Script", "SCPT"),
    ("Skill", "SKIL"),
    ("Sound", "SOUN"),
    ("SoundGen", "SNDG"),
    ("Spell", "SPEL"),
    ("StartScript", "SSCR"),
    ("Static", "STAT"),
    ("Weapon", "WEAP"),
];

/// The record type of a tag (any case, `NPC` padded to `NPC_`), or `None`.
pub fn type_of(tag: &str) -> Option<&'static str> {
    let mut t = tag.trim().to_ascii_uppercase();
    while t.len() < 4 {
        t.push('_');
    }
    TYPES.iter().find(|(_, g)| *g == t).map(|(n, _)| *n)
}

/// The tag of a record type, or `None`.
pub fn tag_of(record_type: &str) -> Option<&'static str> {
    TYPES.iter().find(|(n, _)| *n == record_type).map(|(_, g)| *g)
}

/// A record's `type`, or `""`.
pub fn type_name(record: &Value) -> &str {
    record.get("type").and_then(Value::as_str).unwrap_or("")
}

/// A non-empty string field.
fn text(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// A whole number as Python's `int()` reads a JSON number (a float is cut toward zero).
fn int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64))
}

/// A grid: two numbers.
fn grid(v: Option<&Value>) -> Option<(i64, i64)> {
    match v?.as_array()?.as_slice() {
        [x, y] => Some((int(x)?, int(y)?)),
        _ => None,
    }
}

/// Whether a CELL record is an interior (its `IS_INTERIOR` flag, as a number or as text).
pub fn is_interior(record: &Value) -> bool {
    match record.get("data").filter(|d| d.is_object()).and_then(|d| d.get("flags")) {
        Some(Value::Number(n)) => n.as_i64().is_some_and(|f| f & 1 != 0),
        Some(Value::String(s)) => s.contains("IS_INTERIOR"),
        Some(Value::Null) | None => false,
        Some(other) => other.to_string().contains("IS_INTERIOR"),
    }
}

/// Whether a plugin's version of a record deletes it (`DELETED` among its flags).
pub fn deletes(record: &Value) -> bool {
    record.get("flags").and_then(Value::as_str).is_some_and(|f| f.split('|').any(|p| p.trim() == "DELETED"))
}

/// What identifies a record within its type: an `id` for most; an interior cell by its
/// name and an exterior by `(x, y)` (an interior's grid is a placeholder 0, 0); a start
/// script by the script it starts (OpenMW keys it by NAME); a path grid by its cell's
/// name for an interior, else its grid; a landscape by its grid. `""` when nothing names it.
pub fn record_key(record: &Value) -> String {
    let rtype = type_name(record);
    if rtype == "StartScript"
        && let Some(s) = text(record.get("script"))
    {
        return s.to_string();
    }
    if let Some(id) = text(record.get("id")) {
        return id.to_string();
    }
    if rtype == "Cell" && is_interior(record) {
        return record.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    }
    let data = record.get("data").filter(|d| d.is_object());
    if rtype == "PathGrid" {
        let g = data.and_then(|d| d.get("grid"));
        let at_origin = !g.and_then(Value::as_array).is_some_and(|a| a.len() == 2 && a.iter().any(|v| int(v).unwrap_or(0) != 0));
        if let Some(name) = text(record.get("cell"))
            && at_origin
        {
            return name.to_string();
        }
    }
    let g = match record.get("grid") {
        None | Some(Value::Null) => data.and_then(|d| d.get("grid")),
        some => some,
    };
    match grid(g) {
        Some((x, y)) => format!("({x}, {y})"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_are_keyed_as_the_editor_keys_them() {
        assert_eq!(record_key(&json!({"type": "Weapon", "id": "Iron Dagger"})), "Iron Dagger");
        let room = json!({"type": "Cell", "name": "Vivec, Arena", "data": {"flags": "IS_INTERIOR", "grid": [0, 0]}});
        assert_eq!(record_key(&room), "Vivec, Arena");
        let ext = json!({"type": "Cell", "name": "Balmora", "data": {"flags": "HAS_WATER", "grid": [-3, -2]}});
        assert_eq!(record_key(&ext), "(-3, -2)");
        assert_eq!(record_key(&json!({"type": "Landscape", "grid": [1, 2]})), "(1, 2)");
        assert_eq!(record_key(&json!({"type": "PathGrid", "cell": "Vivec, Arena", "data": {"grid": [0, 0]}})), "Vivec, Arena");
        assert_eq!(record_key(&json!({"type": "PathGrid", "cell": "Balmora", "data": {"grid": [-3, -2]}})), "(-3, -2)");
        assert_eq!(record_key(&json!({"type": "StartScript", "id": "x", "script": "Main"})), "Main");
        assert_eq!(record_key(&json!({"type": "Header"})), "");
        assert_eq!(type_of("npc"), Some("Npc"));
        assert_eq!(tag_of("Npc"), Some("NPC_"));
        assert!(deletes(&json!({"flags": "PERSISTENT | DELETED"})));
        assert!(!deletes(&json!({"flags": "UNDELETED"})));
    }
}
