"""The ``FACT`` record -- a faction. A port of ``types/faction.rs``.

A joinable faction: its display name, its rank titles, a large ``FADT`` block of
its favoured attributes and skills and the requirements to advance through each
of its ten ranks, and its opinion of other factions. Each rank title is a fixed
thirty-two-byte ``RNAM``; each reaction is an ``ANAM`` faction id followed by an
``INTV`` disposition.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import AttributeId, SkillId
from wraithguard.esp.flags import FactionFlags, ObjectFlags
from wraithguard.esp.record import Record, register

_RANKS = 10
_SKILLS = 7


@dataclass
class FactionRequirement:
    """One rank's advancement requirements (20 bytes): attributes, skills, rep."""

    attributes: tuple[int, int] = (0, 0)
    primary_skill: int = 0
    favored_skill: int = 0
    reputation: int = 0


def _default_requirements() -> tuple[FactionRequirement, ...]:
    """One default :class:`FactionRequirement` per rank, for a fresh ``FADT``."""
    return tuple(FactionRequirement() for _ in range(_RANKS))


@dataclass
class FactionData:
    """The ``FADT`` block: favoured attributes/skills, per-rank requirements, flags."""

    favored_attributes: tuple[AttributeId, AttributeId] = field(
        default_factory=lambda: (AttributeId.default(), AttributeId.default())
    )
    requirements: tuple[FactionRequirement, ...] = field(default_factory=_default_requirements)
    favored_skills: tuple[SkillId, ...] = field(
        default_factory=lambda: tuple(SkillId.default() for _ in range(_SKILLS))
    )
    flags: FactionFlags = field(default_factory=lambda: FactionFlags(0))


@dataclass
class FactionReaction:
    """A faction's disposition toward another (an ``ANAM`` id + ``INTV`` value)."""

    faction: str = ""
    reaction: int = 0


@register
@dataclass
class Faction(Record):
    """A ``FACT`` record."""

    TAG: ClassVar[bytes] = b"FACT"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    rank_names: list[str] = field(default_factory=list)
    reactions: list[FactionReaction] = field(default_factory=list)
    data: FactionData = field(default_factory=FactionData)
