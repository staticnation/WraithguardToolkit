# R4: the release build and a look on a real GPU

The last step of the Gardenfell 18hu rebase (`REBASE-18hu.md`). Everything the sandbox can
run passed on 2026-10-12 (below); what is left needs Windows, the release toolchain and a
GPU. Tick as you go; anything that looks wrong, a screenshot and the cell name is enough.

## 1. Build and gate (PowerShell, repository root)

```powershell
.\tools\setup_dev_env.ps1 -NativeOnly           # the Python module (native/ uses ordercore)
.\.venv-t\Scripts\python -m ruff check .
.\.venv-t\Scripts\python -m black --check .
.\.venv-t\Scripts\python -m mypy
.\.venv-t\Scripts\python -m mypy --platform linux
.\.venv-t\Scripts\python -m pytest
.\.venv-t\Scripts\python tools\make_pot.py --check
.\tools\build_and_check_rust.ps1 -Pages         # release viewer, all crates, clippy, page boots
```

Each must pass; the page boots end with `page errors: 0`. Optional, Robin's whole suite on
our copy of his crates: unzip his `fixture.zip` into `viewer-shell\gardenfell\fixture`
(git ignores it), then `cd viewer-shell\gardenfell; cargo test; cd ..\..` - 503 pass.

Already passed in the sandbox (Linux, Python 3.10, Rust 1.91): ruff, black, mypy (both),
make_pot, pytest (all files, `test_standards` test by test; only the Python-version test
fails there, as always), cargo tests for luacore 23, ordercore 7, native 27, viewcore 29,
gardenfell-g7 2 and the carried-patch tests, the `check` and `check-commands` builds,
clippy `-D warnings` on every crate including the Tauri viewer, the cell and mesh page
boots, the editor suites 12/12, and all 52 shaders through glslangValidator.

## 2. Look at it (Cell Preview after a Sort)

First run (2026-10-12) froze in "Placing objects" on the 1233-folder setup: every archive
was being read whole into memory (patch 0010 fixes it). Retest that same cell first - the
load card's engine and wait times should drop sharply - and if it is still slow, the card's
numbers and the cell name are what to send.

Shadows and light:
- [ ] A sunny exterior with shadows on (Seyda Neen by day): no seams where one cascade
      meets the next, shadows fading out with the fog rather than ending in a line, and no
      shadow laid over a flame, a glow or a spell effect.
- [ ] Preview, Effects, Shadow detail at 2048 and at the install's own: both draw.
- [ ] A vanilla interior: lit as before or better (lights with no mesh now count).
- [ ] The sky at dusk and night: smooth, no colour steps (the dither moved files).

Things that move, or now do not:
- [ ] Where your mods have them: Tamriel Rebuilt's or OAAB's animated water circles
      animate (flipbooks); glowbugs and lamp flares face the camera as you circle
      (billboards); Tamriel Data's bulb mushrooms' glow sits on the cap, not tiled across
      it (glow UV sets).
- [ ] A cell with banners or flags: activators wave, a static on the same kind of mesh
      hangs still (as OpenMW draws it).
- [ ] NPCs on (Preview, Scene) in Balmora: near and far actors animate, and the hover
      highlight's outline sits exactly on a moving NPC (GPU skinning, 18ff, with our far
      actors' pose clock - the part least covered by tests).

The small things:
- [ ] Hold Shift over a door: a tip says where it leads; Shift+click goes through.
- [ ] Preview, Scene, "Pause rendering when still": the viewport stops drawing while the
      camera is still, starts again on the first move; the animations do not jump.
- [ ] Settings frame cap at 30 and 60: smooth, no bursts. Zoom to cursor at the sky still
      zooms where you point.
- [ ] Hovering a list row that is rebuilt (toggle something under it) does not leave the
      ground highlighted.
- [ ] The connect log (Report) lists the new merge counts when a load order has any.

Linux / Steam Deck (the Flatpak), because of the `highp` lesson:
- [ ] Shadows and the water's shore are smooth, not banded.

Speed:
- [ ] Balmora: frame rate and load time against 4.2.1 (HANDOFF: 84 fps). The shadow
      cache and warmed shaders should help; a drop is worth a report line.

## 3. Your decisions (R3-LEDGER.md, "Decisions for Static")

1. Highlight objects defaults to the outline - keep?
2. Texture highlighting always on, its switch gone - keep?
3. Neighbour rings default 0 (Robin: 1, nine cells) - keep?
4. Sunshafts / FXAA / DOF / bloom follow the install (Robin: the first three always on) - keep?
5. Robin's pill bar, side panels, Ctrl+F find bar, window memory - want any?

Answered 2026-10-12 (R3-LEDGER.md): 1-3 kept; 4 on unless the install says otherwise,
switches kept; 5 all four in, plus the Editor toolbar's window docks. To look at:
- [ ] The pills sit over the view without covering the tool strip; each icon resets.
- [ ] Ctrl+F finds a list row; in the Editor it is still the Editor's filter.
- [ ] Drag the left panel wider, restart: same width. Settings, "Reset width".
- [ ] Move and size the window, restart: same place and size (and maximized stays
      maximized); unplug the second screen it was on: opens at the default.
- [ ] Editor toolbar, right-click, "Across the top/bottom of the window": a bar of its
      own, the view resizing to fit, no overlap.

## 4. Then

Commit and push (the file list is in the last session's notes), send Robin
`Gardenfell-offering-2026-10-12.zip`, and the rebase is done; the text delta follows his
string audit.

## Results

- 2026-10-12, Static's 1233-folder setup, after patch 0010: the cell load that froze now
  runs fast, and Wraithguard runs well. The visual checks in section 2 and the decisions
  in section 3 are still open unless ticked above.
