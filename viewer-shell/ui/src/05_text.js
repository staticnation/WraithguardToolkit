/* =====================================================================================
   Everything the interface says that has no element of its own.

   For anything with an element, the markup is the source: the words are inside it, and a
   `data-tx="key"` names them so the rest of the page can ask for the same string with
   `T('key')`. `data-txt` does the same for a `title` tooltip and `data-txp` for a
   placeholder. Only the strings with nowhere to live — a label built in script, a hint
   whose wording depends on a switch — are written out in `TX_STRINGS` below, each with a
   note saying where it appears.

   These keys used to feed `gardenfell.text.toml`, a file beside the exe holding every
   string so the wording could be reworded or translated without editing the page. Robin
   asked for that to go (round 18aa) and it has: the file, the catalogue that built it, the
   engine command that carried it. What is left is the arrangement that was here before —
   the words in the code, in one place per string — and the keys, which is what any of it
   would be rebuilt on.
   ===================================================================================== */

const TX_STRINGS = {
  /* Round 17v: the report is the thing Robin pastes to me, and it was being selected by
     hand. Each section header carries its own copy button and the head carries one for
     the lot; both say so after they have run, because a clipboard write is silent. */
  "report.copysec":
    ["Copy",
     "Load-order report, the button on each section heading that copies that section."],
  "report.copysec_title":
    ["Copy this section to the clipboard",
     "Load-order report, that button's tooltip."],
  "report.copywait":
    ["Waiting for the GPU…",
     "The report's copy buttons while a GPU line is still being measured; the copy follows once it is in."],
  "report.copied":
    ["Copied",
     "Load-order report, what a copy button says for a moment after it has copied."],
  "report.copyfail":
    ["Could not copy \u2014 select the text and copy it by hand",
     "Load-order report, said when the clipboard refused the write."],
  /* Round 17y: a room without a sky holds the Atmosphere switch off; this is the reason
     on hover. Round 17y also: the door button in the object dialogue. */
  "preview.sky_room":
    ["Off in a room: an interior has no sky unless its cell is flagged to behave like an exterior. Your setting comes back when you leave.",
     "Left column, the tooltip on the dimmed Atmosphere row while an interior is up."],
  /* Said in script, so it has no element to take a default from: the never-accept-paint
     button changes what it says depending on whether this mesh is already on the list. */
  /* The paint verdict — one of three, picked in script from the pattern list and the
     per-reference switch, so the box has no single default to read from the markup. */
  /* Round 18n: the canopies section of the object dialogue. The verdict is one of two,
     picked in script from the canopy list, and the picker's last entry makes a new one. */
  /* Said in script when the painting button is greyed out: Simplified mode has no
     real ground to paint, so the button is disabled there and its tooltip says why. */
  /* The button's label is set in script — it names the state you are in, and reading
     the "default" back out of a relabelled button is how a label eats itself. */
  /* The Advanced scope's rule-kind chip, same reason: its label names the state it is
     in, and a `data-tx` on an element whose text the script rewrites makes the first
     relabel become the "default" and eat the wording it replaced. */
  "vp.cull_off":
    ["Highlight culling",
     "Viewport, the Highlight culling button while it is off."],
  "vp.cull_terrain":
    ["Highlight culling: Terrain only",
     "Viewport, the Highlight culling button in its second state."],
  "vp.cull_both":
    ["Highlight culling: Terrain + statics",
     "Viewport, the Highlight culling button in its third state."],
  "vp.hiobj_outline":
    ["Highlight objects: Outline",
     "Viewport, the Highlight objects button in its third state (round 18dq): an outline around the pointed-at object instead of the gold fill."],
  /* The two group headings in the rules list, and what hovering one says. They are built
     in script rather than written in the markup — the list is drawn from the config — so
     they need entries here to reach the wording file at all. */
  /* Round 18bf: the multi-edit's "paste one list over every selected rule", offered both
     where the lists are identical and where they differ. */
  /* Round 18o: three of the rule's own settings a card may state for itself - its
     height offset, its tilt and its shoreline turn. The pickers say what the rule does
     in their first entry, so "the rule's" is never a mystery. */
  /* Round 18p: the rest of the rule's terrain gates on the card. Robin: "moss on the
     north faces of a rule that grows everywhere", "mushrooms in the hollows of a rule
     that covers the ridges too" - so each is a plain low/high pair, blank meaning no
     gate, and a card can only narrow what the rule already allowed. */
  /* Round 18q: the card's two folded sections, its name, and the canopy list. */
  "preview.mode_hint_simple":
    ["A synthetic patch: instant, good for judging density and clustering.",
     "Left column, under the mode picker while Simplified is chosen."],
  "preview.mode_hint_cell":
    ["The real landscape and every placed object. Slower; the only way to see clipping.",
     "Left column, under the mode picker while Real cell is chosen."],
  "vp.nav_wasd_text":
    ["WASD navigation",
     "Viewport, the navigation button's own words while the WASD scheme is on. The Orbit wording is the button's markup (vp.nav_orbit)."],
  "preview.fogd_from":
    ["fog from",
     "Left column, on the sources line after Reset to install — before the file the fog distances came from."],
  "preview.sky_src":
    ["Sky:",
     "Left column, under the Weather picker — the lead-in before the list of files the sky's colours were read from."],
  "preview.sky_src_none":
    ["Sky: the game's own colours, until an install is connected.",
     "Left column, under the atmosphere rows while nothing is connected."],
  "preview.sky_src_builtin":
    ["no weather in the ini, the game's own colours",
     "Left column, under the atmosphere rows — the ini or openmw.cfg had no [Weather] blocks."],
  /* Round 17m: the file is named, because it is no longer always MGE.ini — G7's fork of
     MGE XE keeps everything in mgeXE.toml — and because a detector whose guess is
     invisible is how the wrong file went unnoticed for three months. */
  "preview.sky_src_mge2":
    ["{file} fog",
     "Left column, under the atmosphere rows — fog distances, and the file they were read from (mgeXE.toml or mge3\\MGE.ini)."],
  "preview.sky_src_mge_how":
    ["how the fog file was chosen: {how}",
     "Left column, the title (hover text) on the sources line — why that MGE config file was the one read."],
  "preview.sky_src_mge_default":
    ["MGE's default fog",
     "Left column, under the atmosphere rows — no MGE.ini, MGE XE's built-in fog distances."],
  /* Round 18i: the renderer the install runs decides how the scene is lit - MGE XE's
     per-pixel lighting, the fixed pipeline, or OpenMW's shaders - and the sources line
     names it. Robin: "If G7's fork of MGE XE is installed, always check the mgeXE.toml as
     well for relevant entries [...] no matter what version of MGE XE they use, or OpenMW." */
  "preview.sky_src_renderer":
    ["{label} lighting",
     "Left column, under the atmosphere rows — the renderer whose lighting model the preview follows, by its own name (e.g. 'MGE-XE G7 Fork v0.20.0', 'OpenMW', 'Morrowind.exe, no MGE')."],
  "preview.renderer_model_mge":
    ["MGE XE per-pixel lighting: 1/(q·d² + c) per lamp, the linear term dropped as its shader drops it; the lit colour tonemapped, not clamped; the grass takes sun and ambient only.",
     "Hover text on the sources line — what the MGE lighting model is."],
  "preview.renderer_model_ffp":
    ["fixed pipeline (per vertex in the game): 1/(c + l·d + q·d²) per lamp, the sum clamped to white.",
     "Hover text on the sources line — what the fixed-pipeline lighting model is (vanilla, MGE with per-pixel lighting off, OpenMW's legacy method)."],
  "preview.renderer_model_openmw":
    ["OpenMW shaders: 1/(c + l·d + q·d²) per lamp, capped at 1, faded out between {bounds}× the radius and twice that.",
     "Hover text on the sources line — what the OpenMW shader lighting model is; {bounds} is its light bounds multiplier."],
  "preview.renderer_att":
    ["[LightAttenuation] from {file}: constant {c}, linear {l}, quadratic {q} (a 256-unit lamp: c {c256}, l {l256}, q {q256})",
     "Hover text on the sources line — the light attenuation settings read, the file they came from, and the coefficients they give a 256-unit lamp."],
  "preview.renderer_att_default":
    ["[LightAttenuation] not in {file}: the game's shipped values (linear 3/r)",
     "Hover text on the sources line — no [LightAttenuation] section was found, so the shipped values are used."],
  "preview.renderer_post":
    ["post shaders: {list}",
     "Hover text on the sources line — the post-process shaders the install's chain names that the preview has an answer for (SSAO, sunshafts…), or 'none'."],
  /* Round 18ad: the two words beside the lock in the mesh editor. Both here rather than
     one in the markup, because `syncMeshLock` writes over that very element - which is
     the trap 18aa fell into with the connect buttons: a key whose wording is read back
     off an element a script rewrites answers with whatever the script last put there. */
  /* Round 18ap: keyed for the language packs (wording-plan.md) — what 11_events.js says. */
  "dlg.cancel":
    ["Cancel",
     "The option that changes nothing, in the second confirmation and the paste dialogue."],
  "top.connected_title":
    ["Connected — click to connect a different {kind} install",
     "Top bar, the tooltip on the connect button of the install that is live; {kind} is \"MO2\", \"OpenMW\" or \"Data Files\"."],
  /* Round 18bv: the export report. Robin: "Why not make a new report button that appears
     in the message box after an export ... On pressing it, we open up an export report
     dialogue which is similar in design and functionality as the report button opens from
     the top panel."

     Headings and the button only. The lines under them are developer diagnostics and stay
     English by design, the same rule the Loading and Performance sections already follow
     (wording-plan.md, "What is in scope") — and here there is a second reason for it: the
     same numbers come out of `GardenfellCLI bench` in English, and the whole point of the
     dialogue is that the two can be compared side by side. */
  "report.load_order":
    ["Plugin load order",
     "Load-order report: the first heading."],
  "report.masters":
    ["masters: {list}",
     "Load-order report, under a plugin: its masters; {list} is them, joined by dots."],
  "report.not_loaded":
    [" (NOT LOADED)",
     "After a master in that list that the order does not load."],
  "report.index_warnings":
    ["Could not read while indexing objects: {list}",
     "Load-order report: the warning box when some object records could not be read; {list} is the reasons."],
  "report.missing_plugins":
    ["<b>{n} plugin(s) listed but not found</b> through the current stack: {list}",
     "Load-order report: the warning box for plugins the order names but the install lacks; {list} is up to eight of them."],
  "report.assets":
    ["Assets",
     "Load-order report: the assets heading."],
  "report.assets_ok":
    ["Every referenced mesh and texture loaded cleanly.",
     "Load-order report, under Assets, when nothing was missing."],
  "report.loading":
    ["Loading",
     "Load-order report: the timings heading."],
  "report.performance":
    ["Performance",
     "Load-order report: the performance heading."],
  "report.tint":
    ["Ground tint",
     "Load-order report: the ground tint heading."],
  "report.tint_slider":
    ["tint slider {pct}%",
     "Load-order report, under Ground tint; {pct} is the slider."],
  "report.tint_off":
    [" — off, so nothing below is read by the grass",
     "After that line while the tint is off."],
  "report.textures":
    ["Textures",
     "Load-order report: the textures heading."],
  "vp.noengine_title":
    ["The cell viewer engine is not running",
     "The viewport when the page is opened in a plain browser: the big line."],
  "vp.noengine_body":
    ["This page is the front half of Wraithguard&rsquo;s cell viewer. Open it from Wraithguard&rsquo;s Cell Preview.",
     "Under that line."],
  "top.engine_title":
    ["native engine · {n} worker threads",
     "Top bar, the tooltip on the folder state; {n} is the engine's thread count."],
  "profiles.start_failed":
    ["Could not open your profiles — starting from defaults. {err}",
     "Toast at start-up when the profile store could not be read."],
  /* Round 18ap: keyed for the language packs (wording-plan.md) — what 11_events.js says, first half. */
  "vp.noctx_title":
    ["3D preview unavailable",
     "The viewport, the big line when WebGL could not start."],
  "vp.noctx_body":
    ["The rule editor and .toml export still work — only the viewport is affected.",
     "The viewport, under that line, after the browser's own reason."],
  "paint.n_cells.one":
    ["{n} cell",
     "Part of a \"Cleared … from …\" toast: how many cells."],
  "paint.n_cells.other":
    ["{n} cells",
     "The same for several."],
  /* Round 18ap: the paint layer row's own strings, which had keys with inline fallbacks and no catalogue entries. */
  /* Round 18ap: keyed for the language packs — the mesh editor, the twins, the statics export and import (18_cellpreview.js). */
  /* Round 18ap: keyed — the mesh editor's own lines, the stamp grid and the copy-setup dialogue. */
  /* Round 18ap: keyed — the statics export and import dialogues' innards, the busy card. */
  "cell.stats_title":
    ["real cell preview",
     "Viewport, the statistics box: its heading in Real cell mode."],
  "cell.no_cell_chosen":
    ["no cell chosen",
     "Viewport, the statistics box, before a cell is chosen."],
  /* Round 18cy: the visit history's two arrows on the statistics box's heading row. */
  "cell.hist_back_title":
    ["Back to {cell}",
     "The statistics box: the left arrow's tooltip; {cell} is the cell visited before this one."],
  "cell.hist_fwd_title":
    ["Forward to {cell}",
     "The statistics box: the right arrow's tooltip; {cell} is the cell visited after this one."],
  "cell.hist_back_none":
    ["No earlier cell this session",
     "The statistics box: the left arrow's tooltip when there is no cell to go back to."],
  "cell.hist_fwd_none":
    ["No later cell",
     "The statistics box: the right arrow's tooltip when there is no cell to go forward to."],
  "cell.status_choose":
    ["Choose a cell to load.",
     "Left column, Preview: the status line under the cell button before one is chosen."],
  "connect.failed_partway":
    ["The install could not be opened: {err}. Nothing is loaded — try connecting again.",
     "Toast when a connect threw part-way through and the page was left empty."],
  "cell.status_failed":
    ["That cell would not load: {err}",
     "Left column, Preview: the status line when a cell could not be read at all."],
  "cell.status_none_loaded":
    ["Nothing loaded. {list}",
     "Left column, Preview: the status line when every cell asked for failed, listing why."],
  "busy.loading_cell":
    ["Loading cell",
     "The busy card over the viewport while a cell loads."],
  "busy.n_of":
    ["{done} of {total}",
     "Under the busy card's heading: progress through a count."],
  "busy.building_terrain":
    ["Building terrain",
     "The busy card while the terrain is built."],
  "busy.placing_objects":
    ["Placing objects",
     "The busy card while the objects are placed."],
  "busy.n_of_meshes":
    ["{done} of {total} meshes",
     "Under \"Placing objects\": progress through the meshes."],
  "busy.scattering":
    ["Scattering grass",
     "The busy card, and the bottom bar, while grass is scattered."],
  "busy.scattering_sub":
    ["Scattering grass — {sub}",
     "The bottom bar's form of that with progress; {sub} is \"cell 3 of 9\"."],
  "busy.cell_n_of":
    ["cell {done} of {total}",
     "The progress under \"Scattering grass\"."],
  "busy.building_grass":
    ["Building grass",
     "The busy card while the grass is built for drawing."],
  /* Round 18ap: keyed for the language packs — the cell status line, the statistics box, the land-texture list and the cell picker (18_cellpreview.js). */
  "cell.status_loaded":
    ["{name} — {cells}, {refs}",
     "Left column, Preview: the status line under the cell button once a cell is loaded. {name} is the cell, {cells} and {refs} are the two counts below."],
  "cell.n_cells":
    ["{n} cell(s)",
     "The status line: how many cells were loaded (the chosen one and its neighbours)."],
  "cell.n_references":
    ["{n} references",
     "The status line: how many object references those cells hold."],
  "cell.status_interior":
    ["interior, no landscape",
     "The status line, appended after \" · \" when the loaded cell is an interior."],
  "cell.status_no_land":
    ["{n} without a LAND record",
     "The status line, appended when some loaded exterior cells have no landscape record."],
  "cell.status_failed":
    ["{n} failed to load, so the blend at those edges is not what the export will use",
     "The status line, appended when neighbouring cells would not load."],
  "cell.status_unread":
    ["{n} plugin(s) could not be read: {list}",
     "The status line, appended when a plugin in the load order could not be read; {list} names them."],
  "cell.status_unresolved":
    ["{n} reference(s) point at a master that is not loaded, so they cannot be identified and may duplicate",
     "The status line, appended when references name a master that is not in the load order."],
  "vp.recentred":
    ["View recentred",
     "Toast after the recentre-view shortcut puts the camera back."],
  "cell.st_water":
    ["water",
     "Statistics box (viewport, Real cell mode): the row for an interior's water."],
  "cell.st_water_none":
    ["none",
     "Statistics box: the water row's value when the cell has no water level."],
  "cell.st_water_units":
    ["{n} units",
     "Statistics box: the water row's value — the level in game units."],
  "cell.st_water_tip":
    ["Level from {level} · water flag from {flag}",
     "Statistics box: the water row's tooltip — which plugin set the level and which set the flag."],
  "cell.st_water_nobody":
    ["nobody — the flag alone",
     "Statistics box: the water tooltip's {level} when no plugin set a level."],
  "cell.st_cell":
    ["cell",
     "Statistics box: the row naming the cell."],
  "cell.st_cells_loaded":
    ["cells loaded",
     "Statistics box: how many cells are loaded."],
  "cell.st_textures":
    ["textures",
     "Statistics box: how many land textures the loaded cells use."],
  "cell.st_references":
    ["references",
     "Statistics box: how many object references the loaded cells hold."],
  "cell.st_overridden":
    ["overridden by later",
     "Statistics box, under references: references a later plugin replaced."],
  "cell.st_deleted":
    ["deleted by later",
     "Statistics box, under references: references a later plugin deleted."],
  "cell.st_moved_out":
    ["moved out",
     "Statistics box, under references: references a later plugin moved to another cell."],
  "cell.st_moved_in":
    ["moved in",
     "Statistics box, under references: references a later plugin moved into this cell."],
  "cell.st_objects":
    ["objects found",
     "Statistics box: how many placed objects the loaded cells hold."],
  "cell.st_no_mesh_record":
    ["no mesh record",
     "Statistics box, under objects: objects whose record names no mesh. Shown red."],
  "cell.st_mesh_missing":
    ["mesh missing",
     "Statistics box, under objects: objects whose mesh file is not in the load order. Shown red."],
  "cell.st_editor_markers":
    ["editor markers",
     "Statistics box, under objects: editor markers the game never draws."],
  "cell.st_corpses":
    ["corpses",
     "Statistics box: bodies among the objects."],
  "cell.st_npc_markers":
    ["NPC markers",
     "Statistics box, under corpses: NPC markers."],
  "cell.st_actors_skipped":
    ["live actors skipped",
     "Statistics box: living actors left out of avoidance."],
  "cell.st_moths":
    ["moths",
     "Statistics box: how many lamps in the scene carry Nocturnal Moths (only when that mod is installed)."],
  "cell.st_moths_tip":
    ["MWSE - Nocturnal Moths is installed. {from} names {meshes} lantern mesh(es); {lamps} of the lamps in this scene wear one. They are drawn during the Lights picker’s night hours, and the Particles switch carries them.",
     "Statistics box: the moths row's tooltip. {from} is where the lantern list came from (the mod's own list, or the one it ships with)."],
  "cell.moths_from_own":
    ["the mod’s own list",
     "The moths tooltip's {from} when the installed mod's own lantern list was read."],
  "cell.moths_from_shipped":
    ["the list the mod ships with",
     "The moths tooltip's {from} when the mod's list could not be read and the built-in one is used."],
  "cell.st_triangles":
    ["triangles",
     "Statistics box: the triangle counts."],
  "cell.st_triangles_v":
    ["{grass} grass · {statics} statics",
     "Statistics box: the triangles row's value."],
  "cell.no_textures":
    ["No land textures loaded.",
     "Left column, the land-texture list before a cell is loaded."],
  "cell.tex_no_image":
    ["no image",
     "The land-texture list: the tag on a texture whose image file is not in the overlay."],
  "cell.tex_no_image_title":
    ["The LTEX record names {file}, which is not in the overlay as a loose file.",
     "The land-texture list: the \"no image\" tag's tooltip."],
  "cell.tex_no_file":
    ["no file",
     "The \"no image\" tooltip's {file} when the LTEX record names no file at all."],
  "cell.btn_none":
    ["<none>",
     "Left column, Preview: the cell button before a cell is chosen."],
  "cell.btn_interior":
    ["interior",
     "Left column, Preview: the cell button for an interior with no name."],
  "cell.btn_loaded_title":
    ["{cell} — loaded; click to choose another",
     "The cell button's tooltip once a cell is loaded."],
  "cell.btn_title":
    ["Choose a cell to load",
     "The cell button's tooltip before a cell is chosen."],
  "connect.setup_first":
    ["Connect a setup first.",
     "Toast when the cell picker is opened before a setup is connected."],
  "cell.pick_head":
    ["Choose a cell",
     "The cell picker dialogue's heading."],
  "cell.pick_filter":
    ["filter… (name, region, or grid like -3,12)",
     "The cell picker's filter box placeholder."],
  "cell.pick_all":
    ["All",
     "The cell picker: the filter button for every cell."],
  "cell.pick_named":
    ["Named",
     "The cell picker: the filter button for named cells."],
  "cell.pick_wild":
    ["Wilderness",
     "The cell picker: the filter button for unnamed exteriors."],
  "cell.pick_int":
    ["Interiors",
     "The cell picker: the filter button for interiors (shown when interiors are listed)."],
  "cell.pick_show_int_title":
    ["Interiors have no landscape, so nothing is scattered in one yet — this is the first step towards them",
     "The cell picker: the Show interiors switch's tooltip."],
  "cell.pick_show_int":
    ["Show interiors",
     "The cell picker: the label beside the switch that lists interiors."],
  "cell.pick_clear":
    ["Clear selection",
     "The cell picker: the button that unloads the chosen cell."],
  "cell.pick_count":
    ["{n} of {total}",
     "The cell picker: the pill in the heading — matches out of all cells."],
  "cell.pick_first_900":
    ["Showing the first 900 of {n}. Narrow the filter to see more.",
     "The cell picker: the note when more than 900 cells match."],
  "cell.pick_nothing":
    ["Nothing matches.",
     "The cell picker: shown when the filter matches no cell."],
  /* Round 18cp: the picker's map — the world painted from its ground textures, a cell a
     square, in place of the list (31_cellmap.js). */
  "cell.view_list":
    ["List",
     "The cell picker: the button that shows the cells as a list."],
  "cell.view_map":
    ["Map",
     "The cell picker: the button that shows the cells as a map of the world."],
  "cell.view_list_title":
    ["Every cell as a list, interiors included",
     "The cell picker: the List button's tooltip."],
  "cell.view_map_title":
    ["The exterior world as a map: wheel to zoom, middle button to pan, click a cell to load it, right-click to star it",
     "The cell picker: the Map button's tooltip."],
  "cell.map_reading":
    ["Reading the map… {done} of {total} cells",
     "The cell picker's map, while its tiles are being read: {done} and {total} are counts."],
  "cell.map_failed":
    ["The map could not be read: {err}",
     "The cell picker's map, when the engine could not paint it; {err} is the engine's message."],
  "cell.map_empty":
    ["Nothing to map — this load order has no exterior landscape.",
     "The cell picker's map, when the world has no cell with a landscape."],
  "cell.map_tip_where":
    ["({x}, {y}) · {region}",
     "The map's hover note, second line: the cell's grid position and its region. The first line is the cell's name, or its region when it has none."],
  "cell.map_tip_where_noregion":
    ["({x}, {y})",
     "The map's hover note, second line, for a cell in no region: its grid position alone."],
  "cell.map_wilderness":
    ["Wilderness",
     "The map's hover note: what an unnamed cell in no region is called."],
  "cell.map_star_note":
    ["Right-click to star it · starred",
     "The map's hover note, third line, on a starred cell."],
  "cell.map_star_note_off":
    ["Right-click to star it",
     "The map's hover note, third line, on a cell that is not starred."],
  "cell.map_current":
    ["loaded now",
     "The map's hover note: added after the name of the cell that is currently loaded."],
  /* Round 18ap: keyed for the language packs — the object dialogue (verdicts, paint, canopies) and the two pattern lists (18_cellpreview.js). */
  "connect.cell_preview":
    ["Connect a setup first — cell preview needs the plugin data.",
     "Toast when Real cell mode is chosen before a setup is connected."],
  /* Round 18ap: keyed for the language packs — the statics export list (18_cellpreview.js). */
  /* Round 18ap: keyed for the language packs — the rules list, the rule head, the paste dialogue, the pickers and the scope field (09_ui.js). */
  "name.default_title":
    ["Name it",
     "The name-it dialogue's heading when the caller gives none."],
  "name.create":
    ["Create",
     "The name-it dialogue's confirming button when the caller gives no wording."],
  "name.taken":
    ["There is already a rule called that. Both will work — they are told apart by what they are, not by what they are called — but the two will be hard to tell apart in a list.",
     "The name-it dialogue: the warning under the field when the name is already in use."],
  /* Round 18ap: keyed for the language packs — the slots and bans sections and the grass card (09_ui.js). */
  "card.tag_no_folder":
    ["no folder",
     "A grass card's tag: no Data Files folder is connected, so the mesh cannot be checked."],
  /* Round 18bf: the card names a mesh **or** an object, chosen here rather than inferred
     from whichever field you filled in last. */
  /* Round 18av: a card naming an object from the load order (20_objects.js). */
  /* Round 18bg: the picker's two views and what a tile says when there is nothing to
     draw. Robin: "make the reference 'from the load order' picker have the option of
     either list view (what we have now) or a grid view like other pickers […] If the
     object misses something to render completely, write a message in the grid square
     'No visuals'." */
  /* Round 18ap: keyed for the language packs — painting tools, the import and export dialogues, the report load order (11_events.js). */
  "top.folder_report_title":
    ["Click for the plugin load order and asset report",
     "Top bar: the connected-setup label's tooltip."],
  "report.order_read":
    ["Read from {source} — {n} plugin(s), in the order the game loads them. Later entries override earlier ones. This should match MO2's right pane top-to-bottom.",
     "The report dialogue: where the load order came from; {source} is bold."],
  "report.order_none":
    ["<b>No plugin load order found.</b> Looked for the MO2 profile's Morrowind.ini [Game Files], then loadorder.txt / plugins.txt, then Morrowind.ini in Data Files. Texture and region checking stays off until one is found.",
     "The report dialogue when no load order was found."],
  /* Round 18ap: keyed for the language packs — profiles and the rebind dialogue (23_profiles.js). */
  "profiles.no_engine":
    ["The engine is not running.",
     "Toast when the profiles dialogue is opened with no engine."],
  /* Round 18ap: keyed for the language packs — the rules sets (24_rules.js). */
  /* Round 18ap: keyed for the language packs — the export dialogue and its summaries (21_openmw.js). */
  /* Round 18bf (§I22): the Advanced list's three tick counters. They were built by
     concatenation — `total.on+' of '+total.all+' ticked'` — so no language pack could
     reach them and `t_wording` could not see them. */
  /* Round 18bf (§I23): why Save is greyed when the engine could not write the file. It
     used to stay live and write a zero-byte one. */
  "busy.cancel":
    ["Cancel export",
     "The button under the loading bar that stops a running export."],
  /* Round 18av: the Objects .esp and its dependencies (21_openmw.js, `Deps`). */
  /* Round 18ap: keyed for the language packs — the land-texture picker and the colour rule picker. */
  /* Round 18ap: keyed for the language packs — the files folder and its move (19_settings.js). */
  "profiles.not_saved_read":
    ["profile not saved — settings could not be read: {err}",
     "The report dialogue: a warning when the settings could not be gathered for saving."],
  "profiles.not_saved":
    ["profile not saved: {err}",
     "The report dialogue: a warning when the profile file could not be written."],
  "settings.btn_title":
    ["Settings",
     "Top bar: the Settings button's tooltip before the folder is known."],
  /* Round 18ap: keyed for the language packs — the viewport legend (06_gl.js). */
  "legend.shallower":
    ["shallower than {deg}",
     "Viewport legend, the \"No grass placed\" note: ground under the rule's lowest slope."],
  "legend.steeper":
    ["steeper than {deg}",
     "Viewport legend, the \"No grass placed\" note: ground over the rule's steepest slope."],
  "legend.facing_away":
    ["facing away from {deg}",
     "Viewport legend, the \"No grass placed\" note: ground facing away from the rule's direction."],
  "legend.this_card":
    ["(this card)",
     "Viewport legend: appended to the facing note when it is the card's own facing, in the mesh editor."],
  "legend.below":
    ["below {n}",
     "Viewport legend, the \"No grass placed\" note: ground under the rule's lowest height."],
  "legend.above":
    ["above {n}",
     "Viewport legend, the \"No grass placed\" note: ground over the rule's highest height."],
  "legend.wrong_shore":
    ["the wrong distance from the sea",
     "Viewport legend, the \"No grass placed\" note: the shoreline band."],
  "legend.wrong_curve":
    ["the wrong shape of ground",
     "Viewport legend, the \"No grass placed\" note: the curvature band."],
  "legend.wrong_object":
    ["the wrong distance from an object",
     "Viewport legend, the \"No grass placed\" note: the near-objects band."],
  "legend.canopy":
    ["what does or does not hang over it",
     "Viewport legend, the \"No grass placed\" note: a canopy condition."],
  "legend.no_grass":
    ["No grass placed",
     "Viewport legend: the cull overlay's entry; its note lists the reasons."],
  "legend.painted_bare":
    ["Painted: no grass",
     "Viewport legend: the No grass cover colour."],
  "legend.painted_grass":
    ["Painted: grass here",
     "Viewport legend: the Grass cover colour."],
  "legend.a_rule":
    ["a rule",
     "Viewport legend: what to call a painted rule with no name."],
  "legend.painted_rule":
    ["Painted: {rule}",
     "Viewport legend: a rule colour's entry."],
  "legend.brush":
    ["Brush",
     "Viewport legend: the brush ring while painting."],
  "legend.brush_note":
    ["where it will land",
     "The brush entry's note."],
  "legend.hover_texture":
    ["Hovered texture",
     "Viewport legend: the highlight of a hovered land texture."],
  "legend.hover_texture_note":
    ["the ground it covers",
     "The hovered-texture entry's note."],
  "legend.hover_slot":
    ["Hovered grass slot",
     "Viewport legend: the highlight of a hovered grass card."],
  "legend.hover_slot_note":
    ["every blade it placed",
     "The hovered-slot entry's note."],
  "legend.mesh_missing":
    ["Mesh not installed",
     "Viewport legend: the orange spikes where a missing mesh would stand."],
  "legend.mesh_missing_note":
    ["a position won and nothing to draw",
     "The mesh-not-installed entry's note."],
  "legend.reserved":
    ["Reserved, nothing drawn",
     "Viewport legend: the markers for empty and object-id slots."],
  "legend.reserved_note":
    ["empty or object-id slots",
     "The reserved entry's note."],
  /* Round 18ap: keyed for the language packs — connecting a setup and its log (13_mo2.js). */
  "busy.reading_install":
    ["Reading the install",
     "The busy overlay while a setup is connected."],
  "busy.resolving_overlay":
    ["resolving the overlay…",
     "The busy overlay's second line: working out which mod folders win."],
  "busy.drawing_scene":
    ["drawing the scene",
     "The connect's busy card, while the viewport rebuilds after the world has been read."],
  /* Round 18cx: the cell picker's map is read at the connect, so the picker opens with it. */
  "busy.reading_map":
    ["reading the map…",
     "The connect's busy card, while the cell picker's map is read ahead, before its cell count is known."],
  "busy.reading_map_n":
    ["reading the map… {done} of {total} cells",
     "The connect's busy card, while the cell picker's map is read ahead: {done} and {total} are cell counts."],
  "connect.log_map":
    ["map: {cells} cells read ahead in {ms} ms",
     "The connect log: the cell picker's map, read at the connect so the picker opens at once."],
  "connect.log_map_failed":
    ["map: not read ahead — {err}",
     "The connect log, when the cell picker's map could not be read at the connect; {err} is the reason. The picker reads it again when opened."],
  "busy.indexing":
    ["indexing cells and assets…",
     "The busy overlay's second line: reading the plugins and listing the assets."],
  "connect.log_plugins":
    ["{plugins} plugins, {size}, {records} records",
     "The report dialogue, the connect log: what was read."],
  "connect.log_times":
    ["read {read} ms · parse {parse} ms · merge {merge} ms",
     "The connect log: how long each step took."],
  "connect.log_overlay_times":
    ["overlay: layout {layout} ms · index {index} ms ({files} files in {roots} folders) · archives {archives} ms · plugin list {plugins} ms · headers {headers} ms",
     "The connect log: how long the engine took to work the install out, step by step, before any plugin was read."],
  "connect.log_page":
    ["this window: load order {probe} ms · reading the world {open} ms · listing {listing} ms · first scene {scene} ms",
     "The connect log: how long the window waited at each step of the connect, on its own clock; the first scene is counted from the connect's start to the first cell scene standing."],
  "connect.log_cells":
    ["{cells} cells, {lands} landscapes",
     "The connect log: cells and landscapes found."],
  "connect.log_indexed":
    ["{meshes} mesh files and {textures} textures indexed",
     "The connect log: the asset index."],
  "connect.log_archives.one":
    ["{n} archive read — {names} — {packed} packed files behind the loose ones",
     "The connect log: the .bsa archives read (one)."],
  "connect.log_archives.other":
    ["{n} archives read — {names} — {packed} packed files behind the loose ones",
     "The connect log: the .bsa archives read."],
  "connect.log_no_archives":
    ["no .bsa archives read — the load order names none and the install has none",
     "The connect log when no archive was read."],
  "connect.log_order":
    ["load order from {file}",
     "The connect log: which file the load order came from."],
  "connect.log_mods":
    ["{n} active mods in the overlay",
     "The connect log (MO2): active mods."],
  "connect.log_mods_off":
    ["{n} switched off and not read",
     "The connect log (MO2): appended for disabled mods."],
  "connect.log_overlay":
    ["overlay, later overrides earlier: {roots}",
     "The connect log: the mod folders whose files win, listed the way MO2 lists them — the last one named wins."],
  "connect.log_overlay_more":
    ["{n} more before these",
     "The connect log: the count of lower-priority folders, shown at the head of the overlay line."],
  "connect.log_refs":
    ["{kept} references kept, {overridden} overridden, {deleted} deleted, {moved} moved",
     "The connect log: how the plugins' references merged."],
  "connect.log_mcp":
    ["Morrowind Code Patch: {n} features applied",
     "The connect log: the Code Patch was found."],
  "connect.log_mcp_banners":
    ["\"Improved animation support\" is on, under which the game's scripted banners hang still; here they wave outdoors all the same",
     "The connect log: appended when that Code Patch feature is on."],
  "connect.log_missing":
    ["plugin not found in the overlay: {list}",
     "The connect log: plugins in the load order that were not found."],
  "connect.toast_mods":
    ["{n} active mods",
     "The toast after connecting (MO2): the mod count, before the plugin count."],
  "connect.toast":
    ["{plugins} plugins, {cells} cells — {ms} ms",
     "The toast after connecting a setup."],
  /* Round 18bi: the plugin chooser. Robin asked for it so a mod creator can see their own
     work without the rest of a 562-plugin order underneath it. */
  /* Round 18ap: keyed for the language packs — the simplified preview and mesh loading (10_preview.js). */
  "report.mesh_missing":
    ["mesh missing — {path}",
     "The report dialogue: a mesh the engine could not read."],
  "report.not_a_mesh":
    ["the engine sent something that is not a mesh",
     "A grass card's swatch tooltip, and the report: the mesh data was not a mesh."],
  "report.no_geometry":
    ["{path}: no visible geometry (filler or placeholder mesh)",
     "The report dialogue: a mesh with nothing to draw."],
  "report.missing_from_bundle":
    ["missing from the bundle",
     "A grass card's swatch tooltip: the engine's mesh bundle had no entry for it."],
  /* Round 18ap: keyed for the language packs — the asset browser (12_browser.js). */
  "browse.fav_failed":
    ["Could not save that favourite: {err}",
     "Toast when starring an entry could not be saved."],
  "browse.star_on_title":
    ["Starred — kept at the top of every picker of this kind",
     "The star on a picker row: its tooltip while starred."],
  "browse.star_off_title":
    ["Star this, to keep it at the top of every picker of this kind",
     "The star on a picker row: its tooltip while not starred."],
  /* Round 18ap: keyed for the language packs — the undo toasts (22_history.js). */
  /* Round 18ap: keyed for the language packs — units and the toast cue (03_core.js). */
  "toast.show_it":
    ["Show it",
     "A toast's cue line when the caller gives no wording."],
  "unit.mb":
    ["{n} MB",
     "A size in megabytes."],
  "unit.s":
    ["{n} s",
     "A time in seconds."],
  "unit.min_s":
    ["{m} min {s} s",
     "A time in minutes and seconds."],
  /* Round 18ap: keyed for the language packs — leftovers found by the markup walk. */
  "preview.grassfar_cells.one":
    ["{n} cell",
     "Left column, Preview: the grass-distance slider's value."],
  "preview.grassfar_cells.other":
    ["{n} cells",
     "Left column, Preview: the grass-distance slider's value."],
  "preview.grassfar_off":
    ["Off",
     "Left column, Preview: the grass-distance slider's value at zero."],
  /* Round 18aq: the language picker in Settings (19_settings.js). */
  /* Round 18cg: the colour themes, by name. Names rather than descriptions — the row a
     name sits in is painted in that theme's own colours, so what it looks like is shown
     rather than said, and a translator is left with five words instead of five sentences
     about oxide and vellum. They are palettes named after the houses they came from
     (`theming-plan.md`), so a pack will most likely keep them as they are; the keys exist
     because a pack that transliterates its own alphabet should be able to. */
  "settings.theme_wraithguard":
    ["Wraithguard (Default)",
     "Settings, Theme: Wraithguard's own palette, as the toolkit is wearing it."],
  "settings.theme_default":
    ["Gardenfell Green",
     "Settings, Theme: the palette the program ships with — sage on true grey."],
  "settings.theme_velothi":
    ["Of Ash and Blight",
     "Settings, Theme: a palette — ember on ashfall."],
  "settings.theme_redoran":
    ["Redoran Bone",
     "Settings, Theme: a palette — oxide on vellum, the light one."],
  "settings.theme_telvanni":
    ["Telvanni Bio",
     "Settings, Theme: a palette — spore green on aubergine."],
  "settings.theme_hlaalu":
    ["Hlaalu Clay",
     "Settings, Theme: a palette — coin gold on yellow clay."],
  "settings.theme_firmament":
    ["The Firmament",
     "Settings, Theme: a palette — dark greys and starlight, Masser's rust for the warm colours; the quiet one."],
  "settings.theme_unknown":
    ["A theme this version has not got",
     "Settings, Theme: the entry for a theme the profile names and this build has no colours for; its code is shown beside it."],
  "settings.lang_english":
    ["English (built in)",
     "Settings, Language: the picker's first entry — the wording built into the program."],
  "settings.lang_pack_option":
    ["{name} — {translated} of {total} translated",
     "Settings, Language: a pack in the picker, by its own name, with its coverage."],
  "settings.lang_pack_broken":
    ["{name} — could not be read",
     "Settings, Language: a pack file the engine could not read, greyed in the picker."],
  "settings.lang_pack_missing_option":
    ["{code} — not found",
     "Settings, Language: the picker entry for the language the profile asks for when no pack provides it."],
  "settings.lang_note":
    ["Packs are gardenfell.<code>.toml files in {dir} — the languages folder under where your files are kept, so they move with the rest. Drop one there and choose it here; the choice is saved in your profile. Text the program draws as you work catches up as it is redrawn.",
     "Settings, Language: the note under the picker; {dir} is the languages folder."],
  "settings.lang_missing_note":
    ["This profile asks for {file}, which is not in that folder — English until it is.",
     "Settings, Language: the note when the profile's pack is not on this machine."],
  "settings.lang_broken_note":
    ["{file} could not be read: {err}",
     "Settings, Language: the note when the chosen pack file will not parse."],
  "settings.lang_pack_note":
    ["{name} by {author}, made for the cell viewer {madeFor} — {translated} of {total} strings translated; the rest show English.",
     "Settings, Language: the note describing the chosen pack."],
  "settings.lang_orphans_note":
    ["{n} of its keys are not in this version of the cell viewer any more — kept in the file, not used. Refresh this pack to sort them to the end.",
     "Settings, Language: appended when the pack has keys the catalogue lacks."],
  "settings.lang_problems_note.one":
    ["{n} line could not be used:",
     "Settings, Language: the heading over the pack's refused lines (one)."],
  "settings.lang_problems_note.other":
    ["{n} lines could not be used:",
     "Settings, Language: the heading over the pack's refused lines."],
  "settings.lang_problem_line":
    ["line {line}, {key}: {what}",
     "Settings, Language: one refused line of the pack — its line number, key and reason."],
  "settings.lang_problems_toast":
    ["{name}: {n} line(s) could not be used — Settings says which.",
     "Toast when a pack with refused lines is applied."],
  "settings.lang_missing_toast":
    ["This profile asks for the language pack {file}, which is not in {dir}. English until it is.",
     "Toast at start-up (and on switching profile) when the profile names a pack the machine lacks."],
  "settings.lang_switched":
    ["Now speaking {name}. Text the program draws as you work catches up as it is redrawn.",
     "Toast after a language pack is chosen."],
  "settings.lang_switched_english":
    ["Back to English.",
     "Toast after English is chosen."],
  /* Round 18as: Create new language file, and the two Open folder buttons. */
  "settings.lang_new_head":
    ["New language file",
     "Settings, Language: the title of the two questions Create new language file asks."],
  "settings.lang_new_code_body":
    ["The language's code, as in BCP 47 — sv, de, pt-BR, zh-Hans. It names the file: gardenfell.<code>.toml.",
     "Settings, Language: the first question — the code."],
  "settings.lang_new_next":
    ["Next",
     "Settings, Language: the button that takes the code and asks for the name."],
  "settings.lang_new_taken":
    ["A pack with that code is already in the languages folder — choose it in the picker, or pick another code.",
     "Settings, Language: the warning under the code field when a pack for that code exists."],
  "settings.lang_new_name_body":
    ["What the language calls itself — Svenska, Deutsch, Português do Brasil. This is what the picker shows for {file}.",
     "Settings, Language: the second question — the name; {file} is the pack file about to be made."],
  "settings.lang_created":
    ["Created {file} — every string listed with its English above. Fill it in, then choose it here; Refresh this pack brings it up to date after a new version of Gardenfell.",
     "Toast after Create new language file."],
  "settings.lang_create_failed":
    ["Could not create the language file: {err}",
     "Toast when Create new language file failed; {err} is the engine's reason."],
  "settings.open_failed":
    ["Could not open the folder: {err}",
     "Toast when Open folder or Open languages folder could not start the file browser; {err} is the engine's reason."],
  "settings.lang_updated":
    ["Refreshed {file} — {added} new strings added empty, {orphans} no longer used; the old file is kept as {backup}.",
     "Toast after Refresh this pack."],
  "settings.lang_update_failed":
    ["Could not refresh the pack: {err}",
     "Toast when Refresh this pack failed."],
  /* Round 18ar: engine messages worded from the catalogue — the rules validation (validate.rs). */
  /* Round 18ar: engine messages worded from the catalogue — profiles, rules sets, connecting, the preview patch, language packs. */
  "eng.not_mo2_folder":
    ["{path} does not look like an MO2 folder — expected mods\\ and profiles\\ inside it",
     "An engine error (layout.rs, main.rs): connecting MO2 from the wrong folder."],
  "eng.mo2_dir_missing":
    ["ModOrganizer.ini points {key} at {path}, which is not there",
     "An engine error (layout.rs, main.rs): ModOrganizer.ini names a mods or profiles folder that does not exist."],
  "eng.mo2_profiles_unreadable":
    ["cannot read profiles: {err}",
     "An engine error (layout.rs, main.rs): the MO2 profiles folder could not be listed."],
  "eng.mo2_no_profiles":
    ["no profiles found in MO2",
     "An engine error (layout.rs, main.rs): the MO2 folder has no profiles."],
  "eng.mo2_profile_missing":
    ["profile \"{wanted}\" is missing; using \"{used}\"",
     "A connect warning (layout.rs): the remembered MO2 profile is gone."],
  "eng.mo2_no_game_path":
    ["could not find the game's Data Files from ModOrganizer.ini gamePath — vanilla assets and the DLC will be missing",
     "A connect warning (layout.rs)."],
  "eng.mo2_modlist_unreadable":
    ["cannot read {path}\\modlist.txt: {err}",
     "An engine error (layout.rs): the MO2 profile's modlist could not be read."],
  "eng.mo2_mods_missing":
    ["{n} enabled mod(s) listed in modlist.txt are not in mods\\",
     "A connect warning (layout.rs)."],
  "eng.mo2_no_order":
    ["no plugin load order found — looked for [Game Files] in {path}\\Morrowind.ini and in the game folder",
     "An engine error (layout.rs): MO2 connect found no load order."],
  "eng.openmw_no_data":
    ["no usable data= directories in {path}",
     "An engine error (layout.rs): openmw.cfg names no data folder that exists."],
  "eng.openmw_data_missing":
    ["{n} data= director(ies) in openmw.cfg do not exist",
     "A connect warning (layout.rs)."],
  "eng.not_a_folder":
    ["{path} is not a folder",
     "An engine error (layout.rs): the chosen Data Files path is not a folder."],
  "eng.no_meshes_folder":
    ["no Meshes\\ folder here — is this really Data Files?",
     "A connect warning (layout.rs)."],
  "eng.pack_no_language":
    ["no [language] table — a pack starts with [language] and its code",
     "An engine error (lang.rs): a language pack without its header."],
  "eng.pack_no_code":
    ["[language] has no code — \"sv\", \"pt-BR\", \"zh-Hans\"",
     "An engine error (lang.rs): a language pack whose header has no code."],
  "eng.pack_bad_code":
    ["[language] code {code} is not a language tag — letters, digits and hyphens only",
     "An engine error (lang.rs): a language pack with a malformed code. {code} is quoted."],
  "eng.pack_code_en":
    ["en is English itself, the wording built in — a pack needs a code of its own, such as en-GB",
     "An engine error (lang.rs): Create new language file was given the code en."],
  "eng.pack_not_found":
    ["no language pack {file} in {dirs}",
     "An engine error (lang.rs): the profile's language pack is not in the language folders."],
  "eng.pack_not_string":
    ["not a string",
     "A language pack line refused (lang.rs): the value is not a string."],
  "eng.pack_placeholders":
    ["placeholders differ — the English has {english}, this has {pack}",
     "A language pack line refused (lang.rs): its placeholders differ from the English's."],
  "eng.pack_stray_lt":
    ["a stray < — write &lt; for a less-than sign",
     "A language pack line refused (lang.rs): a < that is not a tag."],
  "eng.pack_tag":
    ["uses <{tag}>, which the English does not — a pack may use only the tags its English uses",
     "A language pack line refused (lang.rs): a tag the English does not use."],
  /* Round 18ar: engine messages worded from the catalogue — the desktop shell commands (src-tauri/src/main.rs). */
  "eng.no_install":
    ["no install connected",
     "An engine error (src-tauri/src/main.rs): a command that needs a setup was called before one was connected."],
  "eng.no_install_is":
    ["no install is connected",
     "An engine error (src-tauri/src/main.rs): the same, from the export path."],
  "eng.no_interior":
    ["no interior called {name}",
     "An engine error (src-tauri/src/main.rs): an interior the load order does not have."],
  "eng.pack_exists":
    ["{file} is already there — choose it in Settings, or give the new one another code",
     "An engine error (src-tauri/src/main.rs): Create new language file found the pack already on disk."],
  "eng.pack_missing_file":
    ["no language pack {file}",
     "An engine error (src-tauri/src/main.rs): Refresh this pack found no such file."],
  "eng.open_failed":
    ["could not open {dir} in the file browser: {err}",
     "An engine error (profiles.rs): the system's file browser would not start for Open folder."],
  "eng.not_in_load_order":
    ["not found in the load order: {path}",
     "An engine error (src-tauri/src/main.rs): a mesh or texture path no root has."],
  "eng.could_not_read":
    ["could not read {path}",
     "An engine error (src-tauri/src/main.rs): a file in the load order that would not read."],
  "eng.order_no_plugins":
    ["{file} lists no plugins — expected [Game Files] GameFile0=… or content=… lines",
     "An engine error (src-tauri/src/main.rs): connecting a setup whose load-order file names no plugins."],
};

