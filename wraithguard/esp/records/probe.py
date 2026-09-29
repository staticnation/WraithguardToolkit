"""The ``PROB`` record -- a probe. A port of ``types/probe.rs``.

Identical in shape to a lockpick: the item strings plus a sixteen-byte ``PBDT``
block of weight, value, quality and uses.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register


@dataclass
class ProbeData:
    """The ``PBDT`` block: weight, value, quality and uses (16 bytes)."""

    weight: float = 0.0
    value: int = 0
    quality: float = 0.0
    uses: int = 0


@register
@dataclass
class Probe(Record):
    """A ``PROB`` record."""

    TAG: ClassVar[bytes] = b"PROB"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    id: str = ""
    name: str = ""
    script: str = ""
    mesh: str = ""
    icon: str = ""
    data: ProbeData = field(default_factory=ProbeData)
