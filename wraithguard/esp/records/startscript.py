"""The ``SSCR`` record -- a start script. A port of ``types/startscript.rs``.

A script the game runs at startup. Unusually, its id is stored under ``DATA`` and
the script name under ``NAME`` -- the reverse of the usual convention, so the
tags are mapped deliberately here.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class StartScript(Record):
    """An ``SSCR`` record."""

    TAG: ClassVar[bytes] = b"SSCR"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    script: str = ""
