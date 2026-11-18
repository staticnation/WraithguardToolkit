//! Lenient plugin reading (`plugin::set_lenient`, Settings' "Read past unknown subrecords"):
//! a record with a subrecord its type does not know is refused strictly, read leniently and
//! marked, and no writer drops the subrecord silently. One test, because the switch is the
//! process's and the tests of a file run side by side.

use editcore::mergeobjects::{Options, merge};
use editcore::plugin;

fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    [&tag[..], &u32::try_from(data.len()).unwrap().to_le_bytes(), data].concat()
}

/// `NPC_ noise` named `name`, with an unknown `FILL` between its subrecords when `fill`.
fn npc(name: &str, fill: bool) -> Vec<u8> {
    let mut body = sub(b"NAME", b"noise\0");
    if fill {
        body.extend(sub(b"FILL", &[7; 40]));
    }
    body.extend(sub(b"FNAM", format!("{name}\0").as_bytes()));
    [&b"NPC_"[..], &u32::try_from(body.len()).unwrap().to_le_bytes(), &[0; 8], &body].concat()
}

fn name_of(v: &serde_json::Value) -> &str {
    v["name"].as_str().unwrap_or("")
}

#[test]
fn lenient_reading_marks_what_it_steps_over_and_nothing_writes_it() {
    let bytes = npc("Noise", true);

    // Strict, the default: the record and the plugin are refused.
    assert!(!plugin::lenient());
    assert!(plugin::records_json(&bytes, None).is_err());
    assert!(plugin::read(&bytes).is_err());
    assert!(plugin::record_value(&bytes).is_err());

    // Lenient: read past the FILL, and marked with the strict read's error.
    plugin::set_lenient(true);
    let json = plugin::records_json(&bytes, None).unwrap();
    let values: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert_eq!(name_of(&values[0]), "Noise");
    assert!(plugin::lossy(&values[0]).unwrap().contains("FILL"), "{json}");
    assert_eq!(plugin::read(&bytes).unwrap().objects.len(), 1);
    assert!(plugin::lossy(&plugin::record_value(&bytes).unwrap()).is_some());
    assert!(plugin::lossy(&plugin::values(&bytes, None, false).unwrap()[0]).is_some());
    // A record that reads strictly is not marked.
    let plain = plugin::records_json(&npc("Noise", false), None).unwrap();
    assert!(!plain.contains(plugin::LOSSY));

    // The patch writer refuses a marked record, naming it and why.
    let err = plugin::records_bytes(&json).unwrap_err().to_string();
    assert!(err.contains("noise") && err.contains("FILL"), "{err}");
    assert!(plugin::records_bytes(&plain).is_ok());

    // Merge objects: a record whose last version was read leniently is left out ...
    let dir = std::env::temp_dir().join(format!("wg_lenient_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let put = |name: &str, bytes: Vec<u8>| {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        (name.to_string(), p)
    };
    let opts = Options::default();
    let last_marked = [put("a.esp", npc("Noise", false)), put("b.esp", npc("Noisy", false)), put("c.esp", npc("Noise", true))];
    let out = merge(&last_marked, &opts);
    assert!(out.records.is_empty(), "{:?}", out.lines);
    assert_eq!(out.failed, ["NPC_ noise"]);
    assert!(out.lines.iter().any(|l| l.contains("Left out") && l.contains("c.esp")), "{:?}", out.lines);
    // ... and one read so earlier in the order is merged from, the merged record unmarked.
    let middle_marked = [put("a.esp", npc("Noise", false)), put("b.esp", npc("Noisy", true)), put("c.esp", npc("Noise", false))];
    let out = merge(&middle_marked, &opts);
    assert_eq!(out.records.len(), 1, "{:?}", out.lines);
    assert_eq!(name_of(&out.records[0]), "Noisy");
    assert!(plugin::lossy(&out.records[0]).is_none());

    plugin::set_lenient(false);
    let _ = std::fs::remove_dir_all(&dir);
}
