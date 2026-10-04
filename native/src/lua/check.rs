//! Scripts checked by the Teal compiler, embedded through htl (ynishi/htl, MIT OR
//! Apache-2.0, which carries tl 0.24.8, MIT): no Lua install, no luarocks.
//!
//! A `.tl` file is checked as Teal; a `.lua` file as plain Lua, where Teal infers what it
//! can and lets the rest be. Either way the declarations of [`super::teal`] answer
//! `require("openmw.*")`, and the setup's data folders answer a script's requires of its
//! own modules (`require("scripts.mymod.util")`), as OpenMW's virtual file system does.
//!
//! Each diagnostic carries a `kind` from its wording, so the caller can keep what is
//! reliable for plain Lua (a key a documented record does not have, the wrong number of
//! arguments, an unknown global) and leave out what is only Teal's inference giving up.

use std::path::{Path, PathBuf};

use htl_core::Htl;
use htl_core::diagnostic::Severity;

/// One finding.
#[derive(Debug, Clone)]
pub struct Diag {
    pub line: usize,
    pub col: usize,
    /// `error`, `warning` or `lint`.
    pub severity: &'static str,
    /// `invalid_key`, `arity`, `unknown_variable`, `module_not_found`, `syntax`,
    /// `unused_variable`, `unused_function`, `unused_argument`, `unused`, `lint` or `type`.
    pub kind: &'static str,
    /// Teal's rule name (`tl:error`, `tl:unused`, `nil-index`, ...), when it has one.
    pub rule: Option<String>,
    pub message: String,
}

fn kind_of(severity: Severity, rule: Option<&str>, message: &str, syntax: bool) -> &'static str {
    if syntax {
        return "syntax";
    }
    if matches!(severity, Severity::Lint) {
        return "lint";
    }
    if rule == Some("tl:unused") || message.starts_with("unused ") {
        return if message.starts_with("unused variable") {
            "unused_variable"
        } else if message.starts_with("unused function") {
            "unused_function"
        } else if message.starts_with("unused argument") {
            "unused_argument"
        } else {
            "unused"
        };
    }
    if message.starts_with("invalid key '") {
        "invalid_key"
    } else if message.starts_with("wrong number of arguments") {
        "arity"
    } else if message.starts_with("unknown variable: ") {
        "unknown_variable"
    } else if message.starts_with("module not found") {
        "module_not_found"
    } else {
        "type"
    }
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "lint",
    }
}

/// Checks each file. `include` are folders of modules, lowest priority first: the data
/// folders in load order, then the declarations. The outer error is a checker that could
/// not start; an inner one is a file the checker could not read.
///
/// The files are shared out over threads, each with a checker (a Lua state) of its own -
/// a state belongs to one thread - so a big load order takes a fraction of the time.
/// Nothing here touches Python.
pub fn check(paths: &[PathBuf], include: &[PathBuf]) -> anyhow::Result<Vec<Result<Vec<Diag>, String>>> {
    // A checker costs ~0.1 s to start (it loads the Teal compiler), so a thread is only
    // worth it for a few files.
    const PER_THREAD_MIN: usize = 4;
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = cores.min(paths.len().div_ceil(PER_THREAD_MIN)).max(1);
    if threads == 1 {
        return check_serial(paths, include);
    }
    let chunk = paths.len().div_ceil(threads);
    let parts: Vec<anyhow::Result<Vec<Result<Vec<Diag>, String>>>> = std::thread::scope(|s| {
        let handles: Vec<_> = paths.chunks(chunk).map(|part| s.spawn(move || check_serial(part, include))).collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err(anyhow::anyhow!("a checker thread panicked"))))
            .collect()
    });
    let mut out = Vec::with_capacity(paths.len());
    for part in parts {
        out.extend(part?);
    }
    Ok(out)
}

