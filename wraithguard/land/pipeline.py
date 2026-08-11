"""The whole merge, in the order Merged Lands performs it.

``build_merged_lands.py`` was doing this inline and doing it in the wrong shape.
This module is the algorithm as the original states it, with each step where it
belongs:

1. Build the **reference landmass** from the masters.
2. Compute each plugin's **difference** against it.
3. Copy the reference into a **merged landmass** and fold every difference in.
4. **Repair seams** across the whole landmass.
5. **Clean**: drop cells the load order already delivers.
6. Convert to TES3 records and save.

**The mistake this module exists to fix.** The tool previously merged only
*contested* cells -- those more than one mod edited -- because those are the
ones with anything to resolve. That is true of steps 2 and 3 and false of step
4. Seams are shared between *adjacent* cells, and a contested cell's neighbour
may have been edited by only one mod. Leaving that neighbour out means its side
of the shared border is never reconciled, and the tear survives the repair that
existed to remove it.

So every cell any mod modified is merged, seams are repaired across all of them,
and the redundant ones are dropped at the end by
:mod:`~wraithguard.land.cleaning` -- which is exactly why the original cleans
*after* repairing rather than filtering before.

**Memory.** Merged heights are held for every modified cell at once, which a
large load order makes real: around ten thousand cells at 4,225 vertices. They
are :class:`array.array` of machine ints, roughly 17 KB per cell and about
180 MB in total. As Python lists of boxed ints the same data would cost well
over a gigabyte, which is the difference between this running and not.
"""

from __future__ import annotations

import logging
from array import array
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Final

from wraithguard.land.cleaning import (
    CellDigest,
    CleaningReport,
    clean_landmass,
    digest,
)
from wraithguard.land.diff import ALL_LAYERS, LandData, RelativeGrid
from wraithguard.land.heights import vertex_normals_from_heights
from wraithguard.land.landmass import plugin_differences
from wraithguard.land.merge import ConflictStrategy, merge_layer
from wraithguard.land.seams import SeamReport, find_tears, repair_seams
from wraithguard.land.slope import SlopeReport, limit_slopes
from wraithguard.tes3fields.landscape import LAND_SIZE

if TYPE_CHECKING:
    from collections.abc import Sequence

    from wraithguard.land.diff import LandscapeDiff, LandscapeLayers
    from wraithguard.land.landmass import Landmass, PluginRecords
    from wraithguard.land.meta import PluginMeta
    from wraithguard.land.textures import KnownTextures

_log: Final = logging.getLogger(__name__)

#: Exterior cell coordinates.
Coords = tuple[int, int]

#: Vertices in one cell.
_CELL_VERTICES: Final = LAND_SIZE * LAND_SIZE


@dataclass(slots=True)
class MergedCell:
    """One cell after every plugin's edits have been folded in.

    Attributes:
        coords: Where it is.
        heights: Absolute heights in world units, or ``None`` when no plugin
            supplied any.
        textures: Shared texture indices, flat.
        world_map: The 9x9 world map grid, flat.
        colors: Vertex colours, flat and interleaved.
        normals: Vertex normals, flat and interleaved. Filled by
            :func:`resolve_normals` once the heights are final.
        editors: Every plugin that changed it, in load order.
        contested: Vertices more than one plugin moved.
        major: Contested vertices whose compromise sits far from an intent.
        new_land: Whether the masters had no terrain here.
    """

    coords: Coords
    heights: array[int] | None = None
    textures: list[int] | None = None
    world_map: list[int] | None = None
    colors: list[int] | None = None
    normals: list[int] | None = None
    editors: list[str] = field(default_factory=list)
    contested: int = 0
    major: int = 0
    new_land: bool = False
    #: Digests of the single mod's own layers, filled the first time each layer
    #: is folded. Cleaning needs them to tell "the load order already does this"
    #: from "seam repair moved it". Digests rather than grids because the only
    #: question asked of them is equality, and holding five real grids per cell
    #: would cost over a gigabyte on a large load order.
    sole_source: CellDigest = field(default_factory=CellDigest)


