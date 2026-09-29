"""The ``SNDG`` record -- a creature sound generator. A port of ``types/soundgen.rs``.

Binds a creature to a sound for a kind of event (a footstep, a moan, a swim
stroke): an id, a four-byte ``DATA`` giving the event type, the creature id, and
the sound id.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.enums import SoundGenType
from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_DATA_SIZE = 4


@register
@dataclass
class SoundGen(Record):
    """An ``SNDG`` record."""

    TAG: ClassVar[bytes] = b"SNDG"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    sound_gen_type: SoundGenType = field(default_factory=SoundGenType.default)
    creature: str = ""
    sound: str = ""
