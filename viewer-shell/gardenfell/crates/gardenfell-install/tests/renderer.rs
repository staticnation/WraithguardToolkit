//! The renderer the install runs, read from its own files — round 18i.
//!
//! Robin: "We have rules to identify what install we are using, and that should identify
//! what we do in how we render the scene [...] I want the tool to be used by anyone, no
//! matter what version of MGE XE they use, or OpenMW."
//!
//! The lamp coefficients are the round's proof. His game-folder `Morrowind.ini` says
//! constant 0.0 / linear 3.0 / quadratic 0; the MO2 *profile's* ini — the one the game
//! reads — says 0.34 / 0 / 3.25; and MWSE, asked in the running game, reported
//! `c=0.3400 l=0.000000 q=0.00004959` on a 256-radius lantern and `q=0.00006535` on a
//! 223 (3.25 / r² both). The reader below reproduces those numbers from that ini text.

use gardenfell_bytes::json::{parse, Val};
use gardenfell_install::layout::Layout;
use gardenfell_install::renderer::{dll_label, renderer_json, Attenuation};
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("gdn-renderer-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Robin's MO2 profile ini, the [LightAttenuation] block verbatim.
const PROFILE_INI: &str = "[General]\r\nMax FPS=240\r\n\r\n[LightAttenuation]\r\nUseConstant=1\r\nConstantValue=0.340\r\n\
UseLinear=1\r\nLinearMethod=1\r\nLinearValue=0.000\r\nLinearRadiusMult=1.0\r\nUseQuadratic=1\r\nQuadraticMethod=2\r\n\
QuadraticValue=3.250\r\nQuadraticRadiusMult=1.0\r\nOutQuadInLin=0\r\n\r\n[Weather]\r\nSunrise Time=6\r\n";

/// His game-folder ini — the stale one, which is exactly why it must not be the file read.
const GAME_INI: &str = "[LightAttenuation]\nUseConstant=1\nConstantValue=0.0\n;\nUseLinear=1\nLinearMethod=1\nLinearValue=3.0\n\
LinearRadiusMult=1.0\n;\nUseQuadratic=1\nQuadraticMethod=2\nQuadraticValue=0\nQuadraticRadiusMult=1.0\nOutQuadInLin=0\n";

#[test]
fn the_profile_ini_gives_the_coefficients_mwse_reported() {
    let a = Attenuation::from_ini(PROFILE_INI);
    assert!(a.found);
    let [c, l, q] = a.coefficients(256.0, false);
    assert!((c - 0.34).abs() < 1e-6, "{c}");
    assert_eq!(l, 0.0);
    assert!((q - 0.00004959).abs() < 2e-8, "{q}");
    let [_, _, q223] = a.coefficients(223.0, false);
    assert!((q223 - 0.00006535).abs() < 2e-8, "{q223}");
    let [_, _, q16] = a.coefficients(16.0, false);
    assert!((q16 - 0.01269531).abs() < 1e-7, "a 16-radius 'off' lantern: {q16}");
}

#[test]
fn the_game_folder_ini_would_have_given_linear_only() {
    let a = Attenuation::from_ini(GAME_INI);
    let [c, l, q] = a.coefficients(256.0, false);
    assert_eq!(c, 0.0);
    assert!((l - 3.0 / 256.0).abs() < 1e-7, "{l}");
    assert_eq!(q, 0.0);
}

#[test]
fn vanilla_and_openmw_fallbacks_read_alike() {
    // No section at all: the shipped ini's values.
    let v = Attenuation::from_ini("[General]\nMax FPS=60\n");
    assert!(!v.found);
    assert_eq!(v.coefficients(256.0, false), [0.0, 3.0 / 256.0, 0.0]);
    // openmw.cfg spells the same block as fallback= lines.
    let cfg = "data=\"C:\\\\mw\\\\Data Files\"\nfallback=LightAttenuation_UseConstant,1\nfallback=LightAttenuation_ConstantValue,0.34\n\
fallback=LightAttenuation_UseLinear,0\nfallback=LightAttenuation_UseQuadratic,1\nfallback=LightAttenuation_QuadraticMethod,2\n\
fallback=LightAttenuation_QuadraticValue,3.25\nfallback=Weather_Clear_Sky_Day_Color,095,135,203\n";
    let o = Attenuation::from_ini(cfg);
    assert!(o.found);
    let [c, l, q] = o.coefficients(256.0, false);
    assert!((c - 0.34).abs() < 1e-6 && l == 0.0 && (q - 3.25 / 65536.0).abs() < 1e-9, "{c} {l} {q}");
}

#[test]
fn out_quad_in_lin_swaps_the_terms_by_cell_and_all_zero_means_constant_one() {
    let mut a = Attenuation::vanilla();
    a.use_linear = false;
    a.use_quadratic = false;
    a.out_quad_in_lin = true;
    a.quadratic_value = 16.0;
    // Outdoors the quadratic, indoors the linear, whatever the Use flags say.
    assert_eq!(a.coefficients(256.0, false), [0.0, 0.0, 16.0 / 65536.0]);
    assert_eq!(a.coefficients(256.0, true), [0.0, 3.0 / 256.0, 0.0]);
    a.out_quad_in_lin = false;
    assert_eq!(a.coefficients(256.0, false), [1.0, 0.0, 0.0], "nothing enabled: a constant 1, not a division by zero");
    // Radii under 16 are treated as 16, as OpenMW does (the engine's lights never go lower).
    let b = Attenuation::from_ini(PROFILE_INI);
    assert_eq!(b.coefficients(1.0, false), b.coefficients(16.0, false));
}

#[test]
fn the_shim_names_itself() {
    let mut g7 = vec![0u8; 100];
    g7.extend_from_slice(b"MGE-XE G7 Fork v0.20.0\0mgeXE.toml\0");
    assert_eq!(dll_label(&g7).as_deref(), Some("MGE-XE G7 Fork v0.20.0"));
    let mut uf = vec![0u8; 100];
    uf.extend_from_slice(b"MGE XE is not configured\0MGE XE 0.19.1\0mgeXE.log\0");
    assert_eq!(dll_label(&uf).as_deref(), Some("MGE XE 0.19.1"));
    assert_eq!(dll_label(b"some other dll"), None);
}

// ---- the whole reply, per install --------------------------------------------------

fn dll(path: &Path, markers: &[&str]) {
    let mut b = vec![0u8; 512];
    for m in markers {
        b.extend_from_slice(m.as_bytes());
        b.extend_from_slice(&[0u8; 16]);
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b).unwrap();
}

const TOML: &str = r#"
schema_version = 2
[render]
fov = 127.9096
enable_shaders = true
[distant_land]
enabled = true
per_pixel_lighting = true
per_pixel_mode = "always"
expanded_light_limit = true
render_grass = true
draw_distance = 8.0
[distant_land.water]
wave_height = 50
caustics_intensity = 50
reflect_land = true
reflect_near_statics = true
reflect_sky = true
blur_reflections = true
[distant_land.fog]
above_water_start = 2.32
above_water_end = 8.0
[distant_land.shadows]
enabled = true
map_resolution = 2048
[lighting.weather.clear]
sun = 1.0
ambient = 0.9
[shaders]
chain = ["SSAO HQ", "Underwater Effects", "Sunshafts", "Bloom Fine", "Depth of Field", "FXAA"]
"#;

const INI: &str = "[Render State]\r\nHardware Shader=True\r\nHorizontal Screen FOV=127.91\r\n[Distant Land]\r\nDistant Land=On\r\nDraw Distance=5\r\n\
Water Wave Height=40\r\nWater Caustics Intensity=60\r\nSun Shadows=True\r\nSun Shadow Map Resolution=1024\r\nPer Pixel Shader=True\r\n\
Per Pixel Shader Flags=Interiors only\r\nRender Grass=True\r\n[Per Pixel Lighting]\r\nClear Sun Brightness=1.10\r\nClear Ambient Brightness=1.00\r\n\
[Shader Chain]\r\nSSAO HQ\r\nSunshafts\r\ndeband_fogaware\r\n";

fn layout_for(game: &Path, order: &Path) -> Layout {
    Layout { game_dir: Some(game.to_path_buf()), order_file: order.to_path_buf(), ..Default::default() }
}

fn s<'a>(v: &'a Val, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}
fn b(v: &Val, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}
fn f(v: &Val, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64()).unwrap_or(f64::NAN)
}

