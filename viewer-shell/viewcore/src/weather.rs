//! The game's sky — round 13.
//!
//! Robin: the atmosphere should look like MGE XE's. Read from G7's branch of MGE XE
//! (`XE Common.fx`, `distantland.cpp`), what MGE draws is not a physical sky at all: it
//! is **the game's own weather colour ramp** — sky, fog, ambient and sun colours at
//! sunrise, day, sunset and night, per weather, from `Morrowind.ini`'s `[Weather X]`
//! blocks — interpolated by the game hour, bent by a small empirical formula with three
//! scattering constants, and fogged exponentially by distances from `mge3\MGE.ini`.
//! Weather Adjuster (an MWSE mod) overrides the ramp per *region* through a JSON file.
//!
//! Round 17h: **Clear only.** The preview used to offer all ten weathers, and rounds 17
//! to 17g spent themselves on the rain, the snow and the storms that go with them.
//! Robin: "I am not happy with how the weather turned out, but I don't think it's worth
//! investing more in, so lets clean it up instead. Remove all weathers except Clear from
//! the program (engine, page and all)." So one ramp is read, one fog ratio, one preset
//! per region — and the wind, the precipitation and the storm keys, which only ever fed
//! the particles, are gone with them.
//!
//! Wraithguard: the ten weathers are back for what they do to the sky, the fog and the
//! water - their colour ramps, fog ratios, Weather Adjuster presets and `Glare View` -
//! and still without the particles. The page picks one (Preview, Weather).
//!
//! This module reads all three sources and hands them to the page as one JSON reply;
//! the page interpolates and the shaders draw. Nothing here decides where grass goes.
//!
//! Sources, in the order they are looked for:
//!   - `[Weather]` timings and `[Weather X]` colours: the load order's own ini (the MO2
//!     profile's Morrowind.ini, or the game's).
//!   - `MWSE/config/Weather Adjuster.json` through the overlay (a preset mod ships it;
//!     MO2's `overwrite` holds what the game saved), with `regions` (region name →
//!     preset) and `presets` (name → per-weather colours, `inscatter`, `outscatter`,
//!     `skylightScatter`, `skylightScatterMix`).
//!   - `mge3\MGE.ini` beside the game: `[Distant Land]` Above Water Fog Start/End,
//!     Below Water Fog Start/End (round 18dt), Use Exponential Fog, Use Atmosphere
//!     Scattering; `[Distant Land Weather]` `<W> Fog Ratio` / `Fog Offset`.
//!   - The load order's ini's `[Water]` block (round 18dt): the game's underwater colour
//!     and fog amounts, which every renderer's underwater fog is made of (`water_json`).

use crate::json::{escape_into, Val};
use crate::vfs::Vfs;
use std::path::Path;

/// The ten weathers, as the ini, the adjuster and MGE name them. Round 17h cut this to
/// Clear alone along with the rain, snow and storm particles; Wraithguard brings the
/// weathers back for their sky, fog and water only - no particles - so the preview can
/// show a cell under any of them, and the water can answer to them (Wonders of Water).
pub const WEATHERS: [&str; 10] =
    ["Clear", "Cloudy", "Foggy", "Overcast", "Rain", "Thunderstorm", "Ashstorm", "Blight", "Snow", "Blizzard"];

/// One ini section as key → value, keys lowercased.
pub(crate) fn section<'a>(text: &'a str, name: &str) -> Vec<(String, &'a str)> {
    let mut out = Vec::new();
    let mut inside = false;
    let want = name.to_ascii_lowercase();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.trim_matches(|c| c == '[' || c == ']').trim().eq_ignore_ascii_case(&want);
            continue;
        }
        if !inside || t.is_empty() || t.starts_with(';') {
            continue;
        }
        if let Some(eq) = t.find('=') {
            out.push((t[..eq].trim().to_ascii_lowercase(), t[eq + 1..].trim()));
        }
    }
    let _ = want;
    out
}

pub(crate) fn lookup<'a>(sec: &[(String, &'a str)], key: &str) -> Option<&'a str> {
    let k = key.to_ascii_lowercase();
    sec.iter().find(|(kk, _)| *kk == k).map(|(_, v)| *v)
}

pub(crate) fn num(sec: &[(String, &str)], key: &str, fallback: f32) -> f32 {
    lookup(sec, key).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(fallback)
}

