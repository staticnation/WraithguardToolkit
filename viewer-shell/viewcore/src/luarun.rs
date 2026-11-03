//! Wraithguard: OpenMW Lua run against the Preview's scene (`ui/src/74_wg_lua.js`).
//!
//! The Lua is the install's own: OpenMW's LuaJIT, loaded at run time from where each
//! platform's build keeps it (never shipped with the viewer) - `lua51.dll` beside
//! `openmw.exe`, `lib/libluajit-5.1.so.2` beside the Linux launcher, `OpenMW.app/Contents/
//! Frameworks/libluajit-5.1.*.dylib` on macOS. Where none is found it is the Lua 5.4 the
//! viewer already embeds (mlua, through luacore's Teal checks).
//!
//! What runs is `luarun.lua`: the OpenMW API over a copy of the scene, driven a frame at a
//! time with JSON in and JSON out. Nothing in it calls back into Rust, so the only C calls
//! made are loading a chunk, calling a global function with a string and reading the
//! string it returns - the same five on LuaJIT and on 5.4, and a script's error never
//! unwinds through a Rust frame.
//!
//! The scripts are OpenMW's built-ins first (`resources/vfs/builtin.omwscripts` and
//! `vfs-mw`'s, the ones that act in a scene) and then the load order's, their text from the
//! data folders over `resources/vfs-mw` over `resources/vfs`, as OpenMW mounts them.

use std::collections::{BTreeMap, HashSet};
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::{Path, PathBuf};

use crate::vfs::Vfs;

/// The OpenMW API in Lua (`luarun.lua`).
pub const PRELUDE: &str = include_str!("luarun.lua");

/// OpenMW's built-in scripts the Preview runs: the ones that act on a scene (AI,
/// activation, use, crimes, music, weather, the world's events, settings storage), the
/// interfaces mods lean on (`I.Combat`, `I.UI`, `I.SkillProgression`,
/// `I.AnimationController`, `I.Settings`), and the UI templates mods draw their windows
/// with (`I.MWUI`). The camera, input, console and HUD ones need the game itself.
pub const BUILTINS: [&str; 21] = [
    "scripts/omw/mwui/init.lua",
    "scripts/omw/settings/global.lua",
    "scripts/omw/settings/player.lua",
    "scripts/omw/activationhandlers.lua",
    "scripts/omw/usehandlers.lua",
    "scripts/omw/worldeventhandlers.lua",
    "scripts/omw/crimes.lua",
    "scripts/omw/ai.lua",
    "scripts/omw/mechanics/animationcontroller.lua",
    "scripts/omw/mechanics/actorcontroller.lua",
    "scripts/omw/mechanics/globalcontroller.lua",
    "scripts/omw/skillhandlers.lua",
    "scripts/omw/playerskillhandlers.lua",
    "scripts/omw/combat/interface.lua",
    "scripts/omw/combat/local.lua",
    "scripts/omw/combat/global.lua",
    "scripts/omw/ui.lua",
    "scripts/omw/music/actor.lua",
    "scripts/omw/music/music.lua",
    "scripts/omw/weather.lua",
    "scripts/omw/cellhandlers.lua",
];

/* ---- the scene's collision, for nearby.castRay ------------------------------------------ */

/// One object's collision as the scene places it: its triangles in world units.
pub struct Shape {
    pub tris: Vec<[f32; 9]>,
    lo: [f32; 3],
    hi: [f32; 3],
    pub enabled: bool,
    /// OpenMW's collision type (`nearby.COLLISION_TYPE`): World 1, Door 2, Actor 4.
    pub kind: u32,
}

/// `nearby.COLLISION_TYPE` as OpenMW has it.
pub mod collision_type {
    pub const WORLD: u32 = 1;
    pub const DOOR: u32 = 2;
    pub const ACTOR: u32 = 4;
    pub const HEIGHT_MAP: u32 = 8;
    pub const WATER: u32 = 32;
    /// What a cast hits when it does not say: World, Door, Actor and HeightMap.
    pub const DEFAULT: u32 = 15;
    /// The collision type of an object by its record's tag.
    pub fn of_tag(tag: &str) -> u32 {
        match tag {
            "DOOR" => DOOR,
            "NPC_" | "CREA" => ACTOR,
            _ => WORLD,
        }
    }
}

/// What a ray can hit: the objects' collision shapes (by key) and, outside, the land.
#[derive(Default)]
pub struct Collider {
    pub shapes: std::collections::HashMap<String, Shape>,
    /// The land's heights by grid, the engine's own (`World::lands`): a square with none is
    /// flat at -2048, as OpenMW has it.
    pub world: Option<std::sync::Arc<crate::world::World>>,
    /// Whether the player is outside (the land is there to hit).
    pub exterior: bool,
    /// The water's height where there is water: the sea outside (0); an interior's own,
    /// when it has water.
    pub water: Option<f64>,
}

/// The scene's collision (one Lua at a time, as the runner).
pub static COLLIDER: std::sync::Mutex<Option<Collider>> = std::sync::Mutex::new(None);

