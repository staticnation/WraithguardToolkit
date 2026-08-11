# Project layout

Everything needed to **build, run and test** Wraithguard Toolkit lives in this
folder. Reference material (the upstream projects whose formats and behaviour
this tool mirrors) and scratch output were deliberately left outside it.

```
WraithguardToolkit/
├── *.md                          Project documentation, all at the top level:
│                                 README, QUICKSTART, CHANGELOG, CREDITS,
│                                 CODE_REVIEW, PROJECT_LAYOUT, MLOX_RULES,
│                                 NIF_PROVENANCE, MERGED_LANDS, AUDIT_REPORT,
│                                 REMAINING_WORK, and SMOKE_TEST.
├── License/                      One folder per upstream project whose licence
│                                 travels with code we ported or adapted.
├── wraithguard_toolkit.py        Engine + CLI. No GUI import; runs headless.
├── wraithguard_toolkit_gui.py    Tkinter front-end. Imports the engine.
├── wraithguard/               Shared foundation package.
│   ├── i18n.py                gettext translation, the _() marker.
│   ├── logging_setup.py       Levelled logging (stderr) + trace file.
│   ├── gui/                   GUI support (needs Tk): theming, widgets,
│   │                          tes3cmd + conflict-window mixins, app dir.
│   │                          Fully typed and mypy-gated, like the rest.
│   ├── mwscript/              Compiled-script (SCDT) reading + disassembly.
│   │                          Makes the diff window's bytecode legible.
│   ├── tes3fields/            Decodes binary LAND / PGRD fields for the diff
│   │                          window (heights, normals, colors, textures,
│   │                          world map, path-grid edges), plus the generated
│   │                          TES3 record schema that says what each field is.
│   ├── viz/                   Maps and visualisations as self-contained HTML:
│   │                          cell coverage map, conflict map, terrain height
│   │                          deltas, path-grid graphs, 3D surface, color
│   │                          ramps, generated-page cleanup, and the Markdown
│   │                          renderer behind in-app Help. No Tk, no CDN.
│   ├── images/                Every texture format the game and its mods use,
│   │                          decoded without a dependency: DDS (BC1-BC5, BC7,
│   │                          uncompressed, DX10 header), Targa, bitmap, and a
│   │                          zlib-only PNG writer. Picks the decoder by
│   │                          inspecting the bytes, because Morrowind's file
│   │                          extensions are genuinely unreliable. Also
│   │                          classifies a texture's *role* (diffuse, normal,
│   │                          glow, specular...) across the vanilla NIF slots,
│   │                          OpenMW's name suffixes and OSG unit names, so a
│   │                          normal map is never compared against a photo.
│   ├── nif/                   Morrowind NIF meshes: block reader, geometry,
│   │                          texture resolution, BSA-aware VFS, and the
│   │                          3D viewer page.
│   ├── land/                  The Merged Lands port: reference landmass,
│   │                          per-plugin diff, merge strategies, seam repair,
│   │                          slope and curvature conditioning, plugin emit.
│   ├── patch/                 Building a *new* patch plugin from records
│   │                          chosen in the diff viewer. Never writes to a
│   │                          source mod. Also the conflict-status model
│   │                          (`status.py`), its per-record and per-plugin
│   │                          roll-ups (`summary.py`), entry alignment for
│   │                          repeated fields (`align.py`), and dialogue
│   │                          topic ordering (`dialogue.py`).
│   ├── rules/                 mlox rule handling: patterns, parser,
│   │                          expression front-end.
│   ├── configurator/          openmw.cfg: read, simulate, emit TOML.
│   ├── momw.py                MOMW plugin-order.yml (curated lists).
│   ├── net/                   Downloads: rule files, curated order.
│   ├── plugins/               Plugin location + header metadata.
│   ├── sort/                  Load-order sort: graph primitives + engine.
│   ├── tracing.py             Crash-survival trace logs (main + sort).
│   └── versions.py            Version regex + mlox's canonical form.
├── wasm/                      A bridge from Greatness7's `tes3` NIF reader to
│                               the 3D viewer, so the page can parse a mesh
│                               itself instead of being sent packed geometry.
│                               **Written, never compiled** - see its README.
│                               Needs a Rust toolchain, which nothing else here
│                               does, and is not part of the Python build.
├── tools/                     Developer scripts (not shipped).
│   ├── check_placeholders.py  Verifies %(key)s placeholders match their dicts.
│   ├── check_undefined.py     Finds names a module uses but never imports.
│   ├── check_bc7.py           Compares the BC7 decoder against an independent
│   │                          one across all 8 modes and all 64 partitions.
│   ├── check_images.py        The same for every other texture format, plus
│   │                          real corpus files. Needs Pillow, which is an
│   │                          oracle here and not a dependency.
│   ├── check_bsa.py           Validates the BSA reader against a shipped
│   │                          archive: every extracted file must start with
│   │                          the magic its extension implies.
│   ├── check_nif_layouts.py   Runs the NIF reader over a corpus and buckets
│   │                          what it could not parse, and why.
│   ├── check_textures.py      Traces a texture from a mesh's reference to the
│   │                          pixels the viewer shows, naming the step it
│   │                          stopped at. For "why is this mesh untextured".
│   ├── gen_opcodes.py         Regenerates the opcode table from MWEdit and
│   │                          MWSE's customfunctions.dat.
│   ├── gen_tes3_schema.py     Regenerates the TES3 record schema from the
│   │                          UESP format-page export.
│   └── make_pot.py            Extracts _() strings into the .pot template.
├── tests/                     pytest suite (3,205 tests: 3,202 hermetic + a Tk
│                               smoke set that runs under xvfb in CI).
├── testdata/                  Copies of a real setup, used by the tests.
├── locale/                    wraithguard_toolkit.pot (English template),
│                               translator guide, .mo catalogues.
├── art/                       Icons, banner, Nexus description.
├── build/                     PyInstaller / auto-py-to-exe configuration.
├── License/                   This project's own MIT licence (`LICENSE`), plus
│                               the licences of the projects it ports from.
├── pyproject.toml             ruff / black / pytest / mypy configuration.
├── theme_template.json        Commented starting point for a custom GUI theme.
└── *.md                       README, QUICKSTART, CHANGELOG, CREDITS,
                               SMOKE_TEST; REMAINING_WORK (what a reviewer
                               would still flag, measured); CODE_REVIEW (a
                               running log, appended per work-block, oldest
                               first); *_BRIEF.md retired stubs (work done;
                               each points at its CODE_REVIEW section and
                               can be deleted).
```

