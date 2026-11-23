//! The files a Lua mod reads, and the settings the player saved, for the Preview's Lua
//! (Static 2026-11-23: "do settings files like tomls reach the lua scripts?" - "all three").
//!
//! - Text files of the data folders, on demand (`openmw.vfs` `open`/`lines`/`pathsWithPrefix`,
//!   MWSE's `io.open` for reading, `json.loadfile`, `mwse.loadConfig`): the worker asks the
//!   program mid-call (`luaproc`, the same channel as `luarec`'s records), and [`answer`] reads
//!   it from the VFS - text only, at most [`MAX_TEXT`] bytes.
//! - YAML (`openmw.markup` `loadYaml`/`decodeYaml`: S3maphore's playlists, Variation
//!   Framework, Bullseye) through serde-saphyr, and TOML (MWSE's `toml.loadFile`/`decode`/
//!   `loadMetadata`) through Gardenfell's reader, given to the Lua as JSON.
//! - The settings the player saved in OpenMW (`player_storage.bin`, `global_storage.bin`,
//!   beside openmw.cfg): read once at the start ([`storage_json`]) and put in the storage
//!   sections before any script runs, so OpenMW's settings scripts find them and do not put
//!   the defaults in their place. The file's layout - a version byte (0; 1 in the OpenMW Static
//!   plays on, Static 2026-11-23: his player_storage.bin, 275 sections, read whole with the same
//!   layout), then one value:
//!   0x00 a number (f64), 0x01 a long string (u32 length), 0x02 a boolean (a byte),
//!   0x03 ... 0x04 a table (key, value pairs to its end), 0x10/0x11/0x14 a vector of 2/3/4
//!   f64s, 0x12 a transform (16 f64s), 0x13 a rotation (4 f64s), 0x15 a colour (4 f32s);
//!   0b001SSSSS a string of up to 31 bytes; 0b01TTTTTT a custom value (its type name's length
//!   less one, then a u32 data length, the name, the data); 0b1SSSSTTT the same, short. Read
//!   from the layout as OpenMW writes it (all little-endian); none of OpenMW's code.

use serde_json::{json, Map, Value};

use crate::vfs::Vfs;

/// The longest file a script may read here.
pub const MAX_TEXT: usize = 8 << 20;
/// The most paths `pathsWithPrefix` lists.
pub const MAX_LIST: usize = 5000;

/// A path as the VFS keys it: forward slashes, lower case, no leading `Data Files/`.
pub fn vfs_path(p: &str) -> String {
    let mut p = p.replace('\\', "/").to_ascii_lowercase();
    while let Some(r) = p.strip_prefix('/') {
        p = r.to_string();
    }
    if let Some(i) = p.find("data files/") {
        p = p[i + "data files/".len()..].to_string();
    }
    p
}

