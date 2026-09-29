"""The ``ALCH`` record -- a potion. A port of ``types/alchemy.rs``.

The item strings (its icon carried in a ``TEXT`` subrecord, not ``ITEX``), a
twelve-byte ``ALDT`` block of weight, value and an auto-calculate flag, and a
list of ``ENAM`` effect blocks -- one per magic effect the potion has.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import AlchemyFlags, ObjectFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records.effect import Effect


@dataclass
class AlchemyData:
    """The ``ALDT`` block: weight, value and flags (12 bytes)."""

    weight: float = 0.0
    value: int = 0
    flags: AlchemyFlags = field(default_factory=lambda: AlchemyFlags(0))


@register
@dataclass
class Alchemy(Record):
    """An ``ALCH`` record."""

    TAG: ClassVar[bytes] = b"ALCH"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    effects: list[Effect] = field(default_factory=list)
    data: AlchemyData = field(default_factory=AlchemyData)
