//! The Editor's verifier: checks over what the patch carries, as the patch leaves it
//! (`wraithguard/patch/verify.py`, port step 4.1, slice 5e). Cases:
//! `tests/fixtures/verify_cases.json`, run here and by `tests/test_verify_cases.py`.
//!
//! OpenMW-CS's verifier is the reference for what is worth checking; the checks are the
//! toolkit's own, over its records. Each record the patch carries is taken as the patch
//! would write it and checked: ids named that nothing defines, leveled lists, scripts
//! that do not compile, responses that lost their place, journals, path grids (and an
//! exterior's points under the land), the per-type checks, an item's enchantment against
//! what it can carry, and the globals every game needs. Each finding is `{level, check,
//! tag, id, path, message}`, `level` `error` (the game trips on it) or `warning`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde_json::{json, Value};

use crate::edit::{value_at, Editor};
use crate::key::{record_key, tag_of};
use crate::pyval::{self, g, str_of};
use crate::session::{truthy, Found};

/// Records an inventory or a leveled item list may name.
pub const ITEM_TAGS: [&str; 13] = ["ALCH", "APPA", "ARMO", "BOOK", "CLOT", "INGR", "LIGH", "LOCK", "MISC", "PROB", "REPA", "WEAP", "LEVI"];
/// Records a leveled creature list may name.
pub const ACTOR_TAGS: [&str; 3] = ["CREA", "NPC_", "LEVC"];

/// Records that can be placed in a cell.
pub fn object_tags() -> Vec<&'static str> {
    let mut v: Vec<&str> = ITEM_TAGS.to_vec();
    v.extend(["ACTI", "CONT", "DOOR", "STAT", "BODY", "CREA", "NPC_", "LEVC"]);
    v
}

/// Globals every game needs (OpenMW-CS's mandatory ids).
pub const REQUIRED_GLOBALS: [&str; 5] = ["Day", "DaysPassed", "GameHour", "Month", "PCRace"];

/// What a dialogue filter's `id` names, by its type.
fn filter_tags(kind: &str) -> Option<Vec<&'static str>> {
    Some(match kind {
        "Global" => vec!["GLOB"],
        "Journal" => vec!["DIAL"],
        "Item" => ITEM_TAGS.to_vec(),
        "Dead" | "NotId" => vec!["NPC_", "CREA"],
        "NotFaction" => vec!["FACT"],
        "NotClass" => vec!["CLAS"],
        "NotRace" => vec!["RACE"],
        _ => return None,
    })
}

/// Per record type: `(where, tags)` - `where` a dotted path (`*` every entry of a list,
/// a number one place in an entry), `tags` what it may name.
fn reference_fields(record_type: &str) -> Vec<(&'static str, Vec<&'static str>)> {
    let actors_objects = || {
        let mut v = ACTOR_TAGS.to_vec();
        v.extend(object_tags());
        v
    };
    let items = || ITEM_TAGS.to_vec();
    let biped = || vec![("biped_objects.*.male_bodypart", vec!["BODY"]), ("biped_objects.*.female_bodypart", vec!["BODY"])];
    match record_type {
        "Npc" => vec![
            ("ai_packages.*.target", actors_objects()),
            ("race", vec!["RACE"]),
            ("class", vec!["CLAS"]),
            ("faction", vec!["FACT"]),
            ("head", vec!["BODY"]),
            ("hair", vec!["BODY"]),
            ("spells.*", vec!["SPEL"]),
            ("inventory.*.1", items()),
        ],
        "Creature" => vec![("ai_packages.*.target", actors_objects()), ("spells.*", vec!["SPEL"]), ("inventory.*.1", items())],
        "Container" => vec![("inventory.*.1", items())],
        "LeveledItem" => vec![("items.*.0", items())],
        "LeveledCreature" => vec![("creatures.*.0", ACTOR_TAGS.to_vec())],
        "Door" => vec![("open_sound", vec!["SOUN"]), ("close_sound", vec!["SOUN"])],
        "Light" => vec![("sound", vec!["SOUN"])],
        "Race" | "Birthsign" => vec![("spells.*", vec!["SPEL"])],
        "Region" => vec![("sleep_creature", vec!["LEVC"]), ("sounds.*.0", vec!["SOUN"])],
        "Faction" => vec![("reactions.*.faction", vec!["FACT"])],
        "MagicEffect" => vec![
            ("cast_sound", vec!["SOUN"]),
            ("bolt_sound", vec!["SOUN"]),
            ("hit_sound", vec!["SOUN"]),
            ("area_sound", vec!["SOUN"]),
            ("cast_visual", vec!["STAT"]),
            ("bolt_visual", vec!["STAT"]),
            ("hit_visual", vec!["STAT"]),
            ("area_visual", vec!["STAT"]),
        ],
        "Bodypart" => vec![("race", vec!["RACE"])],
        "Armor" | "Clothing" => biped(),
        "SoundGen" => vec![("sound", vec!["SOUN"]), ("creature", vec!["CREA"])],
        "DialogueInfo" => vec![
            ("speaker_id", vec!["NPC_", "CREA"]),
            ("speaker_race", vec!["RACE"]),
            ("speaker_class", vec!["CLAS"]),
            ("speaker_faction", vec!["FACT"]),
            ("player_faction", vec!["FACT"]),
        ],
        "Cell" => vec![("region", vec!["REGN"])],
        _ => Vec::new(),
    }
}

