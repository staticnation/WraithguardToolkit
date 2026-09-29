"""The ``PGRD`` record -- a path grid. A port of ``types/pathgrid.rs``.

The navigation mesh for one cell: a twelve-byte ``DATA`` header (the cell's grid
coordinates, a granularity, and the point count), the cell name, the points
themselves (``PGRP`` -- each a 3D location plus connection bookkeeping and two
bytes of padding), and the flat connection table (``PGRC``). Point and connection
blocks carry their byte length, from which the counts are derived.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.record import Record, register

_DATA_SIZE = 12


@dataclass
class PathGridData:
    """The ``DATA`` header: grid coordinates, granularity, point count (12 bytes)."""

    grid: tuple[int, int] = (0, 0)
    granularity: int = 0
    point_count: int = 0


@dataclass
class PathGridPoint:
    """One path node (16 bytes): a 3D location and its connection bookkeeping."""

    location: tuple[int, int, int] = (0, 0, 0)
    auto_generated: int = 0
    connection_count: int = 0


@register
@dataclass
class PathGrid(Record):
    """A ``PGRD`` record."""

    TAG: ClassVar[bytes] = b"PGRD"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    cell: str = ""
    data: PathGridData = field(default_factory=PathGridData)
    points: list[PathGridPoint] = field(default_factory=list)
    connections: list[int] = field(default_factory=list)
