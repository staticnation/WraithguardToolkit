//! Round 18gf: the reader corrections of `backend-plan.md` part 2, each pinned against the
//! rule it came from. Plugins and archives are built as bytes here rather than from a
//! generator, because every case is a malformed or unusual record that no exporter
//! writes and `t_fixtures` would rightly refuse to call a fixture.

use gardenfell_bsa::bsa::Bsa;
use gardenfell_tes3::espwrite::{OutRef, Writer};
use gardenfell_tes3::land::{DEFAULT_HEIGHT, VERTS};
use gardenfell_nif::nif;
use gardenfell_tes3::tes3::raw;
use gardenfell_install::vfs::Vfs;
use gardenfell_world::world::{parse_plugin, LoadOpts, RawCell, World};
use std::path::PathBuf;

// ---- a plugin as bytes ---------------------------------------------------------------

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

/// The TES3 header: version 1.3, type 0, author, description, record count, then one
/// MAST/DATA pair per master.
fn header(masters: &[&str], records: u32) -> Vec<u8> {
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

fn stat(id: &str, mesh: &str, flags: u32) -> Vec<u8> {
    rec(b"STAT", flags, &[sub(b"NAME", &zstr(id)), sub(b"MODL", &zstr(mesh))])
}

fn cell_data(flags: u32, gx: i32, gy: i32) -> Vec<u8> {
    let mut d = flags.to_le_bytes().to_vec();
    d.extend_from_slice(&gx.to_le_bytes());
    d.extend_from_slice(&gy.to_le_bytes());
    sub(b"DATA", &d)
}

fn frmr(word: u32, id: &str, x: f32, y: f32) -> Vec<u8> {
    let mut data = Vec::new();
    for v in [x, y, 0.0, 0.0, 0.0, 0.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    [sub(b"FRMR", &word.to_le_bytes()), sub(b"NAME", &zstr(id)), sub(b"DATA", &data)].concat()
}

fn mvrf(word: u32, gx: i32, gy: i32) -> Vec<u8> {
    let mut g = gx.to_le_bytes().to_vec();
    g.extend_from_slice(&gy.to_le_bytes());
    [sub(b"MVRF", &word.to_le_bytes()), sub(b"CNDT", &g)].concat()
}

fn nam0(n: u32) -> Vec<u8> {
    sub(b"NAM0", &n.to_le_bytes())
}

/// Round 18gh: the cells of a plugin made of these records, read the way the world reads
/// them (through the typed model), standing alone in no load order.
fn cells(records: &[Vec<u8>]) -> Vec<RawCell> {
    let mut bytes = header(&[], records.len() as u32);
    for r in records {
        bytes.extend_from_slice(r);
    }
    parse_plugin(&bytes, 0, &[], false).cells
}

/// Whether a CELL record's NAM0 sits in its header block, before the first FRMR.
fn nam0_in_header(rec: &raw::Record) -> bool {
    let first_frmr = rec
        .subs
        .iter()
        .position(|s| &s.tag == b"FRMR")
        .unwrap_or(rec.subs.len());
    rec.subs
        .iter()
        .position(|s| &s.tag == b"NAM0")
        .is_some_and(|at| at < first_frmr)
}

/// The values of every NAM0 in a CELL record.
fn nam0s(rec: &raw::Record) -> Vec<u32> {
    rec.subs_tagged(b"NAM0")
        .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]))
        .collect()
}

fn exterior(gx: i32, gy: i32, body: &[Vec<u8>]) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", &zstr("")), cell_data(2, gx, gy)];
    subs.extend_from_slice(body);
    rec(b"CELL", 0, &subs)
}

fn interior(name: &str, body: &[Vec<u8>]) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", &zstr(name)), cell_data(1, 0, 0)];
    subs.extend_from_slice(body);
    rec(b"CELL", 0, &subs)
}

