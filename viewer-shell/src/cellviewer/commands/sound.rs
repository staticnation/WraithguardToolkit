// Sound: a file's facts and samples, what the cell's objects and regions play, the music lists.
// Part of the command layer (`../commands.rs`, which registers every command and says why
// each is `#[tauri::command(async)]`); split out by page, 2026-11-09.

use super::*;

/* ---- sounds (SOUND.md step 1: hear a file) ------------------------------------------

   A sound is decoded here (`viewcore::audio`, Symphonia) and played by the page with Web
   Audio. The page asks for what the file is and its waveform (`sound_info`), then the PCM
   (`sound_pcm`); the last one decoded is kept so the two do not decode it twice. Paths are as
   the data folders hold them (`Sound/Fx/...`, `Music/Explore/...`), found as the game finds
   them: through the overlay, and a `.wav` nothing supplies as its `.mp3`. */

pub(super) static SOUND: Mutex<Option<(String, std::sync::Arc<viewcore::audio::Decoded>, String, Vec<String>)>> = Mutex::new(None);

/// The decoded sound at `path`, its resolved path and who supplies it (the providers'
/// labels, the winner first).
pub(super) fn sound_of(path: &str, state: &State) -> Result<(std::sync::Arc<viewcore::audio::Decoded>, String, Vec<String>), String> {
    let key = viewcore::vfs::norm(path);
    if let Some((k, d, found, from)) = SOUND.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if *k == key {
            return Ok((d.clone(), found.clone(), from.clone()));
        }
    }
    let v = vfs_of(state)?;
    let (found, loc) = viewcore::audio::candidates(path)
        .into_iter()
        .find_map(|c| v.resolve(&c).map(|l| (c, l)))
        .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
    let from: Vec<String> = v.providers(&found, &[]).into_iter().map(|(label, _)| label).collect();
    let bytes = v.load(&loc).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))?;
    let d = std::sync::Arc::new(viewcore::audio::decode(&bytes).map_err(|e| format!("{found}: {e}"))?);
    *SOUND.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, d.clone(), found.clone(), from.clone()));
    Ok((d, found, from))
}

/// A sound's facts and waveform: `{path (as found), container, codec, bits, rate, channels,
/// seconds, peak, rms, from: [provider labels, the winner first], peaks: [[min, max]...]}`,
/// `peaks` `n` buckets (at most 2000).
#[tauri::command(async)]
pub(super) fn sound_info(path: String, n: usize, state: State) -> Result<String, String> {
    let (d, found, from) = sound_of(&path, &state)?;
    let peaks: Vec<[f32; 2]> = viewcore::audio::peaks(&d, n.min(2000)).into_iter().map(|(a, b)| [a, b]).collect();
    Ok(serde_json::json!({
        "path": found, "container": d.container, "codec": d.codec, "bits": d.bits, "rate": d.rate,
        "channels": d.channels, "seconds": d.seconds(), "peak": d.peak(), "rms": d.rms(), "from": from, "peaks": peaks,
    })
    .to_string())
}

/// A Sound record as OpenMW plays it (`mwsound/soundbuffer.cpp`, `insertSound`): the gain
/// `10^((volume/255*3348 - 3348)/2000)`, the ranges (both 0: `fAudioDefaultMin/MaxDistance`)
/// times `fAudioMin/MaxDistanceMult`, the minimum at least 1 and the maximum at least it.
pub(super) fn sound_played(w: &viewcore::world::World, d: &viewcore::world::SoundDef) -> (f64, f64, f64) {
    let g = |k: &str, def: f32| *w.gmst.get(k).unwrap_or(&def) as f64;
    let gain = 10f64.powf((d.volume as f64 / 255.0 * 3348.0 - 3348.0) / 2000.0);
    let (mut min, mut max) = (d.min as f64, d.max as f64);
    if min == 0.0 && max == 0.0 {
        min = g("faudiodefaultmindistance", 5.0);
        max = g("faudiodefaultmaxdistance", 40.0);
    }
    min *= g("faudiomindistancemult", 20.0);
    max *= g("faudiomaxdistancemult", 50.0);
    let min = min.max(1.0);
    (gain, min, max.max(min))
}

pub(super) fn sound_json(w: &viewcore::world::World, id: &str) -> Option<J> {
    let d = w.sounds.get(id)?;
    if d.file.is_empty() {
        return None;
    }
    let (gain, min, max) = sound_played(w, d);
    let mut o = J::obj();
    o.str("sound", id).str("file", &d.file).num("gain", gain).num("min", min).num("max", max);
    Some(o)
}