/// Every `(path, value)` a reference pattern reaches in a value.
fn reach<'a>(value: &'a Value, parts: &[&str], path: &str, out: &mut Vec<(String, &'a Value)>) {
    let Some((head, rest)) = parts.split_first() else {
        out.push((path.to_string(), value));
        return;
    };
    if *head == "*" {
        if let Value::Array(a) = value {
            for (i, item) in a.iter().enumerate() {
                reach(item, rest, &if path.is_empty() { i.to_string() } else { format!("{path}.{i}") }, out);
            }
        }
    } else if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
        let n: usize = head.parse().unwrap_or(usize::MAX);
        if let Value::Array(a) = value
            && n < a.len()
        {
            reach(&a[n], rest, &format!("{path}.{head}"), out);
        }
    } else if let Some(v) = value.as_object().and_then(|m| m.get(*head)) {
        reach(v, rest, &if path.is_empty() { head.to_string() } else { format!("{path}.{head}") }, out);
    }
}

/// The ids a record names: `(path, id, tags it may be)`, empty ones left out.
pub fn references(record_type: &str, record: &Value) -> Vec<(String, String, Vec<&'static str>)> {
    let mut out = Vec::new();
    let mut fields = vec![("script", vec!["SCPT"]), ("enchanting", vec!["ENCH"])];
    fields.extend(reference_fields(record_type));
    for (place, tags) in fields {
        let mut hits = Vec::new();
        reach(record, &place.split('.').collect::<Vec<_>>(), "", &mut hits);
        for (path, v) in hits {
            if let Some(s) = v.as_str().filter(|s| !s.trim().is_empty()) {
                out.push((path, s.trim().to_string(), tags.clone()));
            }
        }
    }
    if record_type == "DialogueInfo" {
        for (i, f) in record.get("filters").and_then(Value::as_array).into_iter().flatten().enumerate() {
            let kind = f.get("filter_type").map(str_of).unwrap_or_else(|| "None".into());
            let name = text(f.get("id"));
            let name = name.trim();
            if let Some(tags) = filter_tags(&kind)
                && !name.is_empty()
            {
                out.push((format!("filters.{i}.id"), name.to_string(), tags));
            }
        }
    }
    for place in ["travel_destinations.*.cell", "ai_packages.*.cell"] {
        let mut hits = Vec::new();
        reach(record, &place.split('.').collect::<Vec<_>>(), "", &mut hits);
        for (path, v) in hits {
            if let Some(s) = v.as_str().filter(|s| !s.trim().is_empty()) {
                out.push((path, s.trim().to_string(), vec!["CELL"]));
            }
        }
    }
    out
}

/// Python's `str(v or "")`.
fn text(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => str_of(v),
        _ => String::new(),
    }
}

/// One finding.
fn finding(level: &str, check: &str, tag: &str, key: &str, path: &str, message: String) -> Value {
    json!({"level": level, "check": check, "tag": tag, "id": key, "path": path, "message": message})
}

