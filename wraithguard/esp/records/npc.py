"""The ``NPC_`` record -- a non-player character. A port of ``types/npc.rs``.

The most involved record: a name, race, class, faction, head and hair meshes and
a script; an ``NPDT`` stats block that comes in two forms (full stats, or a short
auto-calculated one); a packed ``FLAG`` word that folds the NPC flags together
with a three-bit blood type; an inventory and a spell list; and the shared AI
data, AI packages and travel destinations. Every piece round-trips byte-for-byte.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import NpcFlags, ObjectFlags
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
#: The bits ``NpcFlags`` defines; the ``FLAG`` word's low byte, truncated to
#: these, is the flags, and bits 10-12 are the blood type (as the crate does).
_NPC_FLAG_MASK = 0x1F
_BLOOD_SHIFT = 10
_BLOOD_MASK = 0b111

_ATTRS_SIZE = 8
_SKILLS_SIZE = 27


@dataclass
class NpcStats:
    """The full ``NPDT`` stats: attributes, skills and the derived pools (42 bytes)."""

    attributes: bytes = field(default_factory=lambda: bytes(_ATTRS_SIZE))
    skills: bytes = field(default_factory=lambda: bytes(_SKILLS_SIZE))
    health: int = 0
    magicka: int = 0
    fatigue: int = 0


@dataclass
class NpcData:
    """The ``NPDT`` block: level, optional full stats, standing, and gold."""

    level: int = 0
    stats: NpcStats | None = None
    disposition: int = 0
    reputation: int = 0
    rank: int = 0
    gold: int = 0


def _unpack_flags(value: int) -> tuple[NpcFlags, int]:
    """Split the ``FLAG`` word into NPC flags (low byte) and blood type."""
    return NpcFlags(value & _NPC_FLAG_MASK), (value >> _BLOOD_SHIFT) & _BLOOD_MASK


def _pack_flags(npc_flags: NpcFlags, blood_type: int) -> int:
    """Fold NPC flags and blood type back into the ``FLAG`` word."""
    return int(npc_flags) | ((blood_type & _BLOOD_MASK) << _BLOOD_SHIFT)


@register
@dataclass
class Npc(Record):
    """An ``NPC_`` record."""

    TAG: ClassVar[bytes] = b"NPC_"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    race: str = ""
    class_: str = ""
    faction: str = ""
    head: str = ""
    hair: str = ""
    npc_flags: NpcFlags = field(default_factory=lambda: NpcFlags(0))
    blood_type: int = 0
    inventory: list[tuple[int, str]] = field(default_factory=list)
    spells: list[str] = field(default_factory=list)
    ai_data: AiData = field(default_factory=AiData)
    ai_packages: list[AiPackage] = field(default_factory=list)
    travel_destinations: list[TravelDestination] = field(default_factory=list)
    data: NpcData = field(default_factory=NpcData)
