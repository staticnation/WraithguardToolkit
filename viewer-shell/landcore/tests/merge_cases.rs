//! Merge Lands against the Python's own output (`tests/fixtures/land_merge_cases.json`,
//! written by `tools/gen_land_merge_cases.py`): each case's plugins are built from the same
//! recipes (and checked against the Python's input hashes), merged with
//! `landcore::service`, and must give the same progress lines, the same result counts and
//! the same `Merged Lands.esp`, byte for byte.

use std::path::{Path, PathBuf};

use landcore::emit::world_map_from_heights;
use landcore::heights::normals_from_heights;
use landcore::layers::swizzle_texture_indices;
use landcore::service::{build_merged_lands, MergeOptions, DEFAULT_NAME};
use serde_json::Value;
use sha2::{Digest, Sha256};

const CASES: &str = include_str!("../../../tests/fixtures/land_merge_cases.json");
/// What the engine now answers where the audit of 2026-11-11 (`TERRAIN_AUDIT_2026-11-11.md`,
/// WG-A01 and WG-A04) made it differ from the Python these cases were recorded from: heights
/// finalized to VHGT's grid before normals, and slopes limited to what VHGT really stores.
/// A case named here is held to this record instead of the Python's for the lines, the counts
/// and the plugin's bytes (the inputs still must match the Python's hashes). Re-record with
/// `LANDCORE_RECORD_AUDIT=1 cargo test -p landcore --test merge_cases`, after checking the
/// difference is one the audit asks for.
const AUDIT: &str = include_str!("fixtures/land_merge_cases_audit.json");
const SIZE: usize = 65;

fn ints(v: &Value) -> Vec<i32> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap() as i32).collect()
}

fn heights_of(r: &[i32]) -> Vec<i32> {
    let (base, a, b, m, cx, cy, rad, bump) = (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7]);
    let mut out = Vec::with_capacity(SIZE * SIZE);
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let mut v = base + (x * a + y * b).rem_euclid(m);
            if (x - cx).pow(2) + (y - cy).pow(2) <= rad * rad {
                v += bump;
            }
            out.push(v);
        }
    }
    out
}

fn sub(tag: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend((payload.len() as u32).to_le_bytes());
    v.extend(payload);
    v
}

fn record(tag: &[u8], body: &[u8], flags: u32) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend((body.len() as u32).to_le_bytes());
    v.extend(0u32.to_le_bytes());
    v.extend(flags.to_le_bytes());
    v.extend(body);
    v
}

fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

fn land(r: &Value) -> Vec<u8> {
    let g = ints(&r["grid"]);
    let mut body = sub(b"INTV", &[g[0].to_le_bytes(), g[1].to_le_bytes()].concat());
    if let Some(f) = r.get("flags") {
        body.extend(sub(b"DATA", &(f.as_u64().unwrap() as u32).to_le_bytes()));
    }
    let stored = r.get("heights").map(|h| heights_of(&ints(h)));
    let world: Option<Vec<i32>> = stored.as_ref().map(|s| s.iter().map(|v| v * 8).collect());
    if let (Some(stored), Some(world)) = (&stored, &world) {
        let normals: Vec<i32> = match r.get("normals") {
            // One vector at every vertex: normals authored on their own (a normal-only patch).
            Some(v) if v.is_array() => ints(v).iter().copied().cycle().take(3 * SIZE * SIZE).collect(),
            n if n.and_then(Value::as_str).unwrap_or("auto") == "auto" => normals_from_heights(&world.iter().map(|&v| v as f64).collect::<Vec<_>>()),
            _ => vec![0; 3 * SIZE * SIZE],
        };
        body.extend(sub(b"VNML", &normals.iter().map(|&v| v as i8 as u8).collect::<Vec<_>>()));
        let mut vhgt = (stored[0] as f32).to_le_bytes().to_vec();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let d = if x == 0 {
                    if y == 0 { 0 } else { stored[y * SIZE] - stored[(y - 1) * SIZE] }
                } else {
                    stored[y * SIZE + x] - stored[y * SIZE + x - 1]
                };
                vhgt.push(d as i8 as u8);
            }
        }
        vhgt.extend([0, 0, 0]);
        body.extend(sub(b"VHGT", &vhgt));
    }
    if let Some(wm) = r.get("world_map") {
        let values: Vec<i32> = match wm.as_str() {
            Some("auto") => world_map_from_heights(world.as_ref().unwrap()),
            Some(_) => vec![0; 81],
            None => ints(wm),
        };
        body.extend(sub(b"WNAM", &values.iter().map(|&v| v as i8 as u8).collect::<Vec<_>>()));
    }
    if let Some(c) = r.get("colors") {
        let c = ints(c);
        let mut data = Vec::new();
        for y in 0..SIZE as i32 {
            for x in 0..SIZE as i32 {
                data.extend([(c[0] + (x * c[3]).rem_euclid(50)).rem_euclid(256) as u8, (c[1] + (y * c[3]).rem_euclid(50)).rem_euclid(256) as u8, c[2].rem_euclid(256) as u8]);
            }
        }
        body.extend(sub(b"VCLR", &data));
    }
    if let Some(t) = r.get("textures") {
        let t = ints(t);
        let rows: Vec<i32> = (0..256).map(|i| if ((i % 16) + (i / 16)) % t[1] == 0 { t[2] } else { t[0] }).collect();
        let packed = swizzle_texture_indices(&rows);
        body.extend(sub(b"VTEX", &packed.iter().flatten().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()));
    }
    let deleted = r.get("deleted").and_then(Value::as_bool).unwrap_or(false);
    record(b"LAND", &body, if deleted { 0x20 } else { 0 })
}

