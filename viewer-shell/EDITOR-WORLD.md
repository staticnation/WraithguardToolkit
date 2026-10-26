# Open work, the Editor and the world (plan)

Steps 1 to 3 of the road in `TODO.md`: close what is open, land editing, the Editor's fine
details. These are not a port; they are features and decisions on the Rust and page side.
Each item says what it needs from you.

Draft from `TODO.md` only. I did not read the viewer's land or Editor code for this one, so
the slices are an ordering to check, not a design.

## Step 1: close what is open (a checklist, no code)

- The mesh geometry read beside the merge, then exteriors (patch 0016 skips the read; check
  the connect log says "meshes' geometry: not read" and the merge time).
- The Aikido leftovers: push merge_to_master and `cargo update -p merge_to_master` in
  `native/`; mark the ones to ignore with the reasons in `TODO.md`.
- Confirm phase 2a in the game (`WG_Clash_A.esp`, `WG_Clash_B.esp`: the placed one is the
  weapon; `player->additem wg_clash 1` gives the weapon).
- The R4 visual checks (`viewer-shell/R4-CHECKLIST.md` sections 2 and 3).
- Send Robin `subset sort\Gardenfell-offering-2026-10-19.zip` (0001-0014).

## Step 3 first: decisions that unblock the rest

Both are yours; each is a one-line change once decided.

- **Phase 2b: a moved reference given a plain record at home.** (a) home, as the Editor
  does (Gardenfell patch 0017, a note for Robin), or (b) the move stands. Run the in-game
  test first (`WG_Move_B.esp` moves a rock, `WG_Move_C.esp` edits its scale in the home
  cell). The pinned case is `move_cases.json`, "moved, then a plain record in its home cell".
- **Deleting versions in the Object Window's search** and **moving a response** (from the
  4.1 notes): the port keeps today's behaviour until you say otherwise.

## Step 3: the Editor's fine details (small, independent)

1. **The two-types warning** in the Use Report and Verify: an id defined as two object
   types, and which one a placed object is.
2. **MO2 and Morrowind.ini orders into ordercore** (`layout::mo2_profile`, `GameFileN=`,
   `[Archives]`). Pin with case files like the other ordercore readers. Also gives 4.2's
   "one reader of the load order" its last two inputs.
3. **`editor_records` unreadable on your setup.** Find the cause from the fallback's
   Messages line (first 120 characters of the answer); needs that line from you.
4. **Phase 2c leftovers**: the viewer's own scene keys, only if one of them shows a case bug.
5. **Shortcuts**: a key-binding page, built to take the controller bindings later (item 5
   of the road).
6. **Cell borders and names** drawn in the render window beside the cell arrows.
7. **The region map**: cells coloured by region, regions painted onto cells, exteriors made
   by dragging. Built from the cell map; the coverage map in `viewcore` is the base.
8. **Path grid tools**: a connection dragged out from a point, points joined by a box, as
   OpenMW-CS has them (a reference for expectations only; it is GPL-3, never for code).

## Step 2: land editing ("The world")

Merged Lands' land paths are the start, and new flat land at the sea is already in. Slices:

1. **The brush engine**: a brush over the height grid with raise, lower, smooth and flatten;
   the strokes are edits in the queue, so undo and the journal work as for any record.
2. **Seams**: an edit at a cell's edge changes the neighbour's edge. Reuse Merge Lands'
   seam rules rather than writing a second set (`wraithguard/land/seams.py`, in Rust via
   `native/land.rs`).
3. **Normals and the derived data** recomputed for what a stroke touched, and the LOD/
   world-map data the viewer reads.
4. **Land textures painted**: the layer rules (`viewcore/src/coverage.rs` has the layers
   for reading), splat painting and the 16x16 texture grid.
5. **Vertex colours painted**.
6. **The page**: three modes as in OpenMW-CS (height, texture, colour), brush size and
   strength, a preview before the write.

Pin each slice's output against a LAND record written by the engine (the build's byte-for-
byte rule), since a bad LAND record can break a game.

## Order

Step 1's checklist now; the two decisions; then step 3's items 1 to 3 (they are small and
unblock others); step 2; then the rest of step 3, which is mostly UI.
