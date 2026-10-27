#!/usr/bin/env python3
r"""wraithguard_toolkit.py.

Sorts ONLY a subset of plugins (your "custom" mods, e.g. the ones you added
via a umo/momw-customizations.toml file) into an existing openmw.cfg,
using an mlox-format rules database (mlox_base.txt / mlox_user.txt / whatever
`plox` downloads) to decide where they belong.

The existing content= order already in openmw.cfg is treated as FROZEN --
this script never reorders plugins that are already correctly sorted (e.g.
by momw-configurator + plox). It only figures out, for the subset plugins,
where they need to slot in relative to the frozen list and to each other.

By default this tool only PRINTS its plan -- it writes nothing. Pass
--write-cfg to patch openmw.cfg directly, and/or --emit-toml to write a
corrected momw-customizations.toml (the durable fix -- feed it back into
momw-configurator/umo so the order survives future rebuilds, instead of
being overwritten by the next rebuild like a direct cfg patch would be).

After sorting, [Requires], [Conflict], and [Note] rules from the same
rule files are also evaluated (read-only) against the final active plugin
list and printed as warnings -- e.g. "you have A and B active, and they
conflict" or "A requires B, which is missing". These are informational
only: nothing is auto-fixed or blocked because of them, and (like the
sorting rules) they are parsed by a best-effort mlox-logic interpreter,
not the real mlox engine, so treat them as a hint to go check things
yourself rather than ground truth. Pass --no-predicate-warnings to skip
this step entirely.

Also by default, only content= plugins are sorted (via mlox). data=
(folder path) insertions from --customizations are found but NOT acted on
unless you pass --sort-data-paths -- since those aren't ordered by mlox at
all, this is opt-in so you don't get surprise data= reordering from a run
that was only meant to sort plugins.

When --sort-data-paths IS given, an explicit after/before anchor you wrote
in the TOML always wins. For any insert with NO anchor, the folder itself
is scanned (non-recursively) for .esp/.esm/etc files; if it contains a
plugin that's also somewhere in the mlox-sorted content order, the data=
line is anchored next to whichever existing data= path owns the nearest
neighboring plugin in that order -- e.g. a mod folder holding NewMod.esp
gets its data= line placed next to the data= path that holds whatever
mlox decided goes right before/after NewMod.esp in content=. Every step of
that (path doesn't exist, folder has no plugins, plugin isn't part of this
run's sort, ...) is guarded to fall through to the old "no anchor ->
appended at the end" behavior rather than fail the run.

------------------------------------------------------------------------
USAGE
------------------------------------------------------------------------

  # Preview only (default) -- prints the plan, writes nothing
  python3 wraithguard_toolkit.py \
      --cfg "C:\\Games\\OpenMW\\openmw.cfg" \
      --rules ".\\mlox\\mlox_base.txt" ".\\mlox\\mlox_user.txt" \
      --customizations "C:\\Games\\OpenMW\\momw-customizations.toml"

  # Write a corrected customizations TOML (recommended, durable fix)
  python3 wraithguard_toolkit.py \
      --cfg openmw.cfg --rules mlox/mlox_base.txt \
      --customizations momw-customizations.toml \
      --emit-toml momw-customizations.toml

  # Patch openmw.cfg directly instead/also (one-off, gets overwritten on next rebuild)
  python3 wraithguard_toolkit.py \
      --cfg openmw.cfg --rules mlox/mlox_base.txt \
      --customizations momw-customizations.toml --write-cfg

A timestamped backup of openmw.cfg is written next to the original before
--write-cfg makes any change (unless --no-backup is also given).

Rule files passed later on the command line are treated as higher priority
(mirrors mlox's own mlox_user.txt-overrides-mlox_base.txt behaviour) --
so pass mlox_base.txt first, mlox_user.txt last.

------------------------------------------------------------------------
RULE-ENGINE FIDELITY (how close this is to real mlox)
------------------------------------------------------------------------
The rule parsing and matching are ported from mlox itself (and cross-checked
against plox), so several things behave exactly like the real engine:
- Filename matching handles '*' and '?' wildcards AND the <VER> version-number
  token, with the same metacharacter escaping mlox uses.
- [Order] chains bridge over plugins you don't have: [Order] A, B, C with B
  not installed still enforces A before C (mlox keeps a phantom node; we chain
  the surviving neighbours -- same effect on your installed plugins).
- [Requires]/[Conflict]/[Note] warnings understand ALL/ANY/NOT/DESC nesting
  and the [VER]/[SIZE]/[DESC] functions (reading real plugin version/size/
  header description from the cfg's data= folders; conservative fallback when
  those files aren't reachable). [MWSE-LUA] is parsed but treated as N/A under
  OpenMW. [Patch]/[Version] blocks are parsed only well enough to skip cleanly.

Still deliberately different from full mlox (by design):
- SORTING only ever repositions the subset; the existing content= order in
  openmw.cfg is treated as FROZEN and never reordered. This is the whole point
  (don't disturb a curated MOMW list), not a shortcoming.
- [Requires]/[Conflict]/[Note] are read-only: they print warnings, never change
  the order or block anything. Treat warnings as a prompt to go check, not gospel.
- [NearStart]/[NearEnd] become ordering chains among the listed plugins, not a
  hard push to the absolute start/end of the file.
- data= (folder path) insertions are placed by their after/before anchor (mlox
  has no concept of data-path order). An anchor must point at an EXISTING data=
  line; if not found, the path is appended at the end with a warning.

See README.md for the full feature tour (plugin-order.yml integration, the
mods-folder scanner, and the GUI).
"""

# PEP 563: annotations are strings, so a hint may name a type that is
# only imported for type checking, and no annotation costs import time.
from __future__ import annotations

import argparse
import contextlib
import fnmatch
import os
import re
import struct
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final, cast

# ---------------------------------------------------------------------------
# mlox-exact plugin filename matching (ported from mlox's
# ruleParser._filename_to_regex) so this tool's Order/predicate pattern
# matching behaves like the real engine. mlox filenames may contain three
# special tokens:
#     *      -> any run of characters
#     ?      -> any single character
#     <VER>  -> a version number (e.g. matches the "1.2" in "Foo 1.2.esp")
# and a handful of regex metacharacters that can legally appear in real plugin
# names are escaped exactly the way mlox escapes them (only ()+. ). Previously
# this tool used fnmatch, which has no concept of <VER> -- so the ~578 rule
# lines in mlox_base.txt/mlox_user.txt that use <VER> silently failed to match
# any installed plugin, and every ordering edge those rules would have created
# was lost. This restores them.
# ---------------------------------------------------------------------------
# Pattern translation now lives in wraithguard/rules/patterns.py; behaviour is
# pinned by tests/test_differential.py. Only the names this module actually
# calls are imported -- callers import from wraithguard/ themselves (§23).
from wraithguard import _, get_logger, ngettext, ordercore_bridge, setup_logging
from wraithguard.esp import EspError, Header, read_header
from wraithguard.nif.analysis import MeshAnalyser, MeshFinding
from wraithguard.nif.report import Structure, compare as compare_structures

#: Diagnostics about the run (not the user's report) go through here. The
#: report itself stays on print()/stdout -- see wraithguard/logging_setup.py.
_LOG = get_logger(__name__)

# ---------------------------------------------------------------------------
# Imports from wraithguard/.
#
# The split moved the implementation into packages. This module is the engine
# and CLI -- one caller of those packages among several, not a facade in front
# of them. Until 3.0 it also re-exported 36 names it never called, purely so
# `core.<name>` resolved for the GUI and the tests; those callers now import
# from wraithguard/ directly, and the re-exports are gone (CODE_REVIEW.md §23).
#
# So every import below is one this module *uses*. F401 is enforced here again
# rather than suppressed, which is what makes that claim checkable: an unused
# import in this file is now a lint failure, not the house style.
# ---------------------------------------------------------------------------
from wraithguard.tracing import (
    set_trace_file,
    trace,
)

if TYPE_CHECKING:
    from collections.abc import (
        Callable,
        Collection,
        Iterable,
        Iterator,
        Mapping,
        MutableMapping,
        Sequence,
        Set as AbstractSet,
    )
    from contextlib import AbstractContextManager

    from wraithguard.patch.recordcache import RecordCache

# ---------------------------------------------------------------------------
# mlox [VER]/[SIZE]/[DESC] predicate functions (ported from mlox's ruleParser).
# These let [Requires]/[Conflict]/[Note] rules test a plugin's version, file
# size, or header description -- e.g. "[VER < 2.0 SomeMod.esp]". mlox reads the
# actual plugin file for this; we locate it across the cfg's data= directories.
# When the files aren't reachable (e.g. running on a different machine than the
# mods live on), we fall back to mlox's own conservative "no datadir" behaviour
# rather than guessing, so we never invent a warning we can't substantiate.
# ---------------------------------------------------------------------------

# atomic function forms, matched against a single token produced by the tokenizer


#: Defensive cap on how much of a plugin to read for its header: the TES3
#: header record is tiny, so a larger declared size means a corrupt file, and
#: reading a gigabyte to look for masters helps nobody.
_HEADER_READ_CAP = 1 << 20


def _read_header_record(path: str | Path) -> Header | None:
    """Read a plugin's ``TES3`` header via the in-process ESP reader, or ``None``.

    Reads only the first record -- its declared size then that many bytes -- so a
    huge plugin is not slurped to find its masters, and (because nothing past the
    header is parsed) a file whose later records ``tes3conv`` would refuse still
    yields its header. Never raises: any read or parse problem returns ``None``,
    and the byte-level callers fall back to their own tolerant scan.

    Returns:
        The parsed :class:`~wraithguard.esp.records.Header`, or ``None`` for a
        non-TES3 file, a truncated header, or any decode/parse failure.
    """
    try:
        with Path(path).open("rb") as fh:
            prefix = fh.read(16)  # tag + size + padding + flags
            if len(prefix) < 16 or prefix[:4] != b"TES3":
                return None
            (data_size,) = struct.unpack_from("<I", prefix, 4)
            body = fh.read(min(data_size, _HEADER_READ_CAP))
    except (OSError, struct.error):
        return None
    try:
        return read_header(prefix + body)
    except EspError:
        return None


def read_plugin_masters(path: str | Path) -> list[str]:
    """Return the master files a plugin depends on, from its TES3 header.

    These are the ground-truth load-order dependencies: a plugin must load AFTER every
    master it lists. Returns [] for non-TES3 files (.omwscripts) or any read problem.
    Works for .esm/.esp/.omwaddon/.omwgame.

    Reads the header with the in-process ESP reader (correct cp1252 decoding, the
    format's own MAST/DATA framing); a header the strict reader cannot parse falls
    back to the tolerant byte scan so nothing regresses on an odd file.
    """
    header = _read_header_record(path)
    if header is not None:
        return [name for name, _size in header.masters if name.strip()]
    return _read_plugin_masters_scan(path)


def read_plugin_masters_with_sizes(path: str | Path) -> list[tuple[str, int | None]]:
    """Return [(master_name, recorded_size)] pairs from the TES3 header.

    Like :func:`read_plugin_masters`, but paired with sizes --
    each MAST subrecord is (per the TES3 format) immediately followed by a DATA
    subrecord holding the master's file size (8 bytes) at the time the plugin
    was saved. tes3cmd uses the same pairing for its master-sync check.
    recorded_size is None when the DATA subrecord is absent/malformed.

    Read via the in-process ESP header, which pairs each MAST with its DATA; a
    header too malformed for the strict reader (a MAST with no DATA, say) falls
    back to the byte scan, which tolerates it and reports that size as ``None``.
    """
    header = _read_header_record(path)
    if header is not None:
        return [(name, size) for name, size in header.masters if name.strip()]
    return _read_plugin_masters_with_sizes_scan(path)


def _read_plugin_masters_scan(path: str | Path) -> list[str]:
    """Byte-level MAST scan -- the fallback when the ESP header will not parse."""
    try:
        with Path(path).open("rb") as fh:
            if fh.read(4) != b"TES3":
                return []
            data_size = struct.unpack("<I", fh.read(4))[0]
            fh.read(8)  # header1 + flags
            data = fh.read(min(data_size, _HEADER_READ_CAP))  # header is tiny; cap defensively
    except (OSError, struct.error):
        return []
    masters, i = [], 0
    while i + 8 <= len(data):
        tag = data[i : i + 4]
        sz = struct.unpack_from("<I", data, i + 4)[0]
        i += 8
        chunk = data[i : i + sz]
        i += sz
        if tag == b"MAST":
            nm = chunk.split(b"\x00", 1)[0].decode("cp1252", "replace").strip()
            if nm:
                masters.append(nm)
    return masters


def _read_plugin_masters_with_sizes_scan(path: str | Path) -> list[tuple[str, int | None]]:
    """Byte-level MAST/DATA scan -- the tolerant fallback for a malformed header."""
    try:
        with Path(path).open("rb") as fh:
            if fh.read(4) != b"TES3":
                return []
            data_size = struct.unpack("<I", fh.read(4))[0]
            fh.read(8)  # header1 + flags
            data = fh.read(min(data_size, _HEADER_READ_CAP))
    except (OSError, struct.error):
        return []
    out, i = [], 0
    pending = None  # last MAST name waiting for its DATA size
    while i + 8 <= len(data):
        tag = data[i : i + 4]
        sz = struct.unpack_from("<I", data, i + 4)[0]
        i += 8
        chunk = data[i : i + sz]
        i += sz
        if tag == b"MAST":
            if pending is not None:
                out.append((pending, None))
            nm = chunk.split(b"\x00", 1)[0].decode("cp1252", "replace").strip()
            pending = nm or None
        elif tag == b"DATA" and pending is not None:
            size = struct.unpack_from("<Q", chunk, 0)[0] if len(chunk) >= 8 else None
            out.append((pending, size))
            pending = None
    if pending is not None:
        out.append((pending, None))
    return out


def sync_plugin_master_sizes(
    path: str | Path, index: PluginFileIndex, make_backup: bool = True
) -> tuple[list[tuple[str, int | None, int]], list[str], str | None]:
    """Fix the master sizes recorded in a plugin's TES3 header.

    A VFS-aware replacement for ``tes3cmd header --synchronize``:

    fix the master sizes recorded in a plugin's TES3 header (the DATA subrecord after
    each MAST) to match the installed masters.

        Why not tes3cmd: it assumes a single flat 'Data Files' directory. In an
        OpenMW multi-folder layout the masters usually aren't next to the plugin,
        so tes3cmd resolves them to nothing and writes EMPTY sizes into the
        header (observed in the wild -- 'Synchronized ... length: 79837557 --> ').
        This resolves each master across ALL data folders via the index and
        rewrites only the 8-byte size fields; nothing else in the file changes.

        Returns (updated, unresolved, error):
          updated    -- [(master, old_size, new_size)] fields actually rewritten
          unresolved -- masters whose file couldn't be found (left untouched)
          error      -- message if the file isn't a TES3 plugin / can't be read
        A one-time '<name>.masterfix.bak' copy is made before the first write.
    """
    p = Path(path)
    try:
        raw = bytearray(p.read_bytes())
    except OSError as e:
        return [], [], f"can't read: {e}"
    # A TES3 record header is 16 bytes (tag + size + header1 + flags). A file
    # that starts with the magic but is shorter than that is truncated or
    # corrupt -- reject it rather than unpacking past the buffer.
    if len(raw) < 16 or raw[:4] != b"TES3":
        return [], [], "not a TES3 plugin (no TES3 header)"
    (data_size,) = struct.unpack_from("<I", raw, 4)
    end = min(16 + data_size, len(raw))
    i, updated, unresolved = 16, [], []
    pending = None  # master name waiting for its DATA size field
    while i + 8 <= end:
        tag = bytes(raw[i : i + 4])
        (sz,) = struct.unpack_from("<I", raw, i + 4)
        off = i + 8
        if off + sz > end:
            break
        if tag == b"MAST":
            pending = raw[off : off + sz].split(b"\x00", 1)[0].decode("latin-1", "replace").strip()
        elif tag == b"DATA" and pending:
            if sz >= 8:
                mpath = index.find(pending) if index else None
                if mpath is None:
                    unresolved.append(pending)
                else:
                    try:
                        actual = mpath.stat().st_size
                    except OSError:
                        actual = None
                    (old,) = struct.unpack_from("<Q", raw, off)
                    if actual is not None and old != actual:
                        struct.pack_into("<Q", raw, off, actual)
                        updated.append((pending, old, actual))
            pending = None
        i = off + sz
    if updated:
        if make_backup:
            import shutil as _sh

            bak = p.with_name(p.name + ".masterfix.bak")
            if not bak.exists():
                try:
                    _sh.copy2(p, bak)
                except OSError as e:
                    return [], unresolved, f"couldn't write backup ({e}) -- plugin NOT modified"
        try:
            p.write_bytes(bytes(raw))
        except OSError as e:
            return [], unresolved, f"couldn't write plugin: {e}"
    return updated, unresolved, None


def check_missing_masters(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_origins: Mapping[str, str] | None = None,
) -> tuple[list[str], list[str], list[str], int, set[str]]:
    """Verify every active plugin's TES3 header masters against the load order.

    Returns (missing, order_problems, size_notes, checked):
      missing        -- '[MISSING MASTER]' warnings: a required master is not in
                        the load order. Distinguishes 'installed but not
                        enabled' from 'not found in any data folder' (the
                        latter fails hard at game launch).
      order_problems -- '[MASTER ORDER]' warnings: the master is active but
                        loads AFTER its dependent.
      size_notes     -- '[MASTER SIZE]' notes (tes3cmd-style sync check): the
                        installed master's size differs from the size recorded
                        in the plugin's header -- the plugin was made against a
                        different version of that master. Usually benign.
      checked        -- how many plugin files were actually readable/checked
                        (0 means the mod files aren't reachable; nothing to say).
      problem_names  -- the plugin names behind `missing`/`order_problems`,
                        for UI highlighting.
    """
    origins = subset_origins or {}
    native = ordercore_bridge.native("order_check_missing_masters")
    dirs = getattr(index, "dirs", None)
    if native is not None and dirs is not None:
        try:
            n_missing, n_order, n_sizes, n_checked, n_problems = native(
                [str(p) for p in active_order], [str(d) for d in dirs], dict(origins)
            )
        except Exception as exc:
            if not ordercore_bridge.declined(exc):
                raise
        else:
            return list(n_missing), list(n_order), list(n_sizes), n_checked, set(n_problems)
    pos = {p.lower(): i for i, p in enumerate(active_order)}
    missing, order_problems, size_notes = [], [], []
    problem_names = set()
    checked = 0
    from wraithguard.parallel import read_all

    def headers(p: str) -> tuple[Path | None, list[tuple[str, int | None]]]:
        """One plugin's path and master list, read off its header."""
        path = index.find(p) if index else None
        return (path, read_plugin_masters_with_sizes(path)) if path is not None else (None, [])

    # The headers are read side by side; the checks below still go in load order.
    order = list(active_order)
    for p, (path, pairs) in zip(order, read_all(order, headers), strict=True):
        if path is None:
            continue
        if not pairs and not str(p).lower().endswith((".esp", ".esm", ".omwaddon", ".omwgame")):
            continue
        checked += 1
        origin = origins.get(p.lower())
        tag = f" [{origin}]" if origin else ""
        for m, rec_size in pairs:
            ml = m.lower()
            if ml not in pos:
                mpath = index.find(m) if index else None
                if mpath is None:
                    missing.append(
                        f"[MISSING MASTER] '{p}'{tag} requires '{m}' -- NOT FOUND in any data "
                        f"folder. The game will fail to load with this plugin enabled."
                    )
                else:
                    missing.append(
                        f"[MISSING MASTER] '{p}'{tag} requires '{m}' -- installed but not in "
                        f"the load order. Enable/add it (it must load before '{p}')."
                    )
                problem_names.add(p)
                continue
            if pos[ml] > pos[p.lower()]:
                order_problems.append(
                    f"[MASTER ORDER] '{p}'{tag} loads BEFORE its master '{m}' -- "
                    f"'{m}' must come first."
                )
                problem_names.add(p)
            if rec_size is not None:  # 0 counts: a failed tes3cmd sync zeroes these
                mpath = index.find(m) if index else None
                if mpath is not None:
                    try:
                        actual = mpath.stat().st_size
                    except OSError:
                        actual = None
                    if actual is not None and actual != rec_size:
                        hint = (
                            "header records 0 bytes -- likely damaged by a tes3cmd "
                            "sync that couldn't find the master"
                            if rec_size == 0
                            else "made against a different version of the master (usually fine)"
                        )
                        size_notes.append(
                            f"[MASTER SIZE] '{p}'{tag}: header says '{m}' was {rec_size} bytes, "
                            f"installed copy is {actual} -- {hint}. The tes3cmd window's "
                            f"in-app resync fixes this."
                        )
    return missing, order_problems, size_notes, checked, problem_names


