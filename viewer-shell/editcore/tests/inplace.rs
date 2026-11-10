//! A plugin edited in place (`inplace::build_in_place`, the build's `inplace` mode): the pool
//! written into one of the load order's own plugins. What the build does not decide comes
//! back as the same bytes; what it decides takes its own record's place; the header keeps
//! what it had.

use std::path::{Path, PathBuf};

use editcore::edit::Editor;
use editcore::queue::PatchQueue;
use editcore::session::Session;
use serde_json::{json, Value};

fn rec(t: &str, id: &str, fields: Value) -> Value {
    let mut r = editcore::blank::blank(t).unwrap();
    r["id"] = json!(id);
    for (k, v) in fields.as_object().unwrap() {
        r[k] = v.clone();
    }
    r
}

fn reference(mast: i64, index: i64, id: &str, x: f64) -> Value {
    let mut r = serde_json::to_value(tes3::esp::Reference::default()).unwrap();
    r["mast_index"] = json!(mast);
    r["refr_index"] = json!(index);
    r["id"] = json!(id);
    r["translation"] = json!([x, 0.0, 0.0]);
    r
}

fn cell(name: &str, refs: Vec<Value>) -> Value {
    let mut c = editcore::blank::blank("Cell").unwrap();
    c["name"] = json!(name);
    c["data"]["flags"] = json!("IS_INTERIOR");
    c["references"] = json!(refs);
    c
}

fn info(id: &str, prev: &str, next: &str, text: &str) -> Value {
    rec("DialogueInfo", id, json!({"prev_id": prev, "next_id": next, "text": text}))
}

fn plugin_bytes(masters: &[(&str, u64)], author: &str, records: Vec<Value>) -> Vec<u8> {
    let mut head = editcore::blank::blank("Header").unwrap();
    head["author"] = json!(author);
    head["description"] = json!("as its author wrote it");
    head["num_objects"] = json!(records.len());
    head["masters"] = json!(masters.iter().map(|(m, s)| json!([m, s])).collect::<Vec<_>>());
    let mut all = vec![head];
    all.extend(records);
    editcore::plugin::records_bytes(&Value::Array(all).to_string()).unwrap()
}

/// A plugin's records as their bytes, in file order.
fn frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 16 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        out.push(&bytes[at..at + 16 + size]);
        at += 16 + size;
    }
    out
}

fn read(path: &Path) -> Vec<Value> {
    serde_json::from_str(&editcore::plugin::records_json(&std::fs::read(path).unwrap(), None).unwrap()).unwrap()
}

fn by_id<'a>(records: &'a [Value], t: &str, id: &str) -> &'a Value {
    records.iter().find(|r| r["type"] == t && r["id"].as_str().is_some_and(|i| i.eq_ignore_ascii_case(id))).unwrap_or_else(|| panic!("no {t} {id}"))
}

struct Setup {
    dir: PathBuf,
    ed: Editor,
    target: PathBuf,
    original: Vec<u8>,
}

