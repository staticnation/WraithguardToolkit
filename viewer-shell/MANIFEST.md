# Viewer shell manifest

What is in `viewer-shell/`: `wraithguard-viewer`, the Tauri app behind the cell
viewer (Cell Preview), the mesh viewer and the in-app cell map. The viewer came
from Gardenfell (Robin Hjelte, MIT) and was stripped to a viewer - grass
generation, painting, statics setup, ESP export and the profile/rules managers are
gone from the page, the commands and the engine (4.1.x). This file lists what is
left and what each part is for.

## Layout

| Path | What | Licence |
|---|---|---|
| `src/main.rs` | Shell entry: URL mode (a page Wraithguard serves over loopback - the cell map, conflict views) and `--cell-viewer` (the page below). Generates the Tauri context once for both | MIT |
| `src/cellviewer/mod.rs` | The cell/mesh viewer window and its launch arguments (`--openmw-cfg`, `--cell`, `--title`, `--theme`, `--prefs`, `--mesh-view`, `--extra`) | MIT |
| `src/cellviewer/commands.rs` | The 38 commands the page calls (below) | MIT (from Gardenfell) |
| `viewcore/` | The engine crate, on greatness7's `tes3` crates | MIT (from Gardenfell) |
| `ui/src/` | Page modules, concatenated in `ui/ORDER` order by `build.rs` | **GPL-2.0** silo (MGE XE shader ports) |
| `ui/assets/` | Files shipped beside the page (copied into `ui-dist` by `build.rs`): MGE XE's wave volume `water_NRM.dds`, for setups without MGE XE | GPL-2.0 silo (MGE XE asset) |
| `ui/viewer_only.html` | Hides what the viewer does not use, opens the cell picker | GPL-2.0 silo |
| `ui/tests/boot.js` | Boot check: the page in jsdom against the real engine (`wg-view-serve`), cell and mesh mode | MIT |
| `check/` | Compile check of `main.rs` against a stub Tauri (no WebKit/WebView needed in CI) | MIT |
| `check-commands/` | Compiles `commands.rs` without Tauri, and builds `wg-view-serve`, the headless engine the boot test drives | MIT |
| `capabilities/cellviewer.json` | Window permissions (commands, dialogs, title bar) | MIT |

The page is a separate frontend loaded into the webview, so the GPL-2.0 silo does
not reach the MIT code. See `ui/LICENSE`, `../License/Gardenfell`,
`../License/MGE-XE`, and `CREDITS.md`.

## Page modules (`ui/src`, in load order)

