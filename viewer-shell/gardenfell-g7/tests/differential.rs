//! G7's crate plus the conversion against Robin's own reader, record for record: every
//! record of a kind the conversion covers, in every plugin found, must come out equal.
//!
//! Reads the repository's fixture plugins, and every plugin in the folders named by
//! `GARDENFELL_G7_PLUGINS` (separated as the system separates paths), so a real install's
//! `Data Files` can be checked: `GARDENFELL_G7_PLUGINS="C:\Games\Morrowind\Data Files"`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gardenfell_tes3::tes3::raw;
use gardenfell_tes3::tes3::types::Object;

fn plugins_in(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "esm" | "esp" | "omwaddon" | "omwgame") {
            out.push(p);
        }
    }
}

#[test]
fn g7_and_the_conversion_type_every_record_as_robins_reader_does() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for d in [
        "../ui/tests/fixture/Data Files",
        "../../tests/fixtures/mwedit",
    ] {
        plugins_in(&here.join(d), &mut files);
    }
    if let Some(extra) = std::env::var_os("GARDENFELL_G7_PLUGINS") {
        for d in std::env::split_paths(&extra) {
            plugins_in(&d, &mut files);
        }
    }
    assert!(!files.is_empty(), "no plugins to check");
    // kind -> (records compared, mismatches, first mismatch)
    let mut tally: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
    for f in &files {
        let buf = std::fs::read(f).unwrap();
        let Ok(recs) = raw::parse_plugin(&buf) else {
            continue;
        };
        for rec in recs {
            let ours = gardenfell_g7::records::type_record(&rec);
            let robins = Object::from_raw(&rec).map_err(|e| e.to_string());
            if let (Ok(None), Ok(None)) = (&ours, &robins) {
                continue;
            }
            let e = tally.entry(rec.tag_str()).or_default();
            e.0 += 1;
            let same = match (&ours, &robins) {
                (Ok(a), Ok(b)) => a == b,
                (Err(_), Err(_)) => true,
                _ => false,
            };
            if !same {
                e.1 += 1;
                if e.2.is_empty() {
                    let a = format!("{ours:?}");
                    let b = format!("{robins:?}");
                    let at = a
                        .bytes()
                        .zip(b.bytes())
                        .position(|(x, y)| x != y)
                        .unwrap_or(0);
                    let from = at.saturating_sub(60);
                    e.2 = format!(
                        "{}: G7 …{}… | Robin …{}…",
                        f.file_name().unwrap().to_string_lossy(),
                        a.get(from..(at + 100).min(a.len())).unwrap_or(""),
                        b.get(from..(at + 100).min(b.len())).unwrap_or("")
                    );
                }
            }
        }
    }
    let mut report = format!("{} plugins\n", files.len());
    let mut bad = 0;
    for (k, (n, m, first)) in &tally {
        report.push_str(&format!("{k}: {n} records, {m} differ\n"));
        if *m > 0 {
            bad += m;
            report.push_str(&format!("    {first}\n"));
        }
    }
    println!("{report}");
    assert_eq!(bad, 0, "\n{report}");
}
