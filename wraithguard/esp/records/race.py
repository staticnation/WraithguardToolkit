"""The ``RACE`` record -- a playable race. A port of ``types/race.rs``.

A race's name, its granted spells, its description, and a 140-byte ``RADT`` block:
seven skill bonuses, the male/female range of each of the eight attributes, the
male/female height and weight, and its flags (playable, beast). Each spell is a
fixed thirty-two-byte ``NPCS`` id.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import SkillId
from wraithguard.esp.flags import ObjectFlags, RaceFlags
from wraithguard.esp.record import Record, register

_SKILL_BONUSES = 7
_NPCS_SIZE = 32


@dataclass
class RaceData:
    """The ``RADT`` block: skill bonuses, attribute/size ranges, flags (140 bytes)."""

    skill_bonuses: tuple[tuple[SkillId, int], ...] = field(
        default_factory=lambda: tuple((SkillId.default(), 0) for _ in range(_SKILL_BONUSES))
    )
    strength: tuple[int, int] = (0, 0)
    intelligence: tuple[int, int] = (0, 0)
    willpower: tuple[int, int] = (0, 0)
    agility: tuple[int, int] = (0, 0)
    speed: tuple[int, int] = (0, 0)
    endurance: tuple[int, int] = (0, 0)
    personality: tuple[int, int] = (0, 0)
    luck: tuple[int, int] = (0, 0)
    height: tuple[float, float] = (0.0, 0.0)
    weight: tuple[float, float] = (0.0, 0.0)
    flags: RaceFlags = field(default_factory=lambda: RaceFlags(0))


@register
@dataclass
class Race(Record):
    """A ``RACE`` record."""

    TAG: ClassVar[bytes] = b"RACE"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    spells: list[str] = field(default_factory=list)
    description: str = ""
    data: RaceData = field(default_factory=RaceData)
