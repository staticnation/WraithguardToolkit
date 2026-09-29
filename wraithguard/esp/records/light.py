"""The ``LIGH`` record -- a light. A port of ``types/light.rs``.

The item strings, an optional carry ``SNAM`` sound, and a twenty-four-byte
``LHDT`` block: weight, value, burn time, radius, an RGBA colour, and the light's
flags (dynamic, carriable, flickering, and so on).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import LightFlags, ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class LightData:
    """The ``LHDT`` block: weight, value, time, radius, colour, flags (24 bytes)."""

    weight: float = 0.0
    value: int = 0
    time: int = 0
    radius: int = 0
    color: bytes = b"\x00\x00\x00\x00"
    flags: LightFlags = field(default_factory=lambda: LightFlags(0))


@register
@dataclass
class Light(Record):
    """A ``LIGH`` record."""

    TAG: ClassVar[bytes] = b"LIGH"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    sound: str = ""
    data: LightData = field(default_factory=LightData)
