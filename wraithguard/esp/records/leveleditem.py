"""The ``LEVI`` record -- a leveled item list. A port of ``types/leveleditem.rs``.

A list that resolves, when the game asks, to an item chosen by the player's
level: a chance the list yields nothing, and the entries themselves -- each an
item id (``INAM``) paired with the minimum player level it appears at (``INTV``).
An ``INDX`` gives the count up front; the pairs follow, id then level.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import LeveledItemFlags, ObjectFlags
from wraithguard.esp.record import Record, register

_DATA_SIZE = 4
_COUNT_SIZE = 4
_NNAM_SIZE = 1
_LEVEL_SIZE = 2


@register
@dataclass
class LeveledItem(Record):
    """A ``LEVI`` record: a level-gated list of items."""

    TAG: ClassVar[bytes] = b"LEVI"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    leveled_item_flags: LeveledItemFlags = field(default_factory=lambda: LeveledItemFlags(0))
    chance_none: int = 0
    items: list[tuple[str, int]] = field(default_factory=list)
