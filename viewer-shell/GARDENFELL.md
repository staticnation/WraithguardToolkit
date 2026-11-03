# The viewer and Gardenfell

The viewer was forked from Robin Hjelte's Gardenfell (MIT) at about round 18ee and rebased on
his round 18hu in October 2026. This file is what still matters from that rebase: the ground
rules, what our page kept and left out of his, the checks still to do on a real setup, and how
to do the next merge. The engine half (his seven crates vendored unmodified, our patches, the
G7 adapter) is in `gardenfell/VENDOR.md`; the history is in `CHANGELOG.md`.

## Ground rules (Static, 2026-10-12)

- Wraithguard is not a grass generator (the agreement with Robin): grass generation, painting,
  the statics setup, ESP export and the profile/rules managers stay out. The Editor deleting a
  single grass mesh is fine; brushes came back only for landscape editing (the Land view).
- Robin (2026-10-12): take what is useful, hold off on anything grass until Gardenfell ships
  as the grass generator. 18hu is not final: expect another rebase after his release, and a
  text delta after his audit of the page's ~2000 strings.
- His crates sit in our tree unmodified; our fixes go to him (`gardenfell/patches/`, and the
  page's in `gardenfell/patches/page/`) and are carried until he takes them.
- `gardenfell-session` cannot be vendored (every module uses the grass crates): its non-grass
  changes are ported into our `commands.rs`. Ask Robin whether he would split a grass-free
  session crate out after his release.

## The page: what we kept and left out of his

Taken: the renderer split (his MGE XE ports in `06_mge.js`, GPL-2.0, so `06_gl.js` is MIT),
flipbooks, billboards, glow UV sets, a static's `.kf` not played (`still`), lights with no mesh,
GPU skinning, the fly boost, his shadow rewrite (`37_shadow.js`), pause when still, the door tip
under Shift, the frame cap's pacing, the hover highlight fix, the map's sea color, warmed
shaders, the connect log's load counts, his license header line on each part.

Kept ours over his: the page layout (accordions, top bar, viewport buttons); Wonders of Water
for any install (his acts on MGE XE only); zoom to cursor as his algorithm behind our Settings
switch; the frame cap as our value with his pacing; the shadow receiver as his plus three of
ours (additive draws take no shadow, the fade is Morrowind's near fog, the sampler is `highp`);
our per-cascade cull of animated batches; `goThroughDoor` as his copy.

Not taken: everything grass (rules, cards, canopies, paint and its map marks, the mesh editor
and its install-wide scan, the placements dialogue, avoidance, the .esp import, export).

Decided (Static, 2026-10-12): Highlight objects defaults to the outline; texture highlighting
always on; neighbor rings default 0; sunshafts, FXAA and DOF on unless the install's post
chain says otherwise (bloom follows the install); his pill bar, `Panels`, `Finder` and window
memory brought in, our UI first (the Finder leaves Ctrl+F to the Editor; the left width saved
in the viewer profile; the window written by our `window_set`), plus the Editor toolbar's top
and bottom docks.

## Still to check on a real setup

The release build runs well on Static's setup (2026-10-12). Not yet looked at (the sandbox
has no GPU: shaders compile there, nothing is seen):

- Shadows: no seams between cascades, fading with the fog, none over flames, glows or spell
  effects; Shadow detail at 2048 and at the install's own; a vanilla interior lit as before or
  better; the sky at dusk and night with no color steps.
- Things that move: Tamriel Rebuilt's or OAAB's animated water (flipbooks), glowbugs and lamp
  flares facing the camera (billboards), Tamriel Data's bulb mushrooms' glow on the cap (glow
  UV sets); banners on activators wave while a static on the same mesh hangs still; NPCs in
  Balmora animate near and far, and the hover outline sits on a moving NPC (GPU skinning with
  our far actors' pose clock - the part least covered by tests).
- Small things: Shift over a door shows where it leads, Shift+click goes through; "Pause
  rendering when still" stops and restarts without the animations jumping; the frame cap at 30
  and 60 is smooth; a rebuilt list row does not leave the ground highlighted.
- The UI brought in: the pills sit over the view without covering the tool strip; Ctrl+F finds
  a list row (the Editor's filter in the Editor); the left panel's width and the window's place
  and size survive a restart (and a second screen unplugged opens at the default); the Editor
  toolbar docked across the top or bottom, the view resizing to fit.
- On the Deck (the `highp` lesson): shadows and the water's shore smooth, not banded.
- Speed: Balmora's frame rate and load time against 4.2.1 (84 fps then).

A screenshot and the cell name is enough for anything that looks wrong.

## The next merge (after Robin's release)

Base: his 18hu `page-source` (`gf_new/.../page-source`); target: his release; ours:
`ui/src`. Per part, `git merge-file --diff3` on copies with line endings normalised (written
back as each was), every conflicting region decided by hand and every clean region read for
grass and for calls into what we left out. Then:

- the three page guards (`tools/page_guard/`): names nothing defines, ids nothing creates,
  string keys the catalogue lacks;
- the engine commands the page calls, against `commands.rs`;
- every shader the page compiles, captured from the boot runs (a stub GL that records
  `shaderSource`) and compiled with glslangValidator; samplers reading depth or float data
  stay `highp`;
- `ui/tests/boot.js` (cell and `WG_MESH_VIEW=1`) and `ui/tests/editor/run.js`.

The engine half follows `gardenfell/VENDOR.md` ("Updating").
