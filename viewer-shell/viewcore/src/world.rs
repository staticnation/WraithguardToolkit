//! Gardenfell's `world`: Robin Hjelte's, from `gardenfell-world` (viewer-shell/gardenfell,
//! vendored; carried patches 0006, 0007 and 0009), re-exported so viewcore's callers keep
//! their paths - with Wraithguard's [`World`] around his.
//!
//! The wrapper derefs to his `World`, so `w.cells`, `w.models` and the rest are his; what
//! it adds is what the Editor and the inspector read and his world does not keep: the game
//! settings, each actor's AI Hello, the path grids, where each plugin's LAND records sit,
//! the NPC assembly tables, which plugins are groundcover, and every live record per type
//! (`ordercore::records`), from one more pass over the plugins' records.
//!
//! The world is loaded with OpenMW's rule for an id defined as two kinds
//! (`LoadOpts::openmw_kinds`, patch 0007), and Greatness7's `tes3` crate types the records
//! (`gardenfell_g7`, through patch 0001's hook).

pub use gardenfell_world::world::*;

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use crate::esp::{cstring, i32le, u32le, Records, Subs, Tag};
use crate::vfs::Vfs;

/// Gardenfell's world, and what Wraithguard keeps beside it.
#[derive(Default)]
pub struct World {
    pub base: gardenfell_world::world::World,
    /// Every live record per type (`ordercore::records`): an id defined as two types is
    /// both, as OpenMW keeps them. The Object Window's tabs read this.
    pub records: ordercore::records::Index,
    /// Every id-bearing record, by lowercased id: the plugins whose version is live and
    /// the tag each used, ordered so the **last** entry is what the id resolves to (for
    /// an object, the type OpenMW places, its winning plugin; else the last loaded).
    pub record_where: HashMap<String, Vec<(usize, Tag)>>,
    /// Numeric game settings, by lowercased name (the last plugin's).
    pub gmst: HashMap<String, f32>,
    /// Each NPC's and creature's AI Hello, by lowercased id.
    pub hello: HashMap<String, u8>,
    /// The path grids: exteriors by grid, rooms by `room_key`.
    pub pathgrids_ext: HashMap<(i32, i32), Pathgrid>,
    pub pathgrids_int: HashMap<String, Pathgrid>,
    /// Per exterior grid, every plugin's LAND record: (plugin, byte offset, length).
    pub land_edits: HashMap<(i32, i32), Vec<(usize, u64, u32)>>,
    /// The races, body parts, NPCs and worn items an NPC is assembled from.
    pub npc: crate::npc::NpcTables,
    /// Which plugins (load-order indices) are `groundcover=` ones.
    pub groundcover: HashSet<usize>,
}

