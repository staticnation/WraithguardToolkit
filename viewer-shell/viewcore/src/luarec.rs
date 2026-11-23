//! Any record of the load order for the Preview's Lua, by id, as OpenMW's Lua gives it
//! (Static 2026-11-23: "The fix is for the engine to look up any record from the load order on
//! demand, not just placed ones").
//!
//! `types.Miscellaneous.records['misc_soulgem_petty'].value`, `types.NPC.classes.record(id)
//! .majorSkills`, `types.NPC.races.records[id].height.female`, `core.magic.spells.records[id]`:
//! the scripts asked for records of things the scene had not placed, and had a stand-in. The
//! worker asks the program mid-call (`luaproc`, `{"t":"rec"}`); the program answers from here.
//!
//! [`Records`] finds a record by its id through the world's `record_where` (the plugin whose
//! version is live), reads where each record of that plugin starts once (the first time it
//! is asked about one of them), and after that reads only the record asked for. The record is
//! typed by Gardenfell and given OpenMW's field names (`files/lua_api/openmw/types.lua`).

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use gardenfell_tes3::tes3::bytes::Reader;
use gardenfell_tes3::tes3::raw::Record;
use gardenfell_tes3::tes3::types::{Effect, Object};
use serde_json::{json, Map, Value};

use crate::esp::Tag;

/// OpenMW's skill ids, in the game's order.
pub const SKILLS: [&str; 27] = [
    "block", "armorer", "mediumarmor", "heavyarmor", "bluntweapon", "longblade", "axe", "spear", "athletics", "enchant",
    "destruction", "alteration", "illusion", "conjuration", "mysticism", "restoration", "alchemy", "unarmored", "security",
    "sneak", "acrobatics", "lightarmor", "shortblade", "marksman", "mercantile", "speechcraft", "handtohand",
];
/// OpenMW's attribute ids, in the game's order.
pub const ATTRIBUTES: [&str; 8] = ["strength", "intelligence", "willpower", "agility", "speed", "endurance", "personality", "luck"];
const SPECIALIZATIONS: [&str; 3] = ["combat", "magic", "stealth"];
/// OpenMW's magic effect ids, by the game's effect index.
pub const EFFECTS: [&str; 143] = [
    "waterbreathing", "swiftswim", "waterwalking", "shield", "fireshield", "lightningshield", "frostshield", "burden", "feather",
    "jump", "levitate", "slowfall", "lock", "open", "firedamage", "shockdamage", "frostdamage", "drainattribute", "drainhealth",
    "drainmagicka", "drainfatigue", "drainskill", "damageattribute", "damagehealth", "damagemagicka", "damagefatigue",
    "damageskill", "poison", "weaknesstofire", "weaknesstofrost", "weaknesstoshock", "weaknesstomagicka",
    "weaknesstocommondisease", "weaknesstoblightdisease", "weaknesstocorprusdisease", "weaknesstopoison",
    "weaknesstonormalweapons", "disintegrateweapon", "disintegratearmor", "invisibility", "chameleon", "light", "sanctuary",
    "nighteye", "charm", "paralyze", "silence", "blind", "sound", "calmhumanoid", "calmcreature", "frenzyhumanoid",
    "frenzycreature", "demoralizehumanoid", "demoralizecreature", "rallyhumanoid", "rallycreature", "dispel", "soultrap",
    "telekinesis", "mark", "recall", "divineintervention", "almsiviintervention", "detectanimal", "detectenchantment",
    "detectkey", "spellabsorption", "reflect", "curecommondisease", "cureblightdisease", "curecorprusdisease", "curepoison",
    "cureparalyzation", "restoreattribute", "restorehealth", "restoremagicka", "restorefatigue", "restoreskill",
    "fortifyattribute", "fortifyhealth", "fortifymagicka", "fortifyfatigue", "fortifyskill", "fortifymaximummagicka",
    "absorbattribute", "absorbhealth", "absorbmagicka", "absorbfatigue", "absorbskill", "resistfire", "resistfrost",
    "resistshock", "resistmagicka", "resistcommondisease", "resistblightdisease", "resistcorprusdisease", "resistpoison",
    "resistnormalweapons", "resistparalysis", "removecurse", "turnundead", "summonscamp", "summonclannfear", "summondaedroth",
    "summondremora", "summonancestralghost", "summonskeletalminion", "summonbonewalker", "summongreaterbonewalker",
    "summonbonelord", "summonwingedtwilight", "summonhunger", "summongoldensaint", "summonflameatronach",
    "summonfrostatronach", "summonstormatronach", "fortifyattack", "commandcreature", "commandhumanoid", "bounddagger",
    "boundlongsword", "boundmace", "boundbattleaxe", "boundspear", "boundlongbow", "extraspell", "boundcuirass", "boundhelm",
    "boundboots", "boundshield", "boundgloves", "corprus", "vampirism", "summoncenturionsphere", "sundamage",
    "stuntedmagicka", "summonfabricant", "summonwolf", "summonbear", "summonbonewolf", "summoncreature04", "summoncreature05",
];

