"""The ``SPEL`` record -- a spell. A port of ``types/spell.rs``.

A castable spell (or a granted ability, disease or curse): a name, a twelve-byte
``SPDT`` block (its kind, its cost and its flags) and a list of ``ENAM`` effects,
the shared effect block.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import SpellType
from wraithguard.esp.flags import ObjectFlags, SpellFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records.effect import Effect


@dataclass
class SpellData:
    """The ``SPDT`` block: spell type, cost and flags (12 bytes)."""

    spell_type: SpellType = field(default_factory=SpellType.default)
    cost: int = 0
    flags: SpellFlags = field(default_factory=lambda: SpellFlags(0))


@register
@dataclass
class Spell(Record):
    """A ``SPEL`` record."""

    TAG: ClassVar[bytes] = b"SPEL"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    effects: list[Effect] = field(default_factory=list)
    data: SpellData = field(default_factory=SpellData)
