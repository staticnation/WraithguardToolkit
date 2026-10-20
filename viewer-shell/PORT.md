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
2. **The record cache** (done 2026-10-19). `editcore::cache`: `recordcache.py`'s 900 lines
   in Rust over rusqlite, with the same tables (records, the key index, use reports,
   reports) and the same rules (stamps, the size cap and trim, the reader's signature, two
   connections on one file). Bodies are zstd'd bytes of the caller's. The Python cache is
   now a thin wrapper over `wraithguard_native.RecordCacheDb`, and stores tagged JSON
   (`cache_dumps`: tuples, bytes, big ints and paths kept as they were) instead of pickles.
   The schema went to 4, so a database from an older build is emptied on first open.
   Tests: `editcore/tests/cache.rs`, and `tests/test_record_cache.py` through the wrapper.
   The viewer's session will store its own records here at the flip. Until then only the
   Python app writes the file: two programs with different reader signatures would each
   empty it on open.
3. **The queue, the journal and undo** (done 2026-10-19). `editcore::queue`: `queue.py`'s
   rules (re-deciding replaces, whole or merged, a build consumes only what it wrote),
   the journal (version 1, the Python app's file unchanged: either side reads what the
   other writes) and the undo history (200 steps). `Session::find_in` returns the patch's
   own new records, as the Python `find` does. Cases: `tests/fixtures/queue_cases.json`,
   twelve operation lists whose answers came from the Python side, run by
   `tests/test_queue_cases.py` and `editcore/tests/cases.rs`. The Python queue stays the
   owner until the flip; the Python app writes `queue_journal` (split out of `save_queue`).
4. **Views and edits**, in two parts.
   - **4a, records** (done 2026-10-19). `editcore::edit`: the record dialog (`view`), a
     typed value read as its field's type with its range checked (`set_field`, `coerce`),
     many records at once as one change (`set_many`, `batch`), `revert`, `insert` and
     `duplicate` (ids checked, one id space for objects, a copied script's `begin` line
     renamed), `pending`, and undo and redo naming what they touched. Alongside it:
     `editcore::fields` (the field-kind, flag and enum tables, from `data/fields.json`,
     which `tools/gen_editcore_tables.py` writes from the Python ones), `editcore::blank`
     (a blank record of a type, as the crate makes it: the same as the Python blanks,
     `blank_records.json`) and `editcore::pyval` (values read and quoted as Python does,
     so the refusals read the same). Cases: `tests/fixtures/edit_cases.json`, seven load
     orders and their operations, the answers (messages included) from the Python side,
     run by `tests/test_edit_cases.py` and `editcore/tests/cases.rs`.
   - **4b, placed objects, path grids, new cells and land** (done 2026-10-19).
     `editcore::places`: the reference dialog and its changes (`ref_view`, `set_ref_field`
     with `coerce_ref`, a move out of its square saying where it went, `revert_ref`), new
     references (`place`, `place_many`, `new_view`, `set_new_field` - one moved into
     another square goes to that square's cell - and `remove_new`), path grids
     (`pathgrid_view`, reading links as a list or packed; `set_pathgrid`;
     `revert_pathgrid`), `new_cell`, `new_land` (with `flat_land`, which the crate reads
     back) and `duplicate_cell`. Cases: `tests/fixtures/place_cases.json`, four load
     orders, run by `tests/test_place_cases.py` and `editcore/tests/cases.rs`; new
     references' random names are compared as labels, land's packed grids as digests.
     The patch keeps what it makes in the Python app's form (a path grid's links as a
     list), which the writer packs (slice 6).
