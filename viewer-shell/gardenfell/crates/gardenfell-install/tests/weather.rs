//! The game's sky, read from the install — round 13.
//!
//! `fixture/weather` is cut from Robin's own files: the `[Weather]` timings and two
//! weather blocks of his MO2 profile's Morrowind.ini, two of Ctroost's Weather Adjuster
//! presets, and the fog half of his `mge3\MGE.ini`. The reply is what the page
//! interpolates, so what is checked is that the numbers arrive as the files spell them.

use gardenfell_bytes::json::{parse, Val};
use gardenfell_install::vfs::Vfs;
use gardenfell_install::weather::{adjuster_json, atmosphere_json, ini_json, mge_json};

/// Round 17m: the fog reader takes the whole layout now, because *which* file the
/// settings live in depends on the MGE install — and under Root Builder the d3d8.dll that
/// decides is not in the game folder at all. `mge.rs` and `tests/mge.rs` cover that; here
/// the layout is just a game folder.
fn at(game: &std::path::Path) -> gardenfell_install::layout::Layout {
    gardenfell_install::layout::Layout { game_dir: Some(game.to_path_buf()), ..Default::default() }
}
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixture/weather")
}

fn f(v: &Val, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(f64::NAN)
}
fn rgb(v: &Val) -> [f64; 3] {
    let a = v.as_arr().expect("a colour");
    [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
}

#[test]
fn the_ini_ramp_arrives_as_written() {
    let text = std::fs::read_to_string(fixture().join("Morrowind.ini")).unwrap();
    let v = parse(&ini_json(&text)).expect("json");
    let t = v.get("timing").unwrap();
    assert_eq!(f(t, "sunrise"), 6.0);
    assert_eq!(f(t, "sunset"), 18.0);
    assert_eq!(f(t, "sunriseDuration"), 2.0);
    assert_eq!(f(t, "skyPreSunset"), 1.5);
    assert_eq!(f(t, "ambientPostSunset"), 1.25);
    assert_eq!(f(t, "fogPreSunset"), 2.0);
    assert_eq!(f(t, "sunPostSunset"), 1.25);
    let clear = v.get("weathers").unwrap().get("Clear").expect("Clear");
    // Sky Day Color=095,135,203 → sunrise, day, sunset, night in that order.
    let sky = clear.get("sky").unwrap().as_arr().unwrap();
    let day = rgb(&sky[1]);
    assert!((day[0] - 95.0 / 255.0).abs() < 1e-3 && (day[2] - 203.0 / 255.0).abs() < 1e-3, "{day:?}");
    let night = rgb(&sky[3]);
    assert!((night[0] - 9.0 / 255.0).abs() < 1e-3);
    let sun = clear.get("sun").unwrap().as_arr().unwrap();
    let sunset = rgb(&sun[2]);
    assert!((sunset[1] - 114.0 / 255.0).abs() < 1e-3, "Sun Sunset Color=255,114,079: {sunset:?}");
    assert!((f(clear, "fogDepthDay") - 0.69).abs() < 1e-4);
    assert_eq!(clear.get("cloud").unwrap().as_str().unwrap(), "Tx_Sky_Clear.tga");
    /* Round 17h: Clear and only Clear. The other nine ramps, the wind, the rain and snow
       blocks and the storm keys went with the weather effects (Robin: "Remove all
       weathers except Clear from the program"), so a block for any other weather is not
       read even when the ini has one. */
    assert!(v.get("weathers").unwrap().get("Cloudy").is_none(), "Clear only, round 17h");
    assert!(v.get("weathers").unwrap().get("Rain").is_none());
    assert!(clear.get("wind").is_none() && clear.get("rain").is_none() && clear.get("storm").is_none(),
            "no wind, no precipitation, no storm: {clear:?}");
    assert!((f(clear, "cloudSpeed") - 1.25).abs() < 1e-4, "Cloud Speed is still read");


}

#[test]
fn openmw_cfg_fallback_lines_are_the_same_ramp() {
    // Round 14: an OpenMW install has no Morrowind.ini; the launcher copied the values
    // into openmw.cfg as `fallback=Weather_<Weather>_<Key>,<value>` lines.
    let text = std::fs::read_to_string(fixture().join("openmw.cfg")).unwrap();
    let v = parse(&ini_json(&text)).expect("json");
    assert_eq!(v.get("openmw").unwrap().as_bool(), Some(true));
    assert_eq!(v.get("found").unwrap().as_bool(), Some(true));
    let t = v.get("timing").unwrap();
    assert_eq!(f(t, "skyPreSunrise"), 0.5, "`.5` parses");
    assert_eq!(f(t, "ambientPostSunset"), 1.25);
    let clear = v.get("weathers").unwrap().get("Clear").expect("Clear");
    let day = rgb(&clear.get("sky").unwrap().as_arr().unwrap()[1]);
    assert!((day[0] - 95.0 / 255.0).abs() < 1e-3 && (day[2] - 203.0 / 255.0).abs() < 1e-3, "{day:?}");
    let sunset = rgb(&clear.get("sun").unwrap().as_arr().unwrap()[2]);
    assert!((sunset[1] - 114.0 / 255.0).abs() < 1e-3, "{sunset:?}");
    assert!((f(clear, "fogDepthDay") - 0.69).abs() < 1e-4);
    assert_eq!(clear.get("cloud").unwrap().as_str().unwrap(), "Tx_Sky_Clear.tga");
    // Round 17h: Clear only, whichever file the fallbacks came from.
    assert!(v.get("weathers").unwrap().get("Cloudy").is_none(), "Clear only");
    assert!(v.get("weathers").unwrap().get("Foggy").is_none());

    // A plain ini is not mistaken for openmw.cfg, and an empty file is found:false.
    let ini = std::fs::read_to_string(fixture().join("Morrowind.ini")).unwrap();
    let v = parse(&ini_json(&ini)).unwrap();
    assert_eq!(v.get("openmw").unwrap().as_bool(), Some(false));
    assert_eq!(v.get("found").unwrap().as_bool(), Some(true));
    let v = parse(&ini_json("")).unwrap();
    assert_eq!(v.get("found").unwrap().as_bool(), Some(false));
    assert_eq!(f(v.get("timing").unwrap(), "sunrise"), 6.0, "defaults stand in");
}

/// Round 18dt: the `[Water]` block, which every renderer's underwater fog is made of - the
/// colour and its weight, the four fog amounts by time of day and the indoor one - from the
/// ini and from the same values as `fallback=Water_…` lines. Vanilla's numbers stand behind
/// a missing block, and `found` says which it was.
#[test]
fn the_underwater_fog_block_is_read_from_both_files() {
    let text = std::fs::read_to_string(fixture().join("Morrowind.ini")).unwrap();
    let v = parse(&ini_json(&text)).expect("json");
    let w = v.get("water").expect("water");
    assert_eq!(w.get("found").unwrap().as_bool(), Some(true));
    let c = rgb(w.get("underwaterColor").unwrap());
    assert!((c[0] - 12.0 / 255.0).abs() < 1e-3 && (c[1] - 30.0 / 255.0).abs() < 1e-3 && (c[2] - 37.0 / 255.0).abs() < 1e-3, "{c:?}");
    assert!((f(w, "colorWeight") - 0.85).abs() < 1e-4);
    assert_eq!(f(w, "sunriseFog"), 3.0);
    assert_eq!(f(w, "dayFog"), 2.5);
    assert_eq!(f(w, "sunsetFog"), 3.0);
    assert_eq!(f(w, "nightFog"), 4.0);
    assert_eq!(f(w, "indoorFog"), 3.0);

    let cfg = std::fs::read_to_string(fixture().join("openmw.cfg")).unwrap();
    let v = parse(&ini_json(&cfg)).expect("json");
    let w = v.get("water").expect("water");
    assert_eq!(w.get("found").unwrap().as_bool(), Some(true), "the fallback lines are the block");
    assert_eq!(f(w, "nightFog"), 4.0);
    assert!((f(w, "colorWeight") - 0.85).abs() < 1e-4);
    let c = rgb(w.get("underwaterColor").unwrap());
    assert!((c[2] - 37.0 / 255.0).abs() < 1e-3, "{c:?}");

    // No block at all: vanilla's own numbers, and found:false so the page can say so.
    let v = parse(&ini_json("[Weather]\nSunrise Time=6\n")).unwrap();
    let w = v.get("water").unwrap();
    assert_eq!(w.get("found").unwrap().as_bool(), Some(false));
    assert_eq!(f(w, "dayFog"), 2.5);
    assert!((f(w, "colorWeight") - 0.85).abs() < 1e-4);
}

#[test]
fn weather_adjuster_is_found_through_the_overlay_and_passed_on() {
    let roots = vec![("data".to_string(), fixture().join("Data Files"))];
    let vfs = Vfs::with_roots(&roots);
    let v = parse(&adjuster_json(&vfs)).expect("json");
    assert_eq!(v.get("regions").unwrap().get("Grazelands Region").unwrap().as_str().unwrap(), "Exotic");
    let exotic = v.get("presets").unwrap().get("Exotic").expect("preset");
    let out = rgb(exotic.get("outscatter").unwrap());
    assert!((out[0] - 0.07).abs() < 1e-3 && (out[2] - 0.76).abs() < 1e-3, "{out:?}");
    assert!((f(exotic, "skylightScatterMix") - 0.44).abs() < 1e-3);
    let clear = exotic.get("Clear").unwrap();
    let sky = rgb(clear.get("skyDayColor").unwrap());
    assert!((sky[0] - 0.4634).abs() < 1e-3, "{sky:?}");

    // No file: null, not an error — most installs have no Weather Adjuster.
    let bare = Vfs::with_roots(&[("x".to_string(), fixture().join("mge3"))]);
    assert_eq!(adjuster_json(&bare), "null");
}

#[test]
fn mge_ini_fog_settings_are_read_with_defaults_behind_them() {
    let v = parse(&mge_json(&at(&fixture()))).expect("json");
    assert_eq!(v.get("found").unwrap().as_bool(), Some(true));
    assert_eq!(f(&v, "fogStart"), 2.0);
    assert_eq!(f(&v, "fogEnd"), 5.0);
    // Round 18dt: the below-water pair, as the fixture's MGE.ini spells it.
    assert!((f(&v, "fogBelowStart") + 0.10).abs() < 1e-5, "{}", f(&v, "fogBelowStart"));
    assert!((f(&v, "fogBelowEnd") - 0.90).abs() < 1e-5);
    assert_eq!(v.get("expFog").unwrap().as_bool(), Some(true));
    assert_eq!(v.get("scattering").unwrap().as_bool(), Some(true));
    let w = v.get("weathers").unwrap();
    // Round 17h: one weather, one ratio.
    assert!(w.get("Foggy").is_none() && w.get("Blizzard").is_none(), "Clear only");
    assert_eq!(f(w.get("Clear").unwrap(), "fogRatio"), 1.0);
    assert_eq!(f(w.get("Clear").unwrap(), "fogOffset"), 0.0);

    let none = parse(&mge_json(&gardenfell_install::layout::Layout::default())).unwrap();
    assert_eq!(none.get("found").unwrap().as_bool(), Some(false));
    assert_eq!(f(&none, "fogEnd"), 5.0, "MGE's default when there is no ini");
    assert!((f(&none, "fogBelowStart") + 0.5).abs() < 1e-5 && (f(&none, "fogBelowEnd") - 0.3).abs() < 1e-5, "MGE's below-water defaults");
}

#[test]
fn the_whole_reply_is_one_object() {
    let roots = vec![("data".to_string(), fixture().join("Data Files"))];
    let vfs = Vfs::with_roots(&roots);
    let v = parse(&atmosphere_json(&fixture().join("Morrowind.ini"), &vfs, &at(&fixture()))).expect("json");
    assert!(v.get("ini").is_some() && v.get("adjuster").is_some() && v.get("mge").is_some());
}

/// Round 18fc: Wonders of Water, found through the overlay, its config in the manager's
/// overwrite folder when there is one and the mod's own defaults when there is not.
#[test]
fn wonders_of_water_is_found_through_the_overlay_and_its_config_read_from_the_overwrite() {
    use gardenfell_install::weather::wonders_json;
    let layout = at(&fixture());
    // The mod alone: defaults - enabled, clarity on - and no config file named.
    let roots = vec![("data".to_string(), fixture().join("Data Files"))];
    let vfs = Vfs::with_roots(&roots);
    let v = parse(&wonders_json(&vfs, &layout)).expect("json");
    assert_eq!(v.get("found").unwrap().as_bool(), Some(true));
    assert_eq!(v.get("enabled").unwrap().as_bool(), Some(true));
    assert_eq!(v.get("waterClarity").unwrap().as_bool(), Some(true));
    assert!(v.get("configFile").unwrap().as_str().is_none(), "no config: the defaults");
    assert_eq!(v.get("depthUnits").unwrap().as_f64(), Some(1500.0));
    assert!((v.get("minVision").unwrap().as_f64().unwrap() - 0.1).abs() < 1e-9);
    assert!(v.get("modFile").unwrap().as_str().unwrap().to_ascii_lowercase().ends_with("main.lua"));

    // With the overwrite folder first in the overlay: the person's choices win.
    let over = fixture().parent().unwrap().join("wow").join("overwrite");
    let roots2 = vec![("overwrite".to_string(), over.clone()), ("data".to_string(), fixture().join("Data Files"))];
    let vfs2 = Vfs::with_roots(&roots2);
    let v2 = parse(&wonders_json(&vfs2, &layout)).expect("json");
    assert_eq!(v2.get("waterClarity").unwrap().as_bool(), Some(false), "the config turned clarity off");
    assert_eq!(v2.get("enabled").unwrap().as_bool(), Some(true));
    assert!(v2.get("configFile").unwrap().as_str().unwrap().to_ascii_lowercase().ends_with("wonders of water.json"));

    // Not installed: null, like Weather Adjuster.
    let bare = Vfs::with_roots(&[("x".to_string(), fixture().join("mge3"))]);
    assert_eq!(wonders_json(&bare, &layout), "null");
}
