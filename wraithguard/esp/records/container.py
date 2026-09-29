"""The ``CONT`` record -- a container. A port of ``types/container.rs``.

A chest, barrel, corpse or sack: its model and script, a carry-weight capacity,
its flags (organic, respawns), and its contents. Each ``NPCO`` inventory entry is
a count paired with a fixed thirty-two-byte item id.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ContainerFlags, ObjectFlags
from wraithguard.esp.record import Record, register

_CNDT_SIZE = 4
_FLAG_SIZE = 4
_NPCO_SIZE = 36
_ITEM_ID_SIZE = 32


@register
@dataclass
class Container(Record):
    """A ``CONT`` record."""

    TAG: ClassVar[bytes] = b"CONT"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    encumbrance: float = 0.0
    container_flags: ContainerFlags = field(default_factory=lambda: ContainerFlags(0))
    inventory: list[tuple[int, str]] = field(default_factory=list)
