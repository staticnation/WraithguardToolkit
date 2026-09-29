"""The lint's Rust record scan against the Python it replaced (native/src/lint.rs).

``_lint_one_plugin`` now reads each plugin through ``PluginLinter``; the Python
``_lint_*`` helpers it used are kept in ``_lint_reference.py``. Both must give
the same warnings, in the same order, and leave the load-order accumulators
(``interior_first``, ``pathgrids``) in the same state -- on hand-built plugins
that hit every edge (truncated records, duplicate subrecords, Latin-1 names,
comment and word-boundary rules in script text) and, when a corpus is at hand,
on real plugins.
"""

from __future__ import annotations

import os
import random
import struct
from pathlib import Path

import pytest
from conftest import rec, sub, zstr

pytest.importorskip("wraithguard_native")

import wraithguard_toolkit as core
from tests import _lint_reference as ref


def _tag(plugin: str) -> str:
    return f" [{plugin[:3]}]"


def _both(raw: bytes, *, is_custom: bool, name: str = "Test.esp") -> None:
    old_first: dict[str, tuple[str, str]] = {}
    old_grids: set[str] = set()
    new_first: dict[str, tuple[str, str]] = {}
    new_grids: set[str] = set()
    old = ref._lint_one_plugin(
        raw, name, is_custom=is_custom, tagfor=_tag, interior_first=old_first, pathgrids=old_grids
    )
    new = core._lint_one_plugin(
        raw, name, is_custom=is_custom, tagfor=_tag, interior_first=new_first, pathgrids=new_grids
    )
    assert new == old
    assert new_first == old_first
    assert list(new_first) == list(old_first)
    assert new_grids == old_grids


def _header(author: str = "", description: str = "", masters: tuple[str, ...] = ()) -> bytes:
    hedr = struct.pack("<fi", 1.3, 0) + zstr(author, 32) + zstr(description, 256) + bytes(4)
    body = sub("HEDR", hedr)
    for master in masters:
        body += sub("MAST", zstr(master)) + sub("DATA", bytes(8))
    return rec("TES3", body)


def _cell(
    name: str, flags: int, fog: float, ambi: float | None = None, extra: bytes = b""
) -> bytes:
    body = sub("NAME", zstr(name)) + sub("DATA", struct.pack("<Iif", flags, 0, fog)) + extra
    if ambi is not None:
        body += sub("AMBI", struct.pack("<IIIf", 0, 0, 0, ambi))
    return rec("CELL", body)


def _script(text: str, tag: str = "SCPT") -> bytes:
    field = "SCTX" if tag == "SCPT" else "BNAM"
    return rec(tag, sub("SCHD", bytes(52)) + sub(field, text.encode("latin-1")))


SCRIPTS = [
    "begin x\nPlaceItem gold_001 1 0 0 0\nend",
    "placeitemcell foo\n; ForceJump in a comment\nset a to GetWaterLevel ; ExplodeSpell",
    "x = 1;BecomeWerewolf\r\nIsWerewolf\r\n  \tturnmoonred",
    "GetPCJumpingX PlaceItem_ foo_PlaceItem éGetScale GetScaleé ²GetScale GetScale½",
    "PlaceAtMe PlaceAtMe placeatme\nPLACEATME\nGetSquareRoot GetScale",
    "\n\n;only comments\n",
    "ModScale;ModScale\nSetScale SetDelete SetScale",
    "\x00\x00GetScale\x00",
    "ªGetScale µGetScale \xd7GetScale \xf7GetScale\xa0GetScale\x85IsWerewolf",
    "a;b\nc GetWeaponType\n",
]


