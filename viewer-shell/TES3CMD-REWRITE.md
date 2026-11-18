# tes3cmd without Perl (long-term plan)

Status: **every command a modder or player uses ported** (2026-11-17): `clean`, `header`,
`esp`, `esm`, `common`, `active`, `resetdates`, `overdial`, `recover`, `dump`, `delete`, `diff`,
`modify --replace`, `multipatch`, `fixit`, in `staticnation/tes3cmd-rs`: a library and a
`tes3cmd` command line, held byte for byte to the Perl's own output (standard output, standard
error and every file left behind) on thirty-three cases built by its tests, the released Perl run
beside it there (vendored in `tests/tes3cmd`, run by its CI in two time zones); `T3_REAL_MW`/`T3_REAL_PLUGINS` run the same comparison on real
plugins. Two decisions made doing it: the port reads and writes subrecords as bytes, as the
Perl does, not through greatness7's typed `tes3` (whose re-encoding could change bytes the
comparison requires); and tes3cmd's quirks are kept where its output is the measure (its README
lists them: `--output-dir` a flag, `NAM0` never updated, path grids with capitals never cleaned
as duplicates, a rebuilt cell's header flags lost). Not yet: the master cache (Perl `Storable`
files; the port reads the masters each run). Every option of those commands is ported too (`dump --format`, `--raw`, `--wrap`, savegame
records), but `modify --run` and `--program-file`, which run Perl and stay the Perl's. Activating a patch (`active --on/--off`,
`multipatch`, `fixit`) is where the port does what the Perl means rather than what it does (the
released script dies there).

**Wraithguard runs it built in** (2026-11-17): `toolscore` takes tes3cmd-rs as a git dependency
and the Tools page's tes3cmd runs it inside the app (`toolscore::tes3cmd::Engine::Builtin`,
`tes3cmd::cli::run_in` in the staged folder, no process started), the same staging, backups and
copy-back as before. The external tes3cmd stays as an opt-in fallback ("Use external" on the
page, the path field beside it), and a command running Perl code (`modify --run`,
`--program-file`, a `/.../e` replacement: `needs_perl`) goes to it when one is found. The command
line runs it too: `wraithguard tes3cmd <command> ...`, tes3cmd's own commands and flags.
Compared through the engine against the Perl on the fixture (`header`, `clean`, `dump`,
`delete`, `modify --replace`, `fixit`, `esp`, `common`): the same log and the same files but
for the master cache's line, the Perl's `at FILE line N.` on its errors, and `fixit`, where the
released script dies (`TES3::Util::move` undefined) and the port finishes. Doing it found the
staging hard-linking the masters: a command writing masters (`fixit`'s header sync and date
reset) wrote the person's own Morrowind.esm through the link, with either tes3cmd. The masters
are copied now for the external one, and dropped after a custom command. The built-in one copies
none (2026-11-18): `tes3cmd_rs::cli::run_over` reads them where the setup has them and sends any
write to one to a private copy (tes3cmd-rs's `fsx` overlay), so only the plugins worked on are
staged. `fixit`, `multipatch` and `resetdates` run once over the plugins added, `common` and
`diff` once on two. Per-record results (2026-11-18): every record a run removed, added or
changed, from the plugins before and after (`toolscore::recdiff`, so any tes3cmd's runs), listed
on the page with a filter and undone record by record.

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
- tes3cmd is MIT (its header: "Copyright 2016 by John Moonsugar ... under the MIT License";
  this file said GPL-2.0 until 2026-11-09, which was wrong). So the Perl is ported directly, as
  mlox, Merged Lands, yampt and MWEdit were, and the port carries its copyright notice.

## Where it goes

Decided 2026-11-09: its own repository, `staticnation/tes3cmd-rs` - a library and a `tes3cmd`
command line over greatness7's `tes3`, with tes3cmd's commands and flags, so the MOMW Tools Pack
and existing batch files can use it unchanged, on every platform and without Perl. Wraithguard
takes it as a git dependency (as merge_to_master); the Tools page's tes3cmd section gains an
"in-app" choice per command as each lands, and the launcher stays as the fallback until every
command Wraithguard offers has its own. The command list and order are in `TODO.md`.
