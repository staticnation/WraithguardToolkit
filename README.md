# Wraithguard Toolkit

A desktop program for building, checking and editing a Morrowind load order on
[OpenMW](https://openmw.org/).

It started as one job, and that job is still at its center: sort **only the mods you added**
into a curated [Modding-OpenMW.com](https://modding-openmw.com/) (MOMW) list without ever
reordering the list itself, and save the result as a `momw-customizations.toml` that
survives every MOMW Configurator rebuild. Around it, the program now holds the rest of what
a modded setup needs:

- a **3D Cell Preview** of any cell in your load order, with its scripts running;
- **Conflicts**, record by record and file by file;
- an **Editor** in the Construction Set's mold that writes its changes into one new patch
  plugin;
- a **Land** view for sculpting and painting terrain;
- **Tools** for linting, backups, broken masters, save checks, tes3cmd, Merged Lands and
  more.

One program for Windows, macOS and Linux, with a command line beside it for headless use.

**New here? [QUICKSTART.md](QUICKSTART.md) is the five-minute version.** Writing your own
sort rules? See [MLOX_RULES.md](MLOX_RULES.md). Both also open from **Help** inside the
program.

---

## Contents

- [Getting it](#getting-it)
- [First start: Setup](#first-start-setup)
- [The pages](#the-pages)
  - [Cell Preview](#cell-preview)
  - [Load order](#load-order)
  - [Conflicts](#conflicts)
  - [Editor](#editor)
  - [Land](#land)
  - [Tools](#tools)
  - [Help, Report and Settings](#help-report-and-settings)
- [The command line](#the-command-line)
- [Where files live](#where-files-live)
- [Safety](#safety)
- [Building and developing](#building-and-developing)
- [Project layout](#project-layout)
- [Credits and licenses](#credits-and-licenses)

---

## Getting it

Download a package from the
[Releases page](https://github.com/staticnation/WraithguardToolkit/releases). Each holds
`wraithguard-toolkit` (the program), `wraithguard` (the command line) and the documents the
Help page shows. Unpack it anywhere and run the program; there is nothing to install.

| Platform | Package | Notes |
|---|---|---|
| Windows 10 or later | `wraithguard-windows-x86_64.zip` | Uses the WebView2 runtime Windows ships with. |
| Windows without WebView2 | `wraithguard-windows-x86_64-webview2.zip` | Carries its own runtime in a `webview2` folder beside the program, for PCs where WebView2 was removed (with Edge, LTSC, stripped images). |
| macOS, Apple Silicon | `wraithguard-macos-arm64.tar.gz` | Unsigned: right-click > Open the first time, or `xattr -dr com.apple.quarantine wraithguard-toolkit`. |
| macOS, Intel | `wraithguard-macos-x86_64.tar.gz` | As above. |
| Linux and the Steam Deck | `wraithguard-linux-x86_64.AppImage` | Everything it needs is inside. Make it executable (`chmod +x`, or Properties > Permissions), then run it. On the Deck, use Desktop Mode; add it to Steam as a non-Steam game to start it from Game Mode. Its settings are kept beside the `.AppImage` file. |
| Linux, plain | `wraithguard-linux-x86_64.tar.gz` | Smaller, with the command line. Needs the system's WebKitGTK 4.1 (`libwebkit2gtk-4.1-0`: Ubuntu 22.04+, Debian 12+, Fedora, Arch). |

**Verifying a download.** Each release lists the packages' SHA-256 sums. Compare them with
`certutil -hashfile <file> SHA256` (Windows) or `sha256sum <file>`.

> **Coming from 4.x?** 5.0 is a single Rust program; the Python/Tk app is retired. Your
> settings file carries over. A patch queue left by the 4.x Editor does not, so build it
> into a patch with 4.x before you upgrade.

---

## First start: Setup

The first start opens **Setup**: every path the program runs on, in one place. Each field
is checked as you type, the same way the program will read it. Missing data folders, a TOML
that doesn't parse and rule files that aren't there are named, with what to do about them,
before anything is loaded. **Save and load** saves the settings and loads your install.

| Field | What it is |
|---|---|
| **openmw.cfg** | The cfg OpenMW starts with. Its `data=` folders and `content=` files are the load order every page reads. **Find** looks where OpenMW keeps it. |
| **OpenMW folder** | Where OpenMW is installed. The Lua checks read its API from here, and the Cell Preview runs OpenMW Lua with its LuaJIT and built-in scripts. Optional. |
| **Rule files** | mlox rule files, one per line, lowest priority first: `mlox_base.txt`, then `mlox_user.txt`, then your own. A folder means its `.txt` files. |
| **Your rule file** | Where the rule maker adds your own rules. Keep it last among the rule files. |
| **customizations.toml** | A `momw-customizations.toml`: the mods you add to a curated list. Optional. |
| **Subset file** | A plain list of your plugins and their folders, one per line, instead of a TOML. Optional. |
| **Mods folder** | A folder of mods, scanned into the sort each time it runs. Optional. |
| **plugin-order.yml**, **List name** | MOMW's list of which plugins belong to which curated list, and the list you follow (`total-overhaul`, `expanded-vanilla` ...). |
| **Data-paths cache** | The list's `data=` order, fetched for sorting data paths. |
| **Write customizations to** | Where Export writes the corrected TOML. Empty: back into customizations.toml. |
| **Grass plugins** | Plugins written as `groundcover=` rather than `content=`. |
| **Merge Lands writes to** | The folder `Merged Lands.esp` goes in. Empty: the one holding most of the load order. |
| **Leave out of scans** | Plugins the conflict and cell scans skip: patterns, separated by commas. |

Setup is always one click away in the top bar. It also opens by itself after a start that
never finished loading, so a broken install can't lock you out; **Restart** starts the
program again when a load won't finish.

---

## The pages

The top bar switches between the pages. Each page opens on the setup it last used, and scan
results are cached so the next scan of the same plugins is a lookup.

### Cell Preview

Where the program opens: any cell of the load order in 3D, drawn the way the game draws it.
That means its terrain and textures, objects, NPCs and creatures in their gear, water, the
sky and weather, grass, particles, lights and shadows. The Preview changes only what you
see, never a plugin.

- **Moving around.** Orbit with the right mouse button, or press **Tab** for WASD flight.
  **Cell map** (or the key under Escape) opens the world map to pick another cell, with
  the mods that touch each one. The arrows at the view's edges step one cell over.
  **Shift+click** a door to go through it.
- **What is that?** Click an object for its readout (ORI): what it is, which plugin placed
  it, its owner, lock, contents and inventory, leveled lists, services, travel, spells and
  dialogue. Its buttons open the record in Conflicts or the Editor.
- **The panel's groups:**
  - **Cell**: the cell, its textures and reloading.
  - **Light & sky**: the hour, weather and sun.
  - **Scene**: draw distance, distant land and objects, grass.
  - **Water**, and **Effects**: MGE XE's water, ambient occlusion, bloom, depth of field,
    shadows and rain ripples.
  - **Review a mod**: the loaded cells with one mod's part marked, or with that mod left
    out so you see what it changed, deleted or moved. Also checks for floating or buried
    objects, duplicates and missing files.
  - **Landscape**: the ground as the load order has it, or as Merged Lands would build it
    under each strategy, with the vertices two plugins both moved marked.
  - **Overlays & measure**: path grids, light radii, collision, door links, NPC reach, a
    measuring tape, cell borders.
  - **Links & export**: the spot as an OpenMW console line, a report bundle with a
    screenshot, where a mesh, texture or record is used, and a plugin's cells on the map.
- **Scripts** runs the scene's MWScript and OpenMW Lua against a game state you control:
  the hour, date, globals, journal and what the player carries, set by hand or **From a
  save…**.
  - Lamps go on and off by the hour, things move and turn, sounds and music play, and AI
    walks.
  - ForceGreeting opens the dialogue rehearsal, and a Lua mod's windows are drawn over the
    view.
  - **OpenMW folder…** points it at your install's LuaJIT and built-in scripts (`I.AI`,
    `I.Combat`, `I.UI`, music, activation).
  - Ctrl+click an object to activate it.
- **Body** walks the cell in first person at your character's pace. Speed, Athletics and
  Acrobatics come from a preset or **From a save…**, with swimming, levitation, collision
  and footsteps.
- **Sound**: objects' loops, the region's ambience, weather and music play as you move, and
  every sound field has a play button.
- **The mesh viewer** opens from an object's readout or the Links tools. It shows one mesh,
  or each mod's copy of it side by side or overlaid, with its block tree, editable fields,
  draw order and triangle checks; **Save** writes the edited mesh where you choose.
- **Regions** on the cell map colors the world by region. Paint regions onto cells, and
  drag a box over the sea to make new exteriors (into the Editor's patch).

### Load order

The sort. It runs as the page opens, on what Setup names.

**The idea.** In a curated MOMW setup, the `content=` order already in `openmw.cfg` is
**frozen**. This page works out only where *your* additions belong, relative to that list
and to each other. This is the opposite of a whole-list sorter like PLOX: MOMW warns
against re-sorting their lists, and this page is built so you don't have to. Without a
curated list, the plugins you choose are placed among the rest the same way.

**What moves (the subset).** Your plugins come from any of these, combined:

- the customizations.toml;
- a subset file;
- the mods folder, scanned on each Sort. Every folder that directly holds a plugin or a
  standard asset folder (`meshes`, `textures`, `scripts`, `sound` ...) becomes a `data=`
  path, and its plugins become `content=` entries;
- **Pull unmanaged entries from the cfg**: `content=` and `data=` lines no list or file
  accounts for;
- folders and plugins dragged onto the lists from your file manager.

With **plugin-order.yml** and the **list name** set, plugins on the curated list are kept
out of the sort, and only your true additions move.

**The orders.** `content=` and `data=` as two lists. Drag a row (or select it and move it)
to adjust, and untick one to leave it out. Your own mods are marked, and plugins with a
master problem are marked too. **Sort data paths** also places the `data=` folders your mods
add.

**The warnings**, beside the orders, filtered by kind:

| Kind | What it says |
|---|---|
| Rule warnings | mlox's notes, requirements and conflicts for the plugins you have. |
| Rules not applied | Ordering rules that contradict each other or the frozen list. |
| Masters | A master missing (installed but not enabled, or not found at all) or loading after its dependent. |
| Master sizes | The installed master isn't the version the plugin was built against (0 usually means a failed tes3cmd sync). |
| plugin-order.yml | A "custom" plugin already on the list, an unmanaged plugin, one the list says needs cleaning. |
| Drift | Your base order no longer matches the list's. |
| Stale merges | A generated file (`delta-merged.omwaddon`, `S3LightFixes.esp` ...) older than the plugins it merged: run the Configurator again. |
| Data paths, Groundcover, Unmanaged | Folders not placed, grass plugins, and cfg entries nothing manages. |

Each warning names its plugins as links that find them in the orders. The run's full text
is behind a toggle.

**Export** writes what the lists show:

- Without a target set, it writes the corrected order back into your customizations.toml.
  **Write customizations to** names a different file.
- A timestamped backup is taken before anything is overwritten.
- Mods you unticked are written as removals where the curated list would bring them back.
- `openmw.cfg` is changed only with **Write openmw.cfg** ticked; **Dry run** writes
  nothing.

The durable fix is the TOML: run MOMW Configurator on it again, and your additions sort
into place on every rebuild.

**Downloads.** **Update rules** fetches the managed rule files, **Update plugin-order.yml**
fetches MOMW's latest, and **Fetch data paths** fetches the list's data-path order. Each
download must parse before it replaces anything, and the old file is kept as a `.bak`.

**Rule maker…** writes an mlox rule of any kind the guidelines describe. Plugins come from
the rows you picked or are typed in (wildcards, `[DESC /regex/ ...]`, `[SIZE ...]`,
`[VER ...]`, `[ANY]`/`[ALL]`/`[NOT]` groups). The rule is shown as it will read and is
checked as you write; **Append rule** adds it to your rule file and refuses one with an
error, since mlox silently drops a rule it can't read. [MLOX_RULES.md](MLOX_RULES.md)
explains the syntax.

### Conflicts

Which plugins change the same things, and who wins.

- **Records.** The records two or more plugins define, or only those your mods touch.
  Opening one shows its fields across every plugin, each cell colored by what that plugin
  does: master, identical, override that wins, conflict that wins or loses. Beside it are
  the patch steps: **Add record to patch**, **Take field** from a plugin, or **Define
  value…** for a field. A `LAND` record shows its terrain in each plugin, and **Terrain and
  strategies…** shows what each Merged Lands strategy would make of it.
- **Files.** The loose files two or more data folders provide, the ones that differ first.
  A mesh is flagged when the winner loses something the loser had, and **Compare with what
  it replaced** shows a texture beside the one it replaced, with the difference.
- **Plugins.** The same scan by plugin, then record type, then record, with each plugin
  judged by its worst record and the plugins it shares records with. **Merge fields…**
  queues every field a plugin would change across all its shared records in one go.
- **Map.** The record conflicts on the world grid, colored by how many, or one plugin's
  own.
- **Report** and **CSV…** save the scan.

Everything queued goes to the same patch the Editor builds.

### Editor

The Construction Set's working windows around the Preview's render window: a mode of its
own (the top bar's **Editor**). Every change goes into **one patch plugin**, and your mods
are never touched.

- **The Object Window.** Every record of the load order by type, with a filter (Ctrl+F),
  search by any field, sortable columns (id, name, model, the plugins that define it, how
  many are placed) and new records. **Patch only** narrows it to what the patch changes.
- **The Cell View.** Every cell and the references in the one selected. Double-click to
  open a cell or go to a reference.
- **Records and references.**
  - **The record dialog** shows each field (as a Construction Set form per record type),
    its winning value, and an input that queues a change.
  - **The reference dialog** covers one placed object: position, rotation, scale, owner,
    lock, key, trap, door.
  - F2 and F3 open them for whatever is selected.
- **In the render window:**
  - Drag to move, Alt+drag onto a surface, Shift+drag to turn, F to drop to the ground.
  - **Physics** lets the selection fall, tumble and come to rest on what is around it,
    hollows kept, then **Keep** or **Drop**.
  - Grid and angle snaps, box select, copy and paste, layers, selection sets.
  - The **Q** menu for what to do with the selection.
  - Drag a record from the Object Window to place it, or keep a group as a **prefab**.
  - Undo and redo (Ctrl+Z, Ctrl+Y) cover everything.
- **Use Report and Search & Replace.** Every record that names this one, and the cells it
  is placed in. **Replace with** repoints them all, including scripts and dialogue
  results, which are recompiled.
- **Dialogue.** Topics and responses in the engine's order, drag to reorder, and a
  condition editor. Four other views:
  - **Flow**: a topic as the engine reads it, choices branching.
  - **Map**: the topics around one, zoomable.
  - **Flags**: every variable, quest index and item the dialogue reads or writes.
  - **In game**: a rehearsal with any NPC, in a cell you pick and a game state set by hand
    or from a save, with conditions you didn't set listed as assumed.
- **Scripts and Lua.**
  - **Script Edit** checks MWScript as you type and compiles it the way the Construction
    Set does.
  - **The Lua panel** checks the load order's OpenMW Lua (Teal included), with charts of
    each script.
- **Path grids**: select, drag, add, link with Ctrl+drag, delete.
- **Leveled lists and new cells.** A leveled list can be rolled at a level; new interiors
  and exteriors are made from the Cell View or the region map.
- **Keys.** Every shortcut is listed under **Keys**, and **Change the keys…** remaps them,
  along with a game controller's buttons.
- **Finishing.**
  - **Verify** checks what the patch carries.
  - **Test in OpenMW** writes it to a scratch plugin and starts the game in the cell you're
    looking at.
  - **Build patch** writes the plugin.
  - **Messages** keeps every note and error.

### Land

The top bar's **Land** button: terrain editing with a brush, written into the Editor's
patch a moment after each stroke. With **Preview first** on, strokes wait for **Keep**
(or **Drop**) instead. Ctrl+Z undoes them like any other edit.

- **Height**: raise, lower, smooth, sharpen, flatten (to where the stroke began, a fixed
  height, the sea or the brush's average), ramp, noise, terrace and erode.
- **Texture**: paint a land texture onto the ground's texture cells.
- **Color**: blend the vertex colors the game tints the ground by.
- The brush has shapes, falloffs and dynamics: click, drag or hold; decay and fade;
  velocity, jitter, a stabilizer and a cap. Presets save the settings.
- Overlays show hillshade, a height tint and contour lines.
- Painting where a square has no land makes flat land at the sea floor first, as the
  Construction Set would.

### Tools

Each tool is a section on the left with its work on the right.

| Tool | What it does |
|---|---|
| **Lint** | Problems the game doesn't report: the "evil GMSTs" (only when name and value both match), the interior fog bug, new interiors with no path grid anywhere, blank headers, Tribunal/Bloodmoon calls without the master, `.omwscripts` twins left out. |
| **Backups** | Every copy the program and tes3cmd took before changing a file: restore or delete it. |
| **Remove / rename master** | A master OpenMW can't find: removed (references renumbered, a dry run first) or pointed at the file that replaced it. The original is kept once as `.premaster.bak`. |
| **Save Check** | Whether a save loads with this order (the content files it needs that the order lacks), the space each mod takes in it and how to shrink it, and two saves compared. |
| **Journal** | Every quest's stages in index order, each with the scripts and responses that set it, and their conditions. |
| **tes3cmd** | Runs the external tes3cmd on each plugin (clean, header, custom), staged with its masters; master sizes synced in-app. |
| **Merge Lands** | One `Merged Lands.esp` holding the terrain edits of every mod that reshapes a cell, loaded last. Each plugin is measured against its masters or against the load order before it, and plugins can be left out by name or folder. See [MERGED_LANDS.md](MERGED_LANDS.md). |
| **Merge settings** | A plugin's `.mergedlands.toml`: which of its land layers Merge Lands takes, and how collisions resolve. |
| **Merge to master** | A plugin folded into its master, or a whole load order into one master. |
| **Mesh optimiser** | Draw-order copies of the load order's meshes, written only where it helps, each checked to mean the same mesh as the original. |

### Help, Report and Settings

- **Help** shows this document, the [Quick Start](QUICKSTART.md) and
  [Writing mlox rules](MLOX_RULES.md), with their contents beside them.
- **Report** lists what was read: the plugins with their masters, the archives, the meshes
  and textures found, and anything that couldn't be read.
- **Settings** (the gear) holds the theme, the language, how the mouse moves the camera,
  and the side panel.

---

## The command line

`wraithguard` does the sort, the export and the read-only scans with no window, so it runs
in scripts, CI and on a headless machine. It takes the same inputs as the Load order page:

```
wraithguard --cfg openmw.cfg --rules mlox_base.txt mlox_user.txt \
    --scan-dir "E:\OpenMW\Mods\custom" --subset-file mod_scan_results.txt \
    --plugin-order-yml plugin-order.yml --list-name total-overhaul \
    --sort-data-paths --emit-toml momw-customizations.toml
```

By default it prints the plan and writes nothing.

| Flag | What it does |
|---|---|
| `--cfg PATH`, `--rules ...` | The cfg and the rule files (required). |
| `--customizations PATH`, `--subset ...`, `--subset-file PATH` | Where the subset comes from. |
| `--scan-dir DIR` | Scan a mods folder, write the result to `--subset-file`, then sort it. |
| `--subset-from-cfg` | Sort the cfg's own unmanaged `content=` and `data=` entries. |
| `--groundcover ...` | Plugins written as `groundcover=`. |
| `--plugin-order-yml PATH`, `--list-name NAME`, `--data-paths-cache PATH` | MOMW's list. |
| `--sort-data-paths` | Also place the `data=` folders. |
| `--emit-toml PATH` | Write the corrected customizations TOML (the durable fix). |
| `--write-cfg` | Patch `openmw.cfg` in place. |
| `--dry-run`, `--no-backup` | Write nothing; or skip the `.bak` copy. |
| `--check-conflicts`, `--conflicts-out CSV`, `--conflicts-subset-only` | Record conflicts after the sort. |
| `--resource-conflicts`, `--resources-out CSV` | Loose-file conflicts. |
| `--lint` | The Lint checks. |
| `--cell-map PATH` | A self-contained HTML map of which mods touch which cells. |
| `--exclude ...` | Plugins the scans leave out. |
| `--no-predicate-warnings`, `-v`/`-vv`, `--trace` | Quieter, louder, or a trace log. |

`wraithguard --help` lists them all.

---

## Where files live

- **Settings** (`wraithguard_toolkit_settings.json`), the scan cache and the Editor's
  patch journal are kept **beside the program**. Keep it in a folder you can write to.
- **Profiles and mesh setups** (`.toml`) are beside the program too, unless you choose
  another folder for them; existing files are moved there when you do.
- **Backups** sit beside the file they copy, timestamped.
- **Patches** go where you choose when you Build. **Merged Lands.esp** goes in the folder
  Setup names, or the one holding most of the load order.
- **Exports** go to the customizations TOML (and the cfg, if you ask).

---

## Safety

- **Nothing is written until you say so.** Export, Build patch, Keep and the Tools' write
  buttons are the only things that write. *Dry run* and the command line's default write
  nothing.
- **Backups first.** `openmw.cfg`, a customizations TOML, a plugin whose master is removed,
  and a file tes3cmd cleans are all copied before they change.
- **A curated list's order is never changed.** Only your additions move.
- **Your mods are never modified.** The Editor's patch, Merged Lands and the Land view's
  edits are new plugins, loaded after the rest.
- The MOMW team doesn't support customizations. This program helps you see and place
  them; it can't promise they're conflict-free.

---

## Building and developing

You need Rust (stable, via [rustup](https://rustup.rs)). On Linux you also need the
WebKitGTK 4.1 and GTK 3 development packages (`libwebkit2gtk-4.1-dev libgtk-3-dev
librsvg2-dev`).

```
cp wraithguard_toolkit_icon.ico viewer-shell/icons/
cargo build --release --manifest-path viewer-shell/Cargo.toml       # the program
cargo build --release --manifest-path viewer-shell/cli/Cargo.toml   # the command line
```

The program is `viewer-shell/target/release/wraithguard-toolkit`, and the command line is
`viewer-shell/cli/target/release/wraithguard`.

**Before pushing, see [PREFLIGHT.md](PREFLIGHT.md).** One script builds the program and
runs every check CI runs:

```powershell
.\tools\build_and_check_rust.ps1         # the program and every Rust check
.\tools\build_and_check_rust.ps1 -Pages  # ...plus the page tests (Node 22)
```

**By hand:**

- Each crate: `cargo test --manifest-path viewer-shell/<crate>/Cargo.toml` (`luacore` with
  `--features teal`). Clippy runs with `-D warnings`.
- The page tests live in `viewer-shell/ui/tests`:
  - `node editor/run.js` for the page's own tests;
  - `node boot.js` for the whole page against `wg-view-serve`, the headless engine built
    by `viewer-shell/check-commands`.

CI (`.github/workflows/ci.yml`) runs exactly that list. `build-app.yml` builds the release
packages on a `v*` tag or a `build/**` branch, the Linux AppImage with Tauri's bundler
(`cargo tauri build --bundles appimage` in `viewer-shell`).

The fixtures in `tests/fixtures/` pin the crates to the answers the retired Python gave
(the command line, the sort, the Configurator simulation, Merged Lands), so a change that
alters an answer fails loudly. `tools/README.md` says where each came from.
`CODE_REVIEW.md` records the defects found and the reasoning behind decisions that look
odd. `viewer-shell/MANIFEST.md` lists every file of the page and every engine command.

---

## Project layout

```
WraithguardToolkit/
├── *.md                     README, QUICKSTART, CHANGELOG, CREDITS, MLOX_RULES,
│                            MERGED_LANDS, CODE_REVIEW, PREFLIGHT, TODO ...
├── License/                 The MIT license, and one file per upstream project whose
│                            license travels with code ported here.
├── viewer-shell/            The program and its crates:
│   ├── src/                 wraithguard-toolkit (Tauri): main.rs and the commands.
│   ├── ui/                  Its page (GPL-2.0, kept apart): ui/src assembled in
│   │                        ui/ORDER's order at build time; ui/tests the page tests.
│   ├── cli/                 wraithguard, the command line.
│   ├── viewcore/            The engine: the install, the merged world, meshes, textures,
│   │                        sound, OpenMW Lua.
│   ├── ordercore/           openmw.cfg, the rules, the sort and export.
│   ├── editcore/            Plugins as records, the Editor, the patch.
│   ├── conflictcore/        Record and loose-file conflicts, the mesh editor.
│   ├── toolscore/           The Tools, settings, Setup, the Help renderer.
│   ├── landcore/            Merged Lands and its settings.
│   ├── luacore/             OpenMW Lua and Teal checked.
│   ├── physcore/            The Editor's Physics drop (Rapier).
│   ├── gardenfell/ gardenfell-g7/ meshopt-port/
│   │                        Vendored Gardenfell, its tes3 bridge, meshoptimizer ported.
│   └── check/ check-commands/
│                            Compile checks without Tauri's native tree, and
│                            wg-view-serve, the headless engine the page tests use.
├── tools/                   Developer scripts and the fixture generators.
├── tests/fixtures/          The crates' shared test cases.
├── testdata/                Copies of a real setup, used by the tests.
├── locale/                  Translations (.po; English only for now).
└── art/                     Icons, banner, the Nexus description.
```

---

## Credits and licenses

The program's code is MIT. Its page (`viewer-shell/ui`) is GPL-2.0, because it carries
ports of MGE XE's shaders, and is kept separate from the MIT code.

It stands on a lot of other people's work:

- greatness7's `tes3` crates and Robin Hjelte's Gardenfell;
- mlox and its rule databases, plox, tes3conv, tes3cmd and merge_to_master;
- MGE XE, OpenMW and Modding-OpenMW.com;
- Rapier, and many Rust crates.

[CREDITS.md](CREDITS.md) names each of them, what was taken, and under what license.
[CHANGELOG.md](CHANGELOG.md) has what changed in each release.