@dataclass(slots=True)
class MergeOutcome:
    """Everything the pipeline produced.

    Attributes:
        cells: The merged cells worth writing, keyed by coordinates.
        seams: What seam repair did.
        slopes: What the slope limiter had to adjust.
        cleaning: What cleaning dropped.
        clamped: Vertices too steep for ``VHGT`` to express.
        skipped_plugins: Plugins excluded before merging, and why.
        borrowed: Untouched cells brought in so their borders could be
            reconciled. Most are dropped again by cleaning.
    """

    cells: dict[Coords, MergedCell] = field(default_factory=dict)
    seams: SeamReport = field(default_factory=SeamReport)
    slopes: SlopeReport = field(default_factory=SlopeReport)
    cleaning: CleaningReport = field(default_factory=CleaningReport)
    clamped: int = 0
    skipped_plugins: list[tuple[str, str]] = field(default_factory=list)
    borrowed: int = 0


def _to_array(grid: RelativeGrid) -> array[int]:
    """Read a merged single-component grid into a typed array.

    Args:
        grid: The merged differences.

    Returns:
        The absolute values, one machine int per vertex.
    """
    return array("i", grid.to_flat())


def _fold(
    existing: RelativeGrid | None,
    incoming: RelativeGrid,
    layer: LandData,
    strategy: ConflictStrategy,
) -> tuple[RelativeGrid, int, int]:
    """Merge one plugin's layer into the running result.

    Args:
        existing: What previous plugins produced, or ``None`` for the first.
        incoming: This plugin's differences.
        layer: Which layer.
        strategy: How to settle contested vertices.

    Returns:
        The merged grid, contested count and major count.
    """
    if existing is None:
        return incoming, 0, 0
    merged, report = merge_layer(layer, existing, incoming, strategy)
    return merged, report.contested, report.major


def merge_landmass(
    reference: Landmass,
    plugins: Sequence[PluginRecords],
    known: KnownTextures,
    metas: dict[str, PluginMeta] | None = None,
    strategy: ConflictStrategy = ConflictStrategy.AUTO,
) -> MergeOutcome:
    """Fold every plugin's landscape edits into one landmass.

    Args:
        reference: The reference landmass from the masters.
        plugins: The mods, in load order.
        known: The shared texture table, extended as plugins are read.
        metas: Per-plugin ``.mergedlands.toml`` settings, keyed by plugin name.
        strategy: The command-line strategy, used where a sidecar says ``Auto``.

    Returns:
        Every modified cell, before seam repair and cleaning.
    """
    settings = metas or {}
    outcome = MergeOutcome()
    working: dict[Coords, dict[str, RelativeGrid]] = {}

    for plugin in plugins:
        meta = settings.get(plugin.name)
        if meta is not None and meta.is_previous_merge:
            # A Merged Lands.esp from a previous run. Merging it back in would
            # compound every compromise it already made.
            outcome.skipped_plugins.append((plugin.name, "a previous merge"))
            continue

        allowed = meta.allowed_layers() if meta else ALL_LAYERS
        if not allowed:
            outcome.skipped_plugins.append((plugin.name, "every layer excluded"))
            continue

        for change in plugin_differences(reference, plugin, known, allowed):
            cell = outcome.cells.get(change.coords)
            if cell is None:
                cell = MergedCell(coords=change.coords, new_land=change.new_land)
                outcome.cells[change.coords] = cell
                working[change.coords] = {}
            cell.editors.append(plugin.name)
            cell.new_land = cell.new_land or change.new_land
            _fold_change(cell, working[change.coords], change, meta, strategy)

    for coords, grids in working.items():
        cell = outcome.cells[coords]
        heights = grids.get("heights")
        if heights is not None:
            cell.heights = _to_array(heights)
        for name in ("textures", "world_map", "colors"):
            grid = grids.get(name)
            if grid is not None:
                setattr(cell, name, grid.to_flat())
        inherit_reference_layers(cell, reference)

    _log.info(
        "merged %d cell(s) from %d plugin(s), %d skipped",
        len(outcome.cells),
        len(plugins),
        len(outcome.skipped_plugins),
    )
    return outcome


