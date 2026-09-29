"""The ``CLOT`` record -- a piece of clothing. A port of ``types/clothing.rs``.

The same shape as armor, with a smaller twelve-byte ``CTDT`` block (type, weight,
value and enchantment points) and the same list of biped-object groups for the
body slots it covers.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import ClothingType
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records.bipedobject import BipedObject


@dataclass
class ClothingData:
    """The ``CTDT`` block: type, weight, value and enchantment (12 bytes)."""

    clothing_type: ClothingType = field(default_factory=ClothingType.default)
    weight: float = 0.0
    value: int = 0
    enchantment: int = 0


@register
@dataclass
class Clothing(Record):
    """A ``CLOT`` record."""

    TAG: ClassVar[bytes] = b"CLOT"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    enchanting: str = ""
    biped_objects: list[BipedObject] = field(default_factory=list)
    data: ClothingData = field(default_factory=ClothingData)
