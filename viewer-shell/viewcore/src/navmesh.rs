//! Wraithguard: OpenMW's navigation mesh cache, `navmesh.db`, read for drawing over cells.
//!
//! `openmw-navmeshtool` (and the game, as it plays) keeps the navigator's built tiles in
//! an SQLite file in OpenMW's user data folder - beside `openmw.cfg` on Windows, under
//! `~/.local/share/openmw` on Linux. This reads it to show what the navigator built, so
//! path grids can be placed and checked against it.
//!
//! Licence: OpenMW is GPL-3; nothing here is taken from its code. What is used is the
//! file's layout (facts about the data, worked out from the bytes of a real database and
//! the published SQLite schema): every blob is an 8-byte little-endian length and an LZ4
//! block (decoded below, from the LZ4 block format's public specification); `tiles.input`
//! starts `"rcst"`, a version, the build settings and the agent's bounds; `tiles.data`
//! starts `"pnav"`, a version, then Recast's poly mesh (Recast is zlib-licensed) and its
//! detail mesh. Only the poly mesh is read.
//!
//! Coordinates: the navigator works in Recast's space, Y up and scaled. A vertex is
//! `bmin + v * (cs, ch, cs)`; divided by the scale factor, with Y and Z swapped back, it
//! is a world position. Exterior tiles are in `sys::default`; an interior's worldspace is
//! its cell name, lower-cased.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};

/// The exterior worldspace's name in the database.
pub const EXTERIOR: &str = "sys::default";

/// Polygon flags the navigator sets.
pub const FLAG_WALK: u16 = 1;
pub const FLAG_SWIM: u16 = 2;
pub const FLAG_DOOR: u16 = 4;
pub const FLAG_PATHGRID: u16 = 8;

/// No polygon past this many in one answer: a 49-cell load stays drawable.
const MAX_POLYS: usize = 600_000;

/// Decodes one LZ4 block (no frame) that should come to `size` bytes.
pub fn lz4_block(src: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(size);
    let mut i = 0usize;
    let len_ext = |i: &mut usize, mut n: usize| -> Option<usize> {
        loop {
            let b = *src.get(*i)?;
            *i += 1;
            n += b as usize;
            if b != 255 {
                return Some(n);
            }
        }
    };
    while i < src.len() {
        let token = src[i];
        i += 1;
        let mut lit = (token >> 4) as usize;
        if lit == 15 {
            lit = len_ext(&mut i, lit)?;
        }
        out.extend_from_slice(src.get(i..i + lit)?);
        i += lit;
        if i >= src.len() {
            break; // the last sequence is literals only
        }
        let off = u16::from_le_bytes([*src.get(i)?, *src.get(i + 1)?]) as usize;
        i += 2;
        let mut m = (token & 15) as usize;
        if m == 15 {
            m = len_ext(&mut i, m)?;
        }
        m += 4;
        if off == 0 || off > out.len() {
            return None;
        }
        let start = out.len() - off;
        for k in 0..m {
            let b = out[start + k];
            out.push(b);
        }
        if out.len() > size {
            return None;
        }
    }
    (out.len() == size).then_some(out)
}

/// A blob as stored: its unpacked length (u64 LE) and the LZ4 block.
pub fn unpack(blob: &[u8]) -> Option<Vec<u8>> {
    let n = u64::from_le_bytes(blob.get(..8)?.try_into().ok()?);
    if n > 64 << 20 {
        return None;
    }
    lz4_block(&blob[8..], n as usize)
}

struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u16s(&mut self, n: usize) -> Option<Vec<u16>> {
        let s = self.take(n.checked_mul(2)?)?;
        Some(s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect())
    }
    fn count(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok().filter(|n| *n < 1 << 24)
    }
}

/// What a tile was built with: the settings the drawing needs, and the agent.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub cell_size: f32,
    pub cell_height: f32,
    pub scale: f32,
    pub tile_size: i32,
    /// The agent's collision shape (0 box, 1 rotating box, 2 cylinder) and half extents.
    pub agent_shape: u8,
    pub agent: [f32; 3],
}

impl Input {
    /// A tile's width in world units.
    pub fn tile_world(&self) -> f32 {
        self.tile_size as f32 * self.cell_size / self.scale
    }
    /// The tile holding world coordinate `v` (X or Y).
    pub fn tile_of(&self, v: f32) -> i32 {
        (v / self.tile_world()).floor() as i32
    }
}

