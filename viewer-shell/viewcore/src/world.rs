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
    /// Sound records (SOUN) by lowercased id: the file and DATA's volume and ranges.
    pub sounds: HashMap<String, SoundDef>,
    /// A light's (LIGH SNAM) looped sound, by the light's lowercased id: the SOUN id.
    pub light_sounds: HashMap<String, String>,
    /// A region's ambient sounds (REGN SNAM), by lowercased region id: (SOUN id, chance).
    pub region_sounds: HashMap<String, Vec<(String, u8)>>,
    /// Each region (REGN): its id as written, its name (FNAM) and map color (CNAM), by
    /// lowercased id - the cell map's regions.
    pub region_defs: HashMap<String, RegionDef>,
    /// The sound a script loops on its own object (`PlayLoopSound3D "id"`, or
    /// `PlayLoopSound3DVP "id", volume, pitch`), by lowercased script id: (SOUN id, volume).
    pub script_loops: HashMap<String, (String, f32)>,
    /// An activator's, door's or container's script (SCRI), by the object's lowercased id.
    pub object_scripts: HashMap<String, String>,
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
        /* `geometry: false`: nothing in Wraithguard reads the world's collision geometry
           (`World::geom` is for Gardenfell's grass, which Wraithguard does not grow; the
           collision overlay and the meshes read their files themselves), and reading it
           was 39-43 s of every connect on a big setup. */
        let opts = LoadOpts { openmw_kinds: true, geometry: false, ..opts.clone() };
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
            // The later plugin wins; a deleting one takes the record away.
            for (k, v) in e.sounds {
                match v {
                    Some(d) => self.sounds.insert(k, d),
                    None => self.sounds.remove(&k),
                };
            }
            for (k, v) in e.light_sounds {
                match v {
                    Some(d) => self.light_sounds.insert(k, d),
                    None => self.light_sounds.remove(&k),
                };
            }
            for (k, v) in e.region_sounds {
                match v {
                    Some(d) => self.region_sounds.insert(k, d),
                    None => self.region_sounds.remove(&k),
                };
            }
            for (k, v) in e.region_defs {
                match v {
                    Some(d) => self.region_defs.insert(k, d),
                    None => self.region_defs.remove(&k),
                };
            }
            for (k, v) in e.script_loops {
                match v {
                    Some(d) => self.script_loops.insert(k, d),
                    None => self.script_loops.remove(&k),
                };
            }
            for (k, v) in e.object_scripts {
                match v {
                    Some(d) => self.object_scripts.insert(k, d),
                    None => self.object_scripts.remove(&k),
                };
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
    sounds: Vec<(String, Option<SoundDef>)>,
    light_sounds: Vec<(String, Option<String>)>,
    region_sounds: Vec<(String, Option<Vec<(String, u8)>>)>,
    region_defs: Vec<(String, Option<RegionDef>)>,
    script_loops: Vec<(String, Option<(String, f32)>)>,
    object_scripts: Vec<(String, Option<String>)>,
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
            b"SOUN" => {
                if let Some(x) = sound_def(rec.body) {
                    e.sounds.push(x);
                }
            }
            b"LIGH" => {
                if let Some(x) = light_sound(rec.body) {
                    e.light_sounds.push(x);
                }
            }
            b"REGN" => {
                if let Some(x) = region_sounds(rec.body) {
                    e.region_sounds.push(x);
                }
                if let Some(x) = region_def(rec.body) {
                    e.region_defs.push(x);
                }
            }
            b"SCPT" => {
                if let Some(x) = script_loop(rec.body) {
                    e.script_loops.push(x);
                }
            }
            b"ACTI" | b"DOOR" | b"CONT" => {
                if let Some(x) = object_script(rec.body) {
                    e.object_scripts.push(x);
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

/// A Sound record: its file (relative to `Sound/`) and DATA's volume (0-255) and its
/// minimum and maximum ranges (0-255, scaled by the `fAudio*` settings as OpenMW does).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundDef {
    pub file: String,
    pub volume: u8,
    pub min: u8,
    pub max: u8,
}

fn deleted_of(body: &[u8]) -> bool {
    Subs::new(body).any(|s| &s.tag == b"DELE")
}

/// A SOUN record's id and what it is (`None` when it is deleted).
fn sound_def(body: &[u8]) -> Option<(String, Option<SoundDef>)> {
    let mut id = String::new();
    let mut d = SoundDef::default();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"FNAM" => d.file = cstring(s.data),
            b"DATA" if s.data.len() >= 3 => (d.volume, d.min, d.max) = (s.data[0], s.data[1], s.data[2]),
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    Some((id.to_ascii_lowercase(), (!deleted_of(body)).then_some(d)))
}

/// A LIGH record's id and the Sound it loops (`None`: deleted, or no sound).
fn light_sound(body: &[u8]) -> Option<(String, Option<String>)> {
    let mut id = String::new();
    let mut snd = String::new();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"SNAM" => snd = cstring(s.data),
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    Some((id.to_ascii_lowercase(), (!deleted_of(body) && !snd.is_empty()).then(|| snd.to_ascii_lowercase())))
}

/// A region as the cell map shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionDef {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
}

/// A REGN record's id, name and map color (`None` when it is deleted).
fn region_def(body: &[u8]) -> Option<(String, Option<RegionDef>)> {
    let mut d = RegionDef::default();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => d.id = cstring(s.data),
            b"FNAM" => d.name = cstring(s.data),
            b"CNAM" if s.data.len() >= 3 => d.color = [s.data[0], s.data[1], s.data[2]],
            _ => {}
        }
    }
    if d.id.is_empty() {
        return None;
    }
    Some((d.id.to_ascii_lowercase(), (!deleted_of(body)).then_some(d)))
}

