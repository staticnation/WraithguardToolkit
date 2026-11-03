# Quick Start

From "I added some mods to my OpenMW setup" to "they're sorted, checked, and the fix is
saved" in a few minutes. The full reference is [README.md](README.md).

## What you need

- **The program**, from the
  [Releases page](https://github.com/staticnation/WraithguardToolkit/releases). Unpack it
  and run `wraithguard-toolkit` (`wraithguard-toolkit.exe` on Windows). Nothing else to
  install.
  - Windows 10 or later: the `.zip`. If WebView2 is missing (it was removed with Edge,
    or this is an LTSC or stripped install), take the `-webview2.zip` instead.
  - macOS: `macos-arm64` (Apple Silicon) or `macos-x86_64` (Intel). The first time,
    right-click the program and choose Open.
  - Linux and the Steam Deck: the `.AppImage`. Make it executable, then run it (on the
    Deck, from Desktop Mode). Or the `.tar.gz`, which needs the system's WebKitGTK 4.1
    (`libwebkit2gtk-4.1-0`).
- **Your `openmw.cfg`**. If you use a curated
  [Modding-OpenMW.com](https://modding-openmw.com/) (MOMW) list, the one MOMW
  Configurator wrote.
- **mlox rules**: `mlox_base.txt` and, optionally, `mlox_user.txt`. **Update rules** on
  the Load order page downloads them.
- Optional, for a MOMW list: your **`momw-customizations.toml`** (or a plain list of
  your plugins, or your mods folder), MOMW's **`plugin-order.yml`**, and the **list
  name** (`total-overhaul`, `expanded-vanilla` ...).

---

## In five steps

1. **Setup** opens on the first start. Point **openmw.cfg** at yours (**Find** looks
   where OpenMW keeps it), list the rule files (`mlox_base.txt` first), and add your
   customizations.toml, subset file or mods folder. Each field is checked as you type.
   Then **Save and load**.
2. *(MOMW lists)* Also in Setup: **plugin-order.yml** and the **list name**, so the sort
   tells the curated list apart from your additions and never moves the curated order.
3. Open **Load order**. It sorts as it opens: your mods placed into the order, the
   warnings listed beside it. Tick **Sort data paths** and sort again if your mods add
   asset folders.
4. Look it over. Drag a row (or select it and move it) to adjust; untick one to leave
   it out. The warnings filter by kind: rules, masters, plugin-order.yml, data paths ...
   Each warning's plugin names find the plugin in the order.
5. **Export**. It writes the corrected order into your customizations.toml (a
   timestamped backup first), or into the file Setup names under *Write customizations
   to*. `openmw.cfg` is left alone unless you tick *Write openmw.cfg*. With *Dry run*
   ticked, nothing is written.

### Then apply it

With a MOMW list, run MOMW Configurator again on the updated `momw-customizations.toml`.
Your mods now sort into place on every rebuild, and the curated list stays as it is.

---

## What else is here

The top bar switches between the pages.

- **The Cell Preview** (where the program opens): walk any cell of your load order in
  3D, with its terrain, objects, water, sky, weather and sound. Click an object to see
  what it is and which plugin placed it; Shift+click a door to go through it; **Cell
  map** (or the key under Escape) picks another cell. **Scripts** runs the scene's MWScript and OpenMW Lua;
  **Body** walks it at your character's pace.
- **Conflicts**: which plugins change the same records and files. Each record's fields
  are shown across its plugins, colored by who wins. Queue what you want kept into a
  patch.
- **Editor**: the Construction Set's windows around the render window. Change records,
  place and move objects, edit dialogue, scripts, path grids and terrain, then **Build
  patch** writes one new plugin. Your mods are never modified. **Test in OpenMW** starts
  the game on it.
- **Land**: sculpt, paint and color the terrain with a brush, into the Editor's patch.
- **Tools**: Lint, Backups, Remove / rename master, Save Check, Journal, tes3cmd, Merge
  Lands, Merge settings, Merge to master and the Mesh optimiser.
- **Help**: these documents, inside the program.

---

## The command line

The package's `wraithguard` does the sort and the scans without a window:

```
wraithguard --cfg openmw.cfg --rules mlox_base.txt mlox_user.txt \
    --scan-dir "E:\OpenMW\Mods\custom" --subset-file mod_scan_results.txt \
    --plugin-order-yml plugin-order.yml --list-name total-overhaul \
    --sort-data-paths --emit-toml momw-customizations.toml
```

Leave out `--emit-toml` to preview the plan and write nothing. A timestamped `.bak` is
made before anything is overwritten.

---

## Golden rules

- **Nothing is written until you Export.** *Dry run* writes nothing even then, and the
  command line previews by default.
- **A curated MOMW order is never reordered.** Only your additions move.
- **Your mods are never modified.** Patches, Merged Lands and the Land view's edits all
  go into new plugins.
- The MOMW team doesn't support customizations. This program helps you place and
  inspect them; it can't promise they're conflict-free.