/// [`check`] on this thread, with one checker.
fn check_serial(paths: &[PathBuf], include: &[PathBuf]) -> anyhow::Result<Vec<Result<Vec<Diag>, String>>> {
    let h = Htl::new()?;
    // `add_path` puts a folder in front of those already added.
    for dir in include {
        if dir.is_dir() {
            h.add_path(dir)?;
        }
    }
    Ok(paths.iter().map(|p| check_one(&h, p)).collect())
}

fn check_one(h: &Htl, path: &Path) -> Result<Vec<Diag>, String> {
    let info = h.check(path).map_err(|e| format!("{e:#}"))?;
    let syntax = info.syntax_errors > 0;
    let mut out: Vec<Diag> = info
        .diagnostics()
        .into_iter()
        .map(|d| {
            let rule = d.rule.clone();
            let is_error = matches!(d.severity, Severity::Error);
            Diag {
                line: d.line,
                col: d.col,
                severity: severity_str(d.severity),
                kind: kind_of(d.severity, rule.as_deref(), &d.message, syntax && is_error),
                rule,
                message: d.message,
            }
        })
        .collect();
    out.sort_by_key(|d| (d.line, d.col));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_wording() {
        assert_eq!(kind_of(Severity::Error, Some("tl:error"), "invalid key 'actor' in record 'nearby'", false), "invalid_key");
        assert_eq!(kind_of(Severity::Error, None, "wrong number of arguments (given 3, expects 2)", false), "arity");
        assert_eq!(kind_of(Severity::Warning, Some("tl:unused"), "unused argument dt: any", false), "unused_argument");
        assert_eq!(kind_of(Severity::Error, None, "expected an expression", true), "syntax");
        assert_eq!(kind_of(Severity::Error, None, "argument 1: got string, expected number", false), "type");
    }

    #[test]
    fn threads_give_the_same_answers_in_the_same_order() {
        let dir = std::env::temp_dir().join(format!("wg_lua_par_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = (0..12)
            .map(|i| {
                let p = dir.join(format!("s{i}.lua"));
                // Each file's one unknown global is on its own line number.
                std::fs::write(&p, format!("{}missing{i}()\n", "\n".repeat(i))).unwrap();
                p
            })
            .collect();
        let all = check(&paths, &[]).unwrap();
        for (i, r) in all.iter().enumerate() {
            let d = r.as_ref().unwrap();
            assert_eq!(d.len(), 1, "{d:?}");
            assert_eq!((d[0].line, d[0].kind), (i + 1, "unknown_variable"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_plain_lua_against_declarations() {
        let dir = std::env::temp_dir().join(format!("wg_lua_check_{}", std::process::id()));
        let decl = dir.join("decl");
        let data = dir.join("data");
        std::fs::create_dir_all(decl.join("openmw")).unwrap();
        std::fs::create_dir_all(data.join("scripts").join("m")).unwrap();
        std::fs::write(
            decl.join("openmw").join("nearby.d.tl"),
            "local record nearby\n   actors: {any}\n   castRay: function(a: number, b: number): any\nend\nreturn nearby\n",
        )
        .unwrap();
        std::fs::write(data.join("scripts").join("m").join("util.lua"), "return { twice = function(x) return x * 2 end }\n").unwrap();
        let script = data.join("scripts").join("m").join("main.lua");
        std::fs::write(
            &script,
            "local nearby = require('openmw.nearby')\nlocal util = require('scripts.m.util')\nprint(#nearby.actor, util.twice(2))\nnearby.castRay(1, 2, 3)\nnoSuchThing()\n",
        )
        .unwrap();
        let res = check(&[script], &[data.clone(), decl.clone()]).unwrap();
        let diags = res[0].as_ref().unwrap();
        let kinds: Vec<&str> = diags.iter().map(|d| d.kind).collect();
        assert!(kinds.contains(&"invalid_key"), "{diags:?}");
        assert!(kinds.contains(&"arity"), "{diags:?}");
        assert!(kinds.contains(&"unknown_variable"), "{diags:?}");
        assert!(!kinds.contains(&"module_not_found"), "{diags:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
