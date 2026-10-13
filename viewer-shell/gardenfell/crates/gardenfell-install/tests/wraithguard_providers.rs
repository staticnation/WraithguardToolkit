//! Wraithguard (carried patch 0003): `Vfs::providers` lists every place that has a file,
//! and its first is the file the resolver loads - checked here in the three cases where
//! the first version of the list disagreed (Robin's review, 2026-10-12): one mod with two
//! spellings, archives that hold different spellings, and a texture found without the
//! `textures/` prefix.

use gardenfell_install::vfs::{Loc, Vfs};
use std::path::{Path, PathBuf};

/// A scratch folder for one test, empty.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wg_prov_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(dir: &Path, rel: &str, bytes: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A version-0x100 archive holding `files` (backslash names), written to `path`.
fn bsa(path: &Path, files: &[(&str, &[u8])]) {
    let n = files.len();
    let (mut names, mut offs) = (Vec::new(), Vec::new());
    for (name, _) in files {
        offs.push(names.len() as u32);
        names.extend_from_slice(name.as_bytes());
        names.push(0);
    }
    let mut out = Vec::new();
    for v in [0x100u32, (12 * n + names.len()) as u32, n as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let mut off = 0u32;
    for (_, d) in files {
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        off += d.len() as u32;
    }
    for o in &offs {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&names);
    out.extend(std::iter::repeat(0u8).take(8 * n));
    for (_, d) in files {
        out.extend_from_slice(d);
    }
    std::fs::write(path, out).unwrap();
}

/// The list's first entry is the resolver's answer, and the list holds every place.
fn agrees(v: &Vfs, rel: &str, places: usize) -> Loc {
    let got = v.resolve_texture(rel).expect("the resolver finds it");
    let list = v.texture_providers(rel);
    assert_eq!(list.first().map(|(_, l)| l), Some(&got), "{rel}: {list:?}");
    assert_eq!(list.len(), places, "{rel}: {list:?}");
    got
}

#[test]
fn one_mod_with_both_spellings_lists_the_dds_first() {
    let d = scratch("both");
    put(&d.join("mod"), "textures/tx_rock.tga", b"tga");
    put(&d.join("mod"), "textures/tx_rock.dds", b"dds");
    let v = Vfs::with_roots(&[("mod".into(), d.join("mod"))]);
    let got = agrees(&v, "tx_rock.tga", 2);
    assert_eq!(v.load(&got).unwrap(), b"dds");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn archives_list_the_dds_in_every_archive_before_the_spelled_name() {
    let d = scratch("arch");
    bsa(&d.join("old.bsa"), &[("textures\\tx_rock.dds", b"old dds")]);
    bsa(&d.join("new.bsa"), &[("textures\\tx_rock.tga", b"new tga")]);
    let mut v = Vfs::with_roots(&[]);
    assert!(v.add_archives(&[d.join("old.bsa"), d.join("new.bsa")]).is_empty());
    let got = agrees(&v, "tx_rock.tga", 2);
    assert_eq!(v.load(&got).unwrap(), b"old dds");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_texture_without_the_prefix_is_listed_where_it_is_found() {
    let d = scratch("noprefix");
    put(&d.join("mod"), "tx_loose.dds", b"top level");
    let v = Vfs::with_roots(&[("mod".into(), d.join("mod"))]);
    let got = agrees(&v, "tx_loose.tga", 1);
    assert_eq!(v.load(&got).unwrap(), b"top level");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn meshes_list_as_the_mesh_resolver_looks() {
    let d = scratch("mesh");
    put(&d.join("hi"), "meshes/x/a.nif", b"hi");
    put(&d.join("lo"), "meshes/x/a.nif", b"lo");
    let v = Vfs::with_roots(&[("hi".into(), d.join("hi")), ("lo".into(), d.join("lo"))]);
    let list = v.mesh_providers("x\\a.nif");
    assert_eq!(list.first().map(|(_, l)| l), v.resolve_mesh("x\\a.nif").as_ref());
    assert_eq!(list.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["hi", "lo"]);
    std::fs::remove_dir_all(&d).ok();
}
