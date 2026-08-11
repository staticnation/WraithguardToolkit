"""Combine two plugins' landscape edits, and decide what a contested vertex becomes.

:mod:`~wraithguard.land.diff` establishes *which* vertices each mod moved.
This decides what happens where two mods moved the same one.

**Three of the four cases need no decision at all**, and that is the whole
reason merging works. For any vertex:

* neither plugin moved it -- keep the reference.
* only one moved it -- take that edit, whole. This is the case a load order
  throws away, and across 300 of this repository's plugins it accounts for
  19,078 height vertices against 8,082 genuinely contested ones.
* both moved it -- now a strategy has to choose.

**The strategies**, ported from Merged Lands:

``RESOLVE``
    Average the two edits, weighted toward the larger. A mod that raised a
    vertex by 800 units and one that raised it by 20 produce something close
    to 800: the small edit was probably incidental, the large one intentional.
``OVERWRITE``
    Take the later plugin's edit. Last in the load order wins, but only at the
    contested vertices -- everything else still merges.
``IGNORE``
    Keep the earlier plugin's edit at contested vertices.

**The default is the load order's own answer: OVERWRITE.** Where two mods both
moved a vertex, the later one in the load order wins it -- the same rule the
engine applies to a whole record, but here applied per vertex, so everything
*not* contested still merges. This is what makes the tool a seam resolver rather
than a terrain blender: it keeps each mod's authored ground and reconciles only
the borders between them.

Blending (``RESOLVE``) is available per layer through a ``.mergedlands.toml``
but is **not** the default. Averaging every contested vertex synthesises a
surface neither mod authored, and across a large load order that shows up as
visible stretching -- the OpenMW fork reached the same conclusion and made its
``Auto`` mean load-order-winner for the same reason. ``RESOLVE`` is the right
tool where two edits are both intentional and a compromise is genuinely wanted;
it is asked for, not assumed. The magnitude-weighted average, the curvature
weighting and the minor/major severity split all still exist -- they run when a
sidecar selects ``RESOLVE`` or ``CURVATURE``, not on their own.

**Textures never blend, default or not.** Index 3 and index 7 are two unrelated
textures and their average is a third, so :func:`merge_layer` refuses
``RESOLVE`` on textures outright rather than trust a caller not to ask.

**Severity is reported, not acted on.** A resolved conflict is classified minor
or major by how far the compromise sits from the smaller edit. Both produce the
same merged value; the classification exists so a report can say which cells
deserve a human's attention. Merged Lands uses it to colour its conflict maps,
and keeping the distinction means the same is possible here.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Final

from wraithguard.land.curvature import structure_introduced
from wraithguard.land.diff import LandData, RelativeGrid


class ConflictStrategy(Enum):
    """How to settle a vertex both plugins moved."""

    #: Use whichever strategy suits the layer. See :data:`DEFAULT_STRATEGY`.
    AUTO = "auto"
    #: Average the two edits, weighted toward the larger.
    RESOLVE = "resolve"
    #: Take the later plugin's edit.
    OVERWRITE = "overwrite"
    #: Keep the earlier plugin's edit.
    IGNORE = "ignore"
    #: Average the two edits, weighted by how much *structure* each introduced
    #: rather than by how far it moved the vertex. Heights only. Opt-in: it
    #: produces different terrain from :attr:`RESOLVE` and is not the default.
    CURVATURE = "curvature"


class Severity(Enum):
    """How far a resolved conflict sits from the edits it replaced."""

    #: The compromise is close enough to be unnoticeable in play.
    MINOR = "minor"
    #: The compromise is far from at least one mod's intent. Worth a look.
    MAJOR = "major"


#: What each layer does when asked for :attr:`ConflictStrategy.AUTO`.
#:
#: Every layer is load-order-winner (:attr:`ConflictStrategy.OVERWRITE`): the
#: later plugin wins the vertices it changed, and only those. Blending is opt-in
#: per layer through a ``.mergedlands.toml``. The dict is kept per-layer rather
#: than collapsed to a single value so a future default can differ by layer
#: again without touching callers. See the module docstring for why the default
#: is not ``RESOLVE``.
DEFAULT_STRATEGY: Final[dict[LandData, ConflictStrategy]] = {
    LandData.VERTEX_HEIGHTS: ConflictStrategy.OVERWRITE,
    LandData.VERTEX_NORMALS: ConflictStrategy.OVERWRITE,
    LandData.VERTEX_COLORS: ConflictStrategy.OVERWRITE,
    LandData.WORLD_MAP: ConflictStrategy.OVERWRITE,
    LandData.TEXTURES: ConflictStrategy.OVERWRITE,
}

#: Layers whose values are identifiers rather than quantities. Averaging one
#: produces a valid-looking number that means something unrelated, so it is
#: refused rather than left to a caller's judgement.
CATEGORICAL_LAYERS: Final[frozenset[LandData]] = frozenset({LandData.TEXTURES})

#: Weighting exponent for the averaged compromise. Above 1 it biases toward the
#: larger edit; at 1 it would be a plain magnitude-weighted mean. Merged Lands
#: uses 1.5 and the value is kept rather than re-derived -- it is a tuning
#: choice made against real terrain, not something a proof settles.
_BIAS: Final = 1.5

#: How hard :attr:`ConflictStrategy.CURVATURE` leans on introduced structure.
#: At 0 it degrades exactly to :attr:`ConflictStrategy.RESOLVE`; higher values
#: let a small structural edit outweigh a large featureless one. Measured on a
#: synthetic case, a +500 bulk shift introduces 0.0 radians of structure while
#: a -60 road cut introduces 0.297, so a factor of 8 makes the cut roughly
#: competitive with a shift eight times its size. Chosen to be assertive
#: enough to matter and mild enough that magnitude still dominates when
#: neither edit has structure.
_CURVATURE_FACTOR: Final = 8.0

#: Layers :attr:`ConflictStrategy.CURVATURE` can be applied to. Structure is a
#: property of a height surface; a colour or an index has no gradient to bend.
CURVATURE_LAYERS: Final[frozenset[LandData]] = frozenset({LandData.VERTEX_HEIGHTS})


@dataclass(frozen=True, slots=True)
class ConflictParams:
    """Thresholds separating a minor compromise from a major one.

    The defaults come from Merged Lands, chosen so that a *minor* conflict is
    unlikely to be visible in play.

    Attributes:
        minor_threshold_pct: Share of the smaller edit tolerated before a
            compromise counts as major.
        minor_threshold_min: A floor, so tiny edits do not report a major
            conflict over a rounding-sized difference.
        minor_threshold_max: A ceiling, so enormous edits cannot mask a large
            absolute difference behind a large percentage.
    """

    minor_threshold_pct: float = 0.3
    minor_threshold_min: float = 10.0
    minor_threshold_max: float = 64.0


@dataclass(slots=True)
class MergeReport:
    """What happened while merging one layer.

    Attributes:
        strategy: The strategy actually used, after resolving ``AUTO``.
        taken_from_one: Vertices only the earlier plugin moved.
        taken_from_two: Vertices only the later plugin moved.
        contested: Vertices both moved.
        minor: Contested vertices resolved to a close compromise.
        major: Contested vertices whose compromise is far from an edit.
    """

    strategy: ConflictStrategy
    taken_from_one: int = 0
    taken_from_two: int = 0
    contested: int = 0
    minor: int = 0
    major: int = 0
    #: Where the major conflicts are, for a report to point at.
    major_vertices: list[tuple[int, int]] = field(default_factory=list)

    @property
    def mergeable(self) -> int:
        """Vertices exactly one plugin moved, which a load order would discard."""
        return self.taken_from_one + self.taken_from_two


def average_delta(first: int, second: int, params: ConflictParams) -> tuple[int, Severity]:
    """Blend two edits, biased toward the larger, and rate the compromise.

    Weighting by magnitude means an intentional-looking large edit dominates an
    incidental small one, rather than both being pulled to a midpoint that
    neither mod wanted.

    Args:
        first: The earlier plugin's delta.
        second: The later plugin's delta.
        params: The severity thresholds.

    Returns:
        The blended delta and how far it sits from the smaller edit.
    """
    size_one, size_two = abs(first), abs(second)
    total = size_one + size_two
    if total == 0:
        # Only reachable if a caller passes two zero deltas, which is not a
        # conflict. Returning zero keeps the function total rather than
        # dividing by zero on an input the merge loop never produces.
        return 0, Severity.MINOR

    weight = size_one / total
    biased = weight**_BIAS
    other = (1.0 - weight) ** _BIAS
    weight = biased / (biased + other)
    blended = weight * first + (1.0 - weight) * second

    # Severity is measured against the *smaller* edit, because that is the one
    # a compromise treats worst: the larger edit is already close to the result.
    smaller = min(first, second)
    threshold = min(
        max(params.minor_threshold_pct * smaller, params.minor_threshold_min),
        params.minor_threshold_max,
    )
    severity = Severity.MAJOR if abs(smaller - blended) >= threshold else Severity.MINOR
    return int(blended), severity


def weighted_delta(
    first: int,
    second: int,
    weight_one: float,
    weight_two: float,
    params: ConflictParams,
) -> tuple[int, Severity]:
    """Blend two edits with caller-supplied weights.

    :func:`average_delta` weights by magnitude alone. This lets a caller supply
    its own notion of which edit deserves more say -- see
    :attr:`ConflictStrategy.CURVATURE`.

    Args:
        first: The earlier plugin's delta.
        second: The later plugin's delta.
        weight_one: How much the earlier edit counts. Must not be negative.
        weight_two: How much the later edit counts.
        params: The severity thresholds.

    Returns:
        The blended delta and how far it sits from the smaller edit.
    """
    total = weight_one + weight_two
    if total <= 0.0:
        # Both weights vanished, which means neither edit carries the signal
        # this weighting measures. Falling back to magnitude is better than
        # returning zero: zero would silently discard both mods' work.
        return average_delta(first, second, params)

    share = weight_one / total
    blended = share * first + (1.0 - share) * second

    smaller = min(first, second)
    threshold = min(
        max(params.minor_threshold_pct * smaller, params.minor_threshold_min),
        params.minor_threshold_max,
    )
    severity = Severity.MAJOR if abs(smaller - blended) >= threshold else Severity.MINOR
    return int(blended), severity


def _rows_of(grid: RelativeGrid, *, applied: bool) -> list[list[float]]:
    """Read a single-component grid back as rows of world-unit heights.

    Args:
        grid: The differences.
        applied: ``True`` for the plugin's terrain, ``False`` for the
            reference it was measured against.

    Returns:
        Rows of heights.
    """
    flat = grid.to_flat() if applied else grid.to_flat_reference()
    side = grid.side
    return [[float(v) for v in flat[y * side : (y + 1) * side]] for y in range(side)]


def _resolve_for(layer: LandData, strategy: ConflictStrategy) -> ConflictStrategy:
    """Turn ``AUTO`` into the strategy a layer should use.

    Args:
        layer: The layer being merged.
        strategy: What the caller asked for.

    Returns:
        A concrete strategy.

    Raises:
        ValueError: If ``RESOLVE`` is requested for a categorical layer, where
            averaging would name a texture neither plugin chose.
    """
    if strategy is ConflictStrategy.AUTO:
        return DEFAULT_STRATEGY.get(layer, ConflictStrategy.OVERWRITE)
    averaging = strategy in (ConflictStrategy.RESOLVE, ConflictStrategy.CURVATURE)
    if averaging and layer in CATEGORICAL_LAYERS:
        raise ValueError(
            f"{layer.name} holds identifiers, not quantities: averaging index 3 "
            "and index 7 gives index 5, which is a third unrelated texture. "
            "Use OVERWRITE or IGNORE."
        )
    if strategy is ConflictStrategy.CURVATURE and layer not in CURVATURE_LAYERS:
        raise ValueError(
            f"CURVATURE weights an edit by the structure it adds to a height "
            f"surface, and {layer.name} has no surface to bend. Use RESOLVE."
        )
    return strategy


def merge_layer(
    layer: LandData,
    first: RelativeGrid,
    second: RelativeGrid,
    strategy: ConflictStrategy = ConflictStrategy.AUTO,
    params: ConflictParams | None = None,
) -> tuple[RelativeGrid, MergeReport]:
    """Combine two plugins' edits to one layer of one cell.

    Both grids must share a reference -- they are differences against the same
    landmass, which is what makes their deltas comparable.

    Args:
        layer: Which layer this is, used to pick a default strategy.
        first: The earlier plugin's differences.
        second: The later plugin's differences.
        strategy: How to settle contested vertices.
        params: Severity thresholds, or ``None`` for the defaults.

    Returns:
        The merged differences and a report of what was decided.

    Raises:
        ValueError: If the grids differ in shape, or if ``RESOLVE`` is asked
            for a categorical layer.
    """
    if first.side != second.side or first.components != second.components:
        raise ValueError(
            f"cannot merge a {first.side}x{first.side}x{first.components} grid "
            f"with a {second.side}x{second.side}x{second.components} one"
        )

    chosen = _resolve_for(layer, strategy)
    thresholds = params if params is not None else ConflictParams()
    merged = RelativeGrid(first.to_flat_reference(), first.side, first.components)
    report = MergeReport(strategy=chosen)

    # Curvature weighting needs each side's *terrain*, not just its deltas,
    # because structure is a property of the surface. Reconstructed once here
    # rather than per contested vertex.
    surfaces: tuple[list[list[float]], list[list[float]], list[list[float]]] | None = None
    if chosen is ConflictStrategy.CURVATURE:
        surfaces = (
            _rows_of(first, applied=False),
            _rows_of(first, applied=True),
            _rows_of(second, applied=True),
        )

    for y in range(first.side):
        for x in range(first.side):
            moved_one = first.has_difference(x, y)
            moved_two = second.has_difference(x, y)

            if not moved_one and not moved_two:
                continue
            if moved_one and not moved_two:
                report.taken_from_one += 1
                merged.set_deltas(x, y, first.deltas_at(x, y))
                continue
            if moved_two and not moved_one:
                report.taken_from_two += 1
                merged.set_deltas(x, y, second.deltas_at(x, y))
                continue

            report.contested += 1
            if chosen is ConflictStrategy.OVERWRITE:
                merged.set_deltas(x, y, second.deltas_at(x, y))
                continue
            if chosen is ConflictStrategy.IGNORE:
                merged.set_deltas(x, y, first.deltas_at(x, y))
                continue

            blended: list[int] = []
            worst = Severity.MINOR
            deltas_one = first.deltas_at(x, y)
            deltas_two = second.deltas_at(x, y)

            if surfaces is not None:
                reference, terrain_one, terrain_two = surfaces
                # Weight by magnitude *scaled* by introduced structure, not by
                # structure alone: two edits that both add no structure still
                # have to be told apart, and magnitude is the only signal left.
                added_one = structure_introduced(reference, terrain_one, x, y)
                added_two = structure_introduced(reference, terrain_two, x, y)
                weight_one = abs(deltas_one[0]) * (1.0 + added_one * _CURVATURE_FACTOR)
                weight_two = abs(deltas_two[0]) * (1.0 + added_two * _CURVATURE_FACTOR)
                value, severity = weighted_delta(
                    deltas_one[0], deltas_two[0], weight_one, weight_two, thresholds
                )
                blended.append(value)
                worst = severity
            else:
                for one, two in zip(deltas_one, deltas_two):
                    value, severity = average_delta(one, two, thresholds)
                    blended.append(value)
                    if severity is Severity.MAJOR:
                        worst = Severity.MAJOR
            merged.set_deltas(x, y, tuple(blended))
            if worst is Severity.MAJOR:
                report.major += 1
                report.major_vertices.append((x, y))
            else:
                report.minor += 1

    return merged, report