/// A record's fields naming records nothing defines.
pub fn missing_ids(tag: &str, key: &str, record_type: &str, record: &Value, exists: &dyn Fn(&[&str], &str) -> bool) -> Vec<Value> {
    references(record_type, record)
        .into_iter()
        .filter(|(_, name, tags)| !exists(tags, name))
        .map(|(path, name, tags)| finding("error", "Missing ID", tag, key, &path, format!("{path} names {name}, which no plugin defines ({})", tags.join("/"))))
        .collect()
}

/// A leveled list's entries with no id, or no entries at all.
pub fn leveled(tag: &str, key: &str, record_type: &str, record: &Value) -> Vec<Value> {
    let field = match record_type {
        "LeveledItem" => "items",
        "LeveledCreature" => "creatures",
        _ => return Vec::new(),
    };
    let entries = record.get(field).filter(|e| truthy(e)).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = Vec::new();
    if entries.is_empty() {
        out.push(finding("warning", "Leveled list", tag, key, field, "the list is empty".into()));
    }
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.as_array().and_then(|e| e.first()).and_then(Value::as_str);
        if name.is_none_or(|n| n.trim().is_empty()) {
            out.push(finding("error", "Leveled list", tag, key, &format!("{field}.{i}"), "an entry with no id".into()));
        }
    }
    out
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// A path grid's points on top of each other, points no link reaches, links to points
/// it does not have.
pub fn path_grid(tag: &str, key: &str, points: &[Value], edges: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut seen: Vec<([u64; 3], usize)> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let c: Vec<f64> = p.as_array().into_iter().flatten().take(3).map(num).collect();
        let at = [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)];
        let bits = at.map(|x| (if x == 0.0 { 0.0 } else { x }).to_bits());
        if let Some((_, j)) = seen.iter().find(|(b, _)| *b == bits) {
            out.push(finding("warning", "Path grid", tag, key, &format!("points.{i}"), format!("point {i} is on point {j} ({}, {}, {})", g(at[0]), g(at[1]), g(at[2]))));
        } else {
            seen.push((bits, i));
        }
    }
    let mut linked: HashSet<i64> = HashSet::new();
    for e in edges {
        let a = e.get(0).and_then(Value::as_i64).unwrap_or(-1);
        let b = e.get(1).and_then(Value::as_i64).unwrap_or(-1);
        let n = points.len() as i64;
        if !((0..n).contains(&a) && (0..n).contains(&b)) {
            out.push(finding("error", "Path grid", tag, key, "edges", format!("a link {a}-{b} to a missing point")));
            continue;
        }
        linked.insert(a);
        linked.insert(b);
    }
    if points.len() > 1 {
        for i in 0..points.len() {
            if !linked.contains(&(i as i64)) {
                out.push(finding("warning", "Path grid", tag, key, &format!("points.{i}"), format!("point {i} is unlinked")));
            }
        }
    }
    out
}