/* -------------------------------------------------------------------------------------
   The store.

   `T(key)` is what the rest of the page calls, and the answer is whatever the app ships
   saying: the markup for anything with an element of its own, `TX_STRINGS` for the rest.

   **There was a file here.** `gardenfell.text.toml` beside the exe held every string, so
   the wording could be reworded or translated without editing a 380 KB page — the app
   handed the engine a catalogue at start-up and took back what the file said. Robin,
   round 18aa: "Lets remove the gardenfell.text.toml for now, and just have the text in
   the code instead as it were before we moved it out." So the file, the catalogue, the
   `text_sync` command that carried it and the `TX_WAS` list that let a reworded default
   reach somebody who already had a file are all gone; the keys and `T()` stay, which is
   what any of it would be rebuilt on. A stale `gardenfell.text.toml` in a store is inert
   now — nothing reads it.
   ------------------------------------------------------------------------------------- */
/* Round 18ay: a tooltip's words live in the element's `title` - except while the pointer
   rests on it, when the scroll-note layer (`Tip`, 30_tip.js) has parked them in
   `data-tip` so the browser's own box stays away. These two read and write wherever the
   words are just now, so a language switch rewords a tooltip that happens to be up. */
const titleOf=el=>(typeof Tip!=='undefined')? Tip.read(el) : (el.getAttribute('title')||'');
const setTitle=(el,text)=>{ if(typeof Tip!=='undefined') Tip.write(el,text); else el.setAttribute('title',text); };

