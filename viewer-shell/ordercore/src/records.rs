//! Which version of a record the load order uses, and what a placed object is - as
//! OpenMW resolves them (read from its record store's behaviour, not its code):
//!
//! - Every record type is kept on its own: one id may be a weapon in one plugin and a
//!   misc item in another, and both exist. Within a type the last plugin's version wins;
//!   a deleted one (DELE) takes the record out, and a later plugin may define it again.
//! - A placed object names only an id. Of the object types that define it, the one it is
//!   is the one whose four-letter tag is greatest read as a little-endian number - in
//!   effect by its last letter (`WEAP` over `MISC`, `NPC_` over everything) - and not
//!   the one loaded last.
//!
//! Ids are compared without case, as the game does.

use std::collections::HashMap;

/// A four-letter record tag.
pub type Tag = [u8; 4];

/// The types a placed object can be (OpenMW's id index holds these and no others).
pub const OBJECT_TYPES: [&Tag; 21] = [
    b"ACTI", b"ALCH", b"APPA", b"ARMO", b"BODY", b"BOOK", b"CLOT", b"CONT", b"CREA", b"DOOR", b"INGR", b"LEVC", b"LEVI",
    b"LIGH", b"LOCK", b"MISC", b"NPC_", b"PROB", b"REPA", b"STAT", b"WEAP",
];

/// Whether a tag is one a placed object can be ([`OBJECT_TYPES`]).
pub fn is_object_type(tag: &Tag) -> bool {
    OBJECT_TYPES.contains(&tag)
}

/// The order OpenMW settles an id defined as several object types by: the greatest wins.
pub fn placed_rank(tag: &Tag) -> u32 {
    u32::from_le_bytes(*tag)
}

/// One record as a plugin states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tag: Tag,
    /// The id as the plugin spells it.
    pub id: String,
    pub deleted: bool,
}

/// One live record: its winner and every plugin that defines it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Versions {
    /// The id as the winning plugin spells it.
    pub id: String,
    /// The plugins (load-order indices) whose version counts, in order; the last wins.
    /// A deletion clears the list, so a plugin before it is not one of them.
    pub plugins: Vec<usize>,
}

impl Versions {
    /// The winning plugin.
    pub fn winner(&self) -> usize {
        *self.plugins.last().expect("a live record has a plugin")
    }
}

/// The load order's records: per (tag, lower-case id), the live versions.
#[derive(Clone, Debug, Default)]
pub struct Index {
    live: HashMap<(Tag, String), Versions>,
    /// Lower-case id -> the object types it is live as.
    by_id: HashMap<String, Vec<Tag>>,
}

impl Index {
    /// Build from each plugin's records, plugins in load order.
    pub fn build<P, E>(plugins: P) -> Index
    where
        P: IntoIterator<Item = E>,
        E: IntoIterator<Item = Entry>,
    {
        let mut live: HashMap<(Tag, String), Versions> = HashMap::new();
        for (pi, entries) in plugins.into_iter().enumerate() {
            for e in entries {
                let key = (e.tag, e.id.to_ascii_lowercase());
                if e.deleted {
                    live.remove(&key);
                    continue;
                }
                let v = live.entry(key).or_default();
                v.id = e.id;
                if v.plugins.last() != Some(&pi) {
                    v.plugins.push(pi);
                }
            }
        }
        let mut by_id: HashMap<String, Vec<Tag>> = HashMap::new();
        for (tag, id) in live.keys() {
            by_id.entry(id.clone()).or_default().push(*tag);
        }
        for tags in by_id.values_mut() {
            tags.sort_by_key(placed_rank);
        }
        Index { live, by_id }
    }

    /// A record's live versions, or None when no plugin has it (or the last deleted it).
    pub fn get(&self, tag: &Tag, id: &str) -> Option<&Versions> {
        self.live.get(&(*tag, id.to_ascii_lowercase()))
    }

    /// Every type an id is live as, in [`placed_rank`] order.
    pub fn types(&self, id: &str) -> &[Tag] {
        self.by_id.get(&id.to_ascii_lowercase()).map(Vec::as_slice).unwrap_or(&[])
    }

    /// What a placed object of this id is: the greatest-ranked object type it is live as.
    pub fn placed_type(&self, id: &str) -> Option<Tag> {
        self.types(id).iter().rev().find(|t| is_object_type(t)).copied()
    }

    /// Whether a record of this type is the one a placed object of its id is (any record
    /// that is not an object type is).
    pub fn is_placed(&self, tag: &Tag, id: &str) -> bool {
        !is_object_type(tag) || self.placed_type(id) == Some(*tag)
    }

    /// Every live record: `(tag, lower-case id, versions)`.
    pub fn iter(&self) -> impl Iterator<Item = (&Tag, &str, &Versions)> {
        self.live.iter().map(|((t, id), v)| (t, id.as_str(), v))
    }

    /// How many records are live.
    pub fn len(&self) -> usize {
        self.live.len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(tag: &Tag, id: &str, deleted: bool) -> Entry {
        Entry { tag: *tag, id: id.to_string(), deleted }
    }

    #[test]
    fn the_last_letter_decides_not_the_load_order() {
        let ix = Index::build([vec![e(b"WEAP", "X", false)], vec![e(b"MISC", "x", false)]]);
        assert_eq!(ix.placed_type("x"), Some(*b"WEAP"));
        assert_eq!(ix.get(b"MISC", "X").unwrap().winner(), 1);
        assert!(ix.is_placed(b"WEAP", "x") && !ix.is_placed(b"MISC", "x"));
        assert!(placed_rank(b"NPC_") > placed_rank(b"BODY") && placed_rank(b"LEVI") > placed_rank(b"LEVC"));
    }
}
