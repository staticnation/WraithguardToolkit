//! The renderer the install runs, and what its own files say about how it draws.
//!
//! Round 18i. Robin: *"We have rules to identify what install we are using, and that
//! should identify what we do in how we render the scene. If G7's fork of MGE XE is
//! installed, always check the mgeXE.toml as well for relevant entries."* And: *"I want
//! the tool to be used by anyone, no matter what version of MGE XE they use, or OpenMW.
//! So we need to be able to read any of the engines that exist and fetch the relevant
//! information."*
//!
//! `mge::find` (round 17m) answers *which* engine by asking the `d3d8.dll` that would
//! load; this module reads that engine's files for the entries that bear on the
//! preview and hands them to the page as one `renderer` object, so the page draws the
//! way the install does and the interface can say which file and entry it followed.
//!
//! What is read, and from where:
//!
//! | entry | G7 fork (`mgeXE.toml`) | MGE XE / UF (`mge3\MGE.ini`) | OpenMW (`settings.cfg`) |
//! |---|---|---|---|
//! | per-pixel lighting | `distant_land.per_pixel_lighting`, `per_pixel_mode` | `[Distant Land] Per Pixel Shader`, `Per Pixel Shader Flags` | `[Shaders] lighting method` |
//! | sun / ambient multipliers | `lighting.weather.clear.sun` / `.ambient` | `[Per Pixel Lighting] Clear Sun Brightness` / `Ambient Brightness` | — |
//! | post chain | `shaders.chain`, `render.enable_shaders` | `[Shader Chain]`, `Hardware Shader` | `[Post Processing] enabled`, `chain` |
//! | water | `distant_land.water.*` | `Water Wave Height`, `Water Caustics Intensity`, `Water Reflects …` | `[Water]` |
//! | shadows | `distant_land.shadows.*` | `Sun Shadows`, `Sun Shadow Map Resolution` | `[Shadows]` |
//! | field of view | `render.fov` (horizontal) | `[Render State] Horizontal Screen FOV` | `[Camera] field of view` (vertical) |
//! | draw distance | `distant_land.draw_distance` | `Draw Distance` | `[Camera] viewing distance` |
//! | grass | `distant_land.render_grass` | `Render Grass` | `[Groundcover] enabled`, `density`, `rendering distance` |
//! | underwater fog (18dt) | `distant_land.fog.below_water_start/_end` (weather.rs) | `Below Water Fog Start/End` (weather.rs) | `[Fog] distant underwater fog start/end`, `use distant fog`, `[Camera] viewing distance` |
//!
//! And from the file the *game* owns whichever renderer draws it: `Morrowind.ini`
//! `[LightAttenuation]` (`fallback=LightAttenuation_…` lines in `openmw.cfg`), which is
//! what a lamp's falloff is made of. **Read from the load order's own ini** — under Mod
//! Organizer that is the profile's `Morrowind.ini`, not the game folder's. Robin's game
//! folder ini says constant 0.0 / linear 3.0 / quadratic 0; the profile's, which the game
//! reads, says 0.34 / 0 / 3.25 — and MWSE, asked in the running game, reported exactly
//! `c=0.34 l=0 q=3.25/r²` on every lamp in Balmora (2026-09-06). The same lesson as 17m's
//! fog, one file over.
//!
//! The coefficients follow the engine's own `NiPointLight::setAttenuationForRadius`, as
//! MWSE decompiles it (SharedSE/NIPointLight.cpp) and OpenMW's `configureLight`
//! reproduces it: constant = ConstantValue if UseConstant; linear = LinearValue for
//! method 0, LinearValue / r for 1, / r² for 2; quadratic likewise; `OutQuadInLin`
//! swaps the two between interiors and exteriors; all three zero falls back to a
//! constant of 1.
//!
//! Not read, on purpose: the load order's lamp-hours mod. The Midnight Oil (MWSE) is what
//! turns Robin's exterior lamps off by day, and a reader for its config shipped and was
//! taken out the same day at his word — "I deliberately don't want Midnight Oil settings
//! for toggling lights to be attached to the viewport in the engine, as I instead added
//! the three options (Night time, Always, and Off, with some exceptions added)." The
//! Lights picker is the rule; the game itself never switches a placed light off.