fn skill(i: i32) -> Value {
    usize::try_from(i).ok().and_then(|i| SKILLS.get(i)).map_or(Value::Null, |s| json!(s))
}
fn attribute(i: i32) -> Value {
    usize::try_from(i).ok().and_then(|i| ATTRIBUTES.get(i)).map_or(Value::Null, |s| json!(s))
}
/// A file path as OpenMW's records give it: under its folder, forward slashes, lower case.
fn vfs(dir: &str, p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    let p = p.replace('\\', "/").to_ascii_lowercase();
    if p.starts_with(&format!("{dir}/")) {
        p
    } else {
        format!("{dir}/{p}")
    }
}
fn opt_id(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        json!(s.to_ascii_lowercase())
    }
}

/// A spell's, potion's or enchantment's effects (`MagicEffectWithParams`).
fn effects(list: &[Effect]) -> Value {
    Value::Array(
        list.iter()
            .enumerate()
            .map(|(i, e)| {
                json!({
                    "id": usize::try_from(e.magic_effect).ok().and_then(|n| EFFECTS.get(n)).copied().unwrap_or(""),
                    "index": i,
                    "affectedSkill": skill(e.skill as i32),
                    "affectedAttribute": attribute(e.attribute as i32),
                    "range": e.range, "area": e.area, "duration": e.duration,
                    "magnitudeMin": e.min_magnitude, "magnitudeMax": e.max_magnitude,
                })
            })
            .collect(),
    )
}

/// The common fields of a carried item.
fn item(id: &str, name: &str, script: &str, mesh: &str, icon: &str, weight: f32, value: u32) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), json!(id.to_ascii_lowercase()));
    m.insert("name".into(), json!(name));
    m.insert("mwscript".into(), opt_id(script));
    m.insert("model".into(), json!(vfs("meshes", mesh)));
    m.insert("icon".into(), json!(vfs("icons", icon)));
    m.insert("weight".into(), json!(weight));
    m.insert("value".into(), json!(value));
    m
}