fn ltex(t: &Value) -> Vec<u8> {
    let t = t.as_array().unwrap();
    let mut body = sub(b"NAME", &[latin1(t[0].as_str().unwrap()), vec![0]].concat());
    body.extend(sub(b"INTV", &(t[1].as_u64().unwrap() as u32).to_le_bytes()));
    if let Some(f) = t[2].as_str() {
        body.extend(sub(b"DATA", &[latin1(f), vec![0]].concat()));
    }
    let deleted = t.get(3).and_then(Value::as_bool).unwrap_or(false);
    record(b"LTEX", &body, if deleted { 0x20 } else { 0 })
}

fn plugin(p: &Value) -> Vec<u8> {
    let mut records: Vec<Vec<u8>> = p.get("ltex").and_then(Value::as_array).into_iter().flatten().map(ltex).collect();
    records.extend(p.get("land").and_then(Value::as_array).into_iter().flatten().map(land));
    let name = p["name"].as_str().unwrap();
    let is_master = name.to_lowercase().ends_with(".esm");
    let mut hedr = 1.3f32.to_le_bytes().to_vec();
    hedr.extend((is_master as u32).to_le_bytes());
    hedr.extend([0u8; 288]);
    hedr.extend((records.len() as u32).to_le_bytes());
    let mut body = sub(b"HEDR", &hedr);
    for m in p.get("masters").and_then(Value::as_array).into_iter().flatten() {
        body.extend(sub(b"MAST", &[latin1(m.as_str().unwrap()), vec![0]].concat()));
        body.extend(sub(b"DATA", &0u64.to_le_bytes()));
    }
    [record(b"TES3", &body, 0), records.concat()].concat()
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn root_for(name: &str) -> PathBuf {
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(40).collect();
    let d = std::env::temp_dir().join(format!("landcore_merge_{}_{slug}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    d
}

fn stem(name: &str) -> &str {
    Path::new(name).file_stem().unwrap().to_str().unwrap()
}

#[test]
fn a_whole_merge_gives_the_recorded_plugin_and_report() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let audit: Value = serde_json::from_str(AUDIT).unwrap();
    let recording = std::env::var_os("LANDCORE_RECORD_AUDIT").is_some();
    let mut recorded = serde_json::Map::new();
    for case in doc["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut merged_expect = case["expect"].clone();
        if let Some(over) = audit.get(name).and_then(Value::as_object) {
            for (k, v) in over {
                merged_expect[k.as_str()] = v.clone();
            }
        }
        let expect = &merged_expect;
        let root = root_for(name);
        let data = root.join("data");
        for p in case["plugins"].as_array().unwrap() {
            let pname = p["name"].as_str().unwrap();
            let body = match p.get("raw").and_then(Value::as_str) {
                Some(raw) => raw.as_bytes().to_vec(),
                None => plugin(p),
            };
            assert_eq!(hex(&body), expect["inputs"][pname].as_str().unwrap(), "{name}: {pname} built differently from the Python's");
            std::fs::write(data.join(pname), &body).unwrap();
            if let Some(t) = p.get("toml").and_then(Value::as_str) {
                std::fs::write(data.join(format!("{}.mergedlands.toml", stem(pname))), t).unwrap();
            }
        }
        let order: Vec<String> = case["order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let out = root.join("out").join(DEFAULT_NAME);
        let opts = MergeOptions {
            output: Some(out.clone()),
            substitute_unknown_textures: case.get("substitute").and_then(Value::as_bool).unwrap_or(true),
            verbose: case.get("verbose").and_then(Value::as_bool).unwrap_or(false),
            // The Python's model and cleaning, which these cases were recorded from.
            clean_against_load_order: false,
            // Recorded before the engine stopped at a step it cannot repair.
            allow_clamping: true,
            ..Default::default()
        };
        let mut said = Vec::new();
        let got = build_merged_lands(std::slice::from_ref(&data), &order, &opts, &mut |l| said.push(l.to_string()));
        let rootstr = root.display().to_string();
        // The finish step's and the write's times (Verbose's, for the merge's own log) are not the Python's.
        let un = |lines: &[String]| -> Vec<String> {
            lines.iter().filter(|l| !l.starts_with("  finished in ") && !l.starts_with("  encoded in ")).map(|l| l.replace(&rootstr, "<root>").replace('\\', "/")).collect()
        };
        let want_lines: Vec<String> = expect["lines"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        // The read progress ("  read n/m plugin(s)") is said for the page only, not the Python's.
        said.retain(|l| !(l.starts_with("  read ") && l.ends_with(" plugin(s)")));
        if recording {
            let mut rec = serde_json::Map::new();
            rec.insert("lines".into(), Value::from(un(&said)));
            match &got {
                Err((message, _)) => {
                    rec.insert("error".into(), Value::from(message.clone()));
                }
                Ok(result) => {
                    let mut counts = serde_json::Map::new();
                    for (k, v) in ["cells", "textures", "contested", "major", "seams", "slopes", "clamped", "dropped", "borrowed"].iter().zip([
                        result.cells_written, result.textures_written, result.contested, result.major, result.seam_vertices,
                        result.slope_vertices, result.clamped, result.dropped, result.borrowed,
                    ]) {
                        counts.insert((*k).into(), Value::from(v as u64));
                    }
                    rec.insert("result".into(), Value::Object(counts));
                    if result.output.is_some() {
                        let body = std::fs::read(&out).unwrap();
                        rec.insert("sha256".into(), Value::from(hex(&body)));
                        rec.insert("size".into(), Value::from(body.len() as u64));
                    }
                }
            }
            // Only a case the engine now answers differently from the Python is recorded.
            let same = {
                let py = &case["expect"];
                rec.get("lines") == py.get("lines")
                    && rec.get("result") == py.get("result")
                    && rec.get("sha256") == py.get("sha256")
                    && rec.get("error").map(|e| e.as_str()) == Some(py["error"].as_str())
            } || (rec.get("error").is_none() && rec.get("lines") == case["expect"].get("lines") && rec.get("result") == case["expect"].get("result") && rec.get("sha256") == case["expect"].get("sha256"));
            if !same {
                recorded.insert(name.to_string(), Value::Object(rec));
            }
            let _ = std::fs::remove_dir_all(&root);
            continue;
        }
        assert_eq!(un(&said), want_lines, "{name}: the lines said");
        match got {
            Err((message, lines)) => {
                assert_eq!(Some(message.as_str()), expect["error"].as_str(), "{name}");
                assert_eq!(un(&lines), want_lines, "{name}");
            }
            Ok(result) => {
                assert!(expect["error"].is_null(), "{name}: merged, where the Python stops");
                assert_eq!(un(&result.lines), want_lines, "{name}");
                let r = &expect["result"];
                let counts = [
                    result.cells_written, result.textures_written, result.contested, result.major, result.seam_vertices,
                    result.slope_vertices, result.clamped, result.dropped, result.borrowed,
                ];
                let want: Vec<usize> = ["cells", "textures", "contested", "major", "seams", "slopes", "clamped", "dropped", "borrowed"]
                    .iter()
                    .map(|k| r[k].as_u64().unwrap() as usize)
                    .collect();
                assert_eq!(counts.to_vec(), want, "{name}: the counts");
                match expect.get("sha256").and_then(Value::as_str) {
                    Some(sha) => {
                        let body = std::fs::read(&out).unwrap();
                        if hex(&body) != sha {
                            let keep = std::env::temp_dir().join("landcore_merge_mismatch.esp");
                            std::fs::write(&keep, &body).unwrap();
                            panic!("{name}: Merged Lands.esp differs ({} bytes, want {}); kept as {}", body.len(), expect["size"], keep.display());
                        }
                        let marker = std::fs::read_to_string(out.with_file_name("Merged Lands.mergedlands.toml")).unwrap();
                        assert_eq!(marker, expect["marker"].as_str().unwrap(), "{name}: the marker");
                    }
                    None => assert!(result.output.is_none(), "{name}: wrote a plugin, where the Python wrote none"),
                }
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
    if recording {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/land_merge_cases_audit.json");
        std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(recorded)).unwrap() + "\n").unwrap();
        eprintln!("recorded {}", path.display());
    }
}

/// A load order built from recipes into a fresh folder; answers the folder.
fn setup(name: &str, plugins: Value) -> PathBuf {
    let root = root_for(name);
    let data = root.join("data");
    for p in plugins.as_array().unwrap() {
        let pname = p["name"].as_str().unwrap();
        std::fs::write(data.join(pname), plugin(p)).unwrap();
        if let Some(t) = p.get("toml").and_then(Value::as_str) {
            std::fs::write(data.join(format!("{}.mergedlands.toml", stem(pname))), t).unwrap();
        }
    }
    data
}

fn run(data: &Path, order: &[&str], opts: MergeOptions) -> landcore::service::MergeResult {
    let order: Vec<String> = order.iter().map(|s| s.to_string()).collect();
    let opts = MergeOptions { output: Some(data.parent().unwrap().join("out").join(DEFAULT_NAME)), ..opts };
    build_merged_lands(&[data.to_path_buf()], &order, &opts, &mut |_| {}).unwrap_or_else(|(e, lines)| panic!("{e}\n{}", lines.join("\n")))
}

#[test]
fn the_two_models_and_the_load_order_cleaning() {
    use landcore::pipeline::DiffModel;
    use serde_json::json;
    let flat = json!([400, 1, 1, 9, 0, 0, -1, 0]);
    // A builds a hill; B, later, edits elsewhere in the cell and carries vanilla ground where
    // A's hill is.
    let data = setup(
        "two models",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 20, 20, 6, 60]}]},
            {"name": "B.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 48, 48, 5, 40]}]},
        ]),
    );
    let order = ["Base.esm", "A.esp", "B.esp"];
    // Against the masters both edits survive, and the result is neither A's record nor B's.
    let r = run(&data, &order, MergeOptions { dry_run: true, ..Default::default() });
    assert_eq!((r.cells_written, r.dropped), (1, 0), "{:#?}", r.lines);
    // Against the load order, B's vanilla ground reverts the hill: the merge is B's record,
    // which the load order already loads, so nothing is written.
    let r = run(&data, &order, MergeOptions { dry_run: true, model: DiffModel::LoadOrder, ..Default::default() });
    assert_eq!((r.cells_written, r.dropped), (0, 1), "{:#?}", r.lines);
    assert!(r.lines.contains(&"  each plugin measured against the load order before it".to_string()));
    assert!(r.lines.contains(&"nothing to merge; no plugin written".to_string()));
    let _ = std::fs::remove_dir_all(data.parent().unwrap());

    // A's heights are excluded by its settings: the merge keeps vanilla heights, which the
    // load order does not load (A's do), so the cell is written to put them back. The
    // Python's cleaning dropped it as unchanged from the masters.
    let data = setup(
        "excluded heights",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "toml": "version = \"0\"\nmeta_type = \"Patch\"\n\n[height_map]\nincluded = false\n",
             "land": [{"grid": [0, 0], "flags": 3, "heights": [400, 1, 1, 9, 20, 20, 6, 60], "colors": [10, 20, 30, 1]}]},
        ]),
    );
    for model in [DiffModel::Masters, DiffModel::LoadOrder] {
        let r = run(&data, &["Base.esm", "A.esp"], MergeOptions { dry_run: true, model, ..Default::default() });
        assert_eq!(r.cells_written, 1, "{model:?}: {:#?}", r.lines);
        let old = run(&data, &["Base.esm", "A.esp"], MergeOptions { dry_run: true, model, clean_against_load_order: false, ..Default::default() });
        assert_eq!(old.cells_written, 0, "{model:?}: the Python's cleaning drops it");
    }
    let _ = std::fs::remove_dir_all(data.parent().unwrap());

    // The output's own name, and the ignore list, are left out before anything is read.
    let data = setup(
        "own output",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "Merged Lands.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [0, 0, 0, 1, 0, 0, -1, 0]}]},
            {"name": "Merged Objects.esp"},
        ]),
    );
    let r = run(&data, &["Base.esm", "Merged Lands.esp", "Merged Objects.esp"], MergeOptions { dry_run: true, ..Default::default() });
    assert!(r.lines.contains(&"  left out 2 plugin(s): Merged Lands.esp, Merged Objects.esp".to_string()), "{:#?}", r.lines);
    assert_eq!(r.cells_written, 0);
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// A load order the size of a real one, timed by stage (`cargo test --release -- --ignored
/// --nocapture big_order`): 3,000 master cells and 300 mods of 30 cells each.
#[test]
#[ignore]
fn big_order() {
    use landcore::pipeline::DiffModel;
    use serde_json::json;
    let mut base = Vec::new();
    for x in -30..30 {
        for y in -25..25 {
            base.push(json!({"grid": [x, y], "flags": 15, "heights": [400, 1, 1, 9, 0, 0, -1, 0], "colors": [100, 90, 80, 1], "textures": [1, 3, 2], "world_map": "auto"}));
        }
    }
    let mut plugins = vec![json!({"name": "Base.esm", "ltex": [["a", 0, "a.dds"], ["b", 1, "b.dds"], ["c", 2, "c.dds"]], "land": base})];
    let mut order = vec!["Base.esm".to_string()];
    let mut seed = 7u64;
    let mut rnd = |n: i32| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) as i32).rem_euclid(n)
    };
    for m in 0..300 {
        let mut land = Vec::new();
        let (cx, cy) = (rnd(50) - 25, rnd(40) - 20);
        for k in 0..30 {
            let (x, y) = (cx + k % 6, cy + k / 6);
            land.push(json!({"grid": [x, y], "flags": 15, "heights": [400, 1, 1, 9, rnd(64), rnd(64), 3 + rnd(6), 20 + rnd(60)], "colors": [rnd(200), 90, 80, 1 + rnd(3)], "textures": [1, 2 + rnd(3), 2], "world_map": "auto"}));
        }
        let name = format!("Mod{m:03}.esp");
        plugins.push(json!({"name": name, "masters": ["Base.esm"], "land": land}));
        order.push(name);
    }
    let data = setup("big order", serde_json::Value::Array(plugins));
    let order_refs: Vec<&str> = order.iter().map(String::as_str).collect();
    let models: Vec<DiffModel> = if std::env::var("WG_MASTERS").is_ok() { vec![DiffModel::Masters] } else { vec![DiffModel::LoadOrder] };
    for model in models {
        let t = std::time::Instant::now();
        let order: Vec<String> = order_refs.iter().map(|s| s.to_string()).collect();
        // Verbose: the finish step's times in the log.
        let opts = MergeOptions { output: Some(data.parent().unwrap().join("out").join(DEFAULT_NAME)), model, verbose: true, ..Default::default() };
        let r = build_merged_lands(std::slice::from_ref(&data), &order, &opts, &mut |l| eprintln!("[{:>6.2}s] {l}", t.elapsed().as_secs_f32()));
        let hwm = std::fs::read_to_string("/proc/self/status").unwrap_or_default().lines().find(|l| l.starts_with("VmHWM")).unwrap_or("").to_string();
        let sha = {
            use sha2::Digest;
            let bytes = std::fs::read(opts.output.as_ref().unwrap()).unwrap_or_default();
            sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        eprintln!("{model:?}: {:.2}s, ok={}, {hwm}, sha256 {sha}", t.elapsed().as_secs_f32(), r.is_ok());
    }
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

#[test]
fn a_cancelled_merge_stops_and_writes_nothing() {
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let flat = json!([400, 1, 1, 9, 0, 0, -1, 0]);
    let data = setup(
        "cancelled",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 20, 20, 6, 60]}]},
            {"name": "B.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 48, 48, 5, 40]}]},
        ]),
    );
    let out = data.parent().unwrap().join("out").join(DEFAULT_NAME);
    let order: Vec<String> = ["Base.esm", "A.esp", "B.esp"].iter().map(|s| s.to_string()).collect();
    // Cancelled while the merge runs (as the page's Cancel does, from another thread).
    let stop = Arc::new(AtomicBool::new(false));
    let opts = MergeOptions { output: Some(out.clone()), cancel: Some(stop.clone()), ..Default::default() };
    let got = build_merged_lands(std::slice::from_ref(&data), &order, &opts, &mut |l| {
        if l == "merging ..." {
            stop.store(true, Ordering::Relaxed);
        }
    });
    match got {
        Err((e, lines)) => {
            assert_eq!(e, landcore::service::CANCELLED);
            assert!(lines.contains(&"merging ...".to_string()), "{lines:#?}");
        }
        Ok(r) => panic!("not cancelled: {:#?}", r.lines),
    }
    assert!(!out.exists(), "a cancelled merge wrote {}", out.display());
    // Not cancelled, the same merge writes.
    let opts = MergeOptions { output: Some(out.clone()), cancel: Some(Arc::new(AtomicBool::new(false))), ..Default::default() };
    build_merged_lands(std::slice::from_ref(&data), &order, &opts, &mut |_| {}).unwrap_or_else(|(e, _)| panic!("{e}"));
    assert!(out.exists());
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// WG-A01 (`TERRAIN_AUDIT_2026-11-11.md`): the heights are finalized to what `VHGT` stores
/// *before* the normals are worked out, so every normal the merge generated describes the
/// terrain saved. Two neighbours each bumped differently so that seam repair feathers a shared
/// border to heights off the eight-unit grid; read back from the plugin written, no vertex
/// whose height the merge moved may carry a normal other than the one its saved heights give.
#[test]
fn saved_normals_describe_the_saved_heights() {
    use serde_json::json;
    use tes3::esp::{Plugin, TES3Object};
    let flat = json!([400, 1, 1, 9, 0, 0, -1, 0]);
    let data = setup(
        "saved normals",
        json!([
            {"name": "Base.esm", "land": [
                {"grid": [0, 0], "flags": 1, "heights": flat}, {"grid": [1, 0], "flags": 1, "heights": flat}, {"grid": [2, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 62, 30, 9, 83]}]},
            {"name": "B.esp", "land": [{"grid": [1, 0], "flags": 1, "heights": [400, 1, 1, 9, 3, 30, 11, -61]}]},
        ]),
    );
    let order = ["Base.esm", "A.esp", "B.esp"];
    let r = run(&data, &order, MergeOptions { clean_against_load_order: false, ..Default::default() });
    assert!(r.seam_vertices > 0, "the case must make the seam repair move heights: {:#?}", r.lines);
    assert_eq!(r.clamped, 0);
    let out = data.parent().unwrap().join("out").join(DEFAULT_NAME);
    let plugin = Plugin::from_path(&out).unwrap();
    let base_heights: Vec<i32> = heights_of(&[400, 1, 1, 9, 0, 0, -1, 0]).iter().map(|v| v * 8).collect();
    let mut checked = 0;
    for object in &plugin.objects {
        let TES3Object::Landscape(l) = object else { continue };
        let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
        let heights = landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64).unwrap();
        // Decoding what was saved gives the finalized grid: nothing left for the encoder to move.
        assert!(heights.iter().all(|v| v % 8 == 0), "cell {:?}", l.grid);
        let want = normals_from_heights(&heights.iter().map(|&v| v as f64).collect::<Vec<_>>());
        for i in 0..SIZE * SIZE {
            let got: Vec<i32> = (0..3).map(|k| l.vertex_normals.data[i / SIZE][i % SIZE][k] as i32).collect();
            // The master's own normal is kept where the merge left the height as it was.
            if heights[i] == base_heights[i] {
                continue;
            }
            assert_eq!(got, want[i * 3..i * 3 + 3], "cell {:?} vertex {}: a normal that is not the saved heights'", l.grid, i);
            checked += 1;
        }
    }
    assert!(checked > 300, "{checked}");
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// WG-A02: what loads is the *last whole record* at a coordinate. B, after A, carries the same
/// heights but no vertex colours, so the engine draws A's colours away (white); the merge
/// restores them, so the cell must be written, with its colours, rather than dropped as if A's
/// colours still loaded under B's heights.
#[test]
fn a_later_record_that_omits_colors_is_what_loads() {
    use serde_json::json;
    use tes3::esp::{LandscapeFlags, Plugin, TES3Object};
    let flat = json!([400, 1, 1, 9, 0, 0, -1, 0]);
    let hill = json!([400, 1, 1, 9, 20, 20, 6, 60]);
    let data = setup(
        "later record omits colours",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "land": [{"grid": [0, 0], "flags": 3, "heights": hill, "colors": [10, 20, 30, 1]}]},
            {"name": "B.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": hill}]},
        ]),
    );
    let r = run(&data, &["Base.esm", "A.esp", "B.esp"], MergeOptions::default());
    assert_eq!((r.cells_written, r.dropped), (1, 0), "{:#?}", r.lines);
    let out = data.parent().unwrap().join("out").join(DEFAULT_NAME);
    let plugin = Plugin::from_path(&out).unwrap();
    let lands: Vec<_> = plugin.objects.iter().filter_map(|o| if let TES3Object::Landscape(l) = o { Some(l) } else { None }).collect();
    assert_eq!(lands.len(), 1);
    assert!(lands[0].landscape_flags.contains(LandscapeFlags::USES_VERTEX_COLORS));
    assert_eq!(lands[0].vertex_colors.data[0][0], [10, 20, 30]);
    let _ = std::fs::remove_dir_all(data.parent().unwrap());

    // A white restoration is no change to a cell whose later record has no colours: nothing to write.
    let data = setup(
        "white restoration",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "land": [{"grid": [0, 0], "flags": 3, "heights": hill, "colors": [255, 255, 255, 0]}]},
            {"name": "B.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": hill}]},
        ]),
    );
    let r = run(&data, &["Base.esm", "A.esp", "B.esp"], MergeOptions { dry_run: true, ..Default::default() });
    assert_eq!((r.cells_written, r.dropped), (0, 1), "{:#?}", r.lines);
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// WG-A05: the order decides, not the extension. `Late.esm` comes after `Early.esp`, so its
/// ground (64 units) is what loads over Early's (256); the merge must give it too.
#[test]
fn a_late_master_still_wins_over_an_earlier_mod() {
    use serde_json::json;
    use tes3::esp::{Plugin, TES3Object};
    let level = |stored: i32| json!([stored, 0, 0, 1, 0, 0, -1, 0]);
    let data = setup(
        "late master",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": level(0)}]},
            {"name": "Early.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": level(32)}]},
            {"name": "Late.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": level(8)}]},
        ]),
    );
    let r = run(&data, &["Base.esm", "Early.esp", "Late.esm"], MergeOptions { clean_against_load_order: false, ..Default::default() });
    assert_eq!(r.cells_written, 1, "{:#?}", r.lines);
    assert!(r.lines.contains(&"masters: Base.esm".to_string()), "{:#?}", r.lines);
    let plugin = Plugin::from_path(data.parent().unwrap().join("out").join(DEFAULT_NAME)).unwrap();
    let Some(TES3Object::Landscape(l)) = plugin.objects.iter().find(|o| matches!(o, TES3Object::Landscape(_))) else { panic!("no LAND") };
    let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
    let heights = landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64).unwrap();
    assert!(heights.iter().all(|&h| h == 64), "{:?}", &heights[..4]);
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// WG-A03: B, the last record at the cell, authors normals of its own over its heights. A's hill
/// is not in B's record, so the merge (both hills) differs from what loads and is written; the
/// normals B authored stay wherever the three heights each is made from are still B's, and are
/// worked out only round A's hill. The master's normals were once the only ones kept.
#[test]
fn authored_normals_of_the_winning_record_are_kept() {
    use serde_json::json;
    use tes3::esp::{Plugin, TES3Object};
    let flat = json!([400, 1, 1, 9, 0, 0, -1, 0]);
    let data = setup(
        "authored normals",
        json!([
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat}]},
            {"name": "A.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 20, 20, 6, 60]}]},
            {"name": "B.esp", "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 48, 48, 5, 40], "normals": [3, 4, 120]}]},
        ]),
    );
    let r = run(&data, &["Base.esm", "A.esp", "B.esp"], MergeOptions::default());
    assert_eq!(r.cells_written, 1, "{:#?}", r.lines);
    let plugin = Plugin::from_path(data.parent().unwrap().join("out").join(DEFAULT_NAME)).unwrap();
    let Some(TES3Object::Landscape(l)) = plugin.objects.iter().find(|o| matches!(o, TES3Object::Landscape(_))) else { panic!("no LAND") };
    let authored = |x: usize, y: usize| l.vertex_normals.data[y][x] == [3, 4, 120];
    let kept = (0..SIZE).flat_map(|y| (0..SIZE).map(move |x| (x, y))).filter(|&(x, y)| authored(x, y)).count();
    assert!(kept > 3500, "B's authored normals away from A's hill: {kept}");
    assert!(!authored(20, 20) && !authored(24, 20), "worked out where A's hill moved the heights");
    assert!(authored(60, 5) && authored(48, 48));
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// The seams the terrain already has, repaired only when asked (`MergeOptions::existing_seams`):
/// `Base.esm`'s own neighbouring cells (0,0) and (1,0) are 8 units apart along their border, (1,0)
/// and (2,0) 392, (5,5) and (6,5) 8 again but excepted; one mod changes a cell far from all of them.
#[test]
fn existing_seams_are_repaired_when_asked_and_only_within_the_ceiling() {
    use landcore::pipeline::ExistingSeams;
    use serde_json::json;
    use tes3::esp::{Plugin, TES3Object};
    let level = |v: i32| json!([v, 0, 0, 1, 0, 0, -1, 0]);
    let build = |name: &str| {
        setup(
            name,
            json!([
                {"name": "Base.esm", "land": [
                    {"grid": [0, 0], "flags": 1, "heights": level(400)}, {"grid": [1, 0], "flags": 1, "heights": level(401)},
                    {"grid": [2, 0], "flags": 1, "heights": level(450)},
                    {"grid": [5, 5], "flags": 1, "heights": level(400)}, {"grid": [6, 5], "flags": 1, "heights": level(401)},
                    {"grid": [20, 20], "flags": 1, "heights": level(400)}]},
                // Two mods, each with its own hill, so the merge is neither's record and is written.
                {"name": "A.esp", "land": [{"grid": [20, 20], "flags": 1, "heights": [400, 0, 0, 1, 20, 20, 6, 60]}]},
                {"name": "B.esp", "land": [{"grid": [20, 20], "flags": 1, "heights": [400, 0, 0, 1, 48, 48, 5, 40]}]},
            ]),
        )
    };
    let written = |data: &Path| -> std::collections::BTreeMap<(i32, i32), Vec<i32>> {
        let plugin = Plugin::from_path(data.parent().unwrap().join("out").join(DEFAULT_NAME)).unwrap();
        plugin
            .objects
            .iter()
            .filter_map(|o| if let TES3Object::Landscape(l) = o { Some(l) } else { None })
            .map(|l| {
                let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
                (l.grid, landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64).unwrap())
            })
            .collect()
    };
    let order = ["Base.esm", "A.esp", "B.esp"];
    // Off (the default): only the cell the mod changed.
    let data = build("existing seams off");
    let r = run(&data, &order, MergeOptions::default());
    assert_eq!(written(&data).keys().copied().collect::<Vec<_>>(), vec![(20, 20)], "{:#?}", r.lines);
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
    // On, up to 24 units, (5,5) left alone.
    let data = build("existing seams on");
    let rule = ExistingSeams { max_gap: 24, except: [(5, 5)].into_iter().collect() };
    let r = run(&data, &order, MergeOptions { existing_seams: Some(rule), ..Default::default() });
    let cells = written(&data);
    // (1,0) is written, taken down to (0,0)'s 400 stored units (the shared vertices settle on the
    // 8-unit step between the two, and (0,0) already has it, so it needs no record); (2,0), 392 units
    // away, and the excepted (5,5)/(6,5) are not touched.
    assert_eq!(cells.keys().copied().collect::<Vec<_>>(), vec![(1, 0), (20, 20)], "{:#?}", r.lines);
    assert_eq!(r.clamped, 0);
    let east = &cells[&(1, 0)];
    assert!((1..SIZE - 1).all(|y| east[y * SIZE] == 400 * 8), "the shared border agrees with (0,0)'s");
    // Its two end vertices touch cells the merge leaves out: they stay as the terrain has them.
    assert!(east[0] == 401 * 8 && east[(SIZE - 1) * SIZE] == 401 * 8);
    // The far side of (1,0), against terrain the merge leaves out, is the terrain's own.
    assert!((1..SIZE - 1).all(|y| east[y * SIZE + SIZE - 1] == 401 * 8));
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}