# ---------------------------------------------------------------------------
# TES3 record-level conflict detection (a la TES3View / tes3cmd / TESPCD).
# Parses each active plugin's records and reports where two or more plugins
# define/override the SAME record -- the last one in the load order wins. This
# is read-only and diagnostic: it never changes the sort or the cfg. Depth is
# record-level (record type + editor id), which catches the vast majority of
# real conflicts; it does not diff individual fields (that's xEdit territory).
#
# TES3 binary layout (little-endian):
#   record  = type[4] + dataSize[4] + header1[4] + flags[4] + data[dataSize]
#   subrec  = tag[4]  + dataSize[4] + data[dataSize]
# The editor id is the NAME subrecord for most record types; CELL is special
# (interior = its NAME, exterior = its DATA grid coords). A DELE subrecord marks
# a deletion. Strings are cp1252, null-terminated.
# ---------------------------------------------------------------------------

# record types with no meaningful "editor id" to compare on -- skipped
_TES3_SKIP_TYPES = frozenset({"TES3"})


def _tes3_decode(b: bytes) -> str:
    return b.split(b"\x00", 1)[0].decode("cp1252", "replace").strip()


def _tes3_record_key(rectype: str, blob: bytes) -> tuple[str | None, bool]:
    """Return (record_id, deleted) for one record's subrecord blob.

    Yields:
    ------
    (None, deleted) if it has no id worth comparing on.

    """
    name = None
    cell_data = None
    schd = None  # SCPT script header (name in first 32 bytes)
    intv = None  # LAND grid coords
    deleted = False
    i, n = 0, len(blob)
    while i + 8 <= n:
        tag = blob[i : i + 4]
        sz = struct.unpack_from("<I", blob, i + 4)[0]
        data = blob[i + 8 : i + 8 + sz]
        i += 8 + sz
        if tag == b"NAME" and name is None:
            name = data
        elif tag == b"INAM" and rectype == "INFO" and name is None:
            name = data  # dialogue response id
        elif tag == b"DELE":
            deleted = True
        elif tag == b"DATA" and rectype == "CELL" and cell_data is None:
            cell_data = data
        elif tag == b"SCHD" and rectype == "SCPT" and schd is None:
            schd = data  # script: name is the first 32 bytes
        elif tag == b"INTV" and rectype == "LAND" and intv is None:
            intv = data  # landscape: keyed by exterior grid coords
    if rectype == "CELL":
        cname = _tes3_decode(name) if name else ""
        if cell_data is not None and len(cell_data) >= 12:
            flags, gx, gy = struct.unpack_from("<iii", cell_data, 0)
            if flags & 0x01:  # interior
                return (f"Interior: {cname}", deleted)
            return (f"Exterior ({gx}, {gy})" + (f" [{cname}]" if cname else ""), deleted)
        return (f"Interior: {cname}", deleted)
    if rectype == "SCPT" and schd is not None:
        return (_tes3_decode(schd[:32]) or None, deleted)
    if rectype == "LAND" and intv is not None and len(intv) >= 8:
        gx, gy = struct.unpack_from("<ii", intv, 0)
        return (f"Land ({gx}, {gy})", deleted)
    if name is not None:
        rid = _tes3_decode(name)
        return (rid or None, deleted)
    return (None, deleted)


def parse_tes3_records(path: str | Path) -> Iterator[tuple[str, str, bool]]:
    """Yield (record_type, record_id, deleted) for each game record.

    Reads a TES3
    plugin (.esp/.esm/.omwaddon). Best-effort and fully guarded: a truncated or
    non-TES3 file just yields nothing rather than raising.
    """
    try:
        fh = Path(path).open("rb")  # closed by the caller, not a context manager
    except (OSError, PermissionError):
        return
    with fh:
        head = fh.read(16)
        if len(head) < 16 or head[:4] != b"TES3":
            return
        fh.seek(struct.unpack_from("<I", head, 4)[0], 1)  # skip TES3 header record
        while True:
            rh = fh.read(16)
            if len(rh) < 16:
                break
            rectype = rh[:4].decode("ascii", "replace")
            size = struct.unpack_from("<I", rh, 4)[0]
            blob = fh.read(size)
            if len(blob) < size:
                break
            if rectype in _TES3_SKIP_TYPES:
                continue
            if rectype == "LUAL":
                # OpenMW LuaScriptsCfg record (in .omwaddon/.omwgame): one entry
                # per LUAS subrecord (a Lua script path). Surface each as a
                # LuaScript record so it lines up with .omwscripts declarations.
                for sp in _lual_script_paths(blob):
                    yield "LuaScript", sp, False
                continue
            rid, deleted = _tes3_record_key(rectype, blob)
            if rid is not None:
                yield rectype, rid, deleted


def _lual_script_paths(blob: bytes) -> Iterator[str]:
    """Yield the normalized Lua script path from each LUAS subrecord.

    Reads the LUAL
    (LuaScriptsCfg) record.
    """
    i, n = 0, len(blob)
    while i + 8 <= n:
        tag = blob[i : i + 4]
        sz = struct.unpack_from("<I", blob, i + 4)[0]
        data = blob[i + 8 : i + 8 + sz]
        i += 8 + sz
        if tag == b"LUAS":
            p = data.split(b"\x00", 1)[0].decode("cp1252", "replace").strip()
            if p:
                yield p.replace("\\", "/").lower().lstrip("/")


def parse_omwscripts(path: str | Path) -> Iterator[tuple[str, str, bool]]:
    """Yield ('LuaScript', normalized_path, False) for each declaration.

    Reads an
    OpenMW .omwscripts file. Text format (per OpenMW's parseOMWScripts): one
    'TAGS: script/path.lua' per line, '#' comments and blank lines skipped.
    """
    try:
        text = Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        pos = line.find(":")
        if pos == -1:
            continue
        spath = line[pos + 1 :].strip().strip('"').strip("'")
        if not spath.lower().endswith(".lua"):
            continue
        yield "LuaScript", spath.replace("\\", "/").lower().lstrip("/"), False


def parse_plugin_records(path: str | Path) -> Iterator[tuple[str, str, bool]]:
    """Dispatch to the right record reader for this file's extension.

    Namely:

    .omwscripts is OpenMW's text Lua-attach config; everything else
    (.esp/.esm/.omwaddon/.omwgame) is the TES3 binary format.
    """
    if str(path).lower().endswith(".omwscripts"):
        yield from parse_omwscripts(path)
    else:
        yield from parse_tes3_records(path)


# --- optional tes3conv backend (enables field-level diffing) ----------------
# tes3conv (the tes3 ecosystem's plugin<->JSON converter, also used by TES3
# Conflictsolver) gives clean JSON for every record type. With it we can key
# records exactly and, crucially, DIFF individual fields between conflicting
# plugins. Without it, the built-in binary parser still does record-level
# detection -- just no field-level breakdown.


def find_tes3conv(
    explicit: str | None = None, extra_dirs: Sequence[str | None] | None = None
) -> str | None:
    """Locate a tes3conv executable.

    Order: explicit path, $MLOX_TES3CONV, PATH, then alongside this script / any extra
    dirs given. Returns a path or None.
    """
    import shutil

    names = ["tes3conv", "tes3conv.exe"]
    cands = []
    if explicit:
        cands.append(str(explicit))
    env = os.environ.get("MLOX_TES3CONV")
    if env:
        cands.append(env)
    for nm in names:
        found = shutil.which(nm)
        if found:
            cands.append(found)
    search_dirs = [Path(__file__).resolve().parent]
    search_dirs.extend(Path(d) for d in (extra_dirs or []) if d)
    cands.extend(str(d / nm) for d in search_dirs for nm in names)
    for c in cands:
        try:
            if c and Path(c).is_file():
                return c
        except OSError:  # noqa: PERF203
            # Per-candidate isolation is the point: one unreadable path (a
            # dead network share, a permission-denied dir) must not stop the
            # search for the others.
            continue
    return None


def find_tes3cmd(
    explicit: str | None = None, extra_dirs: Sequence[str | None] | None = None
) -> str | None:
    """Locate tes3cmd.

    Prefers the compiled executable (tes3cmd.exe -- what the MOMW Tools Pack distributes
    and what end users will normally have); the pure-perl 'tes3cmd' script is also
    accepted (it then needs a perl on PATH; see tes3cmd_invocation). Order: explicit
    path, $MLOX_TES3CMD, PATH, then alongside this script / any extra dirs given.
    Returns a path or None.
    """
    import shutil

    names = ["tes3cmd.exe", "tes3cmd.bat", "tes3cmd"]  # compiled build first
    cands = []
    if explicit:
        cands.append(str(explicit))
    env = os.environ.get("MLOX_TES3CMD")
    if env:
        cands.append(env)
    for nm in names:
        found = shutil.which(nm)
        if found:
            cands.append(found)
    search_dirs = [Path(__file__).resolve().parent]
    search_dirs.extend(Path(d) for d in (extra_dirs or []) if d)
    cands.extend(str(d / nm) for d in search_dirs for nm in names)
    for c in cands:
        try:
            if c and Path(c).is_file():
                return c
        except OSError:  # noqa: PERF203
            # Per-candidate isolation is the point: one unreadable path (a
            # dead network share, a permission-denied dir) must not stop the
            # search for the others.
            continue
    return None


def tes3cmd_invocation(path: str | Path) -> tuple[list[str] | None, str | None]:
    """Argv prefix to run the given tes3cmd, or (None, why-not).

    The compiled tes3cmd.exe (MOMW Tools Pack) runs directly. If the path is
    the pure-perl script instead, it's run through a perl interpreter from
    PATH -- with a clear error if there isn't one, since end users normally
    have the compiled build and shouldn't need perl.
    """
    import shutil

    p = Path(path)
    if p.suffix.lower() in (".exe", ".bat", ".cmd"):
        return [str(p)], None
    try:
        # Context-managed: the handle was previously left to the garbage
        # collector, which CPython happens to close promptly and other
        # interpreters do not.
        with p.open("rb") as handle:
            head = handle.read(256)
    except OSError as e:
        return None, f"can't read '{p}': {e}"
    if head.startswith(b"#!") or b"perl" in head.lower():
        perl = shutil.which("perl")
        if not perl:
            return None, (
                f"'{p.name}' is the pure-perl tes3cmd but no perl interpreter was found "
                f"on PATH. Point this at the compiled tes3cmd.exe from the MOMW Tools "
                f"Pack instead (or install perl)."
            )
        return [perl, str(p)], None
    return [str(p)], None


def stage_for_tes3cmd(
    staging_root: str | Path,
    plugin_path: str | Path,
    index: PluginFileIndex | None,
    quiet: bool = False,
) -> tuple[Path | None, list[str]]:
    """Build or refresh a minimal vanilla-Morrowind layout for tes3cmd.

    Lets tes3cmd work on
    ONE plugin from an OpenMW multi-folder setup.

    tes3cmd walks up from its cwd until it finds a directory holding BOTH a
    'Morrowind.ini' and a 'Data Files' folder; masters are then resolved
    inside that single Data Files dir. OpenMW's VFS spreads mods over many
    folders, so run against a mod folder directly tes3cmd can't see the
    masters -- clean silently degrades and header --synchronize corrupts.

    This creates:  <staging_root>/Morrowind.ini      minimal [Game Files]
                   <staging_root>/Data Files/        masters + the plugin
    Masters are HARDLINKED when possible (same volume; read-only use, and a
    hardlink shares the original's timestamp) with copy fallback, and reused
    across runs when size+mtime still match -- so the 100MB+ masters aren't
    recopied per plugin. The target plugin is always a fresh private COPY
    (tes3cmd rewrites it; the original is never touched here).

    Returns (staged_plugin_path, missing_masters). A non-empty
    missing_masters means the caller should SKIP cleaning this plugin --
    tes3cmd compares records against the masters, and cleaning without them
    gives wrong results (the classic batch files refuse too).
    """
    import shutil as _sh

    root = Path(staging_root)
    df = root / "Data Files"
    df.mkdir(parents=True, exist_ok=True)
    p = Path(plugin_path)
    masters = read_plugin_masters(p)
    staged_names, missing = [], []

    def _ensure(src: Path) -> Path | None:
        dest = df / src.name
        try:
            if dest.exists():
                s, d = src.stat(), dest.stat()
                if d.st_size == s.st_size and int(d.st_mtime) == int(s.st_mtime):
                    return dest  # cached from a previous run
                dest.unlink()
            try:
                os.link(src, dest)  # same-volume: instant, no disk cost
                return dest
            except OSError:
                pass  # cross-volume etc. -> copy
            _sh.copy2(src, dest)
            return dest
        except OSError as e:
            if not quiet:
                _LOG.warning(
                    _("couldn't stage '%(name)s': %(error)s"),
                    {"name": src.name, "error": e},
                )
            return None

    for m in masters:
        src = index.find(m) if index else None
        if src is None:
            missing.append(m)
            continue
        if _ensure(Path(src)) is not None:
            staged_names.append(Path(src).name)
        else:
            missing.append(m)

    staged_plugin = df / p.name
    if staged_plugin.exists():
        staged_plugin.unlink()
    _sh.copy2(p, staged_plugin)  # always a private copy
    staged_names.append(p.name)

    ini = root / "Morrowind.ini"
    ini.write_text(
        "[Game Files]\n" + "".join(f"GameFile{i}={n}\n" for i, n in enumerate(staged_names)),
        encoding="latin-1",
        errors="replace",
    )
    return staged_plugin, missing


# ---------------------------------------------------------------------------
# lint checks -- ports of the useful tes3lint (mlox project) and missing_pathgrids.pl
# diagnostics, on the plugin binaries so they see the full OpenMW multi-folder VFS. The
# whole lint is Rust now (viewer-shell/toolscore, lint.rs; its tables carry tes3lint's
# MIT notice), shared with the viewer's Tools page.
# ---------------------------------------------------------------------------


def _iter_tes3_records(raw: bytes) -> Iterator[tuple[bytes, bytes]]:
    """Yield (tag, body) for each top-level record.

    Bodies of record types the caller doesn't care about are skipped over cheaply by
    size.
    """
    n, i = len(raw), 0
    while i + 16 <= n:
        tag = bytes(raw[i : i + 4])
        (sz,) = struct.unpack_from("<I", raw, i + 4)
        yield tag, raw[i + 16 : i + 16 + sz]
        i += 16 + sz


def _iter_subrecords(body: bytes) -> Iterator[tuple[bytes, bytes]]:
    n, i = len(body), 0
    while i + 8 <= n:
        tag = bytes(body[i : i + 4])
        (sz,) = struct.unpack_from("<I", body, i + 4)
        yield tag, body[i + 8 : i + 8 + sz]
        i += 8 + sz


def lint_plugins(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_names: Sequence[str] | None = None,
    origins: Mapping[str, str] | None = None,
    progress: Callable[[int, str], None] | None = None,
) -> tuple[list[str], dict[str, Any]]:
    """Run the lint checks over every active plugin.

    Checks:

        [EVLGMST]     evil GMSTs (name AND value match tes3lint's table) -- fixed
                      by cleaning the plugin (tes3cmd clean removes them).
        [FOGBUG]      interior cell, not behave-like-exterior, fog density exactly
                      0.0 -- renders as a black void on some GPUs.
        [NO PATHGRID] an interior cell introduced by the load order has no PGRD
                      record anywhere in it -- NPCs can't pathfind there.
        [HEADER]      custom plugin with a blank author and/or description.
        [EXP-DEP]     custom plugin calling expansion-only script functions without
                      mastering the expansion.
        [TWIN]        custom plugin whose .omwscripts / content twin is not loaded.

    Vanilla masters and merged/multipatch artifacts are skipped. ``origins`` maps
    plugin_lower -> provenance for the warning text. ``progress`` is not called: the
    plugins are read side by side in Rust (``toolscore::lint``).

    Returns:
        ``(warnings, stats)``; stats has ``scanned``, ``unreadable``, ``warnings``,
        ``interior_cells`` and ``pathgrids``.
    """
    del progress
    order = [str(p) for p in active_order]
    paths: list[str | None] = []
    for plugin in order:
        found = index.find(plugin) if index else None
        paths.append(None if found is None else str(found))
    warnings, stats = ordercore_bridge.require("lint_order")(
        order, paths, [str(s) for s in (subset_names or ())], dict(origins or {})
    )
    return list(warnings), dict(stats)


def flatten_dict(d: Mapping[str, Any], parent_key: str = "", sep: str = ".") -> dict[str, Any]:
    """Flatten a nested record dict into dotted keys.

    Lists are kept as whole
    values) -- ported from TES3 Conflictsolver so field comparison matches it.
    """
    items: list[tuple[str, Any]] = []
    for k, v in d.items():
        nk = f"{parent_key}{sep}{k}" if parent_key else str(k)
        if isinstance(v, dict):
            items.extend(flatten_dict(v, nk, sep=sep).items())
        else:
            items.append((nk, v))
    return dict(items)


def _rec_deleted(rec: Mapping[str, Any]) -> bool:
    if not isinstance(rec, dict):
        return False
    flags = rec.get("flags")
    if isinstance(flags, (list, tuple)):
        return any("delet" in str(f).lower() for f in flags)
    if isinstance(flags, str):
        return "delet" in flags.lower()
    if isinstance(flags, int):
        return bool(flags & 0x20)  # TES3 deleted flag
    return bool(rec.get("deleted"))


