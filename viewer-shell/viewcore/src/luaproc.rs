//! The Preview's Lua in a process of its own, which the program can stop (security, the
//! second audit's last open item, 2026-11-23).
//!
//! `luarun` stops a mod's script that runs too long with a count hook, and that hook only
//! fires between Lua instructions: a script that spends its time inside one native call -
//! `string.rep` of a few gigabytes, a pattern the matcher in C takes minutes over on LuaJIT,
//! a `table.concat` of everything - is past where the hook can reach it, and so was the
//! program, because the Lua ran on one of its threads. Here the Lua runs in a worker: the
//! program itself started again with `--lua-worker`, talking over its standard input and
//! output. Each call has a deadline, and a call past it ends the worker, whatever it is in
//! the middle of. The next start makes a new one.
//!
//! The line: one message a line, `WGL1 ` and JSON (anything else the worker prints - a
//! script's `print`, a test harness - is not ours and is passed over).
//!
//!   program -> worker   {"t":"new","jit":path|null,"force54":bool}
//!                       {"t":"col", ...}  what changed of the scene's collision (no answer)
//!                       {"t":"call","func":name,"arg":json}
//!   worker -> program   {"t":"ok","v":text}  {"t":"err","v":text}
//!                       {"t":"rec","kinds":"CLAS,RACE","id":id}  mid-call: a record of the
//!                       load order (`luarec`); the program answers {"t":"recv","v":json|null}
//!                       and the call goes on (Static 2026-11-23)
//!
//! `nearby.castRay` is answered in the worker, against its own copy of the scene's collision
//! (`luarun::Collider::apply`): the program sends what changed before each call - the models'
//! triangles once each, the objects' placings as they move, the land around the player. A ray
//! asked of the program in the middle of a call was a round trip each (Static 2026-11-23).

use std::io::{BufRead, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::luarun::{self, Runner};

const TAG: &str = "WGL1 ";

/// What a call that was stopped answers: the caller drops the worker.
pub const STOPPED: &str = "WG_LUA_STOPPED:";

fn send(out: &mut impl Write, v: &serde_json::Value) -> std::io::Result<()> {
    writeln!(out, "{TAG}{v}")?;
    out.flush()
}

fn read_msg(input: &mut impl BufRead) -> Option<serde_json::Value> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        // From the tag on: a line someone else began (a test harness's "test ... ") still counts.
        if let Some(j) = line.trim_end().split_once(TAG).map(|(_, j)| j) {
            if let Ok(v) = serde_json::from_str(j) {
                return Some(v);
            }
        }
    }
}

/// How much memory the worker may take (Static 2026-11-23): with LuaJIT's compiler on, a compiled
/// loop is past the prelude's memory check, so the process itself is capped. `WG_LUA_WORKER_MB`
/// sets it otherwise (the tests). An allocation past it fails: Lua answers "not enough memory" to
/// the script, and a failure outside Lua ends the worker, which the program reports as stopped.
pub const MEMORY_CAP_MB: u64 = 2048;

/// The worker's own memory capped at `mb`: the address space on Linux (`RLIMIT_AS`), the data
/// segment elsewhere on Unix (`RLIMIT_DATA`; macOS does not hold mmap to it, so there the
/// worker's deadline is what stops a runaway), and the process's committed memory on Windows
/// (a Job object it puts itself in). False when the system refused.
pub fn cap_memory(mb: u64) -> bool {
    let bytes = mb.saturating_mul(1024 * 1024);
    #[cfg(unix)]
    {
        #[cfg(target_os = "linux")]
        let what = libc::RLIMIT_AS;
        #[cfg(not(target_os = "linux"))]
        let what = libc::RLIMIT_DATA;
        let lim = libc::rlimit { rlim_cur: bytes as libc::rlim_t, rlim_max: bytes as libc::rlim_t };
        // SAFETY: setrlimit reads the struct it is given and changes only this process's limit.
        unsafe { libc::setrlimit(what, &lim) == 0 }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_PROCESS_MEMORY};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        // SAFETY: a new unnamed job, its limit set from a struct of the right size, and this
        // process put in it; the handle is kept for the process's life (the limit goes with it).
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return false;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            info.ProcessMemoryLimit = bytes as usize;
            let set = SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const core::ffi::c_void, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
            set != 0 && AssignProcessToJobObject(job, GetCurrentProcess()) != 0
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = bytes;
        false
    }
}