#[test]
fn the_g7_fork_is_read_from_its_toml() {
    let d = tmp("g7");
    std::fs::write(d.join("mgeXE.toml"), TOML).unwrap();
    dll(&d.join("d3d8.dll"), &["mgeXE.toml", "MGE-XE G7 Fork v0.20.0"]);
    let order = d.join("Morrowind.ini");
    std::fs::write(&order, PROFILE_INI).unwrap();
    let v = parse(&renderer_json(&layout_for(&d, &order), PROFILE_INI)).expect("json");
    assert_eq!(s(&v, "engine"), "mgexe");
    assert_eq!(s(&v, "fork"), "g7");
    assert_eq!(s(&v, "label"), "MGE-XE G7 Fork v0.20.0");
    assert!(s(&v, "file").ends_with("mgeXE.toml"), "{}", s(&v, "file"));
    let l = v.get("lighting").unwrap();
    assert_eq!(s(l, "model"), "mge-ppl");
    assert!(b(l, "perPixel") && s(l, "perPixelMode") == "always");
    assert_eq!(f(l, "sunMult"), 1.0);
    assert!((f(l, "ambientMult") - 0.9).abs() < 1e-6);
    assert!(b(l, "expandedLightLimit"));
    assert!(!b(l, "clampLighting"), "the tonemap, not a clamp");
    let p = v.get("post").unwrap();
    assert!(b(p, "enabled") && b(p, "ssao") && b(p, "sunshafts") && b(p, "bloom") && b(p, "dof") && b(p, "underwater"));
    assert!(b(p, "fxaa"), "18do: the chain names FXAA");
    assert_eq!(p.get("chain").unwrap().as_arr().unwrap().len(), 6);
    let w = v.get("water").unwrap();
    assert_eq!(f(w, "waveHeight"), 50.0);
    assert!(b(w, "blur"));
    let sh = v.get("shadows").unwrap();
    assert!(b(sh, "enabled") && f(sh, "resolution") == 2048.0);
    let fov = v.get("fov").unwrap();
    assert!((f(fov, "deg") - 127.9096).abs() < 1e-3 && s(fov, "axis") == "h");
    assert_eq!(f(&v, "drawDistance"), 8.0);
    assert!(b(&v, "grass") && b(&v, "distantLand"));
    // The attenuation rides along, from the order file, with a worked example at 256.
    let a = v.get("attenuation").unwrap();
    assert!(b(a, "found"));
    let ex = a.get("at256").unwrap().as_arr().unwrap();
    assert!((ex[0].as_f64().unwrap() - 0.34).abs() < 1e-6);
    assert!(s(&v, "attenuationFile").ends_with("Morrowind.ini"));
}