def _interior_cell_names(records: Iterable[Mapping[str, Any]]) -> set[str]:
    """Collect the lower-cased names of every interior CELL record.

    Path grids don't carry the interior flag themselves -- only their parent
    CELL record does (DATA.flags bit 0x01) -- so anything that needs to tell
    an interior path grid from an unnamed exterior one has to look this up
    first. This is deliberately NOT "is the grid (0, 0)": interior path grids
    do store (0, 0) by convention, but a real exterior cell can legitimately
    sit at world origin too (Ashlands Region), so grid alone is ambiguous and
    the flag is the only signal that isn't.

    Args:
        records: One plugin's tes3conv records.

    Returns:
        Lower-cased interior cell names.
    """
    out: set[str] = set()
    for rec in records:
        if not isinstance(rec, dict):
            continue
        if str(rec.get("type") or "").lower() != "cell":
            continue
        raw_data = rec.get("data")
        data = raw_data if isinstance(raw_data, dict) else {}
        flags = data.get("flags")
        interior = (
            bool(flags & 0x01)
            if isinstance(flags, int)
            else (flags is not None and "INTERIOR" in str(flags).upper())
        )
        if not interior:
            continue
        name = rec.get("id") or rec.get("name")
        if name:
            out.add(str(name).lower())
    return out


def _tes3conv_record_key(
    rec: Mapping[str, Any], interior_cells: AbstractSet[str] | None = None
) -> tuple[str, str] | None:
    """(type, id) for a tes3conv JSON record.

    tes3conv (via the tes3 crate) emits internally-tagged JSON: {"type": "Npc", "id":
    ...}. Most records carry an 'id' (or 'name'); id-less ones -- exterior cells,
    Landscape, path grids -- carry a 'grid' instead, so we key those by their coords
    (which TES3 Conflictsolver's plain 'id or name' misses, collapsing them all
    together). Returns None for the file header / anything with no usable id.

    Args:
        rec: One tes3conv JSON record.
        interior_cells: Lower-cased names of interior cells in this same
            plugin, from :func:`_interior_cell_names`. Used to key a
            cell-scoped record (path grids) by name alone when its cell is
            genuinely interior, vs. by name-plus-coordinates for a named
            exterior cell. Without it, falls back to the old grid-is-zero
            heuristic, which is only ambiguous exactly at world origin.
    """
    if not isinstance(rec, dict):
        return None
    rtype = rec.get("type")
    if not rtype or str(rtype).lower() in ("header", "tes3"):
        return None
    rid = rec.get("id") or rec.get("name")
    if str(rtype) == "StartScript" and rec.get("script"):
        # OpenMW keys a start script by NAME, the script it starts; tes3conv calls
        # NAME `script` and DATA (never read) `id` (patch.records.record_key).
        rid = rec["script"]
    if rid is None or str(rid) == "":
        grid = rec.get("grid")
        if grid is None and isinstance(rec.get("data"), dict):
            grid = rec["data"].get("grid")
        gx, gy = (
            (grid[0], grid[1])
            if isinstance(grid, (list, tuple)) and len(grid) >= 2
            else (None, None)
        )
        cell = rec.get("cell") or rec.get("cell_name")
        if cell is None and isinstance(rec.get("data"), dict):
            cell = rec["data"].get("cell") or rec["data"].get("cell_name")
        if cell:
            # Cell-scoped records (e.g. PathGrid): keying by grid alone
            # collapses every interior's pathgrid across every plugin into
            # one bogus "(0, 0)" conflict, since interiors all store that
            # grid by convention. Key by the CELL name instead for
            # interiors, and by name-plus-coordinates for named exteriors.
            is_interior = (
                str(cell).lower() in interior_cells
                if interior_cells is not None
                else not (gx or gy)  # no flag lookup available: old heuristic
            )
            rid = str(cell) if is_interior else f"{cell} ({gx}, {gy})"
        elif gx is not None:
            rid = f"({gx}, {gy})"
    if rid is None or str(rid) == "":
        return None
    return (str(rtype), str(rid))


def _no_window_kwargs() -> dict[str, Any]:
    """Return subprocess kwargs that suppress the Windows console flash.

    On Windows, when a windowed (GUI / auto-py-to-exe / ``--noconsole``) build
    shells out to a console program like tes3conv -- otherwise you get one popup
    per plugin. No-op elsewhere.

    Delegates to :func:`wraithguard.proc.no_window_kwargs`, the one
    implementation the package modules also use, so the merge and patch services
    and this script cannot drift apart on it.
    """
    from wraithguard.proc import no_window_kwargs

    return no_window_kwargs()


def _keys_and_cells(records: list[Any]) -> tuple[list[list[Any]], list[list[Any]]]:
    """The compact record keys and the cell list of one plugin's records.

    ``(rectype, rid, deleted)`` per record (first wins; an ``.omwaddon``'s Lua scripts
    as ``("LuaScript", path)``), and ``("ext", gx, gy)`` / ``("int", name, None)`` per
    CELL -- what conflict detection and the cell map need, from the records as
    :func:`_tes3conv_record_key` keys them.

    Args:
        records: One plugin's records in the tes3conv schema.

    Returns:
        The key list and the cell list, as lists (the sidecar files' shape).
    """
    # A cheap first pass over the already-in-memory list (no extra I/O):
    # path grids need to know which cells are interior before they can be
    # keyed correctly, and a cell's own record can appear anywhere in the
    # file relative to its path grid's.
    interior_cells = _interior_cell_names(records)

    keys: list[list[Any]] = []
    cells: list[list[Any]] = []
    seen: set[Any] = set()
    for rec in records:
        if not isinstance(rec, dict):
            continue
        # Lua scripts declared by an .omwaddon LuaScriptsCfg (keyless record). tes3 with
        # LUAL support (Greatness7/tes3#7) names the type ScriptConfigList.
        if str(rec.get("type", "")).lower().replace("_", "") in (
            "luascriptscfg",
            "lual",
            "scriptconfiglist",
        ):
            for s in rec.get("scripts") or rec.get("mScripts") or []:
                sp = (
                    s.get("script_path") or s.get("path") or s.get("mScriptPath")
                    if isinstance(s, dict)
                    else (s if isinstance(s, str) else None)
                )
                if sp:
                    lk = ("LuaScript", str(sp).replace("\\", "/").lower().lstrip("/"))
                    if lk not in seen:
                        seen.add(lk)
                        keys.append([lk[0], lk[1], False])
        k = _tes3conv_record_key(rec, interior_cells)
        if not k or k in seen:
            continue
        seen.add(k)
        rtype, rid = k
        keys.append([rtype, rid, _rec_deleted(rec)])
        if str(rtype).lower() == "cell":
            name = str(rec.get("id") or rec.get("name") or rid)
            if name.lower() in interior_cells:
                cells.append(["int", name, None])
            else:
                _raw_data = rec.get("data")
                data = _raw_data if isinstance(_raw_data, dict) else {}
                grid = data.get("grid") or rec.get("grid")
                if isinstance(grid, (list, tuple)) and len(grid) >= 2:
                    cells.append(["ext", int(grid[0]), int(grid[1])])
                else:
                    mm = re.match(r"^\((-?\d+), (-?\d+)\)$", str(rid))
                    if mm:
                        cells.append(["ext", int(mm.group(1)), int(mm.group(2))])
    return keys, cells


