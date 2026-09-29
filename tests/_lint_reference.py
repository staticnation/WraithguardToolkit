"""The Python lint helpers that native/src/lint.rs replaced, kept as the parity reference.

Verbatim in substance (docstrings stripped); see ``test_lint_native_parity.py``.
The tables (evil GMSTs, expansion functions, skip lists) are still the live ones
in ``wraithguard_toolkit``.
"""

# ruff: noqa
# fmt: off
from __future__ import annotations

import re
import struct
from typing import Callable, Iterator, NamedTuple

from wraithguard_toolkit import _BLOODMOON_FUNCS, _EVIL_GMSTS, _LINT_SKIP_CELLS, _TRIBUNAL_FUNCS

def _iter_tes3_records(raw: bytes) -> Iterator[tuple[bytes, bytes]]:
    (n, i) = (len(raw), 0)
    while i + 16 <= n:
        tag = bytes(raw[i:i + 4])
        (sz,) = struct.unpack_from('<I', raw, i + 4)
        yield (tag, raw[i + 16:i + 16 + sz])
        i += 16 + sz


def _iter_subrecords(body: bytes) -> Iterator[tuple[bytes, bytes]]:
    (n, i) = (len(body), 0)
    while i + 8 <= n:
        tag = bytes(body[i:i + 4])
        (sz,) = struct.unpack_from('<I', body, i + 4)
        yield (tag, body[i + 8:i + 8 + sz])
        i += 8 + sz


def _lint_zstr(b: bytes) -> str:
    return b.split(b'\x00', 1)[0].decode('latin-1', 'replace').strip()


class _CellFacts(NamedTuple):
    name: str
    cell_id: str
    fog_bug: bool


_RE_TB_FUN = re.compile('^[^;\\n]*?\\b(' + '|'.join(_TRIBUNAL_FUNCS) + ')\\b', re.IGNORECASE | re.MULTILINE)


_RE_BM_FUN = re.compile('^[^;\\n]*?\\b(' + '|'.join(_BLOODMOON_FUNCS) + ')\\b', re.IGNORECASE | re.MULTILINE)


def _lint_expansion_calls(body: bytes, tag: bytes) -> tuple[set[str], set[str]]:
    want = b'SCTX' if tag == b'SCPT' else b'BNAM'
    tribunal: set[str] = set()
    bloodmoon: set[str] = set()
    for (subtag, data) in _iter_subrecords(body):
        if subtag == want and data:
            text = data.decode('latin-1', 'replace')
            tribunal.update((match.group(1) for match in _RE_TB_FUN.finditer(text)))
            bloodmoon.update((match.group(1) for match in _RE_BM_FUN.finditer(text)))
    return (tribunal, bloodmoon)


def _lint_masters(body: bytes) -> set[str]:
    return {_lint_zstr(data).lower() for (subtag, data) in _iter_subrecords(body) if subtag == b'MAST'}


def _lint_header_gaps(body: bytes) -> list[str]:
    for (subtag, data) in _iter_subrecords(body):
        if subtag == b'HEDR' and len(data) >= 296:
            fields = (('author', _lint_zstr(data[8:40])), ('description', _lint_zstr(data[40:296])))
            return [word for (word, value) in fields if not value]
    return []


def _lint_evil_gmst(body: bytes) -> str | None:
    name: str | None = None
    value_tag: str | None = None
    value = b''
    for (subtag, data) in _iter_subrecords(body):
        if subtag == b'NAME':
            name = _lint_zstr(data).lower()
        elif subtag in (b'STRV', b'INTV', b'FLTV'):
            (value_tag, value) = (subtag.decode(), data)
    known = _EVIL_GMSTS.get(name) if name else None
    if known and value_tag == known[0] and (value.rstrip(b'\x00') == known[1].rstrip(b'\x00')):
        return name
    return None


