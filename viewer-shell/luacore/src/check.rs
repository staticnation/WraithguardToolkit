//! Scripts checked by the Teal compiler, embedded through htl (ynishi/htl, MIT OR
//! Apache-2.0, which carries tl 0.24.8, MIT): no Lua install, no luarocks.
//!
//! Moved here from `native/src/lua/check.rs` (4.6, folding `native/`), behind the `teal`
//! feature: the viewer's Lua panel runs it ([`teal_findings`]), as the Tk Lua window did.
//!
//! A `.tl` file is checked as Teal; a `.lua` file as plain Lua, where Teal infers what it
//! can and lets the rest be. Either way the declarations of [`crate::teal`] answer
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
pub fn check(paths: &[PathBuf], include: &[PathBuf]) -> anyhow::Result<Vec<Result<Vec<Diag>, String>>> {
    check_with(paths, include, &[])
}

/// [`check`], with modules whose globals every file sees: a Cyan project's
/// `global_env_def` (`tlconfig.lua`), Teal's `--global-env-def`.
pub fn check_with(
    paths: &[PathBuf],
    include: &[PathBuf],
    globals: &[String],
) -> anyhow::Result<Vec<Result<Vec<Diag>, String>>> {
    // A checker costs ~0.1 s to start (it loads the Teal compiler), so a thread is only
    // worth it for a few files.
    const PER_THREAD_MIN: usize = 4;
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = cores.min(paths.len().div_ceil(PER_THREAD_MIN)).max(1);
    if threads == 1 {
        return check_serial(paths, include, globals);
    }
    let chunk = paths.len().div_ceil(threads);
    let parts: Vec<anyhow::Result<Vec<Result<Vec<Diag>, String>>>> = std::thread::scope(|s| {
        let handles: Vec<_> =
            paths.chunks(chunk).map(|part| s.spawn(move || check_serial(part, include, globals))).collect();
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

/// [`check_with`] on this thread, with one checker.
fn check_serial(
    paths: &[PathBuf],
    include: &[PathBuf],
    globals: &[String],
) -> anyhow::Result<Vec<Result<Vec<Diag>, String>>> {
    let h = Htl::new()?;
    // `add_path` puts a folder in front of those already added.
    for dir in include {
        if dir.is_dir() {
            h.add_path(dir)?;
        }
    }
    if let Some(program) = with_globals(&h, globals) {
        return Ok(paths.iter().map(|p| check_one(&program, p)).collect());
    }
    Ok(paths.iter().map(|p| check_one(&h, p)).collect())
}

/// A program on `checker` whose environment predefines `globals` (Teal's
/// `predefined_modules`, what `--global-env-def` sets). htl makes its environment with
/// `tl.new_env` and has no option for them, so the checker's `tl.new_env` is wrapped to
/// add them, and a program started on it (`Htl::with_checker`) gets an environment made
/// that way. None when there are none, or when the modules cannot be loaded (the checks
/// then run without them, as before).
fn with_globals(checker: &Htl, globals: &[String]) -> Option<Htl> {
    let names: Vec<&String> = globals
        .iter()
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .collect();
    if names.is_empty() {
        return None;
    }
    let list = names.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>().join(", ");
    let code = format!(
        "local tl = require('tl')\n\
         local make = tl.new_env\n\
         tl.new_env = function(opts)\n\
           opts = opts or {{}}\n\
           opts.predefined_modules = {{ {list} }}\n\
           return make(opts)\n\
         end\n"
    );
    checker.lua().load(code).set_name("=wg-global-env-def").exec().ok()?;
    Htl::with_checker(checker).ok()
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

/// The packages stubbed when no install is read: OpenMW's, as of 0.51
/// (`openmw_api.py`'s `_STUB_PACKAGES`).
pub const STUB_PACKAGES: &[&str] = &[
    "openmw.ambient", "openmw.animation", "openmw.async", "openmw.camera", "openmw.content", "openmw.core",
    "openmw.debug", "openmw.input", "openmw.interfaces", "openmw.markup", "openmw.menu", "openmw.nearby",
    "openmw.postprocessing", "openmw.self", "openmw.storage", "openmw.types", "openmw.ui", "openmw.util",
    "openmw.vfs", "openmw.world", "openmw_aux.calendar", "openmw_aux.time", "openmw_aux.ui", "openmw_aux.util",
];

/// What a Teal pass did: the line for the report's header, and each script's findings
/// (`None` for one the checker could not read; the reason is dropped, as the Python did).
pub struct TealPass {
    pub description: String,
    pub checked: usize,
    pub findings: Vec<Option<Vec<crate::analysis::Finding>>>,
}

/// The whole Teal pass over a setup's scripts (`openmw_api.py`'s `teal_setup` and
/// `add_teal_findings`): declarations written from the install's documentation (or stubs
/// with none) into a temporary folder, each script checked - as the `.tl` beside it when
/// there is one, its typed source - and the diagnostics turned into findings.
///
/// `files`: each script's file on disk (`None` for one packed in an archive: not checked).
/// `data_dirs`: the setup's data folders, lowest priority first. `resources`: the install's
/// `resources` folder, when found.
pub fn teal_findings(files: &[Option<PathBuf>], data_dirs: &[PathBuf], resources: Option<&Path>) -> anyhow::Result<TealPass> {
    use crate::{findings, parser, scan, teal};
    let docs = resources.and_then(crate::ldt::read_resources);
    let folder = std::env::temp_dir().join(format!(
        "wg_teal_decl_{}_{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
    ));
    let _ = std::fs::remove_dir_all(&folder);
    let (description, members, stubs) = match &docs {
        Some(api) => {
            teal::write_declarations(api, &folder)?;
            let ver = api.version.as_deref().map_or_else(|| "OpenMW".to_string(), |v| format!("OpenMW {v}"));
            let members: std::collections::HashMap<String, Vec<String>> = teal::members(api).into_iter().collect();
            (format!("{ver}'s documented Lua API (read from {})", resources.map(|r| r.display().to_string()).unwrap_or_default()), members, false)
        }
        None => {
            let pk: Vec<String> = STUB_PACKAGES.iter().map(|s| s.to_string()).collect();
            teal::write_stubs(&pk, &folder)?;
            ("no OpenMW install found: API checks off".to_string(), Default::default(), true)
        }
    };
    // Each job: the file to check, whether it is Teal, and the .lua whose names count as bound.
    let jobs: Vec<(usize, PathBuf, bool, PathBuf)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let f = f.as_ref().filter(|f| f.is_file())?;
            let tl = f.with_extension("tl");
            Some(if tl.is_file() { (i, tl, true, f.clone()) } else { (i, f.clone(), false, f.clone()) })
        })
        .collect();
    // Lowest priority first: OpenMW's own files, the data folders, the folders the mods'
    // own Teal projects name (tlconfig.lua), the declarations.
    let mut include: Vec<PathBuf> = Vec::new();
    if let Some(vfs) = resources.map(|r| r.join("vfs")).filter(|v| v.is_dir()) {
        include.push(vfs);
    }
    include.extend(data_dirs.iter().cloned());
    include.extend(findings::tlconfig_dirs(data_dirs));
    include.push(folder.clone());
    let globals = findings::tlconfig_globals(data_dirs);
    let paths: Vec<PathBuf> = jobs.iter().map(|j| j.1.clone()).collect();
    let res = if paths.is_empty() { Ok(Vec::new()) } else { check_with(&paths, &include, &globals) };
    let _ = std::fs::remove_dir_all(&folder);
    let res = res?;
    let mut out: Vec<Option<Vec<crate::analysis::Finding>>> = vec![None; files.len()];
    for (r, (i, _, is_teal, src)) in res.into_iter().zip(&jobs) {
        if let Ok(diags) = r {
            let bound = scan::read_text(src)
                .ok()
                .and_then(|text| parser::parse(&text, false).ok())
                .map(|tree| crate::analysis::bound_names(&tree))
                .unwrap_or_default();
            let refs: Vec<findings::DiagRef<'_>> = diags
                .iter()
                .map(|d| findings::DiagRef { kind: d.kind, severity: d.severity, line: d.line, message: &d.message })
                .collect();
            out[*i] = Some(findings::findings_for(&refs, *is_teal, &bound, &members, stubs));
        }
    }
    Ok(TealPass { description, checked: jobs.len(), findings: out })
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

    #[test]
    fn the_whole_pass_without_an_install_uses_stubs() {
        let dir = std::env::temp_dir().join(format!("wg_lua_pass_{}", std::process::id()));
        let data = dir.join("data");
        std::fs::create_dir_all(data.join("scripts").join("m")).unwrap();
        let script = data.join("scripts").join("m").join("main.lua");
        std::fs::write(&script, "local core = require('openmw.core')\nnoSuchThing()\nreturn {}\n").unwrap();
        let got = teal_findings(&[Some(script), None], &[data], None).unwrap();
        assert_eq!(got.checked, 1);
        assert!(got.description.starts_with("no OpenMW install"));
        assert!(got.findings[1].is_none());
        let f = got.findings[0].as_ref().unwrap();
        assert!(f.iter().any(|x| x.2 == 2), "{f:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
