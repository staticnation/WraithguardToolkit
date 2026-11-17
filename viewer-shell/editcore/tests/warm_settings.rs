//! A launch's game settings (`Session::warm` of `GMST`, Verify's "game settings"; Static,
//! 2026-11-17): a plugin the key index says has none is given none without its records being
//! asked for, and what is given is what reading every plugin gives.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use editcore::cache::Cache;
use editcore::session::Session;
use tes3::esp::{GameSetting, GameSettingValue, Header, Plugin, Static, TES3Object};

fn write(path: &Path, settings: &[(&str, f32)], statics: usize) {
    let mut objects = vec![TES3Object::Header(Header { version: 1.3, ..Default::default() })];
    for (id, v) in settings {
        objects.push(TES3Object::GameSetting(GameSetting { id: id.to_string(), value: GameSettingValue::Float(*v), ..Default::default() }));
    }
    for i in 0..statics {
        objects.push(TES3Object::Static(Static { id: format!("wg_rock_{i}"), mesh: "r.nif".into(), ..Default::default() }));
    }
    Plugin { objects }.save_path(path).unwrap();
}

/// `count` plugins, every `every`-th with two game settings, all with a few statics.
fn order(dir: &Path, count: usize, every: usize) -> Vec<(String, PathBuf)> {
    (0..count)
        .map(|i| {
            let name = format!("p{i:04}.esp");
            let path = dir.join(&name);
            let settings: Vec<(&str, f32)> = if i % every == 0 { vec![("fEnchantmentMult", i as f32), ("iHelmWeight", 5.0)] } else { vec![] };
            write(&path, &settings, 3);
            (name, path)
        })
        .collect()
}

fn launch(plugins: &[(String, PathBuf)], db: &Path) -> Session {
    let s = Session::new(plugins.to_vec());
    s.set_disk_cache(Arc::new(Cache::new(db.to_path_buf(), "t", 1 << 26)));
    s
}

fn settings(s: &Session) -> Vec<Vec<(String, String)>> {
    s.records_all("GMST").iter().map(|m| m.iter().map(|(k, v)| (k.clone(), v.to_string())).collect()).collect()
}

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn dir(tag: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("wg-warm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d)
}

#[test]
fn a_plugin_with_no_game_settings_is_given_none_and_the_rest_are_as_read() {
    let d = dir("same");
    let plugins = order(&d.0, 40, 7);
    let db = d.0.join("c.sqlite3");
    // The first launch reads them all, and indexes their keys.
    let first = launch(&plugins, &db);
    first.warm_ids(&["GMST"]);
    let read = settings(&first);
    assert_eq!(read.iter().filter(|m| !m.is_empty()).count(), 6, "every seventh has settings");
    drop(first);
    // The next: the same answer, the plugins without settings never asked of the record cache.
    let next = launch(&plugins, &db);
    next.warm_ids(&["GMST"]);
    assert_eq!(settings(&next), read);
    // A plugin that gains settings since is read again, not taken as having none.
    write(&plugins[1].1, &[("fNew", 2.0)], 4);
    let after = launch(&plugins, &db);
    after.warm_ids(&["GMST"]);
    let got = settings(&after);
    assert_eq!(got[1], vec![("fnew".to_string(), got[1][0].1.clone())]);
    assert_eq!(got[0], read[0]);
}

/// A launch's game settings over 620 plugins, 31 with settings, the cache warm
/// (`cargo test --release -p editcore --test warm_settings -- --ignored --nocapture`).
#[test]
#[ignore]
fn launch_settings_time() {
    let d = dir("time");
    let plugins = order(&d.0, 620, 20);
    let db = d.0.join("c.sqlite3");
    let first = launch(&plugins, &db);
    first.warm_ids(&["GMST"]);
    first.warm(&["GMST"]);
    drop(first);
    for run in 0..3 {
        let s = launch(&plugins, &db);
        s.warm_ids(&["GMST"]);
        let t = std::time::Instant::now();
        s.warm(&["GMST"]);
        eprintln!("run {run}: warm GMST {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
}
