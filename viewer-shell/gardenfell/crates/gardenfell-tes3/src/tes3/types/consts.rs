//! The known values of every flag word and enumeration, as typed constants. `tes3`
//! flags.rs and enums.rs.
//!
//! One module per `tes3` flag struct or enum, each constant typed as the width the field
//! has in the file. Where `tes3` has the same enumeration at two widths (`SkillId` as
//! `i32` and `SkillId2` as `i8`, for instance) there is one module of `i32` constants,
//! and a field of a narrower width compares with a cast: `data.skill == skill::BLOCK as i8`.
//! A field holds the integer the file holds, so a value no constant names is kept, never
//! mapped to a default.

// ---- flags.rs --------------------------------------------------------------------------

/// `tes3` `ObjectFlags`: the record header flags.
pub mod object {
    pub const MODIFIED: u32 = 0x2;
    pub const DELETED: u32 = 0x20;
    pub const PERSISTENT: u32 = 0x400;
    pub const IGNORED: u32 = 0x1000;
    pub const BLOCKED: u32 = 0x2000;
}

/// `tes3` `LandscapeFlags`.
pub mod landscape {
    pub const USES_VERTEX_HEIGHTS_AND_NORMALS: u32 = 0x1;
    pub const USES_VERTEX_COLORS: u32 = 0x2;
    pub const USES_TEXTURES: u32 = 0x4;
}

/// `tes3` `CellFlags`.
pub mod cell {
    pub const IS_INTERIOR: u32 = 0x1;
    pub const HAS_WATER: u32 = 0x2;
    pub const RESTING_IS_ILLEGAL: u32 = 0x4;
    pub const BEHAVES_LIKE_EXTERIOR: u32 = 0x80;
}

/// `tes3` `LightFlags`.
pub mod light {
    pub const DYNAMIC: u32 = 0x1;
    pub const CAN_CARRY: u32 = 0x2;
    pub const NEGATIVE: u32 = 0x4;
    pub const FLICKER: u32 = 0x8;
    pub const FIRE: u32 = 0x10;
    pub const OFF_BY_DEFAULT: u32 = 0x20;
    pub const FLICKER_SLOW: u32 = 0x40;
    pub const PULSE: u32 = 0x80;
    pub const PULSE_SLOW: u32 = 0x100;
}

/// `tes3` `MiscItemFlags`.
pub mod misc_item {
    pub const KEY: u32 = 0x1;
}

/// `tes3` `AlchemyFlags`.
pub mod alchemy {
    pub const AUTO_CALCULATE: u32 = 0x1;
}

/// `tes3` `EnchantingFlags`.
pub mod enchanting {
    pub const AUTO_CALCULATE: u32 = 0x1;
}

/// `tes3` `SpellFlags`.
pub mod spell {
    pub const AUTO_CALCULATE: u32 = 0x1;
    pub const PC_START_SPELL: u32 = 0x2;
    pub const ALWAYS_SUCCEEDS: u32 = 0x4;
}

/// `tes3` `WeaponFlags`.
pub mod weapon {
    pub const IGNORES_NORMAL_WEAPON_RESISTANCE: u32 = 0x1;
    pub const SILVER: u32 = 0x2;
}

/// `tes3` `ClassFlags`.
pub mod class {
    pub const PLAYABLE: u32 = 0x1;
}

/// `tes3` `FactionFlags`.
pub mod faction {
    pub const HIDDEN_FROM_PC: u32 = 0x1;
}

/// `tes3` `RaceFlags`.
pub mod race {
    pub const PLAYABLE: u32 = 0x1;
    pub const BEAST_RACE: u32 = 0x2;
}

/// `tes3` `MagicEffectFlags`.
pub mod magic_effect {
    pub const TARGET_SKILL: u32 = 0x1;
    pub const TARGET_ATTRIBUTE: u32 = 0x2;
    pub const NO_DURATION: u32 = 0x4;
    pub const NO_MAGNITUDE: u32 = 0x8;
    pub const HARMFUL: u32 = 0x10;
    pub const CONTINUOUS_VFX: u32 = 0x20;
    pub const CAN_CAST_SELF: u32 = 0x40;
    pub const CAN_CAST_TOUCH: u32 = 0x80;
    pub const CAN_CAST_TARGET: u32 = 0x100;
    pub const ALLOW_SPELLMAKING: u32 = 0x200;
    pub const ALLOW_ENCHANTING: u32 = 0x400;
    pub const NEGATIVE_LIGHTING: u32 = 0x800;
    pub const APPLIED_ONCE: u32 = 0x1000;
    pub const UNKNOWN_CHAMELEON: u32 = 0x2000;
    pub const NON_RECASTABLE: u32 = 0x4000;
    pub const ILLEGAL_DAEDRA: u32 = 0x8000;
    pub const UNREFLECTABLE: u32 = 0x10000;
    pub const CASTER_LINKED: u32 = 0x20000;
}