/// "255,189,157" → [1.0, 0.741, 0.616]; None when the value is not three numbers.
fn colour(v: &str) -> Option<[f32; 3]> {
    let parts: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse::<f32>().ok()).collect();
    if parts.len() < 3 {
        return None;
    }
    Some([parts[0] / 255.0, parts[1] / 255.0, parts[2] / 255.0])
}

fn push_colour(out: &mut String, c: &[f32; 3]) {
    out.push_str(&format!("[{:.4},{:.4},{:.4}]", c[0], c[1], c[2]));
}

fn push_ramp(out: &mut String, key: &str, sec: &[(String, &str)], prefix: &str) {
    // Sunrise, Day, Sunset, Night — the order the page interpolates in.
    out.push('"');
    out.push_str(key);
    out.push_str("\":[");
    for (i, when) in ["Sunrise", "Day", "Sunset", "Night"].iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let c = lookup(sec, &format!("{prefix} {when} Color")).and_then(colour).unwrap_or([0.5, 0.5, 0.5]);
        push_colour(out, &c);
    }
    out.push(']');
}

/// OpenMW keeps the same values as `fallback=Weather_Clear_Sky_Day_Color,095,105,160`
/// lines in `openmw.cfg` (round 14). Rewritten into the ini shape — `[Weather Clear]`
/// blocks and a `[Weather]` block — so one reader serves both installs. Underscores in
/// the key are the ini's spaces (`Sky_Pre-Sunrise_Time` → `Sky Pre-Sunrise Time`).
/// Returns None when the text has no `fallback=Weather_` line at all.
pub fn openmw_to_ini(text: &str) -> Option<String> {
    let mut timing = String::from("[Weather]\n");
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut any = false;
    for line in text.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("fallback=") else { continue };
        /* Round 18dt: the `[Water]` block too - `fallback=Water_UnderwaterColor,012,030,037`
           is the ini's `UnderwaterColor=012,030,037` under `[Water]`. Its keys have no
           spaces in the ini, so they are carried over as they are. */
        if let Some(rest) = rest.strip_prefix("Water_") {
            let Some(comma) = rest.find(',') else { continue };
            let (key, value) = (&rest[..comma], rest[comma + 1..].trim());
            any = true;
            let name = "Water".to_string();
            let block = match blocks.iter_mut().find(|(n, _)| *n == name) {
                Some(b) => b,
                None => {
                    blocks.push((name, String::new()));
                    blocks.last_mut().unwrap()
                }
            };
            block.1.push_str(&format!("{key}={value}\n"));
            continue;
        }
        let Some(rest) = rest.strip_prefix("Weather_") else { continue };
        let Some(comma) = rest.find(',') else { continue };
        let (key, value) = (&rest[..comma], rest[comma + 1..].trim());
        any = true;
        // The first token is a weather name when a block of that name exists.
        let weather = WEATHERS.iter().find(|w| key.starts_with(&format!("{w}_"))).copied();
        match weather {
            Some(w) => {
                let k = key[w.len() + 1..].replace('_', " ");
                let name = format!("Weather {w}");
                let block = match blocks.iter_mut().find(|(n, _)| *n == name) {
                    Some(b) => b,
                    None => {
                        blocks.push((name, String::new()));
                        blocks.last_mut().unwrap()
                    }
                };
                block.1.push_str(&format!("{k}={value}\n"));
            }
            None => timing.push_str(&format!("{}={value}\n", key.replace('_', " "))),
        }
    }
    if !any {
        return None;
    }
    for (name, body) in blocks {
        timing.push_str(&format!("[{name}]\n{body}"));
    }
    Some(timing)
}

