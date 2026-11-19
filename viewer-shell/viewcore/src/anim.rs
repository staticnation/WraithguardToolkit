//! Named animation groups, for the scripts the Preview runs - Wraithguard.
//!
//! MWScript's `PlayGroup` and `LoopGroup` and OpenMW Lua's `openmw.animation` name a
//! group ("Idle3", "Hit2", "WalkForward"); the renderer plays one clip per mesh, the
//! one [`KfSequence::idle`] picks. A request for another group is the same mesh with the
//! `.kf` narrowed to that group, so `idle` picks it and nothing below this changes:
//!
//! - the page asks for `__grp/<group>/<model>` (an NPC: `__npc/<id>?group=<group>`);
//! - [`only`] is the `.kf` with its text keys cut down to that group's, renamed `Idle`;
//! - [`groups`] lists what a `.kf` has, with each group's window, so the page knows how
//!   long a `PlayGroup` runs before the actor goes back to its idle.
//!
//! Copyright (c) 2026 StaticNation, MIT (as the rest of viewcore).

use crate::nif::KfSequence;

/// The path prefix the page asks a mesh playing a named group by.
pub const GROUP_PREFIX: &str = "__grp/";

/// `(group, model)` from `__grp/<group>/<model>`, or `None` for any other path.
pub fn group_request(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix(GROUP_PREFIX).or_else(|| path.strip_prefix("__grp\\"))?;
    let (g, m) = rest.split_once(['/', '\\'])?;
    if g.is_empty() || m.is_empty() {
        return None;
    }
    Some((g, m))
}

/// The model a path names, without a group request in front of it.
pub fn model_of(path: &str) -> &str {
    group_request(path).map(|(_, m)| m).unwrap_or(path)
}

/// The `.kf` narrowed to the group `name`: its window becomes the only clip, called
/// `Idle`, so the binding plays it in a loop. `None` when the file has no such group
/// (or it has no length), and the caller draws the mesh's own idle instead.
pub fn only(kf: &KfSequence, name: &str) -> Option<KfSequence> {
    let (a, b) = kf.clip(name)?;
    Some(KfSequence { text_keys: vec![(a, "Idle: Start".into()), (b, "Idle: Stop".into())], tracks: kf.tracks.clone() })
}

/// The groups a `.kf` marks, in file order, each with the window [`KfSequence::clip`]
/// gives it (its loop when it has one). Groups with no length are left out: there is
/// nothing to play.
pub fn groups(kf: &KfSequence) -> Vec<(String, f32, f32)> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (_, k) in &kf.text_keys {
        for line in k.lines() {
            let line = line.trim();
            let Some((name, tail)) = line.rsplit_once(':') else { continue };
            if !tail.trim().eq_ignore_ascii_case("start") {
                continue;
            }
            let name = name.trim();
            if name.is_empty() || seen.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                continue;
            }
            seen.push(name.to_string());
            if let Some((a, b)) = kf.clip(name) {
                out.push((name.to_string(), a, b));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kf() -> KfSequence {
        KfSequence {
            text_keys: vec![
                (0.0, "Idle: Start".into()),
                (2.0, "Idle: Stop\r\nIdle2: Start".into()),
                (2.5, "Idle2: Loop Start".into()),
                (4.0, "Idle2: Loop Stop".into()),
                (5.0, "Idle2: Stop".into()),
                (6.0, "Death1: Start".into()),
                (6.0, "Death1: Stop".into()),
            ],
            tracks: Vec::new(),
        }
    }

    #[test]
    fn a_group_request_names_the_group_and_the_model() {
        assert_eq!(group_request("__grp/Idle3/r/xsiltstrider.nif"), Some(("Idle3", "r/xsiltstrider.nif")));
        assert_eq!(group_request("r/xsiltstrider.nif"), None);
        assert_eq!(group_request("__grp//x.nif"), None);
        assert_eq!(model_of("__grp/Hit1/x.nif"), "x.nif");
        assert_eq!(model_of("x.nif"), "x.nif");
    }

    #[test]
    fn groups_are_listed_with_their_windows() {
        let g = groups(&kf());
        assert_eq!(g, vec![("Idle".to_string(), 0.0, 2.0), ("Idle2".to_string(), 2.5, 4.0)]);
    }

    #[test]
    fn only_makes_the_group_the_idle() {
        let k = only(&kf(), "idle2").expect("the file has it");
        assert_eq!(k.idle(), Some((2.5, 4.0)));
        assert!(only(&kf(), "Death1").is_none(), "no length, nothing to play");
        assert!(only(&kf(), "Swim").is_none());
    }
}