/// A typed record as OpenMW's Lua gives it (its field names), with `type`'s own `recordType`
/// (the four letters) beside: `None` for a kind the scripts have no records of.
pub fn to_lua(o: &Object) -> Option<Value> {
    use Object as O;
    let mut m = match o {
        O::MiscItem(r) => {
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, r.data.weight, r.data.value);
            m.insert("isKey".into(), json!(r.data.flags & 1 != 0));
            m
        }
        O::Weapon(r) => {
            let d = &r.data;
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, d.weight, d.value);
            for (k, v) in [
                ("type", json!(d.weapon_type)), ("health", json!(d.health)), ("speed", json!(d.speed)), ("reach", json!(d.reach)),
                ("enchantCapacity", json!(d.enchantment as f32 / 10.0)), ("enchant", opt_id(&r.enchanting)),
                ("chopMinDamage", json!(d.chop_min)), ("chopMaxDamage", json!(d.chop_max)),
                ("slashMinDamage", json!(d.slash_min)), ("slashMaxDamage", json!(d.slash_max)),
                ("thrustMinDamage", json!(d.thrust_min)), ("thrustMaxDamage", json!(d.thrust_max)),
                ("isMagical", json!(d.flags & 1 != 0)), ("isSilver", json!(d.flags & 2 != 0)),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Armor(r) => {
            let d = &r.data;
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, d.weight, d.value);
            for (k, v) in [
                ("type", json!(d.armor_type)), ("health", json!(d.health)), ("baseArmor", json!(d.armor_rating)),
                ("enchantCapacity", json!(d.enchantment as f32 / 10.0)), ("enchant", opt_id(&r.enchanting)),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Clothing(r) => {
            let d = &r.data;
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, d.weight, d.value as u32);
            m.insert("type".into(), json!(d.clothing_type));
            m.insert("enchantCapacity".into(), json!(d.enchantment as f32 / 10.0));
            m.insert("enchant".into(), opt_id(&r.enchanting));
            m
        }
        O::Book(r) => {
            let d = &r.data;
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, d.weight, d.value);
            m.insert("isScroll".into(), json!(d.book_type == 1));
            m.insert("skill".into(), skill(d.skill));
            m.insert("text".into(), json!(r.text));
            m.insert("enchantCapacity".into(), json!(d.enchantment as f32 / 10.0));
            m.insert("enchant".into(), opt_id(&r.enchanting));
            m
        }
        O::Alchemy(r) => {
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, r.data.weight, r.data.value);
            m.insert("isAutocalc".into(), json!(r.data.flags & 1 != 0));
            m.insert("effects".into(), effects(&r.effects));
            m
        }
        O::Ingredient(r) => {
            let d = &r.data;
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, d.weight, d.value);
            let list: Vec<Value> = (0..4)
                .filter(|&i| d.effects[i] >= 0)
                .map(|i| {
                    json!({
                        "id": usize::try_from(d.effects[i]).ok().and_then(|n| EFFECTS.get(n)).copied().unwrap_or(""),
                        "index": i, "affectedSkill": skill(d.skills[i]), "affectedAttribute": attribute(d.attributes[i]),
                        "range": 0, "area": 0, "duration": 0, "magnitudeMin": 0, "magnitudeMax": 0,
                    })
                })
                .collect();
            m.insert("effects".into(), Value::Array(list));
            m
        }
        O::Light(r) => {
            let d = &r.data;
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, d.weight, d.value);
            let f = d.flags;
            for (k, v) in [
                ("duration", json!(d.time)), ("radius", json!(d.radius)),
                ("color", json!(u32::from_le_bytes(d.color))), ("sound", opt_id(&r.sound)),
                ("isDynamic", json!(f & 0x1 != 0)), ("isCarriable", json!(f & 0x2 != 0)), ("isNegative", json!(f & 0x4 != 0)),
                ("isFlicker", json!(f & 0x8 != 0)), ("isFire", json!(f & 0x10 != 0)), ("isOffByDefault", json!(f & 0x20 != 0)),
                ("isFlickerSlow", json!(f & 0x40 != 0)), ("isPulse", json!(f & 0x80 != 0)), ("isPulseSlow", json!(f & 0x100 != 0)),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Apparatus(r) => {
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, r.data.weight, r.data.value);
            m.insert("type".into(), json!(r.data.apparatus_type));
            m.insert("quality".into(), json!(r.data.quality));
            m
        }
        O::Lockpick(r) => {
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, r.data.weight, r.data.value);
            m.insert("quality".into(), json!(r.data.quality));
            m.insert("maxCondition".into(), json!(r.data.uses));
            m
        }
        O::Probe(r) => {
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, r.data.weight, r.data.value);
            m.insert("quality".into(), json!(r.data.quality));
            m.insert("maxCondition".into(), json!(r.data.uses));
            m
        }
        O::RepairItem(r) => {
            let mut m = item(&r.id, &r.name, &r.script, &r.mesh, &r.icon, r.data.weight, r.data.value);
            m.insert("quality".into(), json!(r.data.quality));
            m.insert("maxCondition".into(), json!(r.data.uses));
            m
        }
        O::Activator(r) => {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("mwscript".into(), opt_id(&r.script));
            m.insert("model".into(), json!(vfs("meshes", &r.mesh)));
            m
        }
        O::Container(r) => {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("mwscript".into(), opt_id(&r.script));
            m.insert("model".into(), json!(vfs("meshes", &r.mesh)));
            m.insert("weight".into(), json!(r.encumbrance));
            m.insert("isOrganic".into(), json!(r.container_flags & 1 != 0));
            m.insert("isRespawning".into(), json!(r.container_flags & 2 != 0));
            m
        }
        O::Door(r) => {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("mwscript".into(), opt_id(&r.script));
            m.insert("model".into(), json!(vfs("meshes", &r.mesh)));
            m.insert("openSound".into(), opt_id(&r.open_sound));
            m.insert("closeSound".into(), opt_id(&r.close_sound));
            m
        }
        O::Static(r) => {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("model".into(), json!(vfs("meshes", &r.mesh)));
            m
        }
        O::Npc(r) => {
            let f = r.npc_flags;
            let mut m = Map::new();
            for (k, v) in [
                ("id", json!(r.id.to_ascii_lowercase())), ("name", json!(r.name)), ("mwscript", opt_id(&r.script)),
                ("model", json!(vfs("meshes", &r.mesh))), ("race", opt_id(&r.race)), ("class", opt_id(&r.class)),
                ("head", opt_id(&r.head)), ("hair", opt_id(&r.hair)), ("primaryFaction", opt_id(&r.faction)),
                ("primaryFactionRank", if r.faction.is_empty() { json!(0) } else { json!(r.data.rank as i32 + 1) }),
                ("baseDisposition", json!(r.data.disposition)), ("baseGold", json!(r.data.gold)), ("bloodType", json!(r.blood_type)),
                ("isMale", json!(f & 0x1 == 0)), ("isEssential", json!(f & 0x2 != 0)), ("isRespawning", json!(f & 0x4 != 0)),
                ("isAutocalc", json!(f & 0x10 != 0)), ("isPersistent", json!(r.flags & 0x400 != 0)),
                ("level", json!(r.data.level)), ("spells", json!(r.spells.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>())),
                ("travelDestinations", json!([])),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Creature(r) => {
            let d = &r.data;
            let f = r.creature_flags;
            let mut m = Map::new();
            for (k, v) in [
                ("id", json!(r.id.to_ascii_lowercase())), ("name", json!(r.name)), ("mwscript", opt_id(&r.script)),
                ("model", json!(vfs("meshes", &r.mesh))), ("type", json!(d.creature_type)), ("soulValue", json!(d.soul)),
                ("combatSkill", json!(d.combat)), ("magicSkill", json!(d.magic)), ("stealthSkill", json!(d.stealth)),
                ("attack", json!([d.attack1.0, d.attack1.1, d.attack2.0, d.attack2.1, d.attack3.0, d.attack3.1])),
                ("baseGold", json!(d.gold)), ("bloodType", json!(r.blood_type)), ("baseCreature", Value::Null),
                ("isBiped", json!(f & 0x1 != 0)), ("isRespawning", json!(f & 0x2 != 0)), ("canUseWeapons", json!(f & 0x4 != 0)),
                ("canSwim", json!(f & 0x10 != 0)), ("canFly", json!(f & 0x20 != 0)), ("canWalk", json!(f & 0x40 != 0)),
                ("isEssential", json!(f & 0x80 != 0)), ("isPersistent", json!(r.flags & 0x400 != 0)), ("level", json!(d.level)),
                ("spells", json!(r.spells.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>())), ("travelDestinations", json!([])),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Class(r) => {
            let d = &r.data;
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("description".into(), json!(r.description));
            m.insert("attributes".into(), json!([attribute(d.attribute1), attribute(d.attribute2)]));
            m.insert("majorSkills".into(), json!([skill(d.major1), skill(d.major2), skill(d.major3), skill(d.major4), skill(d.major5)]));
            m.insert("minorSkills".into(), json!([skill(d.minor1), skill(d.minor2), skill(d.minor3), skill(d.minor4), skill(d.minor5)]));
            m.insert("specialization".into(), usize::try_from(d.specialization).ok().and_then(|i| SPECIALIZATIONS.get(i)).map_or(Value::Null, |s| json!(s)));
            m.insert("isPlayable".into(), json!(d.flags & 1 != 0));
            m
        }
        O::Race(r) => {
            let d = &r.data;
            let b = &d.skill_bonuses;
            let mut skills = Map::new();
            for (s, n) in [(b.skill_0, b.bonus_0), (b.skill_1, b.bonus_1), (b.skill_2, b.bonus_2), (b.skill_3, b.bonus_3), (b.skill_4, b.bonus_4), (b.skill_5, b.bonus_5), (b.skill_6, b.bonus_6)] {
                if let Value::String(k) = skill(s) {
                    skills.insert(k, json!(n));
                }
            }
            let mut attrs = Map::new();
            for (k, v) in ATTRIBUTES.iter().zip([d.strength, d.intelligence, d.willpower, d.agility, d.speed, d.endurance, d.personality, d.luck]) {
                attrs.insert((*k).into(), json!({"male": v[0], "female": v[1]}));
            }
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("description".into(), json!(r.description));
            m.insert("height".into(), json!({"male": d.height[0], "female": d.height[1]}));
            m.insert("weight".into(), json!({"male": d.weight[0], "female": d.weight[1]}));
            m.insert("isPlayable".into(), json!(d.flags & 1 != 0));
            m.insert("isBeast".into(), json!(d.flags & 2 != 0));
            m.insert("skills".into(), Value::Object(skills));
            m.insert("attributes".into(), Value::Object(attrs));
            m.insert("spells".into(), json!(r.spells.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>()));
            m
        }
        O::Spell(r) => {
            let f = r.data.flags;
            let mut m = Map::new();
            for (k, v) in [
                ("id", json!(r.id.to_ascii_lowercase())), ("name", json!(r.name)), ("type", json!(r.data.spell_type)),
                ("cost", json!(r.data.cost)), ("isAutocalc", json!(f & 1 != 0)), ("autocalcFlag", json!(f & 1 != 0)),
                ("starterSpellFlag", json!(f & 2 != 0)), ("alwaysSucceedFlag", json!(f & 4 != 0)), ("effects", effects(&r.effects)),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Enchanting(r) => {
            let d = &r.data;
            let mut m = Map::new();
            for (k, v) in [
                ("id", json!(r.id.to_ascii_lowercase())), ("type", json!(d.enchant_type)), ("cost", json!(d.cost)),
                ("charge", json!(d.max_charge)), ("isAutocalc", json!(d.flags & 1 != 0)), ("autocalcFlag", json!(d.flags & 1 != 0)),
                ("effects", effects(&r.effects)),
            ] {
                m.insert(k.into(), v);
            }
            m
        }
        O::Faction(r) => {
            let d = &r.data;
            let ranks: Vec<Value> = r
                .rank_names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let q = d.requirements.get(i);
                    json!({
                        "name": n,
                        "attributeValues": q.map(|q| q.attributes.to_vec()).unwrap_or_default(),
                        "primarySkillValue": q.map_or(0, |q| q.primary_skill),
                        "favouredSkillValue": q.map_or(0, |q| q.favored_skill),
                        "factionReaction": q.map_or(0, |q| q.reputation),
                    })
                })
                .collect();
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("hidden".into(), json!(d.flags & 1 != 0));
            m.insert("attributes".into(), json!(d.favored_attributes.iter().map(|&a| attribute(a)).collect::<Vec<_>>()));
            m.insert("skills".into(), json!(d.favored_skills.iter().filter(|&&s| s >= 0).map(|&s| skill(s)).collect::<Vec<_>>()));
            m.insert("ranks".into(), Value::Array(ranks));
            m
        }
        O::Birthsign(r) => {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("name".into(), json!(r.name));
            m.insert("description".into(), json!(r.description));
            m.insert("texture".into(), json!(vfs("textures", &r.texture)));
            m.insert("spells".into(), json!(r.spells.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>()));
            m
        }
        O::Script(r) => {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.id.to_ascii_lowercase()));
            m.insert("text".into(), json!(r.text));
            m
        }
        _ => return None,
    };
    let tag = o.tag();
    m.insert("recordType".into(), json!(String::from_utf8_lossy(&tag)));
    Some(Value::Object(m))
}

