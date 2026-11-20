//! Named animation groups, for the scripts the Preview runs - Wraithguard.
//!
//! MWScript's `PlayGroup` and `LoopGroup` and OpenMW Lua's `openmw.animation` name a
//! group ("Idle3", "Hit2", "WalkForward"), on the whole body or on some of OpenMW's four
//! bone groups (lower body, torso, left arm, right arm: legs walking while the arms swing).
//! The renderer plays one clip per mesh, the one [`KfSequence::idle`] picks, so a request
//! is turned into one clip that holds all of them:
//!
//! - the page asks for `__grp/<spec>/<model>` (an NPC: `__npc/<id>?group=<spec>`), the spec
//!   one slot for the whole body or four, comma-separated, one per bone group, each
//!   `group@at*speed` - `at` the renderer's clock when it began, an empty group the mesh's
//!   own idle;
//! - [`layered`] moves each slot's window of the `.kf` into a band of its own on the time
//!   axis (`band_start`) and gives each track the band of the bone group its node is in, so
//!   one clip, `Idle`, spans them all;
//! - [`retime`], on the parts read with it, gives each animated link its band's window and
//!   a clock that puts the band's first frame at `at` and runs at `speed` - so every group
//!   starts on its own first frame, at its own time, and loops its own length.
//!
//! [`groups`] lists what a `.kf` has, with each group's window, so the page knows how long a
//! group runs before the actor goes back to its idle.
//!
//! Nothing below `nif` changes: the binding and the renderer read controllers as they always
//! did (`t = frequency * secs + phase`, looped in `[start, stop]`).
//!
//! Copyright (c) 2026 StaticNation, MIT (as the rest of viewcore).

use crate::nif::{AnimLink, DrawPart, KfData, KfSequence, KfTrack};

/// The path prefix the page asks a mesh playing named groups by.
pub const GROUP_PREFIX: &str = "__grp/";

/// OpenMW's bone groups, in the order of a four-slot spec (and of `BLEND_MASK`'s bits).
pub const BONE_GROUPS: [&str; 4] = ["LowerBody", "Torso", "LeftArm", "RightArm"];

/// One bone group's part of a request: the group (empty: the mesh's idle), the renderer's
/// clock when it began, its speed.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub group: String,
    pub at: f32,
    pub speed: f32,
}

/// `(spec, model)` from `__grp/<spec>/<model>`, or `None` for any other path.
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

/// The four slots of a spec: one entry is the whole body, four are the bone groups in
/// [`BONE_GROUPS`] order. `group`, `group@at` and `group@at*speed` all read; a bad number
/// is 0 (at) or 1 (speed). `None` for any other count, or when every slot is the idle.
pub fn parse_spec(spec: &str) -> Option<[Slot; 4]> {
    let one = |s: &str| -> Slot {
        let (g, rest) = s.split_once('@').unwrap_or((s, ""));
        let (at, sp) = rest.split_once('*').unwrap_or((rest, ""));
        let speed = sp.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
        Slot { group: g.trim().to_string(), at: at.trim().parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(0.0), speed }
    };
    let parts: Vec<&str> = spec.split(',').collect();
    let slots: [Slot; 4] = match parts.len() {
        1 => {
            let s = one(parts[0]);
            [s.clone(), s.clone(), s.clone(), s]
        }
        4 => [one(parts[0]), one(parts[1]), one(parts[2]), one(parts[3])],
        _ => return None,
    };
    if slots.iter().all(|s| s.group.is_empty()) {
        return None;
    }
    Some(slots)
}

/// The bone group a node is in, by OpenMW's roots (`Bip01 Spine1` the torso, the
/// clavicles the arms, everything else the lower body), read off the Biped names a
/// Morrowind skeleton uses. A creature whose bones are named otherwise is all lower body.
pub fn bone_group(node: &str) -> usize {
    let n = format!(" {} ", node.to_ascii_lowercase());
    let arm = |side: &str| ["clavicle", "upperarm", "forearm", "hand", "finger"].iter().any(|w| n.contains(&format!(" {side} {w}")));
    if arm("l") || n.contains("shield bone") {
        2
    } else if arm("r") || n.contains("weapon bone") {
        3
    } else if ["spine1", "spine2", "neck", "head"].iter().any(|w| n.contains(w)) {
        1
    } else {
        0
    }
}