/// The worker: the Lua, answering the program until its input ends. Returns the exit code.
pub fn serve() -> i32 {
    // Capped before any Lua runs (`MEMORY_CAP_MB`).
    let mb = std::env::var("WG_LUA_WORKER_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(MEMORY_CAP_MB);
    cap_memory(mb);
    // A record the scripts ask for, asked of the program in the middle of the call.
    if let Ok(mut g) = luarun::RECORD_ASK.lock() {
        *g = Some(Box::new(|kinds: &str, id: &str| {
            send(&mut std::io::stdout().lock(), &serde_json::json!({"t": "rec", "kinds": kinds, "id": id})).ok()?;
            loop {
                let m = read_msg(&mut std::io::stdin().lock())?;
                if m.get("t").and_then(|t| t.as_str()) == Some("recv") {
                    return m.get("v").and_then(|v| v.as_str()).map(str::to_string);
                }
            }
        }));
    }
    let mut run: Option<Runner> = None;
    let mut took = 0.0;
    loop {
        let Some(m) = read_msg(&mut std::io::stdin().lock()) else { return 0 };
        let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let answer = match s("t").as_str() {
            "new" => {
                let jit = m.get("jit").and_then(|v| v.as_str()).map(Path::new);
                Runner::new(jit, m.get("force54").and_then(|v| v.as_bool()).unwrap_or(false)).map(|r| {
                    let b = r.backend.clone();
                    run = Some(r);
                    b
                })
            }
            // The scene's collision, for the rays: applied, and nothing answered.
            "col" => {
                if let Ok(mut c) = luarun::COLLIDER.lock() {
                    c.get_or_insert_with(Default::default).apply(&m);
                }
                continue;
            }
            "call" => match run.as_mut() {
                Some(r) => {
                    let t = Instant::now();
                    let got = r.call(&s("func"), &s("arg"));
                    took = t.elapsed().as_secs_f64() * 1000.0;
                    got
                }
                None => Err("the Lua was not started".into()),
            },
            _ => continue,
        };
        let msg = match answer {
            // `ms`: the call's own time in the worker, for the round trip's split.
            Ok(v) => serde_json::json!({"t": "ok", "v": v, "ms": took}),
            Err(e) => serde_json::json!({"t": "err", "v": e}),
        };
        if send(&mut std::io::stdout().lock(), &msg).is_err() {
            return 0;
        }
    }
}

/// The program's side: one worker, and the Lua in it.
pub struct Proc {
    child: Child,
    to: ChildStdin,
    from: Receiver<serde_json::Value>,
    /// What runs the scripts (`Runner::backend`), said by the worker.
    pub backend: String,
    dead: bool,
    /// The last call's time in the worker, ms (the round trip less the pipe).
    pub last_ms: f64,
    /// Who answers the scripts' records (`{"t":"rec"}`): the load order's (`luarec::Records`).
    pub records: Option<luarun::RecordAsk>,
}

impl Proc {
    /// A worker (`exe` with `args`, which start `serve`) with the Lua started in it: the
    /// install's LuaJIT at `jit` when it opens, else Lua 5.4. `limit`: how long starting may take.
    pub fn start(exe: &Path, args: &[&str], envs: &[(&str, &str)], jit: Option<&Path>, force_54: bool, limit: Duration) -> Result<Proc, String> {
        let mut c = Command::new(exe);
        c.args(args).envs(envs.iter().copied()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window for it (CREATE_NO_WINDOW).
            c.creation_flags(0x0800_0000);
        }
        let mut child = c.spawn().map_err(|e| format!("the Lua worker could not be started: {e}"))?;
        let to = child.stdin.take().ok_or("the Lua worker has no input")?;
        let out = child.stdout.take().ok_or("the Lua worker has no output")?;
        let (tx, from) = mpsc::channel();
        std::thread::spawn(move || {
            let mut r = std::io::BufReader::new(out);
            while let Some(m) = read_msg(&mut r) {
                if tx.send(m).is_err() {
                    break;
                }
            }
        });
        let mut p = Proc { child, to, from, backend: String::new(), dead: false, last_ms: 0.0, records: None };
        let jit = jit.map(|j| j.to_string_lossy().into_owned());
        p.backend = p.ask(&serde_json::json!({"t": "new", "jit": jit, "force54": force_54}), limit)?;
        Ok(p)
    }

    /// One of the prelude's entry points, as `Runner::call`, given `limit` to answer. Past
    /// it the worker is ended and this answers `STOPPED`; so does a worker that has died.
    pub fn call(&mut self, func: &str, arg: &str, limit: Duration) -> Result<String, String> {
        self.ask(&serde_json::json!({"t": "call", "func": func, "arg": arg}), limit)
    }

    /// What changed of the scene's collision, for the worker's rays (`Collider::apply`'s form,
    /// without `t`): sent ahead of the next call, no answer.
    pub fn send_col(&mut self, delta: &serde_json::Value) {
        if self.dead {
            return;
        }
        let mut m = delta.clone();
        if let Some(o) = m.as_object_mut() {
            o.insert("t".into(), "col".into());
        }
        if send(&mut self.to, &m).is_err() {
            self.end();
        }
    }

    /// Whether the worker is still there to ask.
    pub fn alive(&self) -> bool {
        !self.dead
    }

    fn ask(&mut self, msg: &serde_json::Value, limit: Duration) -> Result<String, String> {
        if self.dead {
            return Err(format!("{STOPPED} the Lua was stopped earlier; start it again"));
        }
        let deadline = Instant::now() + limit;
        if send(&mut self.to, msg).is_err() {
            self.end();
            return Err(format!("{STOPPED} the Lua worker has gone (it may have crashed)"));
        }
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.from.recv_timeout(left) {
                Ok(m) => {
                    let v = m.get("v").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    match m.get("t").and_then(|t| t.as_str()) {
                        Some("ok") => {
                            self.last_ms = m.get("ms").and_then(|x| x.as_f64()).unwrap_or(0.0);
                            return Ok(v);
                        }
                        Some("err") => return Err(v),
                        Some("rec") => {
                            let g = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let answer = self.records.as_mut().and_then(|f| f(&g("kinds"), &g("id")));
                            if send(&mut self.to, &serde_json::json!({"t": "recv", "v": answer})).is_err() {
                                self.end();
                                return Err(format!("{STOPPED} the Lua worker has gone (it may have crashed)"));
                            }
                        }
                        _ => {}
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.end();
                    return Err(format!("{STOPPED} a script ran for more than {} s in one call and the Lua was stopped", limit.as_secs()));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.end();
                    return Err(format!("{STOPPED} the Lua worker has gone (it may have crashed)"));
                }
            }
        }
    }

    fn end(&mut self) {
        self.dead = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if !self.dead {
            self.end();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worker, when this test binary is started as one (the tests below do).
    #[test]
    #[ignore]
    fn as_worker() {
        if std::env::var_os("WG_LUA_WORKER_TEST").is_some() {
            std::process::exit(serve());
        }
    }

    fn worker(limit: Duration) -> Result<Proc, String> {
        let exe = std::env::current_exe().unwrap();
        let jit = std::env::var("WG_LUAJIT").ok();
        Proc::start(
            &exe,
            &["--exact", "luaproc::tests::as_worker", "--include-ignored", "--nocapture", "--test-threads=1"],
            &[("WG_LUA_WORKER_TEST", "1")],
            jit.as_deref().map(Path::new),
            jit.is_none(),
            limit,
        )
    }

    #[test]
    fn a_scene_runs_in_the_worker_and_a_stuck_call_is_stopped() {
        let mut p = worker(Duration::from_secs(30)).unwrap();
        assert!(p.backend.contains("Lua"), "{}", p.backend);
        // The prelude answers as in the program's own thread.
        let script = "print('from the worker', 1 + 1)";
        let start = |text: &str| {
            let mut esc = String::from("\"");
            crate::json::escape_into(text, &mut esc);
            esc.push('"');
            format!(r#"{{"scripts":[{{"path":"scripts/t/a.lua","flags":["GLOBAL"]}}],"sources":{{"scripts/t/a.lua":{esc}}},"files":[],"weathers":["Clear"]}}"#)
        };
        let a = p.call("__wg_start", &start(script), Duration::from_secs(30)).unwrap();
        assert!(a.contains(r#""text":"from the worker 2""#), "{a}");
        // A script's own error is the script's, not the worker's: it carries on.
        assert!(p.call("__wg_no_such_entry", "", Duration::from_secs(5)).is_err());
        assert!(p.alive());

        // The Lua's own limit put far off, as one a native call would be past: the worker's
        // deadline ends it, and every call after says so.
        p.call("__wg_set_limits", "60,262144", Duration::from_secs(5)).unwrap();
        let t0 = Instant::now();
        let e = p.call("__wg_start", &start("while true do end"), Duration::from_millis(400)).unwrap_err();
        assert!(e.starts_with(STOPPED), "{e}");
        assert!(t0.elapsed() < Duration::from_secs(5), "{:?}", t0.elapsed());
        assert!(!p.alive());
        assert!(p.call("__wg_step", "{}", Duration::from_secs(5)).unwrap_err().starts_with(STOPPED));
    }

    /// A record the scene never placed, asked for by a script in the middle of a call: the
    /// worker asks the program (`{"t":"rec"}`), the program answers from the load order, and the
    /// call goes on; asked again, the worker keeps what it was told (Static 2026-11-23).
    #[test]
    fn a_record_is_asked_of_the_program_mid_call() {
        let mut p = worker(Duration::from_secs(30)).unwrap();
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen = asked.clone();
        p.records = Some(Box::new(move |kinds: &str, id: &str| {
            seen.lock().unwrap().push(format!("{kinds}:{id}"));
            (kinds == "CLAS" && id == "wg_thief").then(|| r#"{"id":"wg_thief","majorSkills":["sneak","security"],"recordType":"CLAS"}"#.to_string())
        }));
        let script = "local types = require('openmw.types')\n\
            local c = types.NPC.classes.record('wg_thief')\n\
            local again = types.NPC.classes.records['wg_thief']\n\
            print('class', c.majorSkills[1], again.majorSkills[2], types.NPC.classes.records['nobody'] ~= nil)";
        let mut esc = String::from("\"");
        crate::json::escape_into(script, &mut esc);
        esc.push('"');
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/rec.lua","flags":["GLOBAL"]}}],"sources":{{"scripts/t/rec.lua":{esc}}},"files":[],"weathers":["Clear"]}}"#);
        let a = p.call("__wg_start", &start, Duration::from_secs(30)).unwrap();
        assert!(a.contains(r#""text":"class sneak security true""#), "{a}");
        assert_eq!(*asked.lock().unwrap(), vec!["CLAS:wg_thief".to_string(), "CLAS:nobody".to_string()], "asked once each");
        assert!(p.alive());
    }

    /// The files a script reads and the settings the player saved (`luafiles`, Static
    /// 2026-11-23): a saved setting wins over the group's default; a text file, a listing and
    /// YAML for OpenMW's scripts; an MWSE mod's config, a JSON file, TOML and `io.open`.
    #[test]
    fn files_and_saved_settings_reach_the_scripts() {
        let mut p = worker(Duration::from_secs(30)).unwrap();
        p.records = Some(Box::new(|kind: &str, arg: &str| {
            let text = |a: &str| match a {
                "scripts/m/data.txt" => Some("first\nsecond"),
                "scripts/m/list.yaml" => Some("tracks:\n  - a.mp3\n  - b.mp3\n"),
                "mwse/config/wgmod.json" => Some(r#"{"volume": 7}"#),
                "mwse/config/extra.json" => Some(r#"{"n": 3}"#),
                "mwse/mods/wg/meta.toml" | "wg-metadata.toml" => Some("[package]\nname = \"WG\"\n"),
                _ => None,
            };
            let v = match kind {
                "@file" => serde_json::json!(text(&crate::luafiles::vfs_path(arg))?),
                "@list" => serde_json::json!(["scripts/m/data.txt", "scripts/m/list.yaml"]),
                "@yaml" => crate::luafiles::yaml_json(text(arg)?)?,
                "@toml" => crate::luafiles::toml_json(text(&crate::luafiles::vfs_path(arg))?)?,
                _ => return None,
            };
            Some(v.to_string())
        }));
        let omw = "local storage = require('openmw.storage') local vfs = require('openmw.vfs') local markup = require('openmw.markup')\n\
            local s = storage.playerSection('SettingsWg')\n\
            if s:get('style') == nil then s:set('style', 'default') end\n\
            local f = vfs.open('scripts\\\\m\\\\data.txt') local first = f:read('*l')\n\
            local n = 0 for _ in vfs.pathsWithPrefix('scripts/m/') do n = n + 1 end\n\
            local y = markup.loadYaml('scripts/m/list.yaml')\n\
            print('omw', s:get('style'), s:get('pos').x, first, n, y.tracks[2], vfs.fileExists('scripts/m/nope.txt'))\n\
            print('kept', s:get('max') == math.huge, s:get('byNum')[3], s:get('byNum')['3'] == nil, s:get('obj') == nil)\n\
            return {}";
        let mwse = "local c = mwse.loadConfig('wgmod', { volume = 1, mute = false })\n\
            local e = json.loadfile('config/extra')\n\
            local t = toml.loadMetadata('wg')\n\
            local f = io.open('Data Files\\\\MWSE\\\\config\\\\wgmod.json')\n\
            mwse.log('mwse %d %s %d %s %s %s', c.volume, tostring(c.mute), e.n, t.package.name, f and 'open' or 'nil', tostring(io.open('x.txt', 'w')))";
        let esc = |t: &str| {
            let mut e = String::from("\"");
            crate::json::escape_into(t, &mut e);
            e.push('"');
            e
        };
        let start = format!(
            r#"{{"storage":{{"player":{{"SettingsWg":{{"style":"mine","pos":{{"$v2":[0.25,0.5]}},"max":{{"$raw":"00000000000000f07f","$num":"inf"}},"byNum":{{"$table":[[3,"x"],[7,"y"]]}},"obj":{{"$raw":"92616263"}}}}}}}},"scripts":[{{"path":"scripts/t/omw.lua","flags":["PLAYER"]}}],"sources":{{"scripts/t/omw.lua":{},"mwse/mods/wg/main.lua":{}}},"mwse":["mwse/mods/wg/main.lua"],"files":[],"weathers":["Clear"]}}"#,
            esc(omw),
            esc(mwse)
        );
        let a = p.call("__wg_start", &start, Duration::from_secs(30)).unwrap();
        assert!(a.contains(r#""text":"omw mine 0.25 first 2 b.mp3 false""#), "{a}");
        assert!(a.contains(r#""text":"kept true x true true""#), "an infinity, a number key and a value only kept for writing back: {a}");
        assert!(a.contains(r#""text":"mwse 7 false 3 WG open nil""#), "{a}");
    }

    /// The rays are the worker's own: the scene's collision is sent to it (`send_col`) and a
    /// script's `nearby.castRay` meets it with no round trip; the land as well.
    #[test]
    fn a_ray_in_the_worker_meets_what_the_program_sent() {
        let mut p = worker(Duration::from_secs(30)).unwrap();
        let quad: Vec<f32> = [[-500.0f32, 0.0, 0.0], [500.0, 0.0, 0.0], [500.0, 0.0, 400.0], [-500.0, 0.0, 0.0], [500.0, 0.0, 400.0], [-500.0, 0.0, 400.0]].concat();
        let heights = vec![50.0f32; 65 * 65];
        p.send_col(&serde_json::json!({
            "exterior": true,
            "models": {"meshes/wall.nif": luarun::f32s_b64(&quad)},
            "objects": {"w1": {"model": "meshes/wall.nif", "m": [1,0,0,0, 0,1,0,500, 0,0,1,100], "origin": [0, 0], "enabled": true, "kind": 1}},
            "lands": [[0, 0, luarun::f32s_b64(&heights)]],
        }));
        let script = "local nearby = require('openmw.nearby') local util = require('openmw.util') local done = false\n\
            return { engineHandlers = { onUpdate = function() if done then return end done = true\n\
            local a = nearby.castRay(util.vector3(0, 0, 200), util.vector3(0, 1000, 200))\n\
            local g = nearby.castRay(util.vector3(100, 100, 300), util.vector3(100, 100, -300))\n\
            print('hit', a.hitObject and a.hitObject.recordId, math.floor(a.hitPos.y + 0.5), g.hit, g.hitPos and math.floor(g.hitPos.z + 0.5)) end } }";
        let mut esc = String::from("\"");
        crate::json::escape_into(script, &mut esc);
        esc.push('"');
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/ray.lua","flags":["NPC"]}}],"sources":{{"scripts/t/ray.lua":{esc}}},"files":[],"weathers":["Clear"]}}"#);
        p.call("__wg_start", &start, Duration::from_secs(30)).unwrap();
        let got = p.call("__wg_step", r#"{"dt":0.05,"objects":{"n1":{"id":"guard","tag":"NPC_","pos":[0,0,0],"cell":{"name":"","ext":true,"gx":0,"gy":0}},
            "w1":{"id":"ex_wall","tag":"STAT","pos":[0,500,100],"cell":{"name":"","ext":true,"gx":0,"gy":0}}}}"#, Duration::from_secs(30)).unwrap();
        assert!(got.contains(r#""text":"hit ex_wall 500 true 50""#), "{got}");
    }

    /// A script that eats memory in a loop the hook does not watch (the limit put far off) is
    /// stopped by the worker's own cap, long before its deadline; the program goes on.
    #[test]
    fn a_script_that_eats_memory_is_stopped_by_the_worker_s_cap() {
        let exe = std::env::current_exe().unwrap();
        let jit = std::env::var("WG_LUAJIT").ok();
        let mut p = Proc::start(
            &exe,
            &["--exact", "luaproc::tests::as_worker", "--include-ignored", "--nocapture", "--test-threads=1"],
            &[("WG_LUA_WORKER_TEST", "1"), ("WG_LUA_WORKER_MB", "768")],
            jit.as_deref().map(Path::new),
            jit.is_none(),
            Duration::from_secs(30),
        )
        .unwrap();
        p.call("__wg_set_limits", "60,1000000000", Duration::from_secs(5)).unwrap();
        let script = "local t = {} for i = 1, 100000 do t[i] = string.rep('x', 1048576) .. i end print('ate it all')";
        let mut esc = String::from("\"");
        crate::json::escape_into(script, &mut esc);
        esc.push('"');
        let start = format!(r#"{{"scripts":[{{"path":"scripts/t/eat.lua","flags":["GLOBAL"]}}],"sources":{{"scripts/t/eat.lua":{esc}}},"files":[],"weathers":["Clear"]}}"#);
        let t0 = Instant::now();
        let got = p.call("__wg_start", &start, Duration::from_secs(60));
        assert!(t0.elapsed() < Duration::from_secs(40), "{:?}", t0.elapsed());
        // On LuaJIT the script is told and the worker goes on; on Lua 5.4 the worker ends at the
        // cap and the call says it was stopped. Either way the program is untouched.
        match got {
            Ok(a) => assert!(a.contains("not enough memory") && !a.contains("ate it all"), "{a}"),
            Err(e) => assert!(e.starts_with(STOPPED), "{e}"),
        }
    }
}
