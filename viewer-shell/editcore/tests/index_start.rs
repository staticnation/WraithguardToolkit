//! The use index is brought up to date in the background once the Editor is open, on its own.

use editcore::cache::Cache;
use editcore::links::Links;
use std::time::{Duration, Instant};

#[test]
fn plugins_the_index_lacks_are_indexed_after_the_world_loads() {
    let dir = std::env::temp_dir().join(format!("wg-index-start-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let plugins: Vec<(String, std::path::PathBuf)> = (0..3)
        .map(|i| {
            let p = dir.join(format!("p{i}.esp"));
            std::fs::write(&p, format!("not a plugin {i}")).unwrap();
            (format!("p{i}.esp"), p)
        })
        .collect();
    let paths: Vec<_> = plugins.iter().map(|p| p.1.clone()).collect();
    let db = dir.join("use.sqlite3");
    let watch = Cache::new(db.clone(), "editor-use-index", editcore::cache::MAX_BYTES);
    assert_eq!(watch.missing_uses(&paths).len(), 3, "nothing is indexed to begin with");

    let _links = Links::open(plugins, None).with_use_index(db).start_indexing();
    let until = Instant::now() + Duration::from_secs(20);
    while !watch.missing_uses(&paths).is_empty() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(watch.missing_uses(&paths).is_empty(), "every plugin was indexed without a report being asked");
    let _ = std::fs::remove_dir_all(&dir);
}