/// A text file of the data folders: `None` when there is none, it is too long, or it is not text.
pub fn text(vfs: &Vfs, path: &str) -> Option<String> {
    let loc = vfs.resolve(&vfs_path(path))?;
    let bytes = vfs.load(&loc)?;
    if bytes.len() > MAX_TEXT || bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// YAML as JSON.
pub fn yaml_json(text: &str) -> Option<Value> {
    serde_saphyr::from_str::<Value>(text).ok()
}

/// TOML as JSON.
pub fn toml_json(text: &str) -> Option<Value> {
    fn conv(v: &crate::toml::Value) -> Value {
        use crate::toml::Value as T;
        match v {
            T::Str(s) => json!(s),
            T::Int(i) => json!(i),
            T::Float(f) => json!(f),
            T::Bool(b) => json!(b),
            T::Array(a) => Value::Array(a.iter().map(conv).collect()),
            T::Table(t) => Value::Object(t.iter().map(|(k, v)| (k.clone(), conv(v))).collect()),
        }
    }
    let t = crate::toml::parse(text).ok()?;
    Some(Value::Object(t.iter().map(|(k, v)| (k.clone(), conv(v))).collect()))
}

/// The Lua's question about a file (`kind` and its argument), answered as JSON: `@file` the
/// text, `@list` the paths under a prefix, `@yaml`/`@toml` a file read as data,
/// `@yamltext`/`@tomltext` text given as data. `None`: no such file, or not that kind.
pub fn answer(vfs: &Vfs, kind: &str, arg: &str) -> Option<String> {
    let v = match kind {
        "@file" => json!(text(vfs, arg)?),
        "@list" => {
            let p = vfs_path(arg);
            let mut all: Vec<String> = vfs.list(&p, &[]).into_iter().map(|f| f.replace('\\', "/").to_ascii_lowercase()).collect();
            all.sort();
            all.dedup();
            all.truncate(MAX_LIST);
            json!(all)
        }
        "@yaml" => yaml_json(&text(vfs, arg)?)?,
        "@yamltext" => yaml_json(arg)?,
        "@toml" => toml_json(&text(vfs, arg)?)?,
        "@tomltext" => toml_json(arg)?,
        _ => return None,
    };
    Some(v.to_string())
}

/// OpenMW's storage file (see the module's note) as JSON: each section a table of its values.
/// A vector or colour is `{"$v2": [x, y]}`, `{"$v3": ...}`, `{"$v4": ...}`, `{"$color": [r, g,
/// b, a]}`; a transform or a custom value its bytes as they were, `{"$raw": hex}`, so the file
/// written back ([`storage_write`]) keeps it. `None` for a file that is not one.
pub fn storage_json(bytes: &[u8]) -> Option<Value> {
    struct R<'a> {
        b: &'a [u8],
        at: usize,
    }
    impl R<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> {
            let s = self.b.get(self.at..self.at + n)?;
            self.at += n;
            Some(s)
        }
        fn f64(&mut self) -> Option<f64> {
            Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
        }
        fn f32(&mut self) -> Option<f64> {
            Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?) as f64)
        }
        fn u32(&mut self) -> Option<usize> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
        }
        fn str(&mut self, n: usize) -> Option<String> {
            Some(String::from_utf8_lossy(self.take(n)?).into_owned())
        }
        /// The bytes from `from` to here, as `{"$raw": hex}`.
        fn raw(&self, from: usize) -> Value {
            json!({"$raw": self.b[from..self.at].iter().map(|b| format!("{b:02x}")).collect::<String>()})
        }
        /// One value; `Err(())` at a table's end.
        fn value(&mut self, depth: usize) -> Option<Result<Value, ()>> {
            if depth > 64 {
                return None;
            }
            let from = self.at;
            let t = *self.take(1)?.first()?;
            if t & 0x80 != 0 {
                let (name, len) = (((t & 7) + 1) as usize, ((t >> 3) & 15) as usize);
                self.take(name + len)?;
                return Some(Ok(self.raw(from)));
            }
            if t & 0x40 != 0 {
                let name = ((t & 63) + 1) as usize;
                let len = self.u32()?;
                self.take(name + len)?;
                return Some(Ok(self.raw(from)));
            }
            if t & 0x20 != 0 {
                return Some(Ok(json!(self.str((t & 0x1f) as usize)?)));
            }
            let v = match t {
                0x00 => {
                    let f = self.f64()?;
                    if f.is_finite() {
                        json!(f)
                    } else {
                        // JSON has no infinity (a mod's "no maximum" is math.huge): the number's
                        // bytes kept, and what it is for the scripts (S3maphore, Static 2026-11-23).
                        let mut r = self.raw(from);
                        r["$num"] = json!(if f.is_nan() { "nan" } else if f > 0.0 { "inf" } else { "-inf" });
                        r
                    }
                }
                0x01 => {
                    let n = self.u32()?;
                    json!(self.str(n)?)
                }
                0x02 => json!(*self.take(1)?.first()? != 0),
                0x03 => {
                    let mut pairs: Vec<(Value, Value)> = Vec::new();
                    while let Ok(k) = self.value(depth + 1)? {
                        let v = self.value(depth + 1)?.ok()?;
                        pairs.push((k, v));
                    }
                    table(pairs)
                }
                0x04 => return Some(Err(())),
                0x10 => json!({"$v2": [self.f64()?, self.f64()?]}),
                0x11 => json!({"$v3": [self.f64()?, self.f64()?, self.f64()?]}),
                0x14 => json!({"$v4": [self.f64()?, self.f64()?, self.f64()?, self.f64()?]}),
                0x12 => {
                    self.take(16 * 8)?;
                    self.raw(from)
                }
                0x13 => {
                    self.take(4 * 8)?;
                    self.raw(from)
                }
                0x15 => json!({"$color": [self.f32()?, self.f32()?, self.f32()?, self.f32()?]}),
                _ => return None,
            };
            Some(Ok(v))
        }
    }
    /// A table's pairs as JSON: a list when its keys are 1..n, an object when they are all text,
    /// else its pairs as they are, `{"$table": [[key, value]...]}` - a number or a true/false key
    /// stays one (Static's file, 2026-11-23: about fifty tables keyed by numbers out of order,
    /// which as an object's text keys would have been written back as text: `t["3"]`, not `t[3]`).
    fn table(mut pairs: Vec<(Value, Value)>) -> Value {
        let n = pairs.len();
        let list = !pairs.is_empty() && {
            let mut ks: Vec<f64> = pairs.iter().filter_map(|(k, _)| k.as_f64()).collect();
            ks.sort_by(|a, b| a.total_cmp(b));
            ks.len() == n && ks.iter().enumerate().all(|(i, &k)| k == (i + 1) as f64)
        };
        if list {
            pairs.sort_by(|a, b| a.0.as_f64().unwrap_or(0.0).total_cmp(&b.0.as_f64().unwrap_or(0.0)));
            return Value::Array(pairs.into_iter().map(|(_, v)| v).collect());
        }
        if !pairs.iter().all(|(k, _)| k.is_string()) {
            return json!({"$table": pairs.into_iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()});
        }
        let mut m = Map::new();
        for (k, v) in pairs {
            if let Value::String(s) = k {
                m.insert(s, v);
            }
        }
        Value::Object(m)
    }
    if !matches!(bytes.first(), Some(&(0 | 1))) {
        return None;
    }
    let mut r = R { b: bytes, at: 1 };
    let v = r.value(0)?.ok()?;
    v.is_object().then_some(v)
}

