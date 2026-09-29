"""The ``LEVC`` record -- a leveled creature list. A port of ``types/leveledcreature.rs``.

The creature counterpart of the leveled item list, identical in shape: a chance
of nothing, and creature id (``CNAM``) / minimum-level (``INTV``) pairs, counted by
an ``INDX``.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import LeveledCreatureFlags, ObjectFlags
from wraithguard.esp.record import Record, register

_DATA_SIZE = 4
_COUNT_SIZE = 4
_NNAM_SIZE = 1
_LEVEL_SIZE = 2


@register
@dataclass
class LeveledCreature(Record):
    """A ``LEVC`` record: a level-gated list of creatures."""

    TAG: ClassVar[bytes] = b"LEVC"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    leveled_creature_flags: LeveledCreatureFlags = field(
        default_factory=lambda: LeveledCreatureFlags(0)
    )
    chance_none: int = 0
    creatures: list[tuple[str, int]] = field(default_factory=list)
