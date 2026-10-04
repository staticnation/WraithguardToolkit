//! OpenMW's Lua API, read from the documentation every OpenMW install carries.
//!
//! OpenMW describes its Lua API in Lua Development Tools (LDT) doc comments: the
//! `openmw.*` packages in `resources/lua_api/openmw/*.lua`, and the built-in scripts'
//! interfaces and the `openmw_aux.*` helpers in the Lua files under `resources/vfs/`.
//! Those files are GPLv3 and stay where they are: this reads them from the user's own
//! install at run time, so the toolkit ships none of them, and the API it checks against
//! is the one the setup's OpenMW actually has.
//!
//! The comment format (as OpenMW writes it):
//!
//! ```text
//! ---
//! -- @module nearby                      a package; `@context local`, `@usage require('openmw.nearby')`
//! -- @type GameObject                    a type; `@extends #Other`, parentless `@field`s are its
//! -- @field [parent=#nearby] openmw.core#ObjectList actors
//! -- @function [parent=#GameObject] isValid
//! -- @param self                         a method
//! -- @param #string name (optional) ...
//! -- @return #boolean
//! ```
//!
//! Types are `#name` (this file's type, or a primitive), `some.module#Name`,
//! `#list<T>` and `#map<K, V>`; a tag with no type leaves it unknown.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A type as the documentation writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    /// Not stated, or nothing this can name.
    Any,
    /// `string`, `number`, `boolean`, `integer`, `nil`, `table`, `function`, `userdata`, `thread`.
    Prim(String),
    /// A documented type: `module` is the module path when given (`openmw.core`), else the
    /// type belongs to the file it is written in.
    Named { module: Option<String>, name: String },
    /// `#list<T>`.
    List(Box<TypeRef>),
    /// `#map<K, V>`.
    Map(Box<TypeRef>, Box<TypeRef>),
}

/// A field of a type or package.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeRef,
}

/// A function's parameter.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    /// The description says "(optional)".
    pub optional: bool,
    /// `...`.
    pub vararg: bool,
}

/// A function of a type or package.
#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    /// Without `self`.
    pub params: Vec<Param>,
    pub returns: Vec<TypeRef>,
    /// Its first parameter is `self`: called as `obj:name()`.
    pub method: bool,
}

/// A documented type, or a package's own table (named as its `@module`).
#[derive(Debug, Clone, Default)]
pub struct Type {
    pub name: String,
    pub extends: Vec<TypeRef>,
    pub fields: Vec<Field>,
    pub functions: Vec<Func>,
    /// `@list <T>`: the type is a list of T.
    pub list_of: Option<TypeRef>,
    /// `@map <K, V>`: the type is a map.
    pub map_of: Option<(TypeRef, TypeRef)>,
}

/// One documented module: an `openmw.*` package, an `openmw_aux.*` helper, or a built-in
/// interface (`require('openmw.interfaces').AI`).
#[derive(Debug, Clone, Default)]
pub struct Module {
    /// The `@module` name; the package's own table is the type of that name.
    pub name: String,
    /// What a script requires (`openmw.nearby`), or `openmw.interfaces` for an interface.
    pub require: String,
    /// The interface name (`AI`) when this is an interface.
    pub interface: Option<String>,
    /// Where it may be used (`global`, `local`, `player`, `menu`, `load`).
    pub contexts: Vec<String>,
    /// The file it was read from, relative to `resources/`.
    pub file: String,
    /// Its types by name, the package's own table among them.
    pub types: BTreeMap<String, Type>,
}

/// Everything read from an install.
#[derive(Debug, Clone, Default)]
pub struct Api {
    /// The first line of `resources/version` (`0.51.0`), when there is one.
    pub version: Option<String>,
    pub modules: Vec<Module>,
}

const PRIMS: &[&str] = &["string", "number", "boolean", "integer", "nil", "table", "function", "userdata", "thread"];

/// Reads the API from an install's `resources` folder.
///
/// Returns None when the folder has no `lua_api/openmw` (not an OpenMW install, or one too
/// old to have a Lua API).
pub fn read_resources(resources: &Path) -> Option<Api> {
    let api_dir = resources.join("lua_api").join("openmw");
    if !api_dir.is_dir() {
        return None;
    }
    let version = std::fs::read_to_string(resources.join("version"))
        .ok()
        .and_then(|t| t.lines().next().map(|l| l.trim().to_string()))
        .filter(|v| !v.is_empty());
    let mut files: Vec<PathBuf> = read_dir_sorted(&api_dir).into_iter().filter(|p| is_lua(p)).collect();
    let vfs = resources.join("vfs");
    let mut more = Vec::new();
    walk(&vfs, &mut more);
    more.sort();
    files.extend(more);
    let mut api = Api { version, modules: Vec::new() };
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        if !text.contains("@module") {
            continue;
        }
        let rel = f.strip_prefix(resources).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let default_require = if rel.starts_with("lua_api/openmw/") { Some(format!("openmw.{stem}")) } else { None };
        api.modules.extend(parse_file(&text, &rel, default_require.as_deref()));
    }
    Some(api)
}

