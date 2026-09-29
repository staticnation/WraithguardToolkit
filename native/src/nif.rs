//! Meshes (`.nif`), on `tes3::nif`: what Resource Conflicts reports about a mesh, the
//! mesh viewer's block panel, and its field editor.
//!
//! - `summary`: shapes with their vertex and triangle counts, the textures named,
//!   collision, animation, node and block counts (`wraithguard/nif/report.py` Structure).
//! - `blocks`: the block tree (from the crate's links) and every block's fields - its
//!   `Debug` form flattened into rows, big arrays as a count - with the fields the
//!   editor can change marked editable (`wraithguard/nif/inspect.py`).
//! - `edit`: those fields set through typed setters and the file saved by the crate,
//!   which writes the blocks reachable from the roots (`wraithguard/nif/edit.py`).

use std::collections::HashMap;
use std::io;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde_json::{Value, json};
use tes3::nif::*;

fn bad(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// NetImmerse 4.0.0.0 files (some older exporters), which Morrowind and OpenMW load
/// with the same block layout as 4.0.0.2 - the only version the crate accepts - are
/// handed to it as 4.0.0.2. Anything else is left as it is.
fn as_4002(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const V4000: &[u8; 40] = b"NetImmerse File Format, Version 4.0.0.0\n";
    let text_ok = bytes.len() >= 44 && (&bytes[..40] == V4000 || bytes[..40] == NiStream::HEADER);
    if text_ok && bytes[40..44] == 0x0400_0000u32.to_le_bytes() {
        let mut b = bytes.to_vec();
        b[..40].copy_from_slice(&NiStream::HEADER);
        b[40..44].copy_from_slice(&NiStream::VERSION.to_le_bytes());
        return std::borrow::Cow::Owned(b);
    }
    std::borrow::Cow::Borrowed(bytes)
}

fn load(bytes: &[u8]) -> io::Result<NiStream> {
    let b = as_4002(bytes);
    crate::guarded(|| NiStream::from_bytes(&b))
}

fn type_name(o: &NiType) -> String {
    String::from_utf8_lossy(o.type_name()).into_owned()
}

/// The blocks in file order (the order the crate loaded them), and each key's index.
fn order(stream: &NiStream) -> (Vec<NiKey>, HashMap<NiKey, usize>) {
    let keys: Vec<NiKey> = stream.objects.keys().collect();
    let index = keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    (keys, index)
}

fn name_of(o: &NiType) -> String {
    <&NiObjectNET>::try_from(o).map(|n| n.name.clone()).unwrap_or_default()
}

fn normalise_texture(p: &str) -> String {
    p.trim().replace('\\', "/").to_lowercase()
}

fn external_texture(o: &NiType) -> Option<String> {
    match <&NiSourceTexture>::try_from(o) {
        Ok(t) => match &t.source {
            TextureSource::External(s) => Some(s.clone()),
            _ => None,
        },
        Err(()) => None,
    }
}

// ---- summary -----------------------------------------------------------------------

/// What a mesh contains, as `report.py`'s Structure. A file the crate cannot read comes
/// back with no blocks read and the reason, so it is reported as unreadable rather than
/// as empty.
pub fn summary(bytes: &[u8]) -> Value {
    let declared = if bytes.len() >= 48 { u32::from_le_bytes(bytes[44..48].try_into().unwrap()) } else { 0 };
    let stream = match load(bytes) {
        Ok(s) => s,
        Err(e) => {
            return json!({"shapes": [], "textures": [], "has_collision": false, "has_animation": false,
                          "node_count": 0, "blocks_read": 0, "blocks_declared": declared,
                          "stopped_reason": e.to_string()});
        }
    };
    let mut shapes = Vec::new();
    let mut textures: Vec<String> = Vec::new();
    let (mut collision, mut animation, mut nodes) = (false, false, 0usize);
    for (_, o) in stream.objects.iter() {
        let t = type_name(o);
        if t == "NiTriShape"
            && let Ok(shape) = <&NiTriShape>::try_from(o)
        {
            let (mut v, mut tris) = (0usize, 0usize);
            if let Some(NiType::NiTriShapeData(d)) = stream.objects.get(shape.geometry_data.key) {
                v = d.vertices.len();
                tris = d.triangles.len();
            }
            shapes.push(json!({"name": shape.name.clone(), "vertices": v, "triangles": tris}));
        }
        if let Some(tex) = external_texture(o)
            && !tex.trim().is_empty()
        {
            let n = normalise_texture(&tex);
            if !textures.contains(&n) {
                textures.push(n);
            }
        }
        collision |= t == "RootCollisionNode";
        animation |= t.ends_with("Controller");
        nodes += usize::from(t.ends_with("Node"));
    }
    json!({"shapes": shapes, "textures": textures, "has_collision": collision, "has_animation": animation,
           "node_count": nodes, "blocks_read": stream.objects.len(), "blocks_declared": declared,
           "stopped_reason": ""})
}

// ---- blocks: tree and fields -------------------------------------------------------

/// A short summary of a block for its tree row (as geometry.py's block_tree said it).
fn note_of(stream: &NiStream, o: &NiType) -> String {
    let t = type_name(o);
    if let NiType::NiTriShapeData(d) = o {
        return format!("{} verts, {} tris", d.vertices.len(), d.triangles.len());
    }
    if t == "NiSourceTexture" {
        return external_texture(o).map(|s| normalise_texture(&s)).unwrap_or_else(|| "embedded".into());
    }
    if t == "RootCollisionNode" {
        return "collision".into();
    }
    if t.ends_with("Controller") {
        return "animated".into();
    }
    if t == "NiSkinInstance" {
        return "skinned".into();
    }
    let _ = stream;
    String::new()
}

/// Every block's place in the tree: a node's children under it first (the scene graph
/// decides), then anything else a block links to that has no place yet, in file order.
/// What the roots never reach is listed at the top level.
fn tree(stream: &NiStream, keys: &[NiKey], index: &HashMap<NiKey, usize>) -> Vec<Value> {
    let n = keys.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let roots: Vec<usize> = stream.roots.iter().filter_map(|r| index.get(&r.key).copied()).collect();
    let is_root: Vec<bool> = (0..n).map(|i| roots.contains(&i)).collect();
    for (i, k) in keys.iter().enumerate() {
        if let Ok(node) = <&NiNode>::try_from(&stream.objects[*k]) {
            for c in &node.children {
                if let Some(&ci) = index.get(&c.key)
                    && ci != i
                    && !is_root[ci]
                    && parent[ci].is_none()
                {
                    parent[ci] = Some(i);
                }
            }
        }
    }
    for (i, k) in keys.iter().enumerate() {
        let mut links = Vec::new();
        stream.objects[*k].visitor(&mut |key| links.push(key));
        links.reverse(); // the visitor walks fields last to first
        for key in links {
            if let Some(&ci) = index.get(&key)
                && ci != i
                && !is_root[ci]
                && parent[ci].is_none()
            {
                parent[ci] = Some(i);
            }
        }
    }
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (c, p) in parent.iter().enumerate() {
        if let Some(p) = p {
            kids[*p].push(c);
        }
    }
    let mut seen = vec![false; n];
    fn build(i: usize, depth: usize, stream: &NiStream, keys: &[NiKey], kids: &[Vec<usize>], seen: &mut [bool]) -> Option<Value> {
        if seen[i] || depth > 256 {
            return None;
        }
        seen[i] = true;
        let o = &stream.objects[keys[i]];
        let children: Vec<Value> =
            kids[i].iter().filter_map(|&c| build(c, depth + 1, stream, keys, kids, seen)).collect();
        Some(json!({"index": i, "type": type_name(o), "name": name_of(o), "note": note_of(stream, o),
                    "children": children}))
    }
    let mut out: Vec<Value> = roots.iter().filter_map(|&r| build(r, 0, stream, keys, &kids, &mut seen)).collect();
    for i in 0..n {
        if !seen[i]
            && parent[i].is_none()
            && let Some(v) = build(i, 0, stream, keys, &kids, &mut seen)
        {
            out.push(v);
        }
    }
    for i in 0..n {
        // A cycle of links nothing else reaches: still listed.
        if let Some(v) = build(i, 0, stream, keys, &kids, &mut seen) {
            out.push(v);
        }
    }
    out
}

/// A parsed `Debug` value.
#[derive(Debug)]
enum D {
    Leaf(String, &'static str),
    Struct(String, Vec<(String, D)>),
    Seq(String, Vec<D>),
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&self) -> u8 {
        *self.s.get(self.i).unwrap_or(&0)
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.peek() == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> String {
        let q = self.s[self.i];
        self.i += 1;
        let mut out = Vec::new();
        while self.i < self.s.len() && self.s[self.i] != q {
            if self.s[self.i] == b'\\' && self.i + 1 < self.s.len() {
                self.i += 1;
                match self.s[self.i] {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'0' => out.push(0),
                    b'x' if self.i + 2 < self.s.len() => {
                        let h = std::str::from_utf8(&self.s[self.i + 1..self.i + 3]).unwrap_or("3f");
                        out.push(u8::from_str_radix(h, 16).unwrap_or(b'?'));
                        self.i += 2;
                    }
                    b'u' => {
                        // \u{....}
                        let start = self.i + 2;
                        let end = self.s[start..].iter().position(|&b| b == b'}').map(|p| start + p).unwrap_or(start);
                        let cp = u32::from_str_radix(std::str::from_utf8(&self.s[start..end]).unwrap_or("3f"), 16).unwrap_or(0x3f);
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(char::from_u32(cp).unwrap_or('?').encode_utf8(&mut buf).as_bytes());
                        self.i = end;
                    }
                    c => out.push(c),
                }
            } else {
                out.push(self.s[self.i]);
            }
            self.i += 1;
        }
        self.i += 1;
        String::from_utf8_lossy(&out).into_owned()
    }
    /// A bare token: an identifier, a number, or a path with generics (`PhantomData<...>`).
    fn token(&mut self) -> String {
        let start = self.i;
        let mut depth = 0i32;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == b'<' {
                depth += 1;
            } else if c == b'>' && depth > 0 && self.s[self.i.saturating_sub(1)] != b'-' {
                depth -= 1;
            } else if depth == 0 && matches!(c, b',' | b'}' | b')' | b']' | b'{' | b'(' | b'[' | b':') {
                if c == b':' && self.s.get(self.i + 1) == Some(&b':') {
                    self.i += 2;
                    continue;
                }
                break;
            } else if depth == 0 && (c as char).is_whitespace() {
                break;
            }
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.i]).into_owned()
    }
    fn value(&mut self) -> D {
        self.ws();
        match self.peek() {
            b'"' | b'\'' => D::Leaf(self.string(), "string"),
            b'[' => {
                self.i += 1;
                D::Seq(String::new(), self.items(b']'))
            }
            b'(' => {
                self.i += 1;
                D::Seq(String::new(), self.items(b')'))
            }
            _ => {
                let name = self.token();
                self.ws();
                match self.peek() {
                    b'{' => {
                        self.i += 1;
                        let mut fields = Vec::new();
                        loop {
                            self.ws();
                            if self.eat(b'}') || self.i >= self.s.len() {
                                break;
                            }
                            if self.peek() == b'.' {
                                // `..` (a non-exhaustive struct)
                                self.i += 2;
                                continue;
                            }
                            let f = self.token();
                            self.eat(b':');
                            let v = self.value();
                            fields.push((f, v));
                            self.eat(b',');
                        }
                        D::Struct(name, fields)
                    }
                    b'(' => {
                        self.i += 1;
                        D::Seq(name, self.items(b')'))
                    }
                    _ => {
                        let kind = if name.parse::<f64>().is_ok() || matches!(name.as_str(), "NaN" | "inf" | "-inf") {
                            if name.contains(['.', 'e', 'N', 'i']) { "float" } else { "int" }
                        } else if name == "true" || name == "false" {
                            "bool"
                        } else {
                            "enum"
                        };
                        D::Leaf(name, kind)
                    }
                }
            }
        }
    }
    fn items(&mut self, close: u8) -> Vec<D> {
        let mut out = Vec::new();
        loop {
            self.ws();
            if self.eat(close) || self.i >= self.s.len() {
                break;
            }
            out.push(self.value());
            self.eat(b',');
        }
        out
    }
}

