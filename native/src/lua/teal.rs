//! Teal declarations (`.d.tl`) for OpenMW's Lua API, written from what [`super::ldt`] read.
//!
//! Every type goes into one module, `wg_openmw_api.d.tl`, as a type nested in its record
//! (named `<module>__<Type>`), so packages that refer to each other (`openmw.core` and
//! `openmw.types` do, both ways) need no `require` between them. Each package is then a
//! two-line `.d.tl` at the path a script requires (`openmw/nearby.d.tl`) that returns its
//! field of that record. A type another type `@extends` is declared as an `interface` and
//! its children as `record X is Parent`, which is what lets `openmw.self` be passed where a
//! `GameObject` is expected. Whatever the documentation leaves untyped is `any`, the same
//! as OpenMW's own generated declarations do, and every parameter is optional (see
//! [`func_type`]).
//!
//! The files are written to a folder the caller chooses (a cache in the user's profile):
//! they are derived from the user's own install and are never shipped.
//!
//! [`write_stubs`] writes the fallback for a setup with no OpenMW install to read: each
//! known package as `any`, so a script's requires resolve and the checks that need no API
//! (unknown globals, unused locals, its own functions' arity) still run.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use super::ldt::{Api, Func, Module, Type, TypeRef};

/// The module every package's declaration returns a field of.
pub const API_MODULE: &str = "wg_openmw_api";

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil",
    "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Globals of OpenMW's Lua (LuaJIT, Lua 5.1) that Teal's standard library, which follows
/// Lua 5.4, does not have.
const EXTRA_GLOBALS: &str = "global unpack: function<T>(t: {T}, i?: integer, j?: integer): T...\n";

/// A declared name Teal accepts as a field: an identifier, not a Lua keyword.
fn field_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !LUA_KEYWORDS.contains(&name)
}

/// A module's key in type names: `openmw.core` -> `core`, `openmw_aux.util` -> `aux_util`,
/// interface `AI` -> `I_AI`.
fn module_key(m: &Module) -> String {
    let base = match &m.interface {
        Some(i) => format!("I_{i}"),
        None => m.require.strip_prefix("openmw.").unwrap_or(&m.require).replace("openmw_", "").to_string(),
    };
    base.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// Where each type is declared: `(module index, type name)` -> its Teal name.
struct Names {
    ids: HashMap<(usize, String), String>,
    /// require path -> module index (packages, not interfaces).
    by_require: HashMap<String, usize>,
    /// type name -> modules that have it, for a `#Name` that is not the file's own.
    by_name: HashMap<String, Vec<usize>>,
}

impl Names {
    fn new(api: &Api, keys: &[String]) -> Self {
        let mut ids = HashMap::new();
        let mut by_require = HashMap::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, m) in api.modules.iter().enumerate() {
            if m.interface.is_none() {
                by_require.entry(m.require.clone()).or_insert(i);
            }
            for name in m.types.keys() {
                let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
                ids.insert((i, name.clone()), format!("{}__{}", keys[i], safe));
                by_name.entry(name.clone()).or_default().push(i);
            }
        }
        Names { ids, by_require, by_name }
    }

    /// The Teal name of a documented type, seen from module `from`.
    fn resolve(&self, from: usize, module: &Option<String>, name: &str) -> Option<&String> {
        let at = match module {
            Some(path) => *self.by_require.get(path)?,
            None => {
                if self.ids.contains_key(&(from, name.to_string())) {
                    from
                } else {
                    *self.by_name.get(name)?.first()?
                }
            }
        };
        self.ids.get(&(at, name.to_string()))
    }

    fn teal(&self, from: usize, t: &TypeRef) -> String {
        match t {
            TypeRef::Any => "any".into(),
            TypeRef::Prim(p) => match p.as_str() {
                "string" | "number" | "integer" | "boolean" | "nil" | "thread" => p.clone(),
                "table" => "{any:any}".into(),
                "function" => "function(...: any): any...".into(),
                _ => "any".into(),
            },
            TypeRef::Named { module, name } => self.resolve(from, module, name).cloned().unwrap_or_else(|| "any".into()),
            TypeRef::List(inner) => format!("{{{}}}", self.teal(from, inner)),
            TypeRef::Map(k, v) => {
                let k = self.teal(from, k);
                // A map keyed by anything but a string or number is a table of any.
                let k = if k == "string" || k == "number" || k == "integer" { k } else { "any".into() };
                format!("{{{}:{}}}", k, self.teal(from, v))
            }
        }
    }
}