impl Deref for World {
    type Target = gardenfell_world::world::World;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl DerefMut for World {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl World {
    /// Robin's `World::load_with` with OpenMW's kind rule and G7's reader, then
    /// Wraithguard's pass over the same plugins.
    pub fn load_with(paths: &[PathBuf], vfs: &Vfs, want_land: bool, opts: &LoadOpts) -> World {
        crate::init(); // already done at startup in the apps; tests start here
        let opts = LoadOpts { openmw_kinds: true, ..opts.clone() };
        let base = gardenfell_world::world::World::load_with(paths, vfs, want_land, &opts);
        let mut w = World { base, ..Default::default() };
        w.add_extras(paths);
        w
    }

    /// The records' extras, every plugin read once more (in parallel), then merged in
    /// load order: the later plugin wins.
    fn add_extras(&mut self, paths: &[PathBuf]) {
        let per: Vec<Extras> = crate::pool::par_map(paths, |_, p| std::fs::read(p).map(|b| extras_of(&b)).unwrap_or_default());
        self.records = ordercore::records::Index::build(per.iter().map(|e| e.ids.iter().cloned()));
        for (pi, e) in per.into_iter().enumerate() {
            self.npc.merge(e.npc);
            for (k, v) in e.gmst {
                self.gmst.insert(k, v);
            }
            for (k, v) in e.hello {
                self.hello.insert(k, v);
            }
            for mut p in e.pathgrids {
                p.plugin = pi as i32;
                if p.grid == (0, 0) && !p.cell.is_empty() {
                    self.pathgrids_int.insert(room_key(&p.cell), p.clone());
                }
                self.pathgrids_ext.insert(p.grid, p);
            }
            for (g, at, len) in e.land_at {
                self.land_edits.entry(g).or_default().push((pi, at, len));
            }
        }
        for (tag, id, v) in self.records.iter() {
            self.record_where.entry(id.to_string()).or_default().extend(v.plugins.iter().map(|&p| (p, *tag)));
        }
        for (id, defs) in self.record_where.iter_mut() {
            let wins = self.records.placed_type(id);
            // The placed type last; otherwise load order, then the tag, for a stable order.
            defs.sort_by_key(|(p, t)| (wins == Some(*t), *p, ordercore::records::placed_rank(t)));
        }
    }
}

/// What one plugin adds beside Robin's world.
#[derive(Default)]
struct Extras {
    ids: Vec<ordercore::records::Entry>,
    npc: crate::npc::NpcRaw,
    gmst: Vec<(String, f32)>,
    hello: Vec<(String, u8)>,
    pathgrids: Vec<Pathgrid>,
    land_at: Vec<((i32, i32), u64, u32)>,
}

fn extras_of(buf: &[u8]) -> Extras {
    let mut e = Extras::default();
    let mut recs = Records::new(buf);
    while let Some(rec) = recs.next_filtered(&|_| true) {
        if rec.tag != *b"TES3" {
            if let Some(id) = crate::objects::record_id(rec.tag, rec.body) {
                let deleted = Subs::new(rec.body).any(|s| &s.tag == b"DELE");
                e.ids.push(ordercore::records::Entry { tag: rec.tag, id, deleted });
            }
        }
        crate::npc::collect(&mut e.npc, rec.tag, rec.body);
        match &rec.tag {
            b"LAND" => {
                let at = rec.body.as_ptr() as usize - buf.as_ptr() as usize - 16;
                for s in Subs::new(rec.body) {
                    if &s.tag == b"INTV" && s.data.len() >= 8 {
                        e.land_at.push(((i32le(s.data, 0), i32le(s.data, 4)), at as u64, (16 + rec.body.len()) as u32));
                        break;
                    }
                }
            }
            b"PGRD" => {
                if let Some(p) = decode_pgrd(rec.body, -1) {
                    e.pathgrids.push(p);
                }
            }
            b"GMST" => {
                if let Some(g) = gmst_number(rec.body) {
                    e.gmst.push(g);
                }
            }
            b"NPC_" | b"CREA" => {
                if let Some(h) = actor_hello(rec.body) {
                    e.hello.push(h);
                }
            }
            _ => {}
        }
    }
    e
}

/// The plugins an openmw.cfg (and the cfgs it chains) loads on `groundcover=` lines, in
/// order: Robin's chain reader's answer.
pub fn read_groundcover(cfg: &Path) -> Vec<String> {
    gardenfell_bytes::openmwcfg::read(cfg).groundcover.into_iter().filter(|n| is_plugin_name(n)).collect()
}

/// Wraithguard: a path grid (PGRD) - its points, in the cell's own space for an exterior
/// (the game adds the cell's corner) and the world's for a room, and the edges between.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pathgrid {
    pub grid: (i32, i32),
    pub cell: String,
    pub points: Vec<[i32; 3]>,
    pub edges: Vec<(u32, u32)>,
    pub plugin: i32,
}

/// A PGRD record's body.
pub fn decode_pgrd(body: &[u8], plugin_idx: i32) -> Option<Pathgrid> {
    let mut p = Pathgrid { plugin: plugin_idx, ..Default::default() };
    let mut conns: Vec<u8> = Vec::new();
    let mut targets: Vec<u32> = Vec::new();
    let mut deleted = false;
    for s in Subs::new(body) {
        match &s.tag {
            b"DATA" if s.data.len() >= 8 => p.grid = (i32le(s.data, 0), i32le(s.data, 4)),
            b"NAME" => p.cell = cstring(s.data),
            b"DELE" => deleted = true,
            b"PGRP" => {
                for c in s.data.chunks_exact(16) {
                    p.points.push([i32le(c, 0), i32le(c, 4), i32le(c, 8)]);
                    conns.push(c[13]);
                }
            }
            b"PGRC" => {
                for c in s.data.chunks_exact(4) {
                    targets.push(u32le(c, 0));
                }
            }
            _ => {}
        }
    }
    if deleted {
        return None;
    }
    let mut at = 0usize;
    let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for (i, n) in conns.iter().enumerate() {
        for _ in 0..*n {
            // Each connection is listed from both ends; kept once, low index first.
            if let Some(&t) = targets.get(at) {
                let i = i as u32;
                if (t as usize) < p.points.len() && t != i {
                    let e = (i.min(t), i.max(t));
                    if seen.insert(e) {
                        p.edges.push(e);
                    }
                }
            }
            at += 1;
        }
    }
    Some(p)
}

/// A GMST's number: its INTV or FLTV (`None` for a string setting).
fn gmst_number(body: &[u8]) -> Option<(String, f32)> {
    let mut name = String::new();
    let mut val = None;
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => name = cstring(s.data),
            b"INTV" if s.data.len() >= 4 => val = Some(i32le(s.data, 0) as f32),
            b"FLTV" if s.data.len() >= 4 => val = Some(f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]])),
            _ => {}
        }
    }
    if name.is_empty() { None } else { val.map(|v| (name.to_ascii_lowercase(), v)) }
}