/// Base.esm, Other.esp and Mod.esp (the one edited; Base.esm its only master), with one
/// record of Mod.esp carrying bytes the reader does not write back.
fn setup(name: &str) -> Setup {
    let dir = std::env::temp_dir().join(format!("wg-inplace-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base = plugin_bytes(&[], "Bethesda", vec![
        rec("Npc", "fargoth", json!({"name": "Fargoth"})),
        rec("Npc", "caius", json!({"name": "Caius"})),
        rec("MiscItem", "misc_cup", json!({"name": "Cup"})),
        cell("Test Cell", vec![reference(0, 1, "misc_cup", 10.0), reference(0, 2, "misc_cup", 20.0)]),
    ]);
    let other = plugin_bytes(&[("Base.esm", base.len() as u64)], "Someone", vec![rec("Npc", "caius", json!({"name": "Caius the Other"}))]);
    let mut this = plugin_bytes(&[("Base.esm", base.len() as u64)], "The Author", vec![
        rec("Npc", "fargoth", json!({"name": "Fargoth of Mod"})),
        rec("MiscItem", "misc_plate", json!({"name": "Plate"})),
        rec("Dialogue", "bread", json!({"dialogue_type": "Topic"})),
        info("1", "", "2", "one"),
        info("2", "1", "3", "two"),
        info("3", "2", "", "three"),
        cell("Test Cell", vec![reference(1, 1, "misc_cup", 11.0), reference(0, 1, "misc_plate", 30.0), reference(0, 7, "misc_plate", 40.0)]),
        cell("Other Cell", vec![reference(0, 9, "misc_plate", 50.0)]),
        rec("MiscItem", "misc_last", json!({"name": "Last"})),
    ]);
    // Padding in the NPC's AIDT: the game's own files have it, and the reader writes it as 0.
    let at = this.windows(4).position(|w| w == b"AIDT").expect("the NPC has an AIDT");
    this[at + 8 + 5] = 0xAB;
    let back = editcore::plugin::records_bytes(&editcore::plugin::records_json(&this, None).unwrap()).unwrap();
    assert_ne!(back, this, "the plugin must be one the reader does not write back the same");
    let paths = [("Base.esm", base), ("Other.esp", other), ("Mod.esp", this.clone())];
    let mut plugins = Vec::new();
    for (n, bytes) in &paths {
        let p = dir.join(n);
        std::fs::write(&p, bytes).unwrap();
        plugins.push((n.to_string(), p));
    }
    let target = dir.join("Mod.esp");
    Setup { ed: Editor::new(Session::new(plugins), PatchQueue::new(), None), dir, target, original: this }
}

fn build(s: &mut Setup) -> editcore::build::PatchResult {
    let job = s.ed.prepare_in_place(&s.target).unwrap_or_else(|e| panic!("{e}"));
    let result = editcore::build::run_patch(&s.ed.session, &job, false, &mut |_| {}).unwrap_or_else(|e| panic!("{e}"));
    s.ed.finish_patch(&job, &result);
    result
}

