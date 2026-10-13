//! Round 18gg: the typed TES3 model (`gardenfell_tes3::tes3`) against real plugins.
//!
//! The model is only worth building on if every record of a real file types, writes, and
//! reads its own output back to itself. This runs `tes3::check::check` over every plugin
//! the repository carries (the fixtures under `fixture/` and `page-source/`), and over
//! the game's own three masters when `GARDENFELL_ESM_DIR` names their folder, which is
//! the real test. Exactness (written back to the very bytes the file held) is not a gate
//! because Bethesda's files store many strings without a NUL and leave uninitialized
//! bytes in padding; it is pinned to a floor so a layout that starts dropping something
//! is noticed.

use std::fs;
use std::path::PathBuf;

use gardenfell_tes3::tes3::check::{check, Report};
use gardenfell_tes3::tes3::{raw, types};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn plugins_under(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(plugins_under(&p));
        } else if gardenfell_bytes::paths::is_plugin(&p.to_string_lossy()) {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn clean_or_explain(path: &PathBuf, rep: &Report) {
    assert!(rep.raw_exact.is_ok(), "{}: raw layer: {:?}", path.display(), rep.raw_exact);
    assert!(rep.clean(), "{}: not clean:\n{rep}", path.display());
}

#[test]
fn every_fixture_plugin_types_and_reads_itself_back() {
    let mut files = plugins_under(&repo().join("fixture"));
    files.extend(plugins_under(&repo().join("page-source")));
    assert!(files.len() > 30, "found only {} fixture plugins", files.len());
    for path in &files {
        let buf = fs::read(path).unwrap();
        let rep = check(&buf);
        assert_eq!(rep.records, raw::parse_plugin(&buf).unwrap().len());
        let name = path.file_name().unwrap().to_string_lossy();
        // `page-source/mkesm.js` pads its two plugins with `FILL` subrecords and a 40000
        // byte `FRMR` to make them big; they are not the format, and the strict reader
        // must say so by name rather than take them.
        if path.parent().unwrap().ends_with("page-source")
            && (name == "Morrowind.esm" || name == "GrassMod.esp")
        {
            assert!(rep.raw_exact.is_ok(), "{}: raw layer: {:?}", path.display(), rep.raw_exact);
            let cell = &rep.tags["CELL"];
            assert!(cell.first_unfit.as_ref().unwrap().1.contains("CELL::FRMR is"), "{rep}");
            continue;
        }
        clean_or_explain(path, &rep);
    }
}

/// The game's own masters, where `GARDENFELL_ESM_DIR` names their folder. The floors are
/// what the 18gg run measured (Morrowind 54.81%, Tribunal 64.68%, Bloodmoon 68.78%), a
/// little under, so a drop is a finding and a rise is free.
#[test]
fn the_vanilla_masters_type_and_read_themselves_back() {
    let Ok(dir) = std::env::var("GARDENFELL_ESM_DIR") else { return };
    let mut seen = std::collections::BTreeSet::new();
    let mut any = false;
    for (name, floor) in [("Morrowind.esm", 54.0), ("Tribunal.esm", 64.0), ("Bloodmoon.esm", 68.0)] {
        let path = PathBuf::from(&dir).join(name);
        let Ok(buf) = fs::read(&path) else { continue };
        any = true;
        let rep = check(&buf);
        clean_or_explain(&path, &rep);
        let exact = rep.exact() as f64 * 100.0 / rep.typed() as f64;
        assert!(exact >= floor, "{name}: {exact:.2}% written back exactly, floor {floor}%\n{rep}");
        seen.extend(rep.tags.keys().cloned());
    }
    // Every one of the 43 record types occurs in the three together (SSCR came with
    // Tribunal), so each layout has met the real thing.
    if any {
        for t in types::TAGS {
            assert!(seen.contains(&raw::tag_str(t)), "no {} in the masters", raw::tag_str(t));
        }
    }
}

/// The three questions the check asks, each answered on a record built to fail it.
#[test]
fn the_check_tells_the_three_failures_apart() {
    // A record whose body claims more bytes than the file has: the raw layer refuses.
    let mut rec = raw::Record::new(b"STAT");
    rec.push(b"NAME", b"rock\0".to_vec());
    let mut buf = raw::write_plugin(&[rec.clone()]);
    buf.truncate(buf.len() - 1);
    let rep = check(&buf);
    assert!(rep.raw_exact.is_err());
    assert_eq!(rep.records, 0);

    // A subrecord the layout does not name: typed, no; raw, yes.
    let mut odd = rec.clone();
    odd.push(b"ZZZZ", vec![9]);
    let rep = check(&raw::write_plugin(&[odd]));
    assert!(rep.raw_exact.is_ok());
    assert_eq!(rep.typed(), 0);
    assert!(rep.tags["STAT"].first_unfit.as_ref().unwrap().1.contains("STAT::ZZZZ"));

    // A string without its NUL: typed and idempotent, not exact, and the report says where.
    let mut bare = raw::Record::new(b"STAT");
    bare.push(b"NAME", b"rock".to_vec());
    bare.push(b"MODL", b"f\\rock.nif\0".to_vec());
    let rep = check(&raw::write_plugin(&[bare]));
    assert!(rep.clean());
    assert_eq!(rep.exact(), 0);
    let (_, id, at) = rep.tags["STAT"].first_inexact.clone().unwrap();
    assert_eq!(id, "rock");
    assert_eq!(at, 4, "the size word of the first subrecord");
}

// ---- the typed NIF model (`gardenfell_nif::netimmerse`) ----------------------------------------

use gardenfell_nif::netimmerse::check::check as check_nif;

fn meshes_under(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(meshes_under(&p));
        } else {
            let lower = p.to_string_lossy().to_lowercase();
            if lower.ends_with(".nif") || lower.ends_with(".kf") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Every mesh the repository carries reads to the end and reads its own output back.
/// The two game meshes among them (`ashcloud.nif`, `ex_stronghold_wall00.nif`) are the
/// ones with uninitialized has-words, so they are read-and-idempotent, not byte for byte;
/// the generated fixtures are byte for byte.
#[test]
fn every_fixture_mesh_reads_to_the_end_and_back() {
    let mut files = meshes_under(&repo().join("fixture"));
    files.extend(meshes_under(&repo().join("page-source")));
    assert!(files.len() > 40, "found only {} fixture meshes", files.len());
    let mut exact = 0;
    for path in &files {
        let buf = fs::read(path).unwrap();
        let rep = check_nif(&buf);
        // `page-source/test2.nif` carries an `NiWeirdThing` block on purpose, for the
        // page's reader to skip; the strict model must refuse it by name.
        if path.ends_with("page-source/test2.nif") {
            assert!(rep.read.as_ref().unwrap_err().contains("NiWeirdThing"), "{rep}");
            continue;
        }
        assert!(rep.clean(), "{}: {rep}", path.display());
        if rep.exact.is_ok() {
            exact += 1;
        }
    }
    assert!(exact + 4 >= files.len(), "{exact} of {} byte for byte", files.len());
}

/// A corpus of real meshes to measure against: the game's own archive when `GARDENFELL_BSA`
/// names `Morrowind.bsa` (the convention `tests/alpha.rs` and `tests/anim.rs` use), and a
/// folder of loose files walked to the bottom when `GARDENFELL_MESH_DIR` names one. Robin's
/// rule (18gg): vanilla is not enough on its own, OAAB_Data has the more demanding files,
/// so both gates are run. Names come out with the archive's or the folder's own spelling
/// and `/`-separated below the folder; `get` takes that name back.
struct Corpus {
    label: String,
    names: Vec<String>,
    get: Box<dyn Fn(&str) -> Option<Vec<u8>>>,
}

fn corpora() -> Vec<Corpus> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("GARDENFELL_BSA") {
        let bsa = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)).expect("the archive opens");
        let names: Vec<String> = bsa
            .names()
            .iter()
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".nif") || l.ends_with(".kf")
            })
            .cloned()
            .collect();
        out.push(Corpus {
            label: p,
            names,
            get: Box::new(move |n| bsa.get(&gardenfell_bytes::paths::norm(n)).map(|b| b.to_vec())),
        });
    }
    if let Ok(p) = std::env::var("GARDENFELL_MESH_DIR") {
        let root = PathBuf::from(&p);
        let mut names = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).expect("the folder lists") {
                let e = e.unwrap().path();
                if e.is_dir() {
                    stack.push(e);
                } else if let Some(x) = e.extension().and_then(|x| x.to_str()) {
                    if x.eq_ignore_ascii_case("nif") || x.eq_ignore_ascii_case("kf") {
                        let rel = e.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                        names.push(rel);
                    }
                }
            }
        }
        names.sort();
        out.push(Corpus { label: p, names, get: Box::new(move |n| fs::read(root.join(n)).ok()) });
    }
    out
}