/// Responses the patch touches that have lost their place in their topic (`topic` as
/// `Editor::topic` gives it).
pub fn dialogue_chain(topic: &Value, touched: &HashSet<String>) -> Vec<Value> {
    let tid = str_of(&topic["id"]);
    topic["responses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| touched.contains(&r["id"].as_str().unwrap_or("").to_lowercase()) && truthy(&r["orphan"]))
        .map(|r| {
            finding(
                "error",
                "Dialogue",
                "INFO",
                r["id"].as_str().unwrap_or(""),
                "prev_id",
                format!("its place in {tid} names a response the topic does not have: the game reads it last"),
            )
        })
        .collect()
}

/// A journal's stages: one quest name, one entry per index, every one with text.
pub fn journal(topic: &Value) -> Vec<Value> {
    let tid = str_of(&topic["id"]);
    let entries: Vec<&Value> = topic["responses"].as_array().into_iter().flatten().collect();
    let mut out = Vec::new();
    let names = entries.iter().filter(|r| r["quest"] == "Name").count();
    if names > 1 {
        out.push(finding("error", "Journal", "DIAL", &tid, "quest", "more than one quest name".into()));
    }
    let stages: Vec<&&Value> = entries.iter().filter(|r| r["quest"] != "Name").collect();
    if names > 0 && stages.is_empty() {
        out.push(finding("warning", "Journal", "DIAL", &tid, "", "a quest name with no stages".into()));
    }
    let mut seen: HashMap<i64, String> = HashMap::new();
    for r in stages {
        let index = if truthy(&r["disposition"]) { r["disposition"].as_f64().map_or(0, |d| d.trunc() as i64) } else { 0 };
        let rid = r["id"].as_str().unwrap_or("").to_string();
        if let Some(first) = seen.get(&index) {
            out.push(finding("error", "Journal", "DIAL", &tid, &rid, format!("index {index} twice ({first}, {rid})")));
        }
        seen.entry(index).or_insert(rid.clone());
        if text(r.get("text")).trim().is_empty() {
            out.push(finding("warning", "Journal", "DIAL", &tid, &rid, format!("stage {index} has no text")));
        }
    }
    out
}

/// Whether a flags field (names joined with `|`) has a flag.
fn has_flag(record: &Value, path: &str, name: &str) -> bool {
    let v = value_at(record, path).map(|v| text(Some(v))).unwrap_or_default();
    v.to_uppercase().replace(' ', "").split('|').any(|f| f == name.to_uppercase())
}

/// The VFS paths an asset field may be (a texture's `.dds` too).
fn asset_paths(prefix: &str, path: &str) -> Vec<String> {
    let rel = path.trim().replace('\\', "/").to_lowercase();
    let full = if rel.starts_with(prefix) { rel } else { format!("{prefix}{rel}") };
    let mut out = vec![full.clone()];
    if (prefix == "textures/" || prefix == "icons/") && !full.ends_with(".dds") {
        let stem = full.rsplit_once('.').map_or(full.as_str(), |(s, _)| s);
        out.push(format!("{stem}.dds"));
    }
    out
}

/// Python's `==` of two optional values (absent is `None`).
fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None | Some(Value::Null), None | Some(Value::Null)) => true,
        (Some(x), Some(y)) => pyval::eq(x, y),
        _ => false,
    }
}

/// The values listed more than once, as text, sorted.
fn twice(values: &[Value]) -> Vec<String> {
    let set: BTreeSet<String> = values.iter().filter(|x| values.iter().filter(|y| pyval::eq(x, y)).count() > 1).map(str_of).collect();
    set.into_iter().collect()
}