impl Shape {
    /// `tris` in the mesh's own units (three vertices a triangle), placed by `m` (3x4, rows,
    /// the renderer's matrix) and `origin` (the scene's, added to x and y).
    pub fn placed(tris: &[[f32; 3]], m: &[f32; 12], origin: [f32; 2], enabled: bool) -> Shape {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        let tr = |v: &[f32; 3]| -> [f32; 3] {
            [
                m[0] * v[0] + m[1] * v[1] + m[2] * v[2] + m[3] + origin[0],
                m[4] * v[0] + m[5] * v[1] + m[6] * v[2] + m[7] + origin[1],
                m[8] * v[0] + m[9] * v[1] + m[10] * v[2] + m[11],
            ]
        };
        let mut out = Vec::with_capacity(tris.len() / 3);
        for t in tris.chunks_exact(3) {
            let (a, b, c) = (tr(&t[0]), tr(&t[1]), tr(&t[2]));
            for p in [a, b, c] {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            out.push([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
        }
        Shape { tris: out, lo, hi, enabled, kind: collision_type::WORLD }
    }
}

/// Where a ray from `a` to `b` first meets: `(point, normal, key)` - the key of the object
/// hit, `None` for the land.
pub type Hit = ([f64; 3], [f64; 3], Option<String>);

impl Collider {
    /// What the ray meets first, of the collision types in `mask` (`collision_type`).
    pub fn cast(&self, a: [f64; 3], b: [f64; 3], ignore: Option<&str>, mask: u32) -> Option<Hit> {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < 1e-6 {
            return None;
        }
        let mut best: Option<(f64, [f64; 3], Option<String>)> = None;
        for (k, s) in &self.shapes {
            if !s.enabled || s.kind & mask == 0 || Some(k.as_str()) == ignore || !box_hit(a, d, s.lo, s.hi) {
                continue;
            }
            for t in &s.tris {
                if let Some((u, n)) = tri_hit(a, d, t) {
                    if u <= 1.0 && best.as_ref().is_none_or(|b| u < b.0) {
                        best = Some((u, n, Some(k.clone())));
                    }
                }
            }
        }
        if self.exterior && mask & collision_type::HEIGHT_MAP != 0 {
            if let Some((u, n)) = self.land_hit(a, d, best.as_ref().map_or(1.0, |b| b.0)) {
                best = Some((u, n, None));
            }
        }
        // The water's surface, from either side.
        if let Some(w) = self.water.filter(|_| mask & collision_type::WATER != 0) {
            if d[2].abs() > 1e-9 {
                let u = (w - a[2]) / d[2];
                if (0.0..=1.0).contains(&u) && best.as_ref().is_none_or(|b| u < b.0) {
                    best = Some((u, [0.0, 0.0, if d[2] < 0.0 { 1.0 } else { -1.0 }], None));
                }
            }
        }
        best.map(|(u, n, k)| ([a[0] + d[0] * u, a[1] + d[1] * u, a[2] + d[2] * u], n, k))
    }

    /// The land's height at a point (the game's triangles: each quad cut from (x+1, y) to
    /// (x, y+1)), -2048 where a square has no land.
    pub fn ground(&self, x: f64, y: f64) -> f64 {
        const CELL: f64 = 8192.0;
        const N: usize = 65;
        let (gx, gy) = ((x / CELL).floor() as i32, (y / CELL).floor() as i32);
        let Some(land) = self.world.as_ref().and_then(|w| w.lands.get(&(gx, gy))) else { return -2048.0 };
        let step = CELL / (N - 1) as f64;
        let fx = ((x - gx as f64 * CELL) / step).clamp(0.0, (N - 1) as f64 - 1e-3);
        let fy = ((y - gy as f64 * CELL) / step).clamp(0.0, (N - 1) as f64 - 1e-3);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let h = |x: usize, y: usize| land.heights[y * N + x] as f64;
        if tx + ty <= 1.0 {
            h(x0, y0) * (1.0 - tx - ty) + h(x0 + 1, y0) * tx + h(x0, y0 + 1) * ty
        } else {
            h(x0 + 1, y0 + 1) * (tx + ty - 1.0) + h(x0, y0 + 1) * (1.0 - tx) + h(x0 + 1, y0) * (1.0 - ty)
        }
    }

    /// The land along the ray, up to `limit` of it: stepped at most 32 units at a time, the
    /// crossing then halved down to a hundredth of a unit.
    fn land_hit(&self, a: [f64; 3], d: [f64; 3], limit: f64) -> Option<(f64, [f64; 3])> {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let at = |u: f64| [a[0] + d[0] * u, a[1] + d[1] * u, a[2] + d[2] * u];
        let above = |u: f64| { let p = at(u); p[2] - self.ground(p[0], p[1]) };
        if above(0.0) < 0.0 {
            return None;
        }
        let n = ((len * limit) / 32.0).ceil().max(1.0) as usize;
        let mut prev = 0.0;
        for i in 1..=n {
            let u = limit * i as f64 / n as f64;
            if above(u) < 0.0 {
                let (mut lo, mut hi) = (prev, u);
                for _ in 0..24 {
                    let mid = (lo + hi) / 2.0;
                    if above(mid) < 0.0 { hi = mid } else { lo = mid }
                }
                let p = at(hi);
                let e = 16.0;
                let (gx0, gx1) = (self.ground(p[0] - e, p[1]), self.ground(p[0] + e, p[1]));
                let (gy0, gy1) = (self.ground(p[0], p[1] - e), self.ground(p[0], p[1] + e));
                let nn = [-(gx1 - gx0) / (2.0 * e), -(gy1 - gy0) / (2.0 * e), 1.0];
                let l = (nn[0] * nn[0] + nn[1] * nn[1] + 1.0).sqrt();
                return Some((hi, [nn[0] / l, nn[1] / l, nn[2] / l]));
            }
            prev = u;
        }
        None
    }
}

fn box_hit(a: [f64; 3], d: [f64; 3], lo: [f32; 3], hi: [f32; 3]) -> bool {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        let (l, h) = (lo[k] as f64 - 1e-3, hi[k] as f64 + 1e-3);
        if d[k].abs() < 1e-12 {
            if a[k] < l || a[k] > h {
                return false;
            }
            continue;
        }
        let (mut n, mut f) = ((l - a[k]) / d[k], (h - a[k]) / d[k]);
        if n > f {
            std::mem::swap(&mut n, &mut f);
        }
        t0 = t0.max(n);
        t1 = t1.min(f);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Möller-Trumbore: where along `d` (0..1 of it) the ray from `a` meets the triangle, and
/// the triangle's normal facing the ray.
fn tri_hit(a: [f64; 3], d: [f64; 3], t: &[f32; 9]) -> Option<(f64, [f64; 3])> {
    let v = |i: usize| [t[i * 3] as f64, t[i * 3 + 1] as f64, t[i * 3 + 2] as f64];
    let (p0, p1, p2) = (v(0), v(1), v(2));
    let sub = |x: [f64; 3], y: [f64; 3]| [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
    let cross = |x: [f64; 3], y: [f64; 3]| [x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]];
    let dot = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    let (e1, e2) = (sub(p1, p0), sub(p2, p0));
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(a, p0);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let w = dot(d, q) * inv;
    if w < 0.0 || u + w > 1.0 {
        return None;
    }
    let tt = dot(e2, q) * inv;
    if tt < 0.0 {
        return None;
    }
    let mut n = cross(e1, e2);
    let l = dot(n, n).sqrt().max(1e-12);
    if dot(n, d) > 0.0 {
        n = [-n[0], -n[1], -n[2]];
    }
    Some((tt, [n[0] / l, n[1] / l, n[2] / l]))
}

/// The ray the Lua asks for (`__wg_ray`), against the scene's collision: the types in
/// `mask` (`collision_type`).
pub fn ray(a: [f64; 3], b: [f64; 3], ignore: Option<&str>, mask: u32) -> Option<Hit> {
    let g = COLLIDER.lock().ok()?;
    g.as_ref()?.cast(a, b, ignore, mask)
}

type St = *mut c_void;

/// The install's LuaJIT, its five calls (Lua 5.1's C API).
struct Jit {
    _lib: libloading::Library,
    l: St,
    loadbuffer: unsafe extern "C" fn(St, *const c_char, usize, *const c_char) -> c_int,
    pcall: unsafe extern "C" fn(St, c_int, c_int, c_int) -> c_int,
    getfield: unsafe extern "C" fn(St, c_int, *const c_char),
    pushlstring: unsafe extern "C" fn(St, *const c_char, usize),
    tolstring: unsafe extern "C" fn(St, c_int, *mut usize) -> *const c_char,
    settop: unsafe extern "C" fn(St, c_int),
    close: unsafe extern "C" fn(St),
}

const LUA_GLOBALSINDEX: c_int = -10002;

/// The calls `__wg_ray` makes on the LuaJIT it is registered in (set each time one opens).
#[derive(Clone, Copy)]
struct RayFns {
    gettop: unsafe extern "C" fn(St) -> c_int,
    type_: unsafe extern "C" fn(St, c_int) -> c_int,
    tonumber: unsafe extern "C" fn(St, c_int) -> f64,
    tolstring: unsafe extern "C" fn(St, c_int, *mut usize) -> *const c_char,
    pushnumber: unsafe extern "C" fn(St, f64),
    pushlstring: unsafe extern "C" fn(St, *const c_char, usize),
}
static RAY_FNS: std::sync::Mutex<Option<RayFns>> = std::sync::Mutex::new(None);

/// `__wg_ray(x1, y1, z1, x2, y2, z2 [, ignore [, mask]])` on LuaJIT: nothing on a miss, else the
/// point, the normal and the key of what was hit (nothing more for the land). It never
/// raises a Lua error, so nothing unwinds through it.
unsafe extern "C" fn ray_c(l: St) -> c_int {
    let Some(f) = RAY_FNS.lock().ok().and_then(|g| *g) else { return 0 };
    let got = std::panic::catch_unwind(|| {
        // SAFETY: called by Lua with its own state; indexes within the arguments given.
        unsafe {
            let n = (f.gettop)(l);
            let num = |i: c_int| (f.tonumber)(l, i);
            let ignore = if n >= 7 && (f.type_)(l, 7) == 4 {
                let mut len = 0usize;
                let p = (f.tolstring)(l, 7, &mut len);
                (!p.is_null()).then(|| String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len)).into_owned())
            } else {
                None
            };
            let mask = if n >= 8 && (f.type_)(l, 8) == 3 { num(8) as u32 } else { collision_type::DEFAULT };
            ray([num(1), num(2), num(3)], [num(4), num(5), num(6)], ignore.as_deref(), mask)
        }
    });
    let Ok(Some((p, nrm, key))) = got else { return 0 };
    // SAFETY: pushes onto the same state; Lua makes room for a C function's results.
    unsafe {
        for x in p.iter().chain(nrm.iter()) {
            (f.pushnumber)(l, *x);
        }
        match key {
            Some(k) => {
                (f.pushlstring)(l, k.as_ptr() as *const c_char, k.len());
                7
            }
            None => 6,
        }
    }
}

impl Jit {
    fn open(path: &Path) -> Result<Jit, String> {
        // SAFETY: OpenMW's own LuaJIT; the symbols are Lua 5.1's C API, called with its types.
        unsafe {
            let lib = libloading::Library::new(path).map_err(|e| format!("{}: {e}", path.display()))?;
            macro_rules! sym {
                ($n:literal) => {
                    *lib.get($n).map_err(|e| format!("{}: {e}", path.display()))?
                };
            }
            let newstate: unsafe extern "C" fn() -> St = sym!(b"luaL_newstate\0");
            let openlibs: unsafe extern "C" fn(St) = sym!(b"luaL_openlibs\0");
            let pushcclosure: unsafe extern "C" fn(St, unsafe extern "C" fn(St) -> c_int, c_int) = sym!(b"lua_pushcclosure\0");
            let setfield: unsafe extern "C" fn(St, c_int, *const c_char) = sym!(b"lua_setfield\0");
            let fns = RayFns {
                gettop: sym!(b"lua_gettop\0"),
                type_: sym!(b"lua_type\0"),
                tonumber: sym!(b"lua_tonumber\0"),
                tolstring: sym!(b"lua_tolstring\0"),
                pushnumber: sym!(b"lua_pushnumber\0"),
                pushlstring: sym!(b"lua_pushlstring\0"),
            };
            let jit = Jit {
                loadbuffer: sym!(b"luaL_loadbuffer\0"),
                pcall: sym!(b"lua_pcall\0"),
                getfield: sym!(b"lua_getfield\0"),
                pushlstring: sym!(b"lua_pushlstring\0"),
                tolstring: sym!(b"lua_tolstring\0"),
                settop: sym!(b"lua_settop\0"),
                close: sym!(b"lua_close\0"),
                l: newstate(),
                _lib: lib,
            };
            if jit.l.is_null() {
                return Err("LuaJIT could not make a state".into());
            }
            openlibs(jit.l);
            if let Ok(mut g) = RAY_FNS.lock() {
                *g = Some(fns);
            }
            pushcclosure(jit.l, ray_c, 0);
            setfield(jit.l, LUA_GLOBALSINDEX, c"__wg_ray".as_ptr());
            Ok(jit)
        }
    }

    /// The string at the top of the stack (popped).
    unsafe fn take_string(&self) -> String {
        let mut n = 0usize;
        let p = unsafe { (self.tolstring)(self.l, -1, &mut n) };
        let s = if p.is_null() { String::new() } else { String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p as *const u8, n) }).into_owned() };
        unsafe { (self.settop)(self.l, -2) };
        s
    }

    fn exec(&mut self, code: &str, name: &str) -> Result<(), String> {
        let name = CString::new(format!("={name}")).unwrap_or_default();
        // SAFETY: a live state; the chunk is pushed, then called with no arguments.
        unsafe {
            if (self.loadbuffer)(self.l, code.as_ptr() as *const c_char, code.len(), name.as_ptr()) != 0 {
                return Err(self.take_string());
            }
            if (self.pcall)(self.l, 0, 0, 0) != 0 {
                return Err(self.take_string());
            }
        }
        Ok(())
    }

    fn call(&mut self, func: &str, arg: &str) -> Result<String, String> {
        let f = CString::new(func).map_err(|e| e.to_string())?;
        // SAFETY: a live state; one string argument, one result, both left balanced.
        unsafe {
            (self.getfield)(self.l, LUA_GLOBALSINDEX, f.as_ptr());
            (self.pushlstring)(self.l, arg.as_ptr() as *const c_char, arg.len());
            if (self.pcall)(self.l, 1, 1, 0) != 0 {
                return Err(self.take_string());
            }
            Ok(self.take_string())
        }
    }
}