def _fold_change(
    cell: MergedCell,
    grids: dict[str, RelativeGrid],
    change: LandscapeDiff,
    meta: PluginMeta | None,
    default: ConflictStrategy,
) -> None:
    """Fold one plugin's differences for one cell into the running result.

    Args:
        cell: The cell being built, updated with counts.
        grids: The running merged grids for that cell.
        change: This plugin's differences.
        meta: The plugin's settings, if it has any.
        default: The strategy to use where the sidecar says ``Auto``.
    """
    # New land has no common ancestor to average toward, so blending two
    # authored landscapes would make a third nobody wrote. A sidecar asking
    # for something else is still honoured -- it is an explicit instruction,
    # not a default being applied blindly.
    for name, layer, source in (
        ("heights", LandData.VERTEX_HEIGHTS, change.heights),
        ("textures", LandData.TEXTURES, change.textures),
        ("world_map", LandData.WORLD_MAP, change.world_map),
        ("colors", LandData.VERTEX_COLORS, change.colors),
    ):
        if source is None:
            continue

        chosen = meta.strategy_for(layer) if meta else ConflictStrategy.AUTO
        if chosen is ConflictStrategy.AUTO:
            chosen = default if layer is LandData.VERTEX_HEIGHTS else ConflictStrategy.AUTO
            if cell.new_land and layer is LandData.VERTEX_HEIGHTS:
                chosen = ConflictStrategy.OVERWRITE

        previous = grids.get(name)
        merged, contested, major = _fold(previous, source, layer, chosen)
        grids[name] = merged
        # The first plugin to touch a layer *is* that layer's sole source, so
        # its own terrain is recorded here. A second plugin folding into the
        # same layer means the cell is contested and cleaning will not consider
        # dropping it, so the recorded digest is cleared rather than left to
        # describe something that is no longer one mod's work.
        setattr(cell.sole_source, name, digest(source.to_flat()) if previous is None else None)
        if layer is LandData.VERTEX_HEIGHTS:
            cell.contested += contested
            cell.major += major


def inherit_reference_layers(cell: MergedCell, reference: Landmass) -> None:
    """Fill a merged cell's untouched layers from the reference.

    **A merged ``LAND`` record replaces the whole record, not the layers it
    changed.** If a mod reshaped a cell's heights and nobody touched its
    textures, a merged record carrying heights alone leaves that cell with no
    texture data at all -- the mod's and the game's are both gone, because the
    record that held them has been superseded. The result is untextured
    terrain, and nothing in the merge would report it.

    Merged Lands avoids this by seeding every cell from the reference
    (``LandscapeDiff::from_reference``), so an unmodified layer still carries
    the reference's values and ``to_terrain()`` returns real terrain rather
    than zeros. This does the same, after folding, for whatever the merge did
    not touch.

    Measured before this existed: of 24 cells written from two Solstheim mods,
    13 lost textures the reference had, 14 lost vertex colours and 12 lost the
    world map.

    A cell the masters never had inherits nothing, which is correct: there was
    no terrain there to keep.

    Args:
        cell: The merged cell, filled in place.
        reference: The reference landmass.
    """
    layers = reference.get(cell.coords)
    if layers is None:
        return
    if cell.heights is None and layers.heights is not None:
        cell.heights = array("i", layers.heights)
    for name in ("textures", "world_map", "colors"):
        if getattr(cell, name) is None:
            source = getattr(layers, name)
            if source is not None:
                setattr(cell, name, list(source))


