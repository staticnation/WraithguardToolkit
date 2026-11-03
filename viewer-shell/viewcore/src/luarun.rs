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
/// activation, music, weather, the world's events, settings storage). The camera, input,
/// UI, console, combat and spellcasting ones need the game itself.
pub const BUILTINS: [&str; 8] = [
    "scripts/omw/activationhandlers.lua",
    "scripts/omw/worldeventhandlers.lua",
    "scripts/omw/ai.lua",
    "scripts/omw/music/actor.lua",
    "scripts/omw/music/music.lua",
    "scripts/omw/weather.lua",
    "scripts/omw/cellhandlers.lua",
    "scripts/omw/settings/global.lua",
];

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
pub fn start_json(vfs: &Vfs, resources: Option<&Path>, registered: &[(String, Vec<String>)], weathers: &[String]) -> String {
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
            I.AI.startPackage({ type = 'Travel', destPosition = util.vector3(600, 0, 0) }) print('package', I.AI.getActivePackage().type) end } }";
        let vfs = Vfs::with_roots(&[]);
        let mut start = start_json(&vfs, Some(&res), &[], &["Clear".into(), "Rain".into()]);
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
        assert!(all.contains(r#""k":"ai""#) && all.contains(r#""kind":"travel""#), "ai.lua's _startAiTravel: {all}");
        assert!(all.contains(r#""k":"activate","key":"d1""#) || all.contains(r#""key":"d1","by":"player""#) || all.contains(r#""k":"activate""#), "activationhandlers' standard action: {all}");
        assert!(all.contains(r#""k":"music""#), "OpenMW's music playlists: {all}");
    }
}