/// A seam against a cell the merge only borrowed: (20,20) is the merged cell, (21,20) its borrowed
/// neighbour (held fixed), and (22,20) is 8 units off (21,20) and no neighbour of the merged cell. With
/// existing seams on, (22,20) is brought in to meet (21,20) - which does not move.
#[test]
fn a_seam_beyond_a_borrowed_cell_is_repaired_to_meet_it() {
    use landcore::pipeline::ExistingSeams;
    use serde_json::json;
    use tes3::esp::{Plugin, TES3Object};
    let level = |v: i32| json!([v, 0, 0, 1, 0, 0, -1, 0]);
    let data = setup(
        "seam beyond borrowed",
        json!([
            {"name": "Base.esm", "land": [
                {"grid": [20, 20], "flags": 1, "heights": level(400)}, {"grid": [21, 20], "flags": 1, "heights": level(400)},
                {"grid": [22, 20], "flags": 1, "heights": level(401)}]},
            {"name": "A.esp", "land": [{"grid": [20, 20], "flags": 1, "heights": [400, 0, 0, 1, 20, 20, 6, 60]}]},
            {"name": "B.esp", "land": [{"grid": [20, 20], "flags": 1, "heights": [400, 0, 0, 1, 48, 48, 5, 40]}]},
        ]),
    );
    let order = ["Base.esm", "A.esp", "B.esp"];
    let heights = |data: &Path| {
        let plugin = Plugin::from_path(data.parent().unwrap().join("out").join(DEFAULT_NAME)).unwrap();
        plugin
            .objects
            .iter()
            .filter_map(|o| if let TES3Object::Landscape(l) = o { Some(l) } else { None })
            .map(|l| {
                let raw: Vec<u8> = l.vertex_heights.data.iter().flatten().map(|&d| d as u8).collect();
                (l.grid, landcore::heights::decode_heights(&raw, l.vertex_heights.offset as f64).unwrap())
            })
            .collect::<std::collections::BTreeMap<(i32, i32), Vec<i32>>>()
    };
    let r = run(&data, &order, MergeOptions { existing_seams: Some(ExistingSeams::default()), ..Default::default() });
    let cells = heights(&data);
    assert_eq!(cells.keys().copied().collect::<Vec<_>>(), vec![(20, 20), (22, 20)], "{:#?}", r.lines);
    let far = &cells[&(22, 20)];
    assert!((0..SIZE).all(|y| far[y * SIZE] == 400 * 8), "(22,20) meets the borrowed (21,20), which stayed at 400");
    let _ = std::fs::remove_dir_all(data.parent().unwrap());
}