/// Where each record of a plugin starts (offset, length), by tag and lowercased id.
type Offsets = HashMap<(Tag, String), (u64, u64)>;

/// A lowercased id's plugins and tags, the live one last (the world's `record_where`).
pub type Where = Box<dyn Fn(&str) -> Option<Vec<(usize, Tag)>> + Send>;

/// Records of a load order by id (see the module's note). `paths`: the plugins, in load order.
pub struct Records {
    paths: Vec<PathBuf>,
    where_: Where,
    offsets: HashMap<usize, Offsets>,
}

impl Records {
    pub fn new(paths: Vec<PathBuf>, where_: Where) -> Records {
        Records { paths, where_, offsets: HashMap::new() }
    }

    /// Over a loaded world (shared, not copied).
    pub fn of_world(w: std::sync::Arc<crate::world::World>) -> Records {
        let paths = w.plugin_paths.clone();
        Records::new(paths, Box::new(move |id: &str| w.record_where.get(id).cloned()))
    }

    /// The record `id` as OpenMW's Lua gives it (JSON), of one of the `kinds` (four-letter tags;
    /// any, when empty); `None` when the load order has none.
    pub fn lookup(&mut self, kinds: &[&str], id: &str) -> Option<String> {
        let lid = id.to_ascii_lowercase();
        let defs = (self.where_)(&lid)?;
        let &(p, tag) = defs.iter().rev().find(|(_, t)| kinds.is_empty() || kinds.iter().any(|k| k.as_bytes() == t))?;
        let path = self.paths.get(p)?.clone();
        let offs = match self.offsets.get(&p) {
            Some(o) => o,
            None => {
                let buf = std::fs::read(&path).ok()?;
                let o = offsets_of(&buf);
                self.offsets.entry(p).or_insert(o)
            }
        };
        let &(at, len) = offs.get(&(tag, lid))?;
        let mut f = std::fs::File::open(&path).ok()?;
        f.seek(SeekFrom::Start(at)).ok()?;
        let mut bytes = vec![0u8; len as usize];
        f.read_exact(&mut bytes).ok()?;
        to_lua(&typed(&bytes)?).map(|v| v.to_string())
    }
}