/// Reads an unpacked `tiles.input`: `"rcst"`, version 2, nine f32 settings (cell height,
/// cell size, detail sample distance and error, max climb, simplification error, max
/// slope, scale factor, swim height scale), six i32 (border, max edge length, verts per
/// polygon, region merge and min area, tile size), then the agent: u8 shape, 3 f32.
pub fn read_input(b: &[u8]) -> Option<Input> {
    let mut c = Cur { b, p: 0 };
    if c.take(4)? != b"rcst" || c.u32()? != 2 {
        return None;
    }
    let mut f = [0f32; 9];
    for v in &mut f {
        *v = c.f32()?;
    }
    let mut n = [0i32; 6];
    for v in &mut n {
        *v = c.i32()?;
    }
    let agent_shape = c.u8()?;
    let agent = [c.f32()?, c.f32()?, c.f32()?];
    let i = Input { cell_height: f[0], cell_size: f[1], scale: f[7], tile_size: n[5], agent_shape, agent };
    (i.scale > 0.0 && i.cell_size > 0.0 && i.tile_size > 0).then_some(i)
}

/// One navmesh polygon, in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub verts: Vec<[f32; 3]>,
    pub flags: u16,
    pub area: u8,
}

/// Reads an unpacked `tiles.data`: `"pnav"`, version 1, user id, cell size and height,
/// then the poly mesh - nverts, npolys, maxpolys, nvp (i32); bmin, bmax (3 f32 each); cs,
/// ch (f32); border (i32); max edge error (f32); verts (u16, 3 per vertex); polys (u16,
/// 2 * nvp per polygon: vertex indices then neighbours, 0xffff unused); regions (u16 per
/// polygon); flags (u16 per used polygon); areas (u8 per polygon). `scale` is the
/// settings' factor from Recast space back to world units.
pub fn read_polys(b: &[u8], scale: f32) -> Option<Vec<Poly>> {
    let mut c = Cur { b, p: 0 };
    if c.take(4)? != b"pnav" || c.u32()? != 1 {
        return None;
    }
    c.take(12)?; // user id, cell size, cell height
    let nverts = c.count()?;
    let npolys = c.count()?;
    let maxpolys = c.count()?;
    let nvp = c.count()?;
    if npolys > maxpolys || nvp == 0 || nvp > 32 {
        return None;
    }
    let bmin = [c.f32()?, c.f32()?, c.f32()?];
    c.take(12)?; // bmax
    let cs = c.f32()?;
    let ch = c.f32()?;
    c.take(8)?; // border, max edge error
    let verts = c.u16s(nverts * 3)?;
    let polys = c.u16s(maxpolys * nvp * 2)?;
    c.u16s(maxpolys)?; // regions
    let flags = c.u16s(npolys)?;
    let areas = c.take(maxpolys)?;
    let world = |k: usize| -> Option<[f32; 3]> {
        let v = verts.get(k * 3..k * 3 + 3)?;
        let x = bmin[0] + v[0] as f32 * cs;
        let y = bmin[1] + v[1] as f32 * ch;
        let z = bmin[2] + v[2] as f32 * cs;
        Some([x / scale, z / scale, y / scale])
    };
    let mut out = Vec::with_capacity(npolys);
    for p in 0..npolys {
        let idx = &polys[p * nvp * 2..p * nvp * 2 + nvp];
        let vs: Option<Vec<[f32; 3]>> =
            idx.iter().take_while(|i| **i != 0xffff).map(|i| world(*i as usize)).collect();
        let vs = vs?;
        if vs.len() >= 3 {
            out.push(Poly { verts: vs, flags: flags[p], area: areas[p] });
        }
    }
    Some(out)
}

/* ---- finding the file ------------------------------------------------------------- */