/// `tes3` `ContainerFlags`.
pub mod container {
    pub const ORGANIC: u32 = 0x1;
    pub const RESPAWNS: u32 = 0x2;
    pub const IS_BASE: u32 = 0x8;
}

/// `tes3` `CreatureFlags`, one byte.
pub mod creature {
    pub const BIPED: u8 = 0x1;
    pub const RESPAWN: u8 = 0x2;
    pub const WEAPON_AND_SHIELD: u8 = 0x4;
    pub const IS_BASE: u8 = 0x8;
    pub const SWIMS: u8 = 0x10;
    pub const FLIES: u8 = 0x20;
    pub const WALKS: u8 = 0x40;
    pub const ESSENTIAL: u8 = 0x80;
}

/// `tes3` `NpcFlags`, one byte.
pub mod npc {
    pub const FEMALE: u8 = 0x1;
    pub const ESSENTIAL: u8 = 0x2;
    pub const RESPAWN: u8 = 0x4;
    pub const IS_BASE: u8 = 0x8;
    pub const AUTO_CALCULATE: u8 = 0x10;
}

/// `tes3` `ServiceFlags`.
pub mod service {
    pub const BARTERS_WEAPONS: u32 = 0x1;
    pub const BARTERS_ARMOR: u32 = 0x2;
    pub const BARTERS_CLOTHING: u32 = 0x4;
    pub const BARTERS_BOOKS: u32 = 0x8;
    pub const BARTERS_INGREDIENTS: u32 = 0x10;
    pub const BARTERS_LOCKPICKS: u32 = 0x20;
    pub const BARTERS_PROBES: u32 = 0x40;
    pub const BARTERS_LIGHTS: u32 = 0x80;
    pub const BARTERS_APPARATUS: u32 = 0x100;
    pub const BARTERS_REPAIR_ITEMS: u32 = 0x200;
    pub const BARTERS_MISC_ITEMS: u32 = 0x400;
    pub const OFFERS_SPELLS: u32 = 0x800;
    pub const BARTERS_ENCHANTED_ITEMS: u32 = 0x1000;
    pub const BARTERS_ALCHEMY: u32 = 0x2000;
    pub const OFFERS_TRAINING: u32 = 0x4000;
    pub const OFFERS_SPELLMAKING: u32 = 0x8000;
    pub const OFFERS_ENCHANTING: u32 = 0x10000;
    pub const OFFERS_REPAIRS: u32 = 0x20000;
}

/// `tes3` `LeveledItemFlags`.
pub mod leveled_item {
    pub const CALCULATE_FOR_EACH_ITEM: u32 = 0x1;
    pub const CALCULATE_FROM_ALL_LEVELS: u32 = 0x2;
}

/// `tes3` `LeveledCreatureFlags`.
pub mod leveled_creature {
    pub const CALCULATE_FROM_ALL_LEVELS: u32 = 0x1;
}

/// `tes3` `BodypartFlags`, one byte.
pub mod bodypart {
    pub const FEMALE: u8 = 0x1;
    pub const NOT_PLAYABLE: u8 = 0x2;
}

// ---- enums.rs --------------------------------------------------------------------------

/// `tes3` `FileType` (u32): the header's file kind.
pub mod file_type {
    pub const ESP: u32 = 0;
    pub const ESM: u32 = 1;
    pub const ESS: u32 = 32;
}

/// `tes3` `GlobalType` (u8): the one-byte type letter of a global variable.
pub mod global_type {
    pub const FLOAT: u8 = b'f';
    pub const LONG: u8 = b'l';
    pub const SHORT: u8 = b's';
}

/// `tes3` `AttributeId` (i32) and `AttributeId2` (i8), one set of `i32` constants.
pub mod attribute {
    pub const NONE: i32 = -1;
    pub const STRENGTH: i32 = 0;
    pub const INTELLIGENCE: i32 = 1;
    pub const WILLPOWER: i32 = 2;
    pub const AGILITY: i32 = 3;
    pub const SPEED: i32 = 4;
    pub const ENDURANCE: i32 = 5;
    pub const PERSONALITY: i32 = 6;
    pub const LUCK: i32 = 7;
}