/// OpenMW-CS's per-type checks, on one record as the patch leaves it; files looked for
/// when `file_exists` can tell.
pub fn record_checks(tag: &str, key: &str, record_type: &str, record: &Value, file_exists: Option<&dyn Fn(&str) -> bool>) -> Vec<Value> {
    let mut out = Vec::new();
    let check = record_type;
    let txt = |path: &str| value_at(record, path).map(|v| text(Some(v))).unwrap_or_default().trim().to_string();
    let mut add = |level: &str, path: &str, message: String| out.push(finding(level, check, tag, key, path, message));
    let data = record.get("data").filter(|d| d.is_object()).cloned().unwrap_or(json!({}));
    let asset = |add: &mut dyn FnMut(&str, &str, String), path: &str, prefix: &str, needed: bool| {
        let name = txt(path);
        if name.is_empty() {
            if needed {
                add("error", path, "no file is given".into());
            }
            return;
        }
        if let Some(exists) = file_exists
            && !asset_paths(prefix, &name).iter().any(|p| exists(p))
        {
            add("error", path, format!("{name} is in no data folder or archive"));
        }
    };
    if ["Class", "Faction", "Race", "Region", "Birthsign", "Spell"].contains(&record_type) && txt("name").is_empty() {
        add("warning", "name", "the name is missing".into());
    }
    match record_type {
        "Class" => {
            if same(data.get("attribute1"), data.get("attribute2")) {
                add("error", "data.attribute2", "the same attribute is listed twice".into());
            }
            let mut skills: Vec<Value> = Vec::new();
            for k in ["major", "minor"] {
                for i in 1..=5 {
                    if let Some(v) = data.get(format!("{k}{i}")) {
                        skills.push(v.clone());
                    }
                }
            }
            for t in twice(&skills) {
                add("error", "data", format!("{t} is listed twice among the skills"));
            }
            if has_flag(record, "data.flags", "PLAYABLE") && txt("description").is_empty() {
                add("warning", "description", "a playable class has no description".into());
            }
        }
        "Faction" => {
            let attrs = data.get("favored_attributes").filter(|v| truthy(v)).and_then(Value::as_array).cloned().unwrap_or_default();
            if attrs.len() == 2 && pyval::eq(&attrs[0], &attrs[1]) {
                add("error", "data.favored_attributes", "the same attribute is listed twice".into());
            }
            let skills = data.get("favored_skills").filter(|v| truthy(v)).and_then(Value::as_array).cloned().unwrap_or_default();
            for t in twice(&skills) {
                add("error", "data.favored_skills", format!("{t} is listed twice"));
            }
        }
        "Race" => {
            if txt("description").is_empty() {
                add("warning", "description", "the description is missing".into());
            }
            let list = |k: &str| data.get(k).filter(|v| truthy(v)).and_then(Value::as_array).cloned().unwrap_or_else(|| vec![json!(1), json!(1)]);
            for (sex, i) in [("male", 0), ("female", 1)] {
                let (height, weight) = (list("height"), list("weight"));
                if i < height.len() && num(&height[i]) <= 0.0 {
                    add("error", "data.height", format!("the {sex} height is not above 0"));
                }
                if i < weight.len() && num(&weight[i]) < 0.0 {
                    add("error", "data.weight", format!("the {sex} weight is below 0"));
                }
            }
        }
        "Region" => {
            let chances = record.get("weather_chances").filter(|v| truthy(v)).cloned().unwrap_or(json!({}));
            let total: i64 = match &chances {
                Value::Object(m) => m.values().map(|v| num(v).trunc() as i64).sum(),
                _ => 100,
            };
            if total != 100 {
                add("error", "weather_chances", format!("the weather chances add up to {total}, not 100"));
            }
        }
        "Birthsign" => {
            if txt("description").is_empty() {
                add("warning", "description", "the description is missing".into());
            }
            asset(&mut add, "texture", "textures/", true);
        }
        "Bodypart" => {
            asset(&mut add, "mesh", "meshes/", true);
            if ["", "Skin"].contains(&txt("data.bodypart_type").as_str()) && txt("race").is_empty() {
                add("warning", "race", "a skin part with no race".into());
            }
        }
        "Sound" => asset(&mut add, "sound_path", "sound/", true),
        "SoundGen" => {
            if txt("sound").is_empty() {
                add("error", "sound", "no sound is given".into());
            }
        }
        "MagicEffect" => {
            if txt("description").is_empty() {
                add("warning", "description", "the description is missing".into());
            }
            asset(&mut add, "icon", "icons/", true);
            asset(&mut add, "texture", "textures/", false);
        }
        "Skill" => {
            if txt("description").is_empty() {
                add("warning", "description", "the description is missing".into());
            }
        }
        "GameSetting" => {
            let want = match key.chars().next().map(|c| c.to_ascii_lowercase()) {
                Some('f') => Some("Float"),
                Some('i') => Some("Integer"),
                Some('s') => Some("String"),
                _ => None,
            };
            let have = record.get("value").filter(|v| v.is_object()).map(|v| v.get("type").map(str_of).unwrap_or_else(|| "None".into())).unwrap_or_default();
            if let Some(w) = want
                && !have.is_empty()
                && have != w
            {
                add("error", "value", format!("{key} is a {have} setting; its name says {w}"));
            }
        }
        _ => {}
    }
    if ["Spell", "Enchanting", "Alchemy"].contains(&record_type) && !record.get("effects").is_some_and(truthy) {
        add("error", "effects", "no magic effects".into());
    }
    if record_type == "Enchanting" {
        let int = |k: &str| data.get(k).filter(|v| truthy(v)).map_or(0, |v| num(v).trunc() as i64);
        let (cost, charge) = (int("cost"), int("max_charge"));
        if data.get("enchant_type").map(str_of).as_deref() != Some("ConstantEffect") && cost > charge {
            add("warning", "data.cost", format!("each use costs {cost}, more than its charge of {charge}"));
        }
    }
    out
}

