"""The ``SOUN`` record -- a sound. A port of ``types/sound.rs``.

An id, the sound file it plays, and a three-byte ``DATA`` block: a volume and a
min/max audible range.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_DATA_SIZE = 3


@dataclass
class SoundData:
    """The ``DATA`` block: volume and the (min, max) range (3 bytes)."""

    volume: int = 0
    range: tuple[int, int] = (0, 0)


@register
@dataclass
class Sound(Record):
    """A ``SOUN`` record."""

    TAG: ClassVar[bytes] = b"SOUN"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    sound_path: str = ""
    data: SoundData = field(default_factory=SoundData)