/// `tes3` `Specialization` (i32).
pub mod specialization {
    pub const NONE: i32 = -1;
    pub const COMBAT: i32 = 0;
    pub const MAGIC: i32 = 1;
    pub const STEALTH: i32 = 2;
}

/// `tes3` `SkillId` (i32) and `SkillId2` (i8), one set of `i32` constants.
pub mod skill {
    pub const NONE: i32 = -1;
    pub const BLOCK: i32 = 0;
    pub const ARMORER: i32 = 1;
    pub const MEDIUM_ARMOR: i32 = 2;
    pub const HEAVY_ARMOR: i32 = 3;
    pub const BLUNT_WEAPON: i32 = 4;
    pub const LONG_BLADE: i32 = 5;
    pub const AXE: i32 = 6;
    pub const SPEAR: i32 = 7;
    pub const ATHLETICS: i32 = 8;
    pub const ENCHANT: i32 = 9;
    pub const DESTRUCTION: i32 = 10;
    pub const ALTERATION: i32 = 11;
    pub const ILLUSION: i32 = 12;
    pub const CONJURATION: i32 = 13;
    pub const MYSTICISM: i32 = 14;
    pub const RESTORATION: i32 = 15;
    pub const ALCHEMY: i32 = 16;
    pub const UNARMORED: i32 = 17;
    pub const SECURITY: i32 = 18;
    pub const SNEAK: i32 = 19;
    pub const ACROBATICS: i32 = 20;
    pub const LIGHT_ARMOR: i32 = 21;
    pub const SHORT_BLADE: i32 = 22;
    pub const MARKSMAN: i32 = 23;
    pub const MERCANTILE: i32 = 24;
    pub const SPEECHCRAFT: i32 = 25;
    pub const HAND_TO_HAND: i32 = 26;
}

