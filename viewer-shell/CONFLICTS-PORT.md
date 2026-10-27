# Conflicts into Rust (plan)

**Status:** slices 1-8, 10 and 11 done (`conflictcore`: `records`, `fields`, `resources`,
`mesh`, `textures`, `patch`, `map`, `vfs`; `native/src/conflicts.rs`; the Conflicts page
`ui/src/60_wg_conflicts.js` and the `conflicts_*` commands). Slice 9 moves to 4.4 (Merge
Settings), landed. `gui/conflicts.py`, `viz/conflictmap.py` and `viz/cellmap.py` retire with Tk
(4.5); the cell map page is the CLI's (`viewer-shell/cli/src/cellmap.rs`) already.

Step 4.3 of the road in `TODO.md`: record conflicts (`gui/conflicts.py`, 3.8k lines),
resource conflicts, the cell map. Same method as `PORT.md` (4.1) and the load-order
slices (4.2): the logic moves first, pinned by case files both sides run; Python calls it
through `native/`; then the viewer becomes the owner and each action a Tauri command.

Draft from reading `TODO.md`, `PORT.md`, `gui/conflicts.py`'s outline and the engine in
`wraithguard_toolkit.py` (`_scan_touch` 2379, `detect_conflicts` 2450, `diff_record_fields`
2187, `detect_resource_conflicts` 2814, `analyse_mesh_conflicts` 2936). Line numbers are from
the zip I was given; re-check them as each slice starts.

## What is already Rust

- Reading a plugin's keys: `native/src/esp.rs` (`keys_and_cells`, `conflict_key`) and
  `editcore::plugin`/`key`.
- The VFS walk and byte comparison: `native/src/fsio.rs` (`walk`, `identical`, `par_map`).
- NIF reading (`native/src/nif.rs`), images (`img.rs`), the coverage map
  (`viewcore/src/coverage.rs`), per-cell conflict counts (`viewcore/src/usage.rs`).

So the slices below are mostly the folding, the rules and the reports around reads that
already exist, plus the window.

## Where it lives (decide first)

A new crate `viewer-shell/conflictcore` (MIT, like ordercore/editcore), depending on
editcore for plugin reading and keys. Not ordercore (load order only) and not editcore (the
Editor's session). Decide: or a `conflicts` module in editcore, if you would rather not add a
crate; it would pull the same dependencies either way.

## Slices

1. **Record scan** (`_scan_touch`, `detect_conflicts`). The touch table (type, id) to
   (plugin, deleted) hits in load order, a plugin's repeats collapsed, `winner`,
   `deleted_by`, `involves_subset`, subset-first sort, `stats`. Cases:
   `tests/fixtures/conflict_cases.json` from `tools/gen_conflict_cases.py`. Declined
   (`Unsupported`): the tes3conv session path (the native reader is the one that ships).
2. **Singles** (`_list_singles`, `list_subset_singles`, `list_other_singles`). The same
   touch table, the other filter. Small; shares slice 1's cases file with new cases.
3. **Field diff** (`diff_record_fields`, `patch/status.py` `conflict_all`/`conflict_this`,
   272 lines). What the field view shows per field and plugin. The Editor's status column
   already needs the same rules, so check what editcore has before writing it again.
4. **Reports** (`format_conflict_report`, `write_conflict_csv`, the JSON dump at
   `conflicts.py:3393`). Pinned byte for byte, as export was in 2g. Decide: wording stays
   Python's until the flip, as in 2d.
5. **Resource scan** (`detect_resource_conflicts`, `format_resource_report`,
   `write_resource_csv`). Folding the walks in load order, the `identical` flag and its
   sort (differing first), `exclude_exts`, `compare_contents=False` carrying no key, plugin
   files skipped, subset folders matched case- and slash-folded.
6. **Mesh analysis** (`analyse_mesh_conflicts`, `nif/analysis.py`, 242 lines). Only meshes
   already known to differ; findings attached under `"mesh"`, absent meaning "not looked
   at". Pinned against a few real NIF pairs; the `limit` guard and the counters.
7. **Textures** (`conflicts.py` 1080-1400: the sides, the maps, the compare payload,
   `merge/textures.py`, 101 lines). On `img.rs`; the two-sided payload the viewer reads.
8. **Patch from a conflict** (`_patch_whole_record`, `_patch_field`, `_ask_patch_winner`,
   `_value_seed`, `_patch_field_value`). The queue and `set_field` are editcore's already, so
   only the pure rules move here (which value seeds the box, which plugin wins a field);
   the dialogs are UI and belong to the redesign.
9. **Merge strategies** (`_compare_merge_strategies`, the `merge/` package, ~1.4k lines).
   Overlaps 4.4 (Merge Settings); do it there if it turns out to share the model.
   *Landed in 4.4:* `_compare_merge_strategies` is the Conflicts page's "Terrain and
   strategies" on a `LAND` record (`conflicts_land_surfaces`, `landcore::preview`): each
   plugin's terrain and the merge under Overwrite, Resolve, Ignore and Curvature, shaded or
   as a difference. The strategy itself is chosen in the plugin's Merge settings (the Tools
   page, `landcore::meta`), which the terrain view opens for any of the cell's plugins.
   The `merge/` package (merge_to_master) is not used by the GUI and is not part of this.
10. **The map** (`viz/conflictmap.py` 517, `geometry.py`, `heightdelta.py`, `terrain3d.py`,
    `cellmap.py` 332). The viewer's coverage map already draws most of it: add the conflict
    layer to it (count per cell from `usage.rs`'s rule), the height-delta and terrain views
    on `viewcore`'s land, and retire `cellmap.py`.
11. **Python calls `conflictcore`** through native for 1-7 (as 2j did), then **the flip**:
    a Conflicts page, each action a Tauri command, the two scans run as jobs the page can
    cancel (the Python windows cannot), and `gui/conflicts.py` retired. Each Tk window is
    redesigned as it moves, not copied.

Order: 1, 2, 5 first (they are the scans, and everything else reads their output), then 3,
4, 6, 7, then 10 beside the page, 8 and 9 last.

## Not ported / left

- The tes3conv engine path (`Tes3ConvSession`) in the record scan: native only.
- Anything that is only Tk: the tree, sort indicators, searches (`_conf_search_find`,
  `_res_search_find`), the CSV save dialogs. The page replaces them.
