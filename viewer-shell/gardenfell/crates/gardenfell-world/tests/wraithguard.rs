//! Patch 0007 from Wraithguard (Static): with `LoadOpts::openmw_kinds`, what a placed
//! object of an id defined as two kinds is, as OpenMW resolves it (`esmstore.cpp`:
//! records load into their own kind's store, a deletion erases from that store only, and
//! `rebuildIdsIndex` walks the stores in record-name order, the later overwriting).

use gardenfell_install::vfs::Vfs;
use gardenfell_world::world::{LoadOpts, World};
use std::path::PathBuf;

/// A record as a plugin file holds it: tag, size, header words, then subrecords.
fn record(tag: &[u8; 4], subs: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (t, d) in subs {
        body.extend_from_slice(*t);
        body.extend_from_slice(&(d.len() as u32).to_le_bytes());
        body.extend_from_slice(d);
    }
    let mut out = tag.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend(body);
    out
}

fn header() -> Vec<u8> {
    let mut hedr = vec![0u8; 300];
    hedr[..4].copy_from_slice(&1.3f32.to_le_bytes());
    record(b"TES3", &[(b"HEDR", &hedr)])
}

fn write(dir: &PathBuf, name: &str, parts: &[Vec<u8>]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, parts.concat()).unwrap();
    p
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gdn-wg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn an_id_of_two_kinds_is_placed_as_openmw_places_it_whatever_the_order() {
    // A.esp has `wg_clash` as a weapon; B.esp, later, as a misc item. OpenMW keeps both and
    // places the weapon (WEAP's tag outranks MISC's), so the world draws the weapon's mesh.
    let dir = tmp("clash");
    let a = write(&dir, "A.esp", &[header(), record(b"WEAP", &[(b"NAME", b"wg_clash\0"), (b"MODL", b"w\\sword.nif\0")])]);
    let b = write(&dir, "B.esp", &[header(), record(b"MISC", &[(b"NAME", b"WG_Clash\0"), (b"MODL", b"m\\cup.nif\0")])]);
    let w = World::load_with(&[a.clone(), b.clone()], &Vfs::default(), false, &LoadOpts { openmw_kinds: true, ..LoadOpts::default() });
    assert_eq!(w.models.get("wg_clash").map(String::as_str), Some("w\\sword.nif"));
    // The other order places the weapon too.
    let w = World::load_with(&[b, a], &Vfs::default(), false, &LoadOpts { openmw_kinds: true, ..LoadOpts::default() });
    assert_eq!(w.models.get("wg_clash").map(String::as_str), Some("w\\sword.nif"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_deleted_kind_gives_way_to_the_other() {
    let dir = tmp("dele");
    let a = write(
        &dir,
        "A.esp",
        &[
            header(),
            record(b"WEAP", &[(b"NAME", b"x\0"), (b"MODL", b"w\\sword.nif\0")]),
            record(b"MISC", &[(b"NAME", b"x\0"), (b"MODL", b"m\\cup.nif\0")]),
        ],
    );
    let b = write(&dir, "B.esp", &[header(), record(b"WEAP", &[(b"NAME", b"x\0"), (b"DELE", &[0; 4])])]);
    let w = World::load_with(&[a, b], &Vfs::default(), false, &LoadOpts { openmw_kinds: true, ..LoadOpts::default() });
    assert_eq!(w.models.get("x").map(String::as_str), Some("m\\cup.nif"));
    let _ = std::fs::remove_dir_all(&dir);
}