fn land(gx: i32, gy: i32, data_flags: Option<u32>, heights: Option<f32>, vtex: bool) -> Vec<u8> {
    let mut g = gx.to_le_bytes().to_vec();
    g.extend_from_slice(&gy.to_le_bytes());
    let mut subs = vec![sub(b"INTV", &g)];
    if let Some(f) = data_flags {
        subs.push(sub(b"DATA", &f.to_le_bytes()));
    }
    if let Some(h) = heights {
        // One base height and all-zero deltas: a flat cell at `h` after the ×8.
        let mut v = (h / 8.0).to_le_bytes().to_vec();
        v.extend_from_slice(&[0u8; VERTS * VERTS + 3]);
        subs.push(sub(b"VHGT", &v));
    }
    if vtex {
        subs.push(sub(b"VTEX", &[0u8; 512]));
    }
    rec(b"LAND", 0, &subs)
}

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("gf18gf-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn plugin(&self, name: &str, masters: &[&str], records: &[Vec<u8>]) -> PathBuf {
        let mut bytes = header(masters, records.len() as u32);
        for r in records {
            bytes.extend_from_slice(r);
        }
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
    fn load(&self, paths: &[PathBuf], opts: &LoadOpts) -> World {
        let vfs = Vfs::with_roots(&[("t".to_string(), self.0.clone())]);
        World::load_with(paths, &vfs, true, opts)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ids(w: &World, grid: (i32, i32)) -> Vec<String> {
    let mut v: Vec<String> =
        w.cells.get(&grid).map(|c| c.refs.values().map(|r| r.id.clone()).collect()).unwrap_or_default();
    v.sort();
    v
}

// ---- NAM0 is positional (items 2 and 3) ---------------------------------------------

#[test]
fn references_before_nam0_are_persistent_and_after_it_temporary() {
    let c = cells(&[rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("")),
            cell_data(2, 1, 1),
            frmr(1, "a", 0.0, 0.0),
            frmr(2, "b", 0.0, 0.0),
            nam0(1),
            frmr(3, "c", 0.0, 0.0),
        ],
    )]);
    assert!(!c[0].deleted);
    let temp: Vec<(String, bool)> = c[0]
        .refs
        .iter()
        .map(|r| (r.id.clone(), r.temporary))
        .collect();
    assert_eq!(
        temp,
        vec![("a".into(), false), ("b".into(), false), ("c".into(), true)]
    );
}

#[test]
fn a_nam0_in_the_header_block_makes_every_reference_temporary() {
    let c = cells(&[rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("")),
            cell_data(2, 1, 1),
            nam0(2),
            frmr(1, "a", 0.0, 0.0),
            frmr(2, "b", 0.0, 0.0),
        ],
    )]);
    assert_eq!(c[0].refs.len(), 2);
    assert!(c[0].refs.iter().all(|r| r.temporary), "{:?}", c[0].refs);
}

#[test]
fn a_dele_after_a_references_data_is_the_cells_not_the_references() {
    let c = cells(&[rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("Room")),
            cell_data(1, 0, 0),
            frmr(1, "a", 0.0, 0.0),
            sub(b"DELE", &[0u8; 4]),
        ],
    )]);
    assert!(
        c[0].deleted,
        "the DELE came after the reference ended at its DATA"
    );
    assert!(
        !c[0].refs[0].deleted,
        "and the reference itself is not deleted"
    );
    // Written before the references, as tes3 writes it, it is the header's.
    let c = cells(&[rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("Room")),
            cell_data(1, 0, 0),
            sub(b"DELE", &[0u8; 4]),
        ],
    )]);
    assert!(c[0].deleted);
    // And a reference's own DELE deletes the reference, not the room.
    let c = cells(&[rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("Room")),
            cell_data(1, 0, 0),
            [
                sub(b"FRMR", &1u32.to_le_bytes()),
                sub(b"NAME", &zstr("a")),
                sub(b"DELE", &[0u8; 4]),
            ]
            .concat(),
        ],
    )]);
    assert!(!c[0].deleted);
    assert!(c[0].refs[0].deleted);
}

#[test]
fn the_patcher_puts_the_grass_behind_the_persistent_references_and_counts_only_the_grass() {
    use gardenfell_tes3::esppatch::patch;
    // A room of three persistent references and no NAM0 at all.
    let source = {
        let mut b = header(&[], 1);
        b.extend_from_slice(&interior(
            "Room",
            &[frmr(1, "npc_a", 0.0, 0.0), frmr(2, "npc_b", 1.0, 0.0), frmr(3, "npc_c", 2.0, 0.0)],
        ));
        b
    };
    // Two blades of grass for the same room.
    let added = {
        let mut b = header(&[], 2);
        b.extend_from_slice(&stat("GRS_x", "grass\\x.nif", 0));
        b.extend_from_slice(&interior(
            "Room",
            &[nam0(2), frmr(1, "GRS_x", 5.0, 5.0), frmr(2, "GRS_x", 6.0, 6.0)],
        ));
        b
    };
    let (out, rep) = patch(&source, &added).expect("patched");
    assert_eq!(rep.refs, 2);
    // Read the patched room back the way tes3 would.
    let recs = raw::parse_plugin(&out).unwrap();
    let rooms = parse_plugin(&out, 0, &[], false).cells;
    let mut seen = false;
    for (r, room) in recs.iter().filter(|r| &r.tag == b"CELL").zip(&rooms) {
        assert!(
            !nam0_in_header(r),
            "NAM0 must not sit ahead of the persistent references"
        );
        let got: Vec<(String, bool)> = room
            .refs
            .iter()
            .map(|r| (r.id.clone(), r.temporary))
            .collect();
        assert_eq!(
            got,
            vec![
                ("npc_a".into(), false),
                ("npc_b".into(), false),
                ("npc_c".into(), false),
                ("GRS_x".into(), true),
                ("GRS_x".into(), true)
            ]
        );
        // And NAM0's value is the count of what follows it.
        assert_eq!(nam0s(r), vec![2], "one NAM0, not two");
        seen = true;
    }
    assert!(seen);
}