| Module | Size | What it does |
|---|---|---|
| `01_head.html` | 155 KB | Styles for every panel; themes. |
| `02_body.html` | 56 KB | Layout: top bar, left panels, viewport, dialogs. |
| `03_core.js` | 16 KB | Engine bridge (`Engine.call/bytes/pick`), DOM helpers, constants. Everything depends on it. |
| `04_img.js` | 2 KB | Image helpers the page still does itself. |
| `05_text.js` | 49 KB | All UI wording and i18n (`T()`). |
| `06_gl.js` | 490 KB | WebGL2 renderer: terrain, instanced meshes, lighting, fog/tonemap, camera, picking. *Contains MGE XE ports.* |
| `07_ini.js` | 4 KB | The small config model the settings still read. |
| `09_ui.js` | 30 KB | App state and panel rendering. |
| `10_preview.js` | 63 KB | Asset loading (meshes/textures through the engine), animation playback (`animVisibleAt`), preview rebuild scheduling. |
| `12_browser.js` | 6 KB | Asset lists for the pickers, and the Favourites store the cell picker uses. |
| `13_mo2.js` | 20 KB | Connecting an install: the setup Wraithguard hands over (openmw.cfg, data paths, groundcover). |
| `15_esp.js` | 10 KB | `GameData`: what the install contains (cells, textures, scopes). |
| `17_cell.js` | 13 KB | One cell's contents as the engine read them. |
| `18_cellpreview.js` | 151 KB | The cell viewer: terrain, texture blending, placed objects, lights, interiors, neighbours, groundcover. |
| `19_settings.js` | 54 KB | Preview settings (lighting, sky, water, effects), the Settings dialog, themes, language. |
| `23_viewer_profile.js` | 1 KB | The viewer profile: preferences only; the setup comes from Wraithguard. |
| `24_ori.js` | 5 KB | ORI object inspector: click an object for its reference, plugins and asset sources. |
| `16_gamedata_ui.js` | 13 KB | Pickers, and the `Busy`/`Status` cards. |
| `25_sky.js` | 28 KB | The game's sky: weather, sun and moons, clouds, fog colour from the install. |
| `26_water.js` | 60 KB | MGE XE water (dynamic-ripple waves, reflection blur, hue/opacity tint, surf, sewer waves, the bundled wave volume) and sunshafts. *GPL port; see `CREDITS.md` for the surf and sewer-wave sources.* |
| `28_particles.js` | 25 KB | NIF particle systems (flames, mist, waterfalls). |
| `29_ssao.js` | 37 KB | Ambient occlusion (SSAO, or SSGI). *GPL port.* |
| `33_fxaa.js` | 8 KB | FXAA antialiasing. |
| `34_outline.js` | 8 KB | Outline highlight for picked and highlighted objects. |
| `35_underwater.js` | 16 KB | Underwater effects. *GPL port.* |
| `36_dof.js` | 11 KB | Depth of field. |
| `37_shadow.js` | 27 KB | Sun shadows (cascades). |
| `30_tip.js` | 11 KB | Tooltips. |
| `31_cellmap.js` | 51 KB | The cell picker's world map. |
| `32_cellhistory.js` | 9 KB | Cell visit history (back/forward). |
| `38_wg_viewport.js` | 5 KB | Wraithguard's viewport settings: zoom to cursor, frame-rate cap, pause while hidden. |
| `39_wg_coverage.js` | 6 KB | Wraithguard's cell map inside the cell picker: which mods touch which cells. |
| `40_wg_modhl.js` | 2 KB | Highlight every object a chosen plugin supplied. |
| `41_wg_meshview.js` | 23 KB | The mesh viewer: one mesh, or each mod's copy side by side, with the block tree and field editor. |
| `42_wg_bloom.js` | 8 KB | Bloom: MGE XE's Bloom Fine (Hrnchamd), after the sunshafts and before the depth of field. *GPL port.* |
| `43_wg_rain.js` | 7 KB | Rain ripples: MGE XE's precipitation ripple simulation (drops and the damped wave equation) for Rain, Thunderstorm, Snow and Blizzard, added onto the water's close normal. *GPL port.* |
| `44_wg_precip.js` | 12 KB | Weather effects: OpenMW-style rain around the camera in Rain and Thunderstorm, and the game's snow, blizzard, ash and blight particle meshes carried with the camera. |
| `45_wg_nav.js` | 3 KB | From Wraithguard's conflict viewer: a record found where it stands, the cell opened on it, and what Wraithguard asks an open viewer to show. |
| `46_wg_tfh.js` | 10 KB | The full help on the right: owner, lock, contents and inventory with the game's icons, leveled lists, services, travel, spells, dialogue, and the contents in the mesh viewer. |
| `11_events.js` | 90 KB | Wires every button, key and mouse event (WASD fly, orbit, picking). Loads last. |

## Engine commands (`src/cellviewer/commands.rs`)

"Called from" lists the page modules (by number) that invoke each command.