impl Drop for Jit {
    fn drop(&mut self) {
        // SAFETY: the state is this struct's own and closed once, before the library unloads.
        unsafe { (self.close)(self.l) }
    }
}

enum Vm {
    Jit(Jit),
    Lua54(mlua::Lua),
}

/// A running Lua: the scripts started, a frame at a time after.
pub struct Runner {
    vm: Vm,
    /// What runs the scripts, for the panel: `LuaJIT (path)` or `Lua 5.4 (built in)`.
    pub backend: String,
}

// SAFETY: the state is only ever used behind the caller's lock, one thread at a time.
unsafe impl Send for Runner {}

impl Runner {
    /// A Lua with the prelude loaded: the install's LuaJIT when `jit` is found and opens,
    /// else Lua 5.4. `force_54` skips the LuaJIT (tests of the fallback).
    pub fn new(jit: Option<&Path>, force_54: bool) -> Result<Runner, String> {
        let mut why = String::new();
        if let (Some(p), false) = (jit, force_54) {
            match Jit::open(p) {
                Ok(mut j) => {
                    j.exec(PRELUDE, "wg_luarun")?;
                    return Ok(Runner { vm: Vm::Jit(j), backend: format!("LuaJIT ({})", p.display()) });
                }
                Err(e) => why = format!(" - the install's LuaJIT did not open: {e}"),
            }
        }
        let lua = mlua::Lua::new();
        let rayf = lua
            .create_function(|lua, (x1, y1, z1, x2, y2, z2, ig, mask): (f64, f64, f64, f64, f64, f64, Option<String>, Option<f64>)| {
                let mut out = mlua::MultiValue::new();
                let mask = mask.map_or(collision_type::DEFAULT, |m| m as u32);
                if let Some((p, n, k)) = ray([x1, y1, z1], [x2, y2, z2], ig.as_deref(), mask) {
                    for x in p.iter().chain(n.iter()) {
                        out.push_back(mlua::Value::Number(*x));
                    }
                    if let Some(k) = k {
                        out.push_back(mlua::Value::String(lua.create_string(&k)?));
                    }
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())?;
        lua.globals().set("__wg_ray", rayf).map_err(|e| e.to_string())?;
        lua.load(PRELUDE).set_name("=wg_luarun").exec().map_err(|e| e.to_string())?;
        Ok(Runner { vm: Vm::Lua54(lua), backend: format!("Lua 5.4 (built in){why}") })
    }

    /// One of the prelude's entry points (`__wg_start`, `__wg_step`): JSON in, JSON out.
    pub fn call(&mut self, func: &str, arg: &str) -> Result<String, String> {
        match &mut self.vm {
            Vm::Jit(j) => j.call(func, arg),
            Vm::Lua54(lua) => {
                let f: mlua::Function = lua.globals().get(func).map_err(|e| e.to_string())?;
                f.call::<String>(arg).map_err(|e| e.to_string())
            }
        }
    }
}

/// Where the install keeps its LuaJIT, from its `resources` folder: Windows, Linux and
/// macOS builds of OpenMW each have it in a place of their own.
pub fn find_luajit(resources: &Path) -> Option<PathBuf> {
    let install = resources.parent()?;
    let mut tries = vec![
        install.join("lua51.dll"),
        install.join("lib").join("libluajit-5.1.so.2"),
        install.join("libluajit-5.1.so.2"),
    ];
    // macOS: OpenMW.app/Contents/Resources/resources -> Contents/Frameworks.
    for up in [install.parent(), Some(install)].into_iter().flatten() {
        if let Ok(dir) = std::fs::read_dir(up.join("Frameworks")) {
            let mut found: Vec<PathBuf> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("libluajit") && n.ends_with(".dylib")))
                .collect();
            found.sort();
            tries.extend(found);
        }
    }
    tries.into_iter().find(|p| p.is_file())
}