/// The `[Weather]` timings and every `[Weather X]` block of an ini, as JSON.
/// An `openmw.cfg` is accepted too — its `fallback=Weather_…` lines are the same values.
pub fn ini_json(text: &str) -> String {
    let converted = openmw_to_ini(text);
    let text = converted.as_deref().unwrap_or(text);
    let w = section(text, "Weather");
    let found = WEATHERS.iter().any(|n| !section(text, &format!("Weather {n}")).is_empty());
    let mut out = format!("{{\"found\":{found},\"openmw\":{},\"timing\":{{", converted.is_some());
    let keys = [
        ("sunrise", "Sunrise Time", 6.0),
        ("sunset", "Sunset Time", 18.0),
        ("sunriseDuration", "Sunrise Duration", 2.0),
        ("sunsetDuration", "Sunset Duration", 2.0),
        ("skyPreSunrise", "Sky Pre-Sunrise Time", 0.5),
        ("skyPostSunrise", "Sky Post-Sunrise Time", 1.0),
        ("skyPreSunset", "Sky Pre-Sunset Time", 1.5),
        ("skyPostSunset", "Sky Post-Sunset Time", 0.5),
        ("ambientPreSunrise", "Ambient Pre-Sunrise Time", 0.5),
        ("ambientPostSunrise", "Ambient Post-Sunrise Time", 2.0),
        ("ambientPreSunset", "Ambient Pre-Sunset Time", 1.0),
        ("ambientPostSunset", "Ambient Post-Sunset Time", 1.25),
        ("fogPreSunrise", "Fog Pre-Sunrise Time", 0.5),
        ("fogPostSunrise", "Fog Post-Sunrise Time", 1.0),
        ("fogPreSunset", "Fog Pre-Sunset Time", 2.0),
        ("fogPostSunset", "Fog Post-Sunset Time", 1.0),
        ("sunPreSunrise", "Sun Pre-Sunrise Time", 0.0),
        ("sunPostSunrise", "Sun Post-Sunrise Time", 0.0),
        ("sunPreSunset", "Sun Pre-Sunset Time", 1.0),
        ("sunPostSunset", "Sun Post-Sunset Time", 1.25),
        // Round 15: when the stars come out and go (the game's names, vanilla values).
        ("starsPostSunsetStart", "Stars Post-Sunset Start", 1.0),
        ("starsPreSunriseFinish", "Stars Pre-Sunrise Finish", 2.0),
        ("starsFadingDuration", "Stars Fading Duration", 2.0),
    ];
    for (i, (k, ini, d)) in keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{k}\":{}", num(&w, ini, *d)));
    }
    out.push_str("},\"weathers\":{");
    let mut first = true;
    for name in WEATHERS {
        let sec = section(text, &format!("Weather {name}"));
        if sec.is_empty() {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push('"');
        out.push_str(name);
        out.push_str("\":{");
        push_ramp(&mut out, "sky", &sec, "Sky");
        out.push(',');
        push_ramp(&mut out, "fog", &sec, "Fog");
        out.push(',');
        push_ramp(&mut out, "ambient", &sec, "Ambient");
        out.push(',');
        push_ramp(&mut out, "sun", &sec, "Sun");
        out.push_str(",\"sunDisc\":");
        push_colour(&mut out, &lookup(&sec, "Sun Disc Sunset Color").and_then(colour).unwrap_or([1.0, 0.74, 0.62]));
        out.push_str(&format!(",\"fogDepthDay\":{},\"fogDepthNight\":{}", num(&sec, "Land Fog Day Depth", 0.7), num(&sec, "Land Fog Night Depth", 0.7)));
        out.push_str(",\"cloud\":\"");
        escape_into(lookup(&sec, "Cloud Texture").unwrap_or(""), &mut out);
        out.push('"');
        out.push_str(&format!(",\"cloudSpeed\":{}", num(&sec, "Cloud Speed", 1.0)));
        // How much of the sun shows through this weather (1 clear, 0 overcast): the game's
        // own sun visibility scale, which MGE reads back as sunVis.
        out.push_str(&format!(",\"glareView\":{}", num(&sec, "Glare View", 1.0)));
        out.push('}');
    }
    out.push_str("},\"water\":");
    water_json(text, &mut out);
    out.push('}');
    out
}

/// Round 18dt: the game's underwater fog, from the ini's `[Water]` block (the same
/// `fallback=Water_…` lines in openmw.cfg). What the renderers make of it:
///
/// - The colour, every renderer: `UnderwaterColor × UnderwaterColorWeight + the weather's
///   fog colour × (1 − weight)` (OpenMW `FogManager::getFogColor(true)`; MGE XE reads the
///   game's own colour back and skips its scattering underwater, `adjustFog`).
/// - The range, vanilla and OpenMW (`FogManager::configure`): start = min(view, 7168) ×
///   (1 − f), end = min(view, 7168), where f is `UnderwaterSunriseFog / DayFog / SunsetFog /
///   NightFog` interpolated by the hour with the Fog timings (weather.cpp), or
///   `UnderwaterIndoorFog` in a cell without weather. By day f = 2.5: 60 % fog at the eye.
/// - The range under MGE XE is its own below-water pair (`mge_json`, in cells).
///
/// Vanilla's numbers stand behind a missing block; `found` says whether one was read.
fn water_json(text: &str, out: &mut String) {
    let w = section(text, "Water");
    let col = lookup(&w, "UnderwaterColor").and_then(colour).unwrap_or([12.0 / 255.0, 30.0 / 255.0, 37.0 / 255.0]);
    out.push_str(&format!("{{\"found\":{},\"underwaterColor\":", !w.is_empty()));
    push_colour(out, &col);
    out.push_str(&format!(
        ",\"colorWeight\":{},\"sunriseFog\":{},\"dayFog\":{},\"sunsetFog\":{},\"nightFog\":{},\"indoorFog\":{}}}",
        num(&w, "UnderwaterColorWeight", 0.85),
        num(&w, "UnderwaterSunriseFog", 3.0),
        num(&w, "UnderwaterDayFog", 2.5),
        num(&w, "UnderwaterSunsetFog", 3.0),
        num(&w, "UnderwaterNightFog", 4.0),
        num(&w, "UnderwaterIndoorFog", 3.0)
    ));
}

