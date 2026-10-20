//! The record cache (editcore::cache), case for case with tests/test_record_cache.py where
//! the rule is the cache's own (the Python tests run it again through the native module).

use std::path::{Path, PathBuf};

use editcore::cache::{Cache, REPORTS_KEPT, UseRow};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("wg-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn plugin(&self, name: &str, body: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }
    fn db(&self) -> PathBuf {
        self.0.join("c.sqlite3")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cache(db: &Path) -> Cache {
    Cache::new(db.to_path_buf(), "", editcore::cache::MAX_BYTES)
}

fn keys(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_body_survives_a_round_trip_and_a_changed_plugin_is_a_miss() {
    let d = Dir::new("trip");
    let a = d.plugin("a.esm", &[b'x'; 10]);
    let c = cache(&d.db());
    assert!(c.get(&a, "CELL").is_none());
    c.put(&a, "CELL", b"[1,2,3]", None);
    assert_eq!(c.get(&a, "CELL").as_deref(), Some(&b"[1,2,3]"[..]));
    drop(c);
    assert_eq!(cache(&d.db()).get(&a, "CELL").as_deref(), Some(&b"[1,2,3]"[..]), "across runs");
    std::fs::write(&a, [b'x'; 11]).unwrap();
    assert!(cache(&d.db()).get(&a, "CELL").is_none(), "another size");
}

#[test]
fn a_broken_database_is_a_miss_not_an_error() {
    let d = Dir::new("broken");
    std::fs::write(d.db(), b"this is not a database".repeat(50)).unwrap();
    let a = d.plugin("a.esm", b"x");
    let c = cache(&d.db());
    assert!(c.get(&a, "CELL").is_none());
    c.put(&a, "CELL", b"[]", None);
    assert!(c.holders(&[a], "CELL", "x").is_none());
}

#[test]
fn the_key_index_answers_only_for_plugins_indexed_as_they_are() {
    let d = Dir::new("keys");
    let (a, b) = (d.plugin("a.esm", b"a"), d.plugin("b.esp", b"bb"));
    let both = [a.clone(), b.clone()];
    let c = cache(&d.db());
    assert!(c.holders(&both, "CELL", "room").is_none(), "nothing indexed");
    c.put_keys(&a, "CELL", &keys(&["room", "(0, 0)"]));
    assert!(c.holders(&both, "CELL", "room").is_none(), "one still not indexed");
    c.put_keys(&b, "CELL", &keys(&["(1, 1)"]));
    assert_eq!(c.holders(&both, "CELL", "room"), Some(vec![0]));
    assert_eq!(c.holders(&both, "CELL", "(1, 1)"), Some(vec![1]));
    assert_eq!(c.holders(&both, "CELL", "(9, 9)"), Some(vec![]));
    assert!(c.holders(&both, "LAND", "(1, 1)").is_none(), "another tag");
    assert_eq!(c.all_keys(&both, "CELL").map(|k| k.len()), Some(3));
    std::fs::write(&a, b"changed").unwrap();
    assert!(c.holders(&both, "CELL", "room").is_none());
    assert_eq!(c.missing_keys(&both, "CELL"), vec![0]);
    assert!(!c.keys_current(&a, "CELL"));
    c.put_keys(&a, "CELL", &[]);
    assert_eq!(c.holders(&both, "CELL", "room"), Some(vec![]), "the new keys replaced the old");
    c.put(&a, "CELL", b"[]", Some(&keys(&["vault"])));
    assert_eq!(c.holders(&both, "CELL", "vault"), Some(vec![0]));
    c.clear();
    assert!(c.holders(&both, "CELL", "vault").is_none());
}

#[test]
fn a_removed_plugins_keys_are_forgotten() {
    let d = Dir::new("gone");
    let gone = d.plugin("gone.esp", b"g");
    cache(&d.db()).put_keys(&gone, "CELL", &keys(&["room"]));
    std::fs::remove_file(&gone).unwrap();
    let here = d.plugin("a.esm", b"a");
    cache(&d.db()).put_keys(&here, "CELL", &keys(&["x"]));
    let raw = rusqlite::Connection::open(d.db()).unwrap();
    let n: i64 = raw.query_row("SELECT COUNT(*) FROM keys WHERE key='room'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn a_cache_made_by_another_build_of_the_reader_is_emptied() {
    let d = Dir::new("parser");
    let a = d.plugin("a.esm", b"a");
    let open = |p: &str| Cache::new(d.db(), p, editcore::cache::MAX_BYTES);
    open("reader-1").put(&a, "CELL", b"[1]", Some(&keys(&["vault"])));
    assert!(open("reader-1").get(&a, "CELL").is_some());
    assert!(open("reader-1").keys_current(&a, "CELL"));
    let new = open("reader-2");
    assert!(new.get(&a, "CELL").is_none());
    assert!(!new.keys_current(&a, "CELL"));
    drop(new);
    open("12345").put(&a, "CELL", b"[1]", None);
    assert!(open("12345").get(&a, "CELL").is_some(), "a signature of digits is not a change");
}

#[test]
fn over_its_size_the_cache_drops_the_largest_tags_bodies_and_keeps_the_keys() {
    let d = Dir::new("trim");
    let (a, b) = (d.plugin("a.esm", b"a"), d.plugin("b.esp", b"bb"));
    // Incompressible bodies, so their stored size is what is given.
    let noise = |n: usize, seed: u64| -> Vec<u8> {
        let mut x = seed;
        (0..n).map(|_| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x as u8 }).collect()
    };
    let c = Cache::new(d.db(), "", 300_000);
    c.put(&a, "LAND", &noise(200_000, 1), Some(&keys(&["(0, 0)"])));
    assert!(c.get(&a, "LAND").is_some());
    c.put(&b, "CELL", &noise(100_000, 2), Some(&keys(&["vault"])));
    assert!(c.get(&a, "LAND").is_none(), "the largest tag's bodies went");
    assert!(c.get(&b, "CELL").is_some(), "the one just written was spared");
    assert_eq!(c.holders(&[a], "LAND", "(0, 0)"), Some(vec![0]), "its keys stayed");
    assert!(c.size_bytes() <= 300_000 + 65_536);
}

#[test]
fn two_caches_on_one_file_do_not_lose_each_others_writes() {
    let d = Dir::new("two");
    let plugins: Vec<PathBuf> = (0..40).map(|i| d.plugin(&format!("p{i}.esp"), b"p")).collect();
    let (first, second) = (cache(&d.db()), cache(&d.db()));
    std::thread::scope(|s| {
        for (c, start) in [(&first, 0), (&second, 1)] {
            let mine: Vec<PathBuf> = plugins.iter().skip(start).step_by(2).cloned().collect();
            s.spawn(move || {
                for p in &mine {
                    c.put_keys(p, "CELL", &[p.file_stem().unwrap().to_string_lossy().to_string()]);
                    c.holders(&mine, "CELL", "x");
                }
            });
        }
    });
    let third = cache(&d.db());
    assert_eq!(third.holders(&plugins, "CELL", "p7"), Some(vec![7]));
    assert!(third.missing_keys(&plugins, "CELL").is_empty());
}

#[test]
fn reports_of_an_older_load_order_go_and_are_capped_by_count() {
    let d = Dir::new("reports");
    let c = cache(&d.db());
    c.put_report("old", "uses:Weapon", "sword", b"[1]");
    assert!(c.get_report("old", "uses:Weapon", "sword").is_some());
    c.put_report("new", "uses:Weapon", "sword", b"[]");
    assert!(c.get_report("old", "uses:Weapon", "sword").is_none());
    assert_eq!(c.get_report("new", "uses:Weapon", "sword").as_deref(), Some(&b"[]"[..]));
    for i in 0..REPORTS_KEPT + 5 {
        c.put_report("sig", "uses:Weapon", &format!("k{i}"), b"[]");
    }
    assert!(c.get_report("sig", "uses:Weapon", "k0").is_none(), "the oldest went");
    assert!(c.get_report("sig", "uses:Weapon", &format!("k{}", REPORTS_KEPT + 4)).is_some());
}

#[test]
fn a_load_order_signature_follows_order_and_content() {
    let d = Dir::new("sig");
    let (a, b) = (d.plugin("a.esm", b"a"), d.plugin("b.esp", b"b"));
    let ab = Cache::load_order_signature(&[a.clone(), b.clone()]).unwrap();
    assert_ne!(Some(ab.clone()), Cache::load_order_signature(&[b.clone(), a.clone()]));
    std::fs::write(&a, b"changed").unwrap();
    assert_ne!(Some(ab), Cache::load_order_signature(&[a.clone(), b.clone()]));
    assert!(Cache::load_order_signature(&[d.0.join("missing.esp")]).is_none());
}

#[test]
fn the_use_index_answers_by_string_in_load_and_file_order() {
    let d = Dir::new("uses");
    let (a, b) = (d.plugin("a.esm", b"a"), d.plugin("b.esp", b"b"));
    let both = [a.clone(), b.clone()];
    let row = |v: &str, t: &str, k: &str, p: &str, n: i64| -> UseRow { (v.into(), t.into(), k.into(), p.into(), n) };
    let c = cache(&d.db());
    assert!(c.uses_for(&both, "iron_sword").is_none(), "nothing indexed");
    c.put_uses(&a, &[
        row("iron_sword", "Weapon", "iron_sword", "[]", 0),
        row("iron_sword", "LeveledItem", "l_loot", "[\"items\"]", 1),
        row("iron_sword", "Cell", "(0, 0)", "[\"references\"]", 2),
    ]);
    c.put_uses(&b, &[row("iron_sword", "Npc", "guard", "[\"inventory\"]", 1)]);
    let got = c.uses_for(&both, "iron_sword").unwrap();
    let flat: Vec<(usize, &str, Vec<String>, i64)> = got.iter().map(|(i, _t, k, p, n)| (*i, k.as_str(), p.clone(), *n)).collect();
    assert_eq!(
        flat,
        vec![
            (0, "l_loot", vec!["items".to_string()], 1),
            (0, "(0, 0)", vec!["references".to_string()], 2),
            (1, "guard", vec!["inventory".to_string()], 1),
        ]
    );
    let def = c.definers(&both, &[("Weapon".into(), "iron_sword".into())]).unwrap();
    assert_eq!(def.get(&("Weapon".to_string(), "iron_sword".to_string())), Some(&vec![0]));
    assert!(c.missing_uses(&both).is_empty());
}
