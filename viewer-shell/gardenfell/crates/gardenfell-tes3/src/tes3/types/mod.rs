//! One struct per record type, read from a [`Record`] and written back to one.
//!
//! Round 18gg. The layouts are `tes3`'s (`libs/esp/src/types/*.rs`), transcribed field
//! for field with these standing differences, all of them chosen so the typed form
//! holds *more* of the file than `tes3` does:
//!
//! - Enumerations are the integer the file holds, with the known values as named
//!   constants in [`consts`]. `tes3` maps an unknown value to the enum's default, which
//!   is one way a file can change on the way through; an integer cannot.
//! - Flag words are plain integers with named bits in [`consts`], for the same reason.
//! - Strings are [`String`]s decoded from Windows-1252 up to the first NUL, and written
//!   back with one NUL, as the Construction Set writes them. A string the file stored
//!   without its NUL, or with bytes after it, is the one place a typed round trip is not
//!   byte-exact, and the `roundtrip` command counts those.
//! - The record header's second dword, which no reader uses, is dropped (as `tes3` does),
//!   and written back as zero.
//! - Where `tes3` writes something the file did not have, or drops something it had, the
//!   file wins here: a `LAND` block is held by presence and not by the `DATA` bits, a
//!   `GMST` with no value stays without one, an actor without `AIDT` stays without, a
//!   Morrowind-only `WEAT` stays 8 bytes, a reference's `XSCL` of 1.0 and `NAM9` of 1
//!   are written when the file had them, an `NPC_` or `CREA` `FLAG` word keeps every
//!   bit, and a cell's references stay in file order (`Cell::sort_references` gives
//!   `tes3`'s order). Each of these is noted where it lives. Round 18gg measured the
//!   result over the game's three masters: every record types and reads its own output
//!   back to itself, and 55 to 69 percent come back byte for byte, the rest being
//!   strings the Construction Set stored without a NUL, uninitialized padding bytes, and
//!   `NAM0` counts that never matched the references after them (see `esp-format.md`).
//!
//! Reading is strict: a subrecord the layout does not name, or one of the wrong size,
//! is an error naming the record, the subrecord and the byte, never a silent skip. A
//! caller that wants the lenient reading uses the [`raw`](super::raw) layer, which keeps
//! everything, and reads the types it understands from there.
//!
//! The per-record files are named for the record struct (`static_.rs` for [`Static`],
//! `npc.rs` for [`Npc`]); the shared pieces several records carry (`Effect`, `AiData`,
//! the AI packages, inventory lists) have their own files.

use super::bytes::{decode, Error, Reader, Result, Writer};
use super::raw::{Record, Sub, Tag, FLAG_DELETED};

pub mod consts;

mod activator;
mod aidata;
mod aipackage;
mod alchemy;
mod apparatus;
mod armor;
mod bipedobject;
mod birthsign;
mod bodypart;
mod book;
mod cell;
mod class;
mod clothing;
mod container;
mod creature;
mod dialogue;
mod dialogueinfo;
mod door;
mod effect;
mod enchanting;
mod faction;
mod gamesetting;
mod globalvariable;
mod header;
mod ingredient;
mod landscape;
mod landscapetexture;
mod leveledcreature;
mod leveleditem;
mod light;
mod lockpick;
mod magiceffect;
mod miscitem;
mod npc;
mod pathgrid;
mod probe;
mod race;
mod reference;
mod region;
mod repairitem;
mod script;
mod skill;
mod sound;
mod soundgen;
mod spell;
mod startscript;
mod static_;
mod weapon;

pub use activator::*;
pub use aidata::*;
pub use aipackage::*;
pub use alchemy::*;
pub use apparatus::*;
pub use armor::*;
pub use bipedobject::*;
pub use birthsign::*;
pub use bodypart::*;
pub use book::*;
pub use cell::*;
pub use class::*;
pub use clothing::*;
pub use container::*;
pub use creature::*;
pub use dialogue::*;
pub use dialogueinfo::*;
pub use door::*;
pub use effect::*;
pub use enchanting::*;
pub use faction::*;
pub use gamesetting::*;
pub use globalvariable::*;
pub use header::*;
pub use ingredient::*;
pub use landscape::*;
pub use landscapetexture::*;
pub use leveledcreature::*;
pub use leveleditem::*;
pub use light::*;
pub use lockpick::*;
pub use magiceffect::*;
pub use miscitem::*;
pub use npc::*;
pub use pathgrid::*;
pub use probe::*;
pub use race::*;
pub use reference::*;
pub use region::*;
pub use repairitem::*;
pub use script::*;
pub use skill::*;
pub use sound::*;
pub use soundgen::*;
pub use spell::*;
pub use startscript::*;
pub use static_::*;
pub use weapon::*;

