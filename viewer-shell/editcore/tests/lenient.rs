//! Lenient plugin reading (`plugin::set_lenient`, Settings' "Read past unknown subrecords"):
//! a record with a subrecord its type does not know is refused strictly, read leniently with
//! that subrecord kept and written back where it was, and where keeping it cannot be proven,
//! marked and never written. One test, because the switch is the process's and the tests of a
//! file run side by side.

use editcore::mergeobjects::{Options, merge};
use editcore::plugin;

fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    [&tag[..], &u32::try_from(data.len()).unwrap().to_le_bytes(), data].concat()
}

fn record(body: &[u8]) -> Vec<u8> {
    [&b"NPC_"[..], &u32::try_from(body.len()).unwrap().to_le_bytes(), &[0; 8], body].concat()
}

/// `NPC_ noise` named `name` with only its id and name, and an unknown `FILL` between them
/// when `fill`. The crate writes such a record back whole (its data, AI and the rest), so
/// with a FILL it is one whose keeping cannot be proven.
fn bare(name: &str, fill: bool) -> Vec<u8> {
    let mut body = sub(b"NAME", b"noise\0");
    if fill {
        body.extend(sub(b"FILL", &[7; 40]));
    }
    body.extend(sub(b"FNAM", format!("{name}\0").as_bytes()));
    record(&body)
}

/// `NPC_ noise` named `name` as the crate writes it, as a real plugin's NPC is, with an
/// unknown `FILL` after its `NAME` when `fill`.
fn npc(name: &str, fill: bool) -> Vec<u8> {
    let whole = plugin::records_bytes(&plugin::records_json(&bare(name, false), None).unwrap()).unwrap();
    if !fill {
        return whole;
    }
    let after_name = 16 + 8 + 6;
    let body = [&whole[16..after_name], &sub(b"FILL", &[7; 40])[..], &whole[after_name..]].concat();
    record(&body)
}

fn name_of(v: &serde_json::Value) -> &str {
    v["name"].as_str().unwrap_or("")
}

#[test]
fn lenient_reading_keeps_what_it_steps_over_and_writes_it_back() {
    let bytes = npc("Noise", true);

    // Strict, the default: the record and the plugin are refused.
    assert!(!plugin::lenient());
    assert!(plugin::records_json(&bytes, None).is_err());
    assert!(plugin::read(&bytes).is_err());
    assert!(plugin::record_value(&bytes).is_err());

    // Lenient: read past the FILL, which is kept, with the subrecord it followed.
    plugin::set_lenient(true);
    let json = plugin::records_json(&bytes, None).unwrap();
    let values: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert_eq!(name_of(&values[0]), "Noise");
    assert!(plugin::lossy(&values[0]).is_none(), "{json}");
    let kept = &values[0][plugin::UNKNOWN];
    assert_eq!((kept[0]["tag"].as_str(), kept[0]["after"].as_str()), (Some("FILL"), Some("NAME")), "{json}");
    assert_eq!(plugin::read(&bytes).unwrap().objects.len(), 1);
    assert_eq!(plugin::record_value(&bytes).unwrap()[plugin::UNKNOWN], *kept);
    assert_eq!(plugin::values(&bytes, None, false).unwrap()[0][plugin::UNKNOWN], *kept);
    // A record that reads strictly is not marked.
    let plain = plugin::records_json(&npc("Noise", false), None).unwrap();
    assert!(!plain.contains(plugin::LOSSY) && !plain.contains(plugin::UNKNOWN));

    // Written back as it was, byte for byte; and changed, with the FILL still after its NAME.
    assert_eq!(plugin::records_bytes(&json).unwrap(), bytes);
    let mut edited = values.clone();
    edited[0]["name"] = "Noisy".into();
    assert_eq!(plugin::records_bytes(&serde_json::Value::Array(edited).to_string()).unwrap(), npc("Noisy", true));

    // Where keeping it cannot be proven (the crate writes back more than the record had), the
    // record is marked instead, and the patch writer refuses it, naming it and why.
    let odd = bare("Noise", true);
    let json = plugin::records_json(&odd, None).unwrap();
    let values: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(plugin::lossy(&values[0]).unwrap().contains("FILL"), "{json}");
    let err = plugin::records_bytes(&json).unwrap_err().to_string();
    assert!(err.contains("noise") && err.contains("FILL"), "{err}");

    // Merge objects: the last version's kept FILL stays with the merged record ...
    let dir = std::env::temp_dir().join(format!("wg_lenient_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let put = |name: &str, bytes: Vec<u8>| {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        (name.to_string(), p)
    };
    let opts = Options::default();
    let written = |out: &editcore::mergeobjects::Outcome| plugin::records_bytes(&serde_json::Value::Array(out.records.clone()).to_string()).unwrap();
    let last_kept = [put("a.esp", npc("Noise", false)), put("b.esp", npc("Noisy", false)), put("c.esp", npc("Noise", true))];
    let out = merge(&last_kept, &opts);
    assert_eq!(out.records.len(), 1, "{:?}", out.lines);
    assert_eq!(written(&out), npc("Noisy", true));
    // ... an earlier version's is not merged ...
    let middle_kept = [put("a.esp", npc("Noise", false)), put("b.esp", npc("Noisy", true)), put("c.esp", npc("Noise", false))];
    let out = merge(&middle_kept, &opts);
    assert_eq!(written(&out), npc("Noisy", false));
    // ... and a last version that lost what it stepped over leaves the record out.
    let last_lost = [put("a.esp", npc("Noise", false)), put("b.esp", npc("Noisy", false)), put("c.esp", bare("Noise", true))];
    let out = merge(&last_lost, &opts);
    assert!(out.records.is_empty(), "{:?}", out.lines);
    assert_eq!(out.failed, ["NPC_ noise"]);
    assert!(out.lines.iter().any(|l| l.contains("Left out") && l.contains("c.esp")), "{:?}", out.lines);

    plugin::set_lenient(false);
    let _ = std::fs::remove_dir_all(&dir);
}