#[test]
fn the_patcher_grows_a_nam0_that_sits_among_the_references() {
    use gardenfell_tes3::esppatch::patch;
    // Two persistent references, then NAM0 and one temporary: the shape the CS writes for
    // a room with an NPC in it.
    let source = {
        let mut b = header(&[], 1);
        b.extend_from_slice(&interior(
            "Room",
            &[frmr(1, "npc_a", 0.0, 0.0), frmr(2, "door", 1.0, 0.0), nam0(1), frmr(3, "crate", 2.0, 0.0)],
        ));
        b
    };
    let added = {
        let mut b = header(&[], 2);
        b.extend_from_slice(&stat("GRS_x", "grass\\x.nif", 0));
        b.extend_from_slice(&interior("Room", &[nam0(1), frmr(1, "GRS_x", 5.0, 5.0)]));
        b
    };
    let (out, _) = patch(&source, &added).expect("patched");
    let recs = raw::parse_plugin(&out).unwrap();
    let r = recs.iter().find(|r| &r.tag == b"CELL").unwrap();
    assert_eq!(
        nam0s(r),
        vec![2],
        "one NAM0, grown from 1 to 2 where it sat"
    );
    let rooms = parse_plugin(&out, 0, &[], false).cells;
    let persistent: Vec<&str> = rooms[0]
        .refs
        .iter()
        .filter(|r| !r.temporary)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(persistent, vec!["npc_a", "door"]);
}

// ---- a mismatched MVRF (D2) ---------------------------------------------------------

#[test]
fn a_mismatched_mvrf_is_dropped_under_the_game_and_applied_under_openmw() {
    let d = Dir::new("mvrf");
    let a = d.plugin(
        "A.esm",
        &[],
        &[
            stat("rock", "f\\rock.nif", 0),
            exterior(1, 1, &[nam0(1), frmr(1, "rock", 8192.0 + 10.0, 8192.0 + 10.0)]),
        ],
    );
    // B moves "reference 2" but the FRMR that follows is reference 1: a malformed move.
    let b = d.plugin(
        "B.esp",
        &["A.esm"],
        &[exterior(
            1,
            1,
            &[
                nam0(1),
                [mvrf(0x0100_0002, 2, 1), frmr(0x0100_0001, "rock", 8192.0 + 10.0, 8192.0 + 10.0)].concat(),
            ],
        )],
    );
    let game = d.load(&[a.clone(), b.clone()], &LoadOpts::default());
    assert_eq!(ids(&game, (1, 1)), vec!["rock".to_string()], "the game keeps the reference where it was");
    assert!(ids(&game, (2, 1)).is_empty());
    assert_eq!(game.stats.refs_move_dropped, 1);
    let openmw = d.load(&[a, b], &LoadOpts { mvrf_strict: false, ..Default::default() });
    assert!(ids(&openmw, (1, 1)).is_empty(), "OpenMW applies the move to the reference that followed");
    assert_eq!(ids(&openmw, (2, 1)), vec!["rock".to_string()]);
    assert_eq!(openmw.stats.refs_move_dropped, 0);
}

#[test]
fn the_order_file_decides_the_move_rule() {
    assert!(!LoadOpts::for_order_file(std::path::Path::new("C:\\Games\\OpenMW\\openmw.cfg")).mvrf_strict);
    assert!(
        !LoadOpts::for_order_file(std::path::Path::new("/home/robin/.config/openmw/OpenMW.cfg")).mvrf_strict
    );
    assert!(
        LoadOpts::for_order_file(std::path::Path::new("C:\\Modding\\MO2\\profiles\\Default\\Morrowind.ini"))
            .mvrf_strict
    );
}