/// A record type: its tag, and the two directions.
pub trait Typed: Sized + Default {
    const TAG: Tag;
    /// The typed form of a record. An error names what did not fit.
    fn from_raw(rec: &Record) -> Result<Self>;
    /// The record back, in the canonical subrecord order with the header flags set.
    fn to_raw(&self) -> Result<Record<'static>>;
}

// ---- reading -------------------------------------------------------------------------

/// A cursor over a record's subrecords, for layouts that read them in sequence (a `FNAM`
/// that must be followed by its `FLTV`, a reference's run of subrecords after `FRMR`).
pub struct Subs<'a> {
    pub rec: &'a Record<'a>,
    pub at: usize,
}

impl<'a> Subs<'a> {
    pub fn new(rec: &'a Record) -> Subs<'a> {
        Subs { rec, at: 0 }
    }
    /// The next subrecord, if any, advancing past it.
    pub fn next(&mut self) -> Option<&'a Sub<'a>> {
        let s = self.rec.subs.get(self.at);
        if s.is_some() {
            self.at += 1;
        }
        s
    }
    /// The next subrecord without advancing.
    pub fn peek(&self) -> Option<&'a Sub<'a>> {
        self.rec.subs.get(self.at)
    }
    /// The next subrecord's tag, if any.
    pub fn peek_tag(&self) -> Option<&'a Tag> {
        self.peek().map(|s| &s.tag)
    }
    /// The next subrecord, which must carry this tag.
    pub fn expect(&mut self, tag: &Tag) -> Result<&'a Sub<'a>> {
        match self.next() {
            Some(s) if &s.tag == tag => Ok(s),
            Some(s) => Err(unexpected_after(self.rec, s, tag)),
            None => Err(Error {
                at: 0,
                what: format!(
                    "{}: ended where {} was expected",
                    self.rec.tag_str(),
                    super::raw::tag_str(tag)
                ),
            }),
        }
    }
    /// Steps back one subrecord, for a loop that read one too many.
    pub fn back(&mut self) {
        self.at = self.at.saturating_sub(1);
    }
}

/// The error for a subrecord the layout does not name.
pub fn unexpected(rec: &Record, s: &Sub) -> Error {
    Error { at: offset_of(rec, s), what: format!("{}::{} is not in the layout", rec.tag_str(), s.tag_str()) }
}

/// The error for a subrecord that is not the one a sequence needed next.
pub fn unexpected_after(rec: &Record, s: &Sub, wanted: &Tag) -> Error {
    Error {
        at: offset_of(rec, s),
        what: format!(
            "{}::{} where {} was expected",
            rec.tag_str(),
            s.tag_str(),
            super::raw::tag_str(wanted)
        ),
    }
}

/// The record's tag must be the type's.
pub fn check_tag(rec: &Record, want: &Tag) -> Result<()> {
    if &rec.tag != want {
        return Err(Error {
            at: 0,
            what: format!("a {} record read as {}", rec.tag_str(), super::raw::tag_str(want)),
        });
    }
    Ok(())
}

/// The byte offset of a subrecord's data within the record body (header excluded), for
/// error messages.
fn offset_of(rec: &Record, s: &Sub) -> usize {
    let mut at = 0;
    for x in &rec.subs {
        if std::ptr::eq(x, s) {
            return at + 8;
        }
        at += 8 + x.data.len();
    }
    at
}

/// A subrecord's bytes as text: Windows-1252, up to the first NUL.
pub fn text(s: &Sub) -> String {
    decode(&s.data)
}

/// A reader over a subrecord that must be exactly `n` bytes (`tes3`'s `stream.expect(n)`).
pub fn sized<'a>(rec: &Record, s: &'a Sub, n: usize) -> Result<Reader<'a>> {
    if s.data.len() != n {
        return Err(Error {
            at: offset_of(rec, s),
            what: format!(
                "{}::{} is {} bytes, the layout says {n}",
                rec.tag_str(),
                s.tag_str(),
                s.data.len()
            ),
        });
    }
    Ok(Reader::new(&s.data))
}

/// A reader over a subrecord of any size.
pub fn reader<'a>(s: &'a Sub<'_>) -> Reader<'a> {
    Reader::new(&s.data)
}

