//! Where a moved exterior reference stands: the shared move cases
//! (tests/fixtures/move_cases.json at the repository's root), which the Python Editor runs
//! too. Each case's plugins are built as bytes and loaded through the engine's merge
//! (`viewcore::world::World::load_with`), then every exterior cell is searched for the
//! reference. The byte helpers are Robin Hjelte's (MIT), from Gardenfell's
//! `gardenfell-world/tests/round18gf.rs`.

use viewcore::json::{parse, Val};
use viewcore::vfs::Vfs;
use viewcore::world::{LoadOpts, World};
use std::path::PathBuf;

const CASES: &str = include_str!("../../../tests/fixtures/move_cases.json");

// ---- a plugin as bytes (round18gf.rs) -----------------------------------------------

fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(data);
    v
}

fn zstr(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn rec(tag: &[u8; 4], flags: u32, subs: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = subs.concat();
    let mut v = tag.to_vec();
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&body);
    v
}

fn header(masters: &[String], records: u32) -> Vec<u8> {
    let mut hedr = Vec::with_capacity(300);
    hedr.extend_from_slice(&1.3f32.to_le_bytes());
    hedr.extend_from_slice(&0u32.to_le_bytes());
    hedr.extend_from_slice(&[0u8; 32]);
    hedr.extend_from_slice(&[0u8; 256]);
    hedr.extend_from_slice(&records.to_le_bytes());
    let mut subs = vec![sub(b"HEDR", &hedr)];
    for m in masters {
        subs.push(sub(b"MAST", &zstr(m)));
        subs.push(sub(b"DATA", &0u64.to_le_bytes()));
    }
    rec(b"TES3", 0, &subs)
}

fn stat(id: &str, mesh: &str) -> Vec<u8> {
    rec(b"STAT", 0, &[sub(b"NAME", &zstr(id)), sub(b"MODL", &zstr(mesh))])
}

fn exterior(gx: i32, gy: i32, body: &[Vec<u8>]) -> Vec<u8> {
    let mut d = 2u32.to_le_bytes().to_vec();
    d.extend_from_slice(&gx.to_le_bytes());
    d.extend_from_slice(&gy.to_le_bytes());
    let mut subs = vec![sub(b"NAME", &zstr("")), sub(b"DATA", &d)];
    subs.extend_from_slice(body);
    rec(b"CELL", 0, &subs)
}

/// A point inside a grid square.
fn inside(g: (i32, i32)) -> (f32, f32) {
    (g.0 as f32 * 8192.0 + 10.0, g.1 as f32 * 8192.0 + 10.0)
}

/// One reference record: MVRF/CNDT first when it moves, then FRMR/NAME and either DATA
/// (placed where it moves to, or at home) or DELE.
fn reference(word: u32, id: &str, deleted: bool, home: (i32, i32), to: Option<(i32, i32)>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some((gx, gy)) = to {
        let mut g = gx.to_le_bytes().to_vec();
        g.extend_from_slice(&gy.to_le_bytes());
        out.extend([sub(b"MVRF", &word.to_le_bytes()), sub(b"CNDT", &g)].concat());
    }
    out.extend([sub(b"FRMR", &word.to_le_bytes()), sub(b"NAME", &zstr(id))].concat());
    if deleted {
        out.extend(sub(b"DELE", &[0u8; 4]));
    } else {
        let (x, y) = inside(to.unwrap_or(home));
        let mut data = Vec::new();
        for v in [x, y, 0.0, 0.0, 0.0, 0.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        out.extend(sub(b"DATA", &data));
    }
    out
}

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("wg-moves-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn grid(v: &Val) -> Option<(i32, i32)> {
    let a = v.as_arr()?;
    Some((a[0].as_i64()? as i32, a[1].as_i64()? as i32))
}

/// Every exterior cell the case's reference stands in, as the engine merges the plugins.
fn stands_in(case: &Val, tag: &str) -> Vec<(i32, i32)> {
    const HOME: (i32, i32) = (1, 1);
    let d = Dir::new(tag);
    let mut paths = Vec::new();
    for p in case.get("plugins").and_then(Val::as_arr).unwrap() {
        let name = p.str_or("name", "");
        let masters: Vec<String> =
            p.get("masters").and_then(Val::as_arr).unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect();
        let mut body = Vec::new();
        for r in p.get("refs").and_then(Val::as_arr).unwrap() {
            let r = r.as_arr().unwrap();
            let word = ((r[0].as_i64().unwrap() as u32) << 24) | r[1].as_i64().unwrap() as u32;
            body.push(reference(word, "rock", r[2].as_bool().unwrap(), HOME, grid(&r[3])));
        }
        let mut records = Vec::new();
        if masters.is_empty() {
            records.push(stat("rock", "f\\rock.nif"));
        }
        records.push(exterior(HOME.0, HOME.1, &body));
        let mut bytes = header(&masters, records.len() as u32);
        for r in &records {
            bytes.extend_from_slice(r);
        }
        let path = d.0.join(&name);
        std::fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    let vfs = Vfs::with_roots(&[("t".to_string(), d.0.clone())]);
    let w = World::load_with(&paths, &vfs, true, &LoadOpts::default());
    let mut at: Vec<(i32, i32)> =
        w.cells.iter().filter(|(_, c)| c.refs.values().any(|r| r.id == "rock")).map(|(g, _)| *g).collect();
    at.sort();
    at
}

#[test]
fn every_move_case_stands_where_the_editor_says() {
    let cases = parse(CASES).unwrap();
    let mut failed = Vec::new();
    for (i, case) in cases.get("cases").and_then(Val::as_arr).unwrap().iter().enumerate() {
        let name = case.str_or("name", "");
        let got = stands_in(case, &i.to_string());
        // An open case pins each side's answer apart (`expect.engine`).
        let expect = case.get("expect").unwrap();
        let expect = expect.get("engine").unwrap_or(expect);
        let want: Vec<(i32, i32)> = expect.as_arr().unwrap().iter().map(|g| grid(g).unwrap()).collect();
        if want != got {
            failed.push(format!("{name}: want {want:?}, the engine has {got:?}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
