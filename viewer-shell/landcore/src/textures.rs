//! The shared land-texture table (`land/textures.py`).
//!
//! Every plugin numbers its `LTEX` records from zero, so a `VTEX` value means nothing
//! outside the plugin that wrote it. Textures are matched across plugins by identifier into
//! one table, each plugin's indices are translated into it, and before writing the table is
//! compacted to the textures the merge actually paints with. `VTEX` stores `LTEX` index + 1;
//! 0 is no texture.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::read::Record;

pub const NO_TEXTURE: i64 = 0;

/// `vtex_of`.
pub fn vtex_of(ltex: i64) -> i64 {
    ltex + 1
}

/// One texture of the shared table (`KnownTexture`): `source` the plugin that last named its
/// file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownTexture {
    pub identifier: String,
    pub index: i64,
    pub file_name: Option<String>,
    pub source: String,
}

/// The shared table (`KnownTextures`): textures by identifier, and the last plugin's
/// index -> identifier, kept across plugins as the Python keeps it.
#[derive(Clone, Debug, Default)]
pub struct KnownTextures {
    by_id: HashMap<String, KnownTexture>,
    order: Vec<String>,
    table: Vec<(i64, String)>,
}

impl KnownTextures {
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// The textures in index order (`sorted`).
    pub fn sorted(&self) -> Vec<&KnownTexture> {
        let mut v: Vec<&KnownTexture> = self.order.iter().map(|id| &self.by_id[id]).collect();
        v.sort_by_key(|t| t.index);
        v
    }

    /// Learn a plugin's `LTEX` records and answer its translation (`observe`).
    pub fn observe(&mut self, plugin: &str, records: &[Record]) -> HashMap<i64, i64> {
        for r in records {
            let Record::Ltex { deleted, id, index, file_name } = r else { continue };
            if !self.by_id.contains_key(id) {
                if *deleted {
                    continue;
                }
                let t = KnownTexture { identifier: id.clone(), index: self.by_id.len() as i64, file_name: None, source: String::new() };
                self.by_id.insert(id.clone(), t);
                self.order.push(id.clone());
            }
            let known = self.by_id.get_mut(id).unwrap();
            if let Some(f) = file_name
                && known.file_name.as_ref() != Some(f)
            {
                known.file_name = Some(f.clone());
                known.source = plugin.to_string();
            }
            match self.table.iter_mut().find(|(i, _)| i == index) {
                Some(slot) => slot.1 = id.clone(),
                None => self.table.push((*index, id.clone())),
            }
        }
        self.translation()
    }

    /// Every index seen so far to its shared `VTEX` value (`translation`).
    pub fn translation(&self) -> HashMap<i64, i64> {
        let mut m = HashMap::from([(NO_TEXTURE, NO_TEXTURE)]);
        for (index, id) in &self.table {
            if let Some(k) = self.by_id.get(id) {
                m.insert(vtex_of(*index), vtex_of(k.index));
            }
        }
        m
    }
}

/// A plugin's `VTEX` values in the shared table (`translate_indices`), and those no `LTEX`
/// defines, with how often each appears (left as they are).
pub fn translate_indices(values: &[i64], mapping: &HashMap<i64, i64>) -> (Vec<i64>, BTreeMap<i64, usize>) {
    let mut unknown = BTreeMap::new();
    let out = values
        .iter()
        .map(|v| match mapping.get(v) {
            Some(s) => *s,
            None => {
                *unknown.entry(*v).or_insert(0) += 1;
                *v
            }
        })
        .collect();
    (out, unknown)
}

/// The smallest real value a mapping gives (`fallback_texture_index`).
fn fallback_texture_index(mapping: &HashMap<i64, i64>) -> i64 {
    mapping.values().copied().filter(|v| *v != NO_TEXTURE).min().unwrap_or(NO_TEXTURE)
}

/// The table compacted to the textures `used` paints with (`compact_textures`): the mapping
/// from shared to written values, the textures kept (renumbered), and the values no texture
/// defines (substituted with the smallest kept one when `substitute_unknown`).
pub fn compact_textures(known: &KnownTextures, used: &BTreeSet<i64>, substitute_unknown: bool) -> (HashMap<i64, i64>, Vec<KnownTexture>, Vec<i64>) {
    let by_index: HashMap<i64, &KnownTexture> = known.sorted().into_iter().map(|t| (vtex_of(t.index), t)).collect();
    let mut mapping = HashMap::from([(NO_TEXTURE, NO_TEXTURE)]);
    let mut kept: Vec<KnownTexture> = Vec::new();
    let mut unresolved = Vec::new();
    for &value in used {
        if value == NO_TEXTURE {
            continue;
        }
        let Some(t) = by_index.get(&value) else {
            unresolved.push(value);
            continue;
        };
        kept.push(KnownTexture { identifier: t.identifier.clone(), index: kept.len() as i64, file_name: t.file_name.clone(), source: t.source.clone() });
        mapping.insert(value, vtex_of(kept.len() as i64 - 1));
    }
    if substitute_unknown && !unresolved.is_empty() {
        let fallback = fallback_texture_index(&mapping);
        for v in &unresolved {
            mapping.insert(*v, fallback);
        }
    }
    (mapping, kept, unresolved)
}