def resolve_normals(outcome: MergeOutcome, reference: Landmass) -> int:
    """Recompute vertex normals, keeping the originals where nothing moved.

    Normals light the terrain, so a cell whose heights changed but whose
    normals did not is lit as though the old ground were still there. Every
    moved vertex therefore needs a fresh normal.

    **Recomputing all of them is slightly lossy.** A mod may hand-author a
    normal to fake a lighting effect its geometry does not produce, and blanket
    recomputation discards that wherever the terrain never moved. Merged Lands
    keeps the original in exactly that case (``recompute_vertex_normals``), and
    so does this.

    **A zero authored normal is never kept.** ``(0, 0, 0)`` is not a lighting
    choice, it is missing data -- a reference cell that declared heights but
    left its ``VNML`` unfilled, which tes3conv hands back as zeros. Inheriting it
    would replace a correctly-recomputed normal with one that lights the vertex
    flat, and the engine paints a flat-normal vertex black: the dark squares that
    show up in coastal and underwater cells, where the reference normals are most
    often zero. So the inherited normal is taken only where it is non-zero, and
    the fresh one stands everywhere else. The OpenMW fork made this same fix to
    ``recompute_vertex_normals`` for the same reason.

    Args:
        outcome: The merged cells, with ``normals`` filled in place.
        reference: The reference landmass, for the normals to preserve.

    Returns:
        How many vertices kept an inherited normal rather than a fresh one.
    """
    preserved = 0
    for coords, cell in outcome.cells.items():
        if cell.heights is None:
            continue
        rows = [
            [float(v) for v in cell.heights[y * LAND_SIZE : (y + 1) * LAND_SIZE]]
            for y in range(LAND_SIZE)
        ]
        computed = vertex_normals_from_heights(rows)

        layers = reference.get(coords)
        original = layers.normals if layers is not None else None
        base = layers.heights if layers is not None else None
        if original is not None and base is not None and len(base) == len(cell.heights):
            for y in range(LAND_SIZE):
                for x in range(LAND_SIZE):
                    index = y * LAND_SIZE + x
                    if cell.heights[index] != base[index]:
                        continue
                    start = index * 3
                    inherited = (
                        original[start],
                        original[start + 1],
                        original[start + 2],
                    )
                    if inherited == (0, 0, 0):
                        # Missing data, not a lighting choice: keep the fresh
                        # normal rather than paint the vertex flat/black.
                        continue
                    computed[y][x] = inherited
                    preserved += 1

        cell.normals = [c for row in computed for triple in row for c in triple]

    if preserved:
        _log.info("kept %d hand-authored normal(s) where the height did not move", preserved)
    return preserved


def add_reference_neighbours(outcome: MergeOutcome, reference: Landmass) -> set[Coords]:
    """Bring in untouched vanilla cells that border a merged one.

    **Why the merged set is not enough.** A cell a mod reshaped sits next to
    cells nobody touched, and it shares its boundary vertices with them. If
    only modified cells are in the landmass, that border is never reconciled
    and the merged terrain tears against vanilla ground -- measured on two
    Solstheim mods: 19 such borders, 16 of them disagreeing, the worst by 5,024
    world units.

    Merged Lands avoids this by seeding the merged landmass with the *entire*
    reference and cleaning at the end. Only the immediate neighbours are needed
    to get the same borders, so that is what this adds: the same repair for a
    fraction of the memory.

    A neighbour brought in this way carries no edits. Seam repair either moves
    it -- in which case it must be written, and
    :mod:`~wraithguard.land.cleaning` keeps it -- or it does not, and cleaning
    drops it as unmodified. Nothing is emitted that did not have to be.

    Args:
        outcome: The merged cells, extended in place.
        reference: The reference landmass.

    **A borrowed cell is authoritative, not a negotiating party.** It is a copy
    of what the game already has, and the cell beyond it -- which is not
    borrowed -- still holds those heights. Averaging a merged cell against it
    moves ground the next cell out does not move, which does not remove the
    tear; it relocates it one cell further from the edit. So the merged side
    adopts the borrowed cell's heights whole, the borrowed cell never changes,
    and cleaning drops it again as unmodified.

    Returns:
        The cells borrowed, which the caller must pass to seam repair as
        authoritative.
    """
    existing = set(outcome.cells)
    added: set[Coords] = set()
    # Diagonals as well as sides. A corner vertex is shared by *four* cells,
    # and reconciling it needs all four present -- borrowing only the
    # orthogonal neighbours leaves the diagonal absent and the corner pinned.
    for x, y in sorted(existing):
        for dx, dy in (
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ):
            coords = (x + dx, y + dy)
            if coords in outcome.cells:
                continue
            layers = reference.get(coords)
            if layers is None or layers.heights is None:
                continue
            cell = MergedCell(coords=coords, heights=array("i", layers.heights))
            # A borrowed cell is written whole like any other, so it needs the
            # reference's textures and colours too -- not just the heights the
            # seam repair came for.
            inherit_reference_layers(cell, reference)
            outcome.cells[coords] = cell
            added.add(coords)
    if added:
        _log.info("brought in %d untouched cell(s) bordering the merge", len(added))
    return added


