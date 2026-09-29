"""An object reference inside a cell -- a port of ``types/reference.rs``.

One placed instance of an object: which object (``NAME``), where it sits
(``DATA``'s translation and rotation), and a long list of optional overrides --
its owner, lock and key, trap, soul, remaining charge or health, stack count,
scale, a teleport destination, and more. Not a record of its own; a cell holds
many, each framed by an ``FRMR`` index the cell supplies.

The reference's read ends at its ``DATA`` (a live reference) or ``DELE`` (a
deleted one). A couple of fields are written only under conditions that depend on
whether the reference comes from a master file, so the cell sets ``mast_index``
before writing. Non-finite transforms (which the TES construction set can emit)
are zeroed on read, as the crate does, to keep the data sane.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

from wraithguard.esp.records._ai import TravelDestination

_SCALE_MIN = 0.5
_SCALE_MAX = 2.0
_SCALE_EPSILON = 1e-6
_DATA_SIZE = 24
_DODT_SIZE = 24


@dataclass
class Reference:
    """A placed object reference. ``mast_index``/``refr_index`` come from the cell."""

    mast_index: int = 0
    refr_index: int = 0
    id: str = ""
    temporary: bool = False
    translation: tuple[float, float, float] = (0.0, 0.0, 0.0)
    rotation: tuple[float, float, float] = (0.0, 0.0, 0.0)
    scale: float | None = None
    moved_cell: tuple[int, int] | None = None
    owner: str | None = None
    owner_global: str | None = None
    owner_faction: str | None = None
    owner_faction_rank: int | None = None
    charge_left: int | None = None
    health_left: int | None = None
    object_count: int | None = None
    destination: TravelDestination | None = None
    lock_level: int | None = None
    key: str | None = None
    trap: str | None = None
    soul: str | None = None
    blocked: int | None = None
    deleted: bool | None = None

    def _make_transforms_finite(self) -> None:
        """Zero non-finite transform values and drop a non-finite scale, as the crate."""
        self.translation = tuple(v if math.isfinite(v) else 0.0 for v in self.translation)  # type: ignore[assignment]
        self.rotation = tuple(v if math.isfinite(v) else 0.0 for v in self.rotation)  # type: ignore[assignment]
        if self.scale is not None and not math.isfinite(self.scale):
            self.scale = None

    @property
    def persistent(self) -> bool:
        """Whether the reference is persistent (moved, a teleport door, or flagged)."""
        return self.moved_cell is not None or self.destination is not None or not self.temporary