/// A JSON value re-emitted as JSON — the adjuster file is handed on as it is.
fn emit(v: &Val, out: &mut String) {
    match v {
        Val::Null => out.push_str("null"),
        Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Val::Num(n) => {
            if n.is_finite() {
                out.push_str(&format!("{}", n));
            } else {
                out.push('0');
            }
        }
        Val::Str(s) => {
            out.push('"');
            escape_into(s, out);
            out.push('"');
        }
        Val::Arr(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                emit(x, out);
            }
            out.push(']');
        }
        Val::Obj(kv) => {
            out.push('{');
            for (i, (k, x)) in kv.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                escape_into(k, out);
                out.push_str("\":");
                emit(x, out);
            }
            out.push('}');
        }
    }
}

/// Weather Adjuster's `regions` and `presets`, or null when the overlay has no file.
pub fn adjuster_json(vfs: &Vfs) -> String {
    let Some(loc) = vfs.resolve("mwse/config/weather adjuster.json") else { return "null".into() };
    let Some(bytes) = vfs.load(&loc) else { return "null".into() };
    let text = String::from_utf8_lossy(&bytes);
    let Ok(v) = crate::json::parse(&text) else { return "null".into() };
    let mut out = String::from("{\"regions\":");
    match v.get("regions") {
        Some(r) => emit(r, &mut out),
        None => out.push_str("{}"),
    }
    out.push_str(",\"presets\":");
    match v.get("presets") {
        Some(p) => emit(p, &mut out),
        None => out.push_str("{}"),
    }
    out.push('}');
    out
}

/// The active MGE install's fog and scattering settings, or MGE's defaults when there is
/// no MGE at all.
///
/// Round 17m: *which file* those settings live in is a question of its own, and `mge.rs`
/// answers it — G7's fork keeps them in `mgeXE.toml` and never touches the ini, and under
/// Root Builder the shim that decides is not even in the game folder. This reads whatever
/// that resolver landed on, and reports the file and the reasoning so a wrong answer is
/// visible rather than three months of unexplained fog.
pub fn mge_json(layout: &crate::layout::Layout) -> String {
    let found = crate::mge::find(layout);
    let text = found
        .config
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let mut out = String::from("{");
    out.push_str(&format!("\"found\":{}", !text.is_empty()));
    /* A config that will not parse must say so. Falling back to defaults quietly is
       exactly how the wrong fog survived unnoticed for three months, and it is a real
       possibility here: the fork's own file failed to parse at first, over a Windows path
       written as a multi-line literal string. */
    let mut trouble = String::new();
    match found.kind {
        Some(crate::mge::ConfigKind::Toml) if !text.is_empty() => {
            match crate::toml::parse(&text) {
                Ok(doc) => toml_fog(&doc, &mut out),
                Err(e) => {
                    trouble = format!(" — could not be read ({})", e);
                    ini_fog("", &mut out);
                }
            }
        }
        _ => ini_fog(&text, &mut out),
    }
    out.push_str(",\"file\":\"");
    crate::json::escape_into(
        &found.config.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        &mut out,
    );
    out.push_str("\",\"name\":\"");
    crate::json::escape_into(found.kind.map(|k| k.file()).unwrap_or(""), &mut out);
    out.push_str("\",\"how\":\"");
    crate::json::escape_into(&format!("{}{}", found.how, trouble), &mut out);
    out.push_str("\",\"dll\":\"");
    crate::json::escape_into(
        &found.dll.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        &mut out,
    );
    out.push_str("\"}");
    out
}