/// Flattened rows `(name, kind, value)`: `base.` levels dropped (inherited fields read as
/// the block's own), links as the block index they point at, short numeric tuples as one
/// row, longer arrays as a count.
fn flatten(d: &D, path: &str, links: &HashMap<String, usize>, out: &mut Vec<(String, &'static str, Value)>) {
    let join = |p: &str, f: &str| if p.is_empty() { f.to_string() } else { format!("{p}.{f}") };
    match d {
        D::Leaf(v, kind) => {
            let val = match *kind {
                "int" => v.parse::<i64>().map(Value::from).unwrap_or(Value::from(v.clone())),
                "float" => v.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| json!((f * 1e6).round() / 1e6)).unwrap_or(Value::from(v.clone())),
                "bool" => Value::from(v == "true"),
                _ => Value::from(v.clone()),
            };
            out.push((path.to_string(), kind, val));
        }
        D::Struct(name, fields) if name == "NiLink" => {
            let key = fields.iter().find(|(f, _)| f == "key").map(|(_, v)| match v {
                D::Seq(n, inner) if n == "NiKey" => inner.first().map(|x| match x { D::Leaf(s, _) => s.clone(), _ => String::new() }).unwrap_or_default(),
                _ => String::new(),
            });
            let ix = key.and_then(|k| links.get(&k).copied()).map(|i| i as i64).unwrap_or(-1);
            out.push((path.to_string(), "link", Value::from(ix)));
        }
        D::Struct(_, fields) => {
            for (f, v) in fields {
                if f == "phantom" {
                    continue;
                }
                let p = if f == "base" { path.to_string() } else { join(path, f) };
                flatten(v, &p, links, out);
            }
        }
        D::Seq(name, items) => {
            let leaves: Vec<&D> = items.iter().filter(|x| matches!(x, D::Leaf(..))).collect();
            if name == "Some" && items.len() == 1 {
                flatten(&items[0], path, links, out);
            } else if !name.is_empty() && items.len() == 1 && name != "NiLink" {
                // A one-field tuple variant: External("tex.dds") reads as its value.
                flatten(&items[0], path, links, out);
            } else if leaves.len() == items.len() && (1..=4).contains(&items.len()) {
                let text: Vec<String> = items.iter().map(|x| match x { D::Leaf(s, _) => s.clone(), _ => String::new() }).collect();
                out.push((path.to_string(), "tuple", Value::from(text.join(", "))));
            } else if items.is_empty() {
                out.push((path.to_string(), "list", Value::from("[]")));
            } else if items.len() > 8 {
                out.push((path.to_string(), "list", Value::from(format!("<{} items>", items.len()))));
            } else {
                for (i, x) in items.iter().enumerate() {
                    flatten(x, &format!("{path}[{i}]"), links, out);
                }
            }
        }
    }
}

