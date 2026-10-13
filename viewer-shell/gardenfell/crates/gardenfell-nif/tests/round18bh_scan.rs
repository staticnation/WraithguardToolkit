//! Round 18bh — `scan-meshes`, the generic mesh reader report.
//!
//! Robin asked for a scanner rather than a switch: "I think having a more generic scanner
//! might be good in that case, rather than one for this specific purpose only." The census
//! is the first half — every record type in an install and how many meshes carry one, so
//! "does anything here use X" is answerable without anybody having to ask about X first.
//! The problem list is the half that matters more: a mesh the reader could not follow, one
//! whose records it had to find by scanning, and one that read but came back empty. That
//! last column is what would have found §G2 — a stripified mesh read as nothing at all —
//! with nobody suspecting triangle strips.
//!
//! The fixtures are built byte by byte, the way `round18bg.rs` builds its own: what is
//! under test is the bookkeeping over a reader's answers, not a shape.

use gardenfell_nif::meshscan::{self, Census};

// ---- a NIF, built byte by byte (the shapes `round18bg.rs` uses) -------------------------

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn strb(s: &str) -> Vec<u8> {
    let mut o = (s.len() as u32).to_le_bytes().to_vec();
    o.extend_from_slice(s.as_bytes());
    o
}
const IDENT3: [f32; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

fn av(name: &str) -> Vec<u8> {
    let mut o = strb(name);
    o.extend_from_slice(&(-1i32).to_le_bytes()); // extra data
    o.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    o.extend_from_slice(&0x000eu16.to_le_bytes());
    o.extend_from_slice(&f32s(&[0.0, 0.0, 0.0]));
    o.extend_from_slice(&f32s(&IDENT3));
    o.extend_from_slice(&1.0f32.to_le_bytes());
    o.extend_from_slice(&f32s(&[0.0, 0.0, 0.0])); // velocity
    o.extend_from_slice(&0u32.to_le_bytes()); // no properties
    o.extend_from_slice(&0u32.to_le_bytes()); // has bounding volume
    o
}

fn node(ty: &str, name: &str, kids: &[i32]) -> Vec<u8> {
    let mut o = strb(ty);
    o.extend_from_slice(&av(name));
    o.extend_from_slice(&(kids.len() as u32).to_le_bytes());
    for k in kids {
        o.extend_from_slice(&k.to_le_bytes());
    }
    o.extend_from_slice(&0u32.to_le_bytes()); // effects
    o
}

fn shape_of(ty: &str, name: &str, data: i32) -> Vec<u8> {
    let mut o = strb(ty);
    o.extend_from_slice(&av(name));
    o.extend_from_slice(&data.to_le_bytes());
    o.extend_from_slice(&(-1i32).to_le_bytes()); // skin
    o
}

fn geom_head(verts: &[[f32; 3]]) -> Vec<u8> {
    let mut o = Vec::new();
    o.extend_from_slice(&(verts.len() as u16).to_le_bytes());
    o.extend_from_slice(&1u32.to_le_bytes()); // has vertices
    for v in verts {
        o.extend_from_slice(&f32s(v));
    }
    o.extend_from_slice(&0u32.to_le_bytes()); // no normals
    o.extend_from_slice(&f32s(&[0.0, 0.0, 0.0, 4000.0])); // bounding sphere
    o.extend_from_slice(&0u32.to_le_bytes()); // no vertex colours
    o.extend_from_slice(&0u16.to_le_bytes()); // no UV sets
    o.extend_from_slice(&0u32.to_le_bytes());
    o
}

fn shape_data(verts: &[[f32; 3]], tris: &[[u16; 3]]) -> Vec<u8> {
    let mut o = strb("NiTriShapeData");
    o.extend_from_slice(&geom_head(verts));
    o.extend_from_slice(&(tris.len() as u16).to_le_bytes());
    o.extend_from_slice(&((tris.len() * 3) as u32).to_le_bytes());
    for t in tris {
        for i in t {
            o.extend_from_slice(&i.to_le_bytes());
        }
    }
    o.extend_from_slice(&0u16.to_le_bytes()); // match groups
    o
}

fn strips_data(verts: &[[f32; 3]], strips: &[Vec<u16>]) -> Vec<u8> {
    let mut o = strb("NiTriStripsData");
    o.extend_from_slice(&geom_head(verts));
    let ntris: usize = strips.iter().map(|s| s.len().saturating_sub(2)).sum();
    o.extend_from_slice(&(ntris as u16).to_le_bytes());
    o.extend_from_slice(&(strips.len() as u16).to_le_bytes());
    for s in strips {
        o.extend_from_slice(&(s.len() as u16).to_le_bytes());
    }
    for s in strips {
        for p in s {
            o.extend_from_slice(&p.to_le_bytes());
        }
    }
    o
}

fn nif_file(records: &[Vec<u8>]) -> Vec<u8> {
    let mut o = b"NetImmerse File Format, Version 4.0.0.2\n".to_vec();
    o.extend_from_slice(&0x0400_0002u32.to_le_bytes());
    o.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for r in records {
        o.extend_from_slice(r);
    }
    o.extend_from_slice(&1u32.to_le_bytes());
    o.extend_from_slice(&0i32.to_le_bytes());
    o
}

fn quad() -> (Vec<[f32; 3]>, Vec<[u16; 3]>, Vec<u16>) {
    (
        vec![[0., 0., 0.], [10., 0., 0.], [0., 10., 0.], [10., 10., 0.]],
        vec![[0, 1, 2], [2, 1, 3]],
        vec![0, 1, 2, 3],
    )
}

/// A mesh with one `NiTriShape` in it, geometry and all.
fn plain() -> Vec<u8> {
    let (v, t, _) = quad();
    nif_file(&[node("NiNode", "Root", &[1]), shape_of("NiTriShape", "Tri", 2), shape_data(&v, &t)])
}

/// The same mesh written as a triangle strip — §G2's shape.
fn stripped() -> Vec<u8> {
    let (v, _, s) = quad();
    nif_file(&[node("NiNode", "Root", &[1]), shape_of("NiTriStrips", "Strip", 2), strips_data(&v, &[s])])
}

/// A mesh with nothing in it but a node: reads fine, holds no geometry.
fn hollow() -> Vec<u8> {
    nif_file(&[node("NiNode", "Root", &[])])
}

fn scan(files: &[(&str, Vec<u8>)], want: &[&str]) -> Census {
    let want = meshscan::wanted(&want.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let mut c = Census::default();
    for (name, bytes) in files {
        meshscan::one(&mut c, name, bytes, &want);
    }
    c
}

// ---- the census ------------------------------------------------------------------------

#[test]
fn the_census_counts_a_record_once_per_mesh_not_once_per_record() {
    // Two meshes, each holding one NiNode and one NiTriShape; a third holding two shapes.
    let (v, t, _) = quad();
    let two_shapes = nif_file(&[
        node("NiNode", "Root", &[1, 3]),
        shape_of("NiTriShape", "A", 2),
        shape_data(&v, &t),
        shape_of("NiTriShape", "B", 4),
        shape_data(&v, &t),
    ]);
    let c = scan(&[("a.nif", plain()), ("b.nif", plain()), ("c.nif", two_shapes)], &[]);
    assert_eq!(c.found, 3);
    assert_eq!(c.read, 3);
    // Three meshes carry a NiTriShape, not four shapes' worth.
    assert_eq!(c.records.get("NiTriShape"), Some(&3), "{:?}", c.records);
    assert_eq!(c.records.get("NiNode"), Some(&3));
}

#[test]
fn the_rows_come_back_commonest_first_and_ties_go_by_name() {
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped())], &[]);
    let rows = c.rows();
    // NiNode is in both; the two geometry kinds are in one each and tie.
    assert_eq!(rows[0], ("NiNode", 2));
    let rest: Vec<&str> = rows[1..].iter().map(|r| r.0).collect();
    let mut sorted = rest.clone();
    sorted.sort();
    assert_eq!(rest, sorted, "ties are not in name order: {rest:?}");
}