/// Where bone group `k`'s band starts on the layered clip's time axis. A thousand seconds
/// apart, far past any clip's length and any controller a mesh keys on its own; low enough
/// that a key's time keeps its precision as an `f32`.
pub fn band_start(k: usize) -> f32 {
    1000.0 * (k as f32 + 1.0)
}

/// One band of a layered clip: its window on the time axis and the clock it runs on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub phase: f32,
}

fn shifted(d: &KfData, by: f32) -> KfData {
    fn mv<V: Clone>(keys: &[(f32, V)], by: f32) -> Vec<(f32, V)> {
        keys.iter().map(|(t, v)| (*t + by, v.clone())).collect()
    }
    KfData {
        rot: mv(&d.rot, by),
        rot_xyz: d.rot_xyz.as_ref().map(|c| [mv(&c[0], by), mv(&c[1], by), mv(&c[2], by)]),
        rot_order: d.rot_order,
        trans: mv(&d.trans, by),
        scales: mv(&d.scales, by),
    }
}

/// Each slot's window of the `.kf`: its group's, the idle where it names none or one the
/// file lacks, `None` where there is nothing to play (its bones stand in their rest pose).
pub fn windows(kf: &KfSequence, slots: &[Slot; 4]) -> [Option<(f32, f32)>; 4] {
    let mut w: [Option<(f32, f32)>; 4] = [None; 4];
    for (k, s) in slots.iter().enumerate() {
        w[k] = if s.group.is_empty() { kf.idle() } else { kf.clip(&s.group).or_else(|| kf.idle()) };
    }
    w
}

/// The bands for `slots` over their `windows`: each bone group's place on the time axis
/// and the clock that starts it on its first frame at `at`.
pub fn bands(windows: &[Option<(f32, f32)>; 4], slots: &[Slot; 4]) -> [Option<Band>; 4] {
    let mut out: [Option<Band>; 4] = [None; 4];
    for (k, (w, s)) in windows.iter().zip(slots.iter()).enumerate() {
        if let Some((a, b)) = *w {
            let o = band_start(k);
            out[k] = Some(Band { start: o, stop: o + (b - a), frequency: s.speed, phase: o - s.speed * s.at });
        }
    }
    out
}

/// The node an actor's movement is accumulated on: the parent of `Bip01 NonAccum`, `Bip01`
/// in Morrowind's skeletons (OpenMW's `mAccumRoot`).
pub const ACCUM_ROOT: &str = "Bip01";

/// The root motion of a group taken off, as OpenMW takes it (Animation's accumulation,
/// `(1, 1, 0)` for actors): `Bip01`'s translation keys keep their height but stand where the
/// group starts across the ground - the actor itself is what moves. Played with it, a walk
/// carried the body ahead of the actor through each cycle and back at its loop (Static: "still
/// rubberbanding"). `start` is the group's first instant, in the file's own times.
fn strip_root_motion(d: &mut KfData, start: f32) {
    let Some(at) = d.trans.iter().find(|(t, _)| *t >= start - 1e-4).or(d.trans.first()).map(|(_, v)| *v) else { return };
    for (_, v) in d.trans.iter_mut() {
        v[0] = at[0];
        v[1] = at[1];
    }
}

/// How fast a group carries `Bip01` across the ground, units a second, over its window
/// (OpenMW's `calcAnimVelocity`): what the walk or run is played against, so its feet keep
/// the ground at the actor's pace. 0 for a group that does not travel.
pub fn velocity(kf: &KfSequence, a: f32, b: f32) -> f32 {
    let Some(t) = kf.tracks.iter().find(|t| t.node.eq_ignore_ascii_case(ACCUM_ROOT)) else { return 0.0 };
    let keys: Vec<&(f32, [f32; 3])> = t.data.trans.iter().filter(|(k, _)| *k >= a - 1e-4 && *k <= b + 1e-4).collect();
    let (Some(f), Some(l)) = (keys.first(), keys.last()) else { return 0.0 };
    let dt = l.0 - f.0;
    if dt <= 1e-4 {
        return 0.0;
    }
    ((l.1[0] - f.1[0]).powi(2) + (l.1[1] - f.1[1]).powi(2)).sqrt() / dt
}

