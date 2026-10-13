//! Wraithguard (carried patch 0004): the ten weathers are opt-in. The plain readers keep
//! round 17h's Clear only; the `*_with` readers given `ALL_WEATHERS` read every block.

use gardenfell_bytes::json::parse;
use gardenfell_install::weather::{ini_json, ini_json_with, openmw_to_ini_with, ALL_WEATHERS};

const INI: &str = "[Weather]\nSunrise Time=6\n\
[Weather Clear]\nCloud Speed=1.25\nGlare View=1\n\
[Weather Foggy]\nCloud Speed=1.25\nGlare View=0.25\n\
[Weather Blizzard]\nCloud Speed=7.5\nGlare View=0\n";

#[test]
fn the_plain_reader_is_still_clear_only() {
    let v = parse(&ini_json(INI)).unwrap();
    let w = v.get("weathers").unwrap();
    assert!(w.get("Clear").is_some());
    assert!(w.get("Foggy").is_none() && w.get("Blizzard").is_none(), "round 17h unchanged");
}

#[test]
fn all_weathers_reads_every_block_and_its_glare_view() {
    let v = parse(&ini_json_with(INI, &ALL_WEATHERS)).unwrap();
    let w = v.get("weathers").unwrap();
    let glare = |n: &str| w.get(n).unwrap().get("glareView").unwrap().as_f64().unwrap();
    assert_eq!(glare("Clear"), 1.0);
    assert_eq!(glare("Foggy"), 0.25);
    assert_eq!(glare("Blizzard"), 0.0);
    assert!(w.get("Rain").is_none(), "a weather the ini has no block for is left out");
}

#[test]
fn openmw_fallbacks_go_to_their_own_weather() {
    let cfg = "fallback=Weather_Clear_Cloud_Speed,1.25\nfallback=Weather_Cloudy_Glare_View,0.5\n";
    let ini = openmw_to_ini_with(cfg, &ALL_WEATHERS).unwrap();
    assert!(ini.contains("[Weather Cloudy]\nGlare View=0.5"), "{ini}");
}
