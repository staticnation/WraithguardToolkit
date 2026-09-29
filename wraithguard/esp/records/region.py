"""The ``REGN`` record -- a region. A port of ``types/region.rs``.

A named area of the world with its weather odds, its map colour, the creature
that ambushes a sleeper, and the ambient sounds it plays. The weather block comes
in two sizes -- eight bytes (no snow or blizzard, pre-Bloodmoon) or ten -- and is
always written as ten, matching the crate. Each ``SNAM`` sound is a fixed
thirty-two-byte id and a one-byte chance.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class WeatherChances:
    """The ``WEAT`` block: the percentage chance of each weather type."""

    clear: int = 0
    cloudy: int = 0
    foggy: int = 0
    overcast: int = 0
    rain: int = 0
    thunder: int = 0
    ash: int = 0
    blight: int = 0
    snow: int = 0
    blizzard: int = 0


@register
@dataclass
class Region(Record):
    """A ``REGN`` record."""

    TAG: ClassVar[bytes] = b"REGN"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    weather_chances: WeatherChances = field(default_factory=WeatherChances)
    sleep_creature: str = ""
    map_color: bytes = b"\x00\x00\x00\x00"
    sounds: list[tuple[str, int]] = field(default_factory=list)
