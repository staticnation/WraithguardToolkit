//! A blank record of a type, as the crate makes it (every field at its default): what
//! the Editor's "New" starts from (`EditorSession.insert`, which used the Python port's
//! classes, `wraithguard.esp.json.record_to_json(cls())`). Pinned against the Python
//! blanks by `tests/fixtures/blank_records.json`.

use serde_json::Value;
use tes3::esp::*;

macro_rules! blanks {
    ($t:expr, $($name:ident),* $(,)?) => {
        match $t {
            $(stringify!($name) => Some(TES3Object::$name($name::default())),)*
            _ => None,
        }
    };
}

/// The blank record of a tes3conv `type`, as tes3conv-schema JSON; `None` for a type the
/// crate does not model.
pub fn blank(record_type: &str) -> Option<Value> {
    let obj = blanks!(
        record_type, Header, GameSetting, GlobalVariable, Class, Faction, Race, Sound, SoundGen, Skill, MagicEffect,
        Script, Region, Birthsign, StartScript, LandscapeTexture, Spell, Static, Door, MiscItem, Weapon, Container,
        Creature, Bodypart, Light, Enchanting, Npc, Armor, Clothing, RepairItem, Activator, Apparatus, Lockpick, Probe,
        Ingredient, Book, Alchemy, LeveledItem, LeveledCreature, Cell, Landscape, PathGrid, Dialogue, DialogueInfo,
    )?;
    let text = serde_json::to_string(&obj).ok()?;
    serde_json::from_str(&text).ok()
}