| Group | Command | Called from |
|---|---|---|
| Install & load order | `startup_install` | 11 |
| | `open_install` | 13, 15 |
| | `scopes` | 13 |
| | `textures` | 05, 13 |
| | `texture_usage` | 13 |
| | `assets` | 12, 13 |
| | `cells` | 06, 13, 18, 31 |
| Cells & world | `cell_data` | 17, 18 |
| | `interior_data` | 17 |
| | `cell_coverage` | 39 |
| | `world_map` | 31 |
| | `world_map_tiles` | 31 |
| | `atmosphere` | 19, 25 |
| | `ori` | 24, 41 |
| | `ori_dialogue` | 24 (an actor's topics and quests, read from every plugin on request) |
| | `wg_open_record` | 24 (asks Wraithguard to show a record in its conflict viewer, over loopback) |
| | `find_record` | 45 (where a record stands, for the conflict viewer's Show in Cell Preview) |
| | `wg_post` | 45 (asks Wraithguard for the next place to show) |
| Meshes & textures | `mesh_data` | 03, 10 |
| | `texture_data` | 03, 10 |
| | `assets_bundle` | 10 |
| | `read_asset` | 03, 10, 18 |
| | `mesh_collision` | 41 |
| | `wg_save_edited` | 41 (hands an edited mesh back to Wraithguard over loopback) |
| Settings & files | `viewer_profile_read` / `viewer_profile_write` | 23 |
| | `theme_set` | 19 |
| | `picker_set` | 18 |
| | `favourites_list` / `favourite_set` | 12 |
| | `lang_list` / `lang_get` / `lang_create` / `lang_update` | 19 |
| | `open_folder` | 19 |
| | `write_text` | 03 |
| Machine | `cpu_threads` | 11 |
| | `cursor_park` | 11 |

`check-commands/build.rs` fails the build if `generate_handler!` names a command
that is not defined, or a `#[tauri::command]` is left unregistered.

## Engine modules (`viewcore/src`)

| Module | Lines | What it does |
|---|---|---|
| `lib.rs` | 29 | Crate root. |
| `vfs.rs` | 447 | Ordered overlay filesystem: loose files and archives, in `data=` order. |
| `bsa.rs` | 283 | Archives, on `tes3::bsa`. |
| `layout.rs` | 374 | Mod-manager layouts to an ordered overlay (OpenMW cfg, MO2, vanilla). |
| `plugins.rs` | 431 | The load order as a list. |
| `world.rs` | 1383 | Load-order resolution: plugins in, merged world out. |
| `world/from_crate.rs` | 245 | Records read through `tes3::esp`; LAND and creatures stay on the raw reader, and any record the crate refuses falls back to it. |
| `esp.rs` | 483 | The raw TES3 record reader (the fallback above). |
| `npc.rs` | 560 | NPCs assembled for drawing: races, body parts, NPC records and worn items read from the load order; the skeleton with its idle, the parts chosen by OpenMW's equipment priorities, bound to its bones (`__npc/<id>` mesh paths). |
| `inspect.rs` | 330 | The object inspector's full help, read on demand from the plugin files: a reference's owner, lock, key, trap, soul, charge and count; a base record's script, contents, inventory and spells; an actor's dialogue topics and quests. |
| `objects.rs` | 174 | Object definitions by id (and leveled-list resolution) for the world. |
| `refkey.rs` | 38 | A reference's stable name: its plugin and index. |
| `land.rs` | 369 | LAND decoding: heights, normals, vertex colours, texture indices. |
| `coverage.rs` | 162 | Land-texture coverage per cell, and which plugins touch each cell. |
| `nif.rs` | 4285 | Meshes for drawing: geometry, bounds, collision, animation (KF, visibility, particles). |
| `nif/from_crate.rs` | 465 | Meshes read through `tes3::nif`, mapped into the tables above; the own reader stays as the fallback. |
| `preview.rs` | 533 | Meshes packed as the bytes the viewport draws, and their animation as JSON. |
| `bcx.rs` | 470 | BC4, BC5 and BC7 decoding, for a webview that cannot take the blocks (the toolkit's own decoders, from `native/src/img.rs`). |
| `img.rs` | 780 | DDS/TGA/BMP decoding and size caps, off the UI thread. |
| `weather.rs` | 469 | The game's sky from the install (weather, fog, sun). |
| `renderer.rs` | 566 | Which renderer the install runs (OpenMW/MGE XE) and its draw settings. |
| `mge.rs` | 363 | Which MGE XE is installed and its fog/renderer settings. |
| `worldmap.rs` | 256 | Cell picker map tiles. |
| `profiles.rs` | 629 | Files beside the program: the remembered install, theme, cell-picker choice. |
| `lang.rs` | 583 | Language packs. |
| `toml.rs` | 488 | Small TOML reader/writer (settings, favourites, languages). |
| `json.rs` | 390 | Minimal JSON for the bridge. |
| `pool.rs` | 158 | Parallel map (no rayon). |

## Notes

- Dropping an optional effect (SSAO, FXAA, depth of field, shadows, underwater)
  does not remove the GPL: `06_gl.js` and `26_water.js` also carry MGE XE ports.
- The viewer runs on the OS webview: WebView2 on Windows (the `-webview2.zip`
  release bundles a fixed-version runtime), WebKitGTK on Linux (the AppImage
  bundles it), WKWebView on macOS.
