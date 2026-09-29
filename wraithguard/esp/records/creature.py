"""The ``CREA`` record -- a creature. A port of ``types/creature.rs``.

The creature counterpart of the NPC, sharing its inventory, spells and AI
machinery. It differs in its data: a fixed ninety-six-byte ``NPDT`` of the
creature's type, level, attributes, pools, soul value, the AI weights and three
attack ranges; a sound (in ``CNAM``); and an optional scale (``XSCL``) that the
crate clamps to 0.5..2.0 and omits when it is the default 1.0 -- reproduced here.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import CreatureType
from wraithguard.esp.flags import CreatureFlags, ObjectFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records._ai import (
    AiData,
    AiPackage,
    TravelDestination,
)

_AIDT_SIZE = 12
_DODT_SIZE = 24
_FLAG_SIZE = 4
_NPCO_SIZE = 36
_ITEM_ID_SIZE = 32
_SPELL_SIZE = 32
_CREATURE_FLAG_MASK = 0xFF
_BLOOD_SHIFT = 10
_BLOOD_MASK = 0b111
_SCALE_MIN = 0.5
_SCALE_MAX = 2.0
_SCALE_EPSILON = 1e-6


@dataclass
class CreatureData:
    """The ``NPDT`` block: type, level, stats, pools, soul, AI weights, attacks."""

    creature_type: CreatureType = field(default_factory=CreatureType.default)
    level: int = 0
    strength: int = 0
    intelligence: int = 0
    willpower: int = 0
    agility: int = 0
    speed: int = 0
    endurance: int = 0
    personality: int = 0
    luck: int = 0
    health: int = 0
    magicka: int = 0
    fatigue: int = 0
    soul: int = 0
    combat: int = 0
    magic: int = 0
    stealth: int = 0
    attack1: tuple[int, int] = (0, 0)
    attack2: tuple[int, int] = (0, 0)
    attack3: tuple[int, int] = (0, 0)
    gold: int = 0


def _unpack_flags(value: int) -> tuple[CreatureFlags, int]:
    """Split the ``FLAG`` word into creature flags (low byte) and blood type."""
    return CreatureFlags(value & _CREATURE_FLAG_MASK), (value >> _BLOOD_SHIFT) & _BLOOD_MASK


def _pack_flags(creature_flags: CreatureFlags, blood_type: int) -> int:
    """Fold creature flags and blood type back into the ``FLAG`` word."""
    return int(creature_flags) | ((blood_type & _BLOOD_MASK) << _BLOOD_SHIFT)


@register
@dataclass
class Creature(Record):
    """A ``CREA`` record."""

    TAG: ClassVar[bytes] = b"CREA"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    sound: str = ""
    scale: float | None = None
    creature_flags: CreatureFlags = field(default_factory=lambda: CreatureFlags(0))
    blood_type: int = 0
    inventory: list[tuple[int, str]] = field(default_factory=list)
    spells: list[str] = field(default_factory=list)
    ai_data: AiData = field(default_factory=AiData)
    ai_packages: list[AiPackage] = field(default_factory=list)
    travel_destinations: list[TravelDestination] = field(default_factory=list)
    data: CreatureData = field(default_factory=CreatureData)
