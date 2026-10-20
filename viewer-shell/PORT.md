# The Editor's backend into Rust (plan)

Step 4.1 of the road in `TODO.md`. The viewer's Editor reaches Wraithguard through the
links in `__WG_VIEW__.extra.links` (`WgEditor.links()`, 50_wg_editor.js), served by
`wraithguard/gui/editorlink.py` (57 links) over `wraithguard/patch` (16k lines)
and its SQLite record cache. When this step is done, the Editor runs in the viewer with no
Python behind it.

## How it moves

**The logic moves first, then the owner.** A new crate, `viewer-shell/editcore` (MIT, like
ordercore), takes the Editor's logic one piece at a time. Until the last slice the Python
app still owns the session (the queue, the journal, the undo history) and calls editcore
through `native/`, so Python shrinks with each piece and nothing changes for a user. Then
the viewer becomes the owner: it holds an editcore session itself, and each link becomes a
Tauri command.

The owner cannot move one link at a time. The queue is one piece of state, and the Python
app and the viewer are two processes. A link that only reads plugins (no queue) can become
a Tauri command early; one that reads or writes the queue waits for the flip.

**Every piece is pinned by a case file both sides run**, as the cell, record, reference and
move rules are now (`tests/fixtures/*_cases.json`). Where the Python output is the
reference (views, pending lists), a script in `tools/` regenerates the expected answers
from the Python side, and editcore's test compares against them. When Python retires,
those files stay as editcore's tests.

## Slices

1. **Reading and lookups** (done 2026-10-19). `editcore::plugin`: plugins to and from
   tes3conv-schema JSON, moved out of `native/src/esp.rs` (native re-exports it, and the
   Python functions are unchanged). `editcore::key`: `record_key`, `is_interior`, the
   tag/type table. `editcore::session`: `find`, `find_ref` with the home-cell fallback for
   moved references, `masters_of`, `winning_reference`. Run against `record_cases.json`,
   `ref_cases.json` and `move_cases.json` in `editcore/tests/cases.rs`. The Python Editor
   does not call these yet: its `find` reads through the queue and the disk cache, which
   come next.
2. **The record cache** (`recordcache.py`, 900 lines). Rust over rusqlite, the same tables
   (records, the key index, use reports), with record bodies stored as zstd'd JSON rather
   than pickles. This clears the pickle finding in Aikido. The Python cache calls it
   through native; a database from the old build is emptied, as a schema change already
   does.
3. **The queue, the journal and undo** (`queue.py`, `journal.py`, `save_queue` /
   `restore_queue`, the undo history). The journal file format stays as it is, so a
   journal written by the Python app opens in the viewer and the other way round. Cases:
   journals and the edits they hold, then what `find` and `pending` give. `find` then
   returns the patch's own new records, as the Python one does.
4. **Views and edits** (`view`, `set_field` and `coerce`, `set_many`, `revert`, `insert`,
   `duplicate`, `pending`; references: `ref_view`, `set_ref_field`, `revert_ref`, new
   references, `place`, `place_many`; path grids; new cells and land). This brings over
   `field_types.py`, `values.py` and `columns.py`. The expected answers come from the
   Python side.
5. **The rest of the reading**: search and replace, columns, the Use Report (`uses.py`;
   native's `use_report` is already Rust), Verify, dialogue (topics, responses, flow,
   map, flags, play), scripts (words, refs, compile, result check), Lua charts (luacore
   has them), leveled rolls, who can say.
6. **Building and testing a patch** (`service.py`, `merge.py`, `workflow.py`,
   `testrun.py`): the build is merge_to_master, already Rust; what moves is the plan
   around it (targets, checks, status, the test setups).
7. **The flip.** The viewer holds the session (`editcore::Session` in the Tauri state,
   for the connected load order). Each link becomes a command `editor_<name>` with the
   same JSON in and out, and `WgEditor.ask` calls the engine when the viewer has a
   session. The Python app stops serving links. The page tests (`ui/tests/editor`)
   already drive the links through a harness and keep working against the commands.

## The links, by slice

The names are `editorlink.py`'s (`editor_links`); 57 in all.

| Slice | Links |
|---|---|
| 3 | editUndo, editRedo, editRevision, editPending, editReview |
| 4 | editRecord, editSet, editSetMany, editRevert, editInsert, editDuplicate, editRef, editRefSet, editRefRevert, editRefsSet, editPlace, editPlaceMany, editNew, editNewSet, editNewRemove, editNewCell, editNewLand, editDuplicateCell, editPathgrid, editPathgridSet |
| 5 | editSearch, editSearchAll, editReplace, editReplaceAll, editColumns, editUses, editVerify, editFilterChoices, editTopics, editTopic, editNewResponse, editMoveResponse, editCopyTopic, editDialogueFlow, editDialogueMap, editDialogueFlags, editDialoguePlay, editScript, editScriptWords, editScriptRefs, editResult, editFlowchart, editLuaChart, editLeveledRoll, editWhoCanSay |
| 6 | editBuildInfo, editBuildCheck, editBuild, editBuildStatus, editTestSetups, editTestRun, editPreviewPatch |

Plugin-only links (no queue) can become commands before slice 7: editLuaChart,
editScriptWords, editFilterChoices, and the dialogue views when the patch changes no
dialogue.