use crate::json::J;
use crate::layout::Layout;
use crate::toml::{Table, Value};
use crate::weather::{lookup, num, section};
use std::path::PathBuf;

/// `Morrowind.ini [LightAttenuation]`, as the engine keeps it in memory.
#[derive(Clone, Debug, PartialEq)]
pub struct Attenuation {
    pub use_constant: bool,
    pub constant_value: f32,
    pub use_linear: bool,
    pub linear_method: i32,
    pub linear_value: f32,
    pub linear_radius_mult: f32,
    pub use_quadratic: bool,
    pub quadratic_method: i32,
    pub quadratic_value: f32,
    pub quadratic_radius_mult: f32,
    pub out_quad_in_lin: bool,
    /// Whether the section (or any `fallback=LightAttenuation_` line) was present.
    pub found: bool,
}

impl Attenuation {
    /// The game's shipped `Morrowind.ini`, and OpenMW's `openmw.cfg` fallbacks: linear
    /// 3 / r and nothing else.
    pub fn vanilla() -> Self {
        Attenuation {
            use_constant: false,
            constant_value: 0.0,
            use_linear: true,
            linear_method: 1,
            linear_value: 3.0,
            linear_radius_mult: 1.0,
            use_quadratic: false,
            quadratic_method: 2,
            quadratic_value: 16.0,
            quadratic_radius_mult: 1.0,
            out_quad_in_lin: false,
            found: false,
        }
    }

    /// From an ini's `[LightAttenuation]` section, or an `openmw.cfg`'s
    /// `fallback=LightAttenuation_<Key>,<value>` lines. Keys absent keep vanilla's value.
    pub fn from_ini(text: &str) -> Self {
        let mut out = Self::vanilla();
        // openmw.cfg: rewrite the fallback lines into the ini shape and read that.
        let mut converted = String::new();
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("fallback=").and_then(|r| r.strip_prefix("LightAttenuation_")) {
                if let Some(comma) = rest.find(',') {
                    if converted.is_empty() {
                        converted.push_str("[LightAttenuation]\n");
                    }
                    converted.push_str(&format!("{}={}\n", &rest[..comma], rest[comma + 1..].trim()));
                }
            }
        }
        let text = if converted.is_empty() { text } else { converted.as_str() };
        let sec = section(text, "LightAttenuation");
        if sec.is_empty() {
            return out;
        }
        out.found = true;
        let flag = |k: &str, d: bool| -> bool {
            lookup(&sec, k).map(|v| { let v = v.trim(); v == "1" || v.eq_ignore_ascii_case("true") }).unwrap_or(d)
        };
        let int = |k: &str, d: i32| -> i32 { lookup(&sec, k).and_then(|v| v.trim().parse::<f32>().ok()).map(|v| v as i32).unwrap_or(d) };
        out.use_constant = flag("UseConstant", out.use_constant);
        out.constant_value = num(&sec, "ConstantValue", out.constant_value).max(0.0);
        out.use_linear = flag("UseLinear", out.use_linear);
        out.linear_method = int("LinearMethod", out.linear_method);
        out.linear_value = num(&sec, "LinearValue", out.linear_value).max(0.0);
        out.linear_radius_mult = num(&sec, "LinearRadiusMult", out.linear_radius_mult).max(0.0);
        out.use_quadratic = flag("UseQuadratic", out.use_quadratic);
        out.quadratic_method = int("QuadraticMethod", out.quadratic_method);
        out.quadratic_value = num(&sec, "QuadraticValue", out.quadratic_value).max(0.0);
        out.quadratic_radius_mult = num(&sec, "QuadraticRadiusMult", out.quadratic_radius_mult).max(0.0);
        out.out_quad_in_lin = flag("OutQuadInLin", out.out_quad_in_lin);
        out
    }

    /// The `(constant, linear, quadratic)` the engine hands its renderer for a light of
    /// this radius — `NiPointLight::setAttenuationForRadius`, as MWSE decompiles it. The
    /// same function OpenMW's `configureLight` is, so both installs read alike.
    pub fn coefficients(&self, radius: f32, interior: bool) -> [f32; 3] {
        let radius = radius.max(16.0);
        let mut c = 0.0f32;
        let mut l = 0.0f32;
        let mut q = 0.0f32;
        if self.use_constant {
            c = self.constant_value;
        }
        // OutQuadInLin: quadratic outdoors, linear indoors, whatever the Use flags say.
        if self.use_linear || (interior && self.out_quad_in_lin) {
            let r = radius * self.linear_radius_mult;
            l = 0.01;
            if self.linear_method == 0 {
                l = self.linear_value;
            } else if self.linear_method == 1 && r > 0.0 {
                l = self.linear_value / r;
            } else if self.linear_method == 2 && r > 0.0 {
                l = self.linear_value / (r * r);
            }
        }
        if self.use_quadratic || (!interior && self.out_quad_in_lin) {
            let r = radius * self.quadratic_radius_mult;
            q = 0.01;
            if self.quadratic_method == 0 {
                q = self.quadratic_value;
            } else if self.quadratic_method == 1 && r > 0.0 {
                // The engine divides by the *unscaled* radius here (MWSE's decompile
                // keeps that quirk); OpenMW uses the scaled one. Equal at mult 1.
                q = self.quadratic_value / radius;
            } else if self.quadratic_method == 2 && r > 0.0 {
                q = self.quadratic_value / (r * r);
            }
        }
        if c == 0.0 && l == 0.0 && q == 0.0 {
            c = 1.0;
        }
        [c, l, q]
    }

    fn json(&self) -> String {
        let mut j = J::obj();
        j.bool("found", self.found)
            .bool("useConstant", self.use_constant)
            .num("constantValue", self.constant_value as f64)
            .bool("useLinear", self.use_linear)
            .int("linearMethod", self.linear_method.max(0) as u64)
            .num("linearValue", self.linear_value as f64)
            .num("linearRadiusMult", self.linear_radius_mult as f64)
            .bool("useQuadratic", self.use_quadratic)
            .int("quadraticMethod", self.quadratic_method.max(0) as u64)
            .num("quadraticValue", self.quadratic_value as f64)
            .num("quadraticRadiusMult", self.quadratic_radius_mult as f64)
            .bool("outQuadInLin", self.out_quad_in_lin);
        // Worked examples the page and a reader can check against a running game.
        let ex = self.coefficients(256.0, false);
        j.raw("at256", &format!("[{},{},{}]", ex[0], ex[1], ex[2]));
        j.done()
    }
}