/// One record's bytes (header and body), typed.
pub fn typed(bytes: &[u8]) -> Option<Object> {
    let rec = Record::read(&mut Reader::new(bytes)).ok()?;
    Object::from_raw(&rec).ok().flatten()
}

/// Where each id-bearing record of a plugin starts.
fn offsets_of(buf: &[u8]) -> Offsets {
    let mut out = HashMap::new();
    let mut pos = 0usize;
    while pos + 16 <= buf.len() {
        let tag: Tag = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
        let size = u32::from_le_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]) as usize;
        let end = pos + 16 + size;
        if end > buf.len() {
            break;
        }
        if !matches!(&tag, b"CELL" | b"LAND" | b"PGRD" | b"DIAL" | b"INFO" | b"TES3") {
            if let Some(id) = crate::objects::record_id(tag, &buf[pos + 16..end]) {
                out.insert((tag, id.to_ascii_lowercase()), (pos as u64, (end - pos) as u64));
            }
        }
        pos = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }
    fn rec(tag: &[u8; 4], subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.concat();
        let mut v = tag.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend([0u8; 8]);
        v.extend(body);
        v
    }
    fn z(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    /// A plugin's class, race and misc item, found by id and given as OpenMW's Lua has them;
    /// a later plugin's version wins; an id of another kind is not taken for the one asked.
    #[test]
    fn a_record_not_placed_is_found_by_id_with_openmw_s_names() {
        let dir = std::env::temp_dir().join(format!("wg-luarec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cldt = Vec::new();
        // CLDT: two attributes, the specialization, then minor and major skills by turns.
        for v in [0i32, 4, 2, 21, 22, 23, 24, 25, 26, 0, 1, 2, 3] {
            cldt.extend(v.to_le_bytes());
        }
        cldt.extend(1u32.to_le_bytes());
        cldt.extend(0u32.to_le_bytes());
        let class = rec(b"CLAS", &[sub(b"NAME", &z("WG_Thief")), sub(b"FNAM", &z("Thief")), sub(b"CLDT", &cldt)]);
        let mut radt = Vec::new();
        for (s, b) in [(19i32, 10i32), (-1, 0), (-1, 0), (-1, 0), (-1, 0), (-1, 0), (-1, 0)] {
            radt.extend(s.to_le_bytes());
            radt.extend(b.to_le_bytes());
        }
        for _ in 0..8 {
            radt.extend(40i32.to_le_bytes());
            radt.extend(30i32.to_le_bytes());
        }
        for f in [1.0f32, 0.9, 1.1, 0.95] {
            radt.extend(f.to_le_bytes());
        }
        radt.extend(3u32.to_le_bytes());
        let race = rec(b"RACE", &[sub(b"NAME", &z("WG_Cat")), sub(b"FNAM", &z("Cat")), sub(b"RADT", &radt)]);
        let misc = |v: u32| {
            let mut d = 0.1f32.to_le_bytes().to_vec();
            d.extend(v.to_le_bytes());
            d.extend(0u32.to_le_bytes());
            rec(b"MISC", &[sub(b"NAME", &z("wg_gem")), sub(b"MODL", &z("m\\Gem.nif")), sub(b"FNAM", &z("Gem")), sub(b"MCDT", &d)])
        };
        let a = dir.join("a.esp");
        let b = dir.join("b.esp");
        std::fs::write(&a, [class.clone(), race, misc(10)].concat()).unwrap();
        std::fs::write(&b, misc(25)).unwrap();
        let mut w: HashMap<String, Vec<(usize, Tag)>> = HashMap::new();
        w.insert("wg_thief".into(), vec![(0, *b"CLAS")]);
        w.insert("wg_cat".into(), vec![(0, *b"RACE")]);
        w.insert("wg_gem".into(), vec![(0, *b"MISC"), (1, *b"MISC")]);
        let mut r = Records::new(vec![a, b], Box::new(move |id: &str| w.get(id).cloned()));
        let c: Value = serde_json::from_str(&r.lookup(&["CLAS"], "WG_Thief").unwrap()).unwrap();
        assert_eq!(c["majorSkills"], json!(["shortblade", "mercantile", "handtohand", "armorer", "heavyarmor"]), "{c}");
        assert_eq!(c["specialization"], json!("stealth"));
        assert_eq!(c["attributes"], json!(["strength", "speed"]));
        let rc: Value = serde_json::from_str(&r.lookup(&["RACE"], "wg_cat").unwrap()).unwrap();
        assert!((rc["height"]["female"].as_f64().unwrap() - 0.9).abs() < 1e-6 && rc["isBeast"] == json!(true) && rc["skills"]["sneak"] == json!(10), "{rc}");
        let g: Value = serde_json::from_str(&r.lookup(&["MISC"], "wg_gem").unwrap()).unwrap();
        assert_eq!((g["value"].clone(), g["model"].clone()), (json!(25), json!("meshes/m/gem.nif")), "the later plugin's: {g}");
        assert!(r.lookup(&["RACE"], "wg_gem").is_none() && r.lookup(&[], "nothing").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
