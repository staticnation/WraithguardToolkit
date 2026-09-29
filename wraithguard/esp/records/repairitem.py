"""The ``REPA`` record -- a repair item. A port of ``types/repairitem.rs``.

A repair hammer or prong: the item strings plus a sixteen-byte ``RIDT`` block.
Its block orders uses before quality, unlike the lockpick's and probe's -- the
one thing that distinguishes them on the wire.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class RepairItemData:
    """The ``RIDT`` block: weight, value, uses and quality (16 bytes)."""

    weight: float = 0.0
    value: int = 0
    uses: int = 0
    quality: float = 0.0


@register
@dataclass
class RepairItem(Record):
    """A ``REPA`` record."""

    TAG: ClassVar[bytes] = b"REPA"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    data: RepairItemData = field(default_factory=RepairItemData)