/// `tes3` `EffectId` (i32) and `EffectId2` (i16), one set of `i32` constants.
pub mod effect {
    pub const NONE: i32 = -1;
    pub const WATER_BREATHING: i32 = 0;
    pub const SWIFT_SWIM: i32 = 1;
    pub const WATER_WALKING: i32 = 2;
    pub const SHIELD: i32 = 3;
    pub const FIRE_SHIELD: i32 = 4;
    pub const LIGHTNING_SHIELD: i32 = 5;
    pub const FROST_SHIELD: i32 = 6;
    pub const BURDEN: i32 = 7;
    pub const FEATHER: i32 = 8;
    pub const JUMP: i32 = 9;
    pub const LEVITATE: i32 = 10;
    pub const SLOW_FALL: i32 = 11;
    pub const LOCK: i32 = 12;
    pub const OPEN: i32 = 13;
    pub const FIRE_DAMAGE: i32 = 14;
    pub const SHOCK_DAMAGE: i32 = 15;
    pub const FROST_DAMAGE: i32 = 16;
    pub const DRAIN_ATTRIBUTE: i32 = 17;
    pub const DRAIN_HEALTH: i32 = 18;
    pub const DRAIN_MAGICKA: i32 = 19;
    pub const DRAIN_FATIGUE: i32 = 20;
    pub const DRAIN_SKILL: i32 = 21;
    pub const DAMAGE_ATTRIBUTE: i32 = 22;
    pub const DAMAGE_HEALTH: i32 = 23;
    pub const DAMAGE_MAGICKA: i32 = 24;
    pub const DAMAGE_FATIGUE: i32 = 25;
    pub const DAMAGE_SKILL: i32 = 26;
    pub const POISON: i32 = 27;
    pub const WEAKNESS_TO_FIRE: i32 = 28;
    pub const WEAKNESS_TO_FROST: i32 = 29;
    pub const WEAKNESS_TO_SHOCK: i32 = 30;
    pub const WEAKNESS_TO_MAGICKA: i32 = 31;
    pub const WEAKNESS_TO_COMMON_DISEASE: i32 = 32;
    pub const WEAKNESS_TO_BLIGHT_DISEASE: i32 = 33;
    pub const WEAKNESS_TO_CORPRUS: i32 = 34;
    pub const WEAKNESS_TO_POISON: i32 = 35;
    pub const WEAKNESS_TO_NORMAL_WEAPONS: i32 = 36;
    pub const DISINTEGRATE_WEAPON: i32 = 37;
    pub const DISINTEGRATE_ARMOR: i32 = 38;
    pub const INVISIBILITY: i32 = 39;
    pub const CHAMELEON: i32 = 40;
    pub const LIGHT: i32 = 41;
    pub const SANCTUARY: i32 = 42;
    pub const NIGHT_EYE: i32 = 43;
    pub const CHARM: i32 = 44;
    pub const PARALYZE: i32 = 45;
    pub const SILENCE: i32 = 46;
    pub const BLIND: i32 = 47;
    pub const SOUND: i32 = 48;
    pub const CALM_HUMANOID: i32 = 49;
    pub const CALM_CREATURE: i32 = 50;
    pub const FRENZY_HUMANOID: i32 = 51;
    pub const FRENZY_CREATURE: i32 = 52;
    pub const DEMORALIZE_HUMANOID: i32 = 53;
    pub const DEMORALIZE_CREATURE: i32 = 54;
    pub const RALLY_HUMANOID: i32 = 55;
    pub const RALLY_CREATURE: i32 = 56;
    pub const DISPEL: i32 = 57;
    pub const SOUL_TRAP: i32 = 58;
    pub const TELEKINESIS: i32 = 59;
    pub const MARK: i32 = 60;
    pub const RECALL: i32 = 61;
    pub const DIVINE_INTERVENTION: i32 = 62;
    pub const ALMSIVI_INTERVENTION: i32 = 63;
    pub const DETECT_ANIMAL: i32 = 64;
    pub const DETECT_ENCHANTMENT: i32 = 65;
    pub const DETECT_KEY: i32 = 66;
    pub const SPELL_ABSORPTION: i32 = 67;
    pub const REFLECT: i32 = 68;
    pub const CURE_COMMON_DISEASE: i32 = 69;
    pub const CURE_BLIGHT_DISEASE: i32 = 70;
    pub const CURE_CORPRUS: i32 = 71;
    pub const CURE_POISON: i32 = 72;
    pub const CURE_PARALYZATION: i32 = 73;
    pub const RESTORE_ATTRIBUTE: i32 = 74;
    pub const RESTORE_HEALTH: i32 = 75;
    pub const RESTORE_MAGICKA: i32 = 76;
    pub const RESTORE_FATIGUE: i32 = 77;
    pub const RESTORE_SKILL: i32 = 78;
    pub const FORTIFY_ATTRIBUTE: i32 = 79;
    pub const FORTIFY_HEALTH: i32 = 80;
    pub const FORTIFY_MAGICKA: i32 = 81;
    pub const FORTIFY_FATIGUE: i32 = 82;
    pub const FORTIFY_SKILL: i32 = 83;
    pub const FORTIFY_MAGICKA_MULTIPLIER: i32 = 84;
    pub const ABSORB_ATTRIBUTE: i32 = 85;
    pub const ABSORB_HEALTH: i32 = 86;
    pub const ABSORB_MAGICKA: i32 = 87;
    pub const ABSORB_FATIGUE: i32 = 88;
    pub const ABSORB_SKILL: i32 = 89;
    pub const RESIST_FIRE: i32 = 90;
    pub const RESIST_FROST: i32 = 91;
    pub const RESIST_SHOCK: i32 = 92;
    pub const RESIST_MAGICKA: i32 = 93;
    pub const RESIST_COMMON_DISEASE: i32 = 94;
    pub const RESIST_BLIGHT_DISEASE: i32 = 95;
    pub const RESIST_CORPRUS: i32 = 96;
    pub const RESIST_POISON: i32 = 97;
    pub const RESIST_NORMAL_WEAPONS: i32 = 98;
    pub const RESIST_PARALYSIS: i32 = 99;
    pub const REMOVE_CURSE: i32 = 100;
    pub const TURN_UNDEAD: i32 = 101;
    pub const SUMMON_SCAMP: i32 = 102;
    pub const SUMMON_CLANNFEAR: i32 = 103;
    pub const SUMMON_DAEDROTH: i32 = 104;
    pub const SUMMON_DREMORA: i32 = 105;
    pub const SUMMON_GHOST: i32 = 106;
    pub const SUMMON_SKELETON: i32 = 107;
    pub const SUMMON_LEAST_BONEWALKER: i32 = 108;
    pub const SUMMON_GREATER_BONEWALKER: i32 = 109;
    pub const SUMMON_BONELORD: i32 = 110;
    pub const SUMMON_TWILIGHT: i32 = 111;
    pub const SUMMON_HUNGER: i32 = 112;
    pub const SUMMON_GOLDEN_SAINT: i32 = 113;
    pub const SUMMON_FLAME_ATRONACH: i32 = 114;
    pub const SUMMON_FROST_ATRONACH: i32 = 115;
    pub const SUMMON_STORM_ATRONACH: i32 = 116;
    pub const FORTIFY_ATTACK_BONUS: i32 = 117;
    pub const COMMAND_CREATURE: i32 = 118;
    pub const COMMAND_HUMANOID: i32 = 119;
    pub const BOUND_DAGGER: i32 = 120;
    pub const BOUND_LONGSWORD: i32 = 121;
    pub const BOUND_MACE: i32 = 122;
    pub const BOUND_BATTLE_AXE: i32 = 123;
    pub const BOUND_SPEAR: i32 = 124;
    pub const BOUND_LONGBOW: i32 = 125;
    pub const EXTRA_SPELL: i32 = 126;
    pub const BOUND_CUIRASS: i32 = 127;
    pub const BOUND_HELM: i32 = 128;
    pub const BOUND_BOOTS: i32 = 129;
    pub const BOUND_SHIELD: i32 = 130;
    pub const BOUND_GLOVES: i32 = 131;
    pub const CORPRUS: i32 = 132;
    pub const VAMPIRISM: i32 = 133;
    pub const SUMMON_CENTURION_SPHERE: i32 = 134;
    pub const SUN_DAMAGE: i32 = 135;
    pub const STUNTED_MAGICKA: i32 = 136;
    pub const SUMMON_FABRICANT: i32 = 137;
    pub const SUMMON_WOLF: i32 = 138;
    pub const SUMMON_BEAR: i32 = 139;
    pub const SUMMON_BONE_WOLF: i32 = 140;
    pub const SUMMON_04: i32 = 141;
    pub const SUMMON_05: i32 = 142;
}