const Text={
  /** key -> the wording in use. Everything the app ships saying, taken once. */
  said:{},

  /** The built-in wording for one key, wherever it lives.
   *
   *  Read from the snapshot rather than from the page, because half the point of a key is
   *  that a script rewrites the element it names: `markInstall` shortens a connect button
   *  to "MO2" the moment an install is live, and answering "MO2" to
   *  `T('top.connect_mo2')` would leave the button stuck that way when the install
   *  changed. The snapshot is the *shipped* wording, which is what a default is. */
  builtin(key){
    if(Text.said[key]!=null) return Text.said[key];
    if(TX_STRINGS[key]) return TX_STRINGS[key][0];
    /* A key whose element was not there when the snapshot was taken — a row the panel
       builds as it draws it. Read the page for it, but do not keep the answer: an element
       that arrives late is as liable to be rewritten as one that was here all along. */
    const el=document.querySelector('[data-tx="'+key+'"]');
    if(el) return el.innerHTML.replace(/\s+/g,' ').trim();
    const t=document.querySelector('[data-txt="'+key+'"]');
    if(t) return titleOf(t);
    const ph=document.querySelector('[data-txp="'+key+'"]');
    if(ph) return ph.getAttribute('placeholder')||'';
    return '';
  },

  /** Whether a key exists at all — in the pack, the snapshot, the catalogue, or the page.
   *  A pack may carry a plural form the English has not got (`.few` for Czech), which is
   *  why the pack is asked first. */
  has(key){
    return Text.pack[key]!=null || Text.said[key]!=null || !!TX_STRINGS[key] ||
           !!document.querySelector('[data-tx="'+key+'"],[data-txt="'+key+'"],[data-txp="'+key+'"]');
  },

  /* ---- language packs (round 18aq, wording-plan.md phase 2) --------------------------
     `pack` is the translation in use, key -> text, and nothing else: English stays in
     `said`, so an untranslated key shows English and switching back to English is
     emptying the pack. `meta` is the pack's `[language]` header for the picker. */
  pack:{},
  meta:null,

  /** The wording in use for one key: the pack's, or the English. No placeholders filled. */
  current(key){
    const p=Text.pack[key];
    return p!=null? p : Text.builtin(key);
  },

  /* ---- engine messages (round 18ar, wording-plan.md phase 3) -------------------------
     The engine words its messages in English from this same catalogue (`lang::word`,
     `msg!` in Rust; the keys are `eng.*`), so what arrives over the wire is the English
     of a key with its placeholders filled. Under a pack the page matches it back: each
     `eng.*` English is turned into a pattern once, the message is tried against every
     pattern whole and then as the tail of a longer message (the engine wraps some — a
     path and a colon in front), and a match answers `T(key, params)`. Under English
     nothing is touched, so the wire and the screen stay byte for byte what they were. */
  _engPatterns:null,
  engPatterns(){
    if(Text._engPatterns) return Text._engPatterns;
    const out=[];
    for(const key in TX_STRINGS){
      if(!key.startsWith('eng.')) continue;
      const en=TX_STRINGS[key][0];
      // A pattern that is only placeholders and punctuation would match anything.
      if(!/[A-Za-z]{3,}/.test(en.replace(/\{\w+\}/g,''))) continue;
      const names=[];
      const re=en.replace(/[.*+?^${}()|[\]\\]/g,m=>m==='{'||m==='}'? m : '\\'+m)
                 .replace(/\{(\w+)\}/g,(m,n)=>{ names.push(n); return '([\\s\\S]*?)'; });
      out.push({key, names, whole:new RegExp('^'+re+'$'), tail:new RegExp('^([\\s\\S]*?)'+re+'$'),
                literal:en.replace(/\{\w+\}/g,'').length});
    }
    // The most literal pattern first, so "{path}: {err}"-shaped ones never win over a
    // message that names its subject.
    out.sort((a,b)=>b.literal-a.literal);
    return Text._engPatterns=out;
  },

  /** An engine message in the language in use: matched back to its key and translated,
   *  or as it came when nothing matches. English is the identity. */
  word(msg){
    if(Text.lang==='en' || msg==null) return msg;
    const s=String(msg);
    for(const p of Text.engPatterns()){
      let m=p.whole.exec(s), prefix='';
      if(!m){ m=p.tail.exec(s); if(m){ prefix=m[1]; m=m.slice(1); } }
      if(!m) continue;
      const vars={};
      p.names.forEach((n,i)=>{ vars[n]=m[i+1]; });
      return prefix+T(p.key,vars);
    }
    return s;
  },

  /** Puts a pack on the page — or takes it off, with `null`.
   *
   *  The markup first: every keyed element whose wording is still what the *outgoing*
   *  language put there takes the incoming one. An element a script has rewritten since
   *  (the connect button shortened to "MO2", a status line filled in) is left alone — it
   *  catches up on its next render, as does everything rendered from script, and the
   *  picker says so. Then `App.reword()` asks the renderers that are cheap to run again. */
  apply(pack){
    const was={pack:Text.pack, lang:Text.lang};
    const outgoing=key=>{ const p=was.pack[key]; return p!=null? p : Text.builtin(key); };
    Text.pack=(pack && pack.text)||{};
    Text.meta=pack? {code:pack.code, name:pack.name, author:pack.author, madeFor:pack.madeFor,
                     translated:pack.translated, total:pack.total, file:pack.file} : null;
    Text.lang=pack? (pack.code||'en') : 'en';
    const norm=s=>String(s||'').replace(/\s+/g,' ').trim();
    document.querySelectorAll('[data-tx]').forEach(el=>{
      const k=el.dataset.tx;
      if(norm(el.innerHTML)===norm(outgoing(k))) el.innerHTML=Text.current(k);
    });
    document.querySelectorAll('[data-txt]').forEach(el=>{
      const k=el.dataset.txt;
      if(titleOf(el)===outgoing(k)) setTitle(el,Text.current(k));
    });
    document.querySelectorAll('[data-txp]').forEach(el=>{
      const k=el.dataset.txp;
      if((el.getAttribute('placeholder')||'')===outgoing(k)) el.setAttribute('placeholder',Text.current(k));
    });
    if(typeof App==='object' && typeof App.reword==='function') App.reword();
    return was.lang;
  },

  /** The CLDR plural category for a count, in the language in use — the pack's code
   *  (`Text.apply`), English until one is applied: `one` for exactly 1, `other` for
   *  everything else, 0 included. */
  lang:'en',
  plural(n){
    const a=Math.abs(n), i=Math.floor(a), f=a!==i;
    switch(Text.lang.split('-')[0]){
      case 'ru': case 'uk': case 'be':
        if(f) return 'other';
        if(i%10===1 && i%100!==11) return 'one';
        if(i%10>=2 && i%10<=4 && (i%100<12 || i%100>14)) return 'few';
        return 'many';
      case 'pl':
        if(f) return 'other';
        if(i===1) return 'one';
        if(i%10>=2 && i%10<=4 && (i%100<12 || i%100>14)) return 'few';
        return 'many';
      case 'cs': case 'sk':
        if(f) return 'other';
        if(i===1) return 'one';
        if(i>=2 && i<=4) return 'few';
        return 'other';
      case 'fr': case 'pt':
        return (i===0 || i===1) && !f? 'one' : 'other';
      case 'ja': case 'zh': case 'ko': case 'tr': case 'hu': case 'id': case 'vi': case 'th':
        return 'other';
      default:                                   // en, de, sv, nl, es, it, fi, da, nb, el
        return i===1 && !f? 'one' : 'other';
    }
  },

  /** Takes the markup's wording once, before anything has had a chance to rewrite it. */
  start(){
    document.querySelectorAll('[data-tx]').forEach(el=>{
      if(Text.said[el.dataset.tx]==null)
        Text.said[el.dataset.tx]=el.innerHTML.replace(/\s+/g,' ').trim();
    });
    document.querySelectorAll('[data-txt]').forEach(el=>{
      if(Text.said[el.dataset.txt]==null) Text.said[el.dataset.txt]=titleOf(el);
    });
    document.querySelectorAll('[data-txp]').forEach(el=>{
      if(Text.said[el.dataset.txp]==null)
        Text.said[el.dataset.txp]=el.getAttribute('placeholder')||'';
    });
    for(const k in TX_STRINGS)
      if(Text.said[k]==null) Text.said[k]=TX_STRINGS[k][0];
  },
};