/// A function type: `function(self: T, a: A, b?: B, ...: C): R`.
fn func_type(names: &Names, from: usize, owner: &str, f: &Func) -> String {
    let mut ps: Vec<String> = Vec::new();
    if f.method {
        ps.push(format!("self: {owner}"));
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for p in &f.params {
        let ty = names.teal(from, &p.ty);
        if p.vararg {
            ps.push(format!("...: {ty}"));
            break;
        }
        // Every parameter is optional: the documentation says which ones are only now and
        // then (in prose, or not at all - `camera.setMode(mode, force)` is called with one),
        // so too many arguments is what a check can trust, not too few.
        let mut name = if field_ok(&p.name) { p.name.clone() } else { format!("arg{}", ps.len() + 1) };
        while !seen.insert(name.clone()) {
            name.push('_');
        }
        ps.push(format!("{name}?: {ty}"));
    }
    // An undocumented return may still return something: `any...` lets a script use it.
    let rets: Vec<String> = f.returns.iter().map(|r| names.teal(from, r)).collect();
    let ret = match rets.len() {
        0 => ": any...".to_string(),
        _ => format!(": {}", rets.join(", ")),
    };
    format!("function({}){}", ps.join(", "), ret)
}

/// Every member name a type has through what it extends, for not declaring one twice.
fn inherited(api: &Api, names: &Names, mi: usize, t: &Type, depth: usize, out: &mut BTreeSet<String>) {
    if depth > 8 {
        return;
    }
    for e in &t.extends {
        if let TypeRef::Named { module, name } = e {
            let at = match module {
                Some(p) => names.by_require.get(p).copied(),
                None => {
                    if api.modules[mi].types.contains_key(name) {
                        Some(mi)
                    } else {
                        names.by_name.get(name).and_then(|v| v.first().copied())
                    }
                }
            };
            if let Some(at) = at
                && let Some(pt) = api.modules[at].types.get(name) {
                    out.extend(pt.fields.iter().map(|f| f.name.clone()));
                    out.extend(pt.functions.iter().map(|f| f.name.clone()));
                    inherited(api, names, at, pt, depth + 1, out);
                }
        }
    }
}

/// Every declared type's members by its Teal name (`core__GameObject`), what it inherits
/// included: what a "did you mean" looks among when Teal names a key a type lacks.
pub fn members(api: &Api) -> BTreeMap<String, Vec<String>> {
    let keys: Vec<String> = api.modules.iter().map(module_key).collect();
    let names = Names::new(api, &keys);
    let mut out = BTreeMap::new();
    for (i, m) in api.modules.iter().enumerate() {
        for t in m.types.values() {
            let mut all: BTreeSet<String> = t.fields.iter().map(|f| f.name.clone()).collect();
            all.extend(t.functions.iter().map(|f| f.name.clone()));
            inherited(api, &names, i, t, 0, &mut all);
            out.insert(names.ids[&(i, t.name.clone())].clone(), all.into_iter().collect());
        }
    }
    out
}

/// The text of `wg_openmw_api.d.tl`, and each package's `(require path, declaration)`.
pub fn declarations(api: &Api) -> (String, Vec<(String, String)>) {
    let keys: Vec<String> = api.modules.iter().map(module_key).collect();
    let names = Names::new(api, &keys);
    // Types something extends are interfaces.
    let mut parents: BTreeSet<String> = BTreeSet::new();
    for (i, m) in api.modules.iter().enumerate() {
        for t in m.types.values() {
            for e in &t.extends {
                if let TypeRef::Named { module, name } = e
                    && let Some(id) = names.resolve(i, module, name) {
                        parents.insert(id.clone());
                    }
            }
        }
    }
    let mut out = String::new();
    let _ = writeln!(out, "-- Generated by Wraithguard from the OpenMW Lua API documentation of this install.");
    if let Some(v) = &api.version {
        let _ = writeln!(out, "-- OpenMW {v}");
    }
    let _ = writeln!(out, "local record {API_MODULE}");
    let mut emitted: BTreeSet<String> = BTreeSet::new();
    for (i, m) in api.modules.iter().enumerate() {
        for t in m.types.values() {
            let id = names.ids[&(i, t.name.clone())].clone();
            if !emitted.insert(id.clone()) {
                continue;
            }
            let has_members = !t.fields.is_empty() || !t.functions.is_empty() || !t.extends.is_empty();
            if !has_members && t.list_of.is_none()
                && let Some((k, v)) = &t.map_of {
                    let ty = names.teal(i, &TypeRef::Map(Box::new(k.clone()), Box::new(v.clone())));
                    let _ = writeln!(out, "   type {id} = {ty}");
                    continue;
                }
            let kind = if parents.contains(&id) { "interface" } else { "record" };
            let mut is: Vec<String> = Vec::new();
            if let Some(l) = &t.list_of {
                is.push(format!("{{{}}}", names.teal(i, l)));
            }
            for e in &t.extends {
                if let TypeRef::Named { module, name } = e
                    && let Some(p) = names.resolve(i, module, name)
                        && p != &id && !is.contains(p) {
                            is.push(p.clone());
                        }
            }
            let is = if is.is_empty() { String::new() } else { format!(" is {}", is.join(", ")) };
            let _ = writeln!(out, "   {kind} {id}{is}");
            let mut skip = BTreeSet::new();
            inherited(api, &names, i, t, 0, &mut skip);
            let mut seen = BTreeSet::new();
            for f in &t.fields {
                if field_ok(&f.name) && !skip.contains(&f.name) && seen.insert(f.name.clone()) {
                    let _ = writeln!(out, "      {}: {}", f.name, names.teal(i, &f.ty));
                }
            }
            for f in &t.functions {
                if field_ok(&f.name) && !skip.contains(&f.name) && seen.insert(f.name.clone()) {
                    let _ = writeln!(out, "      {}: {}", f.name, func_type(&names, i, &id, f));
                }
            }
            let _ = writeln!(out, "   end");
        }
    }
    // One field per package, holding its table.
    let mut packages: BTreeMap<String, String> = BTreeMap::new();
    for (i, m) in api.modules.iter().enumerate() {
        if m.interface.is_some() || m.require == "openmw.interfaces" {
            continue;
        }
        if packages.contains_key(&m.require) {
            continue;
        }
        let id = names.ids[&(i, m.name.clone())].clone();
        let _ = writeln!(out, "   {}: {}", keys[i], id);
        packages.insert(m.require.clone(), keys[i].clone());
    }
    // `openmw.interfaces`: the built-in interfaces typed as the install documents them, and
    // any other name - a mod's interface, or one looked up by a variable - as `any`.
    let _ = writeln!(out, "   record {INTERFACES}");
    let mut named = BTreeSet::new();
    for (i, m) in api.modules.iter().enumerate() {
        if let Some(name) = &m.interface
            && field_ok(name)
            && m.types.contains_key(&m.name)
            && named.insert(name.clone())
        {
            let _ = writeln!(out, "      {}: {}", name, names.ids[&(i, m.name.clone())]);
        }
    }
    let _ = writeln!(out, "      metamethod __index: function(self: {INTERFACES}, key: string): any");
    let _ = writeln!(out, "   end");
    let _ = writeln!(out, "   interfaces: {INTERFACES}");
    let _ = writeln!(out, "end");
    out.push_str(EXTRA_GLOBALS);
    let _ = writeln!(out, "return {API_MODULE}");
    let mut files: Vec<(String, String)> = packages
        .into_iter()
        .map(|(req, key)| (req, format!("local api = require(\"{API_MODULE}\")\nreturn api.{key}\n")))
        .collect();
    files.push(("openmw.interfaces".into(), format!("local api = require(\"{API_MODULE}\")\nreturn api.interfaces\n")));
    (out, files)
}

/// The record `openmw.interfaces` is, in the API module.
const INTERFACES: &str = "interfaces__all";

/// `openmw.interfaces` with no install to read: what interfaces exist is unknown, so a map
/// of any.
fn interfaces_decl() -> String {
    format!("local _api = require(\"{API_MODULE}\")\nlocal interfaces: {{string:any}}\nreturn interfaces\n")
}

/// The path of a module's declaration under the output folder: `openmw.nearby` ->
/// `openmw/nearby.d.tl`.
fn decl_path(out: &Path, require: &str) -> std::path::PathBuf {
    let mut p = out.to_path_buf();
    let parts: Vec<&str> = require.split('.').collect();
    for d in &parts[..parts.len() - 1] {
        p.push(d);
    }
    p.push(format!("{}.d.tl", parts[parts.len() - 1]));
    p
}

/// Writes the declarations into `out` (emptied first). Returns how many packages.
pub fn write_declarations(api: &Api, out: &Path) -> std::io::Result<usize> {
    let (main, files) = declarations(api);
    reset_dir(out)?;
    std::fs::write(out.join(format!("{API_MODULE}.d.tl")), main)?;
    for (req, text) in &files {
        let p = decl_path(out, req);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, text)?;
    }
    Ok(files.len())
}

