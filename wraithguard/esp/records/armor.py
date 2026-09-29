"""The ``ARMO`` record -- a piece of armor. A port of ``types/armor.rs``.

The item strings, a twenty-four-byte ``AODT`` block (type, weight, value, health,
enchantment points and armor rating), and a list of biped-object groups -- the
body slots the armor covers and their meshes.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import ArmorType
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records.bipedobject import BipedObject


@dataclass
class ArmorData:
    """The ``AODT`` block: type, weight, value, health, enchantment, rating (24 bytes)."""

    armor_type: ArmorType = field(default_factory=ArmorType.default)
    weight: float = 0.0
    value: int = 0
    health: int = 0
    enchantment: int = 0
    armor_rating: int = 0


@register
@dataclass
class Armor(Record):
    """An ``ARMO`` record."""

    TAG: ClassVar[bytes] = b"ARMO"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    enchanting: str = ""
    biped_objects: list[BipedObject] = field(default_factory=list)
    data: ArmorData = field(default_factory=ArmorData)
