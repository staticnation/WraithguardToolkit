//! Many references of one cell moved in one step (`editRefsSet`): gonzo's box of ~600 objects
//! dragged took minutes (2026-11-10). A step's time must not grow with the square of it.

use editcore::links::Links;
use serde_json::{json, Value};

fn reference(index: i64, x: f64) -> Value {
    let mut r = serde_json::to_value(tes3::esp::Reference::default()).unwrap();
    r["mast_index"] = json!(0);
    r["refr_index"] = json!(index);
    r["id"] = json!("misc_lamp");
    r["translation"] = json!([x, 0.0, 0.0]);
    r
}

#[test]
fn six_hundred_references_move_in_one_step() {
    let dir = std::env::temp_dir().join(format!("wg_many_refs_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut head = editcore::blank::blank("Header").unwrap();
    head["masters"] = json!([]);
    let mut lamp = editcore::blank::blank("MiscItem").unwrap();
    lamp["id"] = json!("misc_lamp");
    let mut c = editcore::blank::blank("Cell").unwrap();
    c["name"] = json!("Big Room");
    c["data"]["flags"] = json!("IS_INTERIOR");
    c["references"] = json!((1..=600).map(|i| reference(i, i as f64)).collect::<Vec<_>>());
    let bytes = editcore::plugin::records_bytes(&Value::Array(vec![head, lamp, c]).to_string()).unwrap();
    let path = dir.join("Room.esm");
    std::fs::write(&path, bytes).unwrap();
    let links = Links::open(vec![("Room.esm".into(), path)], Some(dir.join("journal.json")));
    let changes: Vec<Value> = (1..=600)
        .map(|i| json!({"cell": "Big Room", "origin": "Room.esm", "refr": i, "plugins": ["Room.esm"], "path": "translation", "value": [i as f64, 100.0, 0.0]}))
        .collect();
    let t = std::time::Instant::now();
    let got = links.call("editRefsSet", &json!({"changes": changes}).to_string()).unwrap();
    let took = t.elapsed();
    assert!(got.contains("\"changed\":600"), "{got}");
    let pending = links.call("editPending", "{}").unwrap();
    eprintln!("600 references moved in {took:?}; pending {} bytes", pending.len());
    assert!(took.as_secs_f64() < 5.0, "600 references took {took:?}");
    // As the drag sent them before: one at a time, the pending list read after each.
    if std::env::var("WG_ONE_BY_ONE").is_ok() {
        let t = std::time::Instant::now();
        for i in 1..=600 {
            links.call("editRefSet", &json!({"cell": "Big Room", "origin": "Room.esm", "refr": i, "plugins": ["Room.esm"], "path": "translation", "value": [i as f64, 200.0, 0.0]}).to_string()).unwrap();
            links.call("editPending", "{}").unwrap();
        }
        eprintln!("one at a time: {:?}", t.elapsed());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