#[test]
fn an_ini_build_with_interior_only_ppl_leaves_the_exterior_on_the_fixed_pipeline() {
    let d = tmp("ini");
    std::fs::create_dir_all(d.join("mge3")).unwrap();
    std::fs::write(d.join("mge3").join("MGE.ini"), INI).unwrap();
    dll(&d.join("d3d8.dll"), &["MGE3/MGE.ini", "MGE XE 0.19.1"]);
    let order = d.join("Morrowind.ini");
    std::fs::write(&order, GAME_INI).unwrap();
    let v = parse(&renderer_json(&layout_for(&d, &order), GAME_INI)).expect("json");
    assert_eq!(s(&v, "engine"), "mgexe");
    assert_eq!(s(&v, "fork"), "");
    assert_eq!(s(&v, "label"), "MGE XE 0.19.1");
    let l = v.get("lighting").unwrap();
    assert_eq!(s(l, "model"), "ffp", "per-pixel lighting in interiors only: the exterior is vertex-lit");
    assert!(b(l, "perPixel"));
    assert!((f(l, "sunMult") - 1.10).abs() < 1e-6);
    let p = v.get("post").unwrap();
    assert!(b(p, "ssao") && b(p, "sunshafts") && !b(p, "bloom") && !b(p, "fxaa"));
    let w = v.get("water").unwrap();
    assert_eq!(f(w, "waveHeight"), 40.0);
    assert_eq!(f(w, "caustics"), 60.0);
    let fov = v.get("fov").unwrap();
    assert!((f(fov, "deg") - 127.91).abs() < 1e-3);
    assert_eq!(f(&v, "drawDistance"), 5.0);
}