#[test]
fn what_is_not_decided_comes_back_the_same_bytes() {
    let mut s = setup("bytes");
    // One field of one of its own records, and a record it did not have.
    let plate = s.ed.find("MISC", "misc_plate", None).unwrap();
    s.ed.set_field(&plate, "name", &json!("Silver Plate")).unwrap();
    let made = s.ed.insert("MISC", "misc_new").unwrap();
    s.ed.set_field(&made, "name", &json!("New Thing")).unwrap();
    let result = build(&mut s);

    let now = std::fs::read(&s.target).unwrap();
    let (was, is) = (frames(&s.original), frames(&now));
    assert_eq!(is.len(), was.len() + 1, "one record added");
    for (i, (a, b)) in was.iter().zip(&is).enumerate() {
        match i {
            0 => assert_ne!(a, b, "the header counts one more"),
            2 => assert_ne!(a, b, "the plate is the record changed"),
            _ => assert_eq!(a, b, "record {i} was not decided and is not the same bytes"),
        }
    }
    let records = read(&s.target);
    assert_eq!(by_id(&records, "MiscItem", "misc_plate")["name"], json!("Silver Plate"));
    assert_eq!(records.last().unwrap()["id"], json!("misc_new"), "a new record goes last");
    // The header: its own author, description and master; the count set.
    let h = &records[0];
    assert_eq!((h["author"].as_str(), h["description"].as_str()), (Some("The Author"), Some("as its author wrote it")));
    assert_eq!(h["num_objects"], json!(is.len() - 1));
    assert_eq!(h["masters"].as_array().unwrap().len(), 1);
    assert_eq!(h["masters"][0][0], json!("Base.esm"));
    // The plugin as it was is kept beside it, once.
    let backup = result.backup.clone().unwrap();
    assert_eq!(backup, s.dir.join("Mod.esp.preedit.bak"));
    assert_eq!(std::fs::read(&backup).unwrap(), s.original);
    assert!(s.ed.queue.is_empty(), "the pool is spent");
    assert!(result.lines.iter().any(|l| l == "Mod.esp edited in place: 1 record(s) changed, 1 added, 8 left as they were"), "{:#?}", result.lines);

    // A second edit: the backup stays the first.
    let plate = s.ed.find("MISC", "misc_plate", None).unwrap();
    s.ed.set_field(&plate, "name", &json!("Gold Plate")).unwrap();
    build(&mut s);
    assert_eq!(std::fs::read(&backup).unwrap(), s.original);
    assert_eq!(by_id(&read(&s.target), "MiscItem", "misc_plate")["name"], json!("Gold Plate"));
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_record_from_another_plugin_brings_its_master_after_its_own() {
    let mut s = setup("master");
    // Caius as Other.esp has him (it loads after Base.esm; Mod.esp does not know it).
    let caius = s.ed.find("NPC_", "caius", None).unwrap();
    assert_eq!(caius.winner(), "Other.esp");
    s.ed.set_field(&caius, "name", &json!("Caius of Mod")).unwrap();
    let result = build(&mut s);
    assert_eq!(result.masters, ["Base.esm", "Other.esp"]);
    let records = read(&s.target);
    let masters: Vec<&str> = records[0]["masters"].as_array().unwrap().iter().map(|m| m[0].as_str().unwrap()).collect();
    assert_eq!(masters, ["Base.esm", "Other.esp"], "its own first, the new one after");
    assert_eq!(records[0]["masters"][1][1], json!(std::fs::metadata(s.dir.join("Other.esp")).unwrap().len()));
    assert_eq!(by_id(&records, "Npc", "caius")["name"], json!("Caius of Mod"));
    // Its cells still name Base.esm as 1: nothing of them was renumbered or touched.
    let now = std::fs::read(&s.target).unwrap();
    let (was, is) = (frames(&s.original), frames(&now));
    assert_eq!(was[7], is[7]);
    assert_eq!(was[8], is[8]);
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_cells_references_are_changed_in_its_own_record() {
    let mut s = setup("cell");
    // One of its own references moved, one of its master's (which it already changes) too,
    // and one of the master's it did not have; then a new one placed.
    let own = s.ed.session.find_ref("Test Cell", "Mod.esp", 7, None).unwrap();
    s.ed.set_ref_field(&own, "translation", &json!([41.0, 0.0, 0.0])).unwrap();
    let theirs = s.ed.session.find_ref("Test Cell", "Base.esm", 2, None).unwrap();
    s.ed.set_ref_field(&theirs, "translation", &json!([21.0, 0.0, 0.0])).unwrap();
    let cup = s.ed.find("MISC", "misc_cup", None).unwrap();
    s.ed.place("Test Cell", &cup, &json!([60.0, 0.0, 0.0]), None, None).unwrap();
    build(&mut s);

    let records = read(&s.target);
    let cells: Vec<&Value> = records.iter().filter(|r| r["type"] == "Cell").collect();
    assert_eq!(cells.len(), 2, "its own record for the cell is the one changed: no second one");
    let refs = cells[0]["references"].as_array().unwrap();
    let at = |mast: i64, index: i64| refs.iter().find(|r| r["mast_index"] == json!(mast) && r["refr_index"] == json!(index)).unwrap_or_else(|| panic!("no reference {mast}:{index} in {refs:#?}"));
    assert_eq!(at(0, 7)["translation"][0], json!(41.0), "its own, moved, still its own (0)");
    assert_eq!(at(1, 2)["translation"][0], json!(21.0), "the master's, by the master's place in its list");
    assert_eq!(at(1, 1)["translation"][0], json!(11.0), "what it already changed stays");
    assert_eq!(at(0, 1)["translation"][0], json!(30.0));
    // The new one is numbered past every reference of its own, the other cell's 9 included.
    assert_eq!(at(0, 10)["id"], json!("misc_cup"));
    assert_eq!(refs.len(), 5);
    // The other cell is the same bytes.
    let now = std::fs::read(&s.target).unwrap();
    let (was, is) = (frames(&s.original), frames(&now));
    assert_eq!(was[8], is[8]);
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_response_goes_where_its_links_place_it() {
    let mut s = setup("topic");
    // A new response after the first, and the third's text changed where it stands.
    let made = s.ed.new_response("bread", "1").unwrap();
    s.ed.set_field(&made, "text", &json!("new")).unwrap();
    let third = s.ed.find("INFO", "3", None).unwrap();
    s.ed.set_field(&third, "text", &json!("three, changed")).unwrap();
    build(&mut s);

    let records = read(&s.target);
    let at = records.iter().position(|r| r["type"] == "Dialogue").unwrap();
    let block: Vec<(&str, &str)> = records[at + 1..].iter().take_while(|r| r["type"] == "DialogueInfo").map(|r| (r["id"].as_str().unwrap(), r["text"].as_str().unwrap())).collect();
    assert_eq!(block.len(), 4, "{block:?}");
    assert_eq!((block[0], block[1].1, block[2].0, block[3]), (("1", "one"), "new", "2", ("3", "three, changed")), "{block:?}");
    assert_eq!(records.iter().filter(|r| r["type"] == "Dialogue").count(), 1, "the topic is not written twice");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_deleted_record_only_it_has_is_taken_out_and_a_masters_is_marked() {
    let mut s = setup("deleted");
    for (tag, id) in [("MISC", "misc_plate"), ("NPC_", "fargoth"), ("INFO", "2")] {
        let found = s.ed.find(tag, id, None).unwrap();
        s.ed.set_field(&found, "flags", &json!("DELETED")).unwrap();
    }
    let result = build(&mut s);
    let records = read(&s.target);
    assert!(!records.iter().any(|r| r["id"] == json!("misc_plate")), "only Mod.esp had the plate: it is out of the file");
    let fargoth = by_id(&records, "Npc", "fargoth");
    assert!(fargoth["flags"].as_str().unwrap().contains("DELETED"), "Base.esm has Fargoth: a DELETED record stands");
    let infos: Vec<&str> = records.iter().filter(|r| r["type"] == "DialogueInfo").map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(infos, ["1", "3"], "the response is out of its topic");
    assert_eq!(records[0]["num_objects"], json!(records.len() - 1));
    assert!(result.lines.iter().any(|l| l == "2 record(s) taken out of it: deleted, and no other plugin has them"), "{:#?}", result.lines);
    // The records between are the same bytes.
    let now = std::fs::read(&s.target).unwrap();
    let (was, is) = (frames(&s.original), frames(&now));
    assert_eq!(was[9], *is.last().unwrap(), "the last record as it was");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn only_the_load_orders_own_plugin_is_edited_in_place() {
    let mut s = setup("refused");
    let plate = s.ed.find("MISC", "misc_plate", None).unwrap();
    s.ed.set_field(&plate, "name", &json!("x")).unwrap();
    let elsewhere = s.dir.join("Elsewhere.esp");
    std::fs::write(&elsewhere, &s.original).unwrap();
    assert!(s.ed.prepare_in_place(&elsewhere).unwrap_err().0.contains("is not a plugin of the load order"));
    let copy = s.dir.join("copy");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join("Mod.esp"), &s.original).unwrap();
    assert!(s.ed.prepare_in_place(&copy.join("Mod.esp")).unwrap_err().0.contains("not this file"));
    // A dry run writes nothing, and keeps no backup.
    let job = s.ed.prepare_in_place(&s.target).unwrap();
    let dry = editcore::build::run_patch(&s.ed.session, &job, true, &mut |_| {}).unwrap();
    assert!(dry.output.is_none() && dry.backup.is_none());
    assert_eq!(std::fs::read(&s.target).unwrap(), s.original);
    assert!(!s.dir.join("Mod.esp.preedit.bak").exists());
    let _ = std::fs::remove_dir_all(&s.dir);
}

/// On a real plugin (`WG_INPLACE=<file>`, copied first): one NPC's name changed in place, and
/// every other record of the file the same bytes. With the time it took.
#[test]
#[ignore]
fn in_place_given() {
    let given = PathBuf::from(std::env::var("WG_INPLACE").expect("WG_INPLACE=<plugin>"));
    let dir = std::env::temp_dir().join(format!("wg-inplace-given-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let name = given.file_name().unwrap().to_string_lossy().into_owned();
    let target = dir.join(&name);
    std::fs::copy(&given, &target).unwrap();
    let original = std::fs::read(&target).unwrap();
    let npcs: Vec<Value> = serde_json::from_str(&editcore::plugin::records_json(&original, Some(&[*b"NPC_"])).unwrap()).unwrap();
    let id = npcs[npcs.len() / 2]["id"].as_str().unwrap().to_string();
    let mut ed = Editor::new(Session::new(vec![(name.clone(), target.clone())]), PatchQueue::new(), None);
    let npc = ed.find("NPC_", &id, None).unwrap();
    ed.set_field(&npc, "name", &json!("Edited In Place")).unwrap();
    let t = std::time::Instant::now();
    let job = ed.prepare_in_place(&target).unwrap();
    let result = editcore::build::run_patch(&ed.session, &job, false, &mut |_| {}).unwrap();
    println!("{name}: {:.2} s; {}", t.elapsed().as_secs_f32(), result.lines.last().unwrap());
    let now = std::fs::read(&target).unwrap();
    let (was, is) = (frames(&original), frames(&now));
    assert_eq!(was.len(), is.len());
    let differ: Vec<usize> = (0..was.len()).filter(|&i| was[i] != is[i]).collect();
    println!("{} of {} records differ: {differ:?} (the header's count is unchanged)", differ.len(), was.len());
    assert_eq!(differ.len(), 1, "only the NPC");
    assert_eq!(by_id(&read(&target), "Npc", &id)["name"], json!("Edited In Place"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_landscape_from_another_plugin_is_numbered_for_this_ones_textures() {
    use tes3::esp::TES3Object;
    let dir = std::env::temp_dir().join(format!("wg-inplace-land-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ltex = |id: &str, index: i64| rec("LandscapeTexture", id, json!({"index": index, "file_name": format!("{id}.tga")}));
    // Base.esm: sand is its 0 and rock its 1; the land wears sand (1) and rock (2).
    let mut land = tes3::esp::Landscape { grid: (0, 0), ..Default::default() };
    let rows: Vec<landcore::grid::Value> = (0..256).map(|i| if i < 128 { 1 } else { 2 }).collect();
    landcore::emit::attach_texture_indices(&mut land, &rows);
    let land = serde_json::to_value(TES3Object::Landscape(land)).unwrap();
    let base = plugin_bytes(&[], "Bethesda", vec![ltex("tx_sand", 0), ltex("tx_rock", 1), land]);
    // Mod.esp: rock is its 0, grass its 1; it has no land there.
    let this = plugin_bytes(&[("Base.esm", base.len() as u64)], "The Author", vec![ltex("tx_rock", 0), ltex("tx_grass", 1)]);
    let mut plugins = Vec::new();
    for (n, bytes) in [("Base.esm", &base), ("Mod.esp", &this)] {
        let p = dir.join(n);
        std::fs::write(&p, bytes).unwrap();
        plugins.push((n.to_string(), p));
    }
    let target = dir.join("Mod.esp");
    let mut ed = Editor::new(Session::new(plugins), PatchQueue::new(), None);
    ed.queue.add_whole(editcore::queue::Selection { plugin: "Base.esm".into(), record_type: "Landscape".into(), key: "(0, 0)".into() });
    let job = ed.prepare_in_place(&target).unwrap();
    let result = editcore::build::run_patch(&ed.session, &job, false, &mut |_| {}).unwrap_or_else(|e| panic!("{e}"));
    assert!(result.lines.iter().any(|l| l == "1 landscape(s) had their textures renumbered for this plugin, 1 land texture(s) brought with them"), "{:#?}", result.lines);

    let records = read(&target);
    // Sand came along as Mod.esp's 2; rock is the rock it had (0); grass stands.
    let index = |id: &str| by_id(&records, "LandscapeTexture", id)["index"].as_i64().unwrap();
    assert_eq!((index("tx_rock"), index("tx_grass"), index("tx_sand")), (0, 1, 2));
    let land = records.iter().find(|r| r["type"] == "Landscape").unwrap().clone();
    let Ok(TES3Object::Landscape(land)) = serde_json::from_value::<TES3Object>(land) else { panic!("no land") };
    let mut worn: Vec<u16> = land.texture_indices.data.iter().flatten().copied().collect();
    worn.sort_unstable();
    worn.dedup();
    assert_eq!(worn, [1, 3], "rock as Mod.esp's 0 (1), sand as its 2 (3)");
    let _ = std::fs::remove_dir_all(&dir);
}