fn is_lua(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lua"))
}

fn read_dir_sorted(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for p in read_dir_sorted(dir) {
        if p.is_dir() {
            walk(&p, out);
        } else if is_lua(&p) {
            out.push(p);
        }
    }
}

/// One doc block's tags: `(tag, rest of the line)`.
type Block = Vec<(String, String)>;

/// The doc blocks of a file: runs of `--` comment lines, a new block at each `---` line.
fn blocks(text: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut cur: Option<Block> = None;
    for line in text.lines() {
        let t = line.trim_start();
        if !t.starts_with("--") || t.starts_with("--[[") {
            if let Some(b) = cur.take() {
                out.push(b);
            }
            continue;
        }
        if t.starts_with("---") {
            if let Some(b) = cur.take() {
                out.push(b);
            }
            cur = Some(Vec::new());
        }
        let body = t.trim_start_matches('-').trim_start();
        if let Some(rest) = body.strip_prefix('@') {
            let (tag, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            cur.get_or_insert_with(Vec::new).push((tag.to_string(), rest.trim().to_string()));
        } else {
            cur.get_or_insert_with(Vec::new);
        }
    }
    if let Some(b) = cur {
        out.push(b);
    }
    out.retain(|b| !b.is_empty());
    out
}

/// `[parent=#X]` at the start of a tag, and the rest.
fn parent_of(rest: &str) -> (Option<String>, &str) {
    let r = rest.trim_start();
    if let Some(inner) = r.strip_prefix("[parent=")
        && let Some(end) = inner.find(']') {
            let p = inner[..end].trim();
            let p = p.rsplit('#').next().unwrap_or(p).to_string();
            return (Some(p), inner[end + 1..].trim_start());
        }
    (None, r)
}

/// Whether the text at the start of a tag is a type (types always carry a `#`, or are a
/// bare `list<...>`/`map<...>`).
fn starts_type(s: &str) -> bool {
    if s.starts_with('#') || s.starts_with("list<") || s.starts_with("map<") {
        return true;
    }
    let word: String = s.chars().take_while(|c| !c.is_whitespace()).collect();
    word.contains('#') && word.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Splits a type token off the front: up to whitespace outside `<...>`.
fn take_type(s: &str) -> (&str, &str) {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => return (&s[..i], s[i..].trim_start()),
            _ => {}
        }
    }
    (s, "")
}

/// Parses a type token.
pub fn parse_type(tok: &str) -> TypeRef {
    let t = tok.trim().trim_end_matches([',', '.', ';']);
    let t = t.strip_prefix('#').unwrap_or(t);
    if let Some(inner) = t.strip_prefix("list<").and_then(|r| r.strip_suffix('>')) {
        return TypeRef::List(Box::new(parse_type(inner)));
    }
    if let Some(inner) = t.strip_prefix("map<").and_then(|r| r.strip_suffix('>')) {
        // Split at the comma outside nested brackets.
        let mut depth = 0i32;
        for (i, c) in inner.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth == 0 => {
                    return TypeRef::Map(Box::new(parse_type(&inner[..i])), Box::new(parse_type(&inner[i + 1..])));
                }
                _ => {}
            }
        }
        return TypeRef::Map(Box::new(TypeRef::Any), Box::new(parse_type(inner)));
    }
    if let Some((module, name)) = t.split_once('#') {
        let name = ident_prefix(name);
        if name.is_empty() {
            return TypeRef::Any;
        }
        return TypeRef::Named { module: Some(module.replace('_', ".")), name };
    }
    let name = ident_prefix(t);
    if name.is_empty() || name == "any" {
        TypeRef::Any
    } else if PRIMS.contains(&name.as_str()) {
        TypeRef::Prim(name)
    } else {
        TypeRef::Named { module: None, name }
    }
}

