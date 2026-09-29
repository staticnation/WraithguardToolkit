# Cell viewer manifest

Everything under `viewer-shell/` that makes up the cell viewer, after the first strip (grass generation, painting, statics setup, ESP export, profiles and rules managers, install pickers removed). Fill in the **Decision** column (Keep / Strip / Change + notes) for anything else to change.

**Suggested** is my first guess: **Keep** = the viewer needs it; Strip? = grass-generator only; Your call = useful but optional, or mixed with grass code. "Hidden now" = already hidden by `ui/viewer_only.html`; its code still loads.

## Layout

| Path | What | Licence |
|---|---|---|
| `src/main.rs` | Shell entry: URL mode (cell map, NIF viewer, conflicts) and `--cell-viewer` | MIT |
| `src/cellviewer/mod.rs` | Cell viewer window and launch args (`--openmw-cfg`, `--cell`, `--title`) | MIT |
| `src/cellviewer/commands.rs` | The 41 commands the page calls | MIT (from Gardenfell) |
| `viewcore/` | Engine crate | MIT (from Gardenfell) |
| `ui/src/` | Page modules, assembled in `ui/ORDER` order by `build.rs` | GPL-2.0 silo (MGE XE shader ports) |
| `ui/viewer_only.html` | Hides the grass UI, auto-connects, opens the cell picker | GPL-2.0 silo (ours) |
| `ui/tests/boot.js` | Boot check: the page in jsdom against the real engine | MIT |
| `check/`, `check-commands/` | Compile checks without Tauri; `wg-view-serve` headless engine | MIT |
| `capabilities/cellviewer.json` | Window permissions (commands, dialogs, title bar) | MIT |

## 1. UI

Layout is `ui/src/02_body.html`; dialogs marked "built by" are made by a module.

| Area | Item | Suggested | Notes | Decision |
|---|---|---|---|---|
| Top bar | Setup label (what Wraithguard handed over), Report (load order & asset report) | **Keep** | Connect buttons and plugin chooser removed: the setup comes from Wraithguard. | |
| Top bar | Window controls (min/max/close) | **Keep** | Custom title bar; the window is undecorated. | |
| Left `#secCell` | Preview: Cell, Unlit, Brightness, Time of day, Time passes, Time scale, Lights, Atmosphere, Fog density, Sunshafts, Particles, Fog, MGE water, Underwater effects, Ambient occlusion, FXAA, Depth of field, Sun shadows, Adjacent cells, Texture detail, Reload cell | **Keep** | The viewer's controls. | |
| Left `#secCell` | Grass-rendering options: MGE grass light, Wind, Ground tint, Grass distance | Your call | Apply to Gardenfell's generated grass; groundcover plugins currently draw as ordinary statics. | |
| Left `#secCellTex` | Ground textures in cell: coverage per texture, missing-image flag, hover highlight | **Keep** | Repurposed from the rule list. | |
| Viewport toolbar | Orbit/WASD navigation, Grid, Cell arrows, Recentre, Compass, Speed, Highlight objects | **Keep** |  | |
| Viewport toolbar | Empty positions, Highlight culling, Highlight texture, Highlight slot | Your call | Grass-placement debugging from Gardenfell. | |
| Viewport | ORI panel (right-click an object) | **Keep** | New: reference, base record, created/changed by, defined in, mesh and texture sources. | |
| Viewport | Stats box | Your call | Still shows Gardenfell's scatter counters (positions tried, placed, paint marks); to be repurposed. | |
| Dialog `mCell` | Cell picker (list / map, favourites, interiors, visit history) | **Keep** |  | |
| Dialog `mSettings` | Settings: language, navigation, smoothing, theme | **Keep** | Profile and file-store sections removed. | |
| Dialog `mLog` | Load order & asset report | **Keep** |  | |

## 2. Page modules (`ui/src`, load order)