/// `tes3` `EffectRange` (u32).
pub mod effect_range {
    pub const ON_SELF: u32 = 0;
    pub const ON_TOUCH: u32 = 1;
    pub const ON_TARGET: u32 = 2;
}

/// `tes3` `EffectSchool` (u32).
pub mod effect_school {
    pub const ALTERATION: u32 = 0;
    pub const CONJURATION: u32 = 1;
    pub const DESTRUCTION: u32 = 2;
    pub const ILLUSION: u32 = 3;
    pub const MYSTICISM: u32 = 4;
    pub const RESTORATION: u32 = 5;
}

/// `tes3` `ApparatusType` (u32).
pub mod apparatus_type {
    pub const MORTAR_AND_PESTLE: u32 = 0;
    pub const ALEMBIC: u32 = 1;
    pub const CALCINATOR: u32 = 2;
    pub const RETORT: u32 = 3;
}

/// `tes3` `ArmorType` (u32).
pub mod armor_type {
    pub const HELMET: u32 = 0;
    pub const CUIRASS: u32 = 1;
    pub const LEFT_PAULDRON: u32 = 2;
    pub const RIGHT_PAULDRON: u32 = 3;
    pub const GREAVES: u32 = 4;
    pub const BOOTS: u32 = 5;
    pub const LEFT_GAUNTLET: u32 = 6;
    pub const RIGHT_GAUNTLET: u32 = 7;
    pub const SHIELD: u32 = 8;
    pub const LEFT_BRACER: u32 = 9;
    pub const RIGHT_BRACER: u32 = 10;
}

/// `tes3` `BookType` (u32).
pub mod book_type {
    pub const BOOK: u32 = 0;
    pub const SCROLL: u32 = 1;
}

/// `tes3` `ClothingType` (u32).
pub mod clothing_type {
    pub const PANTS: u32 = 0;
    pub const SHOES: u32 = 1;
    pub const SHIRT: u32 = 2;
    pub const BELT: u32 = 3;
    pub const ROBE: u32 = 4;
    pub const RIGHT_GLOVE: u32 = 5;
    pub const LEFT_GLOVE: u32 = 6;
    pub const SKIRT: u32 = 7;
    pub const RING: u32 = 8;
    pub const AMULET: u32 = 9;
}

/// `tes3` `CreatureType` (u32).
pub mod creature_type {
    pub const NORMAL: u32 = 0;
    pub const DAEDRA: u32 = 1;
    pub const UNDEAD: u32 = 2;
    pub const HUMANOID: u32 = 3;
}

