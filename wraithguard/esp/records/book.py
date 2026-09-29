"""The ``BOOK`` record -- a book or scroll. A port of ``types/book.rs``.

The item strings plus a ``TEXT`` (the pages) and a twenty-byte ``BKDT`` block:
weight, value, whether it is a book or a scroll, the skill a skill-book teaches,
and any enchantment.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import BookType, SkillId
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class BookData:
    """The ``BKDT`` block: weight, value, type, skill and enchantment (20 bytes)."""

    weight: float = 0.0
    value: int = 0
    book_type: BookType = BookType.Book
    skill: SkillId = SkillId.None_
    enchantment: int = 0


@register
@dataclass
class Book(Record):
    """A ``BOOK`` record."""

    TAG: ClassVar[bytes] = b"BOOK"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    enchanting: str = ""
    text: str = ""
    data: BookData = field(default_factory=BookData)
