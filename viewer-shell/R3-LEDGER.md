# R3: Gardenfell 18hu's page into the viewer page

The page half of the 18hu rebase (`REBASE-18hu.md`), done 2026-10-12. Base: Robin's 18ef
`page-source/` (`subset sort/grassforge-main`), which our page was forked from. Target: his
18hu `page-source/`. Ours: `viewer-shell/ui/src`. This file is what was decided, part by
part, so the next merge (after Robin's release, base 18hu) only has to look at what he
changes from here.

## Method

- `git merge-file --diff3` per part, on copies with line endings normalised (18 of our 63
  parts are CRLF in the working tree; each was written back as it was). 145 conflicting
  regions in the base merge (197 with the base shown); every one decided by hand, and
  every region that merged cleanly read for grass and for calls into what we left out.
- Three guards over the whole page (`tools/page_guard/`): names nothing defines (eslint
  `no-undef` over the parts in `ORDER`), ids nothing creates, string keys the catalogue
  lacks. Before: 0 / 19 / 8. After: 0 / 18 / 8 - the same kinds as before, no errors.
- The engine commands the page calls, against `commands.rs`: nothing new missing (the
  three his merged code called - his mesh scan and grass cull field - were taken out).
- Every shader the page compiles in the cell and mesh views (52), captured from the boot
  runs and compiled with glslangValidator (GLSL ES): 0 errors. Every sampler that reads
  depth or float data is still `highp` (CLAUDE.md, the Linux lesson).
- Tests: `boot.js` cell (0 page errors, every feature check, R2's key checks),
  `WG_MESH_VIEW=1` (0), `editor/run.js` 12/12; clippy clean on viewcore, check-commands and
  the Tauri viewer crate (which assembles the page from `ORDER`).

## Taken (what changes for a user)

- **Renderer, 06_gl.js and the new 06_mge.js**: Robin moved every MGE XE port out of
  06_gl.js (MIT now) into 06_mge.js (GPL-2.0), interpolated as GLSL strings; our own MGE
  port (the sky's ordered dither, XE Mod Sky.fx) moved there too, so 06_gl.js is MIT as
  its header says. Flipbook textures (18ey: Tamriel Rebuilt's and OAAB's animated water),
  billboards that turn to the eye (18ez), glow maps on their own UV set (18ev: Tamriel
  Data's bulb mushrooms), a static's `.kf` no longer played (18ez, OpenMW's `useAnim()`;
  the page now sends `still`), lights with no mesh (18hl), GPU skinning uniforms (18ff),
  the fly boost (18fh), the animation axis order (`xyzOrder`).
- **Shadows, 37_shadow.js**: his rewrite - up to five cascades sized to the fog's reach,
  a cache of the still casters, a separate pass for what moves (18eg-18gt).
- **Small things**: pause rendering when still (18fh; a switch in the Scene section), the
  door tip while Shift is held over a door (18hf), the frame cap's pacing grid (18fj, on
  our Settings value), the hover highlight that no longer sticks (18fv), animated section
  folds (18er), no browser menu on sliders (18eu), the viewport held while minimised
  (18fh), the map's sea colour from the engine (18hp) and its grid and outline per theme
  (18hh), SSAO and shadow programs warmed at start-up (18gt), the connect log's new load
  counts (18fc/18gf, R2's engine keys), his licence header line on each part, his inline
  NVIDIA FXAA notice, his punctuation in the strings we reworded.

## Kept ours (Wraithguard's version over his)

- Our page layout: the Preview's accordions, our top bar, our viewport buttons; every
  markup conflict.
- Wonders of Water (06_gl.js): ours, a Preview switch for any install. His (18fc) acts on
  MGE XE installs only, and Wraithguard is OpenMW only.
- Zoom to cursor: his algorithm (18fg, better: it anchors on the pivot's plane at the sky
  and holds through the glide), our Settings switch (`opts.zoomToCursor`).
- Frame cap: our Settings value, his pacing.
- Shadow receiver (06_mge.js): his, plus three of ours - additive draws take no shadow
  (MGE), the fade is Morrowind's near fog (MGE's fogMWScalar via `uShadowFog`, not the
  distant fog), and the shadow sampler is `highp`. Offered to Robin
  (`gardenfell/patches/page/`).
- Our per-cascade cull of animated batches, put into his caster loop; our Shadow detail.
- `goThroughDoor`: his copy (18hf); ours in 24_ori.js removed, its callers unchanged.

## Not taken

- **Grass** (the agreement with Robin): rules, cards, canopies, paint and its marks on the
  map (18ff), the mesh editor and its install-wide mesh scan (18ga), the placements
  dialogue (18gv), avoidance and its room census (18gy), the .esp import (18fn), export.
- **His layout features, not grass, left for a decision**: the viewport pill bar
  (18hi-18hk: brightness, grass distance, a room view distance and the neighbour-cells
  select at the top of the viewport), draggable side columns (`Panels`, 18hb), the Ctrl+F
  find bar (`Finder`, 18hf), the window's size and position remembered (18el/18fi; needs
  an engine command we do not have).

## Decisions for Static (taken one way; easy to flip)

1. Highlight objects defaults to the outline (his 18fi) - was fill.
2. Texture highlighting is always on, its switch gone (his 18fv).
3. Neighbour rings default stays 0 (ours); his 18hj loads one ring, nine cells.
4. Sunshafts, FXAA, DOF and bloom still follow the install's post chain (ours); his 18ew
   starts the first three always on.
5. The pill bar, Panels, Finder and window memory above: wanted?

Static's answers (2026-10-12):

1-3. Keep as taken.
4. Sunshafts, FXAA and DOF: on by default, the switches kept; the install's post chain
   still wins where it has one ("follow install, else on"). Bloom follows the install.
5. All four brought in, our UI first:
   - Pill bar (his 18hi-18hk markup, CSS and view-distance code verbatim): brightness and
     grass distance moved out of the Preview accordions into pills; view distance in an
     interior, neighbour rings in an exterior (reset to 0, ours). Our grass slider wiring
     kept (0 = no limit).
   - Panels (18hb) and Finder (18hf) in `09_ui.js`, two changes: `Panels.max()` copes with
     no `#right` column, and the Finder leaves Ctrl+F / Ctrl+Shift+F to the Editor. The
     left width is saved in the viewer profile (`panel_left`); Settings has "Reset width".
   - Window memory (18el/18fi): the page's remember code verbatim, written by our
     `window_set` command to `wraithguard_cellviewer.window.json` beside the viewer
     profile; `restore_window` in `cellviewer/mod.rs` is his, the window built hidden and
     shown after.
   - And (Static) the Editor toolbar's two window docks, top and bottom, as a full-width
     bar (`#edTbWin`); its floating docks stay.

## Text and markup (R3d)

`05_text.js` and `02_body.html` are merged: his rewordings of strings we still have are in
where they merged cleanly, his punctuation on the four we had reworded ourselves. Robin is
still auditing his ~2000 strings, so expect a delta here after his release.

## Not verified (no GPU in the sandbox: the page tests run on a stub WebGL)

- What anything looks like. The shaders compile; whether the shadows, flipbooks,
  billboards, glow UV sets and the sky dither look right is for a real run.
- Skinned actors under his GPU skinning (18ff) with our pose clock for far actors and
  for highlight copies - the code paths run in the tests, the picture does not.
- Performance: his cached shadow casters and warmed shaders should help; not measured.
- A shadow detail of 2048 on a GPU whose largest texture is 8192: five cascades share one
  atlas, so a cascade gets 1638 there (16384, Windows and the Steam Deck, fits 2048).

## The next merge

Base 18hu (`gf_new/.../page-source`), his release as the target, ours as above. Rerun the
three guards, the engine-command check, the shader compile (the boot dump: a stub GL that
records `shaderSource`), and the three suites.
