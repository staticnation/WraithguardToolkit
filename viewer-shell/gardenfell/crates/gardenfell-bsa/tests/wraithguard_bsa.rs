//! Wraithguard (carried patch 0010): an archive is opened, not read. The file-backed
//! archive (`open`) and the in-memory one (`parse`) give the same bytes for every entry,
//! through `read` and through `get`.

use gardenfell_bsa::bsa::Bsa;

/// A version-0x100 archive with `files` (backslash names), laid out as Morrowind's are.
fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let n = files.len();
    let mut names = Vec::new();
    let mut name_offs = Vec::new();
    for (name, _) in files {
        name_offs.push(names.len() as u32);
        names.extend_from_slice(name.as_bytes());
        names.push(0);
    }
    let hash_off = (8 * n + 4 * n + names.len()) as u32;
    let mut out = Vec::new();
    for v in [0x100u32, hash_off, n as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let mut off = 0u32;
    for (_, data) in files {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        off += data.len() as u32;
    }
    for o in &name_offs {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&names);
    out.extend(std::iter::repeat(0u8).take(8 * n)); // the hashes, not read
    for (_, data) in files {
        out.extend_from_slice(data);
    }
    out
}

#[test]
fn opened_and_parsed_archives_give_the_same_bytes() {
    let big = vec![7u8; 1 << 20];
    let files: [(&str, &[u8]); 3] =
        [("meshes\\a.nif", b"mesh"), ("textures\\b.dds", &big), ("textures\\c.dds", b"")];
    let bytes = archive(&files);
    let path = std::env::temp_dir().join(format!("wg_bsa_{}.bsa", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let opened = Bsa::open(&path).expect("opens");
    let parsed = Bsa::parse(&path, bytes).expect("parses");
    assert_eq!(opened.len(), 3);
    for key in ["meshes/a.nif", "textures/b.dds", "textures/c.dds"] {
        let want = parsed.get(key).expect("in memory").to_vec();
        assert_eq!(opened.read(key).as_deref(), Some(&want[..]), "read {key}");
        assert_eq!(opened.get(key), Some(&want[..]), "get {key}");
        assert_eq!(parsed.read(key).as_deref(), Some(&want[..]));
    }
    assert!(opened.read("textures/none.dds").is_none());
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_damaged_archive_is_refused_not_panicked_on() {
    let path = std::env::temp_dir().join(format!("wg_bsa_bad_{}.bsa", std::process::id()));
    std::fs::write(&path, [0u8, 1, 0, 0, 9]).unwrap();
    assert!(Bsa::open(&path).is_err());
    std::fs::write(&path, [0u8, 1, 0, 0]).unwrap();
    assert!(Bsa::open(&path).is_err(), "shorter than a header");
    std::fs::remove_file(&path).ok();
}

/// Carried patch 0011: part of an entry, from the open file and from memory alike, cut
/// at the entry's end.
#[test]
fn part_of_an_entry_is_read_without_the_rest() {
    let data: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    let files: [(&str, &[u8]); 2] = [("meshes\\a.nif", b"mesh"), ("textures\\b.dds", &data)];
    let bytes = archive(&files);
    let path = std::env::temp_dir().join(format!("wg_bsa_range_{}.bsa", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let opened = Bsa::open(&path).expect("opens");
    let parsed = Bsa::parse(&path, bytes).expect("parses");
    for a in [&opened, &parsed] {
        assert_eq!(a.size("textures/b.dds"), Some(5000));
        assert_eq!(a.read_range("textures/b.dds", 100, 28).as_deref(), Some(&data[100..128]));
        assert_eq!(a.read_range("textures/b.dds", 4990, 100).as_deref(), Some(&data[4990..]), "cut at the end");
        assert_eq!(a.read_range("textures/b.dds", 9000, 10).as_deref(), Some(&[][..]), "past the end: nothing");
        assert_eq!(a.read_range("meshes/a.nif", 0, 4).as_deref(), Some(&b"mesh"[..]), "not the next entry");
        assert!(a.read_range("textures/none.dds", 0, 4).is_none());
    }
    std::fs::remove_file(&path).ok();
}
