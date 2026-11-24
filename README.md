# Wraithguard Toolkit

One program for keeping a modded Morrowind in order: sort it, see it, find what fights, fix
it. It reads an OpenMW install (`openmw.cfg`), a Mod Organizer 2 install, or the game's own
(`Data Files` and `Morrowind.ini`), and runs on Windows, macOS and Linux, with a command line
beside it for scripts and headless machines.

- **Load order.** Sorts by the mlox rules. On a curated
  [Modding-OpenMW.com](https://modding-openmw.com/) (MOMW) list it places only the mods you
  added and never reorders the list, saving the result as a `momw-customizations.toml` that
  survives every MOMW Configurator rebuild. Without a list, it sorts the whole order as mlox
  does.
- **Cell Preview.** Any cell of the load order in 3D, drawn as the game draws it, with its
  MWScript and OpenMW Lua running, and a way to see what one mod changed.
- **Conflicts.** Which plugins change the same records and the same files, and who wins.
- **Editor.** The Construction Set's windows around the Preview: records, references,
  dialogue, scripts and path grids, written into one new patch plugin, or into a plugin you
  choose to edit.
- **Land.** Sculpt, paint and tint the terrain with a brush, into the same patch.
- **Tools.** Lint, backups, broken masters, save checks, the quest journal, tes3cmd (built
  in, no Perl), Merged Lands, merge to master, a mesh optimizer, a Lua mod checker and
  settings editor, and an HTN AI builder for FluadHTN.

Nothing is written until you ask, and your `openmw.cfg`, customizations and plugins are
backed up before they change.

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
| Linux and the Steam Deck | `Wraithguard_Toolkit-x86_64.AppImage` | Everything it needs is inside. Make it executable (`chmod +x`, or Properties > Permissions), then run it. On the Deck, use Desktop Mode; add it to Steam as a non-Steam game to start it from Game Mode. Its settings are kept beside the `.AppImage` file. |
| Linux, plain | `wraithguard-linux-x86_64.tar.gz` | Smaller, with the command line. Needs the system's WebKitGTK 4.1 (`libwebkit2gtk-4.1-0`: Ubuntu 22.04+, Debian 12+, Fedora, Arch). |

**Verifying a download.** Each package has its SHA-256 sum beside it on the release (`<package>.sha256`). Compare it with
`certutil -hashfile <file> SHA256` (Windows) or `sha256sum <file>`.

> **Coming from 4.x?** 5.0 is a single Rust program; the Python/Tk app is retired. Your
> settings file carries over. A patch queue left by the 4.x Editor does not, so build it
> into a patch with 4.x before you upgrade. If you installed 4.x with pip, remove it
> (`pip uninstall wraithguard-toolkit`): its `wraithguard` launcher stays on PATH otherwise,
> and typing `wraithguard` runs it instead of 5.x's, which fails with
> "No module named 'wraithguard_toolkit'".

---

## First start: Setup

The first start opens **Setup**: every path the program runs on, in one place, in three
groups (your install, sorting, Tools). Each field is checked as you type, the same way the
program will read it. Missing data folders, a TOML that doesn't parse and rule files that
aren't there are named, with what to do about them, before anything is loaded. **Save and
load** saves the settings and loads your install.

| Field | What it is |
|---|---|
| **The install** | The load order every page reads. **OpenMW**: the `openmw.cfg` it starts with, its `data=` folders and `content=` files (**Find** looks where OpenMW keeps it). **MO2…**: a Mod Organizer 2 folder, with the profile it has selected (or add `\|Profile`). **Morrowind…**: the game's `Data Files`, read with the `Morrowind.ini` beside it. |
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
| **tes3cmd** | An external tes3cmd (the MOMW Tools Pack's `tes3cmd.exe`, or the Perl script), for when the Tools page's *Use external* is ticked. Optional: tes3cmd is built in. |
| **Merge Lands writes to** | The folder `Merged Lands.esp` goes in. Empty: the one holding most of the load order. |
| **Leave out of scans** | Plugins the conflict and cell scans skip: patterns, separated by commas. |

On an MO2 or `Morrowind.ini` install the fields that are OpenMW's or MOMW's alone (the
customizations TOML, plugin-order.yml, the list, grass plugins, the OpenMW folder) are
hidden, and the Load order page sorts by the mlox rules alone. An MO2 install that feeds
OpenMW through MO2's *Export to OpenMW* counts as OpenMW's: its OpenMW folder still applies,
and Export writes both MO2's order and that `openmw.cfg`.

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

- **Moving around.** Three cameras, and **Tab** (or the toolbar's button) goes round them:
  orbit with the right mouse button, WASD flight, and first person at a person's eye height,
  walking on the land and on objects with gravity, steps and jumps.
  **Cell map** (or the key under Escape) opens the world map to pick another cell, with
  the mods that touch each one. The arrows at the view's edges step one cell over.
  **Shift+click** a door to go through it.
- **What is that?** Click an object for its readout (ORI): what it is, which plugin placed
  it, its owner, lock, contents and inventory, leveled lists, services, travel, spells and
  dialogue. Its buttons open the record in Conflicts or the Editor.
- **The panel's groups:**
  - **Cell**: the cell, its textures and reloading.
  - **Light & sky**: the hour, weather and sun.
  - **Scene**: draw distance, distant land and objects, grass, **Occlusion culling** (on
    by default: what stands wholly behind a building or a hill is not drawn, in the view and
    in the water's reflection, judged from the last frames' depth) and **Light GPU** (for a
    weak or built-in GPU: a smaller reflection without small objects, less multisampling,
    distant objects half as far).
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
    **Views** keeps camera spots per cell to jump back to, and **Screenshot…** saves the 3D
    view as a PNG, optionally without highlights and arrows.
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
- **NPCs and creatures** follow their AI packages: they wander, travel, follow and escort
  over the path grids or OpenMW's navmesh, turning as they walk and climbing stairs.
  **Actor info** (🛈) shows over each one its package, the animation it plays and how often it
  is posed.
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

**Plain mlox.** With no curated list to keep, **Plain mlox** places the whole `content=` list
by the mlox rules alone, as mlox itself does. An MO2 or `Morrowind.ini` install is always
sorted this way; Export then writes MO2's `loadorder.txt`, `plugins.txt` and the profile's
`Morrowind.ini` (and the `openmw.cfg` MO2 exports, when it feeds OpenMW), or the game's
`Morrowind.ini` and its plugins' dates. With a customizations TOML named, Export on an
`openmw.cfg` also writes the customizations that make MOMW's Configurator build the same
order.

**The orders.** `content=` and `data=` as two lists. Drag a row (or select it and move it)
to adjust, and untick one to leave it out. A box on each list's header finds rows by name.
Your own mods are marked, and plugins with a master problem are marked too. **Sort data
paths** also places the `data=` folders your mods add.

**Why here?** (or double-click a plugin) says what put a plugin where it is: whether it is
on the curated list or one of yours (and where it came from), the plugin it was placed
right after or before, every rule block that names it (the file and line, what it puts
before and after it, and which of those the order doesn't follow), its masters and whether
each loads before it, and the rules refused for it. Every plugin named is a link to its own
answer.

**Snapshots** keeps the order on the page by name. One is also kept on its own before each
Sort and after each Export that writes (the last 10). **Compare** lists what was added,
taken out, ticked on or off, and moved since; **Restore** puts an older order back on the
page for Export to write. Snapshots are kept per `openmw.cfg`, on this machine.

**The warnings**, beside the orders, filtered by kind:

| Kind | What it says |
|---|---|
| Rule warnings | mlox's notes, requirements and conflicts for the plugins you have. |
| Rules not applied | Ordering rules that contradict each other or the frozen list. |
| Masters | A master missing (installed but not enabled, or not found at all) or loading after its dependent. |
| Master sizes | The installed master isn't the version the plugin was built against (0 usually means a failed tes3cmd sync). |
| Master flags | A plugin whose extension and header disagree: an `.esp` flagged as a master, or an `.esm` that isn't. |
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
- **Reviewed marks.** Click the ○ beside a record or file to mark it reviewed (✓). When
  another plugin or folder comes to touch it, the mark turns to ↻, changed since you
  reviewed it. The lists show everything, only what you haven't reviewed, the reviewed, or
  the changed; **Mark shown reviewed** marks the whole list as filtered.

Everything queued goes to the same patch the Editor builds.

### Editor

The Construction Set's working windows around the Preview's render window: a mode of its
own (the top bar's **Editor**). Every change goes into one pool of edits, kept between runs.
**Build patch** writes it as **a new patch plugin**, loaded after the rest, so your mods are
never touched. Or choose a plugin of the load order under **Edit a plugin**, as the
Construction Set has an active file: the Editor then reads it last, so you edit records as
that plugin has them, and Build patch writes the pool into it in place. Every other record of
that plugin is kept byte for byte, and the plugin as it was is kept once as
`name.preedit.bak`. Plugins are read and written in the install's code page (OpenMW's
`encoding=`, or the game's edition), so Russian and Polish names come out right.

- **The Object Window.** Every record of the load order by type, with a filter (Ctrl+F),
  search by any field, sortable columns (id, name, model, the plugins that define it, how
  many are placed) and new records. **Patch only** narrows it to what the patch changes.
  **Set a field on all of them** takes a value, or an amount: `+10`, `-10`, `*1.25`, `+20%`
  change each record from its own value (whole numbers stay whole), one undo step for all.
- **Ctrl+P, the command palette.** Type to find a page, a tool, a key action, a button on
  screen, a saved view, a cell or any record by id, and Enter to run or open it.
- **Back and forward** (Alt+Left, Alt+Right, or ◀ ▶ in the record dialog's title) through
  the records and references opened.
- **Pending › Diff** shows each changed field of a record: what the load order has, what the
  patch writes, and ↶ to take back that one field. **Diff all** opens them all.
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
  - **Physics** sets things moving and lets them come to rest on what is around them, hollows
    kept, then **Keep** or **Drop**:
    - **Drop:** the selection falls.
    - **Blast:** click a point, and what's in reach is thrown outward.
    - **Throw:** the selection flies the way the camera faces.
    - **Scatter:** click a point, and copies of the selection fall around it and settle; Keep
      places them.

    With **Struck objects move too**, the objects around scatter when hit. A blacklist you can
    edit keeps buildings, walls, rocks, doors, actors and anything big from moving: mesh
    patterns, record types and a size.
  - Grid and angle snaps, box select, copy and paste, layers, selection sets.
  - The **Q** menu for what to do with the selection.
  - Drag a record from the Object Window to place it, or keep a group as a **prefab**.
  - Undo and redo (Ctrl+Z, Ctrl+Y) cover everything.
- **Use Report and Search & Replace.** Every record that names this one, and the cells it
  is placed in. **Replace with** repoints them all, including scripts and dialogue
  results, which are recompiled.
- **Dialogue.** Topics and responses in the engine's order, drag to reorder, and a
  condition editor. Five other views:
  - **Flow**: a topic as the engine reads it, choices branching.
  - **Graph**: the topic as a tree of boxes, its responses in the order the engine tries
    them, their choices and what each does (topics added, journal entries, variables,
    items). Click to open a response; double-click a topic or quest to draw it.
  - **Map**: the topics around one, zoomable.
  - **Flags**: every variable, quest index and item the dialogue reads or writes.
  - A response's text is spell-checked as you type, and a `%name` that is neither a text
    define (`%PCName`, `%PCRank`, `%Faction`…) nor a global variable, or an `@topic`
    without its `#`, is named under it.
  - **In game**: a rehearsal with any NPC, in a cell you pick and a game state set by hand
    or from a save, with conditions you didn't set listed as assumed.
- **Scripts and Lua.**
  - **Script Edit** checks MWScript as you type and compiles it the way the Construction
    Set does. Ctrl+Space completes a record id or one of the script's variables, and
    **Where is it set?** lists every script line that sets the variable under the caret.
  - **The Lua panel** checks the load order's OpenMW Lua (Teal included), with charts of
    each script.
- **Path grids**: select, drag, add, link with Ctrl+drag, delete.
- **Leveled lists and new cells.** A leveled list can be rolled at a level; new interiors
  and exteriors are made from the Cell View or the region map.
- **Keys.** Every shortcut is listed under **Keys**, and **Change the keys…** remaps them,
  along with a game controller's buttons.
- **Finishing.**
  - **Verify** checks what the patch carries: matching ids, sounds, and land borders the
    patch opens with a neighboring cell.
  - **Test in OpenMW** writes it to a scratch plugin and starts the game in the cell you're
    looking at.
  - **Build patch** writes the plugin: a new one (*Append* to it over several sessions, or
    *Replace* it), or the plugin you're editing, in place.
  - **Messages** keeps every note and error.

### Land

The top bar's **Land** button: terrain editing with a brush, written into the Editor's
patch a moment after each stroke. With **Preview first** on, strokes wait for **Keep**
(or **Drop**) instead. Ctrl+Z undoes them like any other edit.

- **Height**: raise, lower, smooth, sharpen, flatten (to where the stroke began, a fixed
  height, the sea or the brush's average), ramp, noise, terrace and erode.
- **Texture**: paint a land texture onto the ground's texture cells. Hovering a texture's
  name shows its picture.
- **Color**: blend the vertex colors the game tints the ground by.
- **Mask**: paint a mask onto the ground (Alt rubs it out), then hold the height and color
  brushes inside it or out of it. The **Texture mask** does the same by what is painted on
  the ground: "only on these textures" levels a road without moving the grass beside it.
  Masks are the page's own and are never written to the patch.
- The brush has shapes, falloffs and dynamics: click, drag or hold; decay and fade;
  velocity, jitter, a stabilizer and a cap. Presets save the settings.
- Overlays show hillshade, a height tint and contour lines.
- **Mirror** lays each dab again across a north-south line, an east-west line, or four
  ways, through the loaded cell's middle or a center you click. The ramp isn't mirrored.
- **Pick** takes a height (Flatten), a texture, or now a vertex color from the ground.
- **Road or path**: click points on the ground, then **Build road**. A strip of the width
  asked is leveled along a curve through them, following the ground but no steeper than
  the slope you set, its shoulders blended back into the ground, and a land texture
  painted along it.
- **Paint by rules**: textures put on the loaded land by height, slope and distance from
  the water, the first rule that matches each texture cell winning (sand below 64 near the
  water, rock steeper than 35°, grass the rest). All the land, or inside or outside the
  painted mask.
- **Heightmap PNG**: **Export…** saves the loaded cells' heights as a 16-bit grey PNG, its
  height range kept in the file; **Import…** puts any PNG (grey or color, 8 or 16 bits) onto
  the loaded cells between a lowest and a highest height, at a strength.
- **Strokes** lists your strokes, roads, rules and imports as you make them; **Back to
  before this** takes that one and every one after it back, as a new change to the patch.
- Painting where a square has no land makes flat land at the sea floor first, as the
  Construction Set would.

### Tools

Each tool is a section on the left with its work on the right.

| Tool | What it does |
|---|---|
| **Lint** | Problems the game doesn't report: the "evil GMSTs" (only when name and value both match), the interior fog bug, new interiors with no path grid anywhere, blank headers, Tribunal/Bloodmoon calls without the master, `.omwscripts` twins left out. |
| **Backups** | Every copy the program and tes3cmd took before changing a file (a master removed, a plugin edited in place or run through tes3cmd, a merge's output replaced): restore or delete it. |
| **Remove / rename master** | A master OpenMW can't find: removed (references renumbered, a dry run first) or pointed at the file that replaced it. The original is kept once as `.premaster.bak`. |
| **Save Check** | Whether a save loads with this order (the content files it needs that the order lacks), the space each mod takes in it and how to shrink it, and two saves compared. |
| **Journal** | Every quest's stages in index order, each with the scripts and responses that set it, and their conditions. **List / Graph**: the same as a tree, each stage with what sets it, or that nothing does. |
| **tes3cmd** | tes3cmd built in ([tes3cmd-rs](https://github.com/staticnation/tes3cmd-rs), no tes3cmd.exe or Perl needed) on each plugin: clean, header or any command, on a copy with its masters read from your data folders and never changed, a backup kept; master sizes synced in-app. `fixit`, `multipatch` and `resetdates` run once over the plugins added, `common` and `diff` on two; a multipatch.esp made goes beside them. Every record a run removed, added or changed is listed under it, filtered by plugin, kind or text, and **Undo selected** puts the ticked records back as they were. Tick *Use external* to run your own tes3cmd instead; commands that run Perl code (`modify --run`) use it either way. |
| **Merge Lands** | One `Merged Lands.esp` holding the terrain edits of every mod that reshapes a cell, loaded last. Each plugin is measured against the load order before it (the default, as OpenMWMergedLands does) or against its masters (contested vertices blended, as the original Merged Lands does). Seams are repaired along the cells plugins changed, and optionally the seams the terrain already had. Plugins can be left out by name or folder, and cells for every plugin or for one (Tamriel Rebuilt's Hunza Camp by default). **Import from OpenMWMergedLands** reads its `merged_lands.toml`. *Dry run*, *Verbose* (with each step's time and the peak memory) and *Cancel* are beside **Build**. See [MERGED_LANDS.md](MERGED_LANDS.md). |
| **Merge objects** | One `Merged Objects.esp` holding every mod's changes to the records two or more plugins change, not only the last one's: [TES3Merge](https://github.com/NullCascade/TES3Merge)'s merge, ported. Each field another plugin changed is kept, leveled lists are combined, and effects merge slot by slot. TES3Merge's fog-bug, summons and cell-name fixes apply. The record types are ticked (TES3Merge's list by default), and plugins can be left out and ids filtered. *Inclusive inventories* gives an NPC every item any plugin gives it, *Leveled lists only* writes `multipatch.esp`, and *Dry run* writes nothing. It reads the load order as the setup has it, for OpenMW, MO2 or the game's own. Load the plugin last. |
| **Merge settings** | A plugin's `.mergedlands.toml`: which of its land layers Merge Lands takes, and how collisions resolve. |
| **Merge to master** | A plugin folded into its master, or a whole load order into one master. |
| **Mesh optimizer** | Draw-order copies of the load order's meshes, written only where it helps, each checked to mean the same mesh as the original. A mesh that Check meshes can mend with a proven repair is optimized from the repaired copy. Meshes in the load order's archives are read too; a copy is written as a loose file. |
| **Lua mods** | Each Lua mod of the load order (OpenMW's and MWSE's) run on its own, and what its scripts said sorted into what you can act on: another mod needed, a plugin not loaded, the mod's own fault, other errors, and what the Preview does not run yet. Above them the load-order lint: interfaces used that no script provides, settings registered twice, a mod in two data folders. At the top, **what the mods need** (an interface, a module, a plugin's global), looked for in your setup: a content file in the data folders that isn't turned on, or a folder of the Load order page's mods folder that isn't in `data=` yet. **Turn on** adds it on the Load order page for Export to write (**Turn on all** does every one found); what isn't installed gets **Search Nexus**. |
| **Lua settings** | The settings OpenMW keeps for your Lua mods (`player_storage.bin`, `global_storage.bin`), by section, changed one value at a time with OpenMW closed. The file is written only when it reads back as exactly what you meant, and the original is kept once as `<file>.wg-backup`. |
| **HTN AI** | An actor's behaviour built from game blocks (wait, wander, walk to, follow the player, attack, say, play an animation, set a value) and checks (the player near, health low, the hour, an item carried...), as a [FluadHTN](https://www.nexusmods.com/morrowind/mods/52111) domain: a list or a graph to edit, undo, templates (guard, merchant, wildlife, companion, sentry, worker, pilgrim, coward) and reusable parts. Actors are picked by record id, faction, class or the cell they are in. **Places** are anchor points set from where the Preview's camera stands, for the **Go to a place** step; the **Schedule** says where an actor is through the day and what it does there (wander, stand, idle, or a reusable part), and **Put in the behaviour** adds it as steps you can edit. It is checked as you build, tried on an actor of the loaded cell in the Preview (on the page's own planner, or on FluadHTN itself; click the ○ beside a step for a breakpoint that pauses the Preview as it starts, then **Step** or **Continue**), and exported as a mod for OpenMW and MWSE. The chips above it say whether your setup has FluadHTN's builds and Rubic0n's LuaJIT; click a FluadHTN chip to choose its folder. FluadHTN is the player's to install and is not included. |
| **Check meshes** | The winning copy of every mesh, loose or in the load order's archives, read for the faults exporters write. **Mendable** means every fault has a repair that is proven to keep the mesh the same: a `Has UV` flag left off where there are UV sets, which OpenMW and older tools misread as the triangles, or UV sets counted that aren't there. **Mendable by a guess** means a node whose lists don't read, which nothing else links to, kept with no children (written only when you tick it). **Unreadable** names why: another NIF version, a block type outside Morrowind's format, or not a NIF. **Write repaired** puts the ticked ones, at their own paths, into a folder you add last in the load order; your mods aren't touched. |

### Help, Report and Settings

- **Help** shows this document, the [Quick Start](QUICKSTART.md) and
  [Writing mlox rules](MLOX_RULES.md), with their contents beside them.
- **Report** lists what was read: the plugins with their masters, the archives, the meshes
  and textures found, and anything that couldn't be read. Its **Performance** part says
  where a frame's time goes: the GPU by pass, the page's own work (drawing, MWScript, the
  actors), what occlusion culling left out, and the Lua round trips, split into their parts
  (making the frame, the call across, the collision, the pipe, reading the frame, the scripts,
  writing the answer).
- **Jobs** (beside Report, while a long job runs or after one has) lists Merge Lands, Merge
  objects, the Mesh optimizer, Check meshes, the Lua check, tes3cmd, Save Check, the Conflicts
  scans and the Load order's sort and export: how long each has taken and how far it has got,
  **Cancel** for Merge Lands, the mesh writes and the Lua check, and **Open folder** for what
  one wrote.
- **Settings** (the gear) holds the theme, the language, how the mouse moves the camera,
  and the side panel. **Read past unknown subrecords** (off by default) lets the scans, the
  Editor and Merge objects read a plugin with a subrecord its record type doesn't know, instead
  of leaving the plugin out. What was stepped over is kept and written back where it was. In the
  rare case that can't be proven on the record, it is marked instead: the Editor won't write it
  into a patch, and Merge objects leaves it out.

---

## The command line

`wraithguard` does the sort, the export, the read-only scans, Merge Lands, Merge objects, Check meshes and tes3cmd with
no window, so it runs in scripts, CI and on a headless machine. It takes the same inputs as
the Load order page:

```
wraithguard --cfg openmw.cfg --rules mlox_base.txt mlox_user.txt \
    --scan-dir "E:\OpenMW\Mods\custom" --subset-file mod_scan_results.txt \
    --plugin-order-yml plugin-order.yml --list-name total-overhaul \
    --sort-data-paths --emit-toml momw-customizations.toml
```

By default it prints the plan and writes nothing.

It also runs the Tools page's Merge objects and Check meshes over the setup's load order:

```
wraithguard --cfg openmw.cfg --merge-objects
wraithguard --mo2 "C:\Modding\MO2" --merge-objects "C:\Modding\MO2\mods\Merged Objects"
wraithguard --cfg openmw.cfg --check-meshes "E:\OpenMW\Repaired meshes"
```

`--merge-objects` writes `Merged Objects.esp` and prints the records it merged, by type. Load
it last. `--check-meshes` lists every mesh that isn't sound, with what was found in it, and
with a folder writes the mendable ones there. `--dry-run` writes nothing for either. Given with
`--rules`, both run after the sort.

It also runs Merge Lands, the Tools page's merge, over the cfg's load order:

```
wraithguard --cfg openmw.cfg --merge-lands
```

That writes `Merged Lands.esp` (and its `.mergedlands.toml` marker) into the data folder
holding most of the load order, or the folder you name (`--merge-lands "E:\OpenMW\Merged"`),
and prints the merge's progress as it goes. A `Merged Lands.esp` already there is replaced. Each plugin's `.mergedlands.toml` settings
apply, as they do on the Tools page. Add `--dry-run` to see the merge and write nothing.
Given with `--rules`, the merge runs after the sort, from the cfg as it is on disk then (so
after `--write-cfg`, if you gave that). Add `Merged Lands.esp` to the cfg's `content=`
yourself, last, as you would after the Tools page's merge.

| Flag | What it does |
|---|---|
| `--cfg PATH`, `--rules ...` | The cfg and the rule files (required; `--rules` not for `--merge-lands`, `--merge-objects` or `--check-meshes` alone). |
| `--mo2 DIR [--profile NAME]`, `--data-files DIR` | An MO2 install or the game's own in place of `--cfg`, sorted by the mlox rules alone. |
| `--plain-mlox` | With `--cfg`: the whole `content=` list by the mlox rules alone, no MOMW list. |
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
| `--merge-lands [FOLDER]` | Merge Lands, writing `Merged Lands.esp` into FOLDER (default: the folder holding most of the load order). |
| `--merge-model loadorder\|masters` | Measure each plugin's terrain against the load order before it (default: the later plugin wins a contested vertex, as OpenMWMergedLands), or against the masters (contested heights, normals, colors and world map are blended, as the original Merged Lands). |
| `--merge-fork-config [FILE]` | Take the plugins, folders and cells OpenMWMergedLands leaves out, and where it writes, from its `merged_lands.toml` (FILE, or found beside the cfg or in OpenMW's config folder). Anything given on the command line still wins. |
| `--merge-keep-world-map` | Write the world maps (WNAM) as the merge has them, as OpenMWMergedLands does (default: entries made again where a repair moved the height, a missing map generated). |
| `--merge-repair-seams [GAP]` | Also repair the seams the terrain already has, between cells no plugin changed, where the gap is at most GAP world units (default 24; a wider one is taken to be meant). Off by default. |
| `--merge-seam-except X,Y ...` | With `--merge-repair-seams`: cells whose borders are never repaired (a cliff or an edge the load order means to have). |
| `--merge-cell-ignore X,Y ...` | Exterior cells left out of the merge for every plugin. |
| `--merge-cell-ignore-by-plugin PLUGIN=X,Y;X,Y ...` | One plugin's terrain left out at those cells (default: Tamriel Rebuilt's Hunza Camp; with no value, none). |
| `--merge-no-clamp` | Stop, writing nothing, if a height step is left that a record cannot store and the slope limiter could not repair (default: clamp it and say so). |
| `--merge-ignore ...` | Plugins Merge Lands leaves out, by name or by folder; replaces the default list (generated plugins such as `delta-merged.omwaddon`). |
| `--merge-objects [FOLDER]` | Merge objects: `Merged Objects.esp` (TES3Merge's merge) written into FOLDER (default: the folder holding most of the load order). |
| `--merge-objects-types TYPE ...` | The record types merged, as tags (`NPC_ LEVI ...`; default TES3Merge's list, `FACT` not in it). |
| `--merge-objects-ignore PLUGIN ...` | Plugins Merge objects leaves out, by file name. |
| `--merge-objects-patches fogbug\|summons\|cellnames\|all\|none ...` | The fixes applied (default all). |
| `--merge-objects-inclusive` | NPC and creature inventories merged inclusively (every item any plugin gives). |
| `--merge-objects-lists-only` | Only the leveled lists, into `multipatch.esp` (TES3Merge's multipatch). |
| `--check-meshes [FOLDER]` | Check meshes: the meshes that are not sound listed with what was found. With FOLDER, the mendable ones are written there repaired, at their own paths. FOLDER is left out of the meshes read, so add it last to your data folders. |
| `--lenient-plugins` | Read past a subrecord a record type doesn't know instead of refusing the plugin, as Settings' *Read past unknown subrecords*. |
| `--check-meshes-guesses` | With `--check-meshes FOLDER`: also write the meshes mendable only by a guess. |
| `--no-predicate-warnings`, `-v`/`-vv`, `--trace` | Quieter, louder, or a trace log. |
| `tes3cmd <command> ...` | tes3cmd itself, built in (below). |

An MO2 install or the game's own takes the place of `--cfg`, and is sorted by the mlox
rules alone (no MOMW list); `--plain-mlox` does the same for an `openmw.cfg`:

```
wraithguard --mo2 "C:\Modding\MO2" --profile Default --rules mlox_base.txt mlox_user.txt
wraithguard --data-files "C:\Games\Morrowind\Data Files" --rules mlox_base.txt --write-cfg
wraithguard --cfg openmw.cfg --plain-mlox --rules mlox_base.txt --sort-data-paths --write-cfg
```

It prints what moves; `--write-cfg` writes the order into the install (the profile's
`Morrowind.ini`, `loadorder.txt` and `plugins.txt`, and the `openmw.cfg` MO2's Export to
OpenMW writes, when it feeds OpenMW; the game's `Morrowind.ini` and its plugins' dates; the
cfg's `content=` lines), each file copied first. With `--plain-mlox`,
`--emit-toml` writes the customizations that make MOMW's configurator build the same order.
The scans and `--merge-lands` read these installs too.

`wraithguard --help` lists them all.

It is tes3cmd too, built in, with tes3cmd's own commands and flags, so a batch file written
for tes3cmd only needs `wraithguard` in front:

```
wraithguard tes3cmd clean --replace "My Mod.esp"
wraithguard tes3cmd header --synchronize "My Mod.esp"
```

It reads `Morrowind.ini` and `Data Files` from the folder it runs in, as tes3cmd does.
`wraithguard tes3cmd help` lists its commands.

---

## Where files live

- **Settings** (`wraithguard_toolkit_settings.json`), the scan cache and the Editor's
  patch journal are kept **beside the program**. Keep it in a folder you can write to.
- **Profiles and mesh setups** (`.toml`) are beside the program too, unless you choose
  another folder for them; existing files are moved there when you do.
- **Backups** sit beside the file they copy: timestamped for the cfg and the TOML, and kept
  once for a plugin: `.premaster.bak`, `.preedit.bak`, `.premerge.bak`, `.preclean.bak`,
  `.precustom.bak` or `.masterfix.bak` (tes3cmd's `header` keeps its own, `Name~1.esp`).
- **tes3cmd's working copies** go in `wraithguard_tes3cmd_staging` in the system's temporary
  folder, with each of the last ten runs' plugins as they were before the run (its `results`), for
  undo.
- **Patches** go where you choose when you Build. **Merged Lands.esp** goes in the folder
  Setup names, or the one holding most of the load order.
- **Exports** go to the customizations TOML (and the cfg, if you ask).

---

## Safety

- **Nothing is written until you say so.** Export, Build patch, Keep and the Tools' write
  buttons are the only things that write. *Dry run* and the command line's default write
  nothing (its `--write-cfg`, `--emit-toml` and `--merge-lands` write).
- **Backups first.** `openmw.cfg`, a customizations TOML, a plugin whose master is removed,
  a plugin you edit in place, and a file tes3cmd changes are all copied before they change.
- **A curated list's order is never changed.** Only your additions move.
- **Your mods change only when you ask for it.** The Editor's patch, Merged Lands and the Land
  view's edits are new plugins, loaded after the rest. A plugin of yours is written only when
  you choose it under *Edit a plugin*, or run tes3cmd or Remove / rename master on it from
  the Tools page, each with a backup first. Merge to master writes a new file and never its
  sources.
- **tes3cmd works on copies.** `clean` and other commands change a copy of each plugin, copied
  back only if the command succeeded and changed it (`header` runs in the plugin's folder and
  keeps tes3cmd's own backup). Any record a run changed can be put back from the Tools page. The built-in tes3cmd never writes a master: one it changes
  (`fixit`'s header sync, `resetdates`) is changed on a private copy.
- The MOMW team doesn't support customizations. This program helps you see and place
  them; it can't promise they're conflict-free.

---

## Building and developing

You need Rust (stable, 1.89 or later, via [rustup](https://rustup.rs)). On Linux you also
need the WebKitGTK 4.1 and GTK 3 development packages (`libwebkit2gtk-4.1-dev libgtk-3-dev
librsvg2-dev`). Cargo fetches the git dependencies itself: the `tes3` fork, merge_to_master
and [tes3cmd-rs](https://github.com/staticnation/tes3cmd-rs).

```
cp wraithguard_toolkit_icon.ico viewer-shell/icons/
cargo build --release --manifest-path viewer-shell/Cargo.toml       # the program
cargo build --release --manifest-path viewer-shell/cli/Cargo.toml   # the command line
```

The program is `viewer-shell/target/release/wraithguard-toolkit`, and the command line is
`viewer-shell/target/release/wraithguard` beside it (the crates are one Cargo workspace, with
one `target` folder).

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
    by `viewer-shell/check-commands` (`WG_MESH_VIEW=1` for the mesh viewer, `WG_SETUP=1`
    for a first start). It starts whichever build is there (`target/serve`, `release`, then
    `debug`; `WG_VIEW_SERVE` names another), fails at once if there is none, and fails
    after ten minutes (`WG_BOOT_LIMIT`) naming the engine calls still waiting;
  - `npm run preview` for the Preview's loading races, and `npm run sinks` for the
    innerHTML lint (a new place the page writes markup from a string fails it).

CI (`.github/workflows/ci.yml`) runs exactly that list; `deny.yml` checks every dependency's
source and license (`viewer-shell/deny.toml`), `audit.yml` and `osv-scanner.yml` look for
advisories, and `sast.yml` runs the static analysis. `build-app.yml` builds the release
packages on a `v*` tag or a `build/**` branch, the Linux AppImage with Tauri's bundler
(`cargo tauri build --bundles appimage` in `viewer-shell`).

The fixtures in `tests/fixtures/` pin the crates to the answers the retired Python gave
(the command line, the sort, the Configurator simulation, Merged Lands), so a change that
alters an answer fails loudly. `tools/README.md` says where each came from.
`viewer-shell/MANIFEST.md` lists every file of the page and every engine command;
`TODO.md` has the open work, and the design notes beside it (`viewer-shell/SOUND.md`,
`GLAM-PLAN.md`, `TES3CMD-REWRITE.md`, `GARDENFELL.md`) the plans it follows.

---

## Project layout

```
WraithguardToolkit/
├── *.md                     README, QUICKSTART, CHANGELOG, CREDITS, PREFLIGHT, TODO,
│                            MLOX_RULES (writing rules), MERGED_LANDS (the port's audit).
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
│   ├── physcore/            The Editor's Physics: drops, blasts, throws (Rapier).
│   ├── gardenfell/ gardenfell-g7/ meshopt-port/
│   │                        Vendored Gardenfell, its tes3 bridge, meshoptimizer ported.
│   └── check/ check-commands/
│                            Compile checks without Tauri's native tree, and
│                            wg-view-serve, the headless engine the page tests use.
├── tools/                   Developer scripts and the fixture generators.
├── tests/fixtures/          The crates' shared test cases.
├── testdata/                Copies of a real setup, used by the tests.
├── locale/                  The 4.x translations, kept for the program's (English only now).
└── art/                     The Nexus description, the icon.
```

---

## Credits and licenses

The program's code is MIT. Its page (`viewer-shell/ui`) is GPL-2.0, because it carries
ports of MGE XE's shaders, and is kept separate from the MIT code.

It stands on a lot of other people's work:

- greatness7's `tes3` crates and Robin Hjelte's Gardenfell;
- mlox and its rule databases, plox, tes3conv, tes3cmd (ported as tes3cmd-rs) and
  merge_to_master;
- MGE XE, OpenMW and Modding-OpenMW.com;
- Rapier, and many Rust crates.

[CREDITS.md](CREDITS.md) names each of them, what was taken, and under what license.
[CHANGELOG.md](CHANGELOG.md) has what changed in each release.
