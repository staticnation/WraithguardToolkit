"""The ``BODY`` record -- a body part. A port of ``types/bodypart.rs``.

One mesh for one part of one race's body (a head, a hand, a tail): an id, the
race it belongs to, its mesh, and a four-byte ``BYDT`` block naming the part, a
vampire flag, its flags, and whether it is skin, armor or clothing.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import BodypartId, BodypartType
from wraithguard.esp.flags import BodypartFlags, ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class BodypartData:
    """The ``BYDT`` block: part, vampire flag, flags and type (4 bytes)."""

    part: BodypartId = field(default_factory=BodypartId.default)
    vampire: bool = False
    flags: BodypartFlags = field(default_factory=lambda: BodypartFlags(0))
    bodypart_type: BodypartType = field(default_factory=BodypartType.default)


@register
@dataclass
class Bodypart(Record):
    """A ``BODY`` record."""

    TAG: ClassVar[bytes] = b"BODY"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    race: str = ""
    mesh: str = ""
    data: BodypartData = field(default_factory=BodypartData)