/// The sounds placed objects loop (SOUND.md step 2): for each object id asked about that
/// is a light with a sound, or an activator, door or container whose script loops one on
/// itself (`PlayLoopSound3D`; `...VP`'s volume times the record's), `{id, sound, file, gain,
/// min, max, via: "light"|"script"}` as OpenMW plays it.
#[tauri::command(async)]
pub(super) fn sound_emitters(ids: Vec<String>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        let k = id.to_ascii_lowercase();
        if !seen.insert(k.clone()) {
            continue;
        }
        let (snd, vol, via) = if let Some(snd) = w.light_sounds.get(&k) {
            (snd, 1.0f32, "light")
        } else if let Some((snd, vol)) = w.object_scripts.get(&k).and_then(|sc| w.script_loops.get(sc)) {
            (snd, *vol, "script")
        } else {
            continue;
        };
        let Some(d) = w.sounds.get(snd).filter(|d| !d.file.is_empty()) else { continue };
        let (gain, min, max) = sound_played(&w, d);
        let mut o = J::obj();
        o.str("id", &k).str("sound", snd).str("file", &d.file).num("gain", gain * vol.clamp(0.0, 1.0) as f64);
        o.num("min", min).num("max", max).str("via", via);
        rows.push(o.done());
    }
    Ok(format!("[{}]", rows.join(",")))
}

/// The load order's settings the world's sound follows (SOUND.md step 3): every fallback
/// whose key starts `Weather_` and names a sound, the thunder or the environmental sounds'
/// timing, as `{key: value}` - from openmw.cfg's `fallback=` lines, or Morrowind.ini's
/// sections spelled the same way (`[Weather Rain]` `Rain Loop Sound ID` is
/// `Weather_Rain_Rain_Loop_Sound_ID`).
#[tauri::command(async)]
pub(super) fn sound_fallbacks(state: State) -> Result<String, String> {
    let order = { state.app().order_file.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install_is"))?;
    let pairs = viewcore::weather::fallbacks(&order);
    let mut o = J::obj();
    for (k, v) in pairs {
        if wanted_sound_fallback(&k) {
            o.str(&k, &v);
        }
    }
    Ok(o.done())
}

pub(super) fn wanted_sound_fallback(k: &str) -> bool {
    let l = k.to_ascii_lowercase();
    l.starts_with("weather_") && (l.contains("sound") || l.contains("thunder") || l.contains("using_precip"))
}

/// Sound records by id (a weather's loop, SOUND.md step 3): `[{id, sound, file, gain, min,
/// max}]` for those the load order has, as OpenMW plays them.
#[tauri::command(async)]
pub(super) fn sound_records(ids: Vec<String>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let rows: Vec<String> = ids
        .iter()
        .filter_map(|id| {
            let k = id.to_ascii_lowercase();
            let mut o = sound_json(&w, &k)?;
            o.str("id", id);
            Some(o.done())
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

/// The music a folder of the data's `Music/` holds (`explore`, `battle`), as OpenMW's
/// playlists find it: every MP3, WAV, OGG and FLAC under it, loose or packed.
#[tauri::command(async)]
pub(super) fn music_list(kind: String, state: State) -> Result<String, String> {
    let v = vfs_of(&state)?;
    let k: String = kind.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let files = v.list(&format!("music/{k}/"), &[".mp3", ".wav", ".ogg", ".flac"]);
    Ok(string_array(&files))
}

/// The load order's regions for the cell map (TODO, "The world"): `[{id, name, color:
/// [r, g, b]}]`, by name.
#[tauri::command(async)]
pub(super) fn regions_list(state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut list: Vec<&viewcore::world::RegionDef> = w.region_defs.values().collect();
    list.sort_by_key(|r| (if r.name.is_empty() { r.id.to_ascii_lowercase() } else { r.name.to_ascii_lowercase() }, r.id.to_ascii_lowercase()));
    let rows: Vec<String> = list
        .iter()
        .map(|r| {
            let mut o = J::obj();
            o.str("id", &r.id).str("name", &r.name).raw("color", &format!("[{},{},{}]", r.color[0], r.color[1], r.color[2]));
            o.done()
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

/// A region's ambient sounds (SOUND.md step 3): `[{sound, file, gain, min, max, chance}]`.
#[tauri::command(async)]
pub(super) fn region_sounds(region: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let rows: Vec<String> = w
        .region_sounds
        .get(&region.to_ascii_lowercase())
        .map(|l| {
            l.iter()
                .filter_map(|(snd, chance)| {
                    let mut o = sound_json(&w, snd)?;
                    o.num("chance", *chance as f64);
                    Some(o.done())
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(format!("[{}]", rows.join(",")))
}

/// A sound's samples for the page to play (`viewcore::audio::pcm_bytes`: `WGS1`, the header,
/// interleaved little-endian `i16`).
#[tauri::command(async)]
pub(super) fn sound_pcm(path: String, state: State) -> Result<tauri::ipc::Response, String> {
    let (d, _, _) = sound_of(&path, &state)?;
    Ok(tauri::ipc::Response::new(viewcore::audio::pcm_bytes(&d)))
}
