# Sound and music in the viewer (plan)

Step 5 of the road in `TODO.md` ("a sound engine"). **Step 1 (hear a file) is built**
(2026-10-27): `viewcore::audio` (Symphonia: WAV in PCM and ADPCM, MP3, Ogg Vorbis, FLAC),
the commands `sound_info` (facts, providers, waveform) and `sound_pcm`, and a player in the
Editor's record dialog for a Sound record and a response's voice file (`ui/src/63_wg_sound.js`:
waveform, play from any point, stop, loop, volume; the SOUN volume as the start). Not yet:
the player in the asset lists and on every field that names a sound, the waveform cache, and
steps 2-4.

## Reference: Resonix

[Resonix](https://github.com/joncarr/resonix) (Tauri 2, Rust and React) is an audio sample
browser: folders, a file table with duration, rate and channels, waveform thumbnails and a
larger cached waveform, play/pause/seek/loop/volume, a live spectrum, favourites, all
cached in SQLite. **It has no licence, so it is all rights reserved: we read it for its
approach and copy nothing** (no code, no assets, no UI). What it shows that applies here:

- Decode with **Symphonia** (WAV, MP3, OGG/Vorbis, FLAC) in Rust, to mono `f32` for analysis.
- A waveform is the decoded samples bucketed into N peaks, cached by file in SQLite, so a
  list of hundreds of files draws without decoding again.
- Playback lives on one thread that owns the output device, driven by commands over a
  channel (an output stream cannot move between threads).
- The spectrum is a tap on the samples being played.

## Where we differ

Resonix plays in Rust (Rodio/CPAL). Ours should **decode in Rust and play in the page
(Web Audio)**:

- Placed sounds have to follow the camera every frame. A `PannerNode` per emitting object
  and the `AudioListener` on the camera do that in the same loop that draws; a Rust player
  would need the camera sent over every frame.
- The page gets PCM from the engine and builds `AudioBuffer`s itself, not
  `decodeAudioData`: what WebKitGTK can decode depends on the machine's GStreamer plugins
  (the Deck's Flatpak runtime differs from a distro's), and Symphonia does not.
- No audio device code in the Rust build (no ALSA or PulseAudio linking on Linux, nothing
  new in the Flatpak).
- The spectrum is a Web Audio `AnalyserNode`.

Symphonia is MPL-2.0 (per-file copyleft): fine to use unmodified from our MIT crates; note
it in `CREDITS.md` when it goes in.

## The engine side (`viewcore::audio`)

Through the VFS (loose files and BSAs, `Vfs::load`), so a sound resolves exactly as the
game resolves it:

- `sound_info(path)`: container and codec, sample rate, channels, bits, duration, peak and
  RMS level, which data folder or archive supplied it (`Vfs::providers`).
- `sound_pcm(path)`: decoded PCM as bytes (a small header, then interleaved 16-bit), the
  same way meshes and textures cross today.
- `sound_peaks(path, n)`: min/max pairs for a waveform, cached by path and modified time
  (in the SQLite cache step 4.1 brings to Rust).

## The page side

One `AudioContext`, with master, effects, voice, music and ambient gains in Settings, off by
default in the cell preview.

1. **Hear a file.** A player in the Editor's Sound record (SOUN) dialog and in the asset
   lists: info, waveform, play/stop/loop/volume. The same for any record field that names a
   sound (doors, lights, magic effects, creature sound generators, a dialogue response's
   voice file).
2. **Hear it in place.** The cell preview's objects that loop a sound (a light's or an
   activator's): a `PannerNode` each, at the object, with SOUN's volume and its min/max
   range (scaled the way OpenMW scales them; read `apps/openmw/mwsound` before choosing the
   numbers). Only objects within range play, under a voice budget.
3. **The world's sound.** A region's ambient sounds by their chances, weather loops, and
   music: the Explore and Battle folders, and OpenMW's playlists.
4. **Checks.** A sound record naming a file no folder supplies; a file the game cannot
   play (what Morrowind.exe and OpenMW each accept differs: check both before writing the
   rule); clipped or silent files; a voice response with no file.

Step 1 is small, and could ride with the Editor's fine details (road step 3) if wanted;
the rest waits for the port.
