"""Repair the cracks that appear where independently merged cells meet.

**Why merging creates seams.** Cells are merged one at a time, but they are not
independent: adjacent cells *share* their boundary vertices. Cell (0, 0)'s
eastern column is the same ground as cell (1, 0)'s western column, and the game
draws both. Merge each cell on its own and those shared vertices can be settled
differently -- one cell's conflict resolved toward mod A, its neighbour's toward
mod B -- and the terrain tears open along the border. It is one of the most
visible failures a merged plugin can have, and nothing in the per-cell merge can
see it.

**Corners before edges, and the order is not incidental.** Four cells meet at
each corner vertex, so a corner has to reconcile four values; an edge only two.
Repairing edges first would let a later corner repair undo the edge's work at
its endpoints. Merged Lands does corners first and asserts that the edge pass
never touches index 0 or 64 for exactly this reason; that assertion is kept here
as a check rather than an assumption.

**Only heights are repaired.** A visible tear is a difference in *elevation*;
two cells disagreeing about a vertex colour produces a shading discontinuity
that the engine already blends, and disagreeing about a texture index is not
something an average could fix. Normals are handled separately -- see
:func:`mask_normals_to_moved_heights`.

Ported from ``repair/seam_detection.rs`` in Merged Lands (MIT, David Von Derau).

**Coordinate convention**, matching the decoder: row 0 is the *south* edge and
column 0 the *west*, so cell ``(x, y)``'s row 64 meets cell ``(x, y+1)``'s row
0, and its column 64 meets cell ``(x+1, y)``'s column 0.

**Storage.** Heights arrive as :class:`array.array` of machine ints rather than
Python lists. A large load order merges around ten thousand cells; at 4,225
vertices each, boxed Python ints would cost well over a gigabyte, while a
typed array costs about 17 KB per cell.
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Final

from wraithguard.tes3fields.landscape import LAND_SIZE

if TYPE_CHECKING:
    from array import array
    from collections.abc import Iterator, Sequence

#: Exterior cell coordinates.
Coords = tuple[int, int]

#: The last index on an edge. Vertex 64 of one cell is vertex 0 of the next.
_LAST: Final = LAND_SIZE - 1

#: How far a seam correction is spread inward, in vertices.
#:
#: Moving only the boundary vertex concentrates the whole correction in one
#: step. Two mods disagreeing by 3,000 units at a border leaves a 1,500-unit
#: jump between the boundary and the vertex beside it -- more than a signed
#: byte can express, so the encoder clamps and the terrain is not what the
#: merge decided. Measured on two overlapping Solstheim mods: merging alone
#: clamped nothing, and repairing seams without feathering clamped 125
#: vertices, every one of them within a single step of a border.
#:
#: **Shallow beats deep, which is not the obvious answer.** Measured on the
#: same pair of mods, clamped vertices by feather depth:
#:
#: ===== ======= ==================
#: depth clamped interior vertices
#: ===== ======= ==================
#: 0     125     0
#: 2     6       378
#: 4     7       1,134
#: 8     9       2,646
#: 32    25      11,718
#: ===== ======= ==================
#:
#: A longer ramp spreads each correction more gently but touches far more
#: interior vertices, and every one is another chance to push terrain that was
#: already steep past the limit -- especially near corners, where a vertex is
#: feathered along both axes and the two adjustments add. Two vertices halves
#: the jump at the boundary, which is enough for almost every case, and leaves
#: the terrain a mod authored almost entirely alone.
#:
#: Seam integrity is unaffected at any depth: only interior vertices move, so
#: the boundaries stay exactly where the repair put them. Verified at every
#: depth above -- maximum residual disagreement across every shared border, 0.
DEFAULT_FEATHER: Final = 2

#: The four neighbours an edge can be shared with.
_SIDES: Final[tuple[tuple[int, int], ...]] = ((-1, 0), (1, 0), (0, 1), (0, -1))

#: The two of :data:`_SIDES` that point at a *higher* neighbour. Used to decide
#: which of two cells owns row/column 64 and which owns row/column 0.
_FORWARD: Final[frozenset[tuple[int, int]]] = frozenset({(1, 0), (0, 1)})

#: For each of a cell's four corners: the vertex in this cell, then the three
#: neighbouring cells that share it and the vertex it is in each of them.
#:
#: Read one row as: "my corner ``(x, y)`` is also the corner ``(x, y)`` of the
#: cell at ``offset``". All four are the same point on the ground.
_CORNERS: Final[tuple[tuple[tuple[Coords, Coords], ...], ...]] = (
    (((0, 0), (0, 0)), ((0, _LAST), (0, -1)), ((_LAST, 0), (-1, 0)), ((_LAST, _LAST), (-1, -1))),
    (
        ((_LAST, 0), (0, 0)),
        ((_LAST, _LAST), (0, -1)),
        ((0, 0), (1, 0)),
        ((0, _LAST), (1, -1)),
    ),
    (
        ((_LAST, _LAST), (0, 0)),
        ((_LAST, 0), (0, 1)),
        ((0, _LAST), (1, 0)),
        ((0, 0), (1, 1)),
    ),
    (
        ((0, _LAST), (0, 0)),
        ((0, 0), (0, 1)),
        ((_LAST, _LAST), (-1, 0)),
        ((_LAST, 0), (-1, 1)),
    ),
)


@dataclass(slots=True)
class SeamReport:
    """What the repair found and did.

    Attributes:
        corner_vertices: Corner vertices moved to a shared value.
        pinned_corners: Corners left alone because a cell sharing them is not
            being written and no reference height was available to anchor to.
        anchored_corners: Corners set to the reference height because a cell
            sharing them is not being written. The absent ground is
            authoritative: it is not going to move.
        edge_vertices: Edge vertices moved to a shared value.
        largest_gap: The widest disagreement found, in world units. A large
            value means two mods genuinely built different terrain at a border,
            and averaging them is a compromise rather than a fix -- worth
            reporting rather than smoothing over silently.
        worst_borders: The borders with the most repaired vertices.
    """

    corner_vertices: int = 0
    edge_vertices: int = 0
    #: Borders still disagreeing after everything has run. See
    #: :func:`find_tears`. Anything other than an empty list is a bug in this
    #: package, not a property of the load order.
    tears: list[Tear] = field(default_factory=list)
    pinned_corners: int = 0
    anchored_corners: int = 0
    largest_gap: int = 0
    worst_borders: list[tuple[Coords, Coords, int]] = field(default_factory=list)
    #: Interior vertices nudged so a seam correction does not become a cliff
    #: the format cannot store. See :data:`DEFAULT_FEATHER`.
    feathered_vertices: int = 0

    @property
    def total(self) -> int:
        """Every vertex the repair moved."""
        return self.corner_vertices + self.edge_vertices


@dataclass(frozen=True, slots=True)
class Tear:
    """One border where two cells still disagree about shared ground.

    Attributes:
        left: The lower cell, or the written cell when the other is absent.
        right: The higher cell, or ``None`` when the neighbour is not being
            written and the reference supplied the values compared against.
        vertices: How many of the 65 shared vertices differ.
        worst: The widest disagreement, in world units.
    """

    left: Coords
    right: Coords | None
    vertices: int
    worst: int


def find_tears(
    cells: dict[Coords, array[int]],
    reference: dict[Coords, array[int]] | None = None,
    untouched: frozenset[Coords] = frozenset(),
) -> list[Tear]:
    """Check that no border still disagrees. The merge's post-condition.

    Merged Lands opens ``clean_landmass_diff`` with
    ``assert_eq!(repair_landmass_seams(landmass), 0)`` -- it repairs the seams,
    then repairs them *again* and requires the second pass to find nothing. That
    assertion is the whole safety net for the one defect a player sees
    instantly: a wall or a chasm along a cell boundary, in a game where cells
    are 8,192 units across and every boundary is somewhere you walk.

    It matters here more than it does there, because more runs after the
    repair. The slope limiter moves vertices, feathering moves vertices, and
    cleaning removes whole cells. Each is written to preserve borders and each
    is a place a future change could stop doing so, silently.

    **Absent neighbours are checked too, and this is the part a naive check
    misses.** A written cell whose neighbour is *not* written does not get to be
    self-consistent: the ground next door still exists, supplied by the master,
    and it is not going to move. So where a neighbour is absent the comparison
    is against the reference, which is exactly what :func:`repair_corners`
    anchors to.

    **A border between two cells we never edit is not ours.** Borrowed vanilla
    cells are present only so an edit next to them has something to agree with,
    and :func:`repair_edges` refuses to move either side of a border they share
    -- whatever they disagree about is the game's own, predates this merge, and
    will still be there whether or not we run. Reporting it would be blaming the
    tool for the terrain it was pointed at. Measured on a real load order: 62
    such borders, the worst 2,648 units, none of them written by us.

    Args:
        cells: The cells being checked.
        reference: The reference terrain, for neighbours that are not present.
            Omit it to check only borders between the given cells.
        untouched: Cells this merge does not edit and will not write. A border
            with one of them is still checked -- that is the whole point of
            borrowing them -- but a border *between two* of them is skipped.

    Returns:
        One entry per disagreeing border, worst first. Empty is the only
        acceptable result.
    """
    tears: list[Tear] = []
    for left, right in _border_pairs(cells):
        if left in untouched and right in untouched:
            continue
        found = _compare_border(cells[left], cells[right], left[0] == right[0])
        if found is not None:
            count, worst = found
            tears.append(Tear(left=left, right=right, vertices=count, worst=worst))

    if reference is not None:
        for coords in sorted(cells):
            if coords in untouched:
                # Its borders with terrain further out are untouched on both
                # sides for the same reason.
                continue
            for dx, dy in _SIDES:
                other = (coords[0] + dx, coords[1] + dy)
                if other in cells:
                    continue
                outside = reference.get(other)
                if outside is None:
                    # No terrain next door at all -- the edge of the world, or
                    # land nothing has ever defined. Nothing to tear against.
                    continue
                low, high = (
                    (cells[coords], outside) if (dx, dy) in _FORWARD else (outside, cells[coords])
                )
                found = _compare_border(low, high, dx == 0)
                if found is not None:
                    count, worst = found
                    tears.append(Tear(left=coords, right=None, vertices=count, worst=worst))

    tears.sort(key=lambda tear: -tear.worst)
    return tears


def _compare_border(low: array[int], high: array[int], vertical: bool) -> tuple[int, int] | None:
    """Compare the 65 vertices two cells share along one border.

    Args:
        low: The lower cell's heights.
        high: The higher cell's heights.
        vertical: ``True`` when the cells differ in ``y``.

    Returns:
        ``(differing vertices, widest gap)``, or ``None`` when they agree.
    """
    count = 0
    worst = 0
    for step in range(LAND_SIZE):
        if vertical:
            a, b = _index(step, _LAST), _index(step, 0)
        else:
            a, b = _index(_LAST, step), _index(0, step)
        gap = abs(low[a] - high[b])
        if gap:
            count += 1
            worst = max(worst, gap)
    return (count, worst) if count else None


def _index(x: int, y: int) -> int:
    """Flat index of a vertex.

    Args:
        x: Column.
        y: Row.

    Returns:
        The index into a flat 65x65 grid.
    """
    return y * LAND_SIZE + x


def _border_pairs(cells: dict[Coords, array[int]]) -> Iterator[tuple[Coords, Coords]]:
    """Every pair of adjacent cells, each pair yielded once.

    Args:
        cells: The merged cells.

    Yields:
        ``(lower, higher)`` coordinate pairs, ordered so the caller always
        knows which side of the border it is on.
    """
    seen: set[tuple[Coords, Coords]] = set()
    queue: deque[tuple[Coords, Coords]] = deque()

    for coords in sorted(cells):
        for dx, dy in _SIDES:
            neighbour = (coords[0] + dx, coords[1] + dy)
            pair = (coords, neighbour) if coords < neighbour else (neighbour, coords)
            if pair not in seen:
                seen.add(pair)
                queue.append(pair)

    while queue:
        left, right = queue.popleft()
        if left in cells and right in cells:
            yield (left, right)


def _anchor_value(
    corner: tuple[tuple[Coords, Coords], ...],
    coords: Coords,
    cells: dict[Coords, array[int]],
    anchor: dict[Coords, array[int]] | None,
) -> int | None:
    """The height a corner must take from terrain outside the merge.

    Args:
        corner: One row of :data:`_CORNERS`.
        coords: The cell the corner belongs to.
        cells: The merged cells.
        anchor: Reference heights per cell.

    Returns:
        The reference height at the first absent cell's copy of this corner, or
        ``None`` when there is no reference to consult.
    """
    if anchor is None:
        return None
    for (x, y), (dx, dy) in corner:
        other = (coords[0] + dx, coords[1] + dy)
        if other in cells:
            continue
        grid = anchor.get(other)
        if grid is not None:
            return grid[_index(x, y)]
    return None


def _shared_cells(coords: Coords, x: int, y: int) -> list[Coords]:
    """Every cell that shares one vertex, including the one asked about.

    Args:
        coords: The cell.
        x: Column.
        y: Row.

    Returns:
        One to four cell coordinates.
    """
    cx, cy = coords
    xs = [cx] + ([cx - 1] if x == 0 else [cx + 1] if x == _LAST else [])
    ys = [cy] + ([cy - 1] if y == 0 else [cy + 1] if y == _LAST else [])
    return [(gx, gy) for gx in xs for gy in ys]


def is_pinned(cells: dict[Coords, array[int]], coords: Coords, x: int, y: int) -> bool:
    """Whether a vertex is shared with terrain the merge is not writing.

    Such a vertex must not move. The cells that *are* present would follow it
    and the absent one would not, tearing the merged landscape against ground
    that stays exactly where it is.

    Args:
        cells: The landmass being written.
        coords: The cell.
        x: Column.
        y: Row.

    Returns:
        ``True`` when any cell sharing this vertex is absent.
    """
    return any(cell not in cells for cell in _shared_cells(coords, x, y))


def repair_corners(
    cells: dict[Coords, array[int]],
    report: SeamReport,
    anchor: dict[Coords, array[int]] | None = None,
    authoritative: frozenset[Coords] = frozenset(),
) -> None:
    """Reconcile every vertex where up to four cells meet.

    **A corner shared with terrain outside the merge is anchored, not
    averaged.** Four cells meet at each corner; where one of them is not being
    written, its ground stays exactly where it is whatever we decide. Averaging
    the cells we *do* own would move three sides of a vertex and leave the
    fourth, which tears the merged landscape against untouched terrain --
    measured at up to 5,024 world units.

    Pinning all four instead is no better: the three cells we write then keep
    three different values and tear against *each other* (measured: 4 borders,
    the worst 3,944 units).

    So the absent cell wins. Every present cell adopts the reference height at
    that corner, which is what the game will show for the cell we are not
    writing. The merged cells agree with each other and with the ground beyond
    them.

    Args:
        cells: The merged cells, modified in place.
        report: Updated with what was repaired.
        authoritative: Cells carrying no edits, present only so their borders
            can be reconciled. They are never moved: they are what the game
            already has, so merged cells adopt their heights instead of
            meeting them halfway.
        anchor: Reference heights per cell, used to settle corners shared with
            terrain outside ``cells``. Without it such corners are left alone,
            which is only safe when every neighbour is present.
    """
    for coords in sorted(cells):
        for corner in _CORNERS:
            present: list[tuple[Coords, int]] = []
            for (x, y), (dx, dy) in corner:
                other = (coords[0] + dx, coords[1] + dy)
                grid = cells.get(other)
                if grid is not None:
                    present.append((other, _index(x, y)))

            if len(present) < 2:
                # One cell alone owns this corner: nothing to disagree with.
                continue

            values = [cells[cell][offset] for cell, offset in present]
            spread = max(values) - min(values)
            if spread:
                report.largest_gap = max(report.largest_gap, spread)

            fixed = [cells[cell][offset] for cell, offset in present if cell in authoritative]
            if fixed:
                # A cell we are not editing shares this corner. It is what the
                # game already has and it is not going to move, so it decides
                # and every merged cell adopts it.
                average = fixed[0]
                report.anchored_corners += 1
            elif len(present) == len(corner):
                # Every cell sharing this corner is ours.
                average = mean(values)
            else:
                outside = _anchor_value(corner, coords, cells, anchor)
                if outside is not None:
                    # Real ground we are not writing. It will not move, so it
                    # decides, and every cell we do write adopts it.
                    average = outside
                    report.anchored_corners += 1
                else:
                    # The absent cell has no terrain at all -- off the edge of
                    # the world, or land no plugin and no master ever defined.
                    # Nothing to tear against, so the cells we *do* write must
                    # simply agree with each other.
                    average = mean(values)
                    report.pinned_corners += 1

            for cell, offset in present:
                if cells[cell][offset] != average:
                    cells[cell][offset] = average
                    report.corner_vertices += 1


def mean(values: Sequence[int]) -> int:
    """Average a set of heights, truncating toward zero.

    Python's ``//`` floors and Rust's integer ``/`` truncates, so they disagree
    by one unit whenever the total is negative and odd -- and Morrowind terrain
    is negative over every stretch of water in the game. The difference is an
    eighth of one ``VHGT`` step and invisible either way, but matching the
    original means a user comparing this tool's output against Merged Lands'
    gets the same numbers rather than a diff they have to explain.

    Both cells sharing a border receive the same value under either rule, so
    this does not affect whether a seam is closed -- only what it closes to.

    Args:
        values: The heights meeting at a shared vertex. Must not be empty.

    Returns:
        Their mean, truncated toward zero.

    Raises:
        ValueError: If ``values`` is empty.
    """
    if not values:
        raise ValueError("cannot average no values")
    total = sum(values)
    count = len(values)
    return -(-total // count) if total < 0 else total // count


def repair_edges(
    cells: dict[Coords, array[int]],
    report: SeamReport,
    authoritative: frozenset[Coords] = frozenset(),
) -> None:
    """Reconcile the vertices two cells share along a border.

    Args:
        cells: The merged cells, modified in place.
        report: Updated with what was repaired.
        authoritative: Cells carrying no edits, present only so their borders
            can be reconciled. They are never moved: they are what the game
            already has, so merged cells adopt their heights instead of
            meeting them halfway.

    Raises:
        ValueError: If a corner is still unequal, which would mean
            :func:`repair_corners` did not run first and the endpoints of every
            border are about to be repaired twice.
    """
    borders: list[tuple[Coords, Coords, int]] = []

    for left, right in _border_pairs(cells):
        low, high = cells[left], cells[right]
        vertical = left[0] == right[0]
        moved = 0

        for step in range(LAND_SIZE):
            if vertical:
                # Row 64 of the southern cell is row 0 of the northern one.
                a, b = _index(step, _LAST), _index(step, 0)
            else:
                # Column 64 of the western cell is column 0 of the eastern one.
                a, b = _index(_LAST, step), _index(0, step)

            first, second = low[a], high[b]
            if first == second:
                continue

            if step in (0, _LAST):
                # A corner. Either it was pinned -- shared with a cell outside
                # the set, so nothing may move it -- or the corner pass did not
                # run and this border's endpoints are about to be averaged
                # twice. The first is expected at the edge of the merged
                # region; the second is a bug, and only the second raises.
                lx, ly = (step, _LAST) if vertical else (_LAST, step)
                if is_pinned(cells, left, lx, ly):
                    report.pinned_corners += 1
                    continue
                raise ValueError(
                    f"corner at step {step} of the border {left}-{right} still "
                    f"differs ({first} vs {second}). Corners must be repaired "
                    "before edges, or their endpoints are averaged twice."
                )

            report.largest_gap = max(report.largest_gap, abs(first - second))
            left_fixed = left in authoritative
            right_fixed = right in authoritative
            if left_fixed and right_fixed:
                # Two cells we are not editing. Whatever they disagree about is
                # the game's own, it predates this merge, and moving either
                # would be editing terrain nobody asked us to touch.
                continue
            if left_fixed:
                average = first
            elif right_fixed:
                average = second
            else:
                average = mean((first, second))
            low[a] = average
            high[b] = average
            moved += 1

        if moved:
            report.edge_vertices += moved
            borders.append((left, right, moved))

    borders.sort(key=lambda entry: -entry[2])
    report.worst_borders = borders[:20]


def feather_corrections(
    cells: dict[Coords, array[int]],
    moves: dict[Coords, dict[int, int]],
    depth: int,
    report: SeamReport,
) -> None:
    """Spread each seam correction inward so it does not become a cliff.

    A boundary vertex moved by ``d`` has its neighbours moved by a linearly
    diminishing share of ``d``: the next vertex in by ``d * (depth-1)/depth``,
    the one after by ``d * (depth-2)/depth``, and so on to zero. The step
    between any two adjacent vertices then carries at most ``d / depth`` of
    the correction rather than all of it.

    Only vertices *inside* the cell are touched, and never another boundary --
    feathering across a cell would undo the repair on the far side.

    Args:
        cells: The merged cells, modified in place.
        moves: Per cell, the flat index of each moved boundary vertex and how
            far it moved.
        depth: How many vertices to spread over.
        report: Updated with how many interior vertices were nudged.
    """
    if depth < 2:
        return

    for coords, moved in moves.items():
        grid = cells.get(coords)
        if grid is None:
            continue
        for offset, delta in moved.items():
            if delta == 0:
                continue
            x, y = offset % LAND_SIZE, offset // LAND_SIZE
            # Feather along whichever axis this vertex sits on the edge of. A
            # corner is on both, so it feathers along both -- which is what
            # spreads a four-cell disagreement into the cell rather than
            # leaving it as a spike.
            for axis_x, axis_y, limit in (
                (-1, 0, x),
                (1, 0, _LAST - x),
                (0, -1, y),
                (0, 1, _LAST - y),
            ):
                if limit not in (0, _LAST):
                    continue
                for step in range(1, min(depth, LAND_SIZE - 1)):
                    nx, ny = x + axis_x * step, y + axis_y * step
                    if not (0 < nx < _LAST and 0 < ny < _LAST):
                        break
                    share = delta * (depth - step) // depth
                    if share == 0:
                        break
                    grid[ny * LAND_SIZE + nx] += share
                    report.feathered_vertices += 1


def repair_seams(
    cells: dict[Coords, array[int]],
    feather: int = DEFAULT_FEATHER,
    anchor: dict[Coords, array[int]] | None = None,
    authoritative: frozenset[Coords] = frozenset(),
) -> SeamReport:
    """Repair every seam in a merged landmass.

    Args:
        cells: Merged absolute heights in world units, keyed by cell, each a
            flat 65x65 array. Modified in place.
        feather: How far to spread each correction inward. Zero repairs the
            boundary only, which is what Merged Lands does and what leaves
            gradients the format cannot store.
        anchor: Reference heights per cell, so corners shared with terrain
            outside ``cells`` can be settled against the ground that will
            actually remain in the game.
        authoritative: Cells that carry no edits and are present only so their
            borders can be reconciled. **They are never moved.** They are what
            the game already has, so a merged cell adopts their heights rather
            than meeting them halfway -- averaging would shift ground that the
            next cell out still holds at its original height, which does not
            remove the tear but moves it one cell further from the edit.

    Returns:
        What was repaired.

    Raises:
        ValueError: If any grid is the wrong length, or corners survive the
            corner pass unequal.
    """
    expected = LAND_SIZE * LAND_SIZE
    for coords, grid in cells.items():
        if len(grid) != expected:
            raise ValueError(f"cell {coords} has {len(grid)} vertices, expected {expected}")

    report = SeamReport()
    before = {coords: dict(_boundary_values(grid)) for coords, grid in cells.items()}
    repair_corners(cells, report, anchor, authoritative)
    repair_edges(cells, report, authoritative)

    if feather >= 2:
        moves = {
            coords: {
                offset: cells[coords][offset] - value
                for offset, value in values.items()
                if cells[coords][offset] != value
            }
            for coords, values in before.items()
        }
        feather_corrections(cells, moves, feather, report)
    return report


def _boundary_values(grid: array[int]) -> dict[int, int]:
    """Every boundary vertex's value, by flat index.

    Only the boundary is recorded rather than the whole grid: a cell has 4,225
    vertices and 256 of them can move, and keeping a full copy of every cell
    would double the memory the merge already needs.

    Args:
        grid: One cell's heights.

    Returns:
        Flat index to value, for the four edges.
    """
    values: dict[int, int] = {}
    for step in range(LAND_SIZE):
        for offset in (
            _index(step, 0),
            _index(step, _LAST),
            _index(0, step),
            _index(_LAST, step),
        ):
            values[offset] = grid[offset]
    return values


def mask_normals_to_moved_heights(
    normals: list[list[tuple[int, int, int]]],
    original: list[list[tuple[int, int, int]]],
    moved: set[tuple[int, int]],
) -> list[list[tuple[int, int, int]]]:
    """Keep recomputed normals only where the height actually changed.

    Recomputing every normal from the merged heights is simple and slightly
    lossy: a mod may have hand-authored normals to fake a lighting effect the
    geometry does not produce, and blanket recomputation discards that
    wherever the terrain did not move. Merged Lands preserves the original
    normal at any vertex whose height is unchanged, which is what this does.

    Args:
        normals: Normals recomputed from the merged heights.
        original: The normals the merge inherited.
        moved: Vertices whose height changed.

    Returns:
        Recomputed normals at moved vertices, inherited ones elsewhere.
    """
    result: list[list[tuple[int, int, int]]] = []
    for y, row in enumerate(normals):
        out: list[tuple[int, int, int]] = []
        for x, value in enumerate(row):
            if (x, y) in moved or y >= len(original) or x >= len(original[y]):
                out.append(value)
            else:
                out.append(original[y][x])
        result.append(out)
    return result