// ---- moved twice (item 5) -------------------------------------------------------------

#[test]
fn a_reference_moved_twice_stands_in_one_cell_only() {
    let d = Dir::new("twice");
    let at = |gx: i32, gy: i32| (gx as f32 * 8192.0 + 10.0, gy as f32 * 8192.0 + 10.0);
    let (x, y) = at(1, 1);
    let a = d.plugin(
        "A.esm",
        &[],
        &[stat("rock", "f\\rock.nif", 0), exterior(1, 1, &[nam0(1), frmr(1, "rock", x, y)])],
    );
    let b = d.plugin(
        "B.esp",
        &["A.esm"],
        &[exterior(
            1,
            1,
            &[nam0(1), [mvrf(0x0100_0001, 2, 1), frmr(0x0100_0001, "rock", at(2, 1).0, at(2, 1).1)].concat()],
        )],
    );
    let c = d.plugin(
        "C.esp",
        &["A.esm"],
        &[exterior(
            1,
            1,
            &[nam0(1), [mvrf(0x0100_0001, 3, 1), frmr(0x0100_0001, "rock", at(3, 1).0, at(3, 1).1)].concat()],
        )],
    );
    let w = d.load(&[a, b, c], &LoadOpts::default());
    assert!(ids(&w, (1, 1)).is_empty(), "left its first cell");
    assert!(ids(&w, (2, 1)).is_empty(), "and the cell the first move put it in: {:?}", ids(&w, (2, 1)));
    assert_eq!(ids(&w, (3, 1)), vec!["rock".to_string()]);
    assert_eq!(w.stats.refs_moved, 2);
    assert_eq!(w.stats.refs_relocated, 1);
}

#[test]
fn deleting_a_moved_reference_in_its_home_cell_takes_it_out_of_where_it_went() {
    let d = Dir::new("delmoved");
    let (x, y) = (8192.0 + 10.0, 8192.0 + 10.0);
    let a = d.plugin(
        "A.esm",
        &[],
        &[stat("rock", "f\\rock.nif", 0), exterior(1, 1, &[nam0(1), frmr(1, "rock", x, y)])],
    );
    let b = d.plugin(
        "B.esp",
        &["A.esm"],
        &[exterior(
            1,
            1,
            &[nam0(1), [mvrf(0x0100_0001, 2, 1), frmr(0x0100_0001, "rock", x + 8192.0, y)].concat()],
        )],
    );
    let c = d.plugin(
        "C.esp",
        &["A.esm"],
        &[exterior(
            1,
            1,
            &[
                nam0(1),
                [
                    sub(b"FRMR", &0x0100_0001u32.to_le_bytes()),
                    sub(b"NAME", &zstr("rock")),
                    sub(b"DELE", &[0u8; 4]),
                ]
                .concat(),
            ],
        )],
    );
    let w = d.load(&[a, b, c], &LoadOpts::default());
    assert!(ids(&w, (2, 1)).is_empty(), "deleted in its home cell, gone from the cell it was moved to");
    assert_eq!(w.stats.refs_relocated, 1);
}

// ---- LAND without heights, and the DATA flags (items 4 and 9) ---------------------------

fn height_at(w: &World, grid: (i32, i32)) -> f32 {
    w.lands.get(&grid).map(|l| l.heights[VERTS * 32 + 32]).expect("a landscape")
}

#[test]
fn a_later_land_without_heights_replaces_the_ground_with_flat_default() {
    let d = Dir::new("land");
    let a = d.plugin("A.esm", &[], &[land(5, 5, Some(0x1 | 0x4), Some(400.0), true)]);
    let b = d.plugin("B.esp", &["A.esm"], &[land(5, 5, Some(0x4), None, true)]);
    let only_a = d.load(&[a.clone()], &LoadOpts::default());
    assert!((height_at(&only_a, (5, 5)) - 400.0).abs() < 1e-3);
    let w = d.load(&[a, b], &LoadOpts::default());
    assert!(
        (height_at(&w, (5, 5)) - DEFAULT_HEIGHT).abs() < 1e-3,
        "flat default ground, as both engines show it"
    );
    assert_eq!(w.stats.lands_defaulted, 1);
    assert_eq!(w.lands.get(&(5, 5)).unwrap().plugin, 1, "the later plugin's record");
}

