//! The Use Report of an id with a space (not indexable): the index plus only the scripts and
//! responses that mention it is the report the whole sources give.

use editcore::cache::Cache;
use editcore::uses::{from_index, index_rows, with_mentions, Use};
use serde_json::{json, Value};
use std::path::PathBuf;

fn sorted(mut v: Vec<Use>) -> Vec<Use> {
    v.sort_by(|a, b| (a.plugin, &a.record_type, &a.key).cmp(&(b.plugin, &b.record_type, &b.key)));
    v
}

#[test]
fn an_id_with_a_space_is_found_in_the_sources_that_mention_it() {
    let dir = std::env::temp_dir().join(format!("wg-mentions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let plugins: Vec<Vec<Value>> = vec![
        vec![
            json!({"type": "Static", "id": "blue ghost", "mesh": "x.nif"}),
            json!({"type": "Activator", "id": "act1", "name": "Blue Ghost", "script": "scr1"}),
            json!({"type": "Script", "id": "scr1", "text": "Begin scr1\n  if ( GetItemCount \"blue ghost\" > 0 )\n  endif\nEnd"}),
            json!({"type": "Script", "id": "scr2", "text": "Begin scr2\n ; xblue ghost is another word\nEnd"}),
        ],
        vec![
            json!({"type": "Dialogue", "id": "greeting"}),
            json!({"type": "DialogueInfo", "id": "42", "script_text": "AddItem \"Blue Ghost\" 1"}),
            json!({"type": "DialogueInfo", "id": "43", "script_text": "Say nothing"}),
        ],
    ];
    let paths: Vec<PathBuf> = (0..plugins.len())
        .map(|i| {
            let p = dir.join(format!("p{i}.esp"));
            std::fs::write(&p, format!("plugin {i}")).unwrap();
            p
        })
        .collect();
    let cache = Cache::new(dir.join("c.sqlite3"), "t", editcore::cache::MAX_BYTES);
    for (recs, path) in plugins.iter().zip(&paths) {
        cache.put_uses(path, &index_rows(recs));
    }
    let all = |i: usize| -> Vec<Value> { plugins[i].iter().filter(|r| r["type"] == "Script" || r["type"] == "DialogueInfo").cloned().collect() };
    let mentioning = |i: usize| with_mentions("blue ghost", plugins[i].iter());

    let whole = from_index(&cache, &paths, "Static", "blue ghost", all).expect("every plugin indexed");
    let narrowed = from_index(&cache, &paths, "Static", "blue ghost", mentioning).expect("every plugin indexed");
    assert_eq!(sorted(narrowed.clone()), sorted(whole));
    let found: Vec<(&str, &str)> = narrowed.iter().map(|u| (u.record_type.as_str(), u.key.as_str())).collect();
    assert!(found.contains(&("Script", "scr1")), "{found:?}");
    assert!(found.contains(&("DialogueInfo", "42")), "{found:?}");
    assert!(found.contains(&("Activator", "act1")), "the index's own matches are kept: {found:?}");
    assert!(!found.contains(&("Script", "scr2")), "a word inside another word is not a mention: {found:?}");
    assert_eq!(with_mentions("blue ghost", plugins[1].iter()).len(), 1, "only the response that mentions it is copied");
    let _ = std::fs::remove_dir_all(&dir);
}