/// Which renderer draws this install.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Engine {
    /// MGE XE — any build; `fork` says which.
    MgeXe,
    OpenMw,
    /// Morrowind.exe with no MGE: the fixed pipeline, per vertex.
    Vanilla,
}

impl Engine {
    pub fn key(&self) -> &'static str {
        match self {
            Engine::MgeXe => "mgexe",
            Engine::OpenMw => "openmw",
            Engine::Vanilla => "vanilla",
        }
    }
}

/// The build's own name for itself, read off the shim: "MGE-XE G7 Fork v0.20.0",
/// "MGE XE 0.19.1". A label for the reader, not a decision — the config file is decided
/// by what the shim reads (`mge::dll_kind`), never by this string.
pub fn dll_label(bytes: &[u8]) -> Option<String> {
    let find = |needle: &[u8]| -> Option<usize> {
        bytes.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
    };
    let take = |at: usize| -> String {
        let mut s = String::new();
        for &b in &bytes[at..bytes.len().min(at + 40)] {
            if b == 0 || b == b'\n' || b == b'\r' || b == b'"' {
                break;
            }
            if b.is_ascii_graphic() || b == b' ' {
                s.push(b as char);
            } else {
                break;
            }
        }
        s.trim().to_string()
    };
    if let Some(at) = find(b"MGE-XE G7 Fork") {
        return Some(take(at));
    }
    // "MGE XE 0.19.1" — the first that is followed by a digit.
    let mut from = 0;
    while let Some(off) = bytes[from..].windows(7).position(|w| w.eq_ignore_ascii_case(b"MGE XE ")) {
        let at = from + off;
        if bytes.get(at + 7).map(|b| b.is_ascii_digit()).unwrap_or(false) {
            let s = take(at);
            if s.len() >= 9 {
                return Some(s);
            }
        }
        from = at + 7;
    }
    None
}