/// A REGN record's id and its ambient sounds: SNAM is 32 bytes of name and a chance (0-100).
fn region_sounds(body: &[u8]) -> Option<(String, Option<Vec<(String, u8)>>)> {
    let mut id = String::new();
    let mut list = Vec::new();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"SNAM" if s.data.len() >= 33 => {
                let n = cstring(&s.data[..32]);
                if !n.is_empty() {
                    list.push((n.to_ascii_lowercase(), s.data[32]));
                }
            }
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    Some((id.to_ascii_lowercase(), (!deleted_of(body)).then_some(list)))
}

/// A SCPT record's id and the sound it loops on its own object (`None`: deleted, or none).
fn script_loop(body: &[u8]) -> Option<(String, Option<(String, f32)>)> {
    let mut id = String::new();
    let mut text = String::new();
    for s in Subs::new(body) {
        match &s.tag {
            b"SCHD" if s.data.len() >= 32 => id = cstring(&s.data[..32]),
            b"SCTX" => text = String::from_utf8_lossy(s.data).into_owned(),
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    let found = if deleted_of(body) { None } else { loop_in_script(&text) };
    Some((id.to_ascii_lowercase(), found))
}

/// The first `PlayLoopSound3D` (or `...VP`) a script's text calls on its own object: not
/// on a line that names another (`x->PlayLoopSound3D`), not in a `;` comment. The id quoted
/// or bare; VP's volume when it is a number (else 1).
pub fn loop_in_script(text: &str) -> Option<(String, f32)> {
    for raw in text.lines() {
        let line = raw.split(';').next().unwrap_or("");
        let low = line.to_ascii_lowercase();
        let Some(at) = low.find("playloopsound3d") else { continue };
        if low[..at].contains("->") {
            continue;
        }
        let mut rest = &line[at + "playloopsound3d".len()..];
        let vp = rest.len() >= 2 && rest[..2].eq_ignore_ascii_case("vp");
        if vp {
            rest = &rest[2..];
        }
        if !rest.starts_with(|c: char| c.is_whitespace() || c == ',' || c == '"') {
            continue;
        }
        let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        let (sound, after) = if let Some(q) = rest.strip_prefix('"') {
            let end = q.find('"')?;
            (&q[..end], &q[end + 1..])
        } else {
            let end = rest.find(|c: char| c.is_whitespace() || c == ',').unwrap_or(rest.len());
            (&rest[..end], &rest[end..])
        };
        if sound.is_empty() {
            continue;
        }
        let vol = if vp {
            after
                .split(|c: char| c.is_whitespace() || c == ',')
                .find(|t| !t.is_empty())
                .and_then(|t| t.parse::<f32>().ok())
                .unwrap_or(1.0)
        } else {
            1.0
        };
        return Some((sound.to_ascii_lowercase(), vol));
    }
    None
}

/// An object's id and its script (`None`: deleted, or no script).
fn object_script(body: &[u8]) -> Option<(String, Option<String>)> {
    let mut id = String::new();
    let mut scri = String::new();
    for s in Subs::new(body) {
        match &s.tag {
            b"NAME" => id = cstring(s.data),
            b"SCRI" => scri = cstring(s.data),
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    Some((id.to_ascii_lowercase(), (!deleted_of(body) && !scri.is_empty()).then(|| scri.to_ascii_lowercase())))
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

    #[test]
    fn sounds_lights_and_regions_keep_the_last_plugin_s_word() {
        let dir = std::env::temp_dir().join(format!("wg_world_snd_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut hedr = vec![0u8; 300];
        hedr[..4].copy_from_slice(&1.3f32.to_le_bytes());
        let header = record(b"TES3", &[(b"HEDR", &hedr)]);
        let mut snam = vec![0u8; 33];
        snam[..6].copy_from_slice(b"Crickt");
        snam[32] = 40;
        let a = [
            header.clone(),
            record(b"SOUN", &[(b"NAME", b"Fire\0"), (b"FNAM", b"Fx\\envrn\\fire.wav\0"), (b"DATA", &[200, 0, 0])]),
            record(b"SOUN", &[(b"NAME", b"Gone\0"), (b"FNAM", b"x.wav\0"), (b"DATA", &[1, 2, 3])]),
            record(b"LIGH", &[(b"NAME", b"Torch_01\0"), (b"SNAM", b"Fire\0")]),
            record(b"REGN", &[(b"NAME", b"Ascadian Isles Region\0"), (b"SNAM", &snam)]),
        ]
        .concat();
        let b = [
            header,
            record(b"SOUN", &[(b"NAME", b"fire\0"), (b"FNAM", b"Fx\\envrn\\fire2.wav\0"), (b"DATA", &[255, 10, 30])]),
            record(b"SOUN", &[(b"NAME", b"Gone\0"), (b"DELE", &[0, 0, 0, 0])]),
        ]
        .concat();
        let (pa, pb) = (dir.join("A.esp"), dir.join("B.esp"));
        std::fs::write(&pa, a).unwrap();
        std::fs::write(&pb, b).unwrap();
        let w = super::World::load_with(&[pa, pb], &crate::vfs::Vfs::default(), false, &super::LoadOpts::default());
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(w.sounds["fire"], super::SoundDef { file: "Fx\\envrn\\fire2.wav".into(), volume: 255, min: 10, max: 30 });
        assert!(!w.sounds.contains_key("gone"));
        assert_eq!(w.light_sounds["torch_01"], "fire");
        assert_eq!(w.region_sounds["ascadian isles region"], vec![("crickt".to_string(), 40)]);
        assert_eq!(w.region_defs["ascadian isles region"].id, "Ascadian Isles Region");
    }

    #[test]
    fn a_script_s_own_loop_is_found_and_others_are_not() {
        use super::loop_in_script as l;
        assert_eq!(l("begin x\nif ( GetSoundPlaying \"Water Layer\" == 0 )\n  PlayLoopSound3D \"Water Layer\"\nendif"), Some(("water layer".into(), 1.0)));
        assert_eq!(l("PlayLoopSound3DVP, \"fire\", 0.5, 1.0"), Some(("fire".into(), 0.5)));
        assert_eq!(l("playloopsound3d steam"), Some(("steam".into(), 1.0)));
        assert_eq!(l("; PlayLoopSound3D \"fire\"\nother_ref->PlayLoopSound3D \"fire\""), None);
        assert_eq!(l("PlayLoopSound3DX \"fire\""), None);
    }
}