#[test]
fn a_land_block_the_data_flags_switch_off_is_not_read() {
    let d = Dir::new("landflags");
    // VHGT is in the file but the flags say "textures only": the game ignores the heights.
    let a = d.plugin("A.esm", &[], &[land(5, 5, Some(0x4), Some(400.0), true)]);
    let w = d.load(&[a], &LoadOpts::default());
    assert!((height_at(&w, (5, 5)) - DEFAULT_HEIGHT).abs() < 1e-3);
    assert_eq!(w.stats.lands_flagged_out, 1);
    assert_eq!(w.stats.lands_defaulted, 1);
    // No DATA at all: everything present counts, which is how every fixture reads.
    let d2 = Dir::new("landnodata");
    let a2 = d2.plugin("A.esm", &[], &[land(5, 5, None, Some(400.0), true)]);
    let w2 = d2.load(&[a2], &LoadOpts::default());
    assert!((height_at(&w2, (5, 5)) - 400.0).abs() < 1e-3);
    assert_eq!(w2.stats.lands_flagged_out, 0);
}

// ---- deletion by header flag, and a deleted cell (items 8 and 10) -----------------------

#[test]
fn a_record_deleted_by_its_header_flag_alone_is_deleted() {
    let d = Dir::new("flag");
    let a = d.plugin("A.esm", &[], &[stat("rock", "f\\rock.nif", 0), stat("tree", "f\\tree.nif", 0)]);
    let b = d.plugin("B.esp", &["A.esm"], &[stat("tree", "f\\tree.nif", 0x20)]);
    let w = d.load(&[a, b], &LoadOpts::default());
    assert!(w.models.contains_key("rock"));
    assert!(!w.models.contains_key("tree"), "deleted by the 0x20 bit with no DELE, as tes3 reads it");
    assert!(!w.objects.contains_key("tree"));
    assert!(w.stats.records_deleted_by_flag >= 1);
}

#[test]
fn a_deleted_interior_leaves_the_world() {
    let d = Dir::new("delroom");
    let a = d.plugin(
        "A.esm",
        &[],
        &[stat("crate", "f\\crate.nif", 0), interior("Room", &[nam0(1), frmr(1, "crate", 1.0, 1.0)])],
    );
    let b = d.plugin(
        "B.esp",
        &["A.esm"],
        &[rec(b"CELL", 0, &[sub(b"NAME", &zstr("Room")), cell_data(1, 0, 0), sub(b"DELE", &[0u8; 4])])],
    );
    let only_a = d.load(&[a.clone()], &LoadOpts::default());
    assert!(only_a.interiors.contains_key("room"));
    let w = d.load(&[a, b], &LoadOpts::default());
    assert!(!w.interiors.contains_key("room"), "{:?}", w.interiors.keys().collect::<Vec<_>>());
    assert_eq!(w.stats.cells_deleted, 1);
}

#[test]
fn a_deleted_exterior_record_forgets_its_references_but_keeps_the_land() {
    let d = Dir::new("delext");
    let (x, y) = (8192.0 + 10.0, 8192.0 + 10.0);
    let a = d.plugin(
        "A.esm",
        &[],
        &[
            stat("rock", "f\\rock.nif", 0),
            land(1, 1, None, Some(100.0), false),
            exterior(1, 1, &[nam0(1), frmr(1, "rock", x, y)]),
        ],
    );
    let b =
        d.plugin("B.esp", &["A.esm"], &[rec(b"CELL", 0x20, &[sub(b"NAME", &zstr("")), cell_data(2, 1, 1)])]);
    let w = d.load(&[a, b], &LoadOpts::default());
    assert!(ids(&w, (1, 1)).is_empty());
    assert!(w.lands.contains_key(&(1, 1)), "the landscape is its own record");
    assert_eq!(w.stats.cells_deleted, 1);
}

// ---- the Objects plugin's cells (items 6 and 7) ------------------------------------------