/// The `.kf` laid out for `windows` (see [`windows`]): each track's window of keys moved
/// into its bone group's band, the accumulation root's travel across the ground taken off
/// ([`strip_root_motion`]). `None` when no slot plays anything.
pub fn layout(kf: &KfSequence, windows: &[Option<(f32, f32)>; 4]) -> Option<KfSequence> {
    if windows.iter().all(|w| w.is_none()) {
        return None;
    }
    let mut tracks = Vec::with_capacity(kf.tracks.len());
    for t in &kf.tracks {
        let k = bone_group(&t.node);
        let Some((a, b)) = windows[k] else { continue };
        let mut d = t.data.window(a, b);
        if t.node.eq_ignore_ascii_case(ACCUM_ROOT) {
            strip_root_motion(&mut d, a);
        }
        tracks.push(KfTrack { node: t.node.clone(), ctrl: t.ctrl.clone(), data: shifted(&d, band_start(k) - a) });
    }
    let text_keys = vec![(0.0, "Idle: Start".to_string()), (band_start(4), "Idle: Stop".to_string())];
    Some(KfSequence { text_keys, tracks })
}

/// [`layout`] and [`bands`] at once: the `.kf` laid out for `slots`, and each bone group's
/// band. `None` when no slot plays anything.
pub fn layered(kf: &KfSequence, slots: &[Slot; 4]) -> Option<(KfSequence, [Option<Band>; 4])> {
    let w = windows(kf, slots);
    layout(kf, &w).map(|k| (k, bands(&w, slots)))
}

/// A spec without its times and speeds: what the laid-out clip depends on, for a cache key.
pub fn spec_groups(slots: &[Slot; 4]) -> String {
    slots.iter().map(|s| s.group.to_ascii_lowercase()).collect::<Vec<_>>().join(",")
}

/// The band a link's keys fall in: the middle of its first and last key, which a clip's
/// window (with the one key either side `window` keeps) puts inside its band.
fn band_of(d: &KfData) -> Option<usize> {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    let mut see = |t: f32| {
        lo = lo.min(t);
        hi = hi.max(t);
    };
    d.rot.iter().for_each(|(t, _)| see(*t));
    d.trans.iter().for_each(|(t, _)| see(*t));
    d.scales.iter().for_each(|(t, _)| see(*t));
    if let Some(c) = &d.rot_xyz {
        c.iter().flatten().for_each(|(t, _)| see(*t));
    }
    if lo > hi {
        return None;
    }
    let k = ((lo + hi) / 2.0 / 1000.0).floor() as i64 - 1;
    (0..4).contains(&k).then_some(k as usize)
}

/// Each animated link of `parts` given its band's window and clock (see the module).
/// Links whose keys are not in a band - a mesh's own controllers - are left as they are.
pub fn retime(parts: &mut [DrawPart], bands: &[Option<Band>; 4]) {
    let fix = |l: &mut AnimLink| {
        let Some(b) = band_of(&l.data).and_then(|k| bands[k]) else { return };
        l.ctrl.start = b.start;
        l.ctrl.stop = b.stop;
        l.ctrl.frequency = b.frequency;
        l.ctrl.phase = b.phase;
        l.ctrl.flags = (l.ctrl.flags & !0x6) | 0x8; // on, looping
    };
    for p in parts.iter_mut() {
        if let Some(a) = p.anim.as_mut() {
            a.nodes.iter_mut().for_each(fix);
        }
        if let Some(s) = p.skin.as_mut() {
            s.bones.iter_mut().flat_map(|b| b.chain.iter_mut()).for_each(fix);
        }
    }
}

/// When `name: Start` is keyed, for a group with no length.
fn start_of(kf: &KfSequence, name: &str) -> Option<f32> {
    let want = format!("{name}: start").to_ascii_lowercase();
    kf.text_keys.iter().find(|(_, k)| k.lines().any(|l| l.trim().to_ascii_lowercase() == want)).map(|(t, _)| *t)
}