/// A file's text from the data folders, else `resources/vfs-mw`, else `resources/vfs` (the
/// order OpenMW mounts them in: the data folders win).
pub fn read_script(vfs: &Vfs, resources: Option<&Path>, path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    if let Some(t) = crate::luascan::source(vfs, &p) {
        return Some(t);
    }
    let r = resources?;
    for sub in ["vfs-mw", "vfs"] {
        if let Some(t) = read_ci(&r.join(sub), &p) {
            return Some(t);
        }
    }
    None
}

/// A file under `root` by a path whose case may not match the disk's.
fn read_ci(root: &Path, rel: &str) -> Option<String> {
    let mut at = root.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        let exact = at.join(part);
        at = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&at).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path()
        };
    }
    std::fs::read(&at).ok().map(|b| luacore::scan::text_of(&b))
}

/// The modules a script reads by a constant name (`require('a.b')`, `require "a.b"`): their
/// paths, `a/b.lua` and `a/b/init.lua`. openmw.* packages are the prelude's own.
pub fn required(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("require") {
        let after = &rest[i + 7..];
        let before_ok = i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric() && rest.as_bytes()[i - 1] != b'_';
        rest = after;
        if !before_ok {
            continue;
        }
        let t = after.trim_start().trim_start_matches('(').trim_start();
        let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
        let Some(end) = t[1..].find(q) else { continue };
        let name = &t[1..1 + end];
        if name == "openmw" || name.starts_with("openmw.") || name.is_empty() {
            continue;
        }
        let base = name.replace('.', "/");
        out.push(format!("{base}.lua"));
        out.push(format!("{base}/init.lua"));
    }
    out
}

/// What `__wg_start` takes: the built-ins and the load order's scripts (built-ins first),
/// every script's and module's text, the music files (for OpenMW's playlists) and the
/// weather names. `registered` is the load order's (`luascan::registered`).
/// The `core.API_REVISION` the scripts are given: the install's, by the version in its
/// `resources/version` (OpenMW writes the release there; the revision is the release's,
/// `luacore::api`), else the newest release's. Scripts compare it to refuse an old OpenMW
/// (`NGarde`: "API_REVISION 100 is less than minimum 129"), so a guess too low stops them.
pub fn api_revision(resources: Option<&Path>) -> i64 {
    let version = resources
        .and_then(|r| std::fs::read_to_string(r.join("version")).ok())
        .and_then(|t| t.lines().next().map(|l| l.trim().to_string()))
        .filter(|v| !v.is_empty());
    let r = luacore::api::for_version(version.as_deref()).revision;
    if r > 0 { r } else { luacore::api::for_version(None).revision }
}