/// A subrecord that holds one little-endian `u32`.
pub fn u32_of(rec: &Record, s: &Sub) -> Result<u32> {
    sized(rec, s, 4)?.u32()
}
/// A subrecord that holds one little-endian `i32`.
pub fn i32_of(rec: &Record, s: &Sub) -> Result<i32> {
    sized(rec, s, 4)?.i32()
}
/// A subrecord that holds one `f32`.
pub fn f32_of(rec: &Record, s: &Sub) -> Result<f32> {
    sized(rec, s, 4)?.f32()
}
/// A subrecord that holds one byte.
pub fn u8_of(rec: &Record, s: &Sub) -> Result<u8> {
    sized(rec, s, 1)?.u8()
}
/// A subrecord that holds one byte, read as `i8`.
pub fn i8_of(rec: &Record, s: &Sub) -> Result<i8> {
    sized(rec, s, 1)?.i8()
}
/// A subrecord that holds one `u16`.
pub fn u16_of(rec: &Record, s: &Sub) -> Result<u16> {
    sized(rec, s, 2)?.u16()
}

/// The header flags with `DELETED` set when the record carries a `DELE` subrecord, which
/// is how `tes3` reads every record (and what D1 in `backend-plan.md` settled: the bit
/// and the subrecord both mean deleted).
pub fn flags_of(rec: &Record) -> u32 {
    if rec.has(b"DELE") {
        rec.flags | FLAG_DELETED
    } else {
        rec.flags
    }
}

// ---- writing -------------------------------------------------------------------------

/// A record under construction: the header flags and the subrecords in canonical order.
pub struct Out {
    rec: Record<'static>,
}

impl Out {
    pub fn new(tag: &Tag, flags: u32) -> Out {
        let mut rec = Record::new(tag);
        rec.flags = flags;
        Out { rec }
    }
    /// A string subrecord, NUL-terminated, always written (`NAME`).
    pub fn text(&mut self, tag: &Tag, s: &str) -> Result<()> {
        let mut w = Writer::with_capacity(s.len() + 1);
        w.cstring(s)?;
        self.rec.push(tag, w.into_bytes());
        Ok(())
    }
    /// A string subrecord written only when the string is not empty (`MODL`, `FNAM`, ...).
    pub fn text_if(&mut self, tag: &Tag, s: &str) -> Result<()> {
        if s.is_empty() {
            return Ok(());
        }
        self.text(tag, s)
    }
    /// A subrecord built by a closure over a writer.
    pub fn sub(&mut self, tag: &Tag, f: impl FnOnce(&mut Writer) -> Result<()>) -> Result<()> {
        let mut w = Writer::new();
        f(&mut w)?;
        self.rec.push(tag, w.into_bytes());
        Ok(())
    }
    /// A subrecord from ready bytes.
    pub fn bytes(&mut self, tag: &Tag, data: Vec<u8>) {
        self.rec.push(tag, data);
    }
    pub fn u32(&mut self, tag: &Tag, v: u32) {
        self.rec.push(tag, v.to_le_bytes().to_vec());
    }
    pub fn i32(&mut self, tag: &Tag, v: i32) {
        self.rec.push(tag, v.to_le_bytes().to_vec());
    }
    pub fn f32(&mut self, tag: &Tag, v: f32) {
        self.rec.push(tag, v.to_le_bytes().to_vec());
    }
    pub fn u8(&mut self, tag: &Tag, v: u8) {
        self.rec.push(tag, vec![v]);
    }
    pub fn i8(&mut self, tag: &Tag, v: i8) {
        self.rec.push(tag, vec![v as u8]);
    }
    pub fn u16(&mut self, tag: &Tag, v: u16) {
        self.rec.push(tag, v.to_le_bytes().to_vec());
    }
    /// The `DELE` subrecord (four zero bytes) when the flags say deleted, as `tes3` and
    /// the Construction Set write it, always last.
    pub fn dele(&mut self) {
        if self.rec.flags & FLAG_DELETED != 0 {
            self.rec.push(b"DELE", vec![0, 0, 0, 0]);
        }
    }
    pub fn finish(self) -> Record<'static> {
        self.rec
    }
}

// ---- every record type at once ---------------------------------------------------------

/// Any record the format has, as its typed form.
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Header(Header),
    GameSetting(GameSetting),
    GlobalVariable(GlobalVariable),
    Class(Class),
    Faction(Faction),
    Race(Race),
    Sound(Sound),
    SoundGen(SoundGen),
    Skill(Skill),
    MagicEffect(MagicEffect),
    Script(Script),
    Region(Region),
    Birthsign(Birthsign),
    StartScript(StartScript),
    LandscapeTexture(LandscapeTexture),
    Spell(Spell),
    Static(Static),
    Door(Door),
    MiscItem(MiscItem),
    Weapon(Weapon),
    Container(Container),
    Creature(Creature),
    Bodypart(Bodypart),
    Light(Light),
    Enchanting(Enchanting),
    Npc(Npc),
    Armor(Armor),
    Clothing(Clothing),
    RepairItem(RepairItem),
    Activator(Activator),
    Apparatus(Apparatus),
    Lockpick(Lockpick),
    Probe(Probe),
    Ingredient(Ingredient),
    Book(Book),
    Alchemy(Alchemy),
    LeveledItem(LeveledItem),
    LeveledCreature(LeveledCreature),
    Cell(Cell),
    Landscape(Landscape),
    PathGrid(PathGrid),
    Dialogue(Dialogue),
    DialogueInfo(DialogueInfo),
}

