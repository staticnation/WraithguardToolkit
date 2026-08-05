"""Per-vertex differences between a reference landscape and a plugin's version.

**The idea the whole merger rests on.** A ``LAND`` record is a whole record, so
the load order keeps one version of a cell and discards the rest. But two mods
that both edit a cell have usually edited *different vertices* of it -- one
flattened a road, another raised a hill in the far corner. Comparing final
terrain against final terrain cannot see that; comparing each against the
*reference* terrain the game ships can. What each mod changed is then a sparse
set of moved vertices, and disjoint sets combine without loss.

So a :class:`RelativeGrid` holds three things: the reference values, the delta
each plugin applied, and a flag per vertex saying whether that delta is
non-zero. The flags are the useful part -- two mods conflict only where both
flags are set, and everywhere else the merge is unambiguous.

**Storage, and why it looks like this.** The toolkit has no third-party
dependencies (``pyproject.toml`` declares ``dependencies = []``) and ships as a
frozen binary, so there is no NumPy to lean on. A cell is 65x65 vertices across
up to five layers, and a large load order has thousands of modified cells; a
nested list of per-vertex tuples would allocate tens of millions of small
objects. Grids here are therefore *flat* lists of machine integers with
multi-component values interleaved, which keeps allocation proportional to
layers rather than vertices. The cost is that indexing is arithmetic instead of
``grid[y][x]``, so it is confined to :meth:`RelativeGrid.offset_of` and the
handful of methods that call it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntFlag
from typing import Final

from wraithguard.tes3fields.landscape import (
    LAND_SIZE,
    TEXTURE_SIZE,
    WNAM_SIZE,
    decode_texture_indices,
    decode_vertex_colors,
    decode_vertex_heights,
    decode_vertex_normals,
    decode_world_map,
)


#: Side of the world-map grid, re-exported so callers need one import.
class LandData(IntFlag):
    """Which layers of a landscape record carry data.

    Mirrors the ``LandscapeFlags`` a ``LAND`` record declares, but as a
    property of *what changed* rather than what the record contains. A plugin
    that edited only textures reports :attr:`TEXTURES` and nothing else, and a
    merge can skip four layers on the strength of it.
    """

    NONE = 0
    VERTEX_COLORS = 0b00010
    TEXTURES = 0b00100
    VERTEX_HEIGHTS = 0b01000
    VERTEX_NORMALS = 0b10000
    WORLD_MAP = 0b100000


#: Every layer -- the default "consider all layers" mask. Written as the
#: inverse of NONE so it tracks the members automatically. Wrapped in
#: ``LandData()`` because typeshed types ``IntFlag.__invert__`` as ``int``; the
#: value is identical, the constructor only restores the declared type.
ALL_LAYERS: Final[LandData] = LandData(~LandData.NONE)


#: The ``landscape_flags`` names tes3conv writes, mapped to the layers each
#: implies. Heights and normals share one flag in the record because the engine
#: stores and validates them together.
_RECORD_FLAG_NAMES: Final[dict[str, LandData]] = {
    "USES_VERTEX_HEIGHTS_AND_NORMALS": LandData.VERTEX_HEIGHTS | LandData.VERTEX_NORMALS,
    "USES_VERTEX_COLORS": LandData.VERTEX_COLORS,
    "USES_TEXTURES": LandData.TEXTURES,
}

#: ``WNAM`` has no flag of its own. tes3's ``uses_world_map_data()`` returns
#: true when *any* named bit is set, so a record declaring anything at all also
#: carries a world map. Mirroring that here keeps our reading of the flags the
#: same as the writer's.
_IMPLIES_WORLD_MAP: Final = LandData.VERTEX_HEIGHTS | LandData.VERTEX_COLORS | LandData.TEXTURES


#: The object flag a plugin sets to say it removed a record. tes3conv writes
#: object flags as a ``" | "``-joined list of names, so this is matched as a
#: token rather than a substring: ``UNDELETED`` would otherwise match.
DELETED_FLAG: Final = "DELETED"


def is_deleted(record: dict[str, object]) -> bool:
    """Whether a plugin marked this record deleted.

    A deleted record is not terrain. Its grids are whatever happened to be in
    the file when the editor struck it out, and reading them as an edit means
    merging a cell the mod explicitly removed.

    Merged Lands asserts on this and aborts the run
    (``textures.rs``: *"tried to add deleted LTEX"*, ``main.rs``: *"tried to
    add deleted LAND"*). We skip and log instead. Aborting a nine-hundred-mod
    merge because one plugin deleted one texture record is not a useful
    outcome, and the conservative result of skipping is that the reference
    terrain survives -- which is what a terrain-only patch should do with a
    record it cannot interpret.

    Args:
        record: A decoded record.

    Returns:
        ``True`` when its object flags include ``DELETED``.
    """
    flags = record.get("flags")
    if isinstance(flags, str):
        return DELETED_FLAG in (part.strip() for part in flags.split("|"))
    if isinstance(flags, list):
        return any(entry == DELETED_FLAG for entry in flags)
    return False


def parse_landscape_flags(value: str | None) -> LandData:
    """Read the ``landscape_flags`` field tes3conv writes.

    The field is flag names joined by ``" | "``, sometimes with a bare hex
    value for bits the converter has no name for. Unknown entries are ignored
    rather than refused: an unnamed bit is not a reason to abandon a cell whose
    named layers are perfectly readable.

    Args:
        value: The field's text, or ``None`` when absent.

    Returns:
        The layers the record declares.
    """
    if not value:
        return LandData.NONE
    flags = LandData.NONE
    for part in value.split("|"):
        flags |= _RECORD_FLAG_NAMES.get(part.strip(), LandData.NONE)
    if flags & _IMPLIES_WORLD_MAP:
        flags |= LandData.WORLD_MAP
    return flags


class RelativeGrid:
    """A reference grid plus the deltas one plugin applied to it.

    Values are stored flat and interleaved: a 65x65 grid of three-component
    normals is one list of 12,675 integers. See the module docstring for why.

    Attributes:
        side: Vertices per edge.
        components: Values per vertex -- 1 for heights, 3 for normals.
    """

    __slots__ = ("_changed", "_delta", "_reference", "components", "side")

    def __init__(self, reference: list[int], side: int, components: int = 1) -> None:
        """Wrap a reference grid with an all-zero set of deltas.

        Args:
            reference: Flat, interleaved reference values.
            side: Vertices per edge.
            components: Values per vertex.

        Raises:
            ValueError: If the list length does not match ``side`` and
                ``components``.
        """
        expected = side * side * components
        if len(reference) != expected:
            raise ValueError(
                f"expected {expected} values for a {side}x{side} grid of "
                f"{components}-component values, got {len(reference)}"
            )
        self._reference = reference
        self._delta = [0] * expected
        # One flag per *vertex*, not per component: a normal whose x moved has
        # moved, and asking the question per component would make every
        # conflict test three times as long for no extra information.
        self._changed = [False] * (side * side)
        self.side = side
        self.components = components

    @classmethod
    def from_difference(
        cls, reference: list[int], plugin: list[int], side: int, components: int = 1
    ) -> RelativeGrid:
        """Build the difference of a plugin's grid against a reference.

        Args:
            reference: The reference values.
            plugin: The plugin's values.
            side: Vertices per edge.
            components: Values per vertex.

        Returns:
            A grid whose deltas reproduce ``plugin`` when applied.

        Raises:
            ValueError: If either list is the wrong length.
        """
        grid = cls(reference, side, components)
        if len(plugin) != len(reference):
            raise ValueError(
                f"plugin grid has {len(plugin)} values, reference has {len(reference)}"
            )
        changed = grid._changed
        delta = grid._delta
        count = components
        for index, (was, now) in enumerate(zip(reference, plugin)):
            if was != now:
                delta[index] = now - was
                changed[index // count] = True
        return grid

    def offset_of(self, x: int, y: int, component: int = 0) -> int:
        """Index of one component of one vertex in the flat storage.

        Args:
            x: Column.
            y: Row.
            component: Which component of the vertex.

        Returns:
            The flat index.
        """
        return (y * self.side + x) * self.components + component

    def value_at(self, x: int, y: int, component: int = 0) -> int:
        """The plugin's value at a vertex: reference plus delta.

        Args:
            x: Column.
            y: Row.
            component: Which component.

        Returns:
            The resulting value.
        """
        index = self.offset_of(x, y, component)
        return self._reference[index] + self._delta[index]

    def delta_at(self, x: int, y: int, component: int = 0) -> int:
        """The delta at a vertex.

        Args:
            x: Column.
            y: Row.
            component: Which component.

        Returns:
            The delta, zero when unchanged.
        """
        return self._delta[self.offset_of(x, y, component)]

    def has_difference(self, x: int, y: int) -> bool:
        """Whether this plugin moved a vertex at all.

        Args:
            x: Column.
            y: Row.

        Returns:
            ``True`` when any component of the vertex changed.
        """
        return self._changed[y * self.side + x]

    def set_value(self, x: int, y: int, values: tuple[int, ...]) -> None:
        """Set every component of a vertex, recomputing its delta.

        Args:
            x: Column.
            y: Row.
            values: One value per component.

        Raises:
            ValueError: If the wrong number of components is supplied.
        """
        if len(values) != self.components:
            raise ValueError(f"expected {self.components} component(s), got {len(values)}")
        changed = False
        for component, value in enumerate(values):
            index = self.offset_of(x, y, component)
            delta = value - self._reference[index]
            self._delta[index] = delta
            changed = changed or delta != 0
        self._changed[y * self.side + x] = changed

    def deltas_at(self, x: int, y: int) -> tuple[int, ...]:
        """Every component's delta at a vertex.

        Args:
            x: Column.
            y: Row.

        Returns:
            One delta per component.
        """
        start = (y * self.side + x) * self.components
        return tuple(self._delta[start : start + self.components])

    def set_deltas(self, x: int, y: int, deltas: tuple[int, ...]) -> None:
        """Set a vertex's deltas directly, without going via a value.

        Merging works in deltas rather than values -- two plugins' edits are
        combined as *changes* against a shared reference, and converting each
        back to an absolute value first would lose exactly the information the
        merge needs.

        Args:
            x: Column.
            y: Row.
            deltas: One delta per component.

        Raises:
            ValueError: If the wrong number of components is supplied.
        """
        if len(deltas) != self.components:
            raise ValueError(f"expected {self.components} delta(s), got {len(deltas)}")
        start = (y * self.side + x) * self.components
        changed = False
        for offset, delta in enumerate(deltas):
            self._delta[start + offset] = delta
            changed = changed or delta != 0
        self._changed[y * self.side + x] = changed

    def to_flat_reference(self) -> list[int]:
        """A copy of the reference values, with no deltas applied.

        Returns:
            The reference grid, safe for another :class:`RelativeGrid` to own.
        """
        return list(self._reference)

    def clear(self, x: int, y: int) -> None:
        """Discard a vertex's delta, returning it to the reference.

        Args:
            x: Column.
            y: Row.
        """
        for component in range(self.components):
            self._delta[self.offset_of(x, y, component)] = 0
        self._changed[y * self.side + x] = False

    @property
    def is_modified(self) -> bool:
        """Whether the plugin changed anything in this grid."""
        return any(self._changed)

    @property
    def num_differences(self) -> int:
        """How many vertices the plugin moved."""
        return sum(self._changed)

    def changed_vertices(self) -> list[tuple[int, int]]:
        """Every vertex this plugin moved.

        Returns:
            ``(x, y)`` pairs, row-major.
        """
        side = self.side
        return [(index % side, index // side) for index, moved in enumerate(self._changed) if moved]

    def to_flat(self) -> list[int]:
        """The plugin's grid as flat values.

        Returns:
            Reference plus delta, component by component.
        """
        return [was + delta for was, delta in zip(self._reference, self._delta)]

    def to_rows(self) -> list[list[int]]:
        """The grid as rows, for single-component grids.

        Returns:
            ``side`` rows of ``side`` values.

        Raises:
            ValueError: If the grid has more than one component per vertex.
        """
        if self.components != 1:
            raise ValueError("to_rows is only meaningful for single-component grids")
        flat = self.to_flat()
        return [flat[y * self.side : (y + 1) * self.side] for y in range(self.side)]


def _flatten(rows: list[list[int]] | list[list[float]]) -> list[int]:
    """Flatten a grid of scalars to a list of ints.

    Args:
        rows: The grid.

    Returns:
        Row-major values.
    """
    return [int(value) for row in rows for value in row]


def _flatten_triples(rows: list[list[tuple[int, int, int]]]) -> list[int]:
    """Flatten a grid of triples, interleaved.

    Args:
        rows: The grid.

    Returns:
        Row-major, component-interleaved values.
    """
    return [component for row in rows for triple in row for component in triple]


@dataclass(slots=True)
class LandscapeLayers:
    """One cell's landscape layers, decoded and flattened.

    This is the plain data a ``LAND`` record carries, in the flat form
    :class:`RelativeGrid` consumes. Absent layers are ``None`` rather than
    zeroed, because "this mod did not supply vertex colours" and "this mod set
    every vertex colour to black" are different claims and merging them the
    same way would be a bug.

    Attributes:
        coords: The cell's exterior grid coordinates.
        declared: The layers the record's flags claim it holds.
    """

    coords: tuple[int, int]
    declared: LandData
    heights: list[int] | None = None
    normals: list[int] | None = None
    world_map: list[int] | None = None
    colors: list[int] | None = None
    textures: list[int] | None = None

    @classmethod
    def from_record(cls, record: dict[str, object]) -> LandscapeLayers:
        """Decode a ``Landscape`` record as tes3conv writes it.

        Every grid arrives base64-encoded with a zstd frame underneath;
        :mod:`wraithguard.tes3fields.landscape` already handles both and is
        reused here rather than reimplemented.

        Args:
            record: One decoded JSON record with ``type == "Landscape"``.

        Returns:
            The decoded layers.

        Raises:
            ValueError: If the record has no usable grid coordinates.
        """
        grid = record.get("grid")
        if not isinstance(grid, (list, tuple)) or len(grid) != 2:
            raise ValueError(f"landscape record has no grid coordinates: {grid!r}")
        coords = (int(grid[0]), int(grid[1]))

        raw_flags = record.get("landscape_flags")
        declared = parse_landscape_flags(raw_flags if isinstance(raw_flags, str) else None)
        layers = cls(coords=coords, declared=declared)

        heights = record.get("vertex_heights")
        if isinstance(heights, dict) and heights.get("data"):
            offset = heights.get("offset", 0.0)
            layers.heights = _flatten(
                decode_vertex_heights(
                    heights["data"], float(offset) if isinstance(offset, (int, float)) else 0.0
                )
            )

        normals = record.get("vertex_normals")
        if isinstance(normals, dict) and normals.get("data"):
            layers.normals = _flatten_triples(decode_vertex_normals(normals["data"]))

        world_map = record.get("world_map_data")
        if isinstance(world_map, dict) and world_map.get("data"):
            layers.world_map = _flatten(decode_world_map(world_map["data"]))

        colors = record.get("vertex_colors")
        if isinstance(colors, dict) and colors.get("data"):
            layers.colors = _flatten_triples(decode_vertex_colors(colors["data"]))

        textures = record.get("texture_indices")
        if isinstance(textures, dict) and textures.get("data"):
            layers.textures = _flatten(decode_texture_indices(textures["data"]))

        return layers


@dataclass(slots=True)
class LandscapeDiff:
    """What one plugin changed in one cell, relative to the reference.

    Attributes:
        coords: The cell's exterior grid coordinates.
        plugin: The plugin the differences came from.
    """

    coords: tuple[int, int]
    plugin: str
    #: Whether the masters had no terrain here, so this plugin *added* land.
    #: Every vertex then reads as changed, which is literally true and
    #: statistically misleading: a new cell is not a conflict recovered from a
    #: load order, it is terrain that never existed. Reports must separate the
    #: two or the totals are dominated by landmass mods.
    new_land: bool = False
    heights: RelativeGrid | None = None
    normals: RelativeGrid | None = None
    world_map: RelativeGrid | None = None
    colors: RelativeGrid | None = None
    textures: RelativeGrid | None = None
    #: Layers the plugin declared but supplied no data for, worth reporting
    #: because it usually means a damaged or hand-edited record.
    missing: list[str] = field(default_factory=list)

    @property
    def is_modified(self) -> bool:
        """Whether this plugin changed the cell at all."""
        return any(
            grid is not None and grid.is_modified
            for grid in (self.heights, self.normals, self.world_map, self.colors, self.textures)
        )

    @property
    def modified_data(self) -> LandData:
        """Which layers this plugin actually changed.

        Distinct from what the record *declares*: a plugin frequently rewrites
        a whole ``LAND`` record while changing only one layer of it, and only
        the layers that really moved need merging.
        """
        modified = LandData.NONE
        for grid, flag in (
            (self.heights, LandData.VERTEX_HEIGHTS),
            (self.normals, LandData.VERTEX_NORMALS),
            (self.world_map, LandData.WORLD_MAP),
            (self.colors, LandData.VERTEX_COLORS),
            (self.textures, LandData.TEXTURES),
        ):
            if grid is not None and grid.is_modified:
                modified |= flag
        return modified

    @property
    def num_differences(self) -> int:
        """Total moved vertices across every layer."""
        return sum(
            grid.num_differences
            for grid in (self.heights, self.normals, self.world_map, self.colors, self.textures)
            if grid is not None
        )


#: Each layer's name, side length and components per vertex.
_LAYERS: Final[tuple[tuple[str, int, int, LandData], ...]] = (
    ("heights", LAND_SIZE, 1, LandData.VERTEX_HEIGHTS),
    ("normals", LAND_SIZE, 3, LandData.VERTEX_NORMALS),
    ("world_map", WNAM_SIZE, 1, LandData.WORLD_MAP),
    ("colors", LAND_SIZE, 3, LandData.VERTEX_COLORS),
    ("textures", TEXTURE_SIZE, 1, LandData.TEXTURES),
)


def diff_against_reference(
    plugin_name: str,
    plugin: LandscapeLayers,
    reference: LandscapeLayers | None,
    allowed: LandData = ALL_LAYERS,
) -> LandscapeDiff:
    """Compute what a plugin changed in a cell, against the reference terrain.

    **A layer the record does not declare is not a layer.** ``DATA`` says which
    grids a ``LAND`` record actually uses, and the engine ignores the rest --
    "If the relevant bit isn't set, the related fields will not be loaded, even
    if present" (UESP). tes3conv nonetheless emits every grid, so an undeclared
    one arrives full of zeros. Diffing it reads as *this mod flattened the
    terrain and painted it black*.

    That is not hypothetical: of 290 landscape records in this repository's
    sample, 21 carry texture data the flags do not declare, 20 carry vertex
    colours and 6 carry heights. Merged Lands avoids this by starting
    ``find_allowed_data`` from the record's own flags, and so does this.

    Args:
        plugin_name: The plugin the layers came from, for reporting.
        plugin: The plugin's version of the cell.
        reference: The reference version, or ``None`` for a cell the masters
            do not contain -- a mod adding new land. Everything it supplies is
            then a change, which is the correct reading: there was nothing
            there before.
        allowed: Layers the caller will consider, from a ``.mergedlands.toml``
            or a command-line choice. Intersected with what the record
            declares; a caller cannot opt *into* a layer the record does not
            use.

    Returns:
        The differences, with a grid per layer the plugin actually changed and
        ``None`` for every layer it did not.
    """
    result = LandscapeDiff(coords=plugin.coords, plugin=plugin_name, new_land=reference is None)

    # The caller's choice and the record's own declaration both have to allow a
    # layer. See the docstring: an undeclared grid is zeros, not terrain.
    effective = allowed & plugin.declared

    for name, side, components, flag in _LAYERS:
        if not effective & flag:
            continue
        mine: list[int] | None = getattr(plugin, name)
        if mine is None:
            # Declared but absent. Worth surfacing: a record claiming heights
            # and carrying none is malformed, and a merge that silently treats
            # it as "no change" would hide that.
            result.missing.append(name)
            continue

        theirs: list[int] | None = getattr(reference, name) if reference is not None else None
        if theirs is None:
            theirs = [0] * len(mine)

        grid = RelativeGrid.from_difference(theirs, mine, side, components)
        if grid.is_modified:
            setattr(result, name, grid)

    return result
