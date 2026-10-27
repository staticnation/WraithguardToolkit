# Quick Start

Get from "I added some custom mods" to "a corrected `momw-customizations.toml`"
in about five minutes. For the full reference, see [README.md](README.md).

## What you need

- Your **`openmw.cfg`** (the one MOMW Configurator generated).
- mlox rules: **`mlox_base.txt`** and (optionally) **`mlox_user.txt`** - or let
  **Update rules** fetch them.
- Your **`momw-customizations.toml`**, a subset file, or your `custom` mods folder.
- Optional but recommended: MOMW's **`plugin-order.yml`** and your list's name
  (e.g. `total-overhaul`).
- The program: a release from the
  [Releases page](https://github.com/staticnation/WraithguardToolkit/releases).
  Windows: the `.zip` (or `-webview2.zip` for a PC without WebView2); macOS:
  `macos-arm64` or `macos-x86_64`; Linux: the `.tar.gz` (needs the system's
  WebKitGTK 4.1, e.g. `libwebkit2gtk-4.1-0`). Unpack it and run
  `wraithguard-viewer` - nothing else to install.

---

## In 5 steps

1. **Setup** opens on the first start. Point **openmw.cfg** at yours (**Find**
   looks where OpenMW keeps it), add the rule files (`mlox_base.txt` first), and
   your **customizations.toml** (or a subset file, or your mods folder to scan).
   Each field is checked as you type. Save.
2. *(Recommended)* Also in Setup: **plugin-order.yml** and the **list name**, so
   the sort tells your curated list apart from your additions and never touches
   the curated order.
3. Open **Load order**. It sorts as it opens: your mods placed into the curated
   order, the warnings beside them. Tick **Sort data paths** (and Sort again) if
   your mods add asset folders.
4. Look it over. Drag a row (or select and move it) to adjust; untick one to leave
   it out. The warnings filter by kind (rules, masters, plugin-order.yml ...).
5. **Export**. It writes the corrected order back into your customizations.toml
   (a timestamped backup first), or into the file named in *Write customizations
   to*. openmw.cfg is left alone unless you tick *Write openmw.cfg*. With *Dry
   run* ticked nothing is written.

### Then apply it

Re-run MOMW Configurator with the updated `momw-customizations.toml`. Your custom
mods now sort into place on every rebuild, and the curated list stays untouched.

### While you are here (optional)

- **Conflicts**: which plugins edit the same records and files, each record's
  fields across its plugins coloured by who wins, and a Plugins tab with each
  plugin's losing work. Queue what you want kept into a patch.
- **Editor**: change records, place objects, edit dialogue and path grids, then
  **Build patch** writes one new plugin - your mods are never modified.
- **Tools**: Lint, Backups, Remove / rename master, Save Check (will this save
  load, and what takes the space), the Journal, tes3cmd, and **Merge Lands** -
  one `Merged Lands.esp` combining every mod's terrain edits, loaded last.
- **The cell viewer**: walk any cell in 3D with its terrain, objects, grass and
  sky; click an object for its readout, Shift+click a door to go through it.
- **Help**: these documents, in the program.

---

## The one-liner (command line)

The package's `wraithguard` does the same headless:

```
wraithguard --cfg openmw.cfg --rules mlox_base.txt mlox_user.txt \
    --scan-dir "E:\OpenMW\Mods\custom" --subset-file mod_scan_results.txt \
    --plugin-order-yml plugin-order.yml --list-name total-overhaul \
    --sort-data-paths --emit-toml momw-customizations.toml
```

Drop `--emit-toml` to just preview the plan and write nothing. A timestamped
`.bak` is made before anything is overwritten.

---

## Golden rules

- **Nothing is written until you Export** (and *Dry run* writes nothing even then;
  the command line previews by default).
- **Your curated MOMW order is never reordered** - only your additions move.
- **Your mods are never modified**: patches and Merged Lands are new plugins.
- Customizations aren't supported by the MOMW team; this tool helps you place and
  inspect them, not guarantee they're conflict-free.