def _synthetic() -> list[tuple[bytes, bool]]:
    evil_name, (evil_tag, evil_value) = next(iter(core._EVIL_GMSTS.items()))
    plugins: list[bytes] = [
        _header("Me", "Desc", ("Morrowind.esm",)),
        _header("", "", ("Morrowind.esm", "Tribunal.esm")) + _header(" ", "x"),
        _header(" \x1c\t", "\xa0\x85", ("BLOODMOON.ESM ",)),
        rec("TES3", sub("HEDR", bytes(100))),
        _header()
        + rec("GMST", sub("NAME", zstr(evil_name)) + sub(evil_tag, evil_value))
        + rec(
            "GMST", sub("NAME", zstr(evil_name.upper())) + sub(evil_tag, evil_value + b"\x00\x00")
        )
        + rec("GMST", sub("NAME", zstr(evil_name)) + sub(evil_tag, b"\x01\x02\x03\x04"))
        + rec("GMST", sub("NAME", zstr(evil_name)) + sub("STRV", evil_value))
        + rec("GMST", sub("NAME", zstr("")) + sub(evil_tag, evil_value))
        + rec("GMST", sub(evil_tag, evil_value) + sub("NAME", zstr(evil_name))),
        _header("a", "b")
        + _cell("Vault", 1, 0.0)
        + _cell("Vault", 1, 0.5)
        + _cell("VAULT two", 1, 0.5, ambi=0.0)
        + _cell("Lit", 1, 0.0, ambi=0.3)
        + _cell("Outside", 0, 0.0)
        + _cell("Like Ext", 1 | 128, 0.0)
        + _cell("Ashlands Region (0, 0)", 1, 0.0)
        + _cell("Neg zero", 1, -0.0)
        + _cell("Short ambi", 1, 0.0, extra=sub("AMBI", bytes(12)))
        + _cell("Ç Ümlaut Hall", 1, 1.0)
        + _cell("Two data", 1, 1.0, extra=sub("DATA", struct.pack("<Iif", 0, 0, 0.0)))
        + rec("CELL", sub("NAME", zstr("tiny")) + sub("DATA", bytes(8)))
        + rec("CELL", sub("DATA", struct.pack("<Iif", 1, 0, 0.0)))
        + rec(
            "PGRD", sub("NAME", zstr("Vault")) + sub("DATA", struct.pack("<iihBB", 0, 0, 0, 0, 0))
        )
        + rec("PGRD", sub("NAME", zstr("Ç ÜMLAUT HALL")) + sub("DATA", bytes(12)))
        + rec("PGRD", sub("NAME", zstr("Ext")) + sub("DATA", struct.pack("<ii", 3, -1)))
        + rec("PGRD", sub("NAME", zstr("")) + sub("DATA", bytes(8)))
        + rec("PGRD", sub("NAME", zstr("Short")) + sub("DATA", bytes(4))),
        _header("x", "y", ("morrowind.esm",))
        + b"".join(_script(s) for s in SCRIPTS)
        + b"".join(_script(s, "INFO") for s in SCRIPTS[:4])
        + rec("SCPT", sub("SCTX", b"")),
        _header("x", "y", ("Tribunal.esm",)) + _script(SCRIPTS[0]) + _script(SCRIPTS[2]),
        # Truncated: the last record claims more than the file holds.
        _header("a", "b") + _cell("Cut", 1, 0.0)[:-3],
        _header("a", "b") + struct.pack("<4sIII", b"CELL", 9999, 0, 0) + sub("NAME", zstr("Big")),
        _header("a", "b") + b"CELL\x05\x00",
    ]
    return [(raw, custom) for raw in plugins for custom in (True, False)]


@pytest.mark.parametrize(("raw", "is_custom"), _synthetic())
def test_synthetic_plugins_match(raw: bytes, is_custom: bool) -> None:
    _both(raw, is_custom=is_custom)


def test_random_script_text_matches() -> None:
    rng = random.Random(1512)  # noqa: S311 - test data, not security
    words = [*core._TRIBUNAL_FUNCS, *core._BLOODMOON_FUNCS, "set", "x", "é", "_", "1"]
    seps = [" ", "\n", ";", "\r\n", "\t", "", ".", "é", "\xa0", "²"]
    for _ in range(300):
        text = "".join(
            (rng.choice(words) if rng.random() < 0.6 else rng.choice(words).swapcase())
            + rng.choice(seps)
            for _ in range(rng.randint(1, 30))
        )
        _both(_header("a", "b") + _script(text), is_custom=True)


def _corpus() -> list[Path]:
    listing = os.environ.get("WG_PLUGIN_CORPUS")
    if not listing:
        return []
    root = Path(listing)
    if root.is_file():  # a list of plugin paths, one per line
        return [Path(line.strip()) for line in root.read_text().splitlines() if line.strip()]
    return sorted(p for p in root.rglob("*") if p.suffix.lower() in (".esp", ".esm", ".omwaddon"))


@pytest.mark.skipif(not _corpus(), reason="set WG_PLUGIN_CORPUS to a folder or a list of plugins")
def test_corpus_matches() -> None:
    for path in _corpus():
        raw = path.read_bytes()
        if raw[:4] != b"TES3":
            continue
        _both(raw, is_custom=True, name=path.name)
