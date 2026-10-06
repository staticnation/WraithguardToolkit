"""The records a script compile looks names up in.

The compiler resolves every bare word it meets against the load order: a global
variable, an object ID, a script and its locals (for ``object.variable``), a
journal topic. :class:`RecordIndex` holds just what it needs to know of each
record, keyed case-insensitively the way the game and MWEdit look IDs up, and
:func:`index_plugins` builds one from plugin files.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Final

from wraithguard.mwscript.script_record import _iter_records, _iter_subrecords

if TYPE_CHECKING:
    from collections.abc import Iterable

#: Record types an item can be carried as (MWEdit's ``IsESMRecordCarryable``).
CARRYABLE: Final[frozenset[str]] = frozenset(
    {
        "ALCH",
        "APPA",
        "ARMO",
        "BOOK",
        "CLOT",
        "INGR",
        "LIGH",
        "LOCK",
        "MISC",
        "PROB",
        "REPA",
        "WEAP",
        "LEVI",
    }
)

#: ``DIAL`` type byte of a journal.
DIAL_JOURNAL: Final = 4


@dataclass(slots=True)
class Record:
    """What the compiler needs of one record.

    Attributes:
        type: The four-letter record type (``NPC_``, ``SCPT``...).
        id: The ID as the record spells it.
        script: For objects, the ``SCRI`` script attached, if any.
        dial_type: For ``DIAL``, its type byte (4 is a journal).
        glob_type: For ``GLOB``, ``s``, ``l`` or ``f``.
        locals: For ``SCPT``, its local names by type ``s``, ``l``, ``f``.
    """

    type: str
    id: str
    script: str = ""
    dial_type: int = -1
    glob_type: str = ""
    locals: dict[str, list[str]] = field(default_factory=dict)

    def find_local(self, name: str) -> tuple[int, str]:
        """A script's local: its 1-based index within its type, and the type.

        Returns ``(-1, "")`` when the script declares no such local.
        """
        low = name.lower()
        for kind in ("s", "l", "f"):
            for index, local in enumerate(self.locals.get(kind, ())):
                if local.lower() == low:
                    return index + 1, kind
        return -1, ""


class RecordIndex:
    """Records by ID, case-insensitive; a later plugin's record replaces an earlier one."""

    def __init__(self) -> None:
        """Start empty."""
        self._by_id: dict[str, Record] = {}
        self._by_type: dict[tuple[str, str], Record] = {}

    def add(self, record: Record) -> None:
        """Add (or replace) a record."""
        low = record.id.lower()
        self._by_id[low] = record
        self._by_type[(record.type, low)] = record

    def find(self, rec_id: str) -> Record | None:
        """The record with this ID, whatever its type."""
        return self._by_id.get(rec_id.lower())

    def find_type(self, rec_id: str, rec_type: str) -> Record | None:
        """The record of this type with this ID."""
        found = self._by_id.get(rec_id.lower())
        if found is not None and found.type == rec_type:
            return found
        return self._by_type.get((rec_type, rec_id.lower()))

    def find_carryable(self, rec_id: str) -> Record | None:
        """An item record with this ID."""
        found = self._by_id.get(rec_id.lower())
        if found is not None and found.type in CARRYABLE:
            return found
        for kind in CARRYABLE:
            hit = self._by_type.get((kind, rec_id.lower()))
            if hit is not None:
                return hit
        return None

    def get_global(self, name: str) -> Record | None:
        """The ``GLOB`` record with this ID."""
        return self.find_type(name, "GLOB")

    def __len__(self) -> int:
        """Number of distinct IDs."""
        return len(self._by_id)


def _cstr(raw: bytes) -> str:
    """A NUL-terminated latin-1 string."""
    return raw.split(b"\x00", 1)[0].decode("latin-1")


def records_from_bytes(data: bytes) -> list[Record]:
    """The records of one plugin's bytes that a compile can refer to."""
    out: list[Record] = []
    if data[:4] != b"TES3":
        return out
    for tag, body in _iter_records(data):
        rec_type = tag.decode("latin-1")
        if rec_type in ("TES3", "INFO", "LAND", "PGRD"):
            continue
        rec = Record(rec_type, "")
        names: dict[str, list[str]] = {}
        counts = (0, 0, 0)
        named = False
        for sub, payload in _iter_subrecords(body):
            if sub == b"NAME" and not named:
                # The first NAME only: a cell's references carry NAMEs of their own.
                rec.id, named = _cstr(payload), True
            elif sub == b"SCHD" and len(payload) >= 44:
                rec.id = _cstr(payload[:32])
                counts = struct.unpack_from("<3I", payload, 32)
            elif sub == b"SCVR":
                names["all"] = [n.decode("latin-1") for n in payload.split(b"\x00") if n]
            elif sub == b"SCRI":
                rec.script = _cstr(payload)
            elif sub == b"DATA" and rec_type == "DIAL" and payload:
                rec.dial_type = payload[0]
            elif sub == b"FNAM" and rec_type == "GLOB" and payload:
                rec.glob_type = chr(payload[0]).lower()
        if rec_type == "SCPT":
            flat = names.get("all", [])
            ns, nl, _nf = counts
            rec.locals = {"s": flat[:ns], "l": flat[ns : ns + nl], "f": flat[ns + nl :]}
        if rec.id or rec_type == "CELL":
            out.append(rec)
    return out


def index_plugins(paths: Iterable[Path | str]) -> RecordIndex:
    """Index the records of plugins in load order (later ones win)."""
    index = RecordIndex()
    for path in paths:
        try:
            data = Path(path).read_bytes()
        except OSError:
            continue
        for rec in records_from_bytes(data):
            index.add(rec)
    return index
