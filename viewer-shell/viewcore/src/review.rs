//! Wraithguard: one mod's part in a cell - what it added, changed, moved and took away,
//! and the cell as it would stand with that mod left out of the load order.
//!
//! The merge (`world.rs`) keeps each reference's every version (`CellRef::touched` and
//! `poses`, oldest first) and what each plugin took out of a cell (`Cell::gone`). Leaving
//! plugin `p` out is replaying the order without it: a reference keeps its latest version
//! from anybody else; one only `p` ever supplied is not there; one `p` deleted or moved
//! away is back where it stood before.

use std::collections::HashMap;

use crate::esp::{CellRef, RefNum};
use crate::world::Gone;

/// What a reference is, relative to the plugin under review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Nothing to do with it (or its change was overridden by a later plugin).
    None,
    /// The plugin placed it.
    Added,
    /// The plugin changed it (moved it, rescaled it, swapped its base) and its version
    /// is the one standing.
    Changed,
    /// Without the plugin: the earlier version, where the plugin's change had been.
    Reverted,
    /// Without the plugin: back from a deletion or a move the plugin made.
    Restored,
}

impl Mark {
    pub fn name(self) -> &'static str {
        match self {
            Mark::None => "",
            Mark::Added => "added",
            Mark::Changed => "changed",
            Mark::Reverted => "reverted",
            Mark::Restored => "restored",
        }
    }
}

/// The references as they stand, each marked for plugin `p`.
pub fn with(refs: &HashMap<RefNum, CellRef>, p: i32) -> Vec<(CellRef, Mark)> {
    refs.values()
        .map(|r| {
            let m = if r.touched.last() != Some(&p) {
                Mark::None
            } else if r.touched.first() == Some(&p) {
                Mark::Added
            } else {
                Mark::Changed
            };
            (r.clone(), m)
        })
        .collect()
}

/// The references with plugin `p` left out. `here` says whether a position is in this
/// cell (a reference `p` moved in goes back to the cell it came from).
pub fn without(refs: &HashMap<RefNum, CellRef>, gone: &[Gone], p: i32, here: &dyn Fn(&[f32; 3]) -> bool) -> Vec<(CellRef, Mark)> {
    let mut out: Vec<(CellRef, Mark)> = Vec::new();
    for r in refs.values() {
        if r.touched.last() != Some(&p) {
            out.push((r.clone(), Mark::None));
            continue;
        }
        let n = r.touched.len();
        let Some(j) = (0..n.saturating_sub(1)).rev().find(|&j| r.touched[j] != p) else {
            continue; // only `p` ever supplied it
        };
        let Some(pose) = r.poses.get(j) else { continue };
        if !here(&pose.pos) {
            continue; // `p` moved it here; without `p` it is still where it was
        }
        let mut v = r.clone();
        v.id = pose.id.clone();
        v.pos = pose.pos;
        v.rot = pose.rot;
        v.scale = pose.scale;
        v.plugin = r.touched[j];
        v.touched.truncate(j + 1);
        v.poses.truncate(j + 1);
        out.push((v, Mark::Reverted));
    }
    for g in gone {
        if g.by != p || refs.contains_key(&g.r.num) {
            continue;
        }
        out.push((g.r.clone(), Mark::Restored));
    }
    out
}

/// Whether a position stands in exterior grid `(gx, gy)`.
pub fn in_grid(pos: &[f32; 3], gx: i32, gy: i32) -> bool {
    (pos[0] / 8192.0).floor() as i32 == gx && (pos[1] / 8192.0).floor() as i32 == gy
}

/// How far a reference has moved from where its first plugin put it (0 for one version).
pub fn moved_by(r: &CellRef) -> f32 {
    match r.poses.first() {
        Some(p) if r.poses.len() > 1 => {
            let d = [r.pos[0] - p.pos[0], r.pos[1] - p.pos[1], r.pos[2] - p.pos[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esp::Pose;

    fn r(ix: u32, touched: &[i32], xs: &[f32]) -> CellRef {
        let poses: Vec<Pose> = xs.iter().map(|&x| Pose { id: "rock".into(), pos: [x, 0., 0.], rot: [0.; 3], scale: 1.0 }).collect();
        let last = poses.last().cloned().unwrap_or_default();
        CellRef {
            num: RefNum { content_file: 0, index: ix },
            id: "rock".into(),
            pos: last.pos,
            plugin: *touched.last().unwrap(),
            touched: touched.to_vec(),
            poses,
            ..Default::default()
        }
    }

    #[test]
    fn leaving_a_mod_out_replays_the_order_without_it() {
        let mut refs = HashMap::new();
        for c in [r(1, &[0], &[10.]), r(2, &[1], &[20.]), r(3, &[0, 1], &[30., 35.]), r(4, &[0, 1, 2], &[40., 45., 48.])] {
            refs.insert(c.num, c);
        }
        let gone = vec![Gone { r: r(5, &[0], &[50.]), by: 1, moved: false }, Gone { r: r(6, &[0], &[60.]), by: 2, moved: false }];
        let marks: HashMap<u32, Mark> = with(&refs, 1).into_iter().map(|(c, m)| (c.num.index, m)).collect();
        assert_eq!((marks[&1], marks[&2], marks[&3], marks[&4]), (Mark::None, Mark::Added, Mark::Changed, Mark::None));
        let out: HashMap<u32, (f32, Mark)> = without(&refs, &gone, 1, &|_| true).into_iter().map(|(c, m)| (c.num.index, (c.pos[0], m))).collect();
        assert!(!out.contains_key(&2), "only the mod placed it");
        assert_eq!(out[&3], (30., Mark::Reverted));
        assert_eq!(out[&4], (48., Mark::None), "a later plugin's version still wins");
        assert_eq!(out[&5], (50., Mark::Restored));
        assert!(!out.contains_key(&6), "another plugin's deletion stands");
    }
}