#[test]
fn a_record_nothing_uses_is_simply_absent() {
    let c = scan(&[("a.nif", plain())], &[]);
    assert!(!c.records.contains_key("NiTriStripsData"));
    // Which is the whole promise of the report: the reader read this mesh, so a type
    // missing from the list is a type the install does not use — not one nobody looked for.
    assert_eq!(c.read, c.found);
}

// ---- --mesh-has ------------------------------------------------------------------------

#[test]
fn mesh_has_names_the_meshes_carrying_the_record() {
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped()), ("c.nif", plain())], &["NiTriStripsData"]);
    assert_eq!(c.carrying, vec!["b.nif"]);
}

#[test]
fn mesh_has_does_not_care_about_case_because_a_shell_is_typed_by_hand() {
    let c = scan(&[("a.nif", stripped())], &["nitristripsdata"]);
    assert_eq!(c.carrying, vec!["a.nif"]);
}

#[test]
fn several_records_asked_for_at_once_match_any_of_them() {
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped())], &["NiTriShapeData", "NiTriStripsData"]);
    assert_eq!(c.carrying, vec!["a.nif", "b.nif"]);
}

#[test]
fn asking_for_nothing_carries_nothing() {
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped())], &[]);
    assert!(c.carrying.is_empty(), "nobody asked, so nothing should be singled out");
    assert_eq!(c.records.len(), 5, "{:?}", c.records);
}

// ---- --problems ------------------------------------------------------------------------

