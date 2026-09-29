"""The ``LTEX`` record -- a landscape texture. A port of ``types/landscapetexture.rs``.

Names a terrain texture and gives it the index that landscape records refer to
it by: an id, a four-byte ``INTV`` index, and the texture file in ``DATA``.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class LandscapeTexture(Record):
    """An ``LTEX`` record."""

    TAG: ClassVar[bytes] = b"LTEX"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    index: int = 0
    file_name: str = ""
