//! Round 18gn, backend-plan part 2 item 11 (esp-format §9.4): every physical object shares
//! one id namespace. A later plugin's record replaces an earlier one of another kind, type
//! and all, so a STAT `x` followed by a CREA `x` leaves one creature and no static, a LIGH
//! turned STAT stops being a light, and a deletion takes the id out whichever kind the
//! deleting record was. The world keeps its three tables (`models`, `lights`, `actors`),
//! which everything downstream is keyed by; what changes is that an id is in at most one.

use gardenfell_install::vfs::Vfs;
use gardenfell_world::world::{LoadOpts, World};
use std::path::PathBuf;

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

fn stat(id: &str, mesh: &str) -> Vec<u8> {
    rec(b"STAT", 0, &[sub(b"NAME", &zstr(id)), sub(b"MODL", &zstr(mesh))])
}

fn deleted(tag: &[u8; 4], id: &str) -> Vec<u8> {
    rec(tag, 0, &[sub(b"NAME", &zstr(id)), sub(b"DELE", &[0u8; 4])])
}

fn crea(id: &str, mesh: &str) -> Vec<u8> {
    rec(
        b"CREA",
        0,
        &[
            sub(b"NAME", &zstr(id)),
            sub(b"MODL", &zstr(mesh)),
            sub(b"FNAM", &zstr(id)),
            sub(b"NPDT", &[0u8; 96]),
        ],
    )
}

fn ligh(id: &str, mesh: &str, radius: i32) -> Vec<u8> {
    // LHDT: weight f32, value i32, time i32, radius i32, colour 4 bytes, flags i32.
    let mut lhdt = Vec::new();
    lhdt.extend_from_slice(&0f32.to_le_bytes());
    lhdt.extend_from_slice(&0i32.to_le_bytes());
    lhdt.extend_from_slice(&0i32.to_le_bytes());
    lhdt.extend_from_slice(&radius.to_le_bytes());
    lhdt.extend_from_slice(&[255, 200, 100, 0]);
    lhdt.extend_from_slice(&0i32.to_le_bytes());
    rec(b"LIGH", 0, &[sub(b"NAME", &zstr(id)), sub(b"MODL", &zstr(mesh)), sub(b"LHDT", &lhdt)])
}

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("gf18gn-{}-{}", tag, std::process::id()));
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
    fn load(&self, paths: &[PathBuf]) -> World {
        let vfs = Vfs::with_roots(&[("t".to_string(), self.0.clone())]);
        World::load_with(paths, &vfs, true, &LoadOpts::default())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Which tables hold an id, as a short word, so a failure reads at a glance.
fn where_is(w: &World, id: &str) -> String {
    let mut s = String::new();
    if w.models.contains_key(id) {
        s.push_str("model ");
    }
    if w.lights.contains_key(id) {
        s.push_str("light ");
    }
    if w.actors.contains_key(id) {
        s.push_str("actor ");
    }
    s.trim_end().to_string()
}

#[test]
fn a_static_turned_creature_by_a_later_plugin_is_one_creature() {
    let d = Dir::new("stat2crea");
    let a = d.plugin("A.esm", &[], &[stat("x", "f\\rock.nif"), stat("rock", "f\\rock.nif")]);
    let b = d.plugin("B.esp", &["A.esm"], &[crea("x", "r\\rat.nif")]);
    let w = d.load(&[a, b]);
    assert_eq!(where_is(&w, "x"), "actor");
    assert_eq!(where_is(&w, "rock"), "model", "an id nobody redefined is untouched");
    assert_eq!(w.objects.get("x").and_then(|o| o.kind), Some(gardenfell_tes3::records::ObjKind::Creature));
    assert_eq!(w.stats.ids_rekinded, 1);
}

#[test]
fn a_creature_turned_static_by_a_later_plugin_is_one_static() {
    let d = Dir::new("crea2stat");
    let a = d.plugin("A.esm", &[], &[crea("x", "r\\rat.nif")]);
    let b = d.plugin("B.esp", &["A.esm"], &[stat("x", "f\\rock.nif")]);
    let w = d.load(&[a, b]);
    assert_eq!(where_is(&w, "x"), "model");
    assert_eq!(w.models.get("x").map(String::as_str), Some("f\\rock.nif"));
    assert_eq!(w.stats.ids_rekinded, 1);
}

#[test]
fn a_light_turned_static_stops_being_a_light_and_a_static_turned_light_starts() {
    let d = Dir::new("ligh2stat");
    let a = d.plugin("A.esm", &[], &[ligh("lamp", "l\\lamp.nif", 256), stat("post", "f\\post.nif")]);
    let b = d.plugin(
        "B.esp",
        &["A.esm"],
        &[stat("lamp", "f\\lamp_off.nif"), ligh("post", "l\\post_lit.nif", 128)],
    );
    let only_a = d.load(std::slice::from_ref(&a));
    assert_eq!(
        where_is(&only_a, "lamp"),
        "model light",
        "a LIGH sits in both tables: one record, mesh and light"
    );
    assert_eq!(where_is(&only_a, "post"), "model");
    let w = d.load(&[a, b]);
    assert_eq!(where_is(&w, "lamp"), "model", "the later STAT took the light away");
    assert_eq!(w.models.get("lamp").map(String::as_str), Some("f\\lamp_off.nif"));
    assert_eq!(where_is(&w, "post"), "model light");
    assert_eq!(w.lights.get("post").map(|l| l.radius), Some(128));
    // Only the lamp's light counted: the post gained a light without losing an entry.
    assert_eq!(w.stats.ids_rekinded, 1);
}

#[test]
fn a_deletion_takes_the_id_out_whichever_kind_the_deleting_record_is() {
    let d = Dir::new("delkind");
    let a = d.plugin("A.esm", &[], &[crea("x", "r\\rat.nif"), ligh("lamp", "l\\lamp.nif", 256)]);
    // A deleted STAT record for an id that is a creature, and a deleted CREA for a light:
    // the game's table is by id alone, so both go.
    let b = d.plugin("B.esp", &["A.esm"], &[deleted(b"STAT", "x"), deleted(b"CREA", "lamp")]);
    let w = d.load(&[a, b]);
    assert_eq!(where_is(&w, "x"), "", "{}", where_is(&w, "x"));
    assert_eq!(where_is(&w, "lamp"), "", "{}", where_is(&w, "lamp"));
}

#[test]
fn a_reference_follows_the_ids_latest_kind() {
    let d = Dir::new("refkind");
    // A cell placing `x`; A says static, B says creature. The static's reference must
    // not be drawn as a rock once B has made `x` a rat.
    let cell = rec(
        b"CELL",
        0,
        &[
            sub(b"NAME", &zstr("")),
            sub(
                b"DATA",
                &[&2u32.to_le_bytes()[..], &0i32.to_le_bytes()[..], &0i32.to_le_bytes()[..]].concat(),
            ),
            sub(b"FRMR", &1u32.to_le_bytes()),
            sub(b"NAME", &zstr("x")),
            sub(b"DATA", &[0u8; 24]),
        ],
    );
    let a = d.plugin("A.esm", &[], &[stat("x", "f\\rock.nif"), cell]);
    let b = d.plugin("B.esp", &["A.esm"], &[crea("x", "r\\rat.nif")]);
    let w = d.load(&[a, b]);
    assert_eq!(where_is(&w, "x"), "actor");
    assert!(w.actors.get("x").is_some_and(|a| a.creature));
}