/// An NPC's or creature's id and its AI Hello (AIDT's first byte).
fn actor_hello(body: &[u8]) -> Option<(String, u8)> {
    let mut id = String::new();
    let mut hello = None;
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"AIDT" if !s.data.is_empty() => hello = Some(s.data[0]),
            _ => {}
        }
    }
    if id.is_empty() { None } else { hello.map(|h| (id.to_ascii_lowercase(), h)) }
}

#[cfg(test)]
mod wraithguard_world_tests {
    /// A record as a plugin file holds it: tag, size, header words, then subrecords.
    fn record(tag: &[u8; 4], subs: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (t, d) in subs {
            body.extend_from_slice(*t);
            body.extend_from_slice(&(d.len() as u32).to_le_bytes());
            body.extend_from_slice(d);
        }
        let mut out = tag.to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend(body);
        out
    }

    #[test]
    fn an_id_of_two_kinds_is_both_in_the_index_and_placed_as_openmw_places_it() {
        // A.esp has `wg_clash` as a weapon; B.esp, later, as a misc item.
        let dir = std::env::temp_dir().join(format!("wg_world_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut hedr = vec![0u8; 300];
        hedr[..4].copy_from_slice(&1.3f32.to_le_bytes());
        let header = record(b"TES3", &[(b"HEDR", &hedr)]);
        let a = [header.clone(), record(b"WEAP", &[(b"NAME", b"wg_clash\0"), (b"MODL", b"w\\sword.nif\0")])].concat();
        let b = [header, record(b"MISC", &[(b"NAME", b"WG_Clash\0"), (b"MODL", b"m\\cup.nif\0")])].concat();
        let (pa, pb) = (dir.join("A.esp"), dir.join("B.esp"));
        std::fs::write(&pa, a).unwrap();
        std::fs::write(&pb, b).unwrap();
        let w = super::World::load_with(&[pa, pb], &crate::vfs::Vfs::default(), false, &super::LoadOpts::default());
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(w.models.get("wg_clash").map(String::as_str), Some("w\\sword.nif"));
        assert_eq!(w.records.placed_type("wg_clash"), Some(*b"WEAP"));
        assert_eq!(w.records.get(b"MISC", "wg_clash").map(|v| v.winner()), Some(1));
        assert_eq!(w.record_where["wg_clash"].last(), Some(&(0, *b"WEAP")));
    }
}