/// `mge3\MGE.ini`, as MGE XE and MGE XE UF write it.
fn ini_fog(text: &str, out: &mut String) {
    let dl = section(text, "Distant Land");
    let dw = section(text, "Distant Land Weather");
    let on = |k: &str, d: bool| -> bool {
        lookup(&dl, k).map(|v| v.trim().eq_ignore_ascii_case("true") || v.trim() == "1").unwrap_or(d)
    };
    out.push_str(&format!(",\"fogStart\":{},\"fogEnd\":{}", num(&dl, "Above Water Fog Start", 2.0), num(&dl, "Above Water Fog End", 5.0)));
    // Round 18dt: the fog with the eye under the water, in cells; MGE's defaults -0.5 / 0.3.
    out.push_str(&format!(",\"fogBelowStart\":{},\"fogBelowEnd\":{}", num(&dl, "Below Water Fog Start", -0.5), num(&dl, "Below Water Fog End", 0.3)));
    out.push_str(&format!(",\"expFog\":{},\"scattering\":{}", on("Use Exponential Fog", true), on("Use Atmosphere Scattering", true)));
    out.push_str(&format!(",\"drawDistance\":{}", num(&dl, "Draw Distance", 5.0)));
    out.push_str(",\"weathers\":{");
    for (i, name) in WEATHERS.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // MGE's own defaults for the ratios, when the ini says nothing.
        let (dr, doff) = (1.0, 0.0);
        out.push_str(&format!(
            "\"{name}\":{{\"fogRatio\":{},\"fogOffset\":{}}}",
            num(&dw, &format!("{name} Fog Ratio"), dr),
            num(&dw, &format!("{name} Fog Offset"), doff)
        ));
    }
    out.push('}');
}

/// `mgeXE.toml`, as G7's fork writes it. The same numbers under other names:
/// `[distant_land.fog]` `above_water_start`/`above_water_end`, `exponential`,
/// `atmosphere_scattering`, and a `[distant_land.weather.<name>]` table per weather with
/// `fog_ratio` and `fog_offset`. Weather tables are lowercased there.
fn toml_fog(doc: &crate::toml::Table, out: &mut String) {
    let get = |path: &[&str]| -> Option<&crate::toml::Value> {
        let mut cur = doc.get(path[0])?;
        for k in &path[1..] {
            cur = cur.as_table()?.get(*k)?;
        }
        Some(cur)
    };
    let f = |path: &[&str], d: f32| get(path).and_then(|v| v.as_f32()).unwrap_or(d);
    let b = |path: &[&str], d: bool| get(path).and_then(|v| v.as_bool()).unwrap_or(d);
    out.push_str(&format!(
        ",\"fogStart\":{},\"fogEnd\":{}",
        f(&["distant_land", "fog", "above_water_start"], 2.0),
        f(&["distant_land", "fog", "above_water_end"], 5.0)
    ));
    // Round 18dt: `below_water_start` / `below_water_end`, the fork's names for the same pair.
    out.push_str(&format!(
        ",\"fogBelowStart\":{},\"fogBelowEnd\":{}",
        f(&["distant_land", "fog", "below_water_start"], -0.5),
        f(&["distant_land", "fog", "below_water_end"], 0.3)
    ));
    out.push_str(&format!(
        ",\"expFog\":{},\"scattering\":{}",
        b(&["distant_land", "fog", "exponential"], true),
        b(&["distant_land", "fog", "atmosphere_scattering"], true)
    ));
    out.push_str(&format!(",\"drawDistance\":{}", f(&["distant_land", "draw_distance"], 5.0)));
    out.push_str(",\"weathers\":{");
    for (i, name) in WEATHERS.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let lc = name.to_ascii_lowercase();
        out.push_str(&format!(
            "\"{name}\":{{\"fogRatio\":{},\"fogOffset\":{}}}",
            f(&["distant_land", "weather", &lc, "fog_ratio"], 1.0),
            f(&["distant_land", "weather", &lc, "fog_offset"], 0.0)
        ));
    }
    out.push('}');
}

/// Everything the page's sky needs, in one reply.
pub fn atmosphere_json(order_file: &Path, vfs: &Vfs, layout: &crate::layout::Layout) -> String {
    let ini = std::fs::read_to_string(order_file).unwrap_or_default();
    // Round 18i: and the renderer the install runs, read from its own files.
    format!(
        "{{\"ini\":{},\"adjuster\":{},\"mge\":{},\"renderer\":{}}}",
        ini_json(&ini),
        adjuster_json(vfs),
        mge_json(layout),
        crate::renderer::renderer_json(layout, &ini)
    )
}
