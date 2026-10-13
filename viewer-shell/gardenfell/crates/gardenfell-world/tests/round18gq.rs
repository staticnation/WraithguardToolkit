//! Round 18gq: a world read in the install's code page. With Windows-1251 in force, a
//! Russian plugin's ids and mesh paths come out as the Cyrillic they are and the mesh is
//! found on disk under its own spelling; the load order of the Russian edition's ini reads.
//!
//! The page is process-wide (`codepage::set`), so the one test that changes it is the only
//! test in this binary that decodes anything, and it puts Windows-1252 back before it ends.

use gardenfell_bytes::codepage::{self, CodePage};
use gardenfell_install::vfs::Vfs;
use gardenfell_world::world::{read_load_order, LoadOpts, World};
use std::path::{Path, PathBuf};

fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(data);
    v
}

/// A NUL-terminated string in Windows-1251, as a Russian plugin stores it.
fn zstr(s: &str) -> Vec<u8> {
    let mut v = CodePage::Win1251.encode(s).unwrap();
    v.push(0);
    v
}

fn rec(tag: &[u8; 4], subs: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = subs.concat();
    let mut v = tag.to_vec();
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&body);
    v
}

fn header(records: u32) -> Vec<u8> {
    let mut hedr = Vec::with_capacity(300);
    hedr.extend_from_slice(&1.3f32.to_le_bytes());
    hedr.extend_from_slice(&0u32.to_le_bytes());
    hedr.extend_from_slice(&[0u8; 32]);
    hedr.extend_from_slice(&[0u8; 256]);
    hedr.extend_from_slice(&records.to_le_bytes());
    rec(b"TES3", &[sub(b"HEDR", &hedr)])
}

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("gf18gq-{}-{}", tag, std::process::id()));
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

#[test]
fn a_russian_plugin_reads_as_cyrillic_under_its_own_page_and_its_mesh_is_found() {
    let d = Dir::new("plugin");
    let mesh_dir = d.0.join("Meshes").join("Трава");
    std::fs::create_dir_all(&mesh_dir).unwrap();
    std::fs::write(mesh_dir.join("Трава_01.nif"), b"NetImmerse File Format, Version 4.0.0.2\n").unwrap();
    let mut bytes = header(1);
    bytes.extend(rec(
        b"STAT",
        &[sub(b"NAME", &zstr("gf_трава_01")), sub(b"MODL", &zstr("Трава\\ТРАВА_01.nif"))],
    ));
    let plugin = d.0.join("Трава.esp");
    std::fs::write(&plugin, &bytes).unwrap();
    let vfs = Vfs::with_roots(&[("t".to_string(), d.0.clone())]);

    // As every round before this one read it: Windows-1252, and the letters are not letters.
    assert_eq!(codepage::current(), CodePage::Win1252);
    let w = World::load_with(&[plugin.clone()], &vfs, true, &LoadOpts::default());
    let (id, model) = w.models.iter().next().map(|(k, v)| (k.clone(), v.clone())).expect("one static");
    assert!(id.starts_with("gf_") && !id.contains("трава"), "{id}");
    assert!(model.contains('Ò'), "a Windows-1252 misreading: {model}");
    assert!(vfs.resolve(&format!("meshes\\{model}")).is_none(), "and the mesh is not found that way");

    // In the install's page, which the connect sets from the layout.
    codepage::set(CodePage::Win1251);
    let w = World::load_with(&[plugin], &vfs, true, &LoadOpts::default());
    let (id, model) = w.models.iter().next().map(|(k, v)| (k.clone(), v.clone())).expect("one static");
    assert_eq!(model, "Трава\\ТРАВА_01.nif");
    assert!(id.starts_with("gf_"), "{id}");
    assert!(vfs.resolve(&format!("meshes\\{model}")).is_some(), "the Cyrillic mesh is found on disk");
    // A plugin written in the page in force puts the same bytes back.
    let mut wr = gardenfell_bytes::bytes::Writer::new();
    wr.cstring("Трава\\ТРАВА_01.nif").unwrap();
    assert_eq!(wr.out, zstr("Трава\\ТРАВА_01.nif"));
    let err = wr.cstring("é").unwrap_err();
    assert!(err.what.contains("Windows-1251"), "{}", err.what);
    codepage::set(CodePage::Win1252);
    assert_eq!(codepage::current(), CodePage::Win1252);
}

#[test]
fn the_russian_editions_ini_lists_its_plugins() {
    let ru = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixture/ru");
    let vanilla = ru.join("Morrowind-vanilla.ini");
    let tester = ru.join("Morrowind-tester.ini");
    if !vanilla.is_file() || !tester.is_file() {
        eprintln!("fixture/ru absent, skipping");
        return;
    }
    assert_eq!(read_load_order(&vanilla), ["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]);
    let order = read_load_order(&tester);
    assert_eq!(order.len(), 56, "{order:?}");
    assert_eq!(order[0], "BT_Whitewolf_2_0.esm");
    assert!(order.iter().any(|n| n == "MFR_RuVoice_v1.0.esm"), "{order:?}");
}