/* Taken here, at load, rather than from the start-up sequence: the scripts run after the
   markup, so the page is complete, and `syncNav` writes `T('vp.nav_orbit')` into the very
   button that *holds* that wording. Called later, the snapshot would have read back an
   answer the page had already given — which for one round of round 18aa was an empty
   string, and the navigation button lost its name. */
Text.start();

/** One piece of text, by key. `vars` fills in any `{name}` the wording contains.
 *
 *  Plurals (round 18ap, wording-plan.md decision 2): when `vars.n` is a number and the
 *  catalogue holds `key.one` / `key.few` / `key.many` / `key.other`, the form is chosen
 *  by the language's rule — English has `one` for exactly 1 and `other` for the rest —
 *  and a key given without a suffix serves every count. So a call site says
 *  `T('paint.ready',{n})` and never `' mesh'+(n===1?'':'es')`: the two forms are two
 *  whole strings, which is what a translator needs (Russian and Polish have three). */
function T(key,vars){
  if(vars && typeof vars.n==='number'){
    const form=key+'.'+Text.plural(vars.n);
    if(Text.has(form)) key=form;
    else if(Text.has(key+'.other')) key=key+'.other';
  }
  let s=Text.pack[key];
  if(s==null) s=Text.said[key];
  if(s==null) s=Text.builtin(key);
  /* `{err}` is, by convention, an engine message inside a page sentence — "Could not read
     the setups: {err}" — and the engine speaks catalogue English (round 18ar), so it is
     matched back and translated here, once, for every site that says it. */
  if(vars && vars.err!=null && Text.lang!=='en') vars=Object.assign({},vars,{err:Text.word(vars.err)});
  if(vars) s=String(s).replace(/\{(\w+)\}/g,(m,n)=>(vars[n]==null? m : vars[n]));
  return s;
}