#[test]
fn the_writer_puts_persistent_references_before_nam0_and_clamps_the_scale() {
    let mut w = Writer::for_game(256);
    let refs = [
        OutRef { id: "GRS_a", pos: [1.0; 3], rot: [0.0; 3], scale: 3.0, persistent: false },
        OutRef { id: "rat", pos: [2.0; 3], rot: [0.0; 3], scale: 0.1, persistent: true },
        OutRef { id: "GRS_b", pos: [3.0; 3], rot: [0.0; 3], scale: 1.0, persistent: false },
    ];
    w.cell_flagged("", 1, 1, 2, &refs, 1);
    let out = w.out;
    let recs = raw::parse_plugin(&out).unwrap();
    let c = &parse_plugin(&out, 0, &[], false).cells[0];
    assert_eq!(c.flags, 2, "the flags the load order gave the cell");
    assert!(
        !nam0_in_header(&recs[0]),
        "NAM0 sits after the persistent reference, not in the header block"
    );
    let got: Vec<(String, bool, f32)> = c
        .refs
        .iter()
        .map(|r| (r.id.clone(), r.temporary, r.scale))
        .collect();
    assert_eq!(
        got,
        vec![("rat".into(), false, 0.5), ("GRS_a".into(), true, 2.0), ("GRS_b".into(), true, 1.0)]
    );
    let nam0s = nam0s(&recs[0]);
    assert_eq!(nam0s, vec![2], "two temporaries follow it");
}

#[test]
fn a_grass_cell_is_written_exactly_as_before() {
    // All temporary, flags zero, and the scale as scattered even below 0.5: byte for byte
    // the 18ge shape, which is what keeps every grass plugin digest the same.
    let mut w = Writer::with_capacity(256);
    let refs = [OutRef { id: "GRS_a", pos: [1.0; 3], rot: [0.0; 3], scale: 0.42, persistent: false }];
    w.cell("", 3, 4, &refs, 7);
    let recs = raw::parse_plugin(&w.out).unwrap();
    let tags: Vec<String> = recs[0]
        .subs
        .iter()
        .map(|s| String::from_utf8_lossy(&s.tag).into_owned())
        .collect();
    assert_eq!(
        tags,
        vec!["NAME", "DATA", "NAM0", "FRMR", "NAME", "XSCL", "DATA"]
    );
    let c = &parse_plugin(&w.out, 0, &[], false).cells[0];
    assert_eq!(c.flags, 0);
    assert!(nam0_in_header(&recs[0]));
    let refs = &c.refs;
    assert!(
        (refs[0].scale - 0.42).abs() < 1e-6,
        "a grass author's small blade is not clamped"
    );
}

// ---- the archive's names (item 1) ---------------------------------------------------------

#[test]
fn archive_names_decode_like_record_paths() {
    // One file, named with a 0xE9 byte, as a cp1252 "é": meshes\café.nif.
    let name: Vec<u8> = [b"meshes\\caf".as_slice(), &[0xE9], b".nif\0"].concat();
    let data = b"NIF?".to_vec();
    let mut bsa = Vec::new();
    bsa.extend_from_slice(&0x100u32.to_le_bytes());
    // hash_offset: records (8) + name offsets (4) + names, measured from byte 12.
    let hash_off = 8 + 4 + name.len();
    bsa.extend_from_slice(&(hash_off as u32).to_le_bytes());
    bsa.extend_from_slice(&1u32.to_le_bytes());
    bsa.extend_from_slice(&(data.len() as u32).to_le_bytes()); // size
    bsa.extend_from_slice(&0u32.to_le_bytes()); // offset into the data block
    bsa.extend_from_slice(&0u32.to_le_bytes()); // name offset
    bsa.extend_from_slice(&name);
    bsa.extend_from_slice(&[0u8; 8]); // the hash, unread
    bsa.extend_from_slice(&data);
    let a = Bsa::parse(std::path::Path::new("t.bsa"), bsa).expect("an archive");
    let key = gardenfell_bytes::paths::norm(&gardenfell_tes3::esp::cstring(b"Meshes\\Caf\xE9.nif"));
    assert!(a.has(&key), "the record's spelling of the name finds the archive's: {:?}", a.names());
}

// ---- NIF (items 12 to 14) ------------------------------------------------------------------

fn nif_header(count: u32) -> Vec<u8> {
    let mut v = b"NetImmerse File Format, Version 4.0.0.2\n".to_vec();
    v.extend_from_slice(&0x0400_0002u32.to_le_bytes());
    v.extend_from_slice(&count.to_le_bytes());
    v
}

fn nif_record(ty: &str, body: &[u8]) -> Vec<u8> {
    let mut v = (ty.len() as u32).to_le_bytes().to_vec();
    v.extend_from_slice(ty.as_bytes());
    v.extend_from_slice(body);
    v
}