/// A `settings.cfg`-shaped file (`[Section]` and `key = value`), one value.
fn cfg_get<'a>(text: &'a str, sec: &str, key: &str) -> Option<&'a str> {
    let s = section(text, sec);
    lookup(&s, key)
}

fn cfg_f32(text: &str, sec: &str, key: &str, d: f32) -> f32 {
    cfg_get(text, sec, key).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d)
}

fn cfg_bool(text: &str, sec: &str, key: &str, d: bool) -> bool {
    cfg_get(text, sec, key)
        .map(|v| {
            let v = v.trim();
            v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("on")
        })
        .unwrap_or(d)
}

fn toml_get<'a>(doc: &'a Table, path: &[&str]) -> Option<&'a Value> {
    let mut cur = doc.get(path[0])?;
    for k in &path[1..] {
        cur = cur.as_table()?.get(*k)?;
    }
    Some(cur)
}

/// A post chain's flags: what the preview has an answer for.
fn chain_json(names: &[String], enabled: bool, found: bool) -> String {
    let has = |needle: &str| names.iter().any(|n| n.to_ascii_lowercase().contains(needle));
    let mut j = J::obj();
    j.bool("found", found).bool("enabled", enabled);
    j.raw("chain", &crate::json::string_array(names));
    j.bool("ssao", enabled && has("ssao"))
        .bool("sunshafts", enabled && has("sunshaft"))
        .bool("bloom", enabled && has("bloom"))
        .bool("dof", enabled && has("depth of field"))
        .bool("underwater", enabled && has("underwater"))
        .bool("fxaa", enabled && has("fxaa"));   // 18do: the page has an FXAA pass of its own
    j.done()
}

/// Everything the page needs to draw the way this install does.
pub fn renderer_json(layout: &Layout, order_text: &str) -> String {
    let att = Attenuation::from_ini(order_text);
    let is_openmw = layout
        .order_file
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("openmw.cfg"))
        .unwrap_or(false);

    let mut j = J::obj();
    if is_openmw {
        openmw_json(layout, &att, &mut j);
    } else {
        let found = crate::mge::find(layout);
        match found.kind {
            Some(kind) if found.dll.is_some() || found.config.is_some() => mge_json(layout, &found, kind, &att, &mut j),
            _ => vanilla_json(layout, &att, &mut j),
        }
    }
    j.raw("attenuation", &att.json());
    j.str("attenuationFile", &layout.order_file.display().to_string());
    j.done()
}

fn vanilla_json(layout: &Layout, _att: &Attenuation, j: &mut J) {
    j.str("engine", Engine::Vanilla.key())
        .str("label", "Morrowind.exe, no MGE")
        .str("fork", "")
        .str("how", "no d3d8.dll that reads an MGE config")
        .str("file", "");
    let _ = layout;
    j.raw("lighting", &lighting_json("ffp", false, "", 1.0, 1.0, &[], true));
    j.raw("post", &chain_json(&[], false, false));
    j.raw("water", &J::obj().bool("found", false).done());
    j.raw("shadows", &J::obj().bool("found", false).bool("enabled", false).done());
    j.raw("fov", "null");
    j.raw("drawDistance", "null");
    j.raw("grass", "null");
}

/// The lighting block. `model` is what the page's shader switches on:
/// `mge-ppl` (MGE's FixedFuncEmu: 1/(q·d²+c) per pixel, lambert only, tonemapped, no
/// point lights on grass), `ffp` (the fixed pipeline per vertex — vanilla, MGE with
/// per-pixel lighting off, OpenMW's legacy method: 1/(c+l·d+q·d²) clamped),
/// `openmw-shaders` (OpenMW's shader methods: the same, faded out between the light's
/// bounds radius and twice it).
#[allow(clippy::too_many_arguments)]
fn lighting_json(model: &str, per_pixel: bool, per_pixel_mode: &str, sun: f32, amb: f32, extra: &[(&str, String)], clamp: bool) -> String {
    let mut j = J::obj();
    j.str("model", model)
        .bool("perPixel", per_pixel)
        .str("perPixelMode", per_pixel_mode)
        .num("sunMult", sun as f64)
        .num("ambientMult", amb as f64)
        .bool("clampLighting", clamp);
    for (k, v) in extra {
        j.raw(k, v);
    }
    j.done()
}

