"""The ``BSGN`` record -- a birthsign. A port of ``types/birthsign.rs``.

A name, a menu texture, a description, and the spells or abilities it grants.
Each granted spell is an ``NPCS`` subrecord holding a fixed thirty-two-byte id,
so the list is written with that fixed width and read back null-truncated.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

#: The fixed width of an ``NPCS`` spell-id field, in bytes.
_NPCS_SIZE = 32


@register
@dataclass
class Birthsign(Record):
    """A ``BSGN`` record."""

    TAG: ClassVar[bytes] = b"BSGN"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    texture: str = ""
    description: str = ""
    spells: list[str] = field(default_factory=list)