/// The fields the editor can set on a block, by flattened name, and the input kind.
fn editable(o: &NiType, field: &str) -> Option<&'static str> {
    let is = |ok: bool, k: &'static str| if ok { Some(k) } else { None };
    match field {
        "name" => is(<&NiObjectNET>::try_from(o).is_ok(), "string"),
        "flags" => is(
            <&NiAVObject>::try_from(o).is_ok() || <&NiProperty>::try_from(o).is_ok() || <&NiTimeController>::try_from(o).is_ok(),
            "u16",
        ),
        "translation" | "velocity" => is(<&NiAVObject>::try_from(o).is_ok(), "vec3"),
        "scale" => is(<&NiAVObject>::try_from(o).is_ok(), "f32"),
        "ambient_color" | "diffuse_color" | "specular_color" | "emissive_color" => {
            is(<&NiMaterialProperty>::try_from(o).is_ok(), "vec3")
        }
        "shine" | "alpha" => is(<&NiMaterialProperty>::try_from(o).is_ok(), "f32"),
        "test_ref" => is(<&NiAlphaProperty>::try_from(o).is_ok(), "u8"),
        "source" => is(external_texture(o).is_some(), "string"),
        "value" => is(<&NiStringExtraData>::try_from(o).is_ok(), "string"),
        "frequency" | "phase" | "start_time" | "stop_time" => is(<&NiTimeController>::try_from(o).is_ok(), "f32"),
        _ => None,
    }
}

