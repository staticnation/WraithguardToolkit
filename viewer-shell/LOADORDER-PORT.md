# The Load order page (plan, port step 4.2 slice 2k)

The flip for the load order: Sort and Export, the order panels, the rule panels, the rule
maker and Update rules and Sources move from Wraithguard's main Tk window to a page in the
viewer, each action a Tauri command over `ordercore`, and the Python sort and rule code is
retired. Same method as `PORT.md` (4.1) and slices 2a-2j: the logic moves first, pinned by
case files both sides run; then the owner.

Written 2026-10-26 from `wraithguard_toolkit.py`'s `compute_plan` / `write_plan`,
`wraithguard/configurator/apply.py` and the main window (`wraithguard_toolkit_gui.py`: the
fields, the options, `RuleFilesPanel`, the two `ReorderPanel`s, the log). Line numbers are
from that day's tree; re-check them as each slice starts.

## What is already Rust (2a-2j)

The rule files, patterns, the sort's graph and order, the predicate warnings, MOMW's
`plugin-order.yml` and its post-sort findings, the export's line helpers, `data=` inserts,
the customizations TOML, the cfg read and splice, `write_plan`'s writing half, the rule maker
and derive, Update rules and Sources. Since 2j Python calls all of it through `native/`.

## What is still Python

- **`compute_plan`** (`wraithguard_toolkit.py`, about 280 lines and 1,000 with its helpers):
  `_read_subset_inputs` (the customizations TOML, the subset file or lines, `--subset`,
  `--subset-from-cfg`, the mods-folder scan `scan_mod_directories`), `read_cfg`, the
  groundcover declarations and hold-back, `_apply_plugin_order_yml`, `_pull_cfg_orphans`,
  `_sort_subset` (the header masters, the rule files, the sort, the predicate warnings),
  `_check_masters`, `_plan_data_paths` (with `momw_datapaths`' reconcile), and the optional
  conflict and cell-map scans (`_conflict_and_cellmap_scans`, which are 4.3's).
- **`write_plan`'s orchestration** and the report it prints (sections, the manual-order
  previews, the summary), `cfg_segments`, `write_cfg`.
- **The configurator preview** (`configurator/apply.py`, 376 lines): the simulated
  momw-configurator apply that checks the emitted TOML reproduces the order. Never ported.
- **The main window**: the inputs (openmw.cfg, customizations.toml, subset file and its
  Scan), the outputs (emit TOML, in place, list name, plugin-order.yml, data-path cache),
  the options, the two order panels (drag to reorder, disable, add by hand), the rule-file
  panel (Update rules, Sources, ages), the rule maker, the log, and Sort / Export.

## Slices

1. **The plan's inputs into ordercore.** The customizations TOML (a `toml` crate; Python
   uses `tomllib`), the subset file and lines, the mods-folder scan, `--subset-from-cfg`,
   the groundcover declarations and hold-back, the orphans, the header-master check (plugin
   headers through `editcore::plugin`), the data-path plan and reconcile. Each pinned by a
   case file from the Python side, as before.
   *Done 2026-10-26* (`ordercore::inputs`, `tests/fixtures/input_cases.json`): the subset
   lines (`subset_from_lines`) and TOML form (`subset_from_toml`), `customizations`,
   `groundcover_declarations`, `hold_back_groundcover`, `mod_folders` /
   `scan_mod_directories`, `all_scan_dirs` / `pending_custom_dirs`, `list_plugins_in_dir`,
   `PluginIndex`, `check_missing_masters`, a list's data paths (`parse_data_paths_cache`,
   `managed_cfg_data_path_norms`, `reconcile_data_paths`), `cfg_orphans` and
   `infer_data_path_anchors`. Declined: a TOML value of a type the Python would stringify or
   iterate (so `subset = 'A.esp'` is still read a letter at a time, by the Python), and a
   byte-order mark; a file tomllib refuses is refused by the Python, in its words. The TOML
   reader is the `toml` crate 0.8 (TOML 1.0, as tomllib up to Python 3.14; 0.9 reads 1.1).
2. **`compute_plan` and `write_plan` whole** (`ordercore::plan`): one function each, the plan
   a struct, and the report as structured lines (kind + values) so the page and the Python
   can word and translate them. Python calls them through native (as 2j).
3. **The configurator preview** (`apply.py`) ported, pinned against the Python on the export
   cases' TOMLs.
4. **Tauri commands**: `loadorder_compute`, `loadorder_export`, `loadorder_rules` (files,
   ages, update, sources), `loadorder_author` (validate, render, append), checked by
   `check-commands` as the Editor's are.
5. **The page.** See the decisions below. The panels redesigned, not copied: the two orders
   with drag, disable and add; the inputs and outputs; the options; the rule files; the rule
   maker; the warnings as a list the page can filter instead of a log.
6. **The hand-off**, then **retire**: Wraithguard's Sort and Export go, and the tools still
   in Tk until 4.3 and 4.4 (Conflicts, Cell Map, Merge Lands, Lint) read the sorted order the
   page writes (see below). Then `wraithguard/rules`, `wraithguard/sort`, `momw.py`,
   `configurator/`, `net/updaters.py`, the bridge's fallbacks and the case generators'
   Python bodies are deleted; the case files stay as ordercore's tests.

Order: 1, 2, 3 (no UI, nothing to decide), then 4 and 5 once the page's place is decided,
then 6.

## Decisions (Static, 2026-10-26)

- **The viewer becomes the app's main window**, with a page for each part: Load order, Cell
  viewer, Editor, later Conflicts and Tools. Wraithguard's Tk window shrinks to the tools not
  yet ported.
- **The page writes the plan to a file** beside the patch journal (as the Editor's queue is
  shared), and the Tk tools still waiting for 4.3 / 4.4 (Conflicts, Cell Map, Merge Lands,
  Lint) read their sorted order from it. The Tk window loses its own Sort and Export.
- **The report is a list of warnings** by kind, filterable, each naming its plugins with links
  into the order panels; the full text behind a toggle. So `compute_plan` gives its report as
  structured items (slice 2), which the page lists and words.
