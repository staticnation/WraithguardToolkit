"""The ``ACTI`` record -- an activator. A port of ``types/activator.rs``.

A named, scripted, placed model -- a lever, a bed, a sign. Id, display name,
script and mesh; no data block.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@register
@dataclass
class Activator(Record):
    """An ``ACTI`` record."""

    TAG: ClassVar[bytes] = b"ACTI"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
