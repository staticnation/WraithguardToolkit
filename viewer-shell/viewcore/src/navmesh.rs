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
    /// Its index in its tile's poly mesh.
    pub index: u16,
    /// Per edge (vertex i to i + 1): the polygon of the same tile across it, or
    /// `0x8000 | side` for an edge on the tile's border (a portal the navigator joins
    /// to the neighbouring tile), or `0xffff` for none - Recast's own record.
    pub neighbours: Vec<u16>,
    /// Which tile of the answer it came from (in the order they were read).
    pub tile: u32,
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
            let neighbours = polys[p * nvp * 2 + nvp..p * nvp * 2 + nvp + vs.len()].to_vec();
            out.push(Poly { verts: vs, flags: flags[p], area: areas[p], index: p as u16, neighbours, tile: 0 });
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
    /// A tile's width in world units (0 when none was read).
    pub tile_world: f32,
    /// What each tile was built from: its position, agent, and the collision objects
    /// in it (`shapes` index, world position) - to tell a tile the plugins have since
    /// changed under.
    pub built: Vec<TileBuilt>,
    /// The mesh files `built` names (`shapes.name`, as navmeshtool wrote it).
    pub shapes: Vec<String>,
    shape_ix: std::collections::HashMap<i64, u32>,
}

/// One tile's build input, as far as the viewer compares it with the scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileBuilt {
    pub x: i32,
    pub y: i32,
    pub agent: u8,
    pub objects: Vec<(u32, [f32; 3])>,
    pub ground: Ground,
}

/// The land and water a tile was built from (the `input` blob's middle).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ground {
    /// Per cell: grid, cell size, the water's level.
    pub water: Vec<([i32; 2], i32, f32)>,
    /// Per cell with land: a rectangle of its height grid.
    pub heights: Vec<Heights>,
    /// Per cell without land: grid, cell size, the flat height used instead.
    pub flat: Vec<([i32; 2], i32, f32)>,
}

/// A rectangle of one cell's land heights, as the tile was built from it: `width`
/// columns (along X) by `rows` (along Y), row by row, starting at vertex (`min_x`,
/// `min_y`) of the cell's `original_size` x `original_size` grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heights {
    pub cell: [i32; 2],
    pub cell_size: i32,
    pub width: u32,
    pub rows: u32,
    pub min_x: u32,
    pub min_y: u32,
    pub original_size: u32,
    pub values: Vec<f32>,
}

/// Reads an unpacked `tiles.input` past the settings and agent (where [`read_input`]
/// stops): the water (u64 count; per cell i32 x, y, size, f32 level), the land heights
/// (u64 count; per cell i32 x, y, size, u8 rows, f32 min, f32 max, u64 count and the
/// f32 heights, u64 original size, u8 min x, u8 min y) and the flat cells (u64 count;
/// i32 x, y, size, f32 height). The collision objects follow ([`geometry_objects`]).
pub fn read_ground(b: &[u8]) -> Option<Ground> {
    let mut c = Cur { b, p: 0 };
    if c.take(4)? != b"rcst" || c.u32()? != 2 {
        return None;
    }
    c.take(9 * 4 + 6 * 4 + 1 + 12)?;
    let u64c = |c: &mut Cur| -> Option<usize> {
        let v = u64::from_le_bytes(c.take(8)?.try_into().ok()?);
        usize::try_from(v).ok().filter(|n| *n < 1 << 22)
    };
    let mut g = Ground::default();
    for _ in 0..u64c(&mut c)? {
        let cell = [c.i32()?, c.i32()?];
        g.water.push((cell, c.i32()?, c.f32()?));
    }
    for _ in 0..u64c(&mut c)? {
        let cell = [c.i32()?, c.i32()?];
        let cell_size = c.i32()?;
        let rows = c.u8()? as u32;
        c.take(8)?; // min, max
        let n = u64c(&mut c)?;
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            values.push(c.f32()?);
        }
        let original_size = u32::try_from(u64c(&mut c)?).ok()?;
        let min_x = c.u8()? as u32;
        let min_y = c.u8()? as u32;
        let width = (n as u32).checked_div(rows).unwrap_or(0);
        g.heights.push(Heights { cell, cell_size, width, rows, min_x, min_y, original_size, values });
    }
    for _ in 0..u64c(&mut c)? {
        let cell = [c.i32()?, c.i32()?];
        g.flat.push((cell, c.i32()?, c.f32()?));
    }
    Some(g)
}