/// What `__wg_start` is given. `content` is the load order's `content=` names, in order
/// (`core.contentFiles`).
pub fn start_json(vfs: &Vfs, resources: Option<&Path>, registered: &[(String, Vec<String>)], weathers: &[String], content: &[String]) -> String {
    let mut scripts: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(r) = resources {
        for sub in ["vfs", "vfs-mw"] {
            let Ok(text) = std::fs::read(r.join(sub).join("builtin.omwscripts")) else { continue };
            let parsed = luacore::omwscripts::parse_omwscripts(&luacore::scan::text_of(&text), "builtin.omwscripts");
            for e in parsed.entries {
                let p = e.path.replace('\\', "/").to_ascii_lowercase();
                if BUILTINS.contains(&p.as_str()) && !scripts.iter().any(|(q, _)| q.eq_ignore_ascii_case(&p)) {
                    scripts.push((e.path, e.flags));
                }
            }
        }
    }
    // The menu's and the loader's scripts have no scene to run in.
    for (p, f) in registered {
        let f: Vec<String> = f.iter().filter(|x| *x != "MENU" && *x != "LOAD").cloned().collect();
        if !f.is_empty() {
            scripts.push((p.clone(), f));
        }
    }
    // Every script, and what each requires, read (a module once).
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let mut want: Vec<String> = scripts.iter().map(|(p, _)| p.clone()).collect();
    for aux in ["openmw_aux/util.lua", "openmw_aux/time.lua", "openmw_aux/calendar.lua", "openmw_aux/calendarconfig.lua", "openmw_aux/ui.lua"] {
        want.push(aux.into());
    }
    // A mod's other modules too, by its folder: a `require` built from a string at run time
    // (`require(base .. '.utils.defs')`) is not seen in the text.
    let mut tops: Vec<String> = Vec::new();
    for (p, _) in registered {
        let p = p.replace('\\', "/").to_ascii_lowercase();
        let mut parts = p.split('/');
        if let (Some("scripts"), Some(top), Some(_)) = (parts.next(), parts.next(), parts.next()) {
            let t = format!("scripts/{top}/");
            if !tops.contains(&t) {
                tops.push(t);
            }
        }
    }
    for t in &tops {
        want.extend(vfs.list(t, &[".lua"]).into_iter().take(400));
    }
    let mut seen: HashSet<String> = HashSet::new();
    while let Some(p) = want.pop() {
        let k = p.replace('\\', "/").to_ascii_lowercase();
        if !seen.insert(k.clone()) {
            continue;
        }
        if let Some(t) = read_script(vfs, resources, &k) {
            want.extend(required(&t));
            sources.insert(k, t);
        }
    }
    let files: Vec<String> = vfs.list("music/", &[".mp3", ".wav", ".ogg", ".flac"]);
    use crate::json::{escape_into, string_array, J};
    let list: Vec<String> = scripts.iter().map(|(p, f)| J::obj().str("path", p).raw("flags", &string_array(f)).done()).collect();
    let mut src = String::from("{");
    for (i, (k, t)) in sources.iter().enumerate() {
        if i > 0 {
            src.push(',');
        }
        src.push('"');
        escape_into(k, &mut src);
        src.push_str("\":\"");
        escape_into(t, &mut src);
        src.push('"');
    }
    src.push('}');
    J::obj()
        .raw("scripts", &format!("[{}]", list.join(",")))
        .raw("sources", &src)
        .raw("files", &string_array(&files))
        .raw("weathers", &string_array(weathers))
        .raw("content", &string_array(content))
        .int("apiRevision", api_revision(resources).max(0) as u64)
        .done()
}

#[cfg(test)]
mod tests {
    use super::*;