/// Writes the no-install fallback: each package in `packages` as `any`.
pub fn write_stubs(packages: &[String], out: &Path) -> std::io::Result<usize> {
    reset_dir(out)?;
    let main = format!("-- Generated by Wraithguard: no OpenMW install to read.\nlocal record {API_MODULE}\nend\n{EXTRA_GLOBALS}return {API_MODULE}\n");
    std::fs::write(out.join(format!("{API_MODULE}.d.tl")), main)?;
    let mut n = 0;
    for req in packages {
        let p = decl_path(out, req);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let text = if req == "openmw.interfaces" {
            interfaces_decl()
        } else {
            format!("local _api = require(\"{API_MODULE}\")\nlocal m: any\nreturn m\n")
        };
        std::fs::write(p, text)?;
        n += 1;
    }
    Ok(n)
}

fn reset_dir(out: &Path) -> std::io::Result<()> {
    if out.exists() {
        std::fs::remove_dir_all(out)?;
    }
    std::fs::create_dir_all(out)
}

#[cfg(test)]
mod tests {
    use super::super::ldt::parse_file;
    use super::*;

    const CORE: &str = "---\n-- @module core\n-- @context global|local\n-- @usage local core = require('openmw.core')\n\n---\n-- @type GameObject\n-- @field #string recordId\n\n---\n-- @function [parent=#GameObject] isValid\n-- @param self\n-- @return #boolean\n\n---\n-- @type ObjectList\n-- @list <#GameObject>\n";
    const SELF: &str = "---\n-- @module Self\n-- @context local\n-- @usage local self = require('openmw.self')\n-- @extends openmw.core#GameObject\n\n---\n-- @field [parent=#Self] #table controls\n";

