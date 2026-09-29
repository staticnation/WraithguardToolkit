"""The ``LAND`` record -- a landscape cell. A port of ``types/landscape.rs``.

The terrain of one exterior cell: which cell (``INTV``), which data it carries
(``DATA``), and then large fixed-size grids -- vertex normals (``VNML``), a
height map with its base offset (``VHGT``), the coarse world-map colours
(``WNAM``), vertex colours (``VCLR``) and the per-quad texture indices (``VTEX``).

The grids are kept as raw bytes here: the library's job is to preserve them
exactly, and :mod:`wraithguard.land` already decodes them into heights, normals
and layers. Which grids are written is gated by the landscape flags, exactly as
the crate does, so a cell round-trips byte-for-byte.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar

from wraithguard.esp.flags import LandscapeFlags, ObjectFlags
from wraithguard.esp.record import Record, register

_VNML_SIZE = 12675  # 65 x 65 x 3 signed bytes
_VHGT_SIZE = 4232  # f32 offset + 65 x 65 signed bytes + 3 padding
_VHGT_DATA = 4225  # 65 x 65
_WNAM_SIZE = 81  # 9 x 9 signed bytes
_VCLR_SIZE = 12675  # 65 x 65 x 3 bytes
_VTEX_SIZE = 512  # 16 x 16 u16


@register
@dataclass
class Landscape(Record):
    """A ``LAND`` record: one exterior cell's terrain grids."""

    TAG: ClassVar[bytes] = b"LAND"

    flags: ObjectFlags = field(default_factory=lambda: ObjectFlags(0))
    grid: tuple[int, int] = (0, 0)
    landscape_flags: LandscapeFlags = field(default_factory=lambda: LandscapeFlags(0))
    vertex_normals: bytes = field(default_factory=lambda: bytes(_VNML_SIZE))
    vertex_heights_offset: float = 0.0
    vertex_heights: bytes = field(default_factory=lambda: bytes(_VHGT_DATA))
    world_map_data: bytes = field(default_factory=lambda: bytes(_WNAM_SIZE))
    vertex_colors: bytes = field(default_factory=lambda: bytes(_VCLR_SIZE))
    texture_indices: bytes = field(default_factory=lambda: bytes(_VTEX_SIZE))
