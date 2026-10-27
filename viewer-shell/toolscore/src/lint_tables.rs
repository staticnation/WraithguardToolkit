//! The lint's tables (moved from `wraithguard_toolkit.py`): the evil GMSTs, the expansions'
//! script functions, the files and cells left alone.
//!
//! The "72 evil GMSTs" are settings an old Construction Set copied into a plugin whenever it
//! was used without both expansions loaded; legitimate only inside Tribunal.esm and
//! Bloodmoon.esm. A GMST is flagged only when both its name and its recorded value match (a
//! mod deliberately changing one of these settings is fine). The name/value pairs and the
//! expansion function lists are reproduced from tes3lint:
//!
//!   tes3lint - a diagnostic tool for TES3/Morrowind plugins
//!   Copyright 2009 by John Moonsugar
//!   Distributed as part of the mlox project under the MIT License.
//!
//! Retained here to satisfy the MIT notice requirement: these tables are research (which
//! values the buggy CS actually wrote), not something we rediscovered.

/// A setting's lower-cased name, its value's subrecord and the stale expansion default it
/// carries (trailing NULs left off when compared).
pub const EVIL_GMSTS: &[(&str, &[u8; 4], &[u8])] = &[
    ("fcombatdistancewerewolfmod", b"FLTV", b"\x9a\x99\x99>"),
    ("ffleedistance", b"FLTV", b"\x00\x80;E"),
    ("fwerewolfacrobatics", b"FLTV", b"\x00\x00\x16C"),
    ("fwerewolfagility", b"FLTV", b"\x00\x00\x16C"),
    ("fwerewolfalchemy", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfalteration", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfarmorer", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfathletics", b"FLTV", b"\x00\x00\x16C"),
    ("fwerewolfaxe", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfblock", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfbluntweapon", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfconjuration", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfdestruction", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfenchant", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfendurance", b"FLTV", b"\x00\x00\x16C"),
    ("fwerewolffatigue", b"FLTV", b"\x00\x00\xc8C"),
    ("fwerewolfhandtohand", b"FLTV", b"\x00\x00\xc8B"),
    ("fwerewolfhealth", b"FLTV", b"\x00\x00\x00@"),
    ("fwerewolfheavyarmor", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfillusion", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfintellegence", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolflightarmor", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolflongblade", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfluck", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfmagicka", b"FLTV", b"\x00\x00\xc8B"),
    ("fwerewolfmarksman", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfmediumarmor", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfmerchantile", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfmysticism", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfpersonality", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfrestoration", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfrunmult", b"FLTV", b"\x00\x00\xc0?"),
    ("fwerewolfsecurity", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfshortblade", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfsilverweapondamagemult", b"FLTV", b"\x00\x00\xc0?"),
    ("fwerewolfsneak", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfspear", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfspeechcraft", b"FLTV", b"\x00\x00\x80?"),
    ("fwerewolfspeed", b"FLTV", b"\x00\x00\x16C"),
    ("fwerewolfstrength", b"FLTV", b"\x00\x00\x16C"),
    ("fwerewolfunarmored", b"FLTV", b"\x00\x00\xc8B"),
    ("fwerewolfwillpower", b"FLTV", b"\x00\x00\x80?"),
    ("iwerewolfbounty", b"INTV", b"\x10'\x00\x00"),
    ("iwerewolffightmod", b"INTV", b"d\x00\x00\x00"),
    ("iwerewolffleemod", b"INTV", b"d\x00\x00\x00"),
    ("iwerewolfleveltoattack", b"INTV", b"\x14\x00\x00\x00"),
    ("scompanionshare", b"STRV", b"Companion Share"),
    ("scompanionwarningbuttonone", b"STRV", b"Let the mercenary quit."),
    ("scompanionwarningbuttontwo", b"STRV", b"Return to Companion Share display."),
    ("scompanionwarningmessage", b"STRV", b"Your mercenary is poorer now than when he contracted with you.  Your mercenary will quit if you do not give him gold or goods to bring his Profit Value to a positive value."),
    ("sdeletenote", b"STRV", b"Delete Note?"),
    ("seditnote", b"STRV", b"Edit Note"),
    ("seffectsummoncreature01", b"STRV", b"sEffectSummonCreature01"),
    ("seffectsummoncreature02", b"STRV", b"sEffectSummonCreature02"),
    ("seffectsummoncreature03", b"STRV", b"sEffectSummonCreature03"),
    ("seffectsummoncreature04", b"STRV", b"sEffectSummonCreature04"),
    ("seffectsummoncreature05", b"STRV", b"sEffectSummonCreature05"),
    ("seffectsummonfabricant", b"STRV", b"sEffectSummonFabricant"),
    ("slevitatedisabled", b"STRV", b"Levitation magic does not work here."),
    ("smagiccreature01id", b"STRV", b"sMagicCreature01ID"),
    ("smagiccreature02id", b"STRV", b"sMagicCreature02ID"),
    ("smagiccreature03id", b"STRV", b"sMagicCreature03ID"),
    ("smagiccreature04id", b"STRV", b"sMagicCreature04ID"),
    ("smagiccreature05id", b"STRV", b"sMagicCreature05ID"),
    ("smagicfabricantid", b"STRV", b"Fabricant"),
    ("smaxsale", b"STRV", b"Max Sale"),
    ("sprofitvalue", b"STRV", b"Profit Value"),
    ("steleportdisabled", b"STRV", b"Teleportation magic does not work here."),
    ("swerewolfalarmmessage", b"STRV", b"You have been detected changing from a werewolf state."),
    ("swerewolfpopup", b"STRV", b"Werewolf"),
    ("swerewolfrefusal", b"STRV", b"You cannot do this as a werewolf."),
    ("swerewolfrestmessage", b"STRV", b"You cannot rest in werewolf form."),
];

/// Script functions Tribunal introduced.
pub const TRIBUNAL_FUNCS: &[&str] = &["AddToLevCreature", "AddToLevItem", "ClearForceJump", "ClearForceMoveJump", "ClearForceRun", "DisableLevitation", "EnableLevitation", "ExplodeSpell", "ForceJump", "ForceMoveJump", "ForceRun", "GetCollidingActor", "GetCollidingPC", "GetForceJump", "GetForceMoveJump", "GetForceRun", "GetPCJumping", "GetPCRunning", "GetPCSneaking", "GetScale", "GetSpellReadied", "GetSquareRoot", "GetWaterLevel", "GetWeaponDrawn", "GetWeaponType", "HasItemEquipped", "ModScale", "ModWaterLevel", "PlaceItem", "PlaceItemCell", "RemoveFromLevCreature", "RemoveFromLevItem", "SetDelete", "SetScale", "SetWaterLevel"];

/// Script functions Bloodmoon introduced.
pub const BLOODMOON_FUNCS: &[&str] = &["BecomeWerewolf", "GetPCInJail", "GetPCTraveling", "GetWerewolfKills", "IsWerewolf", "PlaceAtMe", "SetWerewolfAcrobatics", "TurnMoonRed", "TurnMoonWhite", "UndoWerewolf"];

/// Files the lint leaves alone (the base game and merged-file artifacts), lower case.
pub const SKIP: &[&str] = &["bloodmoon.esm", "mashed lists.esp", "merged lands.esp", "merged objects.esp", "morrowind.esm", "multipatch.esp", "tribunal.esm"];

/// Cells left out of the path grid check (the classic exterior exception), lower case.
pub const SKIP_CELLS: &[&str] = &["ashlands region (0, 0)"];