/// An item whose enchantment costs more than the item can carry (its enchantment points
/// times `fEnchantmentMult`).
pub fn enchant_capacity(tag: &str, key: &str, record: &Value, cost: Option<i64>, mult: Option<f64>) -> Vec<Value> {
    let points = record.get("data").and_then(|d| d.get("enchantment")).filter(|p| p.is_number()).and_then(Value::as_f64);
    let (Some(cost), Some(mult), Some(points)) = (cost, mult, points) else { return Vec::new() };
    let capacity = (points * mult).trunc() as i64;
    if cost > capacity {
        return vec![finding(
            "warning",
            "Enchantment",
            tag,
            key,
            "enchanting",
            format!("its enchantment costs {cost}; the item carries {capacity} ({} points x fEnchantmentMult {})", g(points), g(mult)),
        )];
    }
    Vec::new()
}

/// An exterior path grid's points more than `margin` below the land (`heights`: 65 rows
/// of 65, row 0 the south edge; read bilinearly).
pub fn under_ground(tag: &str, key: &str, points: &[Value], heights: &[Vec<f64>], margin: f64) -> Vec<Value> {
    let step = 8192.0 / 64.0;
    let mut out = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let c: Vec<f64> = p.as_array().into_iter().flatten().take(3).map(num).collect();
        let (x, y, z) = (c[0], c[1], c[2]);
        let cx = (x / step).clamp(0.0, 64.0);
        let cy = (y / step).clamp(0.0, 64.0);
        let (c0, r0) = ((cx as usize).min(63), (cy as usize).min(63));
        let (fx, fy) = (cx - c0 as f64, cy - r0 as f64);
        let h = heights[r0][c0] * (1.0 - fx) * (1.0 - fy) + heights[r0][c0 + 1] * fx * (1.0 - fy) + heights[r0 + 1][c0] * (1.0 - fx) * fy + heights[r0 + 1][c0 + 1] * fx * fy;
        if z < h - margin {
            out.push(finding("warning", "Path grid", tag, key, &format!("points.{i}"), format!("point {i} is {:.0} units under the ground", h - z)));
        }
    }
    out
}

/// The globals every game needs, missing from the load order and the patch.
pub fn required_ids(exists: &dyn Fn(&[&str], &str) -> bool) -> Vec<Value> {
    REQUIRED_GLOBALS
        .iter()
        .filter(|n| !exists(&["GLOB"], n))
        .map(|n| finding("error", "Required ID", "GLOB", n, "", format!("the global {n} every game needs is missing")))
        .collect()
}

/// A landscape's 65 x 65 heights from its `vertex_heights` (`None` when there are none,
/// or they cannot be read).
pub fn land_heights(record: &Value) -> Option<Vec<Vec<f64>>> {
    let vh = record.get("vertex_heights").filter(|v| truthy(v))?;
    let data = vh.get("data").filter(|d| !d.is_null())?;
    let raw: Vec<u8> = match data {
        Value::Array(rows) => rows.iter().flat_map(|r| match r {
            Value::Array(xs) => xs.iter().map(|v| num(v) as i64 as i8 as u8).collect::<Vec<u8>>(),
            v => vec![num(v) as i64 as i8 as u8],
        }).collect(),
        v => crate::mwscript::listing::decode_field(v).ok()?,
    };
    if raw.len() < 65 * 65 {
        return None;
    }
    let offset = vh.get("offset").filter(|o| truthy(o)).map_or(0.0, num);
    let mut rows = Vec::with_capacity(65);
    let mut row_height = offset;
    for y in 0..65 {
        let base = y * 65;
        row_height += raw[base] as i8 as f64;
        let mut height = row_height;
        let mut row = vec![height * 8.0];
        for x in 1..65 {
            height += raw[base + x] as i8 as f64;
            row.push(height * 8.0);
        }
        rows.push(row);
    }
    Some(rows)
}

impl Editor {
    /// Every id (lower case) a tag has over the load order's plugins.
    pub fn plugin_ids(&self, tag: &str) -> HashSet<String> {
        self.session.order().iter().flat_map(|p| self.session.records(p, tag).keys().cloned().collect::<Vec<_>>()).collect()
    }