/// JSON back into OpenMW's storage file (the layout in the module's note): what
/// [`storage_json`] read, after an edit. A list is a table keyed 1..n; `$v2`/`$v3`/`$v4`/
/// `$color` the values they stand for; `$raw` the bytes it kept; null leaves the key out (Lua
/// has no nil in a table). `None` for something that is not a table of sections.
pub fn storage_write(v: &Value) -> Option<Vec<u8>> {
    storage_write_as(v, 0)
}

/// [`storage_write`] with the file's version byte (`storage_set` keeps the one it read).
pub fn storage_write_as(v: &Value, version: u8) -> Option<Vec<u8>> {
    fn val(out: &mut Vec<u8>, v: &Value, depth: usize) -> Option<()> {
        if depth > 64 {
            return None;
        }
        match v {
            Value::Bool(b) => out.extend([0x02, *b as u8]),
            Value::Number(n) => {
                out.push(0x00);
                out.extend(n.as_f64()?.to_le_bytes());
            }
            Value::String(s) => key(out, s),
            Value::Array(a) => {
                out.push(0x03);
                for (i, x) in a.iter().enumerate() {
                    if x.is_null() {
                        continue;
                    }
                    out.push(0x00);
                    out.extend(((i + 1) as f64).to_le_bytes());
                    val(out, x, depth + 1)?;
                }
                out.push(0x04);
            }
            Value::Object(m) => {
                let nums = |k: &str, t: u8, n: usize, f32s: bool, out: &mut Vec<u8>| -> Option<()> {
                    let a = m.get(k)?.as_array()?;
                    if a.len() != n {
                        return None;
                    }
                    out.push(t);
                    for x in a {
                        let f = x.as_f64()?;
                        if f32s {
                            out.extend((f as f32).to_le_bytes());
                        } else {
                            out.extend(f.to_le_bytes());
                        }
                    }
                    Some(())
                };
                // A table with keys other than text, pair by pair (`storage_json`'s `table`).
                if let Some(ps) = m.get("$table").and_then(|p| p.as_array()).filter(|_| m.len() == 1) {
                    out.push(0x03);
                    for p in ps {
                        let (k, x) = (p.get(0)?, p.get(1)?);
                        if k.is_null() || x.is_null() {
                            continue;
                        }
                        val(out, k, depth + 1)?;
                        val(out, x, depth + 1)?;
                    }
                    out.push(0x04);
                    return Some(());
                }
                // Bytes kept as they were read (a transform, a game object, an infinity).
                if let Some(h) = m.get("$raw").and_then(|h| h.as_str()) {
                    if h.len() % 2 != 0 {
                        return None;
                    }
                    for i in (0..h.len()).step_by(2) {
                        out.push(u8::from_str_radix(h.get(i..i + 2)?, 16).ok()?);
                    }
                    return Some(());
                }
                if m.len() == 1 {
                    if m.contains_key("$v2") {
                        return nums("$v2", 0x10, 2, false, out);
                    }
                    if m.contains_key("$v3") {
                        return nums("$v3", 0x11, 3, false, out);
                    }
                    if m.contains_key("$v4") {
                        return nums("$v4", 0x14, 4, false, out);
                    }
                    if m.contains_key("$color") {
                        return nums("$color", 0x15, 4, true, out);
                    }
                }
                out.push(0x03);
                for (k, x) in m {
                    if x.is_null() {
                        continue;
                    }
                    key(out, k);
                    val(out, x, depth + 1)?;
                }
                out.push(0x04);
            }
            Value::Null => return None,
        }
        Some(())
    }
    fn key(out: &mut Vec<u8>, s: &str) {
        let b = s.as_bytes();
        if b.len() < 32 {
            out.push(0x20 | b.len() as u8);
        } else {
            out.push(0x01);
            out.extend((b.len() as u32).to_le_bytes());
        }
        out.extend(b);
    }
    if !v.is_object() {
        return None;
    }
    let mut out = vec![version];
    val(&mut out, v, 0)?;
    Some(out)
}