/// The collision objects a tile was built from: the `input` blob ends with them as a
/// u64 count and, per object, its `shapes.shape_id` (i64) and transform - position
/// (3 f32, world units), rotation (3 f32), scale (f32). Found from the end, where the
/// count is the one that makes the list end the blob exactly.
pub fn geometry_objects(b: &[u8]) -> Option<Vec<(i64, [f32; 3])>> {
    const ONE: usize = 8 + 7 * 4;
    let len = b.len();
    let mut n = 0usize;
    while 8 + n * ONE <= len {
        let at = len - 8 - n * ONE;
        if u64::from_le_bytes(b[at..at + 8].try_into().ok()?) == n as u64 {
            let mut out = Vec::with_capacity(n);
            for k in 0..n {
                let o = at + 8 + k * ONE;
                let id = i64::from_le_bytes(b[o..o + 8].try_into().ok()?);
                let f = |i: usize| f32::from_le_bytes(b[o + 8 + i * 4..o + 12 + i * 4].try_into().unwrap_or([0; 4]));
                out.push((id, [f(0), f(1), f(2)]));
            }
            return Some(out);
        }
        n += 1;
    }
    None
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

    /// A shape's mesh file (`shapes.name`), or None.
    pub fn shape_name(&self, id: i64) -> Option<String> {
        let mut st = self.conn.prepare_cached("SELECT name FROM shapes WHERE shape_id = ?1").ok()?;
        st.query_row([id], |r| r.get::<_, String>(0)).optional().ok().flatten()
    }

    /// Every tile of `worldspace` in the inclusive tile range, every agent.
    pub fn read(&self, worldspace: &str, x: (i32, i32), y: (i32, i32), into: &mut Found) -> Result<(), String> {
        let mut st = self
            .conn
            .prepare_cached(
                "SELECT input, data, tile_position_x, tile_position_y FROM tiles WHERE worldspace = ?1 \
                 AND tile_position_x BETWEEN ?2 AND ?3 AND tile_position_y BETWEEN ?4 AND ?5",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(rusqlite::params![worldspace, x.0, x.1, y.0, y.1], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?, r.get::<_, i32>(2)?, r.get::<_, i32>(3)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (inp, data, tx, ty) = row.map_err(|e| e.to_string())?;
            let Some(raw) = unpack(&inp) else {
                into.skipped += 1;
                continue;
            };
            let Some(input) = read_input(&raw) else {
                into.skipped += 1;
                continue;
            };
            into.tile_world = input.tile_world();
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
            if let Some(objs) = geometry_objects(&raw) {
                let mut objects = Vec::with_capacity(objs.len());
                for (id, pos) in objs {
                    let ix = match into.shape_ix.get(&id) {
                        Some(&i) => i,
                        None => {
                            let name = self.shape_name(id).unwrap_or_default();
                            into.shapes.push(name);
                            let i = (into.shapes.len() - 1) as u32;
                            into.shape_ix.insert(id, i);
                            i
                        }
                    };
                    objects.push((ix, pos));
                }
                let ground = read_ground(&raw).unwrap_or_default();
                into.built.push(TileBuilt { x: tx, y: ty, agent: ai, objects, ground });
            }
            let tile_seq = (into.tiles - 1) as u32;
            for mut p in polys {
                p.tile = tile_seq;
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
/// "GDNM" | u8 version 3 | u16 len, db path (UTF-8) | u32 tiles | u32 skipped | u8 capped
///        | u8 agents, per agent: u8 shape, 3 f32 half extents
///        | u32 polys, per polygon: u8 agent, u8 area, u16 flags, u8 n, n * 3 f32 (world),
///          u32 tile (its order in the answer), u16 index in the tile, n * u16 neighbour
///          (per edge: Recast's - a polygon of the tile, 0x8000 | side for a portal,
///          0xffff for none)
///        | f32 tile width (world units)
///        | u32 shapes, per shape: u16 len, mesh path (UTF-8)
///        | u32 tiles built, per tile: i32 x, i32 y, u8 agent, u32 n, n * (u32 shape, 3 f32),
///          u32 water, per cell: i32 x, i32 y, i32 size, f32 level;
///          u32 heights, per cell: i32 x, i32 y, i32 size, u16 width, u16 rows, u16 min x,
///          u16 min y, u16 original size, width * rows f32;
///          u32 flat, per cell: i32 x, i32 y, i32 size, f32 height
/// ```
pub fn encode(path: &Path, f: &Found) -> Vec<u8> {
    let mut o = b"GDNM".to_vec();
    o.push(3);
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
        o.extend_from_slice(&p.tile.to_le_bytes());
        o.extend_from_slice(&p.index.to_le_bytes());
        for i in 0..n {
            o.extend_from_slice(&p.neighbours.get(i).copied().unwrap_or(0xffff).to_le_bytes());
        }
    }
    o.extend_from_slice(&f.tile_world.to_le_bytes());
    o.extend_from_slice(&(f.shapes.len() as u32).to_le_bytes());
    for name in &f.shapes {
        let b = &name.as_bytes()[..name.len().min(65535)];
        o.extend_from_slice(&(b.len() as u16).to_le_bytes());
        o.extend_from_slice(b);
    }
    o.extend_from_slice(&(f.built.len() as u32).to_le_bytes());
    for t in &f.built {
        o.extend_from_slice(&t.x.to_le_bytes());
        o.extend_from_slice(&t.y.to_le_bytes());
        o.push(t.agent);
        o.extend_from_slice(&(t.objects.len() as u32).to_le_bytes());
        for (ix, p) in &t.objects {
            o.extend_from_slice(&ix.to_le_bytes());
            for c in p {
                o.extend_from_slice(&c.to_le_bytes());
            }
        }
        let g = &t.ground;
        o.extend_from_slice(&(g.water.len() as u32).to_le_bytes());
        for (cell, size, level) in &g.water {
            o.extend_from_slice(&cell[0].to_le_bytes());
            o.extend_from_slice(&cell[1].to_le_bytes());
            o.extend_from_slice(&size.to_le_bytes());
            o.extend_from_slice(&level.to_le_bytes());
        }
        o.extend_from_slice(&(g.heights.len() as u32).to_le_bytes());
        for h in &g.heights {
            o.extend_from_slice(&h.cell[0].to_le_bytes());
            o.extend_from_slice(&h.cell[1].to_le_bytes());
            o.extend_from_slice(&h.cell_size.to_le_bytes());
            for v in [h.width, h.rows, h.min_x, h.min_y, h.original_size] {
                o.extend_from_slice(&(v.min(65535) as u16).to_le_bytes());
            }
            let n = (h.width * h.rows) as usize;
            for i in 0..n {
                o.extend_from_slice(&h.values.get(i).copied().unwrap_or(0.0).to_le_bytes());
            }
        }
        o.extend_from_slice(&(g.flat.len() as u32).to_le_bytes());
        for (cell, size, z) in &g.flat {
            o.extend_from_slice(&cell[0].to_le_bytes());
            o.extend_from_slice(&cell[1].to_le_bytes());
            o.extend_from_slice(&size.to_le_bytes());
            o.extend_from_slice(&z.to_le_bytes());
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
        assert_eq!(p[0].neighbours, vec![0xffff; 3]);
        assert_eq!(p[0].index, 0);
    }

    #[test]
    fn geometry_objects_from_the_end() {
        let mut b = b"rcst".to_vec();
        b.extend([7u8; 33]); // settings, agent, mesh, water...: not read here
        b.extend(2u64.to_le_bytes());
        for (id, x) in [(118i64, -21913.0f32), (5, 10.0)] {
            b.extend(id.to_le_bytes());
            for f in [x, -14045.0, 655.0, 0.0, 0.0, 1.55, 1.0] {
                b.extend(f.to_le_bytes());
            }
        }
        let got = geometry_objects(&b).unwrap();
        assert_eq!(got, vec![(118, [-21913.0, -14045.0, 655.0]), (5, [10.0, -14045.0, 655.0])]);
    }

    #[test]
    fn reads_the_ground_a_tile_was_built_from() {
        let mut b = b"rcst".to_vec();
        b.extend(2u32.to_le_bytes());
        b.extend([0u8; 9 * 4 + 6 * 4 + 1 + 12]); // settings, agent
        b.extend(1u64.to_le_bytes()); // water
        for v in [-3i32, -2, 8192] {
            b.extend(v.to_le_bytes());
        }
        b.extend((-1.0f32).to_le_bytes());
        b.extend(1u64.to_le_bytes()); // heights
        for v in [-3i32, -2, 8192] {
            b.extend(v.to_le_bytes());
        }
        b.push(2); // rows
        b.extend((-480.0f32).to_le_bytes());
        b.extend(2688.0f32.to_le_bytes());
        b.extend(6u64.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            b.extend(v.to_le_bytes());
        }
        b.extend(65u64.to_le_bytes());
        b.extend([13u8, 11]);
        b.extend(1u64.to_le_bytes()); // flat
        for v in [-119i32, -123, 8192] {
            b.extend(v.to_le_bytes());
        }
        b.extend((-2048.0f32).to_le_bytes());
        b.extend(0u64.to_le_bytes()); // objects
        let g = read_ground(&b).unwrap();
        assert_eq!(g.water, vec![([-3, -2], 8192, -1.0)]);
        let h = &g.heights[0];
        assert_eq!((h.width, h.rows, h.min_x, h.min_y, h.original_size), (3, 2, 13, 11, 65));
        assert_eq!(h.values[5], 6.0);
        assert_eq!(g.flat, vec![([-119, -123], 8192, -2048.0)]);
        assert_eq!(geometry_objects(&b), Some(vec![]));
    }

    #[test]
    fn user_data_from_cfg() {
        let d = Path::new("/games/omw");
        assert_eq!(cfg_user_data("data=x\nuser-data=\"saves\"\n", d), Some(d.join("saves")));
        assert_eq!(cfg_user_data("user-data=/a\nuser-data=/b", d), Some(PathBuf::from("/b")));
        assert_eq!(cfg_user_data("data=x", d), None);
    }
}
