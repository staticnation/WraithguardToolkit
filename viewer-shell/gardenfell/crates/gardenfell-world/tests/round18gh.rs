//! Round 18gh: the engine's plugin views read through the typed model (`gardenfell_tes3::tes3`).
//! What the byte reader used to decide on its own is decided by `tes3::types` now, and the
//! views on top (`world::parse_plugin`, `esppatch`, `records`) keep every value they had.

use gardenfell_tes3::tes3::raw;
use gardenfell_tes3::tes3::types::{Cell, Typed};
use gardenfell_world::world::parse_plugin;

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

fn cell_data(flags: u32, gx: i32, gy: i32) -> Vec<u8> {
    let mut d = flags.to_le_bytes().to_vec();
    d.extend_from_slice(&gx.to_le_bytes());
    d.extend_from_slice(&gy.to_le_bytes());
    sub(b"DATA", &d)
}

fn frmr(word: u32, id: &str) -> Vec<u8> {
    let data: Vec<u8> = [0f32; 6].iter().flat_map(|v| v.to_le_bytes()).collect();
    [sub(b"FRMR", &word.to_le_bytes()), sub(b"NAME", &zstr(id)), sub(b"DATA", &data)].concat()
}

fn mvrf(word: u32, gx: i32, gy: i32) -> Vec<u8> {
    let mut g = gx.to_le_bytes().to_vec();
    g.extend_from_slice(&gy.to_le_bytes());
    [sub(b"MVRF", &word.to_le_bytes()), sub(b"CNDT", &g)].concat()
}

/// A plugin of these records with an empty TES3 header in front.
fn plugin(records: &[Vec<u8>]) -> Vec<u8> {
    let mut v = rec(b"TES3", 0, &[]);
    for r in records {
        v.extend_from_slice(r);
    }
    v
}

/// Backend-plan D2: a cell whose MVRF names another reference than the FRMR after it
/// round-trips byte for byte through `types::Cell`, and the view says `moved_mismatch`
/// with the move kept for the install's rule to judge.
#[test]
fn a_mismatched_mvrf_round_trips_through_the_typed_cell_and_sets_moved_mismatch() {
    let cell = rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("")),
            cell_data(2, 1, 1),
            sub(b"NAM0", &2u32.to_le_bytes()),
            [mvrf(0x0100_0002, 2, 1), frmr(0x0100_0001, "rock")].concat(),
            [mvrf(0x0100_0007, 3, 1), frmr(0x0100_0007, "tree")].concat(),
        ],
    );
    let recs = raw::parse_plugin(&cell).unwrap();
    let typed = Cell::from_raw(&recs[0]).expect("the cell types");
    assert_eq!(typed.references[0].moved_index, Some((1, 2)));
    assert_eq!(typed.references[0].moved_cell, Some((2, 1)));
    assert_eq!(typed.references[1].moved_index, None);
    assert_eq!(typed.references[1].moved_cell, Some((3, 1)));
    let back = typed.to_raw().unwrap();
    assert_eq!(back, recs[0], "the record as it was read");
    assert_eq!(raw::write_plugin(&[back]), cell, "byte for byte");

    let d = parse_plugin(&plugin(&[cell]), 1, &[0], false);
    assert_eq!(d.records_refused, 0);
    let refs = &d.cells[0].refs;
    assert_eq!(refs.len(), 2);
    assert!(refs[0].moved_mismatch, "the MVRF named reference 2, the FRMR is reference 1");
    assert_eq!(refs[0].moved_to, Some((2, 1)), "the move is kept; `LoadOpts::mvrf_strict` decides");
    assert_eq!(refs[0].num.content_file, 0, "the master the FRMR named");
    assert_eq!(refs[0].num.index, 1);
    assert!(!refs[1].moved_mismatch);
    assert_eq!(refs[1].moved_to, Some((3, 1)));
    assert!(refs.iter().all(|r| r.temporary), "after the cell's NAM0");
}

/// A record the layout refuses is skipped and counted, never guessed past; the records
/// around it read as before.
#[test]
fn a_record_that_does_not_fit_its_layout_is_refused_and_counted() {
    let good = rec(b"STAT", 0, &[sub(b"NAME", &zstr("rock")), sub(b"MODL", &zstr("f\\rock.nif"))]);
    let odd = rec(b"STAT", 0, &[sub(b"NAME", &zstr("tree")), sub(b"ZZZZ", &[1, 2, 3])]);
    let short = rec(b"CELL", 0, &[sub(b"NAME", &zstr("Room")), sub(b"DATA", &[1, 0, 0, 0])]);
    let d = parse_plugin(&plugin(&[good, odd, short]), 0, &[], false);
    assert_eq!(d.records, 4);
    assert_eq!(d.records_refused, 2, "the STAT with a stray subrecord and the CELL with a short DATA");
    assert_eq!(d.models.len(), 1);
    assert_eq!(d.models[0].id, "rock");
    assert!(d.cells.is_empty());
    assert!(d.unreadable.is_none());
    // The ids index still saw every record that has a name: it reads the raw record.
    assert_eq!(d.ids.len(), 2);

    // A file the raw layer cannot read is unreadable, whole, with the reason.
    let mut truncated = plugin(&[rec(b"STAT", 0, &[sub(b"NAME", &zstr("rock"))])]);
    truncated.truncate(truncated.len() - 3);
    let d = parse_plugin(&truncated, 0, &[], false);
    assert_eq!(d.records, 0);
    assert!(d.unreadable.as_deref().is_some_and(|e| e.contains("STAT")), "{:?}", d.unreadable);
}

/// The two places the typed cell does not answer on its own, read off the raw record as
/// the byte reader read them: a cell without DATA is not a cell, and an exterior's INTV is
/// not water.
#[test]
fn a_cell_without_data_is_skipped_and_an_exteriors_intv_is_not_water() {
    let no_data = rec(b"CELL", 0, &[sub(b"NAME", &zstr("Nowhere"))]);
    let ext_intv = rec(
        b"CELL",
        0,
        &[sub(b"NAME", &zstr("")), cell_data(2, 4, 4), sub(b"INTV", &(-100i32).to_le_bytes())],
    );
    let int_intv = rec(
        b"CELL",
        0,
        &[sub(b"NAME", &zstr("Pool")), cell_data(3, 0, 0), sub(b"INTV", &(-760i32).to_le_bytes())],
    );
    let int_whgt = rec(
        b"CELL",
        0,
        &[sub(b"NAME", &zstr("Bath")), cell_data(3, 0, 0), sub(b"WHGT", &(-12.5f32).to_le_bytes())],
    );
    let d = parse_plugin(&plugin(&[no_data, ext_intv, int_intv, int_whgt]), 0, &[], false);
    assert_eq!(d.records_refused, 0);
    assert_eq!(d.cells.len(), 3, "the cell without DATA is not a cell");
    assert_eq!(d.cells[0].water, None, "an exterior carrying INTV is not a room with water");
    assert_eq!(d.cells[1].water, Some(-760.0), "Addamasartus is INTV -760");
    assert_eq!(d.cells[2].water, Some(-12.5));
}