def _lint_cell(body: bytes) -> _CellFacts | None:
    name = ''
    data: bytes | None = None
    ambience: bytes | None = None
    for (subtag, payload) in _iter_subrecords(body):
        if subtag == b'NAME':
            name = _lint_zstr(payload)
        elif subtag == b'DATA' and data is None:
            data = payload
        elif subtag == b'AMBI':
            ambience = payload
    if data is None or len(data) < 12:
        return None
    (flags,) = struct.unpack_from('<I', data, 0)
    if not flags & 1:
        return None
    cell_id = name.lower()
    fog_bug = False
    if not flags & 128:
        if ambience is not None and len(ambience) == 16:
            (fog,) = struct.unpack_from('<f', ambience, 12)
        else:
            (fog,) = struct.unpack_from('<f', data, 8)
        fog_bug = fog == 0.0
    return _CellFacts(name=name, cell_id='' if cell_id in _LINT_SKIP_CELLS else cell_id, fog_bug=fog_bug)


def _lint_interior_pathgrid(body: bytes) -> str | None:
    name = ''
    grid_x: int | None = None
    grid_y: int | None = None
    for (subtag, data) in _iter_subrecords(body):
        if subtag == b'NAME':
            name = _lint_zstr(data)
        elif subtag == b'DATA' and len(data) >= 8:
            (grid_x, grid_y) = struct.unpack_from('<ii', data, 0)
    if grid_x == 0 and grid_y == 0 and name:
        return name.lower()
    return None


def _lint_one_plugin(raw: bytes, plugin: str, *, is_custom: bool, tagfor: Callable[[str], str], interior_first: dict[str, tuple[str, str]], pathgrids: set[str]) -> list[str]:
    warnings: list[str] = []
    evil_gmsts: list[str] = []
    masters: set[str] = set()
    tribunal: set[str] = set()
    bloodmoon: set[str] = set()
    for (tag, body) in _iter_tes3_records(raw):
        if is_custom and tag in (b'SCPT', b'INFO'):
            (found_tribunal, found_bloodmoon) = _lint_expansion_calls(body, tag)
            tribunal |= found_tribunal
            bloodmoon |= found_bloodmoon
        elif tag == b'TES3':
            masters |= _lint_masters(body)
            missing = _lint_header_gaps(body) if is_custom else []
            if missing:
                warnings.append(f"[HEADER] '{plugin}'{tagfor(plugin)}: header has no {' and no '.join(missing)}.")
        elif tag == b'GMST':
            evil = _lint_evil_gmst(body)
            if evil:
                evil_gmsts.append(evil)
        elif tag == b'CELL':
            cell = _lint_cell(body)
            if cell is None:
                continue
            if cell.cell_id and cell.cell_id not in interior_first:
                interior_first[cell.cell_id] = (plugin, cell.name)
            if cell.fog_bug:
                warnings.append(f"[FOGBUG] '{plugin}'{tagfor(plugin)}: interior cell '{cell.name}' has fog density 0.0 -- renders as a black void on some GPUs. Fix by setting any nonzero fog density on the cell.")
        elif tag == b'PGRD':
            interior = _lint_interior_pathgrid(body)
            if interior:
                pathgrids.add(interior)
    if evil_gmsts:
        warnings.append(f"[EVLGMST] '{plugin}'{tagfor(plugin)}: {len(evil_gmsts)} evil GMST(s): {', '.join(sorted(evil_gmsts))} -- stale expansion defaults copied in by an old Construction Set; tes3cmd clean removes them.")
    if tribunal and 'tribunal.esm' not in masters and ('bloodmoon.esm' not in masters):
        warnings.append(f"[EXP-DEP] '{plugin}'{tagfor(plugin)}: scripts use Tribunal function(s) {', '.join(sorted(tribunal))} but the plugin doesn't master Tribunal.esm -- fragile on non-expansion setups (tes3lint !TB-FUN).")
    if bloodmoon and 'bloodmoon.esm' not in masters:
        warnings.append(f"[EXP-DEP] '{plugin}'{tagfor(plugin)}: scripts use Bloodmoon function(s) {', '.join(sorted(bloodmoon))} but the plugin doesn't master Bloodmoon.esm -- fragile on non-expansion setups (tes3lint !BM-FUN).")
    return warnings