#[test]
fn openmw_is_read_from_settings_cfg_beside_openmw_cfg() {
    let d = tmp("openmw");
    let cfg = d.join("openmw.cfg");
    let data = d.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let cfg_text = format!("data=\"{}\"\nfallback=LightAttenuation_UseConstant,0\nfallback=LightAttenuation_UseLinear,1\nfallback=LightAttenuation_LinearMethod,1\nfallback=LightAttenuation_LinearValue,3.0\n", data.display());
    std::fs::write(&cfg, &cfg_text).unwrap();
    std::fs::write(
        d.join("settings.cfg"),
        "[Shaders]\nlighting method = shaders\nlight bounds multiplier = 2.0\nclamp lighting = false\n[Camera]\nfield of view = 75\nviewing distance = 16384\n\
[Groundcover]\nenabled = true\ndensity = 0.5\n[Post Processing]\nenabled = true\nchain = ssao_hq,bloom\n[Shadows]\nenable shadows = true\n[Fog]\nuse distant fog = true\ndistant underwater fog start = -2048\n",
    )
    .unwrap();
    let layout = Layout { game_dir: None, order_file: cfg.clone(), ..Default::default() };
    let v = parse(&renderer_json(&layout, &cfg_text)).expect("json");
    assert_eq!(s(&v, "engine"), "openmw");
    assert!(s(&v, "file").ends_with("settings.cfg"));
    let l = v.get("lighting").unwrap();
    assert_eq!(s(l, "model"), "openmw-shaders");
    assert_eq!(s(l, "lightingMethod"), "shaders");
    assert_eq!(f(l, "lightBoundsMultiplier"), 2.0);
    assert_eq!(f(l, "maximumLightDistance"), 8192.0, "a key left out is OpenMW's default");
    assert!(!b(l, "clampLighting"));
    let p = v.get("post").unwrap();
    assert!(b(p, "enabled") && b(p, "ssao") && b(p, "bloom"));
    let fov = v.get("fov").unwrap();
    assert!(f(fov, "deg") == 75.0 && s(fov, "axis") == "v");
    assert_eq!(f(&v, "drawDistance"), 2.0, "16384 units is two cells");
    assert!(b(&v, "grass"));
    assert_eq!(f(v.get("groundcover").unwrap(), "density"), 0.5);
    assert!(b(v.get("shadows").unwrap(), "enabled"));
    assert!(b(v.get("fog").unwrap(), "useDistantFog"));
    // Round 18dt: the distant underwater pair, one key set and one at OpenMW's default.
    assert_eq!(f(v.get("fog").unwrap(), "distantUnderwaterFogStart"), -2048.0);
    assert!((f(v.get("fog").unwrap(), "distantUnderwaterFogEnd") - 2457.6).abs() < 1e-3);
    let a = v.get("attenuation").unwrap();
    assert!(b(a, "found") && !b(a, "useConstant"));
    // Legacy is the fixed pipeline by another name; no settings.cfg at all is OpenMW's defaults.
    std::fs::write(d.join("settings.cfg"), "[Shaders]\nlighting method = legacy\n").unwrap();
    let v = parse(&renderer_json(&layout, &cfg_text)).expect("json");
    assert_eq!(s(v.get("lighting").unwrap(), "model"), "ffp");
    std::fs::remove_file(d.join("settings.cfg")).unwrap();
    let v = parse(&renderer_json(&layout, &cfg_text)).expect("json");
    assert_eq!(s(v.get("lighting").unwrap(), "model"), "openmw-shaders");
    assert!(s(&v, "how").contains("defaults"), "{}", s(&v, "how"));
    assert!(!b(v.get("post").unwrap(), "found"));
}

#[test]
fn no_mge_and_no_openmw_is_the_fixed_pipeline() {
    let d = tmp("vanilla");
    let order = d.join("Morrowind.ini");
    std::fs::write(&order, "[General]\n").unwrap();
    let v = parse(&renderer_json(&layout_for(&d, &order), "[General]\n")).expect("json");
    assert_eq!(s(&v, "engine"), "vanilla");
    assert_eq!(s(v.get("lighting").unwrap(), "model"), "ffp");
    assert!(b(v.get("lighting").unwrap(), "clampLighting"));
    assert!(!b(v.get("post").unwrap(), "ssao"));
    let a = v.get("attenuation").unwrap();
    assert!(!b(a, "found"), "no section: vanilla's numbers, and said so");
    let ex = a.get("at256").unwrap().as_arr().unwrap();
    assert!((ex[1].as_f64().unwrap() - 3.0 / 256.0).abs() < 1e-7);
}