    fn api() -> Api {
        let mut modules = parse_file(CORE, "lua_api/openmw/core.lua", None);
        modules.extend(parse_file(SELF, "lua_api/openmw/self.lua", None));
        Api { version: Some("0.51.0".into()), modules }
    }

    #[test]
    fn writes_interfaces_records_and_package_files() {
        let (main, files) = declarations(&api());
        assert!(main.contains("interface core__GameObject\n"), "{main}");
        assert!(main.contains("record self__Self is core__GameObject\n"), "{main}");
        assert!(main.contains("isValid: function(self: core__GameObject): boolean"), "{main}");
        assert!(main.contains("record core__ObjectList is {core__GameObject}"), "{main}");
        assert!(main.contains("   core: core__core\n") && main.contains("   self: self__Self\n"), "{main}");
        assert!(main.contains("-- OpenMW 0.51.0"));
        let reqs: Vec<&str> = files.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(reqs, vec!["openmw.core", "openmw.self", "openmw.interfaces"]);
        assert!(files[0].1.contains("return api.core"));
        let mem = members(&api());
        assert_eq!(mem["self__Self"], vec!["controls", "isValid", "recordId"]);
    }

    #[test]
    fn interfaces_are_typed_and_open() {
        let doc = "---\n-- @module AI\n-- @context local\n-- @usage require('openmw.interfaces').AI\n\n---\n-- @function [parent=#AI] startPackage\n-- @param #table p\n";
        let mut modules = parse_file(doc, "ai.lua", Some("AI"));
        modules.extend(api().modules);
        let (main, files) = declarations(&Api { version: None, modules });
        assert!(main.contains("   record interfaces__all\n      AI: I_AI__AI\n"), "{main}");
        assert!(main.contains("metamethod __index: function(self: interfaces__all, key: string): any"), "{main}");
        let (_, text) = files.iter().find(|(r, _)| r == "openmw.interfaces").expect("interfaces file");
        assert!(text.contains("return api.interfaces"), "{text}");
    }

    #[test]
    fn params_are_optional_and_undocumented_returns_are_open() {
        let doc = "---\n-- @module m\n-- @usage require('openmw.m')\n\n---\n-- @function [parent=#m] f\n-- @param #number a (optional) first\n-- @param #string b\n";
        let a = Api { version: None, modules: parse_file(doc, "x.lua", None) };
        let (main, _) = declarations(&a);
        assert!(main.contains("f: function(a?: number, b?: string): any..."), "{main}");
    }
}