def finish(
    outcome: MergeOutcome,
    reference: Landmass,
    *,
    repair: bool = True,
    clean: bool = True,
    limit: bool = True,
) -> MergeOutcome:
    """Repair seams and drop redundant cells.

    Args:
        outcome: The merged cells.
        reference: The reference landmass, for deciding what is unmodified.
        repair: Repair seams. Off only for diagnosis; a merged plugin written
            without it can tear at cell borders.
        clean: Drop cells the load order already delivers.
        limit: Condition the terrain so every step is representable. Without
            it the writer clamps whatever will not fit, and the terrain in the
            plugin is not the terrain the merge decided on.

    Returns:
        The same outcome, with cells removed and reports filled in.
    """
    # Bound unconditionally: the slope limiter consults it too, and
    # `--no-seam-repair` must not leave it undefined.
    borrowed: frozenset[Coords] = frozenset()
    if repair:
        # Neighbours have to join *before* the repair, or their side of every
        # shared border is still unreconciled when it runs.
        borrowed = frozenset(add_reference_neighbours(outcome, reference))
        outcome.borrowed = len(borrowed)

    heights = {c: cell.heights for c, cell in outcome.cells.items() if cell.heights is not None}

    if repair and heights:
        anchor = {
            coords: array("i", layers.heights)
            for coords, layers in reference.cells.items()
            if layers.heights is not None
        }
        outcome.seams = repair_seams(heights, anchor=anchor, authoritative=borrowed)
        _log.info(
            "seam repair moved %d vertex/vertices, widest gap %d units",
            outcome.seams.total,
            outcome.seams.largest_gap,
        )

    if limit and heights:
        outcome.slopes = limit_slopes(heights, authoritative=borrowed)
        if outcome.slopes.adjusted:
            _log.info(
                "slope limiter moved %d vertex/vertices in %d pass(es)",
                outcome.slopes.adjusted,
                outcome.slopes.passes,
            )

    # Normals last: they are a function of the final heights, so they have to
    # be computed after seam repair and the slope limiter have finished moving
    # them.
    resolve_normals(outcome, reference)

    # Checked here, on the repaired landmass, rather than after cleaning. See
    # _check_borders for why the post-cleaning set is the wrong thing to test.
    _check_borders(outcome, reference, borrowed)

    if not clean:
        outcome.cleaning = CleaningReport(kept=len(outcome.cells))
        return outcome

    merged_digests = {c: _digest_cell(cell) for c, cell in outcome.cells.items()}
    reference_digests: dict[Coords, CellDigest] = {}
    for coords in outcome.cells:
        layers = reference.get(coords)
        if layers is not None:
            reference_digests[coords] = _digest_reference(layers)

    sources = {c: cell.editors for c, cell in outcome.cells.items()}
    originals = {c: cell.sole_source for c, cell in outcome.cells.items()}

    keep, outcome.cleaning = clean_landmass(merged_digests, reference_digests, sources, originals)
    outcome.cells = {c: cell for c, cell in outcome.cells.items() if c in keep}
    return outcome


