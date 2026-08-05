"""Drop merged cells that the load order already produces on its own.

**Why a merged plugin should be as small as it can be.** Every ``LAND`` record
it carries overrides whatever the load order had there. Where the merge
genuinely combined two mods that is the point; where the merged result is
simply one mod's own terrain, the record does nothing except take ownership of
a cell for no reason -- and it will keep taking ownership after the user
changes their load order, uninstalls that mod, or installs a patch.

Two cases are droppable:

*Unmodified.* The merge produced terrain identical to the reference. Nothing
edited it, or the edits cancelled.

*Single-source.* Exactly one ``.esp`` edited the cell and the merged result
still matches that plugin's own version. The load order already delivers that
plugin's terrain, so the record is redundant.

**The comparison in the second case is not a formality**, and this is the part
that makes the ordering matter. Seam repair runs over the *whole* landmass and
can move vertices in a cell no conflict ever touched -- a singly-edited cell
next to a contested one gets its shared border pulled to a common value. Such a
cell no longer matches its source plugin and *must* be written, or the tear it
was repaired for comes straight back. So cells are compared after repair, not
counted before it.

Masters are excluded from the single-source count deliberately: an ``.esm``
providing the cell is part of the reference, not a mod competing for it.

**All five layers are compared, not just heights.** Merged Lands'
``has_any_difference`` tests heights, normals, world map, colours *and*
textures, and judging on heights alone silently loses work: a cell where one
mod repainted the textures and another recoloured the vertices has heights
identical to the reference, so a height-only test drops the merged record and
the load order falls back to last-wins -- the recolouring disappears.

**Grids are compared by digest rather than by value.** Holding every layer of
every single-editor cell for the whole run would cost roughly 120 KB a cell
against 17 KB for heights alone -- over a gigabyte on a large load order, for a
comparison that only ever asks *are these the same*. A 32-byte BLAKE2b digest
answers that question in constant space. A collision would drop one cell that
should have been kept; at 2**-256 that is not a risk worth a gigabyte.

Ported from ``repair/cleaning.rs`` in Merged Lands (MIT, David Von Derau).
"""

from __future__ import annotations

import hashlib
import logging
from array import array
from dataclasses import dataclass
from typing import TYPE_CHECKING, Final

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

_log: Final = logging.getLogger(__name__)

#: Exterior cell coordinates.
Coords = tuple[int, int]

#: Extensions that make a plugin a mod rather than part of the reference.
_MOD_SUFFIXES: Final[tuple[str, ...]] = (".esp", ".omwaddon")


def is_mod(name: str) -> bool:
    """Whether a plugin competes for a cell or supplies the reference.

    Args:
        name: The plugin's file name.

    Returns:
        ``True`` for a mod, ``False`` for a master.
    """
    return name.lower().endswith(_MOD_SUFFIXES)


@dataclass(slots=True)
class CleaningReport:
    """What was dropped and why.

    Attributes:
        unmodified: Cells whose merged terrain matches the reference.
        single_source: Cells one mod edited that the load order already
            delivers correctly.
        kept: Cells written to the plugin.
        kept_for_seams: Single-source cells kept only because seam repair
            moved them. Worth counting separately: it is the number of cells
            that exist purely to hold the landmass together, and a surprising
            value there is a sign the repair is doing more than expected.
    """

    unmodified: int = 0
    single_source: int = 0
    kept: int = 0
    kept_for_seams: int = 0

    @property
    def dropped(self) -> int:
        """Every cell removed from the output."""
        return self.unmodified + self.single_source


#: The layers a cell can carry, in the order Merged Lands compares them.
LAYERS: Final[tuple[str, ...]] = ("heights", "normals", "world_map", "colors", "textures")


def digest(values: Iterable[int] | None) -> bytes | None:
    """Reduce a grid to a fixed-size fingerprint.

    Args:
        values: A grid, or ``None``.

    Returns:
        A 32-byte digest, or ``None`` for an absent grid. Values are written as
        signed 32-bit machine ints so that a colour grid and a height grid
        holding the same numbers hash alike -- which is correct, because
        cleaning only ever compares a layer against the same layer.
    """
    if values is None:
        return None
    return hashlib.blake2b(array("i", values).tobytes(), digest_size=32).digest()


@dataclass(slots=True)
class CellDigest:
    """One cell's layers, reduced to what an equality test needs.

    Attributes:
        heights: Digest of the height grid, or ``None`` when absent.
        normals: Digest of the vertex normals.
        world_map: Digest of the 9x9 world map grid.
        colors: Digest of the vertex colours.
        textures: Digest of the texture indices.
    """

    heights: bytes | None = None
    normals: bytes | None = None
    world_map: bytes | None = None
    colors: bytes | None = None
    textures: bytes | None = None

    @property
    def is_empty(self) -> bool:
        """Whether the cell carries no comparable layer at all."""
        return all(getattr(self, name) is None for name in LAYERS)


def differs(left: bytes | None, right: bytes | None) -> bool:
    """Whether two grids hold different values.

    Args:
        left: One grid's digest, or ``None``.
        right: The other's, or ``None``.

    Returns:
        ``True`` when both are present and unequal. Two absent grids are not a
        difference, and one absent grid is not comparable -- neither is
        evidence the merge changed anything. This asymmetry is Merged Lands'
        (``has_difference`` returns ``false`` the moment either side is
        ``None``) and it is what lets a layer the mod never declared be
        ignored rather than read as an edit.
    """
    if left is None or right is None:
        return False
    return left != right


def differs_any(left: CellDigest, right: CellDigest) -> bool:
    """Whether any layer of two cells differs.

    Args:
        left: One cell.
        right: The other.

    Returns:
        ``True`` when at least one layer is present on both sides and unequal.
    """
    return any(differs(getattr(left, name), getattr(right, name)) for name in LAYERS)


def clean_landmass(
    merged: dict[Coords, CellDigest],
    reference: dict[Coords, CellDigest],
    sources: Mapping[Coords, Sequence[str]],
    originals: dict[Coords, CellDigest],
) -> tuple[set[Coords], CleaningReport]:
    """Decide which merged cells are worth writing.

    Call **after** seam repair. Calling before would drop cells that repair is
    about to move, and the tears they were repaired for would return.

    Args:
        merged: The merged layers per cell, after seam repair.
        reference: The reference layers per cell, for cells the masters had.
        sources: Which plugins edited each cell, in load order.
        originals: For each cell, the layers of the single mod that edited it,
            where exactly one did. Cells with more than one editor need no
            entry.

    Returns:
        The cells to write, and a report of what was dropped.
    """
    report = CleaningReport()
    keep: set[Coords] = set()

    for coords, grid in merged.items():
        base = reference.get(coords)
        if base is not None and not base.is_empty and not differs_any(grid, base):
            report.unmodified += 1
            continue

        editors = [name for name in sources.get(coords, ()) if is_mod(name)]
        if len(editors) == 1:
            original = originals.get(coords)
            if original is not None and not original.is_empty and not differs_any(grid, original):
                report.single_source += 1
                continue
            if original is not None:
                # One editor, but the merged terrain no longer matches it:
                # seam repair moved the border. This cell holds the landmass
                # together and has to be written.
                report.kept_for_seams += 1

        keep.add(coords)
        report.kept += 1

    _log.info(
        "cleaning: kept %d cell(s), dropped %d unmodified and %d already "
        "delivered by a single mod",
        report.kept,
        report.unmodified,
        report.single_source,
    )
    return keep, report