/// OpenMW's user data folder for this platform (where the game and navmeshtool put
/// `navmesh.db` unless `user-data=` says otherwise). Linux: XDG first, then a Flatpak's.
pub fn user_data_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Some(h) = std::env::var_os("USERPROFILE") {
            out.push(PathBuf::from(h).join("Documents").join("My Games").join("OpenMW"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = std::env::var_os("HOME") {
            out.push(PathBuf::from(h).join("Library").join("Application Support").join("openmw"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        match std::env::var_os("XDG_DATA_HOME") {
            Some(x) => out.push(PathBuf::from(x).join("openmw")),
            None => {
                if let Some(h) = &home {
                    out.push(h.join(".local").join("share").join("openmw"));
                }
            }
        }
        if let Some(h) = &home {
            out.push(h.join(".var").join("app").join("org.openmw.OpenMW").join("data").join("openmw"));
        }
    }
    out
}

/// The `user-data=` folder a cfg names (the last line wins), its `?userdata?` and
/// `?userconfig?` tokens filled in, a relative path taken from the cfg's folder.
pub fn cfg_user_data(cfg_text: &str, cfg_dir: &Path) -> Option<PathBuf> {
    let line = cfg_text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            (k.trim() == "user-data").then(|| v.trim().to_string())
        })
        .next_back()?;
    let v = line.trim_matches('"').replace("&&", "&").replace("&\"", "\"");
    if v.is_empty() {
        return None;
    }
    let tok = |t: &str, dir: Option<PathBuf>| -> Option<Option<PathBuf>> {
        let rest = v.strip_prefix(t)?;
        Some(dir.map(|d| d.join(rest.trim_start_matches(['/', '\\']))))
    };
    if let Some(p) = tok("?userdata?", user_data_dirs().into_iter().next()) {
        return p;
    }
    if let Some(p) = tok("?userconfig?", crate::profiles::openmw_cfg().and_then(|c| c.parent().map(Path::to_path_buf))) {
        return p;
    }
    if v.starts_with('?') {
        return None; // ?local? / ?global?: the install's folders, not known here
    }
    let p = PathBuf::from(&v);
    Some(if p.is_absolute() { p } else { cfg_dir.join(p) })
}

/// Where to look for `navmesh.db`, in order: a path the user chose; the cfg's
/// `user-data=`; the cfg's own folder; OpenMW's user data folder; the folder of the
/// machine's own `openmw.cfg` (Wraithguard launches the viewer with a cfg it writes).
pub fn candidates(chosen: Option<&Path>, cfg: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        let p = if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("db")) { p } else { p.join("navmesh.db") };
        if !out.contains(&p) {
            out.push(p);
        }
    };
    if let Some(c) = chosen {
        add(c.to_path_buf());
    }
    if let Some(c) = cfg {
        let dir = c.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Ok(t) = std::fs::read_to_string(c) {
            if let Some(u) = cfg_user_data(&t, &dir) {
                add(u);
            }
        }
        add(dir);
    }
    for d in user_data_dirs() {
        add(d);
    }
    if let Some(dir) = crate::profiles::openmw_cfg().and_then(|c| c.parent().map(Path::to_path_buf)) {
        add(dir);
    }
    out
}

/* ---- reading tiles ---------------------------------------------------------------- */

/// An open `navmesh.db`, read-only (the game may have it open too).
pub struct NavDb {
    pub path: PathBuf,
    conn: Connection,
}

/// Polygons for some cells, the agents they were built for, and where they came from.
#[derive(Default)]
pub struct Found {
    pub agents: Vec<(u8, [f32; 3])>,
    /// Index into `agents`, and the polygon.
    pub polys: Vec<(u8, Poly)>,
    pub tiles: usize,
    pub skipped: usize,
    pub capped: bool,
}

impl NavDb {
    pub fn open(path: &Path) -> Result<NavDb, String> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        let ok: Option<String> = conn
            .query_row("SELECT name FROM sqlite_master WHERE type='table' AND name='tiles'", [], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if ok.is_none() {
            return Err(format!("{}: not a navmesh database (no tiles table)", path.display()));
        }
        Ok(NavDb { path: path.to_path_buf(), conn })
    }

    /// The first of `candidates` that exists and opens.
    pub fn find(cands: &[PathBuf]) -> Result<NavDb, String> {
        for c in cands {
            if c.is_file() {
                return NavDb::open(c);
            }
        }
        let list: Vec<String> = cands.iter().map(|p| p.display().to_string()).collect();
        Err(format!(
            "No navmesh.db found. Run openmw-navmeshtool (or play with the navigator's disk cache on), or choose the file. Looked in: {}",
            list.join("; ")
        ))
    }

    /// Any tile's build settings in a worldspace (they are the same for every tile).
    pub fn settings(&self, worldspace: &str) -> Result<Option<Input>, String> {
        let blob: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT input FROM tiles WHERE worldspace = ?1 LIMIT 1", [worldspace], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(blob.and_then(|b| unpack(&b)).and_then(|b| read_input(&b)))
    }

