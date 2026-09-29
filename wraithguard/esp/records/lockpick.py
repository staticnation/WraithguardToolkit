"""The ``LOCK`` record -- a lockpick. A port of ``types/lockpick.rs``.

The item strings plus a sixteen-byte ``LKDT`` block: weight, value, quality and
the number of uses.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class LockpickData:
    """The ``LKDT`` block: weight, value, quality and uses (16 bytes)."""

    weight: float = 0.0
    value: int = 0
    quality: float = 0.0
    uses: int = 0


@register
@dataclass
class Lockpick(Record):
    """A ``LOCK`` record."""

    TAG: ClassVar[bytes] = b"LOCK"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    data: LockpickData = field(default_factory=LockpickData)