| Module | Size | Suggested | What it does | Decision |
|---|---|---|---|---|
| `01_head.html` | 153 KB | **Keep** | Styles for every panel, themes. | |
| `02_body.html` | 52 KB | **Keep** | Layout; see section 1. | |
| `03_core.js` | 18 KB | **Keep** | Engine bridge (`Engine.call/bytes/pick`), DOM helpers, constants. Everything depends on it. | |
| `04_img.js` | 2 KB | **Keep** | Image helpers the page still does itself. | |
| `05_text.js` | 258 KB | **Keep** | All UI wording + i18n (`T()`). Mostly grass-tool strings; shrinks with the UI. | |
| `06_gl.js` | 478 KB | **Keep** | WebGL2 renderer: terrain, instanced meshes, lighting, fog/tonemap, camera, picking. Also draws grass instances. *Contains MGE XE ports (GPL).* | |
| `07_ini.js` | 29 KB | Your call | Grass config model (rules, canopies, bans). Cell preview reads it to scatter grass; goes only if grass preview goes. | |
| `09_ui.js` | 64 KB | **Keep** | App state + panel rendering. Mixed: viewport state and the grass-rule list UI. | |
| `10_preview.js` | 64 KB | **Keep** | Asset loading (meshes/textures via the engine) and preview rebuild scheduling. | |
| `12_browser.js` | 22 KB | Your call | Asset browser (pick a mesh/texture by path) + the Favourites store the cell picker uses. | |
| `13_mo2.js` | 20 KB | **Keep** | Connecting an install: data folder, MO2 (with profiles), OpenMW cfg. | |
| `15_esp.js` | 10 KB | **Keep** | `GameData`: what the install contains (cells, textures, scopes). | |
| `17_cell.js` | 13 KB | **Keep** | One cell's contents as the engine read them. | |
| `18_cellpreview.js` | 162 KB | **Keep** | The cell viewer: terrain, texture blending, placed statics, lights, interiors, neighbours, cell picker. Also grass scatter hookup and paint-on-cell. | |
| `19_settings.js` | 55 KB | **Keep** | Preview settings (lighting/sky/effect toggles), Settings dialog, themes, language. | |
| `23_viewer_profile.js` | 2 KB | **Keep** | The viewer profile: Wraithguard's (preferences only; setup comes from Wraithguard). | |
| `24_ori.js` | 4 KB | **Keep** | ORI object inspector: right-click an object for its reference, plugins, and asset sources. | |
| `16_gamedata_ui.js` | 27 KB | Your call | Pickers + "matches nothing" report; also the `Busy`/`Status` cards the viewer needs. | |
| `25_sky.js` | 29 KB | **Keep** | The game's sky: weather, sun/moons, clouds, fog colour from the install. | |
| `26_water.js` | 42 KB | **Keep** | MGE XE water + sunshafts. *GPL port.* | |
| `28_particles.js` | 25 KB | **Keep** | NIF particle systems (flames, mist, waterfalls). | |
| `29_ssao.js` | 25 KB | Your call | Ambient occlusion. *GPL port.* Optional effect. | |
| `33_fxaa.js` | 8 KB | Your call | FXAA antialiasing (NVIDIA licence). Optional effect. | |
| `34_outline.js` | 8 KB | **Keep** | Outline highlight for picked objects. | |
| `35_underwater.js` | 16 KB | Your call | Underwater effects. *GPL port.* Optional effect. | |
| `36_dof.js` | 11 KB | Your call | Depth of field. Optional effect. | |
| `37_shadow.js` | 27 KB | Your call | Sun shadows (cascades). Optional effect. | |
| `30_tip.js` | 11 KB | **Keep** | Tooltips. | |
| `31_cellmap.js` | 47 KB | **Keep** | Cell picker world map. | |
| `32_cellhistory.js` | 9 KB | **Keep** | Cell visit history (back/forward). | |
| `11_events.js` | 104 KB | **Keep** | Wires every button/key/mouse event and closes the page. Mixed: grass-panel wiring goes with the panels. | |

## 3. Engine commands (`src/cellviewer/commands.rs`)

"Called from" lists the page modules (by number) that invoke each command.

### Viewer: install & load order (7) - suggested: **Keep**

| Command | Called from | Decision |
|---|---|---|
| `assets` | 13 | |
| `cells` | 13 | |
| `open_install` | 13 | |
| `scopes` | 13 | |
| `startup_install` | 11 | |
| `texture_usage` | 13 | |
| `textures` | 13 | |

### Viewer: cells, meshes, textures, sky (12) - suggested: **Keep**

| Command | Called from | Decision |
|---|---|---|
| `assets_bundle` | 10 | |
| `atmosphere` | 25 | |
| `cell_data` | 17 | |
| `cpu_threads` | 11 | |
| `cursor_park` | 11 | |
| `interior_data` | 17 | |
| `mesh_data` | 10 | |
| `preview_cell` | 18 | |
| `read_asset` | 03, 18 | |
| `texture_data` | 10 | |
| `world_map` | 31 | |
| `world_map_tiles` | 31 | |

### Grass: config / scatter / export (6) - suggested: Your call

| Command | Called from | Decision |
|---|---|---|
| `config_toml` | 19 | |
| `interior_scatter` | 18 | |
| `parse_profile` | 23 | |
| `preview_ask` | 10 | |
| `set_config` | 19 | |
| `validate_config` | 11, 16 | |

### Grass: paint / statics / avoidance (3) - suggested: Strip?

| Command | Called from | Decision |
|---|---|---|
| `avoid_verdict` | 18 | |
| `cull_field` | 18 | |
| `statics_list` | 12 | |

### Shared: settings, language, favourites, files, objects (13) - suggested: **Keep**

