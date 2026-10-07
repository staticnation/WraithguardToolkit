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
| `luacore/` | OpenMW Lua scripts read and checked (lexer, parser, checks, the load-order scan, API tables, report, Mermaid charts); shared with the Python backend (`native/` takes it by path) | MIT |
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
| `50_wg_editor.js` | 19 KB | The Editor mode (`WgEditor`): its state, the links to Wraithguard, entering and leaving, the dock. Each window's methods follow in the `50_wg_editor_*.js` parts below, added to it. |
| `50_wg_editor_objects.js` | 12 KB | The Object Window: tabs, rows, search by any field, long lists a page at a time, new records. |
| `50_wg_editor_cells.js` | 6 KB | The Cell View: every cell, the references in the one selected, the cell on screen, copying an interior. |
| `50_wg_editor_record.js` | 24 KB | The record dialog: a record's fields and lists, renaming, changes sent to the pool. |
| `50_wg_editor_ref.js` | 13 KB | The reference dialog: one placed object's position, rotation, scale, ownership, lock, trap, door. |
| `50_wg_editor_move.js` | 21 KB | Moving the selected object in the render window, box selection, undo/redo (the pool's), copy and paste. |
| `50_wg_editor_qmenu.js` | 19 KB | The Q menu, its settings and QuickStart, layers, the selection of several (Ctrl+click). |
| `50_wg_editor_place.js` | 9 KB | Placing new references; the render window drawn as the pending changes leave it. |
| `50_wg_editor_dialogue.js` | 10 KB | The Dialogue window: topics, responses in the engine's order, drag to reorder. |
| `50_wg_editor_script.js` | 7 KB | The Script Edit window: checked as typed, compiled, saved to the patch. |
| `50_wg_editor_lua.js` | 6 KB | The Lua panel: the load order's OpenMW Lua scripts checked, with their charts. |
| `50_wg_editor_uses.js` | 5 KB | The Use Report and Search & Replace. |
| `50_wg_editor_build.js` | 18 KB | What waits in the pool, and Build patch. |
| `50_wg_editor_keys.js` | 5 KB | The Editor's keys, and the page listeners that feed them. |
| `51_wg_editor_ui.js` | 30 KB | The Editor's window furniture: dockable and foldable panels, the toolbar, flyout menus, the dialogs' folding sections, record previews and drag-to-place feedback. |
| `52_wg_dialogue_views.js` | 20 KB | The Dialogue window's other views: a topic's flow with its choice tree, the topic map, the flags (variables, quests, items) the dialogue writes and tests, and the game's dialogue window rehearsed with an NPC. |
| `53_wg_record_forms.js` | 14 KB | The record dialog's Construction Set form per record type: fields laid out as the CS has them, flags as checkboxes, a weapon's damage grid, a light's colour and flicker, the art file turning and the inventory image. |
| `54_wg_pathgrid_edit.js` | 13 KB | The Editor's path grid mode: the loaded cells' PGRD points drawn with their links and edited in the render window (select, drag, add, link, delete, undo), each cell's grid queued whole in the patch pool. |
| `55_wg_messages.js` | 5 KB | The Editor's Messages panel: every toast and uncaught page error kept (WgLog, in `03_core.js`), filtered by kind and text, copied as text; the toolbar button counts errors not yet seen. |
| `56_wg_workflow.js` | 17 KB | The Editor's workflow: prefabs, new cells, the dialogue condition editor and "who can say this", Script Edit's completion, go-to and uses, a leveled list's roll at a level, and search everything with replace. |
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
| | `lua_scan` | 50 (the Editor's Lua panel: the load order's OpenMW Lua scripts checked) |
| | `editor_record_tag` | 50 (what type a record is, for a link the Editor opens by id) |
| | `navmesh` | 48 (OpenMW's navmesh.db over the loaded cells: the Navmesh overlay, under path grid editing) |
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
| `navmesh.rs` | 700 | OpenMW's navmesh.db (navmeshtool's SQLite cache) read for drawing: its own LZ4 block decoder, the tiles' settings and poly meshes in world units, the collision objects each tile was built from (to find tiles the plugins changed under), the file found beside openmw.cfg or in OpenMW's user data folder. Written from the file's layout, not OpenMW's (GPL-3) code; rusqlite (MIT) with SQLite bundled. |
| `luascan.rs` | 190 | The load order's Lua scripts checked through the overlay (luacore's scan; packed scripts read where they are). |

## Notes

- Dropping an optional effect (SSAO, FXAA, depth of field, shadows, underwater)
  does not remove the GPL: `06_gl.js` and `26_water.js` also carry MGE XE ports.
- The viewer runs on the OS webview: WebView2 on Windows (the `-webview2.zip`
  release bundles a fixed-version runtime), WebKitGTK on Linux (the system's, or
  the GNOME runtime's in the Flatpak), WKWebView on macOS.