/// Every mesh and animation in each corpus reads to the end and reads back to itself.
/// Round 18gg ran this over all three vanilla archives (7481, every one clean) and over
/// OAAB_Data's loose meshes (4630, every one clean, 4326 byte for byte).
#[test]
fn every_mesh_in_the_corpus_reads_to_the_end_and_back() {
    for c in corpora() {
        let mut failures = Vec::new();
        for name in &c.names {
            let buf = (c.get)(name).unwrap();
            let rep = check_nif(&buf);
            if !rep.clean() {
                failures.push(format!("{name}: {rep}"));
            }
        }
        let n = c.names.len();
        assert!(n > 500, "only {n} meshes in {}", c.label);
        assert!(
            failures.is_empty(),
            "{}: {} of {n} not clean:\n  {}",
            c.label,
            failures.len(),
            failures.join("\n  ")
        );
    }
}

/// Every mesh in each corpus turned root-last and read by the engine both ways: the same
/// hull, or none both times. This is the test that measures the engine's reader against
/// the typed model record by record, because the turn puts the root after the last record
/// and the engine only finds it when every record before it was measured exactly. It is
/// what found 18gf's bounding volume with box and sphere the wrong way round: the guar's
/// "Bounding Box" node is its last record, so the original read fine with the footer
/// unfound, and the turned file read nothing. With the `.kf` beside the `x` twins, as the
/// world reads them.
#[test]
fn every_mesh_in_the_corpus_reads_the_same_with_its_root_last() {
    use gardenfell_nif::netimmerse::stream::Stream;
    for c in corpora() {
        let (mut n, mut failures) = (0, Vec::new());
        for name in &c.names {
            if !name.to_ascii_lowercase().ends_with(".nif") {
                continue;
            }
            let buf = (c.get)(name).unwrap();
            let Ok(mut s) = Stream::parse(&buf) else { continue };
            let blocks = s.blocks.len();
            if blocks < 2 || s.roots.first().map(|r| r.index) != Some(0) {
                continue;
            }
            let mut order: Vec<usize> = (1..blocks).collect();
            order.push(0);
            assert!(s.reorder(&order));
            let turned = s.write().unwrap();
            n += 1;
            let kf = (c.get)(&format!("{}.kf", &name[..name.len() - 4]))
                .and_then(|b| gardenfell_nif::nif::parse_kf(&b));
            let a = gardenfell_nif::nif::parse_with_kf(&buf, kf.as_ref());
            let b = gardenfell_nif::nif::parse_with_kf(&turned, kf.as_ref());
            let same = match (&a, &b) {
                (Some(x), Some(y)) => {
                    x.tris.len() == y.tris.len() && x.aabb_min == y.aabb_min && x.aabb_max == y.aabb_max
                }
                (None, None) => true,
                _ => false,
            };
            if !same {
                failures.push(format!(
                    "{name}: original {:?}, turned {:?}",
                    a.map(|g| g.tris.len()),
                    b.map(|g| g.tris.len())
                ));
            }
        }
        assert!(n > 500, "only {n} meshes turned in {}", c.label);
        assert!(
            failures.is_empty(),
            "{}: {} of {n} read differently root-last:\n  {}",
            c.label,
            failures.len(),
            failures.join("\n  ")
        );
    }
}

