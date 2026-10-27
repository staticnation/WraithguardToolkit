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
const SIZE: usize = 65;

fn ints(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

fn heights_of(r: &[i64]) -> Vec<i64> {
    let (base, a, b, m, cx, cy, rad, bump) = (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7]);
    let mut out = Vec::with_capacity(SIZE * SIZE);
    for y in 0..SIZE as i64 {
        for x in 0..SIZE as i64 {
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
    let mut body = sub(b"INTV", &[(g[0] as i32).to_le_bytes(), (g[1] as i32).to_le_bytes()].concat());
    if let Some(f) = r.get("flags") {
        body.extend(sub(b"DATA", &(f.as_u64().unwrap() as u32).to_le_bytes()));
    }
    let stored = r.get("heights").map(|h| heights_of(&ints(h)));
    let world: Option<Vec<i64>> = stored.as_ref().map(|s| s.iter().map(|v| v * 8).collect());
    if let (Some(stored), Some(world)) = (&stored, &world) {
        let normals: Vec<i64> = if r.get("normals").and_then(Value::as_str).unwrap_or("auto") == "auto" {
            normals_from_heights(&world.iter().map(|&v| v as f64).collect::<Vec<_>>())
        } else {
            vec![0; 3 * SIZE * SIZE]
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
        let values: Vec<i64> = match wm.as_str() {
            Some("auto") => world_map_from_heights(world.as_ref().unwrap()),
            Some(_) => vec![0; 81],
            None => ints(wm),
        };
        body.extend(sub(b"WNAM", &values.iter().map(|&v| v as i8 as u8).collect::<Vec<_>>()));
    }
    if let Some(c) = r.get("colors") {
        let c = ints(c);
        let mut data = Vec::new();
        for y in 0..SIZE as i64 {
            for x in 0..SIZE as i64 {
                data.extend([(c[0] + (x * c[3]).rem_euclid(50)).rem_euclid(256) as u8, (c[1] + (y * c[3]).rem_euclid(50)).rem_euclid(256) as u8, c[2].rem_euclid(256) as u8]);
            }
        }
        body.extend(sub(b"VCLR", &data));
    }
    if let Some(t) = r.get("textures") {
        let t = ints(t);
        let rows: Vec<i64> = (0..256).map(|i| if ((i % 16) + (i / 16)) % t[1] == 0 { t[2] } else { t[0] }).collect();
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
    for case in doc["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let expect = &case["expect"];
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
            ..Default::default()
        };
        let mut said = Vec::new();
        let got = build_merged_lands(std::slice::from_ref(&data), &order, &opts, &mut |l| said.push(l.to_string()));
        let rootstr = root.display().to_string();
        let un = |lines: &[String]| -> Vec<String> { lines.iter().map(|l| l.replace(&rootstr, "<root>").replace('\\', "/")).collect() };
        let want_lines: Vec<String> = expect["lines"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
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