5. **The rest of the reading**, in parts.
   - **5a, search and columns** (done 2026-10-19). `editcore::search`: the Object Window's
     search by any field (`path:text`, `path=value`, `path>n` ..., its words split as
     `shlex` splits them), the records of a type as the patch leaves them, and text found
     and replaced in every record type at once (`search_all`, `replace_all`, whole words as
     Python's lookarounds find them; `as_written`). `editcore::columns`: each type's own
     Object Window columns and armor's weight class from the load order's game settings.
     The condition editor's choices (`filter_choices`) are in `data/fields.json` now. A
     plugin's records are kept in file order (`IndexMap`), so hits and rows come in the
     Python's order. Cases: `tests/fixtures/search_cases.json`, order included. Pinned as
     the Python does it, and worth a look: the search matches a record's deleting version
     (a later plugin's DELETED record is the one searched).
   - **5b, the Use Report and Search & Replace** (done 2026-10-19). `editcore::uses`: the
     Use Report three ways that give one answer - the plugin files scanned (moved from
     `native/src/uses.rs`, which now calls it), records already read (`report_with`, the
     tests' seam) and the use index (`index_rows`, `from_index` over `editcore::cache`) -
     and Search & Replace (`replace_plan`, `replace_uses`: fields, placed references, the
     patch's own new references, script source). `editcore::rename`: an id renamed in
     script source as the compiler reads one (`replace_id`). `editcore::scripts`:
     `set_script_text`, a script's source saved with its bytecode as one change; the
     compiler is handed in (`Compile`) until it moves here in 5d. Cases:
     `tests/fixtures/use_cases.json`, run by `tests/test_use_cases.py` and
     `editcore/tests/cases.rs` (order included), with a stand-in compiler on both sides;
     `cases.rs` also checks the index answers each report as reading the records does.
     Two Python fixes came with it (CHANGELOG): the index and the in-memory walk now count
     words of dialogue results as the scan did, and a replace or a script save is one
     undo step. The Session holds no cache yet: the viewer wires the index in at the flip.
   - **5c, dialogue** (done 2026-10-20). `editcore::dialogue`: the order the engine reads
     a topic in (`topic_order`, `orphans`, `responses_by_topic`, from `dialogue.py`) and
     the dialogue window - every topic, a topic's responses, a response moved or added, a
     topic copied (`topics`, `topic`, `move_response`, `new_response`, `copy_topic`).
     `editcore::dialogue_graph`: `dialogue_graph.py`'s `DialogueIndex` (a topic's flow,
     the map around it, the flags the dialogue writes and tests, the rehearsal), the
     conditions in words of `tes3fields/dialogue.py` (`describe_filter`,
     `condition_lines`), the result-script reading (`script_effects`), and
     `who_can_say`; `Editor::dialogue_index` keeps the index while the patch's dialogue
     is unchanged, as `editorlink._dialogue_index` does. The Session reads each plugin's
     topics and responses once (`Session::dialogue`, a seam for the tests). Cases:
     `tests/fixtures/dialogue_cases.json`, run by `tests/test_dialogue_cases.py` and
     `editcore/tests/cases.rs` (order included); the responses the patch makes get random
     ids, compared as labels. Two Python fixes came with it (CHANGELOG): the flow and the
     rehearsal now see a response moved in the patch where the dialogue window does, and
     a typed change to a response's nested field no longer leaks into the plugin's
     record the session keeps.
   - **5d, scripts and the rest** (done 2026-10-20). `editcore::mwscript`: the MWScript
     compiler and Script Edit's checks moved here from `native/src/mwscript` (native
     wraps them for Python; `tools/gen_mwscript_compiler.py` writes `data.rs` here now),
     with MWEdit's byte-for-byte test. `ScriptCompiler` (the load order and the patch's
     records, `check_result`, `compile_record`) is the `Compile` that `set_script_text`
     takes. `mwscript::listing`: a `bytecode` field decoded and disassembled
     (`tes3conv.py`, `disassembler.py`; the opcode table is `data/opcodes.json`, written
     by `tools/gen_editcore_tables.py`). `mwscript::flowchart`: a script's flowchart.
     `editcore::scripts`: Script Edit's view (`edit_script`), a result checked
     (`edit_result`), `script_words`, `script_refs`, `script_flowchart`, `leveled_roll`,
     and `lua_chart` (luacore's charts, given back for the viewer to draw rather than
     opened in Wraithguard's chart window). `editcore::frames`: a plugin's framing,
     shared by the compiler's index and the Use Report. Cases:
     `tests/fixtures/script_cases.json`, run by `tests/test_script_cases.py` and
     `editcore/tests/cases.rs` (order included); each side writes the case's plugins to
     files first, for the compiler to read. Three Python fixes came with it (CHANGELOG).
   - **5e, Verify** (done 2026-10-20). `editcore::verify`: `verify.py` check for check -
     the ids a record names that nothing defines (`references`, `missing_ids`), leveled
     lists, the per-type checks (`record_checks`, files looked for through a
     `file_exists` the caller gives), an item's enchantment against what it carries,
     path grids (and an exterior's points under its land, `land_heights`), the scripts
     the patch changes compiled, the responses that lost their place and the journals
     (`dialogue_chain`, `journal`), the references the patch places, and the globals
     every game needs; `Editor::verify` runs them over what the patch carries. Cases:
     `tests/fixtures/verify_cases.json`, run by `tests/test_verify_cases.py` and
     `editcore/tests/cases.rs` (order included).
6. **Building and testing a patch** (`service.py`, `merge.py`, `refedit.py`, the
   Patch Builder's `prepare_patch`/`run_patch`/`finish_patch`, `testrun.py`).
   - **6a, the build** (done 2026-10-20). `editcore::build`: `build_record_patch` with
     what it uses of `records.py` (the master list a patch declares, references
     renumbered for it, an earlier build carried forward, a response taken with its
     topic, the notes on where carried responses sit, the responses the patch moves),
     `merge.py` (a record merged field by field), `refedit.py` (changed and new
     references) and the header; `write_patch` writes it with the crate, a reference's
     unnamed fields given the Python writer's defaults, byte for byte the same file.
     A Lua script list's renumbering (`lual_remap`, merge_to_master's `remap_refnums`)
     moved here from native, which calls it. `Editor::prepare_patch` (the job: the
     plugins to read, the masters' sizes, the queue's snapshot), `run_patch` (on any
     thread: the plugins read whole, `Session::read_whole`) and `Editor::finish_patch`
     (spends the snapshot, an edit made meanwhile kept). Cases:
     `tests/fixtures/build_cases.json` (the document, the report and the written bytes),
     run by `tests/test_build_cases.py` and `editcore/tests/cases.rs` (order included).
   - **6b, around the build** (done 2026-10-20). `editcore::buildjob`: where a patch
     may go (`patch_target`, `build_check`, `preview_target`), whether a build starts
     (`build_mode`), a build's job and `editBuildStatus` (`BuildState`, `build_status`),
     the launch setups (`update_setups`, `test_setups`), what a test starts with
     (`test_request`), `build_info`, and `Builds`, which runs `editBuild` and
     `editTestRun` on threads of their own (the plugins read outside the Editor's
     lock, the queue spent under it). `editcore::testrun`: `testrun.py` - the settings
     file, OpenMW found, its command line (`launch_args`) and the game run, its lines
     heard as it prints them. What the Python links decided inside `editorlink.py` moved
     to `wraithguard/patch/buildjob.py`, so both sides have one function to pin. Cases:
     `tests/fixtures/buildjob_cases.json`, run by `tests/test_buildjob_cases.py` and
     `editcore/tests/cases.rs` (order included). Opening Cell Preview on a written
     patch stays the viewer's own (`preview_target` checks the file).
7. **The flip.** The viewer holds the session, and each link is answered by its engine
   (`editcore::links`, the same JSON in and out as `editorlink.py`'s handlers) instead
   of over the loopback server. The patch queue stays shared with Wraithguard's conflict
   window and Patch Builder, which are still Python, through the journal both write
   (`wraithguard_patch_journal.json`): every change writes it, and each side reads it
   again before a link (and Wraithguard every second, for the Patch Builder) when it is
   not as that side last wrote or read it - the other side's change taken as one step,
   which an undo takes back (Static, 2026-10-20). Staged:
   - **7a, reading and editing** (done 2026-10-20). `editcore::links`: `Links` (the
     Editor and the journal it shares, `Links::sync`), `answer` for the links of slices
     3, 4 and 5a (`ANSWERS`). The shell's `editor_open` (the Editor opened on the load
     order and journal the launch's `extra.editor` names, from `editor_native`) and
     `editor_link`; `WgEditor.ask` sends a link `editor_open` named to the engine and
     the rest to Wraithguard, which keeps serving every link (a viewer that cannot open
     its Editor, an older build, asks it all). Wraithguard's side:
     `EditorSession.sync_journal`, every link registered through `_synced`, the Patch
     Builder redrawn when the viewer changed the queue (`redraw_patch_views`). Cases:
     `tests/fixtures/link_cases.json` (the links' answers, refusals and a journal
     changed by the other side), run by `tests/test_link_cases.py` through the handlers
     and by `editcore/tests/cases.rs` through `editcore::links`; the page's side by
     `ui/tests/editor/native_links.test.js`.
   - **7b, the rest of the Editor** (done 2026-10-20): the Use Report and Search &
     Replace, Script Edit's window and a result's check, the words and the scripts naming
     one, a leveled list's roll, the dialogue window (topics, a topic, a new, moved or
     copied response, the flow, the map, the flags, the rehearsal, who can say a line)
     and the verifier, which looks for the files records name in the install the viewer
     has open (`Ctx::file_exists`, its VFS).
   - **7c, the build** (done 2026-10-20): `editBuildInfo`, `editBuildCheck`, `editBuild`,
     `editBuildStatus`, `editTestSetups` and `editTestRun` answered beside the Editor's
     lock (`Links::build_link`, `editcore::buildjob::Builds`); the launch names the data
     folders, the patch last written and Test in OpenMW's settings file. Wraithguard no
     longer serves the links the viewer answers (`VIEWER_LINKS`, kept equal to
     `ANSWERS` by `tests/test_link_cases.py`): it hands them over as `native:<key>`, and
     a viewer that cannot answer one says it is older than Wraithguard. The handlers stay,
     the reference the link cases pin the viewer to. Wraithguard keeps the four that open
     its own windows (`WRAITHGUARDS`): `editReview` (the Patch Builder),
     `editFlowchart` and `editLuaChart` (the chart window), `editPreviewPatch` (a Cell
     Preview of its own). A merge the conflict window queues carries its base into the
     journal (`refresh_patch_views`), so the viewer can build it.

   What is left of step 4.1 is retiring the Python Editor itself: once nothing but the
   link cases calls the handlers, they and `EditorSession`'s editing go (the conflict
   window and the Patch Builder keep the queue and the journal until they are ported).

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
