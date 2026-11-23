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
//!   the defaults in their place. The file's layout - a version byte (0), then one value:
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
/// b, a]}`; a transform or a custom value null. `None` for a file that is not one.
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
        /// One value; `Err(())` at a table's end.
        fn value(&mut self, depth: usize) -> Option<Result<Value, ()>> {
            if depth > 64 {
                return None;
            }
            let t = *self.take(1)?.first()?;
            if t & 0x80 != 0 {
                let (name, len) = (((t & 7) + 1) as usize, ((t >> 3) & 15) as usize);
                self.take(name + len)?;
                return Some(Ok(Value::Null));
            }
            if t & 0x40 != 0 {
                let name = ((t & 63) + 1) as usize;
                let len = self.u32()?;
                self.take(name + len)?;
                return Some(Ok(Value::Null));
            }
            if t & 0x20 != 0 {
                return Some(Ok(json!(self.str((t & 0x1f) as usize)?)));
            }
            let v = match t {
                0x00 => json!(self.f64()?),
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
                    Value::Null
                }
                0x13 => {
                    self.take(4 * 8)?;
                    Value::Null
                }
                0x15 => json!({"$color": [self.f32()?, self.f32()?, self.f32()?, self.f32()?]}),
                _ => return None,
            };
            Some(Ok(v))
        }
    }
    /// A table's pairs as JSON: a list when its keys are 1..n, else an object (keys as text).
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
        let mut m = Map::new();
        for (k, v) in pairs {
            let key = match k {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            m.insert(key, v);
        }
        Value::Object(m)
    }
    if bytes.first() != Some(&0) {
        return None;
    }
    let mut r = R { b: bytes, at: 1 };
    let v = r.value(0)?.ok()?;
    v.is_object().then_some(v)
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
        assert_eq!(c["obj"], Value::Null);
        assert!(storage_json(&[1, 0x03, 0x04]).is_none(), "another version");
        assert!(storage_json(&[0, 0x03]).is_none(), "cut short");
    }

    #[test]
    fn yaml_and_toml_are_given_as_data() {
        assert_eq!(yaml_json("tracks:\n  - a.mp3\n  - b.mp3\nchance: 0.5\n").unwrap(), json!({"tracks": ["a.mp3", "b.mp3"], "chance": 0.5}));
        let t = toml_json("name = \"Mod\"\n[deps]\nmwse = 1\n").unwrap();
        assert_eq!((t["name"].clone(), t["deps"]["mwse"].clone()), (json!("Mod"), json!(1)));
        assert_eq!(vfs_path("C:\\Games\\Morrowind\\Data Files\\MWSE\\config\\X.json"), "mwse/config/x.json");
    }
}