class Tes3ConvSession:
    """DISK-BACKED tes3conv wrapper.

    Converts each plugin to a JSON file in a dump folder ONCE, and reads it back per-
    plugin on demand -- it does NOT keep every plugin's records in memory (that was
    multi-GB / OOM on a big list). Only the small map of plugin -> json-file-path is
    held. Peak memory is now bounded by a single plugin's JSON, not the whole load
    order.

        dump_dir: where to write the .json files (a temp dir if None). keep: if True
        (or an explicit dump_dir is given) the files are left in place; otherwise a
        temp dump is removed by cleanup().
    """

    # Bump when the sidecar key/cell extraction changes so stale caches are
    # rebuilt (v2: pathgrids keyed by cell, not the shared "(0,0)" grid;
    # v3: interior determination uses the CELL record's own flag bit instead
    # of a grid==(0,0) heuristic, which was ambiguous -- a real exterior cell
    # can legitimately sit at world origin too).
    _SIDECAR_VER = 3

    #: Which reader produced this session's JSON, for the engine label a scan
    #: prints and records in its stats. Overridden by the native subclass.
    engine_name: str = "tes3conv"

    def __init__(self, exe: str, dump_dir: str | None = None, keep: bool = False) -> None:
        """Open a session backed by ``exe``, spooling JSON to ``dump_dir``."""
        import tempfile
        import threading

        self.exe = exe
        # Guards _json_paths so prime() can convert plugins from several threads
        # at once. Only the dict is locked, never the subprocess -- the whole
        # point is to let the tes3conv processes run in parallel.
        self._json_lock = threading.Lock()
        # keep = leave the dump on disk when cleanup() runs. Location and lifetime
        # are independent: a session can dump to a STABLE folder (so its JSON is
        # reused by later scans in the same run) yet still be cleaned up on close
        # when keep is False.
        self.keep = bool(keep)
        self._temp = dump_dir is None
        self.dump_dir = (
            Path(dump_dir) if dump_dir else Path(tempfile.mkdtemp(prefix="mlox_tes3conv_"))
        )
        try:
            self.dump_dir.mkdir(parents=True, exist_ok=True)
        except OSError:
            pass
        self._json_paths: dict[str, str] = {}  # plugin path(str) -> json path on disk

    def _json_for(self, path: str | Path) -> str | None:
        import subprocess

        key = str(path)
        with self._json_lock:
            jp = self._json_paths.get(key)
        if jp and Path(jp).exists():
            return jp
        out = self.dump_dir / (Path(path).stem + ".json")
        if out.exists():
            if not self._stale(out, path):  # reuse existing JSON -- don't re-run tes3conv
                with self._json_lock:
                    self._json_paths[key] = str(out)
                trace(f"tes3conv: REUSE {out.name}")
                return str(out)
            trace(f"tes3conv: STALE, re-convert {out.name} (plugin newer than json)")
        try:
            trace(f"tes3conv: CONVERT {Path(path).name} -> {out.name}")
            # S603: the argument list is built entirely here -- a resolved
            # executable path plus two paths we constructed. No shell, no user
            # string interpolation, so there is nothing to inject through.
            subprocess.run(  # noqa: S603
                [self.exe, str(path), str(out)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=600,
                check=True,
                **_no_window_kwargs(),
            )
            with self._json_lock:
                self._json_paths[key] = str(out)
            return str(out)
        except (OSError, subprocess.SubprocessError):
            # OSError: tes3conv missing/not executable. SubprocessError covers
            # both CalledProcessError (check=True) and TimeoutExpired.
            return None

    #: Default ceiling on concurrent tes3conv processes. Each worker is a live
    #: converter plus its output JSON, so the cap is what keeps peak memory
    #: bounded; four is plenty to hide the per-process startup and I/O without
    #: stacking several large conversions at once.
    _PRIME_WORKERS: int = 4

    def prime(self, paths: Iterable[str | Path], max_workers: int | None = None) -> int:
        """Convert many plugins to on-disk JSON at once, warming the cache.

        tes3conv runs as a separate process per plugin, so a bounded thread pool
        converts several on different cores at the same time without the GIL
        getting in the way -- the read phase of a merge or a scan spends most of
        its wall-clock here. Each plugin writes its own ``<stem>.json`` (a unique
        name, so parallel writes never collide) and is skipped when a fresh JSON
        already exists, exactly as :meth:`_json_for` does one at a time.

        **Bounded on purpose.** Every worker is a live tes3conv process and its
        output held in memory; an unbounded pool would multiply peak memory by
        the plugin count, which is how the original tool reached tens of GB. The
        cap defaults to ``min(cpu_count, _PRIME_WORKERS)``.

        Args:
            paths: The plugin files to convert. Duplicates are collapsed.
            max_workers: Concurrent conversions, or ``None`` for the default cap.
                Values below 1 are treated as 1 (serial).

        Returns:
            How many of the plugins now have a usable JSON on disk.
        """
        from concurrent.futures import ThreadPoolExecutor

        unique = list(dict.fromkeys(str(p) for p in paths))
        if not unique:
            return 0
        cap = max_workers if max_workers else min((os.cpu_count() or 1), self._PRIME_WORKERS)
        workers = max(1, min(cap, len(unique)))
        if workers == 1:
            return sum(1 for p in unique if self._json_for(p))
        with ThreadPoolExecutor(max_workers=workers) as pool:
            return sum(1 for result in pool.map(self._json_for, unique) if result)

    def _records(self, path: str | Path) -> list[Any]:
        import json

        jp = self._json_for(path)
        if not jp:
            return []
        try:
            with Path(jp).open(encoding="utf-8", errors="replace") as fh:
                data = json.load(fh)
            return data if isinstance(data, list) else []
        except (OSError, ValueError):
            # OSError: unreadable/vanished file. ValueError: JSONDecodeError and
            # UnicodeDecodeError both subclass it.
            return []

    def records(self, path: str | Path) -> list[Any]:
        """Return every record in one plugin, header included.

        The header is the point: it carries the master list, and a record
        patch cannot renumber a cell's references without it. `record_map`
        cannot serve this because the header has no id to key it by.

        Args:
            path: The plugin file.

        Returns:
            The decoded records, or an empty list if it could not be read.
        """
        return self._records(path)

    def records_of_types(self, path: str | Path, types: AbstractSet[str]) -> list[Any]:
        """Return only the records of the given tes3conv types, in file order.

        The tes3conv session has every record in its JSON already and filters
        it; the native one (below) never decodes the others at all.

        Args:
            path: The plugin file.
            types: tes3conv ``type`` names (``"Dialogue"``, ``"Script"``, ...).

        Returns:
            Those records, or an empty list if the plugin could not be read.
        """
        return [r for r in self._records(path) if isinstance(r, dict) and r.get("type") in types]

    def record_map(self, path: str | Path) -> dict[tuple[str, str], Any]:
        """Return {(rectype, rid): record} for one plugin, from cached JSON."""
        # Built fresh each call and NOT cached, so only one plugin's records are
        # ever in memory at a time. self._records() returns an in-memory list
        # (already read once), so iterating it twice -- once for the interior
        # lookup, once to key every record -- costs no extra I/O.
        records = self._records(path)
        interior_cells = _interior_cell_names(records)
        m = {}
        for rec in records:
            k = _tes3conv_record_key(rec, interior_cells)
            if k and k not in m:
                m[k] = rec
        return m

    def record_subset(
        self, path: str | Path, wanted: AbstractSet[tuple[str, str]]
    ) -> dict[tuple[str, str], Any]:
        """Return ``{(rectype, rid): record}`` for just the ``wanted`` keys.

        Like :meth:`record_map`, but *streams* the plugin's JSON and keeps only
        the records asked for, so peak memory is one record plus the kept subset
        rather than the whole decoded file. Decoding a plugin's JSON costs about
        3.5x its size in RAM, so on a 600 MB landmass plugin this is the
        difference between ~2 GB and tens of MB. :func:`batch_record_fields` and
        the on-click :func:`diff_record_fields` both want only a handful of a
        plugin's records, and are the callers that need it.

        Streaming needs the ``ijson`` parser (optional). Without it, this falls
        back to :meth:`record_map` filtered to ``wanted`` -- the same result, at
        the old peak memory. So installing ``ijson`` only changes the memory
        profile, never the records returned.

        Args:
            path: The plugin file.
            wanted: The ``(rectype, rid)`` keys to keep.

        Returns:
            Those of the ``wanted`` records that exist, keyed exactly as
            :func:`_tes3conv_record_key` keys them. Absent keys are simply not
            in the result.
        """
        want = set(wanted)
        if not want:
            return {}

        try:
            import ijson
        except ImportError:
            # No streaming parser: same answer, whole-file peak. Optional dep,
            # so a missing one degrades memory rather than breaking the read.
            return {k: v for k, v in self.record_map(path).items() if k in want}

        jp = self._json_for(path)
        if not jp:
            return {}

        # Cell-scoped, id-less records (path grids) are keyed by their parent
        # cell, and whether that cell is interior decides the key shape.
        # record_map learns this from a full pre-pass; the cells sidecar already
        # holds it for a few KB, so streaming needs no second parse of the file.
        interior = {
            str(name).lower() for kind, name, *_ in self.cells(path) if kind == "int" and name
        }

        out: dict[tuple[str, str], Any] = {}
        remaining = set(want)
        try:
            with Path(jp).open("rb") as fh:
                # use_float=True: decode reals as float and integers as int,
                # exactly as json.load does. Without it ijson yields Decimal,
                # which would compare and repr() differently from every other
                # read path and quietly change what the differ calls a conflict.
                for rec in ijson.items(fh, "item", use_float=True):
                    if not isinstance(rec, dict):
                        continue
                    key = _tes3conv_record_key(rec, interior)
                    if key in remaining:
                        out[key] = rec
                        remaining.discard(key)
                        if not remaining:
                            # Everything asked for is in hand; stop parsing the
                            # rest of the file entirely.
                            break
        except (OSError, ValueError):
            # A truncated or otherwise unstreamable file must never silently
            # return less than record_map would: fall back to the whole read.
            return {k: v for k, v in self.record_map(path).items() if k in want}
        return out

    @staticmethod
    def _stale(json_path: Path, plugin_path: str | Path) -> bool:
        """Report whether a cached JSON predates its plugin.

        It is stale when the plugin was modified after the JSON was written,
        so a changed plugin re-converts. If either mtime can't be read, treat the
        cache as good (reuse) rather than needlessly re-running tes3conv.
        """
        try:
            return json_path.stat().st_mtime < Path(plugin_path).stat().st_mtime
        except OSError:
            return False

    def _load_sidecar(self, side: Path, path: str | Path) -> list[tuple[Any, ...]] | None:
        import json

        if side.exists() and not self._stale(side, path):
            try:
                with Path(side).open(encoding="utf-8", errors="replace") as fh:
                    obj = json.load(fh)
                if isinstance(obj, dict) and obj.get("v") == self._SIDECAR_VER:
                    return [tuple(x) for x in obj.get("d", [])]
                # older/mismatched cache format -> rebuild
            except (OSError, ValueError, TypeError):
                # OSError: unreadable sidecar. ValueError: JSON/decode errors.
                # TypeError: a corrupt cache whose "d" entries aren't iterable,
                # which tuple() would reject. Any of them just means "rebuild".
                pass
        return None

    def _build_sidecars(
        self, path: str | Path
    ) -> tuple[list[tuple[Any, ...]], list[tuple[Any, ...]], list[Any]]:
        """Read a plugin's full JSON once and build every sidecar cache.

        Extracts the compact record-key
        list (conflicts), the cell list (map), and the landscape records (Merged
        Lands), writing all three sidecars. Whichever of record_keys()/cells()/
        landscape_records() is called first pays this single read; the others
        then hit their fresh sidecars -- so Check Conflicts, Cell Map and Merged
        Lands together read each big JSON once per run, not three times.
        """
        import json

        records = self._records(path)  # the single big-JSON read
        # The Landscape and LandscapeTexture records, kept whole: this is all
        # Merged Lands needs (heights, normals, colours, texture indices, world
        # map, and the LTEX id/name records), a few hundred KB of a plugin that
        # may run to hundreds of MB. Written to a '<stem>.land.json' sidecar so a
        # merge reads terrain from the cache instead of re-running the converter.
        land = [
            rec
            for rec in records
            if isinstance(rec, dict) and rec.get("type") in ("Landscape", "LandscapeTexture")
        ]
        keys, cells = _keys_and_cells(records)
        stem = Path(path).stem
        for name, payload in (
            (stem + ".keys.json", keys),
            (stem + ".cells.json", cells),
            (stem + ".land.json", land),
        ):
            try:
                with (self.dump_dir / name).open("w", encoding="utf-8") as fh:
                    json.dump({"v": self._SIDECAR_VER, "d": payload}, fh)
            except OSError:  # noqa: PERF203
                # Per-file isolation: failing to write one sidecar must not
                # lose the others. The cache is an optimisation, never required
                # for correctness, so a write failure is silently tolerated.
                pass
        return [tuple(x) for x in keys], [tuple(x) for x in cells], land

    def record_keys(self, path: str | Path) -> list[tuple[Any, ...]]:
        """Return a compact (rectype, rid, deleted) list for every record.

        For one plugin
        (deduped, first-wins, plus .omwaddon Lua scripts as ('LuaScript', p)) --
        all conflict DETECTION needs, a few hundred KB vs the multi-MB JSON. Served
        from a '<stem>.keys.json' sidecar; rebuilt (with the cells sidecar) only if
        the plugin changed. The on-click field diff still reads the full record.
        """
        cached = self._load_sidecar(self.dump_dir / (Path(path).stem + ".keys.json"), path)
        return cached if cached is not None else self._build_sidecars(path)[0]

    def cells(self, path: str | Path) -> list[tuple[Any, ...]]:
        """Return a compact list of the CELLs a plugin touches.

        Entries are ('ext', gx, gy) / ('int', name, None) for the cells a plugin
        touches -- all the cell map needs. Served from a '<stem>.cells.json'
        sidecar; rebuilt (with the keys sidecar) only if the plugin changed.
        """
        cached = self._load_sidecar(self.dump_dir / (Path(path).stem + ".cells.json"), path)
        return cached if cached is not None else self._build_sidecars(path)[1]

    def landscape_records(self, path: str | Path) -> list[Any]:
        """Return the Landscape and LandscapeTexture records for one plugin.

        The whole records, not the compact keys -- everything Merged Lands needs
        to merge terrain -- served from a '<stem>.land.json' sidecar and rebuilt
        (with the other sidecars) only when the plugin changed. A merge over a
        load order the conflict scanner has already seen therefore reads terrain
        straight from the cache instead of re-running the converter per plugin.
        """
        import json

        side = self.dump_dir / (Path(path).stem + ".land.json")
        if side.exists() and not self._stale(side, path):
            try:
                with side.open(encoding="utf-8", errors="replace") as fh:
                    obj = json.load(fh)
                if isinstance(obj, dict) and obj.get("v") == self._SIDECAR_VER:
                    data = obj.get("d")
                    if isinstance(data, list):
                        return data
            except (OSError, ValueError):
                # Unreadable or corrupt cache -> rebuild it below.
                pass
        return self._build_sidecars(path)[2]

    def dumped_dir(self) -> str:
        """Return the folder the JSON spool was written to."""
        return str(self.dump_dir)

    def cleanup(self) -> None:
        """Remove the dump unless keep is set.

        Honors keep regardless of whether the dump is a temp dir or a stable folder, so
        'don't keep' still cleans up a stable dump on close.
        """
        if not self.keep:
            import shutil

            shutil.rmtree(self.dump_dir, ignore_errors=True)


#: What the native session's keys-and-cells read is kept under in the record cache.
_KEYS_CELLS_TAG = "~KC"


class NativeEspSession(Tes3ConvSession):
    """A :class:`Tes3ConvSession` read in process by the Rust backend, with no JSON.

    ``wraithguard_native.plugin_records`` (``native/src/esp.rs``) parses a plugin
    with greatness7's ``tes3::esp`` and hands the records straight to Python as
    dicts in the tes3conv schema -- the same values ``json.load`` gave for the
    spooled JSON, checked equal across 721 plugins -- without writing or parsing
    any JSON. So nothing is spooled and there are no sidecar files:

    * :meth:`record_keys` and :meth:`cells` parse the plugin once in *light* mode
      (references, landscape data, path grid points and script bodies left out,
      the parts no key needs) and keep the small result in memory, per plugin,
      until the plugin changes on disk.
    * :meth:`record_subset` parses only the record types asked for.
    * :meth:`landscape_records` parses only LAND and LTEX.
    * :meth:`records` and :meth:`record_map` parse the whole plugin, as before.

    Memory stays bounded the way the sidecars bounded it: the parse happens in
    Rust, and only the records a caller asks for become Python objects.

    Record types the crate does not model (OpenMW-only tags) are skipped rather
    than failing the file; a malformed record fails the file, which then reads as
    empty, as a failed conversion always has.
    """

    engine_name = "native"

    def __init__(
        self,
        dump_dir: str | None = None,
        keep: bool = False,
        disk_cache: RecordCache | None = None,
    ) -> None:
        """Open a native session.

        Args:
            dump_dir: Where :func:`dump_tes3conv_json` writes, or ``None`` for a temp
                dir removed on :meth:`cleanup`. Nothing else is written there.
            keep: Leave that folder in place on cleanup even if it is a temp dir.
            disk_cache: Where each plugin's keys and cells are kept between runs, so
                the first scan after launch does not read every plugin again (None:
                they are kept in memory only, as before).
        """
        import threading

        super().__init__(exe="", dump_dir=dump_dir, keep=keep)
        self._disk = disk_cache
        self._memo_lock = threading.Lock()
        #: plugin path -> (its (mtime_ns, size) when read, keys, cells)
        self._memo: dict[
            str, tuple[tuple[int, int], list[tuple[Any, ...]], list[tuple[Any, ...]]]
        ] = {}

    @staticmethod
    def _tags_by_type() -> dict[str, bytes]:
        """The record tag of each tes3conv ``type`` name (``"Npc"`` -> ``b"NPC_"``)."""
        import wraithguard.esp.records  # noqa: F401 - registers every record class
        from wraithguard.esp.record import REGISTRY

        return {cls.__name__: tag for tag, cls in REGISTRY.items()}

    def _parse(
        self, path: str | Path, keep: Iterable[bytes] | None = None, light: bool = False
    ) -> list[Any]:
        """The plugin's records as tes3conv-schema dicts, or ``[]`` if unreadable."""
        import wraithguard_native

        try:
            data = Path(path).read_bytes()
            return wraithguard_native.plugin_records(
                data, list(keep) if keep is not None else None, light
            )
        except (OSError, ValueError):
            # OSError: unreadable/vanished plugin. ValueError: malformed plugin
            # bytes. Either reads as "no records", as a failed conversion did.
            return []

    def _records(self, path: str | Path) -> list[Any]:
        trace(f"native esp: READ {Path(path).name}")
        return self._parse(path)

    def records_of_types(self, path: str | Path, types: AbstractSet[str]) -> list[Any]:
        """Only the records of the given tes3conv types; the rest are never decoded."""
        by_type = self._tags_by_type()
        tags = {by_type[t] for t in types if t in by_type}
        if not tags:
            return []
        return self._parse(path, keep=tags)

    def _keys_cells(self, path: str | Path) -> tuple[list[tuple[Any, ...]], list[tuple[Any, ...]]]:
        """The plugin's record keys and cells, from memory while the file is unchanged."""
        key = str(path)
        try:
            st = Path(path).stat()
            stamp = (st.st_mtime_ns, st.st_size)
        except OSError:
            return [], []
        with self._memo_lock:
            hit = self._memo.get(key)
        if hit is not None and hit[0] == stamp:
            return hit[1], hit[2]
        out = self._kept_keys(path)
        if out is None:
            out = self._native_keys(path)
            if self._disk is not None and (out[0] or out[1]):  # an unreadable plugin is not kept
                self._disk.put(Path(path), _KEYS_CELLS_TAG, [out[0], out[1]])
        with self._memo_lock:
            self._memo[key] = (stamp, out[0], out[1])
        return out

    def _kept_keys(
        self, path: str | Path
    ) -> tuple[list[tuple[Any, ...]], list[tuple[Any, ...]]] | None:
        """The plugin's keys and cells from the disk cache, while the file is as it was."""
        if self._disk is None:
            return None
        kept = self._disk.get(Path(path), _KEYS_CELLS_TAG)
        if kept is None or len(kept) != 2:
            return None
        return list(kept[0]), list(kept[1])

    @staticmethod
    def _native_keys(
        path: str | Path,
    ) -> tuple[list[tuple[Any, ...]], list[tuple[Any, ...]]]:
        """The keys and cells computed in Rust (``plugin_keys``), or empty if unreadable.

        Computed there from the same fields, by the same rules, as
        :func:`_keys_and_cells` - checked identical on 721 plugins - but without
        turning every record of the plugin into Python objects first.
        """
        import wraithguard_native

        try:
            keys, cells = wraithguard_native.plugin_keys(Path(path).read_bytes())
        except (OSError, ValueError):
            return [], []
        return list(keys), list(cells)

    def record_keys(self, path: str | Path) -> list[tuple[Any, ...]]:
        """Return a compact ``(rectype, rid, deleted)`` list for every record."""
        return self._keys_cells(path)[0]

    def cells(self, path: str | Path) -> list[tuple[Any, ...]]:
        """Return ``("ext", gx, gy)`` / ``("int", name, None)`` for the cells it touches."""
        return self._keys_cells(path)[1]

    def record_subset(
        self, path: str | Path, wanted: AbstractSet[tuple[str, str]]
    ) -> dict[tuple[str, str], Any]:
        """Return ``{(rectype, rid): record}`` for just the ``wanted`` keys.

        Only the record types named in ``wanted`` are parsed at all.

        Args:
            path: The plugin file.
            wanted: The ``(rectype, rid)`` keys to keep.

        Returns:
            Those of the ``wanted`` records that exist, keyed as
            :func:`_tes3conv_record_key` keys them.
        """
        want = set(wanted)
        if not want:
            return {}
        by_type = self._tags_by_type()
        tags = {by_type[t] for t, _ in want if t in by_type}
        if not tags:
            return {}
        interior = {
            str(name).lower() for kind, name, *_ in self.cells(path) if kind == "int" and name
        }
        out: dict[tuple[str, str], Any] = {}
        for rec in self._parse(path, keep=tags):
            k = _tes3conv_record_key(rec, interior)
            if k in want and k not in out:
                out[k] = rec
        return out

    def landscape_records(self, path: str | Path) -> list[Any]:
        """Return the Landscape and LandscapeTexture records for one plugin."""
        return self._parse(path, keep=(b"LAND", b"LTEX"))

    def prime(self, paths: Iterable[str | Path], max_workers: int | None = None) -> int:
        """Read many plugins' keys and cells at once, on several threads.

        The Rust parse lets go of the interpreter, so plugins parse in parallel.

        Args:
            paths: The plugin files. Duplicates are collapsed.
            max_workers: Concurrent reads, or ``None`` for the CPU count (up to 8).

        Returns:
            How many of the plugins could be read.
        """
        from concurrent.futures import ThreadPoolExecutor

        unique = list(dict.fromkeys(str(p) for p in paths))
        if not unique:
            return 0
        workers = max(1, min(max_workers or min(os.cpu_count() or 1, 8), len(unique)))

        def one(p: str) -> bool:
            return Path(p).is_file() and bool(self._keys_cells(p)[0])

        with ThreadPoolExecutor(max_workers=workers) as pool:
            return sum(1 for ok in pool.map(one, unique) if ok)


def native_backend() -> bool:
    """Whether the Rust backend (``wraithguard_native``) is installed.

    With it, plugins are read and written in process -- no tes3conv, no JSON.

    Returns:
        ``True`` when the module imports.
    """
    try:
        import wraithguard_native  # noqa: F401
    except ImportError:
        return False
    return True


def open_record_session(
    conv: str | None,
    dump_dir: str | None = None,
    keep: bool = False,
    disk_cache: RecordCache | None = None,
) -> Tes3ConvSession | None:
    """The session the conflict tools read plugins through.

    The Rust backend (:class:`NativeEspSession`) whenever it is installed -- it
    reads in process, with no JSON on disk. ``tes3conv`` (``conv``) only when the
    backend is missing; ``None`` when neither is available.

    Args:
        conv: A tes3conv executable, or ``None``.
        dump_dir: The folder :func:`dump_tes3conv_json` writes to (tes3conv also
            spools there).
        keep: Leave that folder in place on cleanup.
        disk_cache: The editor's record cache, for the native session to keep each
            plugin's keys and cells in between runs (tes3conv has its own sidecars).

    Returns:
        The session, or ``None``.
    """
    if not native_backend():
        return Tes3ConvSession(conv, dump_dir=dump_dir, keep=keep) if conv else None
    return NativeEspSession(dump_dir=dump_dir, keep=keep, disk_cache=disk_cache)


def diff_record_fields(
    session: Tes3ConvSession | None, conflict: Mapping[str, Any], paths: Mapping[str, str]
) -> tuple[list[str], dict[str, dict[str, Any]], set[str]]:
    """Field-level comparison for one conflicting record across the plugins that touch it.

    Returns (ordered_keys, per_plugin, differing_keys): ordered_keys  -- dotted field
    keys, in first-seen order per_plugin    -- {plugin: {key: value}} differing_keys--
    the subset of keys whose value differs between plugins (the actual field-level
    conflicts). Empty if identical. Needs a Tes3ConvSession; returns ([], {}, set())
    without one.
    """
    if session is None:
        return [], {}, set()
    key = (conflict["type"], conflict["id"])
    per = {}
    for p in conflict["plugins"]:
        # Stream out just this one record rather than decoding the whole
        # plugin: the on-click diff of a record a 600 MB landmass defines used
        # to materialise ~2 GB to read one entry (see record_subset).
        rec = session.record_subset(paths.get(p, ""), {key}).get(key) if paths.get(p) else None
        per[p] = flatten_dict(rec) if isinstance(rec, dict) else {}
    ordered, seen = [], set()
    for p in conflict["plugins"]:
        for k in per[p]:
            if k not in seen:
                seen.add(k)
                ordered.append(k)
    differing = set()
    for k in ordered:
        vals = {repr(per[p].get(k)) for p in conflict["plugins"] if k in per[p]}
        present = sum(1 for p in conflict["plugins"] if k in per[p])
        if len(vals) > 1 or present != len(conflict["plugins"]):
            differing.add(k)
    return ordered, per, differing


def batch_record_fields(
    session: Tes3ConvSession | None,
    conflicts: Sequence[Mapping[str, Any]],
    paths: Mapping[str, str],
    report: Callable[[int, int], None] | None = None,
    digest: bool = False,
    lock: AbstractContextManager[Any] | None = None,
) -> dict[tuple[str, str], tuple[list[str], dict[str, dict[str, Any]]]]:
    """Field values for many records, with one JSON parse per plugin.

    :func:`diff_record_fields` answers for one record, and calls
    :meth:`Tes3ConvSession.record_map` once per plugin defining it. That method
    deliberately caches nothing -- holding every plugin's records was multi-GB --
    so each call re-reads and re-parses that plugin's entire JSON.

    For one record on demand that is right. For a batch it is quadratic in the
    worst way: judging 2,000 records that Morrowind.esm defines re-parses its
    183 MB JSON 2,000 times, and one parse is 14 seconds cold. The sidecar
    already saves re-running tes3conv; nothing was saving the parse.

    So this inverts the loops. Every plugin is read once, only the wanted
    records are kept from it, and the parse is thrown away before the next.
    With ``ijson`` installed the read is a streaming one
    (:meth:`Tes3ConvSession.record_subset`), so peak memory is one record plus
    the wanted subset rather than the whole plugin's JSON -- the difference
    between tens of MB and ~2 GB on a big landmass plugin. Without ``ijson`` it
    falls back to a whole-file parse per plugin: same records, old peak.

    Args:
        session: The conversion session, or ``None`` for no field data.
        conflicts: The records to read, as the scanner reports them.
        paths: Plugin name to its file path.
        report: Called with ``(plugins done, plugins total)`` as it goes.
        digest: Keep a hash of each value rather than the value. A caller that
            only compares values for equality -- which is all judging does --
            does not need to hold them, and holding them is what makes a whole
            load order run out of memory: a landscape's height and normal
            fields are tens of kilobytes of base64 each, and there are 50,000
            conflicting records. Hashing bounds it to 16 bytes a field. Use it
            for judging; leave it off when the values will be shown.
        lock: Held around *each plugin read*, not around the whole call. The
            tes3conv session is one process on one pipe, so concurrent readers
            must take turns -- but holding it for a whole batch blocks every
            other reader for minutes, and a caller waiting minutes is an
            application the operating system reports as not responding.
            Ignored with the built-in reader, which reads eight plugins at a
            time instead (:mod:`wraithguard.parallel`).

    Returns:
        ``(type, id)`` to ``(ordered field keys, per-plugin values)`` -- the
        same shape :func:`diff_record_fields` returns, minus the differing set,
        which callers judging with :mod:`wraithguard.patch.status` do not use.
    """
    import hashlib

    # Any in and out: a decoded field is whatever tes3conv produced, and what
    # comes back is either that value unchanged or a hash of it.
    def keep(value: Any) -> Any:  # noqa: ANN401
        if not digest:
            return value
        return hashlib.blake2b(repr(value).encode("utf-8", "replace"), digest_size=16).digest()

    if session is None:
        return {}

    wanted: dict[str, set[tuple[str, str]]] = {}
    for conflict in conflicts:
        key = (conflict["type"], conflict["id"])
        for plugin in conflict["plugins"]:
            wanted.setdefault(plugin, set()).add(key)

    held: dict[str, dict[tuple[str, str], dict[str, Any]]] = {}
    # The lock is for tes3conv's one pipe. The built-in reader is safe to share
    # (its only state is a locked memo), so it neither needs the lock nor waits on it.
    native = getattr(session, "engine_name", "") == "native"
    if native:
        lock = None

    def read_one(item: tuple[str, set[tuple[str, str]]]) -> dict[tuple[str, str], dict[str, Any]]:
        """The wanted records of one plugin, flattened (and hashed when digesting)."""
        plugin, keys = item
        path = paths.get(plugin, "")
        if not path:
            return {}
        with lock if lock is not None else contextlib.nullcontext():
            found = session.record_subset(path, keys)
        return {
            key: {name: keep(value) for name, value in flatten_dict(found[key]).items()}
            for key in keys
            if isinstance(found.get(key), dict)
        }

    items = list(wanted.items())
    if native:
        # The built-in reader parses side by side; eight plugins at a time, so a
        # progress report still moves and memory holds eight subsets, not all of them.
        from wraithguard.parallel import MAX_WORKERS, read_all

        done = 0
        for at in range(0, len(items), MAX_WORKERS):
            chunk = items[at : at + MAX_WORKERS]
            for (plugin, _), got in zip(chunk, read_all(chunk, read_one), strict=True):
                held[plugin] = got
            done += len(chunk)
            if report is not None:
                report(done, len(items))
    else:
        for done, item in enumerate(items, start=1):
            held[item[0]] = read_one(item)
            if report is not None:
                report(done, len(items))

    out: dict[tuple[str, str], tuple[list[str], dict[str, dict[str, Any]]]] = {}
    for conflict in conflicts:
        key = (conflict["type"], conflict["id"])
        plugins = list(conflict["plugins"])
        per = {plugin: held.get(plugin, {}).get(key, {}) for plugin in plugins}
        ordered: list[str] = []
        seen: set[str] = set()
        for plugin in plugins:
            for field in per[plugin]:
                if field not in seen:
                    seen.add(field)
                    ordered.append(field)
        out[key] = (ordered, per)
    return out


def plugin_paths(order: Sequence[str], index: PluginFileIndex) -> dict[str, str]:
    """Resolve each plugin in a load order to its file path.

    The plugin-name to path map the conflict window's field diff, tree view,
    patch builder and plugin view all read. The conflict scan produces it as a
    side effect, but it needs no scan: it is only a last-wins lookup per name
    (see :class:`~wraithguard.plugins.PluginFileIndex`, which resolves a name a
    later ``data=`` folder shadows to that folder). Building it on demand is
    what lets those features open without a Check Conflicts run first -- the
    tes3conv JSON is still generated lazily, one plugin at a time, when a field
    is actually read.

    Args:
        order: Plugin file names, in load order.
        index: The data-folder index to resolve names against.

    Returns:
        Plugin name to its resolved path, omitting any name the index cannot
        find (a plugin the data folders do not hold).
    """
    resolved: dict[str, str] = {}
    for name in order:
        found = index.find(name)
        if found is not None:
            resolved[name] = str(found)
    return resolved


def _scan_touch(
    active_order: Sequence[str],
    index: PluginFileIndex,
    session: Tes3ConvSession | None,
) -> tuple[dict[tuple[str, str], list[tuple[str, bool]]], list[str], int, int, dict[str, str]]:
    """Scan the active load order and tally which plugins touch each record.

    The shared, expensive part of :func:`detect_conflicts` and
    :func:`list_subset_singles`: both need "every record, and which plugins
    touch it" -- they only disagree about which counts are worth keeping (2+
    plugins vs. exactly 1 of your own) -- so reading every plugin happens
    once here rather than once per caller.

    Args:
        active_order: Plugin filenames in load order (winner last).
        index: A PluginFileIndex to locate the files on disk.
        session: An optional Tes3ConvSession; when given, records are read
            from tes3conv JSON, else the built-in binary parser is used.

    Returns:
        ``(touch, unreadable, scanned, rec_count, paths)`` -- ``touch`` maps
        ``(type, id)`` to every ``(plugin, deleted)`` hit, in load order.
    """
    from wraithguard.parallel import read_all

    touch: dict[tuple[str, str], list[tuple[str, bool]]] = {}
    unreadable, scanned, rec_count = [], 0, 0
    paths: dict[str, str] = {}

    # tes3conv is one process on one pipe; only the built-in readers run side by side.
    parallel = session is None or getattr(session, "engine_name", "") == "native"

    def read_keys(plugin: str) -> tuple[Path | None, bool, list[tuple[Any, ...]]]:
        """One plugin's keys, and whether they came from the session."""
        path = index.find(plugin) if index else None
        if path is None:
            return None, False, []
        is_omwscripts = str(path).lower().endswith(".omwscripts")
        if session is not None and not is_omwscripts:
            # tes3conv for TES3 records, plus any Lua scripts declared in an
            # .omwaddon's LuaScriptsCfg (so they line up with .omwscripts).
            # record_keys() is sidecar-cached (compact keys, not full records),
            # so re-scans don't re-read the whole tes3conv dump.
            return path, True, list(session.record_keys(path))
        # built-in engine: handles .omwscripts (text) AND TES3 records incl.
        # .omwaddon LUAL scripts, all via parse_plugin_records.
        return path, False, list(parse_plugin_records(path))

    # Read side by side, folded in load order: the tally is the same as one by one.
    order = list(active_order)
    read = read_all(order, read_keys) if parallel else [read_keys(p) for p in order]
    for plugin, (path, from_session, keys) in zip(order, read, strict=True):
        if path is None:
            unreadable.append(plugin)
            continue
        scanned += 1
        paths[plugin] = str(path)
        seen_here = set()  # collapse a record the same plugin defines twice
        for rectype, rid, deleted in keys:
            key = (rectype, rid)
            if not from_session:
                rec_count += 1  # the built-in count includes a plugin's repeats, as before
            if key in seen_here:
                continue
            seen_here.add(key)
            if from_session:
                rec_count += 1
            touch.setdefault(key, []).append((plugin, deleted))
    return touch, unreadable, scanned, rec_count, paths


def _native_scan(
    active_order: Sequence[str], index: PluginFileIndex, subset_names: Sequence[str], mode: str
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """:func:`detect_conflicts` or :func:`_list_singles` with the native session, in Rust.

    conflictcore reads every plugin (side by side, the interpreter released) and folds the
    keys as :func:`_scan_touch` does; the files are found here, by the caller's index.

    Args:
        active_order: Plugin filenames in load order (winner last).
        index: A PluginFileIndex to locate the files on disk.
        subset_names: Your custom plugins.
        mode: ``"conflicts"``, ``"mine"`` (your singles) or ``"other"``.

    Returns:
        ``(records, stats)`` as the Python gives them.
    """
    order = list(active_order)
    paths = []
    for plugin in order:
        found = index.find(plugin) if index else None
        paths.append(None if found is None else str(found))
    return cast(
        "tuple[list[dict[str, Any]], dict[str, Any]]",
        ordercore_bridge.require("conflict_scan")(order, paths, list(subset_names), mode),
    )


def detect_conflicts(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_names: Sequence[str] | None = None,
    session: Tes3ConvSession | None = None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Scan the active load order for record-level conflicts.

    active_order : plugin filenames in load order (winner last).
    index        : a PluginFileIndex to locate the files on disk.
    subset_names : your custom plugins, so conflicts involving them can be flagged.

    session : an optional Tes3ConvSession. When given, records are read from
    tes3conv JSON (exact ids for every record type, and field-level diffing is
    then possible via diff_record_fields); when None, the built-in binary parser
    is used (record-level only).

    Returns (conflicts, stats). conflicts is a list of dicts sorted with your
    custom-involved ones first:
      {type, id, plugins:[load order], winner, involves_subset, deleted_by:[...]}
    stats = {"scanned", "unreadable":[...], "records", "conflicts",
             "engine": "tes3conv"|"builtin", "paths": {plugin: path}}.
    """
    if getattr(session, "engine_name", "") == "native":
        return _native_scan(active_order, index, subset_names or [], "conflicts")
    subset_lower = {s.lower() for s in (subset_names or [])}
    touch, unreadable, scanned, rec_count, paths = _scan_touch(active_order, index, session)

    conflicts = []
    for (rectype, rid), plugs in touch.items():
        if len(plugs) < 2:
            continue
        names = [p for p, _ in plugs]
        conflicts.append(
            {
                "type": rectype,
                "id": rid,
                "plugins": names,
                "winner": names[-1],
                "involves_subset": any(p.lower() in subset_lower for p in names),
                "deleted_by": [p for p, d in plugs if d],
            }
        )
    conflicts.sort(key=lambda c: (not c["involves_subset"], c["type"], str(c["id"]).lower()))
    stats: dict[str, Any] = {
        "scanned": scanned,
        "unreadable": unreadable,
        "records": rec_count,
        "conflicts": len(conflicts),
        "engine": session.engine_name if session is not None else "builtin",
        "paths": paths,
    }
    return conflicts, stats


def _list_singles(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_names: Sequence[str],
    session: Tes3ConvSession | None,
    *,
    want_subset: bool,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Shared core of :func:`list_subset_singles` and :func:`list_other_singles`.

    Both want the same thing -- records with exactly one contributor -- and
    only disagree about which side of ``subset_names`` that contributor has
    to fall on, so the filter condition is the one parameter that differs.

    Args:
        active_order: Plugin filenames in load order (winner last).
        index: A PluginFileIndex to locate the files on disk.
        subset_names: Your custom plugins.
        session: An optional Tes3ConvSession; same meaning as in
            :func:`detect_conflicts`.
        want_subset: ``True`` keeps a single-contributor record when that
            lone plugin IS one of ``subset_names`` (your mods); ``False``
            keeps the complement -- a momw dependency, the game's own
            masters, anything active that isn't yours.

    Returns:
        ``(records, stats)``, matching :func:`detect_conflicts`'s shape:
        each record is ``{type, id, plugins: [the one plugin], winner,
        involves_subset, deleted_by}``, sorted by type then id. ``stats``
        carries a ``"singles"`` count in place of ``"conflicts"``.
    """
    if getattr(session, "engine_name", "") == "native":
        return _native_scan(active_order, index, subset_names, "mine" if want_subset else "other")
    subset_lower = {s.lower() for s in subset_names}
    touch, unreadable, scanned, rec_count, paths = _scan_touch(active_order, index, session)

    records = []
    for (rectype, rid), plugs in touch.items():
        if len(plugs) != 1:
            continue
        plugin, deleted = plugs[0]
        is_subset = plugin.lower() in subset_lower
        if is_subset != want_subset:
            continue
        records.append(
            {
                "type": rectype,
                "id": rid,
                "plugins": [plugin],
                "winner": plugin,
                "involves_subset": is_subset,
                "deleted_by": [plugin] if deleted else [],
            }
        )
    records.sort(key=lambda c: (c["type"], str(c["id"]).lower()))
    stats: dict[str, Any] = {
        "scanned": scanned,
        "unreadable": unreadable,
        "records": rec_count,
        "singles": len(records),
        "engine": session.engine_name if session is not None else "builtin",
        "paths": paths,
    }
    return records, stats


def list_subset_singles(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_names: Sequence[str],
    session: Tes3ConvSession | None = None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """List records that only your own mods touch -- no conflict, just yours.

    A sister process to :func:`detect_conflicts`, not a variant of it: a
    record with exactly one contributor isn't a conflict, and folding it into
    the conflict list would misrepresent both the conflict count and the
    conflict map/table, which are specifically about collisions between
    plugins. This scans the same load order -- paying for the read only once,
    via :func:`_scan_touch` -- but keeps the single-plugin entries
    :func:`detect_conflicts` discards, filtered to ones where that lone
    plugin is one of ``subset_names``.

    The point is parity, not a new UI: the shape matches
    :func:`detect_conflicts`'s output exactly (a "conflict" with one plugin
    is just a record with one column), so the same field-diff panel and the
    same graph/difference/3D buttons already work on it -- "Show
    difference..." simply won't offer itself, correctly, since there's
    nothing to diff against.

    Args:
        active_order: Plugin filenames in load order (winner last).
        index: A PluginFileIndex to locate the files on disk.
        subset_names: Your custom plugins. Unlike ``detect_conflicts``, this
            is meant to be non-empty here -- there is no meaningful "single
            plugin, not mine" entry for this function to return.
        session: An optional Tes3ConvSession; same meaning as in
            :func:`detect_conflicts`.

    Returns:
        ``(records, stats)`` -- see :func:`_list_singles`. ``involves_subset``
        is always ``True`` here.
    """
    return _list_singles(active_order, index, subset_names, session, want_subset=True)


def list_other_singles(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_names: Sequence[str],
    session: Tes3ConvSession | None = None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """List records that only ONE non-custom plugin touches.

    The complement of :func:`list_subset_singles`: everything active that
    isn't yours and isn't conflicting with anything either -- a momw-listed
    dependency's own untouched records, Morrowind.esm/Tribunal.esm/
    Bloodmoon.esm's own base content, any other plugin you didn't add as a
    custom. Useful for pulling a record up in the field-diff/visualisation
    views to just look at it, with nothing to compare it against.

    This can be a much longer list than :func:`list_subset_singles` -- the
    base masters alone define enormous numbers of records -- which is exactly
    why it's a separate, deliberately-not-default function: callers gate it
    behind its own opt-in control rather than fetching it automatically
    alongside a conflict scan.

    Args:
        active_order: Plugin filenames in load order (winner last).
        index: A PluginFileIndex to locate the files on disk.
        subset_names: Your custom plugins, EXCLUDED here. Can be empty --
            then this returns every plugin's single-contributor records.
        session: An optional Tes3ConvSession; same meaning as in
            :func:`detect_conflicts`.

    Returns:
        ``(records, stats)`` -- see :func:`_list_singles`. ``involves_subset``
        is always ``False`` here.
    """
    return _list_singles(active_order, index, subset_names, session, want_subset=False)


def format_conflict_report(
    conflicts: Sequence[Mapping[str, Any]],
    stats: Mapping[str, Any],
    subset_only: bool = False,
    limit: int | None = None,
) -> str:
    """Render conflicts as a readable text report.

    subset_only shows just the ones that involve your custom mods; limit caps how many
    are listed.
    """
    return str(
        ordercore_bridge.require("conflict_report")(
            list(conflicts), dict(stats), subset_only, limit or 0
        )
    )


def write_conflict_csv(path: str | Path, conflicts: Sequence[Mapping[str, Any]]) -> None:
    """Write the full conflict list to a CSV.

    Columns are (type, id, winner, involves_custom,
    deleted_by, plugins-in-load-order).
    """
    text = ordercore_bridge.require("conflict_csv")(list(conflicts))
    with Path(path).open("w", newline="", encoding="utf-8") as fh:
        fh.write(text)


def filter_plugins(
    active_order: Sequence[str], patterns: Sequence[str] | None
) -> tuple[list[str], list[str]]:
    """Return (kept, excluded) after filtering plugin names by glob.

    Matching is case-insensitive
    patterns (fnmatch); a pattern with no glob chars also matches as a substring.
    Lets you drop 'touches-everything' mods (light fixes, groundcover/grass
    generators, delta/merged patches) from a conflict/cell scan.
    """
    pats = [p.strip().lower() for p in (patterns or []) if p and p.strip()]
    if not pats:
        return list(active_order), []
    kept: list[str] = []
    excl: list[str] = []
    for name in active_order:
        low = name.lower()
        hit = any(
            fnmatch.fnmatch(low, p) or (("*" not in p and "?" not in p) and p in low) for p in pats
        )
        (excl if hit else kept).append(name)
    return kept, excl


def _json_value(value: object) -> object:
    """What ``json.dumps`` cannot write, as tes3conv writes it.

    The native reader hands the packed number arrays (landscape grids, script
    bytecode and variables, path grid connections) over as raw bytes; tes3conv's
    JSON holds them as base64 of their zstd compression, so a dump does too.

    Args:
        value: A value ``json.dumps`` refused.

    Returns:
        The base64 text for bytes, else ``str(value)``.
    """
    if isinstance(value, bytes | bytearray):
        from wraithguard.esp.json import _b64zstd

        return _b64zstd(bytes(value))
    return str(value)


def dump_tes3conv_json(
    session: Tes3ConvSession | None,
    plugins: Sequence[str],
    paths: Mapping[str, str],
    outdir: str | Path,
) -> int:
    """Write each plugin's tes3conv JSON to a folder.

    Read back from the session's on-disk
    spool) to outdir/<plugin>.json. Returns the number written. Creates outdir if
    needed. Needs a Tes3ConvSession.
    """
    import json

    if session is None:
        return 0
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    # Convert everything in parallel first: prime() runs the tes3conv processes
    # concurrently (bounded), so the per-plugin loop below only reads fresh JSON
    # instead of waiting on one conversion at a time.
    session.prime([path for p in plugins if (path := paths.get(p))])

    def dump_one(p: str) -> int:
        """Write one plugin's JSON; 1 when written."""
        path = paths.get(p)
        if not path:
            return 0
        try:
            recs = session._records(path)
            (outdir / (Path(p).stem + ".json")).write_text(
                json.dumps(recs, indent=2, ensure_ascii=False, default=_json_value),
                encoding="utf-8",
            )
            return 1
        except OSError:
            return 0

    if getattr(session, "engine_name", "") == "native":
        # The built-in reader is safe to share: several plugins read and written at once.
        from wraithguard.parallel import read_all

        return sum(read_all(plugins, dump_one, cap=4))
    return sum(dump_one(p) for p in plugins)


# ---------------------------------------------------------------------------
# data-path resource (VFS) conflicts: the same loose file (relative path) living
# in two or more data= folders. In OpenMW's VFS the LATER data= folder wins, so
# these overlaps are decided by data-path order -- reorder the data panel to
# change the winner. This is what MO2's "Data" conflicts show for a mod list.
# ---------------------------------------------------------------------------


def detect_resource_conflicts(
    data_dirs: Sequence[str],
    subset_dirs: Sequence[str] | None = None,
    exclude_exts: Collection[str] | None = None,
    compare_contents: bool = True,
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    """Detect loose-file (VFS) conflicts across the data folders.

    A path present in two or more data folders is a *candidate*; whether it is
    a conflict anyone has to think about depends on whether the bytes differ.
    Mods routinely re-ship assets they did not change -- an unedited vanilla
    texture, an asset a patch simply re-includes -- and reporting those beside
    genuine overrides is how a list of thousands stops being read at all. Each
    entry therefore carries ``identical``, and the differing ones sort first.

    Args:
        data_dirs: The ``data=`` folders in load order, winner last.
        subset_dirs: The folders belonging to the user's own mods, used to flag
            conflicts that involve them.
        exclude_exts: Extensions to skip entirely.
        compare_contents: Whether to compare bytes. Left on by default because
            the size check makes it cheap, but exposed so a scan over a slow or
            network filesystem can skip it; entries then carry no ``identical``
            key rather than a guess.

    Returns:
        ``(conflicts, stats)``. Each conflict is
        ``{path, providers, winner, involves_subset[, identical]}`` for every
        relative path present in 2+ folders. Plugin files are skipped: they are
        ordered by ``content=``, not by the VFS.
    """
    # The folders are walked side by side and folded in data-path order, the bytes compared
    # (sizes first), all in Rust: a MOMW setup has over a thousand folders and hundreds of
    # thousands of files.
    return cast(
        "tuple[list[dict[str, Any]], dict[str, int]]",
        ordercore_bridge.require("resource_scan")(
            [str(d) for d in data_dirs],
            [str(d) for d in (subset_dirs or [])],
            sorted(exclude_exts or []),
            compare_contents,
        ),
    )


def _providers_are_identical(rel: str, provider_dirs: Sequence[str]) -> bool:
    """Whether every provider ships byte-identical content for one path.

    Sizes are compared first and the contents are only read when they agree (in
    Rust when the backend is built, :mod:`wraithguard.fsio`), which is what makes
    this affordable: a re-shipped asset and a real override almost
    always differ in length, so the expensive path runs on the minority.

    A file that cannot be read counts as *not* identical. Claiming two files
    match when one of them could not be opened would quietly retire a genuine
    conflict from the list, and an unreadable file is itself worth surfacing.

    Args:
        rel: The path relative to each data folder.
        provider_dirs: The data folders providing it.

    Returns:
        ``True`` when all providers hold the same bytes.
    """
    from wraithguard.fsio import files_identical

    return files_identical([Path(d) / rel for d in provider_dirs])


#: Below this ratio of winner triangles to loser triangles, the winning mesh is
#: simple enough that the swap is worth mentioning. A retexture that also
#: replaces the mesh with a low-poly stand-in is the case this catches.
_MESH_SIMPLER_RATIO = 0.5

#: Logger for the mesh analysis pass, separate so it can be turned up without
#: turning up the whole sorter.
LOG_MESH = get_logger("mesh")


def analyse_mesh_conflicts(
    conflicts: Sequence[MutableMapping[str, Any]],
    limit: int = 0,
) -> dict[str, int]:
    """Read the meshes behind conflicts and record what the winner changes.

    Only paths that are meshes **and** already known to differ are opened. That
    is the whole reason this is affordable: parsing runs at roughly 120 files a
    second, so opening every mesh in a large setup would cost minutes, while
    opening only the contested ones costs a second or two. Identical providers
    are skipped because there is nothing to compare -- the bytes are the same.

    Findings are attached to the conflict entries in place under ``"mesh"``.
    Absent means "not looked at", which is deliberately distinct from "looked
    at and found nothing".

    Args:
        conflicts: The scan's conflicts, modified in place.
        limit: Stop after analysing this many meshes; 0 for no limit. A guard
            for pathological setups, not an expected path.

    Returns:
        Counters: how many were analysed, how many had something worth saying,
        and how many could not be read.
    """
    from wraithguard.nif.report import Difference

    # conflictcore reads the contested meshes side by side, each body once (keyed on content).
    found, stats = ordercore_bridge.require("mesh_analyse")(list(conflicts), limit)
    for entry, finding in zip(conflicts, found, strict=True):
        if finding is None:
            continue
        diff = finding["difference"]
        entry["mesh"] = MeshFinding(
            str(finding["path"]),
            difference=None if diff is None else Difference(**diff),
            unreadable=str(finding["unreadable"]),
            loser_partial=bool(finding["loser_partial"]),
            winner_partial=bool(finding["winner_partial"]),
        )
    LOG_MESH.debug(
        "analysed %d mesh conflict(s): %d finding(s), %d unreadable",
        stats["analysed"],
        stats["findings"],
        stats["unreadable"],
    )
    return dict(stats)


def describe_mesh_finding(finding: MeshFinding) -> str:
    """Say what a mesh conflict costs, in one line, or nothing at all.

    Only *losses* are named. A winner that adds detail is not a problem anybody
    needs to be told about, and a report that lists every difference equally is
    one nobody reads.

    Args:
        finding: The comparison result.

    Returns:
        A short description, or ``""`` when there is nothing worth saying.
    """
    if finding.unreadable:
        return _("could not read the mesh: %(reason)s") % {"reason": finding.unreadable}
    if not finding.reliable or finding.difference is None:
        # Silence rather than a caveat. A partial read cannot prove an absence,
        # so it has nothing to report, and saying "possibly lost collision"
        # would be worse than saying nothing.
        return ""
    parts: list[str] = []
    difference = finding.difference
    if difference.lost_collision:
        parts.append(_("loses collision"))
    if difference.lost_animation:
        parts.append(_("loses animation"))
    if difference.added_textures:
        parts.append(
            ngettext(
                "needs %(count)d texture the other does not ship",
                "needs %(count)d textures the other does not ship",
                len(difference.added_textures),
            )
            % {"count": len(difference.added_textures)}
        )
    ratio = difference.triangle_ratio
    if ratio is not None and ratio < _MESH_SIMPLER_RATIO:
        parts.append(_("%(percent)d%% of the triangles") % {"percent": round(ratio * 100)})
    return ", ".join(parts)


def describe_mesh_detail(
    analyser: MeshAnalyser,
    conflict: Mapping[str, Any],
) -> list[str]:
    """Describe every provider of one mesh, reading them on demand.

    This is the *selected a row* path, not the scan path. Nothing is parsed
    until a user asks about a specific file, which is why it can afford to
    describe every provider rather than only the winner: one mesh is
    milliseconds, and the largest in the corpus is under two seconds.

    Args:
        analyser: The cache to read through, so reselecting a row is free.
        conflict: One conflict entry.

    Returns:
        Lines for a detail panel, empty when the path is not a mesh.
    """
    path = str(conflict.get("path", ""))
    if not path.lower().endswith(".nif"):
        return []
    providers = [str(p) for p in conflict.get("providers", [])]
    if not providers:
        return []
    lines = [_("Mesh contents (read just now):")]
    structures: list[Structure | None] = []
    for index, provider in enumerate(providers, start=1):
        outcome = analyser.structure(Path(provider) / path)
        if isinstance(outcome, str):
            structures.append(None)
            lines.append(
                _("  %(n)d. %(dir)s - could not read: %(why)s")
                % {"n": index, "dir": provider, "why": outcome}
            )
            continue
        structures.append(outcome)
        traits = [
            ngettext("%(count)d shape", "%(count)d shapes", len(outcome.shapes))
            % {"count": len(outcome.shapes)},
            ngettext("%(count)d triangle", "%(count)d triangles", outcome.total_triangles)
            % {"count": outcome.total_triangles},
            ngettext("%(count)d texture", "%(count)d textures", len(outcome.textures))
            % {"count": len(outcome.textures)},
        ]
        # Presence is stated; absence only when the read was complete, because
        # a partial parse cannot prove a node is not there.
        if outcome.has_collision:
            traits.append(_("collision"))
        elif not outcome.partial:
            traits.append(_("no collision"))
        if outcome.has_animation:
            traits.append(_("animated"))
        if outcome.partial:
            traits.append(
                _("PARTIAL: %(read)d of %(declared)d blocks")
                % {"read": outcome.blocks_read, "declared": outcome.blocks_declared}
            )
        lines.append(f"  {index}. {provider} - " + ", ".join(traits))
    winner_dir = str(conflict.get("winner", providers[-1]))
    try:
        winner = structures[providers.index(winner_dir)]
    except ValueError:
        # The declared winner is not among the providers, which is a bug in
        # whoever built the entry. Reporting the contents without a comparison
        # is more useful than guessing which one won.
        return lines
    for index, (provider, loser) in enumerate(zip(providers, structures), start=1):
        if loser is None or winner is None or provider == winner_dir:
            continue
        note = describe_mesh_finding(
            MeshFinding(
                path,
                difference=compare_structures(loser, winner),
                loser_partial=loser.partial,
                winner_partial=winner.partial,
            )
        )
        if note:
            lines.append(
                _("  Against provider %(n)d, the winner %(note)s.") % {"n": index, "note": note}
            )
    for structure in structures:
        if structure is not None and structure.textures:
            lines.append(
                _("  Textures referenced: %(list)s") % {"list": ", ".join(structure.textures[:8])}
            )
            break
    return lines


def _mesh_note(conflict: Mapping[str, Any]) -> str:
    """The one-line mesh finding for a conflict, or ``""``.

    Args:
        conflict: One conflict entry.

    Returns:
        The description, empty when the mesh was not analysed or had nothing
        worth saying.
    """
    finding = conflict.get("mesh")
    return describe_mesh_finding(finding) if isinstance(finding, MeshFinding) else ""


def format_resource_report(
    conflicts: Sequence[Mapping[str, Any]],
    stats: Mapping[str, Any],
    subset_only: bool = False,
    limit: int = 200,
) -> str:
    """Render the loose-file conflict list as a readable report.

    Args:
        conflicts: The scan's conflicts.
        stats: The scan's counters.
        subset_only: Show only conflicts involving the user's own data paths.
        limit: Maximum rows to list; 0 for all.

    Returns:
        The report text.
    """
    text = ordercore_bridge.require("resource_report")(
        list(conflicts), dict(stats), [_mesh_note(c) for c in conflicts], subset_only, limit
    )
    # The mesh count is the one line that is worded for the user's language.
    return "\n".join(
        ordercore_bridge.translated(line, _RESOURCE_MESSAGES) for line in text.split("\n")
    )


#: The resource report's translated line, as conflictcore words it.
_RESOURCE_MESSAGES: Final = (
    (
        "  %(count)d mesh conflict changes what the asset does; those lines are marked below.",
        "  %(count)d mesh conflicts change what the asset does; those lines are marked below.",
    ),
)


def _resource_messages_for_make_pot(count: int) -> str:
    """The msgids of :data:`_RESOURCE_MESSAGES`, where tools/make_pot.py reads them."""
    return ngettext(
        "  %(count)d mesh conflict changes what the asset does; those lines are marked below.",
        "  %(count)d mesh conflicts change what the asset does; those lines are marked below.",
        count,
    )


def _mesh_columns(conflict: Mapping[str, Any]) -> tuple[str, str, str, str]:
    """The mesh columns for one CSV row.

    Every column is blank when the mesh was not analysed or the read was not
    reliable. Blank means "not established", which a spreadsheet filter treats
    differently from "no" -- and writing "no" for a mesh nobody could read
    would be a claim the scan cannot support.

    Args:
        conflict: One conflict entry.

    Returns:
        Note, lost-collision, lost-animation and triangle-ratio cells.
    """
    finding = conflict.get("mesh")
    if not isinstance(finding, MeshFinding):
        return ("", "", "", "")
    note = describe_mesh_finding(finding)
    if not finding.reliable or finding.difference is None:
        return (note, "", "", "")
    ratio = finding.difference.triangle_ratio
    return (
        note,
        "yes" if finding.difference.lost_collision else "no",
        "yes" if finding.difference.lost_animation else "no",
        "" if ratio is None else f"{ratio:.3f}",
    )


def write_resource_csv(path: str | Path, conflicts: Sequence[Mapping[str, Any]]) -> None:
    """Write the loose-file conflict list to a CSV.

    Args:
        path: Where to write.
        conflicts: The scan's conflicts.
    """
    text = ordercore_bridge.require("resource_csv")(
        list(conflicts), [_mesh_columns(c) for c in conflicts]
    )
    with Path(path).open("w", newline="", encoding="utf-8") as fh:
        fh.write(text)


# ---------------------------------------------------------------------------
# cell coverage map ("modmapper"): which mods touch which cells. Exterior cells
# are keyed by grid coords (for a heatmap), interior by name (for a list). Reads
# via either engine; interior/exterior is told apart the way modmapper does
# (interior = the cell's flags bit 0x01, or "INTERIOR" in the flags).
# ---------------------------------------------------------------------------


def _iter_cells(
    path: str | Path, session: Tes3ConvSession | None = None
) -> Iterator[Sequence[Any]]:
    """Yield ('ext', gx, gy) or ('int', name, None) for each CELL in a plugin."""
    if session is not None:
        # session.cells() is sidecar-cached, so map rebuilds skip the big JSON.
        yield from session.cells(path)
    else:
        for rtype, rid, _deleted in parse_tes3_records(path):
            if rtype != "CELL":
                continue
            mm = re.match(r"^Exterior \((-?\d+), (-?\d+)\)", rid)
            if mm:
                yield ("ext", int(mm.group(1)), int(mm.group(2)))
            else:
                # A CELL's rid from _tes3_record_key is always one of these
                # two shapes (see its CELL branch); the interior case is the
                # only one left once the exterior regex hasn't matched.
                yield ("int", rid[len("Interior: ") :], None)


def build_cell_coverage(
    active_order: Sequence[str],
    index: PluginFileIndex,
    subset_names: Sequence[str] | None = None,
    session: Tes3ConvSession | None = None,
) -> dict[str, Any]:
    """Build the per-cell coverage map for the active order.

    Returns {"exterior":

    {(gx,gy):[mods]}, "interior": {name:[mods]}, "scanned", "unreadable":[...],
    "subset_lower": set}. Mods are in load order.
    """
    subset_lower = {s.lower() for s in (subset_names or [])}
    ext: dict[tuple[int, int], list[str]] = {}
    inte: dict[str, list[str]] = {}
    unreadable: list[str] = []
    scanned = 0
    for plugin in active_order:
        path = index.find(plugin) if index else None
        if path is None:
            unreadable.append(plugin)
            continue
        scanned += 1
        se, si = set(), set()
        for cell in _iter_cells(path, session):
            if cell[0] == "ext":
                key = (cell[1], cell[2])
                if key not in se:
                    se.add(key)
                    ext.setdefault(key, []).append(plugin)
            else:
                nm = cell[1]
                if nm not in si:
                    si.add(nm)
                    inte.setdefault(nm, []).append(plugin)
    return {
        "exterior": ext,
        "interior": inte,
        "scanned": scanned,
        "unreadable": unreadable,
        "subset_lower": subset_lower,
    }


def _html_escape(s: object) -> str:
    return (
        str(s)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def generate_cell_map_html(
    coverage: Mapping[str, Any],
    title: str = "Wraithguard Toolkit - Cell Map",
) -> str:
    """Render the cell map as a self-contained HTML page.

    The rendering itself lives in :mod:`wraithguard.viz.cellmap` -- 216 lines of
    HTML, CSS and JavaScript had no business sitting in the sort engine, and as
    one f-string it could not be tested in pieces. This stays as the engine's
    public entry point so existing callers and the CLI are unaffected.

    Args:
        coverage: The result of :func:`build_cell_coverage`.
        title: The page title.

    Returns:
        A complete, self-contained HTML document.
    """
    from wraithguard.viz.cellmap import generate_cell_map_html as _render

    html = _render(coverage, title)
    trace(f"cell map: rendered {len(html)} bytes via viz.cellmap")
    return html


# ---------------------------------------------------------------------------
# openmw.cfg handling
# ---------------------------------------------------------------------------

#: openmw.cfg is normally UTF-8, but it can legitimately contain bytes that
#: are not (a cp1252 accented mod folder, a hand edit in Notepad). Decoding
#: with ``surrogateescape`` maps those bytes to lone surrogates and encoding
#: with it restores them exactly, so a read/write round-trip is byte-
#: preserving. ``errors="replace"`` used to destroy them permanently, breaking
#: the user's data= path.
#: Largest exterior cell coordinate treated as real. Beyond this a plugin is
#: almost certainly corrupt, and plotting it would stretch the map to nothing.
CELL_GRID_LIMIT = 4096

#: Cell-map geometry: a 12px square on a 13px pitch, leaving a 1px gutter.
CELL_MAP_CELL_PX = 12
CELL_MAP_STEP_PX = 13

CFG_READ_ENCODING = "utf-8-sig"
CFG_WRITE_ENCODING = "utf-8"
CFG_ERRORS = "surrogateescape"


def read_toml_text(path: Path) -> str:
    """Read a TOML file, which the spec requires to be UTF-8.

    Raises:
        SystemExit: with an actionable message if it is not valid UTF-8,
            rather than surfacing a raw UnicodeDecodeError.

    """
    try:
        return Path(path).read_text(encoding="utf-8")
    except UnicodeDecodeError as exc:
        raise SystemExit(
            f"{path} is not valid UTF-8 (byte 0x{exc.object[exc.start]:02x} at "
            f"position {exc.start}). TOML files must be UTF-8 -- re-save the "
            f"file as UTF-8 in your editor."
        ) from exc


def read_cfg(
    path: Path,
) -> tuple[list[str], list[int], list[tuple[str, str]], list[int], list[str]]:
    """Read openmw.cfg into its lines plus the content= / data= segments."""
    lines = path.read_text(encoding=CFG_READ_ENCODING, errors=CFG_ERRORS).splitlines()
    content_positions, content_order = [], []
    data_positions, data_order = [], []
    for i, line in enumerate(lines):
        m = re.match(r"^\s*content\s*=\s*(.+?)\s*$", line, re.IGNORECASE)
        if m:
            content_positions.append(i)
            content_order.append((m.group(1), line))
            continue
        m = re.match(r"^\s*data\s*=\s*(.+?)\s*$", line, re.IGNORECASE)
        if m:
            data_positions.append(i)
            data_order.append(line)
    return lines, content_positions, content_order, data_positions, data_order


# ---------------------------------------------------------------------------
# mlox rule parsing (Order / NearStart / NearEnd only)
# ---------------------------------------------------------------------------

# Rule-file parsing now lives in wraithguard/rules/parser.py. Behaviour pinned
# by tests/test_differential.py; callers import it from there directly.
from wraithguard.configurator.cfglines import (
    extract_data_path_value,
    format_data_line,
)
from wraithguard.plugins import PluginFileIndex

# ---------------------------------------------------------------------------
# mlox predicate evaluation (Requires / Conflict / Note) -- read-only,
# reported as warnings after sorting. This is a best-effort reimplementation
# of mlox's tiny lisp-like logic language (ALL/ANY/NOT/DESC), not the real
# mlox engine -- good enough to flag likely problems, not to be trusted blindly.
# ---------------------------------------------------------------------------


# --- [VER]/[SIZE]/[DESC]/[MWSE-LUA] function-token evaluation ---------------


# ---------------------------------------------------------------------------
# plugin-order.yml -- MOMW's source of truth for which plugins belong to which
# curated mod list (and their canonical order). Used, when a --list-name is
# given, to tell the curated base list apart from YOUR custom additions:
#   * curated plugins are excluded from the sort (never reordered -- they're
#     the list's job, not ours) and never highlighted as custom
#   * everything else is a true custom addition
# plus read-only sanity warnings (redundant / orphan / needs-cleaning) and a
# base-order drift check against the yml's canonical order for the list. All of
# this is opt-in and additive: with no --plugin-order-yml, behavior is exactly
# as before.
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# mod folder scan -- generate a subset file directly from a mods directory
# (folded in from the standalone mod_scan.py). Walks a folder tree and, for
# each directory that looks like a mod's data folder -- i.e. it directly
# contains a recognized asset subfolder (meshes/textures/scripts/...) OR a
# plugin file (.esp/.esm/.omwaddon/.omwscripts) -- records that folder's path
# and any plugins in it, then stops descending that branch (OpenMW/mlox expect
# plugins directly inside a data= folder, so there's no need to go deeper). The
# result is exactly the mixed "one data path or plugin per line" format that
# extract_subset_from_subset_file already consumes.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# subset extraction
# ---------------------------------------------------------------------------


#: ``groundcover=Vurt_Grass.esp`` in a subset file. Deliberately the same
#: spelling openmw.cfg uses, so the line means what it looks like.
# ---------------------------------------------------------------------------
# data= (folder path) insertion -- positioned by after/before anchor, since
# mlox has no concept of ordering data paths, only plugins
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# TOML generation -- write a corrected momw-customizations.toml so the fix
# persists across umo/momw-configurator rebuilds, instead of only patching
# openmw.cfg (which a rebuild would just overwrite again)
# ---------------------------------------------------------------------------


def read_savegame_content_files(path: str | Path) -> tuple[list[str] | None, str | None]:
    """Content files an OpenMW .omwsave depends on.

    Saves are ESM3 files: a TES3 header record, then a SAVE record whose DEPE subrecords
    each carry one content filename (components/esm3/savedgame.cpp). Returns (files,
    error): files is None on failure.
    """
    try:
        raw = Path(path).read_bytes()
    except OSError as e:
        return None, f"can't read save: {e}"
    # Rust (viewer-shell/toolscore, save.rs), shared with the viewer's Save Check.
    try:
        return list(ordercore_bridge.require("save_content_files")(raw)), None
    except ValueError as e:
        return None, str(e)


def check_savegame_against_order(
    save_path: str | Path, active_order: Sequence[str]
) -> tuple[list[str] | None, list[str] | None, str | None]:
    """Check a savegame's content files against the active order.

    Returns (save_files, missing, error):

    which of the save's content files are absent from the given load order. A missing
    file means OpenMW will refuse to load (or badly degrade) that save.
    """
    files, err = read_savegame_content_files(save_path)
    if files is None:
        return None, None, err
    active_lower = {str(n).lower() for n in active_order}
    missing = [f for f in files if f.lower() not in active_lower]
    return files, missing, None


BACKUP_PATTERNS = (
    ".preclean.bak",  # ours: original before a staged tes3cmd clean
    ".masterfix.bak",  # ours: original before a master-size resync
    ".precustom.bak",  # ours: original before a staged tes3cmd custom command
    ".pretes3cmd.bak",  # ours: original before a non-staged tes3cmd command
    ".premaster.bak",  # ours: original before a master was removed or renamed
)


def scan_backups(
    dirs: Sequence[str], cfg_path: str | Path | None = None, max_depth: int = 4
) -> list[tuple[Path, Path | None, str]]:
    """Find backup files this tool and the tools it drives leave behind.

    Namely:

    *.preclean.bak, *.masterfix.bak, tes3cmd's 'name~1.ext', and timestamped '*.bak-
    YYYYMMDD-HHMMSS' / Configurator '*.backup.*' copies. Returns [(backup_path,
    original_path_or_None, kind)].
    """
    roots = [str(d) for d in dirs if d]
    if cfg_path:
        roots.append(str(Path(cfg_path).parent))
    # The walk and the name rules are Rust (viewer-shell/toolscore, backups.rs), shared with
    # the viewer's Tools page.
    found = ordercore_bridge.require("backup_scan")(roots, max_depth)
    return [(Path(b), Path(o), k) for b, o, k in found]


#: Schemes we are willing to download from. Anything else (notably ``file:``,
#: ``ftp:`` and ``data:``) is rejected: these URLs come from a persisted
#: settings file and from environment variables, so a tampered value must not
#: be able to make an "update" button read an arbitrary local file and write
#: it over the user's rules.

#: Hard cap on a single download (bytes). The real files are ~250 KB; this
#: stops a hostile or misconfigured endpoint from exhausting memory.


RULES_REPO = (
    "DanaePlays/mlox-rules"  # actively maintained; plox uses it, mlox 1.1+ auto-updates from it
)
# {name} is replaced with the rule filename (mlox_base.txt / mlox_user.txt).
# Users can point this at a fork/mirror via the GUI's Sources dialog or
# $MLOX_RULES_URL_TEMPLATE.


# ---------------------------------------------------------------------------
# graph + stable topological sort
# ---------------------------------------------------------------------------

# Graph primitives now live in wraithguard/sort/graph.py.

# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# console UX helpers -- small, dependency-free "sections" so a run's output
# reads as a few clearly separated stages instead of one long scroll.
# stdout is what the GUI captures/redirects too, so keep this plain text
# (no ANSI codes) -- the GUI does its own colorizing by scanning for tags
# like [CONFLICT] / [REQUIRES] / WARNING: / NOTE: on each line.
# ---------------------------------------------------------------------------


def _section(title: str) -> None:
    print(f"\n{'=' * 70}\n {title}\n{'=' * 70}")


def _subsection(title: str) -> None:
    print(f"\n--- {title} ---")


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def build_arg_parser() -> argparse.ArgumentParser:
    """Build the CLI argument parser."""
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--cfg", required=True, type=Path, help="Path to openmw.cfg")
    ap.add_argument(
        "--rules",
        required=True,
        nargs="+",
        type=Path,
        help="mlox rule file(s) or directories, in increasing priority order "
        "(pass mlox_base.txt first, mlox_user.txt last)",
    )
    ap.add_argument(
        "--customizations",
        type=Path,
        help="momw-customizations.toml to auto-derive the subset from",
    )
    ap.add_argument(
        "--subset",
        nargs="*",
        default=[],
        help="Explicit list of plugin filenames to sort (combined with --customizations if both given)",
    )
    ap.add_argument(
        "--groundcover",
        nargs="*",
        default=[],
        metavar="PLUGIN",
        help="Plugin filenames to declare as grass. They are kept out of content= "
        "and written as groundcover= instead. Their data= folders still go in "
        "normally (OpenMW must be able to find the file), so name the folder with "
        "--subset or in the subset file as usual",
    )
    ap.add_argument(
        "--subset-file",
        type=Path,
        help="Plain text (one plugin per line) or minimal TOML (subset = [...]) "
        "file listing plugins to sort -- shorter to maintain than --subset or "
        "a full momw-customizations.toml block",
    )
    ap.add_argument(
        "--scan-dir",
        type=Path,
        help="Scan this mods folder for data folders and plugins and write the result "
        "to --subset-file (required with this), then sort using it. Folds in the "
        "old mod_scan.py: each folder containing an asset subfolder or a plugin "
        "becomes a data= entry (plus its plugins as content=), and matched branches "
        "aren't descended further.",
    )
    ap.add_argument(
        "--subset-from-cfg",
        action="store_true",
        help="Pull UNMANAGED (orphan) entries out of the --cfg itself and sort them: "
        "every content= plugin and data= path already in openmw.cfg that is neither on "
        "the curated list (--plugin-order-yml + --list-name) nor declared in your "
        "--customizations. Their existing cfg order is kept as the frozen starting "
        "order (until the sort repositions them). Base masters (Morrowind/Tribunal/"
        "Bloodmoon.esm) and the base game's Data Files folder are never pulled. Can be "
        "the only subset source, or combined with --customizations/--subset/--subset-file "
        "to sort those plus the orphans. data= orphans are classified only when a "
        "--data-paths-cache is given (that cached MOMW list order is the 'managed' signal "
        "a folder can match); they are reported, not reordered, since they already "
        "sit in the cfg's data= order.",
    )
    ap.add_argument("--dry-run", action="store_true", help="Print the plan, write nothing")
    ap.add_argument(
        "--no-backup",
        action="store_true",
        help="Skip writing a .bak-<timestamp> copy before overwriting openmw.cfg and/or "
        "before overwriting an existing --emit-toml target (e.g. when writing back "
        "to the same file --customizations pointed at).",
    )
    ap.add_argument(
        "--emit-toml",
        type=Path,
        help="Write a momw-customizations.toml here (with insert blocks reordered/"
        "re-anchored per the mlox+anchor results) instead of/alongside patching "
        "openmw.cfg directly. If --customizations is also given, its other blocks "
        "(removeContent, replace, append, ...) are preserved and only the sorted "
        "plugins/paths are regenerated; if not, a brand new single-block TOML is "
        "generated from --subset/--subset-file alone. This is the durable fix: feed "
        "the output back into momw-configurator so the correct order survives "
        "future rebuilds.",
    )
    ap.add_argument(
        "--list-name",
        help="listName to write into the emitted momw-customizations.toml (the curated "
        "mod list these customizations apply to, e.g. 'total-overhaul'). Overrides "
        "the listName from --customizations if both are given. Without this, the "
        "source TOML's listName is kept, or -- when generating from --subset-file "
        "alone -- it defaults to the placeholder 'generated'. momw-configurator "
        "REQUIRES a correct listName, so set this when generating a fresh TOML.",
    )
    ap.add_argument(
        "--plugin-order-yml",
        type=Path,
        help="MOMW's plugin-order.yml (source of truth for which plugins belong to which "
        "curated mod list). With --list-name, curated plugins for that list are "
        "excluded from the sort (never reordered) so only YOUR custom additions are "
        "touched, and read-only sanity warnings are emitted: redundant (a custom "
        "plugin that's already on the list), orphan (in your cfg but neither on the "
        "list nor in your customizations), needs-cleaning (TES3CMD), and a base-order "
        "drift check against the list's canonical order. Optional; PyYAML is used if "
        "installed, else a built-in parser.",
    )
    ap.add_argument(
        "--data-paths-cache",
        type=Path,
        help="A cached MOMW list data-path order, fetched from the cfg-generator API "
        "(one relative data-path tail per line; see the GUI's 'Fetch data paths' button "
        "or the Load order page's Fetch data paths). Read-only warnings are emitted: the "
        "list's data= paths not present in openmw.cfg, and data= paths whose cfg order "
        "contradicts the list's. Also the signal that tells a list-managed data= path "
        "from an unmanaged (orphan) one for --subset-from-cfg. Optional.",
    )
    ap.add_argument(
        "--write-cfg",
        action="store_true",
        help="Actually patch openmw.cfg in place. Off by default -- "
        "prefer --emit-toml for a fix that survives future rebuilds. "
        "A .bak copy is made first unless --no-backup is also given.",
    )
    ap.add_argument(
        "--sort-data-paths",
        action="store_true",
        help="Also position data= (folder path) insertions from the customizations "
        "TOML: anchored by their after/before field if given, or otherwise "
        "inferred by scanning the folder for plugin files and anchoring next to "
        "whichever existing data= path owns the nearest neighboring plugin in the "
        "mlox-sorted content order. Off by default: mlox has no concept of "
        "data-path order itself, so this is a separate opt-in feature from the "
        "mlox-based plugin sort. When off, any data-path insertions found in "
        "the TOML are left exactly as originally written (in --emit-toml output) "
        "or ignored entirely (for --write-cfg).",
    )
    ap.add_argument(
        "--no-predicate-warnings",
        action="store_true",
        help="Skip evaluating [Requires]/[Conflict]/[Note] rules against the final "
        "plugin list. On by default; this is read-only and never changes the "
        "computed sort or what gets written, only what gets printed.",
    )
    ap.add_argument(
        "--check-conflicts",
        action="store_true",
        help="After sorting, scan the active plugins for TES3 record-level conflicts "
        "(where 2+ plugins define/override the same record -- the last in the load "
        "order wins), like TES3View/tes3cmd. Read-only; needs the plugin files "
        "reachable via the cfg's data= folders. Can be slow on big lists.",
    )
    ap.add_argument(
        "--conflicts-out",
        type=Path,
        help="Write the full conflict list to this CSV (use with --check-conflicts).",
    )
    ap.add_argument(
        "--tes3conv",
        type=Path,
        help="Path to a tes3conv executable - a fallback only, for a build without the "
        "built-in reader (wraithguard_native), which reads and writes plugins in process "
        "and is always used when installed. Auto-detected from PATH / $MLOX_TES3CONV / "
        "next to this script if not given.",
    )
    ap.add_argument(
        "--cell-map",
        type=Path,
        help="Write a self-contained HTML 'modmapper'-style cell map here: an exterior-cell "
        "heatmap (brighter = more mods) plus an interior-cell list, showing which mods "
        "touch which cells (cells your custom mods touch are highlighted). Read-only; "
        "open the file in any browser.",
    )
    ap.add_argument(
        "--resource-conflicts",
        action="store_true",
        help="Scan the cfg's data= folders for loose-file (VFS) conflicts: the same relative "
        "path in 2+ folders (later wins), like MO2's Data conflicts. Read-only.",
    )
    ap.add_argument(
        "--lint",
        action="store_true",
        help="After sorting, run tes3lint-style checks over the active plugins: evil "
        "GMSTs, the interior fog-density-0 bug, interior cells with no pathgrid, "
        "expansion-function use without the expansion mastered, omwaddon/omwscripts "
        "twin mismatches, and blank custom headers. Read-only; VFS-aware.",
    )
    ap.add_argument(
        "--resources-out",
        type=Path,
        help="Write the full resource-conflict list to this CSV (with --resource-conflicts).",
    )
    ap.add_argument(
        "--exclude",
        nargs="*",
        default=[],
        help="Name patterns (glob) to skip in --check-conflicts/--cell-map/"
        "--resource-conflicts scans, e.g. 's3lightfixes*' '*delta*' '*grass*'.",
    )
    ap.add_argument(
        "--conflicts-subset-only",
        action="store_true",
        help="With --check-conflicts, report only conflicts that involve YOUR custom "
        "mods (skip base-list vs base-list conflicts).",
    )
    ap.add_argument(
        "--trace",
        nargs="?",
        const=True,
        default=None,
        metavar="LOGFILE",
        help="Write a debug trace log for troubleshooting (off by default). Use --trace "
        "for the default log (wraithguard_toolkit_trace.log), or --trace PATH to choose "
        "the file.",
    )
    ap.add_argument(
        "--json-dump-dir",
        type=Path,
        help="Only with the tes3conv fallback (no built-in reader): write (and KEEP) its "
        "per-plugin JSON conversions in this folder instead of a temp dir removed on "
        "exit. The built-in reader writes no JSON.",
    )
    ap.add_argument(
        "-v",
        "--verbose",
        action="count",
        default=0,
        help="Show run diagnostics on stderr (-v: progress, -vv: per-item detail). "
        "The report itself always goes to stdout regardless.",
    )
    return ap


def _conflict_and_cellmap_scans(
    args: argparse.Namespace,
    subset: Sequence[str],
    final_order: Sequence[str] | None,
    base_order_names: Sequence[str],
    conf_dirs: Sequence[str],
) -> list[dict[str, Any]]:
    """Run the opt-in TES3 record-conflict and cell-map scans.

    Extracted verbatim from compute_plan during the §16 decomposition; the
    report output is unchanged.

    Returns:
        The conflict list (empty when the scans were not requested).

    """
    conflicts: list[dict[str, Any]] = []
    want_conflicts = getattr(args, "check_conflicts", False)
    cell_map_out = getattr(args, "cell_map", None)
    if want_conflicts or cell_map_out:
        active = final_order if final_order else base_order_names
        active, _excl = filter_plugins(active, getattr(args, "exclude", None))
        if _excl:
            print(
                ngettext(
                    "  (excluded %(count)d plugin by --exclude)",
                    "  (excluded %(count)d plugins by --exclude)",
                    len(_excl),
                )
                % {"count": len(_excl)}
            )
        cindex = PluginFileIndex(conf_dirs)
        conv = find_tes3conv(
            explicit=getattr(args, "tes3conv", None),
            extra_dirs=[str(args.cfg.parent) if args.cfg else None],
        )
        _dump = getattr(args, "json_dump_dir", None)
        # The Rust backend when installed; tes3conv only without it.
        csession = open_record_session(
            conv, dump_dir=str(_dump) if _dump else None, keep=bool(_dump)
        )
        if (
            isinstance(csession, Tes3ConvSession)
            and not isinstance(csession, NativeEspSession)
            and _dump
        ):
            print(_("  Keeping tes3conv JSON dump in: %(path)s") % {"path": csession.dumped_dir()})

        if want_conflicts:
            _section("TES3 RECORD CONFLICTS (read-only)")
            print(
                _("  Engine: %(engine)s")
                % {
                    "engine": (
                        _("native esp reader (field-level)")
                        if isinstance(csession, NativeEspSession)
                        else (
                            f"tes3conv ({conv})"
                            if csession
                            else _("built-in parser (record-level)")
                        )
                    )
                }
            )
            conflicts, cstats = detect_conflicts(
                active, cindex, subset_names=subset, session=csession
            )
            print(
                format_conflict_report(
                    conflicts,
                    cstats,
                    subset_only=getattr(args, "conflicts_subset_only", False),
                    limit=200,
                )
            )
            out = getattr(args, "conflicts_out", None)
            if out and conflicts:
                try:
                    write_conflict_csv(out, conflicts)
                    print(_("\n  Wrote conflict report: %(path)s") % {"path": out})
                except OSError as e:
                    _LOG.error(_("could not write conflict CSV: %(error)s"), {"error": e})

        if cell_map_out:
            _section("CELL MAP (which mods touch which cells)")
            cov = build_cell_coverage(active, cindex, subset_names=subset, session=csession)
            try:
                Path(cell_map_out).write_text(generate_cell_map_html(cov), encoding="utf-8")
                print(
                    _(
                        "  Scanned %(scanned)d plugin(s): %(exterior)d exterior + "
                        "%(interior)d interior cell(s) touched."
                    )
                    % {
                        "scanned": cov["scanned"],
                        "exterior": len(cov["exterior"]),
                        "interior": len(cov["interior"]),
                    }
                )
                print(
                    _("  Wrote cell map: %(path)s  (open it in a browser)") % {"path": cell_map_out}
                )
            except OSError as e:
                _LOG.error(_("could not write cell map: %(error)s"), {"error": e})

        if csession is not None:
            csession.cleanup()  # drop the temp dump folder (kept with --json-dump-dir)
    return conflicts


def _lint_stage(
    args: argparse.Namespace,
    subset: Sequence[str],
    final_order: Sequence[str] | None,
    base_order_names: Sequence[str],
    conf_dirs: Sequence[str],
    subset_origins: Mapping[str, str],
) -> None:
    """Run the opt-in tes3lint-style checks (read-only).

    Extracted verbatim from compute_plan during the §16 decomposition; the
    report output is unchanged.
    """
    if getattr(args, "lint", False):
        _section("LINT (tes3lint-style checks, read-only)")
        _lactive = final_order if final_order else base_order_names
        _lactive, _lexcl = filter_plugins(_lactive, getattr(args, "exclude", None))
        if _lexcl:
            print(
                ngettext(
                    "  (excluded %(count)d plugin by --exclude)",
                    "  (excluded %(count)d plugins by --exclude)",
                    len(_lexcl),
                )
                % {"count": len(_lexcl)}
            )
        _lw, _ls = lint_plugins(
            _lactive, PluginFileIndex(conf_dirs), subset_names=subset, origins=subset_origins
        )
        print(
            _(
                "  Scanned %(scanned)d plugin(s); %(cells)d interior cell(s), "
                "%(grids)d interior pathgrid(s)."
            )
            % {
                "scanned": _ls.get("scanned", 0),
                "cells": _ls.get("interior_cells", 0),
                "grids": _ls.get("pathgrids", 0),
            }
        )
        for _w in _lw:
            print(f"\n{_w}")
        if not _lw:
            print(_("\n  No lint findings."))


def _resource_stage(
    args: argparse.Namespace,
    subset_dirs: Sequence[str],
    conf_dirs: Sequence[str],
) -> None:
    """Run the opt-in data-path (VFS) resource-conflict scan.

    Args:
        args: The run's arguments.
        subset_dirs: This run's own (pending) data folders, to say which side is yours.
        conf_dirs: Every folder to scan.
    """
    want_resources = getattr(args, "resource_conflicts", False)
    if want_resources:
        _section("DATA-PATH RESOURCE (VFS) CONFLICTS (read-only)")
        rconf, rstats = detect_resource_conflicts(conf_dirs, subset_dirs=subset_dirs)
        # Meshes that conflict *and* differ get read. Everything else is left
        # closed: the point of the byte comparison above is that it already
        # told us which files nobody has to think about.
        mesh_stats = analyse_mesh_conflicts(rconf)
        rstats = {**rstats, **{f"mesh_{k}": v for k, v in mesh_stats.items()}}
        print(format_resource_report(rconf, rstats, limit=200))
        rout = getattr(args, "resources_out", None)
        if rout and rconf:
            try:
                write_resource_csv(rout, rconf)
                print(_("\n  Wrote resource report: %(path)s") % {"path": rout})
            except OSError as e:
                _LOG.error(_("could not write resource CSV: %(error)s"), {"error": e})


#: What ``compute_plan`` prints and logs, as ``ordercore::plan`` words it (port step 4.2 slice
#: 2k): each line of its report is matched to one of these and put back into the user's language
#: (``ordercore_bridge.translated``). A pair is an ``ngettext`` message.
_PLAN_MESSAGES: Final = (
    "Scanned '%(path)s': %(folders)d mod folder(s), %(plugins)d plugin(s).",
    "Wrote subset file: %(path)s",
    "  %(file)s: %(plugins)d plugin(s), %(paths)d data path(s)",
    "  NOTE: data path insertions found but not sorted (pass --sort-data-paths to include "
    "them): %(paths)s",
    "  in-memory scan: %(plugins)d plugin(s), %(paths)d data path(s)",
    (
        "  NOTE: data path insertion found but not sorted (pass --sort-data-paths to include "
        "it): %(count)d path",
        "  NOTE: data path insertions found but not sorted (pass --sort-data-paths to include "
        "them): %(count)d paths",
    ),
    "  %(file)s: %(plugins)d content plugin(s), %(paths)d data path(s)",
    "'%(entry)s' from %(source)s doesn't look like a plugin filename or a data folder path (no "
    "recognized extension, no slash) -- skipping.",
    "'%(entry)s' from %(source)s is declared as groundcover but is not a plugin filename -- "
    "skipping.",
    "groundcover declared by this run",
    "  These are written as groundcover= rather than content=. Their data= folders are inserted "
    "normally -- OpenMW has to find the file.",
    "groundcover plugins held back",
    (
        "%(count)d plugin in your subset is already declared as groundcover in openmw.cfg, so it "
        "was NOT added to content=:",
        "%(count)d plugins in your subset are already declared as groundcover in openmw.cfg, so "
        "they were NOT added to content=:",
    ),
    "  Their groundcover= lines are untouched and their data= paths are still written, so the "
    "grass stays enabled -- as grass. Loading one as content= too would spawn every blade as a "
    "real object.",
    (
        "  Loaded %(count)d plugin entry from %(file)s",
        "  Loaded %(count)d plugin entries from %(file)s",
    ),
    "  WARNING: could not read plugin-order.yml (%(error)s) -- skipping yml checks.",
    (
        "  '%(list)s': %(count)d curated plugin on this list",
        "  '%(list)s': %(count)d curated plugins on this list",
    ),
    "  WARNING: no plugins found for list '%(list)s' in the yml -- check the list name "
    "spelling. Curated-set checks skipped.",
    (
        "  Excluded %(count)d curated plugin from the sort: %(names)s",
        "  Excluded %(count)d curated plugins from the sort: %(names)s",
    ),
    "UNMANAGED (ORPHAN) ENTRIES FROM openmw.cfg",
    (
        "  %(count)d orphan plugin pulled from openmw.cfg",
        "  %(count)d orphan plugins pulled from openmw.cfg",
    ),
    "  No unmanaged plugins found -- every content= plugin in the cfg is on the list or in your "
    "customizations.",
    (
        "  %(count)d orphan data= path found in openmw.cfg",
        "  %(count)d orphan data= paths found in openmw.cfg",
    ),
    "  No unmanaged data= paths found -- every data= path in the cfg is on the list's "
    "data-path order or in your customizations.",
    "  data= paths not classified -- fetch a data-paths cache for this list to surface "
    "unmanaged (orphan) data= paths too.",
    "  No plugins to sort. The current openmw.cfg load order is shown above for editing -- "
    "remove entries and Export, or add plugins and Sort again.",
    "\n  Already in cfg, will be repositioned within it: %(names)s",
    "  Not in cfg yet, will be inserted: %(names)s",
    (
        "NOTE: %(name)s: %(count)d conditional/unrecognized line inside ordering rules treated "
        "as not-installed and bridged over (mlox does the same).",
        "NOTE: %(name)s: %(count)d conditional/unrecognized lines inside ordering rules treated "
        "as not-installed and bridged over (mlox does the same).",
    ),
    "  Read header masters for %(have)d of %(total)d custom plugin(s).",
    (
        "%(count)d mlox ordering rule not applied -- your openmw.cfg already orders these the "
        "other way, so your (curated) cfg order is kept:",
        "%(count)d mlox ordering rules not applied -- your openmw.cfg already orders these the "
        "other way, so your (curated) cfg order is kept:",
    ),
    "    - mlox wanted '%(first)s' before '%(second)s', but your load order already has "
    "'%(second)s' before '%(first)s'",
    "\n  No [Conflict]/[Requires]/[Note] warnings triggered.",
    "\n  No plugin-order.yml warnings.",
    "\n  No data path warnings.",
    "  (plugin files not reachable from the data folders; can't read headers)",
    "  All masters present and correctly ordered.",
    (
        "\n[STALE] '%(artifact)s' is older than %(count)d active plugin (e.g. "
        "%(examples)s%(more)s) -- re-run momw-configurator so it gets rebuilt against the "
        "current load order.",
        "\n[STALE] '%(artifact)s' is older than %(count)d active plugins (e.g. "
        "%(examples)s%(more)s) -- re-run momw-configurator so it gets rebuilt against the "
        "current load order.",
    ),
    "  Inferred anchor for '%(value)s': %(mode)s '%(anchor)s' (via plugin %(via)s)",
    "NOTE: '%(value)s' already present in data= list -- skipping duplicate insert.",
    "NOTE: '%(value)s' has no after/before anchor -- appending at end of data= list.",
    "WARNING: anchor '%(anchor)s' not found among existing data= paths for '%(value)s' -- "
    "appending at end instead.",
)

#: The tail ``final data= order`` puts on a line the run inserted.
_INSERTED_TAIL: Final = "  <-- inserted"


def _plan_args(args: argparse.Namespace) -> dict[str, Any]:
    """What ``ordercore::plan::compute_plan`` reads off a run's arguments (paths as ``str``)."""

    def path(name: str) -> str | None:
        value = getattr(args, name, None)
        return str(value) if value else None

    return {
        "cfg": str(args.cfg),
        "customizations": path("customizations"),
        "subset": [str(s) for s in (getattr(args, "subset", None) or [])],
        "subset_file": path("subset_file"),
        "subset_lines": list(getattr(args, "subset_lines", None) or []) or None,
        "scan_dir": path("scan_dir"),
        "subset_from_cfg": bool(getattr(args, "subset_from_cfg", False)),
        "allow_empty_sort": bool(getattr(args, "allow_empty_sort", False)),
        "sort_data_paths": bool(getattr(args, "sort_data_paths", False)),
        "emit_toml": bool(getattr(args, "emit_toml", None)),
        "groundcover": [str(g) for g in (getattr(args, "groundcover", None) or [])],
        "plugin_order_yml": path("plugin_order_yml"),
        "list_name": getattr(args, "list_name", None) or None,
        "rules": [str(r) for r in (getattr(args, "rules", None) or [])],
        "no_predicate_warnings": bool(getattr(args, "no_predicate_warnings", False)),
        "data_paths_cache": path("data_paths_cache"),
        "verbose": bool(getattr(args, "verbose", 0)),
    }


#: The two summary lines whose answer is a word of its own (``yes`` / ``no``).
_WRITTEN_LINE = re.compile(r"  (openmw\.cfg|customizations\.toml) written: +(yes|no)")


def _say(kind: str, text: str, templates: Iterable[str | tuple[str, str]] = _PLAN_MESSAGES) -> None:
    """Print (or log) one item of ordercore's report in the user's language."""
    if kind == "line" and text.endswith(_INSERTED_TAIL):
        print(text[: -len(_INSERTED_TAIL)] + _("  <-- inserted"))
        return
    written = _WRITTEN_LINE.fullmatch(text) if kind == "line" else None
    if written is not None:
        answer = _("yes") if written.group(2) == "yes" else _("no")
        if written.group(1) == "openmw.cfg":
            print(_("  openmw.cfg written:    %(answer)s") % {"answer": answer})
        else:
            print(_("  customizations.toml written: %(answer)s") % {"answer": answer})
        return
    worded = ordercore_bridge.translated(text, templates)
    if kind == "section":
        _section(worded)
    elif kind == "subsection":
        _subsection(worded)
    elif kind == "log":
        _LOG.warning("%s", worded)
    else:
        print(worded)


def compute_plan(args: argparse.Namespace) -> dict:
    """Read the inputs, sort, and check the result: the first half of a run. Writes nothing.

    Answered by ``ordercore::plan`` (port step 4.2 slice 2k); the report is printed as it
    comes, in the user's language. At its ``scans`` item the record-conflict, cell-map, lint
    and resource scans (the Python's until 4.3 / 4.4) run in their place.

    Returns:
        The plan :func:`write_plan` writes out.

    Raises:
        SystemExit: The run stops, with the reason.
    """
    refused, items, got = ordercore_bridge.require("order_compute_plan")(_plan_args(args))
    conflicts: list[dict[str, Any]] = []
    for kind, text, _warning, _names in items:
        if kind == "scans":
            assert got is not None  # noqa: S101 - the report places the scans only in a plan
            final = got["final_order"]
            base = got["base_order_names"]
            dirs = got["scan_dirs"]
            conflicts = _conflict_and_cellmap_scans(args, got["subset"], final, base, dirs)
            _lint_stage(args, got["subset"], final, base, dirs, got["subset_origins"])
            _resource_stage(args, got["custom_data_dirs"], dirs)
        else:
            _say(kind, text)
    if refused is not None:
        raise SystemExit(refused)
    assert got is not None  # noqa: S101 - no refusal, so a plan
    original_toml_data: dict[str, Any] = {}
    if args.customizations and getattr(args, "emit_toml", None):
        import tomllib

        original_toml_data = tomllib.loads(read_toml_text(args.customizations))
    got.pop("subset_origins")
    if got["data_result"] is not None:
        got["data_result"] = [(line, new, value) for line, new, value in got["data_result"]]
    got["custom_anchors"] = {k: (how, a) for k, (how, a) in got["custom_anchors"].items()}
    got["replace_dest_names"] = set(got["replace_dest_names"])
    got["curated_set"] = set(got["curated_set"])
    if getattr(args, "sort_data_paths", False):
        # the two lists hold the same inserts
        got["raw_toml_data_inserts"] = got["data_inserts"]
    return {
        "args": args,
        **got,
        "original_toml_data": original_toml_data,
        "conflicts": conflicts,
    }


def render_cfg_lines(
    plan: dict,
    final_order: list | None = None,
    data_order: list | None = None,
) -> list[str]:
    """The openmw.cfg a plan would write, without writing it (``ordercore::export``).

    What Export would put on disk for this order, used to hand the cell viewer the setup
    as it stands in the order panels.

    Args:
        plan: A computed plan.
        final_order: Enabled plugins in order; None uses the plan's own.
        data_order: Enabled raw data= lines in order; None uses the plan's own.

    Returns:
        The cfg's lines.
    """
    lines: list[str] = ordercore_bridge.require("order_render_cfg_lines")(
        _plain_plan(plan),
        None if final_order is None else list(final_order),
        None if data_order is None else list(data_order),
    )
    return lines


def viewer_setup_cfg(lines: Sequence[str], cfg_dir: Path) -> list[str]:
    """Make a rendered cfg stand on its own for the cell viewer.

    The viewer is handed a copy of the cfg in another folder, and OpenMW reads a
    relative ``data=`` path against the folder the cfg is in, so each relative path is
    made absolute against the real cfg's folder. Everything else is kept as it is.

    Args:
        lines: The cfg lines (e.g. from :func:`render_cfg_lines`).
        cfg_dir: The folder of the real openmw.cfg.

    Returns:
        The lines, with every ``data=`` path absolute.
    """
    out: list[str] = []
    for line in lines:
        value = extract_data_path_value(line)
        if value is not None and not Path(value).is_absolute():
            out.append(format_data_line(str((cfg_dir / value).resolve()), quoted=True))
        else:
            out.append(line)
    return out


#: What ``write_plan`` prints, as ``ordercore::plan::export`` words it (see ``_PLAN_MESSAGES``).
_WRITE_MESSAGES: Final = (
    "\n--- DRY RUN: no files written ---",
    "Backup written: %(path)s",
    "Wrote updated: %(path)s",
    "  openmw.cfg left untouched (pass --write-cfg to patch it directly)",
    "\n  DRY RUN: would write %(path)s\n%(toml)s",
    "  Wrote corrected customizations: %(path)s",
    "  Plugins sorted:        %(count)d",
    "  Data paths inserted:   %(count)d",
    "  Rule warnings raised:  %(count)d",
    "  plugin-order.yml warnings: %(count)d",
    "NOTE: '%(value)s' already present in data= list -- skipping duplicate insert.",
    "NOTE: '%(value)s' has no after/before anchor -- appending at end of data= list.",
    "WARNING: anchor '%(anchor)s' not found among existing data= paths for '%(value)s' -- "
    "appending at end instead.",
    "WARNING: anchor '%(anchor)s' in the emitted TOML matches %(count)d openmw.cfg lines -- "
    "momw-configurator errors on ambiguous matches. Colliding lines: %(lines)s%(more)s",
    "WARNING: remove entry '%(entry)s' matches %(count)d openmw.cfg lines -- momw-configurator "
    "removes ALL of them, silently. Colliding lines: %(lines)s%(more)s",
)


def _plain_plan(plan: dict) -> dict[str, Any]:
    """The plan as plain values (lists for its tuples and sets), for ``ordercore``."""
    data_result = plan.get("data_result")

    def inserts(name: str) -> list[dict[str, Any]]:
        return [
            {"value": d.get("value"), "after": d.get("after"), "before": d.get("before")}
            for d in plan.get(name) or []
        ]

    return {
        "lines": list(plan["lines"]),
        "content_positions": list(plan["content_positions"]),
        "data_positions": list(plan["data_positions"]),
        "final_order": list(plan.get("final_order") or []),
        "data_result": None if data_result is None else [list(r) for r in data_result],
        "new_groundcover": list(plan.get("new_groundcover") or []),
        "subset": list(plan.get("subset") or []),
        "curated_set": sorted(plan.get("curated_set") or ()),
        "base_order_names": list(plan.get("base_order_names") or []),
        "data_order": list(plan.get("data_order") or []),
        "data_inserts": inserts("data_inserts"),
        "raw_toml_data_inserts": inserts("raw_toml_data_inserts"),
        "orphan_data_paths": list(plan.get("orphan_data_paths") or []),
        "original_toml_data": plan.get("original_toml_data") or None,
        "original_content_values": dict(plan.get("original_content_values") or {}),
        "replace_dest_names": sorted(plan.get("replace_dest_names") or ()),
        "custom_anchors": {k: list(v) for k, v in (plan.get("custom_anchors") or {}).items()},
    }


def write_plan(
    args: argparse.Namespace,
    plan: dict,
    final_order: list | None = None,
    data_order: list | None = None,
    disabled_plugins: Collection[str] | None = None,
    disabled_data: Collection[str] | None = None,
) -> dict:
    """Write out a computed plan: the second half of a run (``ordercore::plan::export``).

    ``final_order`` / ``data_order`` override the plan's (orders the person changed: ENABLED
    rows only, ``data_order`` raw ``data=`` lines); ``disabled_plugins`` / ``disabled_data``
    are what they opted out of, given a ``removeContent`` / ``removeData`` where the curated
    list would bring them back. The report is printed as it comes.

    Returns:
        ``{"wrote_cfg": bool, "wrote_toml": bool}``.
    """
    emit = getattr(args, "emit_toml", None)
    items, wrote_cfg, wrote_toml, _toml_text, _cfg_lines, _user_vals = ordercore_bridge.require(
        "order_write_plan"
    )(
        {
            "write_cfg": bool(getattr(args, "write_cfg", False)),
            "cfg": str(args.cfg),
            "emit_toml": str(emit) if emit else None,
            "dry_run": bool(getattr(args, "dry_run", False)),
            "no_backup": bool(getattr(args, "no_backup", False)),
            "sort_data_paths": bool(getattr(args, "sort_data_paths", False)),
            "list_name": getattr(args, "list_name", None) or None,
            # Local clock: goes into a .bak filename the user reads.
            "stamp": datetime.now().strftime("%Y%m%d-%H%M%S"),  # noqa: DTZ005
        },
        _plain_plan(plan),
        (len(plan.get("predicate_warnings") or []), len(plan.get("yml_warnings") or [])),
        None if final_order is None else list(final_order),
        None if data_order is None else list(data_order),
        list(disabled_plugins or []),
        list(disabled_data or []),
    )
    for kind, text, _warning, _names in items:
        _say(kind, text, _WRITE_MESSAGES)
    return {"wrote_cfg": wrote_cfg, "wrote_toml": wrote_toml}


def run_from_args(args: argparse.Namespace) -> dict:
    """Do the actual work for a parsed args object.

    Comes from build_arg_parser(),
    or an equivalent argparse.Namespace/SimpleNamespace built by a caller
    such as the GUI). All progress/results are printed to stdout -- callers
    that want to capture them (e.g. the GUI) should redirect sys.stdout
    around this call rather than expect a return value with the log in it.

    This is just compute_plan() + write_plan() back to back, for callers
    (the CLI) that don't need to inspect/adjust the plan in between. A GUI
    that wants a manual-reorder step in between should call compute_plan()
    and write_plan() itself instead.

    Returns a small summary dict for programmatic callers:
      {"final_order": [...] or None, "predicate_warnings": [...],
       "data_result": [...] or None, "wrote_cfg": bool, "wrote_toml": bool}
    """
    plan = compute_plan(args)
    result = write_plan(args, plan)
    return {
        "final_order": plan["final_order"],
        "predicate_warnings": plan["predicate_warnings"],
        "data_result": plan["data_result"],
        "wrote_cfg": result["wrote_cfg"],
        "wrote_toml": result["wrote_toml"],
    }


def main() -> None:
    """Parse arguments, configure logging and tracing, and run."""
    args = build_arg_parser().parse_args()
    # Diagnostics go to stderr via logging (WARNING by default, more with -v);
    # the report the user asked for stays on stdout. See logging_setup.py.
    setup_logging(verbosity=getattr(args, "verbose", 0))
    tr = getattr(args, "trace", None)
    if tr:
        set_trace_file(tr if isinstance(tr, str) else "wraithguard_toolkit_trace.log")
        trace("CLI started")
    run_from_args(args)


if __name__ == "__main__":
    main()
