"""The ``APPA`` record -- an alchemy apparatus. A port of ``types/apparatus.rs``.

A mortar, alembic, calcinator or retort: the item strings plus a sixteen-byte
``AADT`` block of its type, quality, weight and value.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import ApparatusType
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class ApparatusData:
    """The ``AADT`` block: an apparatus's type, quality, weight and value (16 bytes)."""

    apparatus_type: ApparatusType = ApparatusType.MortarAndPestle
    quality: float = 0.0
    weight: float = 0.0
    value: int = 0


@register
@dataclass
class Apparatus(Record):
    """An ``APPA`` record."""

    TAG: ClassVar[bytes] = b"APPA"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    data: ApparatusData = field(default_factory=ApparatusData)
