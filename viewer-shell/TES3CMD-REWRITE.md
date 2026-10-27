# tes3cmd without Perl (long-term plan)

Status: **not started.** For now (port step 4.4, slice 8, option a) the Tools page launches the
external tes3cmd (`toolscore::tes3cmd`): `clean` and a custom command on a copy staged with the
plugin's masters, `header` in place, and the master-size sync done in-app. This file is the plan
for option b: the commands Wraithguard uses rewritten over the tes3 crate, so the app no longer
needs tes3cmd, or Perl when only the script is installed.

## Why

- One program: tes3cmd is the last external tool the Tools page needs. The compiled
  `tes3cmd.exe` exists only for Windows; on Linux and macOS it is the Perl script, so Perl is a
  dependency the Flatpak and the macOS build would have to carry.
- The staging dance (a private `Data Files` with hard-linked masters and a `Morrowind.ini`)
  exists only because tes3cmd assumes one flat data folder. Done in-process, the masters come
  from the setup's VFS directly.
- tes3cmd's output is text meant for a terminal; the page wants per-record results it can list,
  filter and undo.

## What to rewrite, in order

1. **`clean`** (the one people actually run). What tes3cmd's clean does, as remembered - **to be
   confirmed against `tes3cmd help clean` and its output before any of it is written**:
   - *duplicate records*: a record identical to the same record in a master (the "ITM", same
     id, same bytes after the header flags) is removed. Cells are compared subrecord by
     subrecord, references by `FRMR`, so a cell keeps only what it changes.
   - *junk cells*: an exterior or interior `CELL` that only repeats its master's header and
     carries no references of its own.
   - *evil GMSTs*: the 72 of `toolscore::lint_tables::EVIL_GMSTS` (the lint already finds them).
   - *cell fixes* tes3cmd applies while cleaning: the `AMBI` fog, the water height of an
     interior that does not use water, the `NAM0` reference count.
   - the options: Wraithguard passes `--replace` and `--hide-backups` (both moot in-process);
     the switches that turn single parts of clean on or off are to be listed from
     `tes3cmd help clean` when the slice starts.
   Output: the plugin rewritten (records the crate does not model copied byte for byte, as
   `toolscore::masters` does), and a list of what was removed and why, per record.
2. **`header`**: `--synchronize` is done (`sync_master_sizes`); `--author`, `--description`,
   `--update-masters`, `--update-record-count` are small edits of the `TES3` record.
3. **`fixcells`**, **`dump --type`**: dump is the Editor's record view already; fixcells is the
   `NAM0`/`DATA` part of clean on its own.
4. Anything else people use as a custom command (`delete`, `modify`, `multipatch`) is not
   rewritten: `multipatch` is delta-plugin's job on OpenMW, and the rest stay with the launcher
   for as long as it ships.

## How to pin it

- A corpus of plugins with known cleaning needs (the MOMW "needs cleaning" list from
  `plugin-order.yml` names real ones; the ones whose licence allows go into
  `tests/fixtures/tes3cmd/`).
- For each: tes3cmd's own cleaned output (run once, on the staged layout, recorded) is the
  answer. The rewrite must produce the same records, byte for byte where the crate writes the
  same bytes, and the same set of removed records otherwise. The case file records tes3cmd's
  version.
- tes3cmd is GPL-2.0-or-later (John Moonsugar). The rewrite reads its behaviour from its
  documentation and from running it, the same method as the rest of the port; any code read
  closely enough to be a derivative keeps its notice.

## Where it goes

`toolscore::clean` beside `toolscore::tes3cmd`, using editcore's plugin reader and writer; the
Tools page's tes3cmd section gains an "in-app" choice per command as each lands, and the
launcher stays as the fallback until every command Wraithguard offers has its own.