| Command | Called from | Decision |
|---|---|---|
| `favourite_set` | 12 | |
| `favourites_list` | 12 | |
| `lang_create` | 19 | |
| `lang_get` | 19 | |
| `lang_list` | 19 | |
| `lang_update` | 19 | |
| `open_folder` | 19 | |
| `ori` | 24 | |
| `picker_set` | 18 | |
| `theme_set` | 19 | |
| `viewer_profile_read` | 23 | |
| `viewer_profile_write` | 23 | |
| `write_text` | 03 | |

## 4. Engine modules (`viewcore/src`)

| Module | Lines | Suggested | What it does | Decision |
|---|---|---|---|---|
| `brush.rs` | 116 | Strip? | Paint brush. | |
| `bsa.rs` | 311 | **Keep** | BSA archive reader. | |
| `canopy.rs` | 279 | Your call | Canopy rules for grass. | |
| `cfgjson.rs` | 714 | Your call | Config across the bridge (grass rules + preview settings). | |
| `cfgtoml.rs` | 1046 | Strip? | Grass rules/profile TOML formats. | |
| `config.rs` | 1272 | Your call | Groundcover .ini + preview .ini. Preview settings live here too, so a split rather than a strip. | |
| `coverage.rs` | 205 | **Keep** | Land-texture coverage per cell. | |
| `esp.rs` | 483 | **Keep** | TES3 .esp/.esm reader. | |
| `img.rs` | 780 | **Keep** | DDS/TGA/BMP decoding and size caps, off the UI thread. | |
| `json.rs` | 390 | **Keep** | Minimal JSON for the bridge. | |
| `land.rs` | 369 | **Keep** | LAND decoding: heights, normals, vertex colours, texture indices. | |
| `lang.rs` | 583 | **Keep** | Language packs. | |
| `layout.rs` | 374 | **Keep** | Mod-manager layouts -> ordered overlay (MO2, OpenMW cfg, vanilla). | |
| `lib.rs` | 649 | **Keep** | Crate root; also holds the grass `generate()` pipeline and GenOptions. | |
| `mesh_scatter.rs` | 1592 | Your call | Grass on meshes (cliffs). | |
| `mge.rs` | 363 | **Keep** | Which MGE is installed and its fog/renderer settings. | |
| `nif.rs` | 4141 | **Keep** | NIF 4.0.0.2 reader: geometry, bounds, collision, KF. | |
| `objects.rs` | 348 | Your call | Scattering references by id. | |
| `paint.rs` | 1028 | Strip? | Painted grass masks. | |
| `patch.rs` | 185 | Your call | Synthetic patch behind the "Simplified" preview mode. | |
| `plugins.rs` | 431 | **Keep** | The load order as a choosable list. | |
| `poisson.rs` | 4126 | Your call | Grass placement (Poisson disc). Needed only if grass preview stays. | |
| `pool.rs` | 158 | **Keep** | Parallel map (no rayon). | |
| `preview.rs` | 773 | **Keep** | Viewport payloads (cell bytes) from the world + scatter. | |
| `profiles.rs` | 718 | Strip? | Profile store. | |
| `renderer.rs` | 566 | **Keep** | Which renderer the install runs (OpenMW/MGE) and its draw settings. | |
| `ruleset.rs` | 45 | Strip? | Grass rules store. | |
| `scattercache.rs` | 359 | Your call | Rescatter cache. | |
| `segdist.rs` | 123 | Your call | Segment-to-triangle distance (grass clearance). | |
| `setup.rs` | 265 | Strip? | Statics setup files + history. | |
| `statics.rs` | 1379 | Strip? | Paint on statics (grass on cliffs). | |
| `terrain.rs` | 169 | **Keep** | Ground queries at a point (texture, slope, height). | |
| `toml.rs` | 499 | **Keep** | Small TOML reader/writer (settings, favourites, languages). | |
| `triaccel.rs` | 218 | **Keep** | Triangle bounding blocks for mesh queries (picking, collision). | |
| `validate.rs` | 364 | Strip? | Grass config validation. | |
| `vfs.rs` | 447 | **Keep** | Ordered overlay filesystem (loose files + BSAs, data paths). | |
| `weather.rs` | 469 | **Keep** | The game's sky from the install (weather, fog, sun). | |
| `world.rs` | 1684 | **Keep** | Load-order resolution: plugins in, merged world out. | |
| `worldmap.rs` | 256 | **Keep** | Cell picker map tiles. | |

## Notes

- Grass is one decision: keep grass preview in the viewer (then `07_ini.js`, `poisson.rs`, `mesh_scatter.rs`, `canopy.rs`, the config commands stay), or remove it entirely.
- The optional effects (SSAO, FXAA, depth of field, shadows, underwater) are self-contained modules. Dropping them does not remove the GPL: `06_gl.js` and `26_water.js` also carry MGE XE ports.
- `05_text.js` (wording) and `01_head.html` (styles) shrink with whatever UI goes.
- "(not called)" commands are registered but no page module invokes them directly (they may be reached via a helper, or be dead).