fn mge_json(layout: &Layout, found: &crate::mge::Found, kind: crate::mge::ConfigKind, _att: &Attenuation, j: &mut J) {
    let label = found
        .dll
        .as_ref()
        .and_then(|d| std::fs::read(d).ok())
        .filter(|b| b.len() <= 32 << 20)
        .and_then(|b| dll_label(&b))
        .unwrap_or_else(|| "MGE XE".to_string());
    let fork = if label.to_ascii_lowercase().contains("g7") { "g7" } else { "" };
    j.str("file", &found.config.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
    let text = found.config.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let _ = layout;
    /* Round 18bf (I2): a config that will not parse — or will not open — has to say so.
       This used to fall through to `mge_ini("")`, which is the *ini* defaults: MGE XE's
       renderer settings read as though the fork's own file said nothing, while `how` went
       on naming the file as if it had been read. `weather.rs`' reader of the same file has
       always appended the trouble to `how` for exactly this reason — its own comment:
       "Falling back to defaults quietly is exactly how the wrong fog survived unnoticed for
       three months" — and the fork's file really did fail to parse once, over a Windows
       path written as a multi-line literal string. Two readers of one file, one of them
       honest; now both. */
    let mut trouble = String::new();
    match kind {
        crate::mge::ConfigKind::Toml => match crate::toml::parse(&text) {
            Ok(doc) => mge_toml(&doc, j),
            Err(e) => {
                trouble = if found.config.is_some() && text.is_empty() {
                    " — could not be opened".to_string()
                } else {
                    format!(" — could not be read ({e})")
                };
                mge_ini("", j);
            }
        },
        crate::mge::ConfigKind::Ini => {
            // An ini has no parse step to fail; a file that would not open is still worth
            // saying, because every value below is then a default wearing the file's name.
            if found.config.is_some() && text.is_empty() {
                trouble = " — could not be opened".to_string();
            }
            mge_ini(&text, j);
        }
    }
    j.str("engine", Engine::MgeXe.key())
        .str("label", &label)
        .str("fork", fork)
        .str("how", &format!("{}{}", found.how, trouble))
        .bool("configOk", trouble.is_empty());
}

fn mge_toml(doc: &Table, j: &mut J) {
    let f = |p: &[&str], d: f32| toml_get(doc, p).and_then(|v| v.as_f32()).unwrap_or(d);
    let b = |p: &[&str], d: bool| toml_get(doc, p).and_then(|v| v.as_bool()).unwrap_or(d);
    let s = |p: &[&str], d: &str| toml_get(doc, p).and_then(|v| v.as_str()).unwrap_or(d).to_string();
    let per_pixel = b(&["distant_land", "per_pixel_lighting"], false);
    let mode = s(&["distant_land", "per_pixel_mode"], "always");
    // Per-pixel lighting in interiors only leaves the exterior — where the grass is — on
    // the fixed pipeline.
    let exterior_ppl = per_pixel && !mode.eq_ignore_ascii_case("interiors_only");
    let sun = f(&["lighting", "weather", "clear", "sun"], 1.0);
    let amb = f(&["lighting", "weather", "clear", "ambient"], 1.0);
    let extra = [("expandedLightLimit", b(&["distant_land", "expanded_light_limit"], false).to_string())];
    j.raw("lighting", &lighting_json(if exterior_ppl { "mge-ppl" } else { "ffp" }, per_pixel, &mode, sun, amb, &extra, !exterior_ppl));
    let chain: Vec<String> = toml_get(doc, &["shaders", "chain"])
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    j.raw("post", &chain_json(&chain, b(&["render", "enable_shaders"], true), true));
    let mut w = J::obj();
    w.bool("found", true)
        .num("waveHeight", f(&["distant_land", "water", "wave_height"], 50.0) as f64)
        .num("caustics", f(&["distant_land", "water", "caustics_intensity"], 50.0) as f64)
        .bool("reflectLand", b(&["distant_land", "water", "reflect_land"], true))
        .bool("reflectStatics", b(&["distant_land", "water", "reflect_near_statics"], true))
        .bool("reflectSky", b(&["distant_land", "water", "reflect_sky"], true))
        .bool("blur", b(&["distant_land", "water", "blur_reflections"], false));
    j.raw("water", &w.done());
    let mut sh = J::obj();
    sh.bool("found", true)
        .bool("enabled", b(&["distant_land", "shadows", "enabled"], false))
        .num("resolution", f(&["distant_land", "shadows", "map_resolution"], 2048.0) as f64);
    j.raw("shadows", &sh.done());
    j.raw("fov", &J::obj().num("deg", f(&["render", "fov"], 75.0) as f64).str("axis", "h").done());
    j.num("drawDistance", f(&["distant_land", "draw_distance"], 5.0) as f64);
    j.bool("grass", b(&["distant_land", "render_grass"], true));
    j.bool("distantLand", b(&["distant_land", "enabled"], false));
}

fn mge_ini(text: &str, j: &mut J) {
    let dl = section(text, "Distant Land");
    let rs = section(text, "Render State");
    let ppl = section(text, "Per Pixel Lighting");
    let on = |sec: &[(String, &str)], k: &str, d: bool| -> bool {
        lookup(sec, k)
            .map(|v| {
                let v = v.trim();
                v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("on")
            })
            .unwrap_or(d)
    };
    let per_pixel = on(&dl, "Per Pixel Shader", false);
    let mode = lookup(&dl, "Per Pixel Shader Flags").unwrap_or("Always").trim().to_string();
    let exterior_ppl = per_pixel && !mode.to_ascii_lowercase().contains("interior");
    let sun = num(&ppl, "Clear Sun Brightness", 1.0);
    let amb = num(&ppl, "Clear Ambient Brightness", 1.0);
    j.raw("lighting", &lighting_json(if exterior_ppl { "mge-ppl" } else { "ffp" }, per_pixel, &mode, sun, amb, &[], !exterior_ppl));
    // The chain is bare lines under [Shader Chain]; `section` reads `k=v` lines only.
    let mut chain: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.eq_ignore_ascii_case("[Shader Chain]");
            continue;
        }
        if inside && !t.is_empty() && !t.starts_with(';') {
            chain.push(t.to_string());
        }
    }
    let found = !text.is_empty();
    j.raw("post", &chain_json(&chain, on(&rs, "Hardware Shader", true), found));
    let mut w = J::obj();
    w.bool("found", found)
        .num("waveHeight", num(&dl, "Water Wave Height", 50.0) as f64)
        .num("caustics", num(&dl, "Water Caustics Intensity", 50.0) as f64)
        .bool("reflectLand", on(&dl, "Water Reflects Land", true))
        .bool("reflectStatics", on(&dl, "Water Reflects Near Statics", true))
        .bool("reflectSky", on(&dl, "Enable Sky Reflections", true))
        .bool("blur", on(&dl, "Blur Water Reflections", false));
    j.raw("water", &w.done());
    let mut sh = J::obj();
    sh.bool("found", found)
        .bool("enabled", on(&dl, "Sun Shadows", false))
        .num("resolution", num(&dl, "Sun Shadow Map Resolution", 1024.0) as f64);
    j.raw("shadows", &sh.done());
    j.raw("fov", &J::obj().num("deg", num(&rs, "Horizontal Screen FOV", 75.0) as f64).str("axis", "h").done());
    j.num("drawDistance", num(&dl, "Draw Distance", 5.0) as f64);
    j.bool("grass", on(&dl, "Render Grass", true));
    j.bool("distantLand", on(&dl, "Distant Land", false));
}