def _check_borders(
    outcome: MergeOutcome,
    reference: Landmass,
    borrowed: frozenset[Coords] = frozenset(),
) -> None:
    """Verify that nothing tore, once everything has finished moving.

    Merged Lands repairs the seams and then repairs them *again*, asserting the
    second pass finds nothing. That post-condition is worth more here than
    there, because more happens afterwards: the slope limiter moves vertices
    and feathering moves vertices, and each is a place a future change could
    quietly stop preserving borders.

    **Run before cleaning, not after.** Checking the cells that survive cleaning
    sounds stricter and is simply wrong. Cleaning drops a cell exactly when the
    load order already delivers that terrain -- either nothing edited it, or one
    mod did and its own record produces the same result. So a dropped cell's
    ground is still *there* in the game; it just comes from a different file.
    Treating it as absent and comparing its neighbour against the reference
    instead measures a merged cell against vanilla when the terrain next door
    is the mod's, and reports a tear the size of that mod's edit -- 17,560 units
    on a real load order, for a border that is perfectly intact.

    That also means cleaning cannot open a border this check closed: it only
    ever removes a record whose content the load order reproduces exactly.

    Borders with cells that were never in the merge are still checked against
    the reference, which for those *is* what the game has.

    Args:
        outcome: The merge, after repair and the limiter and before cleaning.
            Its seam report is updated.
        reference: The reference landmass, for neighbours the merge never had.
        borrowed: Cells present only to reconcile a border, which this merge
            neither edits nor writes. Borders *between* two of them are the
            game's own and are not this tool's to answer for.
    """
    written = {
        coords: cell.heights for coords, cell in outcome.cells.items() if cell.heights is not None
    }
    if not written:
        return
    nearby: dict[Coords, array[int]] = {}
    for coords in written:
        for dx, dy in ((-1, 0), (1, 0), (0, 1), (0, -1)):
            other = (coords[0] + dx, coords[1] + dy)
            if other in written or other in nearby:
                continue
            layers = reference.get(other)
            if layers is not None and layers.heights is not None:
                nearby[other] = array("i", layers.heights)

    outcome.seams.tears = find_tears(written, nearby, borrowed)
    if outcome.seams.tears:
        worst = outcome.seams.tears[0]
        _log.error(
            "%d border(s) still disagree after repair -- worst %s/%s, %d vertex/vertices, "
            "%d units. This is a defect in the merge, not in the load order.",
            len(outcome.seams.tears),
            worst.left,
            worst.right if worst.right is not None else "reference",
            worst.vertices,
            worst.worst,
        )


def _digest_cell(cell: MergedCell) -> CellDigest:
    """Fingerprint a merged cell's layers for cleaning.

    Normals are deliberately left out: :func:`resolve_normals` derives them
    from the final heights, so they carry no information the heights do not,
    and both sides of every comparison would agree by construction.

    Args:
        cell: The merged cell, after seam repair and the slope limiter.

    Returns:
        Its digests.
    """
    return CellDigest(
        heights=digest(cell.heights),
        world_map=digest(cell.world_map),
        colors=digest(cell.colors),
        textures=digest(cell.textures),
    )


def _digest_reference(layers: LandscapeLayers) -> CellDigest:
    """Fingerprint the reference's version of a cell.

    Args:
        layers: The reference layers.

    Returns:
        Its digests, with the same layers omitted as :func:`_digest_cell`.
    """
    return CellDigest(
        heights=digest(layers.heights),
        world_map=digest(layers.world_map),
        colors=digest(layers.colors),
        textures=digest(layers.textures),
    )