/// `tes3` `WeaponType` (u16).
pub mod weapon_type {
    pub const SHORT_BLADE_ONE_HAND: u16 = 0;
    pub const LONG_BLADE_ONE_HAND: u16 = 1;
    pub const LONG_BLADE_TWO_CLOSE: u16 = 2;
    pub const BLUNT_ONE_HAND: u16 = 3;
    pub const BLUNT_TWO_CLOSE: u16 = 4;
    pub const BLUNT_TWO_WIDE: u16 = 5;
    pub const SPEAR_TWO_WIDE: u16 = 6;
    pub const AXE_ONE_HAND: u16 = 7;
    pub const AXE_TWO_HAND: u16 = 8;
    pub const MARKSMAN_BOW: u16 = 9;
    pub const MARKSMAN_CROSSBOW: u16 = 10;
    pub const MARKSMAN_THROWN: u16 = 11;
    pub const ARROW: u16 = 12;
    pub const BOLT: u16 = 13;
}

/// `tes3` `SpellType` (u32).
pub mod spell_type {
    pub const SPELL: u32 = 0;
    pub const ABILITY: u32 = 1;
    pub const BLIGHT: u32 = 2;
    pub const DISEASE: u32 = 3;
    pub const CURSE: u32 = 4;
    pub const POWER: u32 = 5;
}

/// `tes3` `EnchantType` (u32).
pub mod enchant_type {
    pub const CAST_ONCE: u32 = 0;
    pub const CAST_ON_STRIKE: u32 = 1;
    pub const CAST_WHEN_USED: u32 = 2;
    pub const CONSTANT_EFFECT: u32 = 3;
}

/// `tes3` `BodypartId` (u8).
pub mod bodypart_id {
    pub const HEAD: u8 = 0;
    pub const HAIR: u8 = 1;
    pub const NECK: u8 = 2;
    pub const CHEST: u8 = 3;
    pub const GROIN: u8 = 4;
    pub const HAND: u8 = 5;
    pub const WRIST: u8 = 6;
    pub const FOREARM: u8 = 7;
    pub const UPPER_ARM: u8 = 8;
    pub const FOOT: u8 = 9;
    pub const ANKLE: u8 = 10;
    pub const KNEE: u8 = 11;
    pub const UPPER_LEG: u8 = 12;
    pub const CLAVICLE: u8 = 13;
    pub const TAIL: u8 = 14;
}

/// `tes3` `BodypartType` (u8).
pub mod bodypart_type {
    pub const SKIN: u8 = 0;
    pub const CLOTHING: u8 = 1;
    pub const ARMOR: u8 = 2;
}

/// `tes3` `BipedObjectType` (u8).
pub mod biped_object_type {
    pub const HEAD: u8 = 0;
    pub const HAIR: u8 = 1;
    pub const NECK: u8 = 2;
    pub const CHEST: u8 = 3;
    pub const GROIN: u8 = 4;
    pub const SKIRT: u8 = 5;
    pub const RIGHT_HAND: u8 = 6;
    pub const LEFT_HAND: u8 = 7;
    pub const RIGHT_WRIST: u8 = 8;
    pub const LEFT_WRIST: u8 = 9;
    pub const SHIELD: u8 = 10;
    pub const RIGHT_FOREARM: u8 = 11;
    pub const LEFT_FOREARM: u8 = 12;
    pub const RIGHT_UPPER_ARM: u8 = 13;
    pub const LEFT_UPPER_ARM: u8 = 14;
    pub const RIGHT_FOOT: u8 = 15;
    pub const LEFT_FOOT: u8 = 16;
    pub const RIGHT_ANKLE: u8 = 17;
    pub const LEFT_ANKLE: u8 = 18;
    pub const RIGHT_KNEE: u8 = 19;
    pub const LEFT_KNEE: u8 = 20;
    pub const RIGHT_UPPER_LEG: u8 = 21;
    pub const LEFT_UPPER_LEG: u8 = 22;
    pub const RIGHT_PAULDRON: u8 = 23;
    pub const LEFT_PAULDRON: u8 = 24;
    pub const WEAPON: u8 = 25;
    pub const TAIL: u8 = 26;
}

/// `tes3` `Sex` (i8).
pub mod sex {
    pub const ANY: i8 = -1;
    pub const MALE: i8 = 0;
    pub const FEMALE: i8 = 1;
}