macro_rules! objects {
    ($($var:ident => $ty:ident),* $(,)?) => {
        /// Every record tag the format has, in `tes3`'s order.
        pub const TAGS: &[Tag] = &[$($ty::TAG),*];

        impl Object {
            /// The typed form of a record, or `None` for a tag the format does not have.
            pub fn from_raw(rec: &Record) -> Result<Option<Object>> {
                $(if rec.tag == $ty::TAG { return Ok(Some(Object::$var($ty::from_raw(rec)?))); })*
                Ok(None)
            }
            pub fn to_raw(&self) -> Result<Record<'static>> {
                match self { $(Object::$var(x) => x.to_raw(),)* }
            }
            pub fn tag(&self) -> Tag {
                match self { $(Object::$var(_) => $ty::TAG,)* }
            }
        }
    };
}

objects! {
    Header => Header,
    GameSetting => GameSetting,
    GlobalVariable => GlobalVariable,
    Class => Class,
    Faction => Faction,
    Race => Race,
    Sound => Sound,
    SoundGen => SoundGen,
    Skill => Skill,
    MagicEffect => MagicEffect,
    Script => Script,
    Region => Region,
    Birthsign => Birthsign,
    StartScript => StartScript,
    LandscapeTexture => LandscapeTexture,
    Spell => Spell,
    Static => Static,
    Door => Door,
    MiscItem => MiscItem,
    Weapon => Weapon,
    Container => Container,
    Creature => Creature,
    Bodypart => Bodypart,
    Light => Light,
    Enchanting => Enchanting,
    Npc => Npc,
    Armor => Armor,
    Clothing => Clothing,
    RepairItem => RepairItem,
    Activator => Activator,
    Apparatus => Apparatus,
    Lockpick => Lockpick,
    Probe => Probe,
    Ingredient => Ingredient,
    Book => Book,
    Alchemy => Alchemy,
    LeveledItem => LeveledItem,
    LeveledCreature => LeveledCreature,
    Cell => Cell,
    Landscape => Landscape,
    PathGrid => PathGrid,
    Dialogue => Dialogue,
    DialogueInfo => DialogueInfo,
}

/// Every record of a plugin typed, with the raw record kept beside it for the ones no
/// type takes (an unknown tag) or that did not fit their layout.
pub enum Read<'a> {
    Typed(Object),
    /// A record whose tag the format does not have.
    Unknown(Record<'a>),
    /// A record of a known tag whose subrecords did not fit the layout, and why.
    Unfit(Record<'a>, Error),
}

/// Types every record of a plugin, keeping the raw form where typing did not take.
pub fn read_plugin(buf: &[u8]) -> Result<Vec<Read<'_>>> {
    let recs = super::raw::parse_plugin(buf)?;
    Ok(recs
        .into_iter()
        .map(|rec| match Object::from_raw(&rec) {
            Ok(Some(o)) => Read::Typed(o),
            Ok(None) => Read::Unknown(rec),
            Err(e) => Read::Unfit(rec, e),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_table_has_every_type_once() {
        assert_eq!(TAGS.len(), 43);
        let mut seen = std::collections::BTreeSet::new();
        for t in TAGS {
            assert!(seen.insert(*t), "{} twice", super::super::raw::tag_str(t));
        }
        assert_eq!(TAGS[0], *b"TES3");
        assert_eq!(TAGS[TAGS.len() - 1], *b"INFO");
    }

    #[test]
    fn an_unknown_subrecord_is_an_error_that_names_it() {
        let mut rec = Record::new(b"STAT");
        rec.push(b"NAME", b"rock\0".to_vec());
        rec.push(b"ZZZZ", vec![1]);
        let e = Static::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("STAT::ZZZZ"), "{e}");
        assert_eq!(e.at, 8 + 5 + 8);
    }

    #[test]
    fn a_wrong_tag_is_refused() {
        let rec = Record::new(b"ACTI");
        assert!(Static::from_raw(&rec).is_err());
        assert!(matches!(Object::from_raw(&Record::new(b"QQQQ")), Ok(None)));
    }
}