## Running

```bash
python wraithguard_toolkit_gui.py          # GUI
python wraithguard_toolkit.py --help       # CLI
```

Only the standard library is required. Optional extras (`tkinterdnd2`,
`PyYAML`, `pywebview`/`tkinterweb`, `tomli` on Python < 3.11) each enable one
feature and degrade gracefully when missing.

## Testing

```bash
python -m pytest                # whole suite (3,202 tests)
python -m ruff check .          # lint (PEP 8 incl. naming + import order)
python -m mypy                  # types (PEP 484) -- gates every shipped file
python -m black --check .       # formatting
python tools/check_undefined.py wraithguard_toolkit_gui.py
python tools/check_placeholders.py   # i18n %(key)s vs dict keys
python tools/make_pot.py --check     # .pot template must be current
```

CI runs exactly this list on Python 3.10 and 3.13, plus `python -m build`
(which exercises the packaging metadata) and coverage against a `fail_under`
floor. Every shipped file is mypy-gated.

The suite is hermetic: no network (a local HTTP server stands in for
upstream), no Tkinter, no reliance on anything outside this folder. The
integration tests use `testdata/`; point them elsewhere with
`MLOX_TEST_DATA_DIR=/path/to/data`.

## Building a binary

`build/auto-py-to-exe_build.json` is an auto-py-to-exe configuration. Paths in
it are absolute and will need updating for your checkout - load it via
*Settings -> Import Config From JSON File* rather than retyping them. The
essentials:

* entry point: `wraithguard_toolkit_gui.py`
* one-file, windowed (no console)
* icon: `wraithguard_toolkit_icon.ico` (the copy in the project root; `art/` holds
  an identical one for reference)
* `--clean` on, so PyInstaller does not reuse a cached analysis

**One data folder does need adding: the 3D viewer library.** PyInstaller
follows imports, not data, so the vendored three.js build is not collected
automatically. It lives at **`wraithguard/viz/assets/`** and is loaded as
`assets/three.cjs`, so the entry maps the folder to `assets/`:

```
--add-data "wraithguard/viz/assets;assets"
```

Without it the app runs normally and the **View in 3D** button reports that the
library was not shipped - deliberately a clear message rather than a blank
window, since a missing data file and a broken viewer look identical otherwise.
`wraithguard/viz/library.py` looks in `sys._MEIPASS` first, exactly as the help
documents do.

*(This paragraph previously named `wraithguard/nif/assets/`, which does not
exist: `nif/viewer.py` gets the library from `viz.three_source()`. Following it
would have added nothing and left the viewer broken.)*

**You do not need to add `wraithguard/` or `locale/` by hand.** PyInstaller
follows the import graph, so the package is collected automatically; the only
`--add-data` entry is `wraithguard_toolkit.py`. `locale/` is a *developer*
directory - the `.pot` template is not a runtime file, and no `.mo` catalogues
ship yet. If you ever do ship translations, add `locale/` as data then; until
that day the app finds no catalogue directory, handles it, and runs in English.

**Verifying the build.** The Log panel's first line is a build stamp:
`Wraithguard Toolkit <version> -- frozen=True built=<timestamp>`. Check it before
believing any exe-only symptom - a stale build looks exactly like a code bug,
which has cost two debugging rounds. See `SMOKE_TEST.md` §5a.

## What was left outside this folder

Kept in the parent workspace, because none of it is needed to build or run:

* **Reference sources** - `mlox-master/`, `plox-main/`, `openmw-master/`,
  `momw-configurator-master/`, `tes3conv-master/`, `TES3Tool-master/`,
  `Tes3EditX-main/`, `modmapper-main/`, `modorganizer-master/`,
  `TES3 Conflictsolver/`. Read while porting; credited in `CREDITS.md`.
* **Third-party tools** - `tes3cmd`, `tes3lint.pl`, `cell_conflicts.pl`,
  `missing_pathgrids.pl` and their `.bat` wrappers. The tool drives `tes3cmd`
  when you point it at one; the Perl scripts' useful checks were ported into
  the native Lint feature.
* **Run output** - logs, `cell_map.html`, `resource_conflicts.csv`,
  `tes3conv_json/`, `output/`, the packaged `.exe` and `.7z`.
* **Superseded** - `mod_scan.py` (folded into the engine's scanner),
  `BRIEFING_sort_engine.md` (the original problem statement).