/// `tes3` `DialogueType` (u32) and `DialogueType2` (u8), one set of `i32` constants.
pub mod dialogue_type {
    pub const TOPIC: i32 = 0;
    pub const VOICE: i32 = 1;
    pub const GREETING: i32 = 2;
    pub const PERSUASION: i32 = 3;
    pub const JOURNAL: i32 = 4;
}

/// `tes3` `FilterType` (u8): the first byte of a dialogue filter's `SCVR`, a digit or letter.
pub mod filter_type {
    pub const NONE: u8 = b'0';
    pub const FUNCTION: u8 = b'1';
    pub const GLOBAL: u8 = b'2';
    pub const LOCAL: u8 = b'3';
    pub const JOURNAL: u8 = b'4';
    pub const ITEM: u8 = b'5';
    pub const DEAD: u8 = b'6';
    pub const NOT_ID: u8 = b'7';
    pub const NOT_FACTION: u8 = b'8';
    pub const NOT_CLASS: u8 = b'9';
    pub const NOT_RACE: u8 = b'A';
    pub const NOT_CELL: u8 = b'B';
    pub const NOT_LOCAL: u8 = b'C';
}

/// `tes3` `FilterFunction` (u16): two text bytes of a dialogue filter's `SCVR` read as one
/// little-endian word, so `"00"` is 0x3030 and `"CX"` is 0x5843.
pub mod filter_function {
    pub const REACTION_LOW: u16 = 12336; // "00"
    pub const REACTION_HIGH: u16 = 12592; // "01"
    pub const RANK_REQUIREMENT: u16 = 12848; // "02"
    pub const REPUTATION: u16 = 13104; // "03"
    pub const HEALTH_PERCENT: u16 = 13360; // "04"
    pub const PC_REPUTATION: u16 = 13616; // "05"
    pub const PC_LEVEL: u16 = 13872; // "06"
    pub const PC_HEALTH_PERCENT: u16 = 14128; // "07"
    pub const PC_MAGICKA: u16 = 14384; // "08"
    pub const PC_FATIGUE: u16 = 14640; // "09"
    pub const PC_STRENGTH: u16 = 12337; // "10"
    pub const PC_BLOCK: u16 = 12593; // "11"
    pub const PC_ARMORER: u16 = 12849; // "12"
    pub const PC_MEDIUM_ARMOR: u16 = 13105; // "13"
    pub const PC_HEAVY_ARMOR: u16 = 13361; // "14"
    pub const PC_BLUNT_WEAPON: u16 = 13617; // "15"
    pub const PC_LONG_BLADE: u16 = 13873; // "16"
    pub const PC_AXE: u16 = 14129; // "17"
    pub const PC_SPEAR: u16 = 14385; // "18"
    pub const PC_ATHLETICS: u16 = 14641; // "19"
    pub const PC_ENCHANT: u16 = 12338; // "20"
    pub const PC_DESTRUCTION: u16 = 12594; // "21"
    pub const PC_ALTERATION: u16 = 12850; // "22"
    pub const PC_ILLUSION: u16 = 13106; // "23"
    pub const PC_CONJURATION: u16 = 13362; // "24"
    pub const PC_MYSTICISM: u16 = 13618; // "25"
    pub const PC_RESTORATION: u16 = 13874; // "26"
    pub const PC_ALCHEMY: u16 = 14130; // "27"
    pub const PC_UNARMORED: u16 = 14386; // "28"
    pub const PC_SECURITY: u16 = 14642; // "29"
    pub const PC_SNEAK: u16 = 12339; // "30"
    pub const PC_ACROBATICS: u16 = 12595; // "31"
    pub const PC_LIGHT_ARMOR: u16 = 12851; // "32"
    pub const PC_SHORT_BLADE: u16 = 13107; // "33"
    pub const PC_MARKSMAN: u16 = 13363; // "34"
    pub const PC_MERCANTILE: u16 = 13619; // "35"
    pub const PC_SPEECHCRAFT: u16 = 13875; // "36"
    pub const PC_HAND_TO_HAND: u16 = 14131; // "37"
    pub const PC_SEX: u16 = 14387; // "38"
    pub const PC_EXPELLED: u16 = 14643; // "39"
    pub const PC_COMMON_DISEASE: u16 = 12340; // "40"
    pub const PC_BLIGHT_DISEASE: u16 = 12596; // "41"
    pub const PC_CLOTHING_MODIFIER: u16 = 12852; // "42"
    pub const PC_CRIME_LEVEL: u16 = 13108; // "43"
    pub const SAME_SEX: u16 = 13364; // "44"
    pub const SAME_RACE: u16 = 13620; // "45"
    pub const SAME_FACTION: u16 = 13876; // "46"
    pub const FACTION_RANK_DIFFERENCE: u16 = 14132; // "47"
    pub const DETECTED: u16 = 14388; // "48"
    pub const ALARMED: u16 = 14644; // "49"
    pub const CHOICE: u16 = 12341; // "50"
    pub const PC_INTELLIGENCE: u16 = 12597; // "51"
    pub const PC_WILLPOWER: u16 = 12853; // "52"
    pub const PC_AGILITY: u16 = 13109; // "53"
    pub const PC_SPEED: u16 = 13365; // "54"
    pub const PC_ENDURANCE: u16 = 13621; // "55"
    pub const PC_PERSONALITY: u16 = 13877; // "56"
    pub const PC_LUCK: u16 = 14133; // "57"
    pub const PC_CORPRUS: u16 = 14389; // "58"
    pub const WEATHER: u16 = 14645; // "59"
    pub const PC_VAMPIRE: u16 = 12342; // "60"
    pub const LEVEL: u16 = 12598; // "61"
    pub const ATTACKED: u16 = 12854; // "62"
    pub const TALKED_TO_PC: u16 = 13110; // "63"
    pub const PC_HEALTH: u16 = 13366; // "64"
    pub const CREATURE_TARGET: u16 = 13622; // "65"
    pub const FRIEND_HIT: u16 = 13878; // "66"
    pub const FIGHT: u16 = 14134; // "67"
    pub const HELLO: u16 = 14390; // "68"
    pub const ALARM: u16 = 14646; // "69"
    pub const FLEE: u16 = 12343; // "70"
    pub const SHOULD_ATTACK: u16 = 12599; // "71"
    pub const WEREWOLF: u16 = 12855; // "72"
    pub const WEREWOLF_KILLS: u16 = 13111; // "73"
    pub const NOT_CLASS: u16 = 22595; // "CX"
    pub const DEAD_TYPE: u16 = 22596; // "DX"
    pub const NOT_FACTION: u16 = 22598; // "FX"
    pub const ITEM_TYPE: u16 = 22601; // "IX"
    pub const JOURNAL_TYPE: u16 = 22602; // "JX"
    pub const NOT_CELL: u16 = 22604; // "LX"
    pub const NOT_RACE: u16 = 22610; // "RX"
    pub const NOT_ID_TYPE: u16 = 22616; // "XX"
    pub const GLOBAL: u16 = 22630; // "fX"
    pub const PC_GOLD: u16 = 22636; // "lX"
    pub const COMPARE_GLOBAL: u16 = 22578; // "2X"
    pub const COMPARE_LOCAL: u16 = 22579; // "3X"
    pub const VARIABLE_COMPARE: u16 = 22643; // "sX"
}

