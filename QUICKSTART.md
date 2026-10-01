# Quick Start

Get from "I added some custom mods" to "a corrected `momw-customizations.toml`"
in about five minutes. For the full reference, see [README.md](README.md).

## What you need

- Your **`openmw.cfg`** (the one MOMW Configurator generated).
- mlox rules: **`mlox_base.txt`** and (optionally) **`mlox_user.txt`**.
- Optional but recommended: MOMW's **`plugin-order.yml`** and your list's name
  (e.g. `total-overhaul`).
- The app itself: a release build from the
  [Releases page](https://github.com/staticnation/WraithguardToolkit/releases)
  (Windows `.exe`, or the `-webview2.zip` for a PC without WebView2; macOS `.app`;
  Steam Deck or any Linux: the `.flatpak`, installed with
  `flatpak install --user <file>`; other Linux: the `.tar.gz`, which needs the
  system's WebKitGTK 4.1, e.g. `libwebkit2gtk-4.1-0`) needs nothing else. From source you need Python
  3.14+ with tkinter (Linux: `sudo apt install python3-tk`) and the Rust module:
  `pip install ./native` with a Rust toolchain installed - see the README's
  *Requirements & setup*.

---

## The GUI in 6 steps

Launch the release build, or from source:

```
python wraithguard_toolkit_gui.py
```

1. **openmw.cfg** - Browse to your `openmw.cfg`.
2. **Rule files** - Add `mlox_base.txt`, then `mlox_user.txt` (base first).
3. **Get your subset** - either:
   - Browse to an existing `momw-customizations.toml` (**customizations.toml**
     field) or a subset text file, **or**
   - click **Scan...** next to *subset file* and pick your `custom` mods folder
     to generate the list automatically. If the scan misses a mod with a
     non-standard layout (an OpenMW Lua mod with no `meshes`/`sound`/... folder
     and no plugin), add it by hand: **Add data folder...** / **Add plugin...**
     under the lists on the left, or drag the folder/plugin onto them.
4. *(Recommended)* Set **list name** (e.g. `total-overhaul`) and point
   **plugin-order.yml** at MOMW's file. Now the tool tells your curated list
   apart from your true additions and won't touch the curated order.
5. **emit corrected TOML to** - choose where to save the result (a new
   `.toml`), then tick **Sort data= paths too** if your mods add asset folders.
6. Click **1. Sort**, look over the panels and log, then **2. Export**.
   - *Export writes nothing while **Dry run** is checked* (it's on by default).
     Uncheck it when you're happy, then Export for real.

### While reviewing (optional)

- **Reorder**: drag rows, or select + **Move Up/Move Down** (multi-select with
  Ctrl/Cmd- and Shift-click).
- **Opt out**: select row(s) and click **Disable / Enable** (or double-click) to
  leave mods out - handy when not everything you scanned needs to load.
- **Read the log colors**: green = inserted/moved by this sort, orange =
  warnings and rules your cfg order overrode, red = errors.
- **Check conflicts**: click **Check Conflicts** to scan for TES3 record-level
  conflicts (two plugins editing the same record; last one wins). Results show
  in the log and a dedicated window - ones involving your mods are marked ★.
  Selecting a record shows a **field-by-field diff**: each plugin's values, with
  differing fields in red.
- **Plugin view**: after a conflict scan, click **Plugin view** for your load
  order as a tree - open a plugin to see what it changes and a record to compare
  it across every plugin. The colours tell you which of your mods are *losing*
  work; they fill in on their own (a background pass judges the order once). You
  can also build a patch from here: the same **Add record / Merge field / Define
  value / Patch Builder** buttons as the conflict window, plus **Merge this
  plugin's fields...** to take chosen fields from one plugin across every record
  it defines.
- **Merge Lands**: build one `Merged Lands.esp` that combines the landscape edits
  of your whole load order and closes the seams between them, instead of the last
  mod winning a whole cell. Enable the output and load
  it **last**. By default the later mod wins only the vertices two mods *contest*,
  and everything else merges - so most load orders need no tuning. When a specific
  seam looks wrong, a `.mergedlands.toml` sidecar overrides it per plugin and per
  layer (winner / blend / yield / drop). Don't guess: open a landscape field diff,
  click **Compare strategies** to see the cell under each option on the 3D view,
  then write the sidecar with **Merge Settings**. The README's *Merged Lands*
  section walks through choosing a strategy - winner vs. blend vs. smallest-impact
  and when each fits.
- **Cell map**: click **Cell Map** for a modmapper-style SVG heatmap of which
  mods touch which exterior/interior cells (click a cell to jump to its list row).
  The map is written to a timestamped `cell_map` file and shown in the viewer's
  window (or with `tkinterweb`, or in your browser, without the viewer) - it is
  never rendered from an in-memory string, so big load orders won't OOM.
- **Cell preview**: click **Cell Preview** to look around a cell in 3D and check
  it for conflicts without loading the game. Pick a cell from the list or the cell
  map; every reference resolves to its winning object across the load order, and
  exteriors also draw their terrain, groundcover, water and neighbouring cells
  under a Morrowind sky with time-of-day and weather. Click an object for its
  `ori`-style readout; Shift+click a door to go through it. Orbit with the right
  mouse button, or switch to WASD (Tab) and hold the right button to fly.
- **Big load orders / memory / speed**: plugins are read by the built-in reader
  (Rust, in process), which parses only what a feature asks for and hands Python
  just those records, so memory stays bounded on 900+ plugins. Each plugin's
  record keys and cells are kept in memory until the file changes, so **repeat
  Check Conflicts and Cell Map runs are near-instant**.

### Then apply it

Feed the emitted `momw-customizations.toml` back into MOMW Configurator (put it
next to your `openmw.cfg` and re-run the Configurator). Your custom mods now sort
into place on every rebuild, and the curated list stays untouched.

---

## The one-liner (CLI)

Scan a mods folder, use MOMW's yml, and write a corrected TOML in one go:

```
python wraithguard_toolkit.py \
    --cfg openmw.cfg \
    --rules mlox_base.txt mlox_user.txt \
    --scan-dir "E:\OpenMW\Mods\custom" --subset-file mod_scan_results.txt \
    --plugin-order-yml plugin-order.yml --list-name total-overhaul \
    --sort-data-paths --emit-toml momw-customizations.toml
```

Drop `--emit-toml` (or run without it) to just preview the plan and write
nothing. A timestamped `.bak` is made before anything is overwritten.

---

## Golden rules

- **Nothing is written until you say so** (Dry run is on; the CLI previews by
  default).
- **Your curated MOMW order is never reordered** - only your additions move.
- Customizations aren't supported by the MOMW team; this tool helps you place
  and inspect them, not guarantee they're conflict-free.
