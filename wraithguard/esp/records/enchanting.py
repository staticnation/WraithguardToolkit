"""The ``ENCH`` record -- an enchantment. A port of ``types/enchanting.rs``.

The enchantment carried by an enchanted item: a sixteen-byte ``ENDT`` block (how
it is cast, its cost, its charge and its flags) and a list of ``ENAM`` effects,
the same shared effect block potions and spells use.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import EnchantType
from wraithguard.esp.flags import EnchantingFlags, ObjectFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records.effect import Effect


@dataclass
class EnchantingData:
    """The ``ENDT`` block: cast type, cost, max charge and flags (16 bytes)."""

    enchant_type: EnchantType = field(default_factory=EnchantType.default)
    cost: int = 0
    max_charge: int = 0
    flags: EnchantingFlags = field(default_factory=lambda: EnchantingFlags(0))


@register
@dataclass
class Enchanting(Record):
    """An ``ENCH`` record."""

    TAG: ClassVar[bytes] = b"ENCH"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    effects: list[Effect] = field(default_factory=list)
    data: EnchantingData = field(default_factory=EnchantingData)