/// A NiNode body: name, extra link, controller link, flags, translation, rotation, scale,
/// velocity, no properties, the bounding volume given, then children and effects.
fn ninode(name: &str, bound: &[u8], children: &[i32]) -> Vec<u8> {
    let mut v = (name.len() as u32).to_le_bytes().to_vec();
    v.extend_from_slice(name.as_bytes());
    v.extend_from_slice(&(-1i32).to_le_bytes());
    v.extend_from_slice(&(-1i32).to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes());
    for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0] {
        v.extend_from_slice(&f.to_le_bytes());
    }
    v.extend_from_slice(&0u32.to_le_bytes()); // properties
    if bound.is_empty() {
        v.extend_from_slice(&0u32.to_le_bytes());
    } else {
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(bound);
    }
    v.extend_from_slice(&(children.len() as u32).to_le_bytes());
    for c in children {
        v.extend_from_slice(&c.to_le_bytes());
    }
    v.extend_from_slice(&0u32.to_le_bytes()); // effects
    v
}

/// A NiTriShape with one triangle, so the draw walk has something to count.
fn tri_shape(data: i32) -> Vec<u8> {
    let mut v = ninode("shape", &[], &[]);
    // A NiTriShape is NiAVObject + data link + skin link; drop the children/effects counts
    // a node has and add the geometry's two links.
    v.truncate(v.len() - 8);
    v.extend_from_slice(&data.to_le_bytes());
    v.extend_from_slice(&(-1i32).to_le_bytes());
    v
}

fn tri_shape_data() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&3u16.to_le_bytes()); // vertices
    v.extend_from_slice(&1u32.to_le_bytes()); // has vertices
    for f in [0.0f32, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0] {
        v.extend_from_slice(&f.to_le_bytes());
    }
    v.extend_from_slice(&0u32.to_le_bytes()); // has normals
    for f in [3.3f32, 3.3, 0.0, 7.0] {
        v.extend_from_slice(&f.to_le_bytes()); // centre, radius
    }
    v.extend_from_slice(&0u32.to_le_bytes()); // has colours
    v.extend_from_slice(&0u16.to_le_bytes()); // uv sets
    v.extend_from_slice(&0u32.to_le_bytes()); // has uv
    v.extend_from_slice(&1u16.to_le_bytes()); // triangles
    v.extend_from_slice(&3u32.to_le_bytes()); // triangle points
    for i in [0u16, 1, 2] {
        v.extend_from_slice(&i.to_le_bytes());
    }
    v.extend_from_slice(&0u16.to_le_bytes()); // match groups
    v
}

fn footer(root: i32) -> Vec<u8> {
    let mut v = 1u32.to_le_bytes().to_vec();
    v.extend_from_slice(&root.to_le_bytes());
    v
}

fn whole(records: &[Vec<u8>], root: i32) -> Vec<u8> {
    let mut v = nif_header(records.len() as u32);
    for r in records {
        v.extend_from_slice(r);
    }
    v.extend_from_slice(&footer(root));
    v
}

#[test]
fn a_sphere_bounding_volume_is_walked_at_its_own_size() {
    /* Sphere: type 0, centre, radius = 20 bytes. The old reader skipped 64 and lost the
       shape. Round 18gg: this test first said the sphere was type 1 and the box type 0,
       the same way round as the reader it was pinning, and both were wrong (nif.xml,
       `tes3` enums.rs: SPHERE 0, BOX 1). A test built from the same belief as the code
       pins the belief, not the format; `round18gg::every_mesh_in_the_corpus_reads_the_same_with_its_root_last`
       is the one built from the files. */
    let mut sphere = 0i32.to_le_bytes().to_vec();
    for f in [0.0f32, 0.0, 0.0, 50.0] {
        sphere.extend_from_slice(&f.to_le_bytes());
    }
    let file = whole(
        &[
            nif_record("NiNode", &ninode("root", &sphere, &[1])),
            nif_record("NiTriShape", &tri_shape(2)),
            nif_record("NiTriShapeData", &tri_shape_data()),
        ],
        0,
    );
    let parts = nif::parse_draw(&file).expect("a mesh");
    assert_eq!(parts.len(), 1, "the shape under a sphere-bounded root is drawn");
    // And a box (type 1) still is.
    let mut bx = 1i32.to_le_bytes().to_vec();
    for f in [0.0f32; 15] {
        bx.extend_from_slice(&f.to_le_bytes());
    }
    let file = whole(
        &[
            nif_record("NiNode", &ninode("root", &bx, &[1])),
            nif_record("NiTriShape", &tri_shape(2)),
            nif_record("NiTriShapeData", &tri_shape_data()),
        ],
        0,
    );
    assert_eq!(nif::parse_draw(&file).expect("a mesh").len(), 1);
}