/// The three verdicts on meshes built to earn each.
#[test]
fn the_nif_check_tells_its_verdicts_apart() {
    use gardenfell_nif::netimmerse::stream::{Link, Stream};
    use gardenfell_nif::netimmerse::types::*;
    let mut s = Stream::new();
    let mut root = NiNode::default();
    root.base.base.name = "root".into();
    root.children = vec![Link::new(1)];
    let mut shape = NiTriShape::default();
    shape.base.base.geometry_data = Link::new(2);
    shape.base.base.base.bounding_volume =
        Some(NiBoundingVolume::Sphere(NiSphereBV { center: [0.0; 3], radius: 1.0 }));
    let mut data = NiTriShapeData::default();
    data.base.base.vertices = vec![[0.0; 3]; 3];
    data.triangles = vec![[0, 1, 2]];
    s.push(root);
    s.push(shape);
    s.push(data);
    s.roots = vec![Link::new(0)];
    let bytes = s.write().unwrap();
    let rep = check_nif(&bytes);
    assert!(rep.clean() && rep.exact.is_ok(), "{rep}");
    assert_eq!(rep.block_count(), 3);

    // A has-word with bytes above the low one: read and idempotent, not exact. The
    // shape's has-bounding-volume word follows its name, the two chain links, the flags,
    // the transform (12 + 36 + 4 + 12 bytes) and the empty property list.
    let mut dirty = bytes.clone();
    let name_at = bytes.windows(10).position(|w| w == b"NiTriShape").unwrap();
    let at = name_at + 10 + 4 + 4 + 4 + 2 + 12 + 36 + 4 + 12 + 4;
    assert_eq!(&dirty[at..at + 4], &[1, 0, 0, 0], "the shape's has-bounding-volume word");
    dirty[at + 2] = 0xC4;
    let rep = check_nif(&dirty);
    assert!(rep.read.is_ok(), "{rep}");
    assert!(rep.idempotent.is_ok(), "{rep}");
    assert!(rep.exact.as_ref().unwrap_err().contains("block 1 (NiTriShape)"), "{rep}");
    let _ = rep.clean();

    // A block type the model does not have: not read, with the block named.
    let mut odd = Stream::new();
    odd.push(NiNode::default());
    let mut bytes = odd.write().unwrap();
    let name_at = bytes.windows(6).position(|w| w == b"NiNode").unwrap();
    bytes[name_at..name_at + 6].copy_from_slice(b"NiNope");
    let rep = check_nif(&bytes);
    assert!(rep.read.as_ref().unwrap_err().contains("block 0 (NiNope"), "{rep}");
}