    /// A numeric game setting as the load order leaves it (the last plugin's).
    fn gmst(&self, name: &str) -> Option<f64> {
        let mut value = None;
        for plugin in self.session.order() {
            if let Some(rec) = self.session.records(plugin, "GMST").get(&name.to_lowercase())
                && let Some(d) = rec.get("value").and_then(|v| v.get("data")).filter(|d| d.is_number())
            {
                value = d.as_f64();
            }
        }
        value
    }

    /// The cost of the enchantment an item names, as the patch leaves it.
    fn enchant_cost(&self, record: &Value) -> Option<i64> {
        let found = self.find("ENCH", text(record.get("enchanting")).trim(), None)?;
        self.as_written(&found).get("data").and_then(|d| d.get("cost")).filter(|c| c.is_number()).and_then(Value::as_f64).map(|c| c.trunc() as i64)
    }

    /// An exterior cell's land heights, the last plugin's (none: flat, not checked).
    fn land(&self, grid: &Value) -> Option<Vec<Vec<f64>>> {
        let g = grid.as_array().filter(|g| g.len() == 2)?;
        let want = (num(&g[0]) as i64, num(&g[1]) as i64);
        let mut found = None;
        for plugin in self.session.order() {
            for rec in self.session.records(plugin, "LAND").values() {
                let rg = rec.get("grid").and_then(Value::as_array);
                if rg.is_some_and(|rg| rg.len() == 2 && (num(&rg[0]) as i64, num(&rg[1]) as i64) == want) {
                    found = Some(rec.clone());
                }
            }
        }
        land_heights(&found?)
    }

    /// Each response's topic: lower-case id -> topic (the last plugin's word winning; the
    /// patch's own responses their own).
    fn info_topics(&self) -> HashMap<String, String> {
        let mut owner = HashMap::new();
        for plugin in self.session.order() {
            let mut topic = String::new();
            for rec in self.session.dialogue(plugin).iter() {
                match rec.get("type").and_then(Value::as_str) {
                    Some("Dialogue") => topic = record_key(rec),
                    Some("DialogueInfo") if !topic.is_empty() => {
                        owner.insert(record_key(rec).to_lowercase(), topic.clone());
                    }
                    _ => {}
                }
            }
        }
        for made in self.queue.new_records() {
            if made.record_type == "DialogueInfo" && !made.topic.is_empty() {
                owner.insert(made.key.to_lowercase(), made.topic.clone());
            }
        }
        owner
    }

    /// A script's compile errors and Script Edit's errors, as findings.
    fn script_findings(&self, found: &Found, record: &Value) -> Vec<Value> {
        match self.script_view(found, Some(&text(record.get("text")))) {
            Err(e) => vec![finding("warning", "Script", &found.tag, &found.key, "text", format!("not checked: {}", e.0))],
            Ok(view) => {
                let errors = view["compile"]["messages"].as_array().into_iter().flatten().chain(view["findings"].as_array().into_iter().flatten()).filter(|m| m["level"] == "error");
                errors.map(|m| finding("error", "Script", &found.tag, &found.key, &format!("text:{}", m["line"]), str_of(&m["message"]))).collect()
            }
        }
    }

