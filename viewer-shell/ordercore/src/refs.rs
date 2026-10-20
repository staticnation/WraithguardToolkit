//! A placed object's identity and which version of it the load order keeps.
//!
//! A reference is named by the plugin that created it and its index there. A plugin
//! writes that as a master index (`0`: itself; `n`: its `n`th master) and the index; read
//! in another plugin, the same reference has another master index, so the name is
//! resolved through each plugin's own master list ([`origin`]).
//!
//! Which version wins ([`winner`]): the last plugin's, in load order, with one exception
//! the game makes. **A deletion sticks**: once a plugin deletes a reference, a later
//! plugin's record for it edits a hidden thing - the game keeps the deleted flag - so the
//! deleting version is the one that stands, and the later ones are listed as ignored.
//! (Robin, from the community: "the reference still exists and is just hidden. So moving
//! it just moves the hidden thing.") The engine's merge has kept this since round 18fc
//! (Gardenfell's `dead` set); the Editor (Python) took the last version, deleted or not,
//! and so offered to edit references the game hides. Cases:
//! `tests/fixtures/ref_cases.json`, run here and by `tests/test_ref_cases.py`.

/// The plugin a reference's master index names, read in the plugin it is in: `0` the
/// plugin itself, `n` its `n`th master; `None` for an index its list does not reach.
pub fn origin<'a>(plugin: &'a str, masters: &'a [String], mast_index: i64) -> Option<&'a str> {
    match mast_index {
        0 => Some(plugin),
        n if n > 0 => masters.get(usize::try_from(n - 1).ok()?).map(String::as_str),
        _ => None,
    }
}

/// One plugin's record for a reference, as [`winner`] reads them: the plugin's position in
/// the versions given, and whether this record deletes the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub plugin: usize,
    pub deleted: bool,
}

/// Which version of one reference stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Winner {
    /// The version that stands (an index into the versions given).
    pub at: usize,
    /// Whether it is deleted.
    pub deleted: bool,
    /// Versions after a deletion that do not delete it again, which the game ignores
    /// (indices into the versions).
    pub ignored: Vec<usize>,
}

/// The version of one reference that stands, from its versions in load order (every
/// record any plugin has for the same origin and index). `None` for no versions.
pub fn winner(versions: &[Version]) -> Option<Winner> {
    let mut at: Option<usize> = None;
    let mut ignored = Vec::new();
    for (i, v) in versions.iter().enumerate() {
        match at {
            // After a deletion, only another deletion takes over (the engine's merge does
            // the same: a deleting record is applied, anything else is dropped).
            Some(w) if versions[w].deleted && !v.deleted => ignored.push(i),
            _ => at = Some(i),
        }
    }
    let at = at?;
    Some(Winner { at, deleted: versions[at].deleted, ignored })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_indices_name_plugins() {
        let m = vec!["Morrowind.esm".to_string(), "Tribunal.esm".to_string()];
        assert_eq!(origin("Mod.esp", &m, 0), Some("Mod.esp"));
        assert_eq!(origin("Mod.esp", &m, 2), Some("Tribunal.esm"));
        assert_eq!(origin("Mod.esp", &m, 3), None);
        assert_eq!(origin("Mod.esp", &m, -1), None);
    }

    #[test]
    fn a_deletion_sticks() {
        let v = |plugin, deleted| Version { plugin, deleted };
        let w = winner(&[v(0, false), v(1, true), v(2, false)]).unwrap();
        assert_eq!((w.at, w.deleted, w.ignored), (1, true, vec![2]));
        let w = winner(&[v(0, false), v(1, true), v(2, false), v(3, true)]).unwrap();
        assert_eq!((w.at, w.deleted, w.ignored), (3, true, vec![2]));
        let w = winner(&[v(0, false), v(1, false)]).unwrap();
        assert_eq!((w.at, w.deleted), (1, false));
    }
}