/// OpenMW's `settings.cfg` lives beside its `openmw.cfg` (the user's config folder),
/// and every key has a default when the file — or the key — is not there.
fn openmw_json(layout: &Layout, _att: &Attenuation, j: &mut J) {
    let cfg: Option<PathBuf> = layout.order_file.parent().map(|p| p.join("settings.cfg")).filter(|p| p.is_file());
    let text = cfg.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let found = cfg.is_some();
    j.str("engine", Engine::OpenMw.key())
        .str("label", "OpenMW")
        .str("fork", "")
        .str("how", if found { "openmw.cfg beside settings.cfg" } else { "openmw.cfg; no settings.cfg, so OpenMW's defaults" })
        .str("file", &cfg.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
    // [Shaders] lighting method: legacy | shaders compatibility | shaders (0.47+).
    let method = cfg_get(&text, "Shaders", "lighting method").unwrap_or("shaders compatibility").trim().to_ascii_lowercase();
    let model = if method == "legacy" { "ffp" } else { "openmw-shaders" };
    let extra = [
        ("lightingMethod", format!("\"{}\"", method)),
        ("lightBoundsMultiplier", cfg_f32(&text, "Shaders", "light bounds multiplier", 1.65).to_string()),
        ("maximumLightDistance", cfg_f32(&text, "Shaders", "maximum light distance", 8192.0).to_string()),
        ("lightFadeStart", cfg_f32(&text, "Shaders", "light fade start", 0.85).to_string()),
        ("maxLights", cfg_f32(&text, "Shaders", "max lights", 8.0).to_string()),
        ("forcePerPixel", cfg_bool(&text, "Shaders", "force per pixel lighting", false).to_string()),
    ];
    let clamp = cfg_bool(&text, "Shaders", "clamp lighting", true);
    j.raw("lighting", &lighting_json(model, method != "legacy", "", 1.0, 1.0, &extra, clamp));
    let post_on = cfg_bool(&text, "Post Processing", "enabled", false);
    let chain: Vec<String> = cfg_get(&text, "Post Processing", "chain")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    j.raw("post", &chain_json(&chain, post_on, found));
    let mut w = J::obj();
    w.bool("found", found)
        .bool("shader", cfg_bool(&text, "Water", "shader", true))
        .bool("refraction", cfg_bool(&text, "Water", "refraction", true))
        .num("reflectionDetail", cfg_f32(&text, "Water", "reflection detail", 2.0) as f64)
        .bool("rainRipples", cfg_bool(&text, "Water", "rain ripples", true));
    j.raw("water", &w.done());
    let mut sh = J::obj();
    sh.bool("found", found)
        .bool("enabled", cfg_bool(&text, "Shadows", "enable shadows", false))
        .num("resolution", cfg_f32(&text, "Shadows", "shadow map resolution", 1024.0) as f64);
    j.raw("shadows", &sh.done());
    j.raw("fov", &J::obj().num("deg", cfg_f32(&text, "Camera", "field of view", 60.0) as f64).str("axis", "v").done());
    // In cells, like MGE's, for the one comparison a reader will make.
    j.num("drawDistance", (cfg_f32(&text, "Camera", "viewing distance", 7168.0) / 8192.0) as f64);
    j.bool("grass", cfg_bool(&text, "Groundcover", "enabled", false));
    let mut g = J::obj();
    g.bool("enabled", cfg_bool(&text, "Groundcover", "enabled", false))
        .num("density", cfg_f32(&text, "Groundcover", "density", 1.0) as f64)
        .num("renderingDistance", cfg_f32(&text, "Groundcover", "rendering distance", 6144.0) as f64);
    j.raw("groundcover", &g.done());
    let mut fog = J::obj();
    fog.bool("useDistantFog", cfg_bool(&text, "Fog", "use distant fog", false))
        .num("viewingDistance", cfg_f32(&text, "Camera", "viewing distance", 7168.0) as f64)
        .num("distantLandFogStart", cfg_f32(&text, "Fog", "distant land fog start", 16384.0) as f64)
        .num("distantLandFogEnd", cfg_f32(&text, "Fog", "distant land fog end", 40960.0) as f64)
        // Round 18dt: the pair the underwater fog takes instead of the vanilla rule when
        // `use distant fog` is on (fogmanager.cpp); the defaults are MGE's -0.5 / 0.3 cells.
        .num("distantUnderwaterFogStart", cfg_f32(&text, "Fog", "distant underwater fog start", -4096.0) as f64)
        .num("distantUnderwaterFogEnd", cfg_f32(&text, "Fog", "distant underwater fog end", 2457.6) as f64)
        .bool("exponential", cfg_bool(&text, "Fog", "exponential fog", false))
        .bool("skyBlending", cfg_bool(&text, "Fog", "sky blending", false));
    j.raw("fog", &fog.done());
    j.bool("distantLand", cfg_bool(&text, "Terrain", "distant terrain", false));
}
