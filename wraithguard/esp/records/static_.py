"""The ``STAT`` record -- a static object. A port of ``types/static_.rs``.

The simplest record there is: an id and a mesh. Nothing but a model placed in
the world.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class Static(Record):
    """A ``STAT`` record."""

    TAG: ClassVar[bytes] = b"STAT"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    mesh: str = ""