    /// The prelude on whichever Lua is given: a global script turning a light off by its
    /// cell and setting a global; OpenMW's rule that openmw.world is not for local scripts.
    fn runs_a_scene(r: &mut Runner) {
        let lamps = "local world = require('openmw.world') local types = require('openmw.types') local done = false\n\
            return { engineHandlers = { onUpdate = function(dt) if done then return end done = true\n\
            for _, o in ipairs(world.getCellByName('Balmora'):getAll(types.Light)) do o.enabled = false end\n\
            world.mwscript.getGlobalVariables().TownLit = 3 print('lamps out', #world.activeActors) end } }";
        let sneaky = "local world = require('openmw.world') return {}";
        let q = |s: &str| { let mut o = String::from("\""); crate::json::escape_into(s, &mut o); o.push('"'); o };
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/lamps.lua","flags":["GLOBAL"]}},{{"path":"scripts/t/sneaky.lua","flags":["NPC"]}}],
            "sources":{{"scripts/t/lamps.lua":{},"scripts/t/sneaky.lua":{}}},"files":["music/explore/a.mp3"],"weathers":["Clear","Rain"]}}"#, q(lamps), q(sneaky));
        let a = r.call("__wg_start", &start).unwrap();
        assert!(a.contains(r#""global":1"#), "{a}");
        let got = r.call("__wg_step", r#"{"dt":0.05,
            "objects":{"l1":{"id":"light_a","tag":"LIGH","pos":[0,0,100],"cell":{"name":"Balmora","ext":false}},
                       "n1":{"id":"guard","tag":"NPC_","pos":[0,0,0],"cell":{"name":"Balmora","ext":false}}},
            "globals":{"townlit":{"name":"TownLit","kind":"s","v":0}}}"#).unwrap();
        assert!(got.contains(r#"{"k":"enable","key":"l1","on":false}"#) || got.contains(r#""on":false"#) && got.contains(r#""key":"l1""#), "{got}");
        assert!(got.contains(r#""name":"townlit""#) && got.contains(r#""v":3"#), "{got}");
        assert!(got.contains("openmw.world is only for global scripts"), "{got}");
        assert!(got.contains(r#""text":"lamps out 2""#), "{got}");
    }

    #[test]
    fn lua_54_runs_a_scene() {
        let mut r = Runner::new(None, true).unwrap();
        assert!(r.backend.starts_with("Lua 5.4"));
        runs_a_scene(&mut r);
    }

    /// The same on an OpenMW build's LuaJIT, when one is given (`WG_LUAJIT=.../libluajit-5.1.so.2`).
    #[test]
    #[ignore]
    fn luajit_runs_a_scene() {
        let p = std::env::var("WG_LUAJIT").expect("WG_LUAJIT");
        let mut r = Runner::new(Some(Path::new(&p)), false).unwrap();
        assert!(r.backend.starts_with("LuaJIT"), "{}", r.backend);
        runs_a_scene(&mut r);
    }

    #[test]
    fn requires_found_by_name() {
        assert_eq!(required("local a = require('scripts.x.y')\nlocal b=require \"m\" local c = require('openmw.core') local d = myrequire('z')"),
            ["scripts/x/y.lua", "scripts/x/y/init.lua", "m.lua", "m/init.lua"]);
    }

    #[test]
    fn luajit_found_where_each_platform_keeps_it() {
        let root = std::env::temp_dir().join(format!("wg_luajit_{}", std::process::id()));
        let mac = root.join("OpenMW.app/Contents");
        std::fs::create_dir_all(mac.join("Resources/resources")).unwrap();
        std::fs::create_dir_all(mac.join("Frameworks")).unwrap();
        std::fs::write(mac.join("Frameworks/libluajit-5.1.2.1.1767980792.dylib"), b"").unwrap();
        let lin = root.join("openmw-0.51.0-Linux-64Bit");
        std::fs::create_dir_all(lin.join("resources")).unwrap();
        std::fs::create_dir_all(lin.join("lib")).unwrap();
        std::fs::write(lin.join("lib/libluajit-5.1.so.2"), b"").unwrap();
        let win = root.join("OpenMW");
        std::fs::create_dir_all(win.join("resources")).unwrap();
        std::fs::write(win.join("lua51.dll"), b"").unwrap();
        let mac_got = find_luajit(&mac.join("Resources/resources"));
        let lin_got = find_luajit(&lin.join("resources"));
        let win_got = find_luajit(&win.join("resources"));
        std::fs::remove_dir_all(&root).ok();
        assert!(mac_got.is_some_and(|p| p.ends_with("Frameworks/libluajit-5.1.2.1.1767980792.dylib")));
        assert!(lin_got.is_some_and(|p| p.ends_with("lib/libluajit-5.1.so.2")));
        assert!(win_got.is_some_and(|p| p.ends_with("lua51.dll")));
    }

    /// OpenMW's own built-ins on the prelude: an install's `resources` (`WG_RESOURCES`) and,
    /// when given, its LuaJIT (`WG_LUAJIT`). A few seconds of a scene with an NPC sent on an
    /// AI package through OpenMW's ai.lua, activation through activationhandlers.lua, and
    /// music through its playlists: no script fails.
    #[test]
    #[ignore]
    fn openmw_builtins_run() {
        let res = PathBuf::from(std::env::var("WG_RESOURCES").expect("WG_RESOURCES"));
        let jit = std::env::var("WG_LUAJIT").ok();
        let mut r = Runner::new(jit.as_deref().map(Path::new), jit.is_none()).unwrap();
        let mod_ai = "local self = require('openmw.self') local I = require('openmw.interfaces') local util = require('openmw.util') local started = false\n\
            return { engineHandlers = { onUpdate = function() if started then return end started = true\n\
            I.AI.startPackage({ type = 'Travel', destPosition = util.vector3(600, 0, 0) }) print('package', I.AI.getActivePackage().type)\n\
            print('interfaces', I.Combat ~= nil, I.AnimationController ~= nil) end } }";
        let vfs = Vfs::with_roots(&[]);
        let mut start = start_json(&vfs, Some(&res), &[], &["Clear".into(), "Rain".into()], &[]);
        // The mod's script, as the load order would register it.
        let q = |s: &str| { let mut o = String::from("\""); crate::json::escape_into(s, &mut o); o.push('"'); o };
        start = start.replacen("\"scripts\":[", "\"scripts\":[{\"path\":\"scripts/t/ai.lua\",\"flags\":[\"NPC\"]},", 1)
            .replacen("\"sources\":{", &format!("\"sources\":{{\"scripts/t/ai.lua\":{},", q(mod_ai)), 1)
            .replacen("\"files\":[", "\"files\":[\"music/explore/a.mp3\",\"music/explore/b.mp3\"", 1);
        let a = r.call("__wg_start", &start).unwrap();
        assert!(!a.contains("\"kind\":\"error\""), "{}: {a}", r.backend);
        let mut all = a.clone();
        let objs = r#"{"dt":0.05,"player":{"pos":[0,0,0],"cell":{"name":"","ext":true,"gx":0,"gy":0}},
            "objects":{"n1":{"id":"guard","tag":"NPC_","pos":[0,0,0],"cell":{"name":"","ext":true,"gx":0,"gy":0}},
                       "d1":{"id":"door","tag":"DOOR","pos":[100,0,0],"cell":{"name":"","ext":true,"gx":0,"gy":0}}},
            "activations":[{"key":"d1","by":"player"}]}"#;
        all.push_str(&r.call("__wg_step", objs).unwrap());
        for _ in 0..60 {
            all.push_str(&r.call("__wg_step", r#"{"dt":0.05}"#).unwrap());
        }
        assert!(!all.contains("\"kind\":\"error\""), "{}: {all}", r.backend);
        assert!(all.contains(r#""text":"package Travel""#), "{all}");
        assert!(all.contains(r#""text":"interfaces true true""#), "the interfaces mods use: {all}");
        assert!(all.contains(r#""k":"ai""#) && all.contains(r#""kind":"travel""#), "ai.lua's _startAiTravel: {all}");
        assert!(all.contains(r#""k":"activate","key":"d1""#) || all.contains(r#""key":"d1","by":"player""#) || all.contains(r#""k":"activate""#), "activationhandlers' standard action: {all}");
        assert!(all.contains(r#""k":"music""#), "OpenMW's music playlists: {all}");
    }

    /// nearby.castRay against the collider: a wall's shape hit (with its key and normal),
    /// `ignore` passing through it, and nothing past its end. On Lua 5.4 and, when given
    /// (`WG_LUAJIT`), the install's LuaJIT.
    #[test]
    fn rays_hit_the_scene_s_collision() {
        // A wall across y = 500, 1000 wide and 400 high, placed by a matrix moved 100 up.
        let quad = [[-500.0, 0.0, 0.0], [500.0, 0.0, 0.0], [500.0, 0.0, 400.0], [-500.0, 0.0, 0.0], [500.0, 0.0, 400.0], [-500.0, 0.0, 400.0]];
        let m = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 500.0, 0.0, 0.0, 1.0, 100.0];
        let mut c = Collider::default();
        c.shapes.insert("w1".into(), Shape::placed(&quad, &m, [0.0, 0.0], true));
        // A door further on, by its type.
        let mut door = Shape::placed(&quad, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 800.0, 0.0, 0.0, 1.0, 100.0], [0.0, 0.0], true);
        door.kind = collision_type::DOOR;
        c.shapes.insert("d1".into(), door);
        *COLLIDER.lock().unwrap() = Some(c);
        let script = "local nearby = require('openmw.nearby') local util = require('openmw.util') local done = false\n\
            return { engineHandlers = { onUpdate = function() if done then return end done = true\n\
            local a = nearby.castRay(util.vector3(0, 0, 200), util.vector3(0, 1000, 200))\n\
            print('hit', a.hit, a.hitObject and a.hitObject.recordId, math.floor(a.hitPos.y + 0.5), a.hitNormal.y)\n\
            local b = nearby.castRay(util.vector3(0, 0, 200), util.vector3(0, 1000, 200), { ignore = a.hitObject })\n\
            local c = nearby.castRay(util.vector3(0, 0, 200), util.vector3(0, 400, 200))\n\
            local d = nearby.castRay(util.vector3(0, 0, 600), util.vector3(0, 1000, 600))\n\
            local e = nearby.castRay(util.vector3(0, 0, 200), util.vector3(0, 1000, 200), { collisionType = nearby.COLLISION_TYPE.Door })\n\
            print('then', b.hit, c.hit, d.hit, b.hitObject and b.hitObject.recordId, e.hitObject and e.hitObject.recordId) end } }";
        let mut esc = String::from("\"");
        crate::json::escape_into(script, &mut esc);
        esc.push('"');
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/ray.lua","flags":["NPC"]}}],"sources":{{"scripts/t/ray.lua":{esc}}},"files":[],"weathers":["Clear"]}}"#);
        let mut runners = vec![Runner::new(None, true).unwrap()];
        if let Ok(p) = std::env::var("WG_LUAJIT") {
            runners.push(Runner::new(Some(Path::new(&p)), false).unwrap());
        }
        for r in &mut runners {
            r.call("__wg_start", &start).unwrap();
            let got = r.call("__wg_step", r#"{"dt":0.05,"objects":{"n1":{"id":"guard","tag":"NPC_","pos":[0,0,0],"cell":{"name":"x","ext":false}},
                "w1":{"id":"ex_wall","tag":"STAT","pos":[0,500,100],"cell":{"name":"x","ext":false}},
                "d1":{"id":"door_a","tag":"DOOR","pos":[0,800,100],"cell":{"name":"x","ext":false}}}}"#).unwrap();
            assert!(got.contains(r#""text":"hit true ex_wall 500 -1"#), "{}: {got}", r.backend);
            assert!(got.contains(r#""text":"then true false false door_a door_a""#), "{}: {got}", r.backend);
        }
        *COLLIDER.lock().unwrap() = None;
    }

    #[test]
    fn the_land_is_hit_where_it_is() {
        let c = Collider { exterior: true, ..Default::default() };
        // No world: every square flat at -2048.
        let h = c.cast([100.0, 100.0, 0.0], [100.0, 100.0, -5000.0], None, collision_type::DEFAULT).unwrap();
        assert!((h.0[2] + 2048.0).abs() < 0.01 && h.2.is_none() && (h.1[2] - 1.0).abs() < 1e-6, "{h:?}");
        assert!(c.cast([100.0, 100.0, 0.0], [100.0, 100.0, -1000.0], None, collision_type::DEFAULT).is_none());
        // Not asked for: the land is not hit; the water is, when asked.
        assert!(c.cast([100.0, 100.0, 0.0], [100.0, 100.0, -5000.0], None, collision_type::WORLD).is_none());
        let c = Collider { exterior: true, water: Some(0.0), ..Default::default() };
        let w = c.cast([100.0, 100.0, 500.0], [100.0, 100.0, -5000.0], None, collision_type::WATER | collision_type::HEIGHT_MAP).unwrap();
        assert!(w.0[2].abs() < 1e-6 && w.1[2] == 1.0, "the sea's surface: {w:?}");
        assert!((c.cast([100.0, 100.0, 500.0], [100.0, 100.0, -5000.0], None, collision_type::DEFAULT).unwrap().0[2] + 2048.0).abs() < 0.01, "under it by default");
    }

    /// ui.create: a window from a template (its content into the template's slot), a
    /// child found by name, a click sent back to the button's own callback, update and
    /// destroy.
    #[test]
    fn ui_elements_are_sent_and_answer_clicks() {
        let script = "local ui = require('openmw.ui') local util = require('openmw.util') local async = require('openmw.async')\n\
            local box = { type = ui.TYPE.Image, props = { resource = ui.texture { path = 'white' }, color = util.color.rgb(0, 0, 0) },\n\
              content = ui.content { { name = 'slot', external = { slot = true }, props = { position = util.vector2(4, 4) } } } }\n\
            local el\n\
            el = ui.create({ layer = 'Windows', template = box, props = { relativePosition = util.vector2(0.5, 0.5), anchor = util.vector2(0.5, 0.5) },\n\
              content = ui.content { { name = 'title', type = ui.TYPE.Text, props = { text = 'Hello', textSize = 18 } },\n\
                { name = 'ok', type = ui.TYPE.Text, props = { text = 'OK' }, events = { mouseClick = async:callback(function() print('clicked') el.layout.content.title.props.text = 'Bye' el:update() end) } } } })\n\
            return { engineHandlers = { onUpdate = function() end } }";
        let mut esc = String::from("\"");
        crate::json::escape_into(script, &mut esc);
        esc.push('"');
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/ui.lua","flags":["PLAYER"]}}],"sources":{{"scripts/t/ui.lua":{esc}}},"files":[],"weathers":["Clear"]}}"#);
        let mut r = Runner::new(None, true).unwrap();
        let a = r.call("__wg_start", &start).unwrap();
        assert!(a.contains(r#""k":"ui""#) && a.contains(r#""layer":"Windows""#), "{a}");
        // The template's image first, the node's content inside its slot.
        assert!(a.contains(r#""path":"white""#) && a.contains(r#""text":"Hello""#), "{a}");
        // The node's own content is put in the template's slot (paths `r.s1`...), the slot
        // itself is the template's (`r.t1`).
        assert!(a.contains(r#""name":"slot""#) && a.contains(r#""path":"r.t1""#) && a.contains(r#""path":"r.s1""#), "the content inside the slot: {a}");
        // The button's path, then a click on it.
        // The button is the slot's second (`r.s2`), the one with a click.
        let path = "r.s2";
        assert!(a.contains(r#""events":["mouseClick"]"#) && a.contains(r#""path":"r.s2""#), "{a}");
        let got = r.call("__wg_step", &format!(r#"{{"dt":0.05,"ui":[{{"id":"e1","path":"{path}","event":"mouseClick","x":3,"y":4}}]}}"#)).unwrap();
        assert!(got.contains(r#""text":"clicked""#) && got.contains(r#""text":"Bye""#), "{got}");
    }

    /// What mods on a real install called that the preview lacked (Static, 2026-11-03):
    /// `core.getRealFrameDuration`, `camera.getThirdPersonDistance`, `types.Player.quests`,
    /// `world.mwscript.getGlobalScript`; and a handler failing every frame is named once.
    #[test]
    fn what_mods_call_answers_and_errors_are_named_once() {
        let player = "local core = require('openmw.core') local camera = require('openmw.camera') local types = require('openmw.types') local self = require('openmw.self')\n\
            local done = false\n\
            return { engineHandlers = { onUpdate = function(dt) if done then return end done = true\n\
              local q = types.Player.quests(self)\n\
              print('frame', core.getRealFrameDuration(), 'tpd', camera.getThirdPersonDistance(), 'stage', q['MS_Lookout'].stage, q.ms_lookout.started)\n\
              q.ms_other:addJournalEntry(20) end } }";
        let global = "local world = require('openmw.world') local said = false\n\
            return { engineHandlers = { onUpdate = function() local s = world.mwscript.getGlobalScript('BountyScript')\n\
              if not said then said = true print('bounty', s.variables.cancel, world.mwscript.getGlobalScript('NotRunning')) end\n\
              error('every frame') end } }";
        let q = |s: &str| { let mut o = String::from("\""); crate::json::escape_into(s, &mut o); o.push('"'); o };
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/p.lua","flags":["PLAYER"]}},{{"path":"scripts/t/g.lua","flags":["GLOBAL"]}}],
            "sources":{{"scripts/t/p.lua":{},"scripts/t/g.lua":{}}},"files":[],"weathers":["Clear"]}}"#, q(player), q(global));
        let mut r = Runner::new(None, true).unwrap();
        r.call("__wg_start", &start).unwrap();
        let step = r#"{"dt":0.05,"journal":{"ms_lookout":10},"mwlocals":{"global:bountyscript":{"script":"BountyScript","vars":{"cancel":1}}}}"#;
        let got = r.call("__wg_step", step).unwrap();
        assert!(got.contains(r#""text":"frame 0.05 tpd 192 stage 10 true""#), "{got}");
        assert!(got.contains(r#""k":"journal""#) && got.contains(r#""id":"ms_other""#) && got.contains(r#""v":20"#), "{got}");
        assert!(got.contains(r#""text":"bounty 1 nil""#), "{got}");
        assert!(got.contains("every frame"), "{got}");
        let again = r.call("__wg_step", r#"{"dt":0.05}"#).unwrap();
        assert!(!again.contains("every frame"), "the same error is not named again: {again}");
    }

    /// What mods check before they start (Static, 2026-11-03): `core.API_REVISION` is the
    /// install's (NGarde refused 100, wanting 129), and `core.contentFiles` is the load order
    /// (Go Home! refused to start when `has` said every plugin was loaded).
    #[test]
    fn the_install_revision_and_the_load_orders_content() {
        let global = "local core = require('openmw.core') local said = false\n\
            return { engineHandlers = { onUpdate = function() if said then return end said = true\n\
              local cf = core.contentFiles\n\
              print('rev', core.API_REVISION, cf.has('morrowind.ESM'), cf.has('NPC Schedule.esp'), cf.indexOf('Mod.omwaddon'), cf.indexOf('nope.esp'), #cf.list) end } }";
        let q = |s: &str| { let mut o = String::from("\""); crate::json::escape_into(s, &mut o); o.push('"'); o };
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/g.lua","flags":["GLOBAL"]}}],
            "sources":{{"scripts/t/g.lua":{}}},"files":[],"weathers":["Clear"],"content":["Morrowind.esm","Mod.omwaddon"],"apiRevision":166}}"#, q(global));
        let mut r = Runner::new(None, true).unwrap();
        r.call("__wg_start", &start).unwrap();
        let got = r.call("__wg_step", r#"{"dt":0.05}"#).unwrap();
        assert!(got.contains(r#""text":"rev 166 true false 2 nil 2""#), "{got}");
    }

    /// An install's revision by its `resources/version`; a version newer than any known, or
    /// none, gets the newest release's.
    #[test]
    fn api_revision_from_the_install() {
        let d = std::env::temp_dir().join(format!("wg_apirev_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("version"), "0.51.0\r\nabc\r\n").unwrap();
        assert_eq!(api_revision(Some(&d)), 129);
        std::fs::write(d.join("version"), "0.52.0\r\n4ff261bf\r\n").unwrap();
        assert_eq!(api_revision(Some(&d)), 166);
        assert_eq!(api_revision(None), 166);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Keys over the Preview reach a player script's onKeyPress and input.isKeyPressed; an
    /// exterior square entered for the first time calls the global scripts' onNewExterior,
    /// once a square.
    #[test]
    fn keys_and_new_exteriors_reach_the_scripts() {
        let player = "local input = require('openmw.input')\n\
            return { engineHandlers = { onKeyPress = function(k) print('key', k.code == input.KEY.E, k.symbol, input.isKeyPressed(input.KEY.E)) end,\n\
              onKeyRelease = function(k) print('up', input.isKeyPressed(k.code)) end } }";
        let global = "return { engineHandlers = { onNewExterior = function(c) print('new', c.gridX, c.gridY) end } }";
        let q = |s: &str| { let mut o = String::from("\""); crate::json::escape_into(s, &mut o); o.push('"'); o };
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/p.lua","flags":["PLAYER"]}},{{"path":"scripts/t/g.lua","flags":["GLOBAL"]}}],
            "sources":{{"scripts/t/p.lua":{},"scripts/t/g.lua":{}}},"files":[],"weathers":["Clear"]}}"#, q(player), q(global));
        let mut r = Runner::new(None, true).unwrap();
        r.call("__wg_start", &start).unwrap();
        let a = r.call("__wg_step", r#"{"dt":0.05,"player":{"pos":[100,100,0],"cell":{"name":"","ext":true,"gx":-2,"gy":-9}},"keys":[{"code":8,"down":true,"symbol":"e"}]}"#).unwrap();
        assert!(a.contains(r#""text":"key true e true""#) && a.contains(r#""text":"new -2 -9""#), "{a}");
        let b = r.call("__wg_step", r#"{"dt":0.05,"keys":[{"code":8,"down":false}]}"#).unwrap();
        assert!(b.contains(r#""text":"up false""#) && !b.contains("new -2"), "once a square: {b}");
        let c = r.call("__wg_step", r#"{"dt":0.05,"player":{"pos":[9000,100,0],"cell":{"name":"","ext":true,"gx":-1,"gy":-9}}}"#).unwrap();
        assert!(c.contains(r#""text":"new -1 -9""#), "{c}");
    }

    /// OpenMW's own `mwui` templates (`WG_RESOURCES`): a player script's window from
    /// `I.MWUI.templates.boxSolid` and `textHeader` is drawn, with the border images.
    #[test]
    #[ignore]
    fn openmw_mwui_templates_draw() {
        let res = PathBuf::from(std::env::var("WG_RESOURCES").expect("WG_RESOURCES"));
        let mut r = Runner::new(None, true).unwrap();
        let script = "local ui = require('openmw.ui') local I = require('openmw.interfaces') local done = false\n\
            return { engineHandlers = { onUpdate = function() if done then return end done = true\n\
            ui.create({ layer = 'Windows', template = I.MWUI.templates.boxSolid, content = ui.content { { template = I.MWUI.templates.textHeader, props = { text = 'Mod window' } } } }) end } }";
        let vfs = Vfs::with_roots(&[]);
        let q = |s: &str| { let mut o = String::from("\""); crate::json::escape_into(s, &mut o); o.push('"'); o };
        let start = start_json(&vfs, Some(&res), &[], &["Clear".into()], &[])
            .replacen("\"scripts\":[", "\"scripts\":[{\"path\":\"scripts/t/w.lua\",\"flags\":[\"PLAYER\"]},", 1)
            .replacen("\"sources\":{", &format!("\"sources\":{{\"scripts/t/w.lua\":{},", q(script)), 1);
        let a = r.call("__wg_start", &start).unwrap();
        assert!(!a.contains("\"kind\":\"error\""), "{a}");
        let got = r.call("__wg_step", r#"{"dt":0.05}"#).unwrap();
        assert!(!got.contains("\"kind\":\"error\""), "{got}");
        assert!(got.contains(r#""text":"Mod window""#) && got.contains("menu_thin_border"), "{got}");
    }
}
