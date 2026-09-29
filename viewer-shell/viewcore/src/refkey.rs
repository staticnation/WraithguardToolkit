//! A reference's stable name: the plugin that created it, and its index in that plugin.

use crate::esp::RefNum;

/// A reference, named by the plugin that created it and its index there - a key that
/// survives the load order changing, where [`RefNum`]'s `content_file` (an index into
/// the order) does not. ORI and the mod highlight name references this way.
///
/// Lowercased on the way in, because these are Morrowind filenames and the same plugin is
/// spelled three ways across a load order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefKey {
    pub plugin: String,
    pub index: u32,
}

impl RefKey {
    pub fn new(plugin: &str, index: u32) -> RefKey {
        RefKey { plugin: plugin.to_ascii_lowercase(), index }
    }
    /// The key for a reference the merge has already resolved, against the load order it
    /// was resolved in. `None` for a reference whose originating file is not in that
    /// order — a broken master link, which `RefNum::resolve` marks with a negative
    /// `content_file`.
    pub fn of(num: RefNum, order: &[String]) -> Option<RefKey> {
        let i = usize::try_from(num.content_file).ok()?;
        Some(RefKey::new(order.get(i)?, num.index))
    }
}

/// The key as it is written: `plugin.esm:12345`.
///
/// A colon, because a plugin name can hold spaces, dots and dashes but not a colon on any
/// filesystem Gardenfell runs on — so splitting on the last one is unambiguous however
/// the mod was named.
pub fn key_text(k: &RefKey) -> String {
    format!("{}:{}", k.plugin, k.index)
}