/// One setting of a storage file changed (`value` null: taken out), the file written back only
/// when it reads again as exactly that (the round trip checked) - `Err` says why not.
pub fn storage_set(bytes: &[u8], section: &str, key: &str, value: Value) -> Result<Vec<u8>, String> {
    let mut v = storage_json(bytes).ok_or("the file is not one OpenMW's Lua storage reads")?;
    let sec = v.as_object_mut().ok_or("no sections")?.entry(section.to_string()).or_insert_with(|| json!({}));
    let m = sec.as_object_mut().ok_or("the section is not a table")?;
    if value.is_null() {
        m.remove(key);
    } else {
        m.insert(key.to_string(), value);
    }
    let out = storage_write_as(&v, bytes[0]).ok_or("the value cannot be stored (a value OpenMW's storage does not hold)")?;
    // The same settings read back, or nothing is written.
    let again = storage_json(&out).ok_or("the file written would not read back")?;
    let clean = |x: &Value| -> Value {
        // Nulls are left out when written: compare without them.
        fn strip(x: &Value) -> Value {
            match x {
                Value::Object(m) => Value::Object(m.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), strip(v))).collect()),
                Value::Array(a) => Value::Array(a.iter().map(strip).collect()),
                Value::Number(n) => json!(n.as_f64()),
                o => o.clone(),
            }
        }
        strip(x)
    };
    if clean(&again) != clean(&v) {
        return Err("the file written would not read back as the same settings: nothing written".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A storage file as OpenMW writes one: a section with a string, a number, a boolean, a
    /// list, a vector and a colour, read back as its values.
    #[test]
    fn the_player_s_saved_settings_are_read() {
        let mut b = vec![0u8, 0x03];
        let s = |b: &mut Vec<u8>, t: &str| {
            b.push(0x20 | t.len() as u8);
            b.extend(t.as_bytes());
        };
        let num = |b: &mut Vec<u8>, x: f64| {
            b.push(0x00);
            b.extend(x.to_le_bytes());
        };
        s(&mut b, "Horizontal_Compass_Settings");
        b.push(0x03);
        s(&mut b, "stylePath");
        s(&mut b, "2");
        s(&mut b, "compassScale");
        num(&mut b, 120.0);
        s(&mut b, "showCompass");
        b.extend([0x02, 1]);
        s(&mut b, "order");
        b.push(0x03);
        num(&mut b, 2.0);
        s(&mut b, "b");
        num(&mut b, 1.0);
        s(&mut b, "a");
        b.push(0x04);
        s(&mut b, "pos");
        b.push(0x10);
        b.extend(0.5f64.to_le_bytes());
        b.extend(0.05f64.to_le_bytes());
        s(&mut b, "tint");
        b.push(0x15);
        for c in [1.0f32, 0.8, 0.4, 1.0] {
            b.extend(c.to_le_bytes());
        }
        // A long string, and a custom value passed over.
        s(&mut b, "note");
        b.push(0x01);
        let long = "x".repeat(40);
        b.extend((long.len() as u32).to_le_bytes());
        b.extend(long.as_bytes());
        s(&mut b, "obj");
        b.extend([0x80 | (2 << 3) | 2, b'a', b'b', b'c', 9, 9]);
        b.push(0x04);
        b.push(0x04);
        let v = storage_json(&b).unwrap();
        let c = &v["Horizontal_Compass_Settings"];
        assert_eq!(c["stylePath"], json!("2"));
        assert_eq!(c["compassScale"], json!(120.0));
        assert_eq!(c["showCompass"], json!(true));
        assert_eq!(c["order"], json!(["a", "b"]), "a 1..n table is a list, in order");
        assert_eq!(c["pos"], json!({"$v2": [0.5, 0.05]}));
        assert!((c["tint"]["$color"][1].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert_eq!(c["note"].as_str().map(str::len), Some(40));
        assert_eq!(c["obj"], json!({"$raw": format!("{:02x}616263{}", 0x80 | (2 << 3) | 2, "0909")}), "a custom value kept as its bytes");
        // Written back as it was read: the same settings, the custom value's bytes kept.
        let back = storage_write(&v).unwrap();
        assert_eq!(storage_json(&back).unwrap(), v);
        // One setting changed: only that one.
        let changed = storage_set(&b, "Horizontal_Compass_Settings", "stylePath", json!("3")).unwrap();
        let c2 = storage_json(&changed).unwrap();
        assert_eq!(c2["Horizontal_Compass_Settings"]["stylePath"], json!("3"));
        assert_eq!(c2["Horizontal_Compass_Settings"]["obj"], c["obj"]);
        assert_eq!(c2["Horizontal_Compass_Settings"]["order"], json!(["a", "b"]));
        let gone = storage_json(&storage_set(&b, "Horizontal_Compass_Settings", "note", Value::Null).unwrap()).unwrap();
        assert!(gone["Horizontal_Compass_Settings"].get("note").is_none());
        assert!(storage_set(&[9, 9], "a", "b", json!(1)).is_err(), "not a storage file: refused");
        assert!(storage_json(&[2, 0x03, 0x04]).is_none(), "another version");
        // A table keyed by numbers out of order: its keys stay numbers.
        let mut nk = vec![0u8, 0x03, 0x21, b't', 0x03, 0x00];
        nk.extend(3.0f64.to_le_bytes());
        nk.extend([0x21, b'x', 0x00]);
        nk.extend(7.0f64.to_le_bytes());
        nk.extend([0x21, b'y', 0x04, 0x04]);
        let nv = storage_json(&nk).unwrap();
        assert_eq!(nv["t"], json!({"$table": [[3.0, "x"], [7.0, "y"]]}));
        assert_eq!(storage_write(&nv).unwrap(), nk, "number keys written as numbers");
        // An infinity (a mod's "no maximum"): kept, and said for the scripts.
        let mut inf = vec![0u8, 0x03, 0x21, b'm', 0x00];
        inf.extend(f64::INFINITY.to_le_bytes());
        inf.push(0x04);
        let iv = storage_json(&inf).unwrap();
        assert_eq!(iv["m"]["$num"], json!("inf"));
        assert_eq!(storage_write(&iv).unwrap(), inf, "written back byte for byte");
        // Version 1 (newer OpenMW): the same layout; written back as version 1.
        let mut b1 = b.clone();
        b1[0] = 1;
        assert_eq!(storage_json(&b1).unwrap(), v);
        assert_eq!(storage_set(&b1, "Horizontal_Compass_Settings", "stylePath", json!("3")).unwrap()[0], 1);
        assert!(storage_json(&[0, 0x03]).is_none(), "cut short");
    }

    /// A real storage file (`WG_STORAGE`, a copy - it is only read): read, written back, read
    /// again as the same settings; and one setting changed, the rest kept.
    #[test]
    #[ignore]
    fn a_real_storage_file_round_trips() {
        let Ok(p) = std::env::var("WG_STORAGE") else { return };
        let b = std::fs::read(&p).unwrap();
        let v = storage_json(&b).expect("read");
        let sections = v.as_object().unwrap();
        let values: usize = sections.values().map(|s| s.as_object().map_or(0, |m| m.len())).sum();
        let raws = v.to_string().matches("$raw").count();
        let back = storage_write_as(&v, b[0]).expect("write");
        let again = storage_json(&back).expect("read back");
        fn diff(a: &Value, b: &Value, at: String, out: &mut Vec<String>) {
            match (a, b) {
                (Value::Object(x), Value::Object(y)) => {
                    for (k, v) in x {
                        match y.get(k) {
                            Some(w) => diff(v, w, format!("{at}/{k}"), out),
                            None => out.push(format!("{at}/{k}: gone")),
                        }
                    }
                    for k in y.keys() {
                        if !x.contains_key(k) {
                            out.push(format!("{at}/{k}: new"));
                        }
                    }
                }
                (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                    for (i, (v, w)) in x.iter().zip(y).enumerate() {
                        diff(v, w, format!("{at}[{i}]"), out);
                    }
                }
                _ if a != b => out.push(format!("{at}: {} -> {}", a.to_string().chars().take(80).collect::<String>(), b.to_string().chars().take(80).collect::<String>())),
                _ => {}
            }
        }
        let mut d = Vec::new();
        diff(&v, &again, String::new(), &mut d);
        assert!(d.is_empty(), "{} difference(s): {:?}", d.len(), &d[..d.len().min(8)]);
        let same_bytes = back == b;
        // The same values the same way: only the order of a table's keys may differ (OpenMW's is
        // its hash table's), so the file is as long as OpenMW's, to the byte.
        assert_eq!(back.len(), b.len(), "written back as long as OpenMW wrote it");
        if let Ok(out) = std::env::var("WG_STORAGE_OUT") {
            std::fs::write(out, &back).unwrap();
        }
        // Every value set to itself: the file reads as before.
        let (name, sec) = sections.iter().find(|(_, s)| s.as_object().is_some_and(|m| !m.is_empty())).unwrap();
        let (k, x) = sec.as_object().unwrap().iter().next().unwrap();
        let again = storage_json(&storage_set(&b, name, k, x.clone()).expect("set")).unwrap();
        assert_eq!(again, v);
        println!("REAL {} bytes, {} sections, {} values, {} kept as raw bytes, written back {} bytes ({}), set {name}.{k}: unchanged",
            b.len(), sections.len(), values, raws, back.len(), if same_bytes { "byte for byte" } else { "same settings, another key order" });
    }

    #[test]
    fn yaml_and_toml_are_given_as_data() {
        assert_eq!(yaml_json("tracks:\n  - a.mp3\n  - b.mp3\nchance: 0.5\n").unwrap(), json!({"tracks": ["a.mp3", "b.mp3"], "chance": 0.5}));
        let t = toml_json("name = \"Mod\"\n[deps]\nmwse = 1\n").unwrap();
        assert_eq!((t["name"].clone(), t["deps"]["mwse"].clone()), (json!("Mod"), json!(1)));
        assert_eq!(vfs_path("C:\\Games\\Morrowind\\Data Files\\MWSE\\config\\X.json"), "mwse/config/x.json");
    }
}