/// The groups a `.kf` marks, in file order, each with the window [`KfSequence::clip`]
/// gives it (its loop when it has one); a group with no length with its one instant.
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
            /* A group with no length - a banner's `Idle`, its still pose - is listed with its
               one instant, so a script's `PlayGroup Idle` finds it (the page takes it as the
               mesh's own idle). */
            if let Some((a, b)) = kf.clip(name) {
                out.push((name.to_string(), a, b));
            } else if let Some(t) = start_of(kf, name) {
                out.push((name.to_string(), t, t));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nif::KfCtrl;

    fn track(node: &str, times: &[f32]) -> KfTrack {
        KfTrack {
            node: node.into(),
            ctrl: KfCtrl { next: -1, flags: 8, frequency: 1.0, phase: 0.0, start: 0.0, stop: 0.0, target: -1, data: -1 },
            data: KfData { rot: times.iter().map(|t| (*t, [1.0, 0.0, 0.0, 0.0])).collect(), rot_xyz: None, rot_order: 0, trans: Vec::new(), scales: Vec::new() },
        }
    }

    fn kf() -> KfSequence {
        let times: Vec<f32> = (0..=12).map(|i| i as f32 * 0.5).collect();
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
            tracks: vec![track("Bip01 Pelvis", &times), track("Bip01 Spine1", &times), track("Bip01 L Forearm", &times), track("Bip01 R Hand", &times)],
        }
    }

    /// A walk whose `Bip01` travels 0 -> 200 across the ground in 2 s (and bobs 0 -> 5 -> 0):
    /// laid out, it stays where the walk starts across the ground and keeps its bob; its
    /// velocity is 100 units a second; a group with no travel is 0.
    #[test]
    fn a_walk_s_root_motion_is_taken_off_and_its_velocity_measured() {
        let mut root = track("Bip01", &[]);
        root.data.trans = vec![(10.0, [0.0, 0.0, 0.0]), (11.0, [0.0, 100.0, 5.0]), (12.0, [0.0, 200.0, 0.0])];
        let mut k = kf();
        k.text_keys.push((10.0, "WalkForward: Start".into()));
        k.text_keys.push((12.0, "WalkForward: Stop".into()));
        k.tracks.push(root);
        assert!((velocity(&k, 10.0, 12.0) - 100.0).abs() < 1e-3);
        assert_eq!(velocity(&k, 0.0, 2.0), 0.0);
        let slots = parse_spec("WalkForward@0").unwrap();
        let (laid, _) = layered(&k, &slots).unwrap();
        let t = laid.tracks.iter().find(|t| t.node == "Bip01").unwrap();
        assert!(t.data.trans.iter().all(|(_, v)| v[0] == 0.0 && v[1] == 0.0), "{:?}", t.data.trans);
        assert!(t.data.trans.iter().any(|(_, v)| v[2] == 5.0), "the bob kept: {:?}", t.data.trans);
        // Any other node keeps its translation.
        let mut k2 = k.clone();
        k2.tracks.last_mut().unwrap().node = "Bip01 Pelvis".into();
        let (laid2, _) = layered(&k2, &slots).unwrap();
        assert!(laid2.tracks.iter().any(|t| t.data.trans.iter().any(|(_, v)| v[1] == 200.0)));
    }

    #[test]
    fn a_group_request_names_the_spec_and_the_model() {
        assert_eq!(group_request("__grp/Idle3/r/xsiltstrider.nif"), Some(("Idle3", "r/xsiltstrider.nif")));
        assert_eq!(group_request("__grp/Idle3@12.5*2,,,/r/x.nif"), Some(("Idle3@12.5*2,,,", "r/x.nif")));
        assert_eq!(group_request("r/xsiltstrider.nif"), None);
        assert_eq!(group_request("__grp//x.nif"), None);
        assert_eq!(model_of("__grp/Hit1/x.nif"), "x.nif");
        assert_eq!(model_of("x.nif"), "x.nif");
    }

    #[test]
    fn a_spec_is_one_slot_or_four() {
        let s = parse_spec("Idle2@12.5*1.5").unwrap();
        assert!(s.iter().all(|x| *x == Slot { group: "Idle2".into(), at: 12.5, speed: 1.5 }));
        let s = parse_spec("WalkForward@3,,Idle9@4*x,").unwrap();
        assert_eq!(s[0], Slot { group: "WalkForward".into(), at: 3.0, speed: 1.0 });
        assert_eq!(s[1].group, "");
        assert_eq!(s[2], Slot { group: "Idle9".into(), at: 4.0, speed: 1.0 });
        assert!(parse_spec(",,,").is_none() && parse_spec("a,b").is_none());
        assert_eq!(spec_groups(&parse_spec("Walk@3,,Idle9@4*2,").unwrap()), "walk,,idle9,", "the cache key leaves the times out");
    }

    #[test]
    fn bones_fall_in_openmw_s_groups() {
        assert_eq!(bone_group("Bip01 Pelvis"), 0);
        assert_eq!(bone_group("Bip01 Spine"), 0);
        assert_eq!(bone_group("Bip01 L Thigh"), 0);
        assert_eq!(bone_group("Bip01 Spine1"), 1);
        assert_eq!(bone_group("Bip01 Head"), 1);
        assert_eq!(bone_group("Bip01 L Clavicle"), 2);
        assert_eq!(bone_group("Shield Bone"), 2);
        assert_eq!(bone_group("Bip01 R Finger1"), 3);
        assert_eq!(bone_group("Weapon Bone"), 3);
        assert_eq!(bone_group("Root Bone"), 0);
    }

    #[test]
    fn layered_puts_each_bone_group_in_its_band() {
        let slots = parse_spec("Idle2@10*2,,Idle2@10*2,Idle2@10*2").unwrap();
        let (k, bands) = layered(&kf(), &slots).expect("plays");
        assert_eq!(k.idle(), Some((0.0, band_start(4))), "one clip over every band");
        // The pelvis plays Idle2's loop (2.5..4) in band 0; the spine the idle (0..2) in band 1.
        assert_eq!(bands[0], Some(Band { start: 1000.0, stop: 1001.5, frequency: 2.0, phase: 1000.0 - 20.0 }));
        assert_eq!(bands[1], Some(Band { start: 2000.0, stop: 2002.0, frequency: 1.0, phase: 2000.0 }));
        let pelvis = &k.tracks[0].data.rot;
        assert!(pelvis.first().unwrap().0 >= 999.9 && pelvis.last().unwrap().0 <= 1001.6, "{pelvis:?}");
        assert_eq!(band_of(&k.tracks[0].data), Some(0));
        assert_eq!(band_of(&k.tracks[1].data), Some(1));
        assert_eq!(band_of(&k.tracks[2].data), Some(2));
        assert_eq!(band_of(&k.tracks[3].data), Some(3));
        // A group the file lacks plays the idle; nothing to play at all is None.
        let (_, b) = layered(&kf(), &parse_spec("Swim").unwrap()).unwrap();
        assert_eq!(b[0].unwrap().stop - b[0].unwrap().start, 2.0);
        let still = KfSequence { text_keys: vec![(0.0, "Idle: Start".into()), (0.0, "Idle: Stop".into())], tracks: Vec::new() };
        assert!(layered(&still, &parse_spec("Swim").unwrap()).is_none());
    }

    #[test]
    fn retime_gives_links_their_band_s_clock() {
        let slots = parse_spec("Idle2@10*2").unwrap();
        let (k, bands) = layered(&kf(), &slots).unwrap();
        let link = |d: &KfData| AnimLink { pre: crate::nif::identity(), local: crate::nif::identity(), ctrl: k.tracks[0].ctrl.clone(), data: d.clone() };
        let own = KfData { rot: vec![(0.0, [1.0, 0.0, 0.0, 0.0]), (3.0, [1.0, 0.0, 0.0, 0.0])], ..Default::default() };
        let mut parts = vec![DrawPart {
            anim: Some(crate::nif::PartAnim { nodes: vec![link(&k.tracks[2].data), link(&own)], ..Default::default() }),
            ..Default::default()
        }];
        retime(&mut parts, &bands);
        let n = &parts[0].anim.as_ref().unwrap().nodes;
        assert_eq!((n[0].ctrl.start, n[0].ctrl.stop, n[0].ctrl.frequency, n[0].ctrl.phase), (3000.0, 3001.5, 2.0, 2980.0));
        // At the clock's reading when it began, the band's first frame.
        assert_eq!(n[0].ctrl.frequency * 10.0 + n[0].ctrl.phase, 3000.0);
        assert_eq!((n[1].ctrl.start, n[1].ctrl.stop), (0.0, 0.0), "a mesh's own controller is left alone");
    }

    #[test]
    fn groups_are_listed_with_their_windows() {
        let g = groups(&kf());
        assert_eq!(g, vec![("Idle".to_string(), 0.0, 2.0), ("Idle2".to_string(), 2.5, 4.0), ("Death1".to_string(), 6.0, 6.0)]);
    }
}