#[test]
fn a_file_that_is_not_netimmerse_4_0_0_2_is_refused() {
    let good = whole(
        &[
            nif_record("NiNode", &ninode("root", &[], &[1])),
            nif_record("NiTriShape", &tri_shape(2)),
            nif_record("NiTriShapeData", &tri_shape_data()),
        ],
        0,
    );
    assert!(nif::parse(&good).is_some());
    let mut other_header = good.clone();
    other_header[36] = b'1'; // "4.0.0.2" -> "4.0.1.2"
    assert!(nif::parse(&other_header).is_none(), "the header line is checked");
    let mut other_version = good.clone();
    let at = b"NetImmerse File Format, Version 4.0.0.2\n".len();
    other_version[at..at + 4].copy_from_slice(&0x0401_0000u32.to_le_bytes());
    assert!(nif::parse(&other_version).is_none(), "the version word is checked");
}

/// 18gf refused a file whose first root is not record 0, since every walk here starts at
/// record 0. 18gg: Robin's install has 232 such files (Blender's exporter writes the root
/// last), so the engine now reads them through the typed model's `root_first`, and the
/// mesh is the same mesh as the one with its root first. `round18gg.rs` does this with
/// the game's own wall; this is the three-record case the refusal was pinned on.
#[test]
fn a_file_whose_first_root_is_not_record_zero_is_read_through_the_typed_model() {
    let turned = [
        nif_record("NiTriShapeData", &tri_shape_data()),
        nif_record("NiNode", &ninode("root", &[], &[2])),
        nif_record("NiTriShape", &tri_shape(0)),
    ];
    let plain = [
        nif_record("NiNode", &ninode("root", &[], &[1])),
        nif_record("NiTriShape", &tri_shape(2)),
        nif_record("NiTriShapeData", &tri_shape_data()),
    ];
    let a = nif::parse(&whole(&turned, 1)).expect("the root-last file reads");
    let b = nif::parse(&whole(&plain, 0)).expect("the root-first file reads");
    assert_eq!(a.tris.len(), b.tris.len());
    assert_eq!(a.aabb_min, b.aabb_min);
    assert_eq!(a.aabb_max, b.aabb_max);
}

#[test]
fn the_euler_axis_order_is_honoured() {
    let e = [0.3f32, 0.7, 1.1];
    let xyz = nif::euler_mat3_ordered(e, 0);
    // XYZ: x first, then y, then z, i.e. R(z)·R(y)·R(x) - what every file was read as.
    let rx = [1., 0., 0., 0., e[0].cos(), -e[0].sin(), 0., e[0].sin(), e[0].cos()];
    let ry = [e[1].cos(), 0., e[1].sin(), 0., 1., 0., -e[1].sin(), 0., e[1].cos()];
    let rz = [e[2].cos(), -e[2].sin(), 0., e[2].sin(), e[2].cos(), 0., 0., 0., 1.];
    let mul = |a: &[f32; 9], b: &[f32; 9]| -> [f32; 9] {
        let mut o = [0f32; 9];
        for r in 0..3 {
            for c in 0..3 {
                o[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
            }
        }
        o
    };
    let want = mul(&mul(&rz, &ry), &rx);
    for i in 0..9 {
        assert!((xyz[i] - want[i]).abs() < 1e-6, "{i}: {} vs {}", xyz[i], want[i]);
    }
    // ZYX (5): the first channel turns about z, the last about x: R(x)·R(y)·R(z).
    let zyx = nif::euler_mat3_ordered(e, 5);
    let (a, b, c) = (e[0], e[1], e[2]);
    let rz1 = [a.cos(), -a.sin(), 0., a.sin(), a.cos(), 0., 0., 0., 1.];
    let ry1 = [b.cos(), 0., b.sin(), 0., 1., 0., -b.sin(), 0., b.cos()];
    let rx1 = [1., 0., 0., 0., c.cos(), -c.sin(), 0., c.sin(), c.cos()];
    let want = mul(&mul(&rx1, &ry1), &rz1);
    for i in 0..9 {
        assert!((zyx[i] - want[i]).abs() < 1e-6, "{i}: {} vs {}", zyx[i], want[i]);
    }
    assert!((0..9).any(|i| (zyx[i] - xyz[i]).abs() > 1e-3), "the two orders are different rotations");
}
