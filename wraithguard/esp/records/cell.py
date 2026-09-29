"""The ``CELL`` record -- an interior or exterior cell. A port of ``types/cell.rs``.

A cell's header (its name, flags and grid, and optional region, map colour, water
height and lighting), followed by every object reference placed in it. Each
reference is framed by an ``FRMR`` carrying a packed master/object index; a moved
reference is preceded by an ``MVRF``/``CNDT`` giving its destination cell; and an
``NAM0`` marks where the persistent references end and the temporary ones begin.

References are kept in file order (the crate re-derives that order from a hash
map, but a file already in it round-trips identically), so a cell read and
written is byte-for-byte its original.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import CellFlags, ObjectFlags
from wraithguard.esp.record import Record, register
from wraithguard.esp.records.reference import Reference

_DATA_SIZE = 12
_CNDT_SIZE = 8
_MAST_SHIFT = 24
_REFR_MASK = 0xFFFFFF


def _unpack(packed: int) -> tuple[int, int]:
    """Split a packed reference index into ``(master index, object index)``."""
    return packed >> _MAST_SHIFT, packed & _REFR_MASK


def _pack(mast_index: int, refr_index: int) -> int:
    """Fold a master and object index back into one packed word."""
    return refr_index | (mast_index << _MAST_SHIFT)


@dataclass
class CellData:
    """The ``DATA`` block: the cell's flags and its exterior grid (12 bytes)."""

    cell_flags: CellFlags = field(default_factory=lambda: CellFlags(0))
    grid: tuple[int, int] = (0, 0)


@dataclass
class AtmosphereData:
    """The ``AMBI`` block: the cell's interior lighting colours and fog (16 bytes)."""

    ambient_color: bytes = b"\x00\x00\x00\x00"
    sunlight_color: bytes = b"\x00\x00\x00\x00"
    fog_color: bytes = b"\x00\x00\x00\x00"
    fog_density: float = 0.0


@register
@dataclass
class Cell(Record):
    """A ``CELL`` record."""

    TAG: ClassVar[bytes] = b"CELL"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    name: str = ""
    data: CellData = field(default_factory=CellData)
    region: str | None = None
    map_color: bytes | None = None
    water_height: float | None = None
    atmosphere_data: AtmosphereData | None = None
    references: list[Reference] = field(default_factory=list)