/// The block panel's data: `{"tree": [...], "blocks": {"<i>": {"type", "fields"}},
/// "complete": true}`.
pub fn blocks(bytes: &[u8]) -> io::Result<Value> {
    let stream = load(bytes)?;
    let (keys, index) = order(&stream);
    let links: HashMap<String, usize> = keys.iter().enumerate().map(|(i, k)| (format!("{k:?}").trim_start_matches("NiKey(").trim_end_matches(')').to_string(), i)).collect();
    let mut blocks = serde_json::Map::new();
    for (i, k) in keys.iter().enumerate() {
        let o = &stream.objects[*k];
        let text = format!("{o:?}");
        let mut p = P { s: text.as_bytes(), i: 0 };
        let parsed = p.value();
        // The enum wrapper (NiType::X(inner)) is a one-item tuple: its inner struct.
        let inner = match &parsed {
            D::Seq(_, items) if items.len() == 1 => &items[0],
            other => other,
        };
        let mut rows = Vec::new();
        flatten(inner, "", &links, &mut rows);
        let fields: Vec<Value> = rows
            .into_iter()
            .filter(|(name, _, _)| !name.is_empty())
            .map(|(name, kind, value)| {
                let ed = editable(o, &name);
                json!({"name": name, "kind": ed.unwrap_or(kind), "value": value, "editable": ed.is_some()})
            })
            .collect();
        blocks.insert(i.to_string(), json!({"type": type_name(o), "fields": fields}));
    }
    Ok(json!({"tree": tree(&stream, &keys, &index), "blocks": blocks, "complete": true}))
}

