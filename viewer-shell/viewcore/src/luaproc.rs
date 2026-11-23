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

/// The worker: the Lua, answering the program until its input ends. Returns the exit code.
pub fn serve() -> i32 {
    let mut run: Option<Runner> = None;
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
                Some(r) => r.call(&s("func"), &s("arg")),
                None => Err("the Lua was not started".into()),
            },
            _ => continue,
        };
        let msg = match answer {
            Ok(v) => serde_json::json!({"t": "ok", "v": v}),
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
        let mut p = Proc { child, to, from, backend: String::new(), dead: false };
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
                        Some("ok") => return Ok(v),
                        Some("err") => return Err(v),
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
}
