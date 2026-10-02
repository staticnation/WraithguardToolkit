"""Scripts registered inside content files: OpenMW's ``LUAL`` records.

An ``.omwaddon`` can register Lua scripts itself instead of shipping an ``.omwscripts``
file. The layout, from OpenMW's ``components/esm/luascripts.cpp``
(``LuaScriptsCfg::load``): a ``LUAL`` record holds, per script, ``LUAS`` (its VFS
path), ``LUAF`` (a ``uint32`` of flags, then one ``uint32`` record type per object
type it attaches to), an optional ``LUAD`` (initialization data), then any number of
``LUAR`` (attach to one record) and ``LUAI`` (attach to one reference) subrecords,
each optionally followed by its own ``LUAD``.

The records are read straight from the file (TES3 framing: a 16-byte record header,
then 8-byte subrecord headers), so nothing else in the plugin is parsed.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

from wraithguard.lua.omwscripts import OmwScripts, ScriptEntry

if TYPE_CHECKING:
    from pathlib import Path

#: LuaScriptCfg flag bits -> the .omwscripts flag they mean.
_FLAG_BITS = ((1 << 0, "GLOBAL"), (1 << 1, "CUSTOM"), (1 << 2, "PLAYER"), (1 << 4, "MENU"))
_LOAD_BIT = 1 << 5
#: ESM::RecNameInts (the record's four-character tag) -> the .omwscripts flag.
_TYPE_FLAGS = {
    b"ACTI": "ACTIVATOR",
    b"ALCH": "POTION",
    b"APPA": "APPARATUS",
    b"ARMO": "ARMOR",
    b"BOOK": "BOOK",
    b"CLOT": "CLOTHING",
    b"CONT": "CONTAINER",
    b"CREA": "CREATURE",
    b"DOOR": "DOOR",
    b"INGR": "INGREDIENT",
    b"LIGH": "LIGHT",
    b"LOCK": "LOCKPICK",
    b"MISC": "MISC_ITEM",
    b"NPC_": "NPC",
    b"PROB": "PROBE",
    b"REPA": "REPAIR",
    b"WEAP": "WEAPON",
}
_REC = struct.Struct("<4sI4xI")  # tag, data size, (unused), flags
_SUB = struct.Struct("<4sI")  # tag, data size


def _flags(word: int, types: list[bytes], per_object: bool) -> tuple[str, ...]:
    """A LUAF flags word and type list as ``.omwscripts`` flags.

    Args:
        word: The ``uint32`` flags.
        types: The four-character record tags it attaches to.
        per_object: It has ``LUAR``/``LUAI`` attachments (to particular records or
            references), which make it a local script.

    Returns:
        The flags, in a stable order.
    """
    out = [name for bit, name in _FLAG_BITS if word & bit]
    if word & _LOAD_BIT:
        out.append("LOAD")
    out.extend(_TYPE_FLAGS.get(t, t.decode("latin-1").strip("\0 _")) for t in types)
    if per_object and "CUSTOM" not in out:
        out.append("CUSTOM")
    return tuple(dict.fromkeys(out))


def _parse_lual(data: bytes, name: str, out: OmwScripts) -> None:
    """Read one LUAL record's scripts into ``out``.

    Args:
        data: The record's data (its subrecords).
        name: The content file, for the entries' source.
        out: Where the entries and problems go.
    """
    subs: list[tuple[bytes, bytes]] = []
    pos = 0
    while pos + _SUB.size <= len(data):
        tag, size = _SUB.unpack_from(data, pos)
        pos += _SUB.size
        subs.append((tag, data[pos : pos + size]))
        pos += size
    i = 0
    while i < len(subs):
        tag, body = subs[i]
        i += 1
        if tag != b"LUAS":
            continue
        path = body.split(b"\0", 1)[0].decode("utf-8", errors="replace")
        word = 0
        types: list[bytes] = []
        per_object = False
        if i < len(subs) and subs[i][0] == b"LUAF" and len(subs[i][1]) >= 4:
            luaf = subs[i][1]
            word = struct.unpack_from("<I", luaf)[0]
            types = [luaf[k : k + 4] for k in range(4, len(luaf) - 3, 4)]
            i += 1
        else:
            out.problems.append((0, f"script {path!r} has no LUAF flags"))
        while i < len(subs) and subs[i][0] in (b"LUAD", b"LUAR", b"LUAI"):
            per_object = per_object or subs[i][0] != b"LUAD"
            i += 1
        flags = _flags(word, types, per_object)
        if not flags:
            out.problems.append((0, f"script {path!r} has no flags (it never starts)"))
            continue
        out.entries.append(ScriptEntry(path=path, flags=flags, source=name, line=0))


def read_lual(path: Path, name: str) -> OmwScripts:
    """The scripts a content file registers in its LUAL records.

    Args:
        path: The file.
        name: Its name as the cfg gives it.

    Returns:
        The scripts, in record order (``line`` is 0: a record has none), and any that
        could not be read.
    """
    out = OmwScripts(name=name)
    try:
        data = path.read_bytes()
    except OSError as exc:
        out.problems.append((0, f"cannot read: {exc}"))
        return out
    pos = 0
    while pos + _REC.size <= len(data):
        tag, size, _ = _REC.unpack_from(data, pos)
        body_start = pos + _REC.size
        if tag == b"LUAL":
            _parse_lual(data[body_start : body_start + size], name, out)
        pos = body_start + size
    return out
