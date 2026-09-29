"""The ``DOOR`` record -- a door. A port of ``types/door.rs``.

Like an activator, plus the two sounds a door makes: the ``SNAM`` it opens with
and the ``ANAM`` it closes with.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class Door(Record):
    """A ``DOOR`` record."""

    TAG: ClassVar[bytes] = b"DOOR"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    open_sound: str = ""
    close_sound: str = ""