/// The identifier at the start of a text (`halfExtents.` -> `halfExtents`).
fn ident_prefix(s: &str) -> String {
    s.trim().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

/// `@param [type] name description`, `@field [type] name description`.
fn typed_name(rest: &str) -> (TypeRef, String, &str) {
    let rest = rest.trim_start();
    let (ty, rest) = if starts_type(rest) {
        let (tok, r) = take_type(rest);
        (parse_type(tok), r)
    } else {
        (TypeRef::Any, rest)
    };
    let (name, desc) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let name = if name.starts_with("...") { "...".to_string() } else { ident_prefix(name) };
    (ty, name, desc)
}

/// `@return #a, #b description` -> the types (none for an untyped return).
fn return_types(rest: &str) -> Vec<TypeRef> {
    let mut out = Vec::new();
    let mut r = rest.trim_start();
    while starts_type(r) {
        let (tok, next) = take_type(r);
        out.push(parse_type(tok));
        let more = tok.ends_with(',');
        r = next;
        if !more {
            break;
        }
    }
    if out.is_empty() {
        out.push(TypeRef::Any);
    }
    out
}

/// The module a `@usage` line requires: `(require name, interface name)`.
fn usage_require(usage: &str) -> Option<(String, Option<String>)> {
    let at = usage.find("require(")?;
    let rest = &usage[at + "require(".len()..];
    let q = rest.chars().next()?;
    if q != '\'' && q != '"' {
        return None;
    }
    let end = rest[1..].find(q)? + 1;
    let name = rest[1..end].to_string();
    let after = rest[end + 1..].strip_prefix(')')?;
    let iface = after.strip_prefix('.').map(ident_prefix).filter(|s| !s.is_empty());
    Some((name, iface))
}

/// Reads the modules one file documents.
pub fn parse_file(text: &str, rel: &str, default_require: Option<&str>) -> Vec<Module> {
    let mut modules: Vec<Module> = Vec::new();
    let mut current: Option<String> = None; // the type parentless tags attach to
    for block in blocks(text) {
        let tag = |name: &str| block.iter().find(|(t, _)| t == name).map(|(_, r)| r.as_str());
        if let Some(name) = tag("module") {
            let name = ident_prefix(name);
            let contexts = tag("context")
                .map(|c| c.split('|').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();
            let usage = block.iter().filter(|(t, _)| t == "usage").find_map(|(_, r)| usage_require(r));
            let (require, interface) = match usage {
                Some(u) => u,
                None => match default_require {
                    Some(d) => (d.to_string(), None),
                    None => continue,
                },
            };
            let mut m = Module { name: name.clone(), require, interface, contexts, file: rel.to_string(), types: BTreeMap::new() };
            m.types.insert(name.clone(), Type { name: name.clone(), ..Default::default() });
            modules.push(m);
            current = Some(name);
        }
        let Some(m) = modules.last_mut() else { continue };
        if let Some(t) = tag("type") {
            let name = ident_prefix(t);
            if !name.is_empty() {
                m.types.entry(name.clone()).or_insert_with(|| Type { name: name.clone(), ..Default::default() });
                current = Some(name);
            }
        }
        let is_type_block = tag("type").is_some() || tag("module").is_some();
        let cur = current.clone();
        if is_type_block
            && let Some(c) = &cur {
                let ty = m.types.get_mut(c).expect("inserted above");
                for (t, r) in &block {
                    match t.as_str() {
                        "extends" => {
                            let e = parse_type(take_type(r).0);
                            if !matches!(e, TypeRef::Prim(_) | TypeRef::Any) {
                                ty.extends.push(e);
                            }
                        }
                        "list" => ty.list_of = Some(parse_type(r.trim().trim_start_matches('<').trim_end_matches('>'))),
                        "map" => {
                            if let TypeRef::Map(k, v) = parse_type(&format!("map{}", r.trim())) {
                                ty.map_of = Some((*k, *v));
                            }
                        }
                        _ => {}
                    }
                }
            }
        // Fields: `[parent=#X]` names the owner, else the block's own type.
        for (t, r) in &block {
            if t != "field" {
                continue;
            }
            let (parent, rest) = parent_of(r);
            let owner = match (parent, is_type_block) {
                (Some(p), _) => p,
                (None, true) => match &cur {
                    Some(c) => c.clone(),
                    None => continue,
                },
                (None, false) => continue,
            };
            let (ty, name, _) = typed_name(rest);
            if name.is_empty() || name == "..." {
                continue;
            }
            let o = m.types.entry(owner.clone()).or_insert_with(|| Type { name: owner.clone(), ..Default::default() });
            if !o.fields.iter().any(|f| f.name == name) {
                o.fields.push(Field { name, ty });
            }
        }
        if let Some(f) = tag("function") {
            let (parent, rest) = parent_of(f);
            let name = ident_prefix(rest);
            let owner = parent.or_else(|| Some(m.name.clone())).expect("set");
            if name.is_empty() {
                continue;
            }
            let mut params = Vec::new();
            let mut method = false;
            let mut returns = Vec::new();
            for (t, r) in &block {
                match t.as_str() {
                    "param" => {
                        let (ty, pname, desc) = typed_name(r);
                        if params.is_empty() && !method && pname == "self" {
                            method = true;
                            continue;
                        }
                        if pname.is_empty() {
                            continue;
                        }
                        let lower = desc.to_ascii_lowercase();
                        let optional = lower.contains("(optional") || lower.starts_with("optional");
                        let vararg = pname == "...";
                        params.push(Param { name: pname, ty, optional, vararg });
                    }
                    "return" => returns.extend(return_types(r)),
                    _ => {}
                }
            }
            let o = m.types.entry(owner.clone()).or_insert_with(|| Type { name: owner.clone(), ..Default::default() });
            if !o.functions.iter().any(|x| x.name == name) && !o.fields.iter().any(|x| x.name == name) {
                o.functions.push(Func { name, params, returns, method });
            }
        }
    }
    modules
}

#[cfg(test)]
mod tests {
    use super::*;

    // Written for these tests in OpenMW's comment format; not taken from OpenMW's files.
    const DOC: &str = r#"
---
-- Things near the player.
-- @context local
-- @module near
-- @usage local near = require('openmw.near')

---
-- The things.
-- @field [parent=#near] openmw.base#ThingList things

---
-- @type Mode
-- @field [parent=#Mode] #number Fast
-- @field [parent=#Mode] #number Slow

---
-- @field [parent=#near] #Mode MODE

---
-- Looks along a line.
-- @function [parent=#near] look
-- @param openmw.util#Vector3 from Where from.
-- @param #table options (optional) How.
-- @return openmw.base#Thing, #number

---
-- A spot.
-- @type Spot
-- @extends openmw.base#Thing
-- @field #map<#string, #boolean> flags Its flags.
-- @field #list<#Spot> neighbours

---
-- Whether it is free.
-- @function [parent=#Spot] isFree
-- @param self
-- @param ... anything
-- @return #boolean
"#;

    #[test]
    fn reads_modules_types_fields_and_functions() {
        let ms = parse_file(DOC, "lua_api/openmw/near.lua", None);
        assert_eq!(ms.len(), 1);
        let m = &ms[0];
        assert_eq!(m.require, "openmw.near");
        assert_eq!(m.contexts, vec!["local"]);
        let near = &m.types["near"];
        assert_eq!(near.fields[0].name, "things");
        assert_eq!(near.fields[0].ty, TypeRef::Named { module: Some("openmw.base".into()), name: "ThingList".into() });
        assert_eq!(near.fields[1].ty, TypeRef::Named { module: None, name: "Mode".into() });
        let look = &near.functions[0];
        assert_eq!(look.name, "look");
        assert!(!look.method);
        assert_eq!(look.params.len(), 2);
        assert!(!look.params[0].optional && look.params[1].optional);
        assert_eq!(look.returns.len(), 2);
        assert_eq!(m.types["Mode"].fields.len(), 2);
        let spot = &m.types["Spot"];
        assert_eq!(spot.extends.len(), 1);
        assert_eq!(
            spot.fields[0].ty,
            TypeRef::Map(Box::new(TypeRef::Prim("string".into())), Box::new(TypeRef::Prim("boolean".into())))
        );
        assert!(matches!(spot.fields[1].ty, TypeRef::List(_)));
        let free = &spot.functions[0];
        assert!(free.method);
        assert!(free.params[0].vararg);
    }

    #[test]
    fn interfaces_take_their_name_from_usage() {
        let src = "return {\n  --- An interface\n  -- @module Helper\n  -- @context local\n  -- @usage require('openmw.interfaces').Helper\n  interface = {\n    --- @field [parent=#Helper] #number version\n  }\n}\n";
        let ms = parse_file(src, "vfs/scripts/x.lua", None);
        assert_eq!(ms[0].require, "openmw.interfaces");
        assert_eq!(ms[0].interface.as_deref(), Some("Helper"));
        assert_eq!(ms[0].types["Helper"].fields[0].name, "version");
    }

    #[test]
    fn untyped_tags_are_any() {
        assert_eq!(parse_type("#string"), TypeRef::Prim("string".into()));
        assert_eq!(typed_name("formId String returned").0, TypeRef::Any);
        assert_eq!(return_types("Nothing useful"), vec![TypeRef::Any]);
    }
}