/// `tes3` `FilterComparison` (u8): the comparison byte of a dialogue filter's `SCVR`.
pub mod filter_comparison {
    pub const EQUAL: u8 = b'0';
    pub const NOT_EQUAL: u8 = b'1';
    pub const GREATER: u8 = b'2';
    pub const GREATER_EQUAL: u8 = b'3';
    pub const LESS: u8 = b'4';
    pub const LESS_EQUAL: u8 = b'5';
}

/// `tes3` `SoundGenType` (u32).
pub mod soundgen_type {
    pub const LEFT_FOOT: u32 = 0;
    pub const RIGHT_FOOT: u32 = 1;
    pub const SWIM_LEFT: u32 = 2;
    pub const SWIM_RIGHT: u32 = 3;
    pub const MOAN: u32 = 4;
    pub const ROAR: u32 = 5;
    pub const SCREAM: u32 = 6;
    pub const LAND: u32 = 7;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_byte_filter_functions_are_their_text_read_little_endian() {
        assert_eq!(filter_function::REACTION_LOW, u16::from_le_bytes(*b"00"));
        assert_eq!(filter_function::WEREWOLF_KILLS, u16::from_le_bytes(*b"73"));
        assert_eq!(filter_function::NOT_CLASS, u16::from_le_bytes(*b"CX"));
        assert_eq!(filter_function::VARIABLE_COMPARE, u16::from_le_bytes(*b"sX"));
        assert_eq!(skill::HAND_TO_HAND, 26);
        assert_eq!(effect::SUMMON_05, 142);
        assert_eq!(object::DELETED, super::super::super::raw::FLAG_DELETED);
    }
}