#[test]
fn a_file_that_is_not_a_nif_at_all_is_unreadable_and_not_counted_as_read() {
    let c = scan(&[("a.nif", plain()), ("junk.nif", b"not a nif".to_vec())], &[]);
    assert_eq!(c.unreadable, vec!["junk.nif"]);
    assert_eq!(c.read, 1);
    assert_eq!(c.found, 2, "it was still offered to the reader");
    assert!(c.problems());
}

#[test]
fn a_mesh_that_reads_but_holds_no_geometry_is_a_problem_of_its_own() {
    let c = scan(&[("hollow.nif", hollow())], &[]);
    assert_eq!(c.empty, vec!["hollow.nif"]);
    assert!(c.unreadable.is_empty(), "it read perfectly well — that is the point");
    assert_eq!(c.read, 1);
}

#[test]
fn a_mesh_with_geometry_is_not_reported_empty() {
    // The §G2 regression in report form: before the strips reader landed, this mesh read
    // as nothing and would have shown up here. It must not any more.
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped())], &[]);
    assert!(c.empty.is_empty(), "something with faces was called empty: {:?}", c.empty);
    assert!(!c.problems(), "a clean install should report no problems");
}

#[test]
fn a_clean_install_has_nothing_to_report() {
    let c = scan(&[("a.nif", plain())], &[]);
    assert!(!c.problems());
    assert!(c.unreadable.is_empty() && c.empty.is_empty());
}

// ---- the TOML report -------------------------------------------------------------------

#[test]
fn the_toml_report_explains_itself_before_it_says_anything() {
    let c = scan(&[("a.nif", plain()), ("junk.nif", b"nope".to_vec())], &[]);
    let t = meshscan::census_toml(&c);
    assert!(t.starts_with('#'), "the report opens with its own explanation");
    // Robin: a --toml that allows "automated commenting that helps a human read it too".
    assert!(t.contains("# The number beside each is how many *meshes* carry at least one"));
    assert!(t.contains("read  = 1"), "{t}");
    assert!(t.contains("found = 2"), "{t}");
    assert!(t.contains("[records]"));
    assert!(t.contains("\"NiTriShape\""), "{t}");
}

#[test]
fn the_toml_report_parses_as_the_toml_it_claims_to_be() {
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped())], &[]);
    let t = meshscan::census_toml(&c);
    let v = gardenfell_bytes::toml::parse(&t).expect("the report is not valid TOML");
    let meshes = v.get("meshes").and_then(|x| x.as_table()).expect("no [meshes] table");
    assert_eq!(meshes.get("read").and_then(|x| x.as_i64()), Some(2));
    assert_eq!(meshes.get("found").and_then(|x| x.as_i64()), Some(2));
    let records = v.get("records").and_then(|x| x.as_table()).expect("no [records] table");
    assert_eq!(records.get("NiNode").and_then(|x| x.as_i64()), Some(2));
    assert_eq!(records.get("NiTriStripsData").and_then(|x| x.as_i64()), Some(1));
}

// ---- the column that would have caught §G2 ---------------------------------------------

/// A mesh carrying a record of a type the format does not have, between two it does.
///
/// This was the shape of every silent NIF failure: the byte reader could not measure the
/// record, so it did not know where the next one started, and it scanned forward to a
/// plausible type name to get its bearings back, losing everything between without a word.
/// The `guessed` column was the only place that ever got said out loud. Round 18gh: the
/// typed model has every type 4.0.0.2 has and refuses one it does not by name, so the file
/// is unreadable, which the `unreadable` column says.
fn with_an_unknown_record() -> Vec<u8> {
    let (v, t, _) = quad();
    let mut junk = strb("NiFooBarWidget");
    junk.extend_from_slice(&[0xAB; 24]);
    nif_file(&[node("NiNode", "Root", &[2]), junk, shape_of("NiTriShape", "Tri", 3), shape_data(&v, &t)])
}

#[test]
fn a_record_of_a_type_the_format_does_not_have_is_reported_as_unreadable() {
    /* Round 18gh: this pinned the `guessed` column, the count of records the byte reader
    had to scan past; it read the mesh anyway, which made it the dangerous case rather
    than the loud one. The typed model does not guess: a type name it does not have
    refuses the file, so the mesh is the loud case now, in the `unreadable` column, and
    the `guessed` column is gone with the guessing. */
    let c = scan(&[("odd.nif", with_an_unknown_record())], &[]);
    assert_eq!(c.unreadable, vec!["odd.nif".to_string()], "the refused mesh is said by name");
    assert!(c.problems(), "a mesh the reader could not read is a problem");
    assert_eq!(c.read, 0);
    assert!(c.empty.is_empty());
}

#[test]
fn a_mesh_the_reader_followed_cleanly_is_not_a_problem() {
    let c = scan(&[("a.nif", plain()), ("b.nif", stripped()), ("h.nif", hollow())], &[]);
    assert!(c.unreadable.is_empty(), "every one of these reads: {:?}", c.unreadable);
    assert_eq!(c.read, 3);
}