    /// Every tile of `worldspace` in the inclusive tile range, every agent.
    pub fn read(&self, worldspace: &str, x: (i32, i32), y: (i32, i32), into: &mut Found) -> Result<(), String> {
        let mut st = self
            .conn
            .prepare_cached(
                "SELECT input, data FROM tiles WHERE worldspace = ?1 \
                 AND tile_position_x BETWEEN ?2 AND ?3 AND tile_position_y BETWEEN ?4 AND ?5",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(rusqlite::params![worldspace, x.0, x.1, y.0, y.1], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (inp, data) = row.map_err(|e| e.to_string())?;
            let Some(input) = unpack(&inp).and_then(|b| read_input(&b)) else {
                into.skipped += 1;
                continue;
            };
            let Some(polys) = unpack(&data).and_then(|b| read_polys(&b, input.scale)) else {
                into.skipped += 1;
                continue;
            };
            into.tiles += 1;
            let key = (input.agent_shape, input.agent);
            let ai = match into.agents.iter().position(|a| *a == key) {
                Some(i) => i,
                None => {
                    into.agents.push(key);
                    into.agents.len() - 1
                }
            };
            let ai = u8::try_from(ai).unwrap_or(u8::MAX);
            for p in polys {
                if into.polys.len() >= MAX_POLYS {
                    into.capped = true;
                    return Ok(());
                }
                into.polys.push((ai, p));
            }
        }
        Ok(())
    }

    /// The polygons over cells given as `"x,y"` (exterior grid) or `"int:Name"`. A tile
    /// straddling two loaded cells is read once.
    pub fn cells(&self, specs: &[String]) -> Result<Found, String> {
        let mut f = Found::default();
        let ext = self.settings(EXTERIOR)?;
        let mut tiles: Vec<(i32, i32)> = Vec::new();
        for spec in specs {
            if let Some(name) = spec.strip_prefix("int:") {
                if !f.capped {
                    self.read(&crate::world::room_key(name), (i32::MIN, i32::MAX), (i32::MIN, i32::MAX), &mut f)?;
                }
                continue;
            }
            let mut it = spec.split(',').map(|v| v.trim().parse::<i32>());
            let (Some(Ok(gx)), Some(Ok(gy))) = (it.next(), it.next()) else { continue };
            let Some(s) = &ext else { continue };
            let span = |g: i32| (s.tile_of(g as f32 * 8192.0), s.tile_of((g + 1) as f32 * 8192.0 - 0.5));
            let (x, y) = (span(gx), span(gy));
            for tx in x.0..=x.1 {
                for ty in y.0..=y.1 {
                    if !tiles.contains(&(tx, ty)) {
                        tiles.push((tx, ty));
                    }
                }
            }
        }
        for (tx, ty) in tiles {
            if f.capped {
                break;
            }
            self.read(EXTERIOR, (tx, tx), (ty, ty), &mut f)?;
        }
        Ok(f)
    }
}

/// The answer the viewer draws:
///
/// ```text
/// "GDNM" | u8 version 1 | u16 len, db path (UTF-8) | u32 tiles | u32 skipped | u8 capped
///        | u8 agents, per agent: u8 shape, 3 f32 half extents
///        | u32 polys, per polygon: u8 agent, u8 area, u16 flags, u8 n, n * 3 f32 (world)
/// ```
pub fn encode(path: &Path, f: &Found) -> Vec<u8> {
    let mut o = b"GDNM".to_vec();
    o.push(1);
    let p = path.display().to_string();
    let pb = &p.as_bytes()[..p.len().min(65535)];
    o.extend_from_slice(&(pb.len() as u16).to_le_bytes());
    o.extend_from_slice(pb);
    o.extend_from_slice(&(f.tiles as u32).to_le_bytes());
    o.extend_from_slice(&(f.skipped as u32).to_le_bytes());
    o.push(u8::from(f.capped));
    o.push(f.agents.len().min(255) as u8);
    for (s, h) in f.agents.iter().take(255) {
        o.push(*s);
        for v in h {
            o.extend_from_slice(&v.to_le_bytes());
        }
    }
    o.extend_from_slice(&(f.polys.len() as u32).to_le_bytes());
    for (a, p) in &f.polys {
        o.push(*a);
        o.push(p.area);
        o.extend_from_slice(&p.flags.to_le_bytes());
        let n = p.verts.len().min(255);
        o.push(n as u8);
        for v in &p.verts[..n] {
            for c in v {
                o.extend_from_slice(&c.to_le_bytes());
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packs bytes as an all-literal LZ4 block (valid, if not small).
    fn literal_block(data: &[u8]) -> Vec<u8> {
        let mut o = Vec::new();
        let n = data.len();
        if n >= 15 {
            o.push(0xf0);
            let mut r = n - 15;
            while r >= 255 {
                o.push(255);
                r -= 255;
            }
            o.push(r as u8);
        } else {
            o.push((n as u8) << 4);
        }
        o.extend_from_slice(data);
        o
    }

    fn blob(data: &[u8]) -> Vec<u8> {
        let mut o = (data.len() as u64).to_le_bytes().to_vec();
        o.extend(literal_block(data));
        o
    }

    #[test]
    fn lz4_match_copies_overlap() {
        // "ab" then a match of 6 at offset 2, then 1 literal "c" to end.
        let src = [0x22, b'a', b'b', 2, 0, 0x10, b'c'];
        assert_eq!(lz4_block(&src, 9).as_deref(), Some(&b"ababababc"[..]));
        // A bad offset is refused, not a panic.
        assert!(lz4_block(&[0x10, b'a', 9, 0], 8).is_none());
    }

    fn input_bytes(scale: f32, tile: i32, agent: [f32; 3]) -> Vec<u8> {
        let mut b = b"rcst".to_vec();
        b.extend(2u32.to_le_bytes());
        for f in [0.2f32, 0.2, 6.0, 1.0, 34.0, 1.3, 46.0, scale, 0.9] {
            b.extend(f.to_le_bytes());
        }
        for i in [16i32, 12, 6, 400, 64, tile] {
            b.extend(i.to_le_bytes());
        }
        b.push(2);
        for f in agent {
            b.extend(f.to_le_bytes());
        }
        b.extend([0u8; 16]); // what follows (the mesh) is not read
        b
    }

    #[test]
    fn reads_settings_and_agent() {
        let i = read_input(&unpack(&blob(&input_bytes(1.0 / 34.0, 128, [29.0, 28.0, 66.5]))).unwrap()).unwrap();
        assert_eq!(i.tile_size, 128);
        assert_eq!(i.agent, [29.0, 28.0, 66.5]);
        assert!((i.tile_world() - 870.4).abs() < 0.01);
        assert_eq!(i.tile_of(-22000.0), -26);
    }

    #[test]
    fn reads_a_polygon_in_world_space() {
        let mut b = b"pnav".to_vec();
        b.extend(1u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(0.2f32.to_le_bytes());
        b.extend(0.2f32.to_le_bytes());
        for i in [3i32, 1, 1, 6] {
            b.extend(i.to_le_bytes());
        }
        for f in [10.0f32, 1.0, 20.0, 0.0, 0.0, 0.0, 0.5, 0.25] {
            b.extend(f.to_le_bytes());
        }
        b.extend(0i32.to_le_bytes());
        b.extend(1.3f32.to_le_bytes());
        for v in [0u16, 0, 0, 2, 4, 0, 0, 0, 2] {
            b.extend(v.to_le_bytes());
        }
        for v in [0u16, 1, 2, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff] {
            b.extend(v.to_le_bytes());
        }
        b.extend(0u16.to_le_bytes()); // region
        b.extend(FLAG_WALK.to_le_bytes());
        b.push(1); // area
        let p = read_polys(&b, 0.5).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].flags, FLAG_WALK);
        // vertex 1: x = 10 + 2*0.5 = 11, y(up) = 1 + 4*0.25 = 2, z = 20 -> world (22, 40, 4)
        assert_eq!(p[0].verts[1], [22.0, 40.0, 4.0]);
        assert_eq!(p[0].verts[2], [20.0, 42.0, 2.0]);
    }

    #[test]
    fn user_data_from_cfg() {
        let d = Path::new("/games/omw");
        assert_eq!(cfg_user_data("data=x\nuser-data=\"saves\"\n", d), Some(d.join("saves")));
        assert_eq!(cfg_user_data("user-data=/a\nuser-data=/b", d), Some(PathBuf::from("/b")));
        assert_eq!(cfg_user_data("data=x", d), None);
    }
}