    /// Every check over what the patch carries: `{findings, checked}` - `checked` how many
    /// records. `file_exists` looks for the files records name (`None`: not looked for).
    pub fn verify(&self, file_exists: Option<&dyn Fn(&str) -> bool>) -> Value {
        let known: std::cell::RefCell<HashMap<String, HashSet<String>>> = Default::default();
        let exists = |tags: &[&str], name: &str| {
            let want = name.to_lowercase();
            tags.iter().any(|t| {
                let mut k = known.borrow_mut();
                let ids = k.entry(t.to_string()).or_insert_with(|| {
                    let mut have = self.plugin_ids(t);
                    have.extend(self.queue.new_records().filter(|m| tag_of(&m.record_type) == Some(*t)).map(|m| m.key.to_lowercase()));
                    have
                });
                ids.contains(&want)
            })
        };
        let mut findings = required_ids(&exists);
        let mut checked = 0;
        let mut infos: Vec<String> = Vec::new();
        let mut journals: BTreeSet<String> = BTreeSet::new();
        for entry in self.pending() {
            if !entry.get("tag").is_some_and(truthy) || entry.get("ref").is_some_and(truthy) || entry.get("new").is_some_and(truthy) {
                continue;
            }
            let Some(found) = self.find(entry["tag"].as_str().unwrap_or(""), entry["id"].as_str().unwrap_or(""), None) else { continue };
            checked += 1;
            let record = self.as_written(&found);
            let (tag, key, rtype) = (found.tag.as_str(), found.key.as_str(), found.record_type.as_str());
            findings.extend(missing_ids(tag, key, rtype, &record, &exists));
            findings.extend(leveled(tag, key, rtype, &record));
            findings.extend(record_checks(tag, key, rtype, &record, file_exists));
            if !text(record.get("enchanting")).trim().is_empty() {
                findings.extend(enchant_capacity(tag, key, &record, self.enchant_cost(&record), self.gmst("fEnchantmentMult")));
            }
            let changes_text = entry.get("whole").is_some_and(truthy)
                || entry.get("made").is_some_and(truthy)
                || entry["changes"].as_array().into_iter().flatten().any(|c| c["path"] == "text");
            if rtype == "Script" && changes_text {
                findings.extend(self.script_findings(&found, &record));
            }
            if rtype == "DialogueInfo" {
                infos.push(key.to_lowercase());
            }
            if rtype == "Dialogue" && record.get("dialogue_type").map(str_of).as_deref() == Some("Journal") {
                journals.insert(key.to_string());
            }
            if rtype == "PathGrid" {
                let grid = record.get("data").and_then(|d| d.get("grid")).filter(|g| truthy(g)).cloned().unwrap_or(json!([0, 0]));
                let at = grid.as_array().map(|g| g.iter().map(num).collect::<Vec<_>>()).unwrap_or_default();
                let cell = if at == [0.0, 0.0] {
                    format!("int:{}", if record.get("cell").is_some_and(truthy) { text(record.get("cell")) } else { key.to_string() })
                } else {
                    format!("({}, {})", str_of(&grid[0]), str_of(&grid[1]))
                };
                let view = self.pathgrid_view(&cell);
                let points = view["points"].as_array().cloned().unwrap_or_default();
                let edges = view["edges"].as_array().cloned().unwrap_or_default();
                findings.extend(path_grid("PGRD", key, &points, &edges));
                if !cell.starts_with("int:")
                    && let Some(heights) = view.get("grid").and_then(|g| self.land(g))
                {
                    findings.extend(under_ground("PGRD", key, &points, &heights, 16.0));
                }
            }
        }
        // References the patch places, or gives another object, naming one nothing defines.
        let objects = object_tags();
        for new in self.queue.new_refs() {
            let name = new.fields.get(&"id".to_string()).map(|v| text(Some(v))).unwrap_or_default();
            if !name.is_empty() && !exists(&objects, &name) {
                findings.push(finding("error", "Reference", "CELL", &new.cell, &new.uid, format!("places {name}, which no plugin defines")));
            }
        }
        for edit in self.queue.ref_edits() {
            let name = edit.changes.get(&"id".to_string()).map(|v| text(Some(v))).unwrap_or_default();
            if !name.is_empty() && !exists(&objects, &name) {
                findings.push(finding("error", "Reference", "CELL", &edit.cell, &format!("{}#{}", edit.origin, edit.refr_index), format!("becomes {name}, which no plugin defines")));
            }
        }
        let mut topics: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        if !infos.is_empty() {
            let owner = self.info_topics();
            for info in infos {
                if let Some(t) = owner.get(&info) {
                    topics.entry(t.clone()).or_default().insert(info);
                }
            }
        }
        let all: BTreeSet<String> = topics.keys().cloned().chain(journals).collect();
        for topic in all {
            if let Ok(read) = self.topic(&topic) {
                let touched = topics.get(&topic).cloned().unwrap_or_default();
                findings.extend(dialogue_chain(&read, &touched));
                if read["type"] == "Journal" {
                    findings.extend(journal(&read));
                }
            }
        }
        json!({"findings": findings, "checked": checked})
    }
}
