//! A placed object's identity and which version of it the load order keeps.
//!
//! A reference is named by the plugin that created it and its index there. A plugin
//! writes that as a master index (`0`: itself; `n`: its `n`th master) and the index; read
//! in another plugin, the same reference has another master index, so the name is
//! resolved through each plugin's own master list ([`origin`]).
//!
//! Which version wins ([`winner`]): the last plugin's, in load order, **unless some plugin
//! deleted the reference: a deletion sticks.** Deletion is a flag the game keeps on the
//! reference, not its removal - the object is still there, hidden - so a later plugin's record
//! for the same reference (an edit, a re-placing, a move) moves or edits a hidden thing and the
//! player still sees nothing (Robin, from the community: "moving it just moves the hidden
//! thing"; his in-game cases, `gardenfell-esp-format.md` §7 and §9.0: the Seyda Neen lighthouse,
//! Wolverine Hall's flag). It is Gardenfell's round 18fc `dead` set. The plugin that deletes
//! first stands, deleted; the plain records after it are ignored ([`Winner::ignored`]); a later
//! deletion of the same reference changes nothing and is neither. A reference nobody deletes:
//! the last record stands, whole.
//!
//! **What this rests on, and what it does not**: Wraithguard put the opposite rule in for a
//! while (Gardenfell patch 0017, "a later record undeletes") on the strength of an in-game test
//! (`tests/ingame/README.md`: five piles) and took it out again (2026-10-31). That test's
//! references are all *persistent* and in an *interior* (no `NAM0` anywhere), where the later
//! record did win; Robin's are *temporary* references in *exteriors*, where the deletion
//! stuck. The two have not been reconciled: `tests/ingame/` Variant T (every reference
//! temporary) is the next check, and if it shows the section matters this rule has to know
//! which section a reference is in (Gardenfell carries `CellRef.temporary`).
//! Cases: `tests/fixtures/ref_cases.json`, run here and by `tests/test_ref_cases.py`.

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
    /// Versions the game ignores (indices into the versions): the plain records after the
    /// first deletion, which edit a hidden object. Empty when nobody deleted the reference.
    pub ignored: Vec<usize>,
}

/// The version of one reference that stands, from its versions in load order (every
/// record any plugin has for the same origin and index). `None` for no versions.
pub fn winner(versions: &[Version]) -> Option<Winner> {
    if versions.is_empty() {
        return None;
    }
    // A deletion sticks: the first plugin to delete it is the one that stands.
    let Some(first) = versions.iter().position(|v| v.deleted) else {
        return Some(Winner { at: versions.len() - 1, deleted: false, ignored: Vec::new() });
    };
    let ignored = (first + 1..versions.len()).filter(|&i| !versions[i].deleted).collect();
    Some(Winner { at: first, deleted: true, ignored })
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
    fn with_no_deletion_the_last_version_stands() {
        let v = |plugin, deleted| Version { plugin, deleted };
        let w = winner(&[v(0, false), v(1, false)]).unwrap();
        assert_eq!((w.at, w.deleted, w.ignored), (1, false, vec![]));
        assert!(winner(&[]).is_none());
    }

    #[test]
    fn a_deletion_sticks() {
        let v = |plugin, deleted| Version { plugin, deleted };
        // an edit after the deletion is ignored
        let w = winner(&[v(0, false), v(1, true), v(2, false)]).unwrap();
        assert_eq!((w.at, w.deleted, w.ignored), (1, true, vec![2]));
        // a deletion after it changes nothing: the first stands, only the plain record is ignored
        let w = winner(&[v(0, false), v(1, true), v(2, false), v(3, true)]).unwrap();
        assert_eq!((w.at, w.deleted, w.ignored), (1, true, vec![2]));
        // a deletion that is the last version
        let w = winner(&[v(0, false), v(1, false), v(2, true)]).unwrap();
        assert_eq!((w.at, w.deleted, w.ignored), (2, true, vec![]));
    }
}