/// Blender's exporter writes the root last. 18gf refused such a file; now the engine reads
/// it through the typed model's `root_first`, and gets the same geometry it gets from the
/// same mesh with its root first. The game's own wall, with its blocks turned round.
#[test]
fn a_mesh_with_its_root_last_reads_as_the_same_mesh() {
    use gardenfell_nif::netimmerse::stream::{root_first, Stream};
    let orig = fs::read(repo().join("fixture/ex_stronghold_wall00.nif")).unwrap();
    let mut s = Stream::parse(&orig).unwrap();
    assert_eq!(s.roots[0].index, 0, "the fixture is a normal file");
    // Block 0 to the end, everything else one step down: a permutation, not a swap, so
    // every link in the file moves.
    let n = s.blocks.len();
    let mut order: Vec<usize> = (1..n).collect();
    order.push(0);
    assert!(s.reorder(&order));
    assert_eq!(s.roots[0].index, n as i32 - 1);
    let turned = s.write().unwrap();
    assert_ne!(turned, orig);

    // The typed model reads both the same, bar the order.
    assert!(check_nif(&turned).clean());
    let back = root_first(&turned).expect("there is a root to move");
    let mut again = Stream::parse(&back).unwrap();
    assert_eq!(again.roots[0].index, 0);
    assert!(!again.root_first());

    // And the engine's reader gets the wall out of both.
    let a = gardenfell_nif::nif::parse(&orig).expect("the original reads");
    let b = gardenfell_nif::nif::parse(&turned).expect("the turned file reads through the typed model");
    assert_eq!(a.tris.len(), b.tris.len());
    assert!(!a.tris.is_empty());
    assert_eq!(a.aabb_min, b.aabb_min);
    assert_eq!(a.aabb_max, b.aabb_max);
    let ta = gardenfell_nif::nif::record_types(&orig).unwrap();
    let tb = gardenfell_nif::nif::record_types(&turned).unwrap();
    assert_eq!(ta.len(), tb.len());
    // Drawn the same too: the same parts, with the same triangle counts and textures.
    let da = gardenfell_nif::nif::parse_draw(&orig).unwrap_or_default();
    let db = gardenfell_nif::nif::parse_draw(&turned).unwrap_or_default();
    assert_eq!(da.len(), db.len());
    for (x, y) in da.iter().zip(&db) {
        assert_eq!(x.idx.len(), y.idx.len());
        assert_eq!(x.tex, y.tex);
    }
}