// ---- edit --------------------------------------------------------------------------

fn as_f32(v: &Value) -> io::Result<f32> {
    v.as_f64().map(|f| f as f32).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).ok_or_else(|| bad(format!("not a number: {v}")))
}
fn as_int<T: TryFrom<i64>>(v: &Value) -> io::Result<T> {
    let n = v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).ok_or_else(|| bad(format!("not a whole number: {v}")))?;
    T::try_from(n).map_err(|_| bad(format!("{n} is out of range")))
}
fn as_vec3(v: &Value) -> io::Result<glam::Vec3> {
    let s = v.as_str().ok_or_else(|| bad("expected \"x, y, z\""))?;
    let parts: Vec<f32> = s.split([',', ' ']).filter(|p| !p.trim().is_empty()).map(|p| p.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|_| bad(format!("not three numbers: {s}")))?;
    if parts.len() != 3 {
        return Err(bad(format!("not three numbers: {s}")));
    }
    Ok(glam::Vec3::new(parts[0], parts[1], parts[2]))
}
fn as_string(v: &Value) -> io::Result<String> {
    v.as_str().map(str::to_string).ok_or_else(|| bad(format!("not text: {v}")))
}

fn set(o: &mut NiType, field: &str, v: &Value) -> io::Result<()> {
    if editable(o, field).is_none() {
        return Err(bad(format!("{} has no editable field {field:?}", type_name(o))));
    }
    let tn = type_name(o);
    let nope = || bad(format!("{tn} cannot take {field}"));
    match field {
        "name" => <&mut NiObjectNET>::try_from(o).map_err(|()| nope())?.name = as_string(v)?,
        "flags" => {
            let f: u16 = as_int(v)?;
            if let Ok(x) = <&mut NiAVObject>::try_from(&mut *o) {
                x.flags = f;
            } else if let Ok(x) = <&mut NiProperty>::try_from(&mut *o) {
                x.flags = f;
            } else {
                <&mut NiTimeController>::try_from(o).map_err(|()| nope())?.flags = f;
            }
        }
        "translation" => <&mut NiAVObject>::try_from(o).map_err(|()| nope())?.translation = as_vec3(v)?,
        "velocity" => <&mut NiAVObject>::try_from(o).map_err(|()| nope())?.velocity = as_vec3(v)?,
        "scale" => <&mut NiAVObject>::try_from(o).map_err(|()| nope())?.scale = as_f32(v)?,
        "ambient_color" | "diffuse_color" | "specular_color" | "emissive_color" | "shine" | "alpha" => {
            let m = <&mut NiMaterialProperty>::try_from(o).map_err(|()| nope())?;
            match field {
                "ambient_color" => m.ambient_color = as_vec3(v)?,
                "diffuse_color" => m.diffuse_color = as_vec3(v)?,
                "specular_color" => m.specular_color = as_vec3(v)?,
                "emissive_color" => m.emissive_color = as_vec3(v)?,
                "shine" => m.shine = as_f32(v)?,
                _ => m.alpha = as_f32(v)?,
            }
        }
        "test_ref" => <&mut NiAlphaProperty>::try_from(o).map_err(|()| nope())?.test_ref = as_int(v)?,
        "source" => <&mut NiSourceTexture>::try_from(o).map_err(|()| nope())?.source = TextureSource::External(as_string(v)?),
        "value" => <&mut NiStringExtraData>::try_from(o).map_err(|()| nope())?.value = as_string(v)?,
        _ => {
            let c = <&mut NiTimeController>::try_from(o).map_err(|()| nope())?;
            let f = as_f32(v)?;
            match field {
                "frequency" => c.frequency = f,
                "phase" => c.phase = f,
                "start_time" => c.start_time = f,
                _ => c.stop_time = f,
            }
        }
    }
    Ok(())
}

