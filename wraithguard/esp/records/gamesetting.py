"""The ``GMST`` record -- a game setting. A port of ``types/gamesetting.rs``.

A named engine constant. Its value is one of three types, and which subrecord
carries it says which: a string in ``STRV``, a float in ``FLTV``, an integer in
``INTV``. The value's Python type stands in for the crate's tagged union -- a
``str``, ``float`` or ``int`` -- and is written back to the matching subrecord.
There is no ``DELE`` here; a game setting is never a deletion.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class GameSetting(Record):
    """A ``GMST`` record: a named value that is a string, float or integer."""

    TAG: ClassVar[bytes] = b"GMST"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    value: str | float | int = 0.0
