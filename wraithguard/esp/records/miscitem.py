"""The ``MISC`` record -- a miscellaneous item. A port of ``types/miscitem.rs``.

Gold, keys, tools, clutter: the item strings plus a twelve-byte ``MCDT`` block
of weight, value and a flag that marks the item a key.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import MiscItemFlags, ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class MiscItemData:
    """The ``MCDT`` block: weight, value and flags (12 bytes)."""

    weight: float = 0.0
    value: int = 0
    flags: MiscItemFlags = field(default_factory=lambda: MiscItemFlags(0))


@register
@dataclass
class MiscItem(Record):
    """A ``MISC`` record."""

    TAG: ClassVar[bytes] = b"MISC"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    data: MiscItemData = field(default_factory=MiscItemData)