/// `edits` (`[{"op": "set_field", "block", "name", "value"}]`, block by file index)
/// applied, and the file saved.
pub fn edit(bytes: &[u8], edits: &[Value]) -> io::Result<Vec<u8>> {
    let mut stream = load(bytes)?;
    let (keys, _) = order(&stream);
    for (n, e) in edits.iter().enumerate() {
        let op = e.get("op").and_then(Value::as_str).unwrap_or("set_field");
        if op != "set_field" {
            return Err(bad(format!("edit {n}: unknown op {op:?}")));
        }
        let i = e.get("block").and_then(Value::as_u64).ok_or_else(|| bad(format!("edit {n}: no block")))? as usize;
        let key = *keys.get(i).ok_or_else(|| bad(format!("edit {n}: no block {i}")))?;
        let name = e.get("name").and_then(Value::as_str).ok_or_else(|| bad(format!("edit {n}: no field name")))?;
        let o = stream.objects.get_mut(key).ok_or_else(|| bad(format!("edit {n}: no block {i}")))?;
        set(o, name, e.get("value").unwrap_or(&Value::Null)).map_err(|err| bad(format!("block {i}: {err}")))?;
    }
    crate::guarded(|| stream.save_bytes())
}

// ---- Python ------------------------------------------------------------------------

#[pyfunction]
fn nif_summary(py: Python<'_>, data: &[u8]) -> String {
    py.detach(|| summary(data).to_string())
}

#[pyfunction]
fn nif_blocks(py: Python<'_>, data: &[u8]) -> PyResult<String> {
    py.detach(|| blocks(data).map(|v| v.to_string())).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn nif_edit<'py>(py: Python<'py>, data: &[u8], edits_json: &str) -> PyResult<Bound<'py, PyBytes>> {
    let edits: Vec<Value> = serde_json::from_str(edits_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let out = py.detach(|| edit(data, &edits)).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, &out))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(nif_summary, m)?)?;
    m.add_function(wrap_pyfunction!(nif_blocks, m)?)?;
    m.add_function(wrap_pyfunction!(nif_edit, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANIM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/gardenfell_anim/anim.nif");

    #[test]
    fn summarises_a_mesh() {
        let s = summary(&std::fs::read(ANIM).unwrap());
        assert_eq!(s["stopped_reason"], "");
        assert!(s["has_animation"].as_bool().unwrap());
        assert_eq!(s["textures"][0], "tr.dds");
        assert_eq!(s["blocks_read"], s["blocks_declared"]);
        let bad = summary(b"not a nif at all, not even close to one.....");
        assert_ne!(bad["stopped_reason"], "");
    }

    #[test]
    fn a_4000_header_reads_as_4002() {
        let mut b = std::fs::read(ANIM).unwrap();
        b[..40].copy_from_slice(b"NetImmerse File Format, Version 4.0.0.0\n");
        b[40..44].copy_from_slice(&0x0400_0000u32.to_le_bytes());
        assert_eq!(summary(&b)["stopped_reason"], "");
    }

    #[test]
    fn blocks_tree_holds_every_block_once_and_edits_apply() {
        let data = std::fs::read(ANIM).unwrap();
        let v = blocks(&data).unwrap();
        let n = v["blocks"].as_object().unwrap().len();
        fn walk(nodes: &Value, out: &mut Vec<u64>) {
            for x in nodes.as_array().unwrap() {
                out.push(x["index"].as_u64().unwrap());
                walk(&x["children"], out);
            }
        }
        let mut seen = Vec::new();
        walk(&v["tree"], &mut seen);
        seen.sort();
        assert_eq!(seen, (0..n as u64).collect::<Vec<_>>());
        let out = edit(&data, &[json!({"op": "set_field", "block": 0, "name": "scale", "value": 2.5})]).unwrap();
        let after = blocks(&out).unwrap();
        let scale = after["blocks"]["0"]["fields"].as_array().unwrap().iter().find(|f| f["name"] == "scale").unwrap();
        assert_eq!(scale["value"], 2.5);
        assert!(edit(&data, &[json!({"block": 0, "name": "rotation.x_axis", "value": "1,0,0"})]).is_err());
    }
}
