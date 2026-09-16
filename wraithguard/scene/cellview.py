"""Turn parsed plugins into a previewable cell -- selection and orchestration.

Ties the pieces together without any IO: given every plugin's parsed records (in
load order), it lists the cells present, finds one cell's per-plugin layers, and
runs :func:`~wraithguard.scene.resolve.resolve_cell` +
:func:`~wraithguard.scene.build.build_scene`. The caller reads plugin bytes
(:func:`wraithguard.esp.plugin.read_plugin` / ``read_header``) and supplies the
mesh loader; everything here is pure and unit-tested.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from wraithguard.esp.flags import CellFlags
from wraithguard.esp.records.cell import Cell
from wraithguard.esp.records.landscape import Landscape
from wraithguard.land.heights import HeightEncodeError
from wraithguard.nif.geometry import Transform
from wraithguard.scene.build import InstancedGroup, build_instanced, build_scene, matrix4_columns
from wraithguard.scene.resolve import ACTOR_TAGS, build_model_index, resolve_cell
from wraithguard.scene.terrain import has_terrain, landscape_textures, terrain_blend_meshes
from wraithguard.scene.water import SEA_LEVEL, water_mesh
from wraithguard.tes3fields.landscape import LAND_CELL_UNITS

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

    from wraithguard.nif.geometry import Mesh
    from wraithguard.scene.build import BuiltScene, InstancedCell
    from wraithguard.scene.resolve import CellAudit, ModelIndex, Placement


@dataclass(frozen=True)
class LoadedPlugin:
    """One parsed plugin in the load order.

    Attributes:
        name: The plugin filename (for reference identity and reporting).
        masters: Its master filenames, in order.
        records: Its records, as :func:`wraithguard.esp.plugin.read_plugin`
            returns them.
    """

    name: str
    masters: Sequence[str]
    records: Sequence[object]


@dataclass(frozen=True)
class CellKey:
    """A load-order-wide identity for a cell: an interior name or an exterior grid."""

    interior: str | None = None
    grid: tuple[int, int] | None = None


def cell_key(cell: Cell) -> CellKey:
    """The identity of a cell record: its lower-cased name if interior, else grid."""
    if cell.data.cell_flags & CellFlags.IS_INTERIOR:
        return CellKey(interior=cell.name.lower(), grid=None)
    return CellKey(interior=None, grid=cell.data.grid)


def cell_label(cell: Cell) -> str:
    """A human label: the interior name, or the exterior region/name + grid."""
    if cell.data.cell_flags & CellFlags.IS_INTERIOR:
        return cell.name or "(unnamed interior)"
    named = cell.name or cell.region or "Wilderness"
    return f"{named} ({cell.data.grid[0]}, {cell.data.grid[1]})"


@dataclass
class CellChoice:
    """A cell offered for preview: its identity, a label, and where it appears."""

    key: CellKey
    label: str
    plugins: list[str] = field(default_factory=list)


def list_cells(plugins: Sequence[LoadedPlugin]) -> list[CellChoice]:
    """Every distinct cell across the load order, with the plugins that touch it.

    Interiors first (by name), then exteriors (by grid), so the picker reads in a
    predictable order.

    Args:
        plugins: The parsed load order.

    Returns:
        One :class:`CellChoice` per distinct cell.
    """
    choices: dict[CellKey, CellChoice] = {}
    for plugin in plugins:
        for record in plugin.records:
            if isinstance(record, Cell):
                key = cell_key(record)
                choice = choices.get(key)
                if choice is None:
                    choice = CellChoice(key=key, label=cell_label(record))
                    choices[key] = choice
                if plugin.name not in choice.plugins:
                    choice.plugins.append(plugin.name)
    return sorted(
        choices.values(),
        key=lambda c: (c.key.interior is None, c.key.interior or "", c.key.grid or (0, 0)),
    )


def cell_layers(
    plugins: Sequence[LoadedPlugin], key: CellKey
) -> list[tuple[str, Sequence[str], Cell | None]]:
    """Each plugin's version of one cell (``None`` when it does not touch it).

    Args:
        plugins: The parsed load order.
        key: The cell to collect, from :func:`cell_key`.

    Returns:
        ``(plugin_name, masters, cell_or_None)`` per plugin, in load order --
        the input :func:`~wraithguard.scene.resolve.resolve_cell` expects.
    """
    layers: list[tuple[str, Sequence[str], Cell | None]] = []
    for plugin in plugins:
        match: Cell | None = None
        for record in plugin.records:
            if isinstance(record, Cell) and cell_key(record) == key:
                match = record
                break
        layers.append((plugin.name, plugin.masters, match))
    return layers


def object_provenance(
    plugins: Sequence[LoadedPlugin], key: CellKey
) -> dict[str, dict[str, object]]:
    """Per-object provenance for the objects a cell places.

    For each object the cell references, records which plugins *define* its object
    record (the record that carries its mesh), which plugins *place* it in this
    cell, and the winning mesh and record type -- the data behind the viewer's
    "ori" readout when a mesh is clicked. Keyed by the object id lower-cased, the
    way the viewer looks it up from the clicked instance's id.

    Args:
        plugins: The parsed load order.
        key: The cell to inspect.

    Returns:
        ``{id_lower: {"id", "type", "model", "definedBy", "placedBy"}}`` for every
        object placed in the cell. ``definedBy`` and ``placedBy`` are plugin names
        in load order; the last of each is the version that wins.
    """
    index = build_model_index(record for plugin in plugins for record in plugin.records)
    # Who defines the object record (carrying a mesh) for each id -- the "touch".
    defined: dict[str, list[str]] = {}
    for plugin in plugins:
        for record in plugin.records:
            rid = getattr(record, "id", "")
            if not rid or getattr(record, "TAG", b"") in ACTOR_TAGS:
                continue
            if getattr(record, "mesh", ""):
                names = defined.setdefault(rid.lower(), [])
                if plugin.name not in names:
                    names.append(plugin.name)
    # Who places it in this cell -- the plugins whose version of the cell carries
    # a non-deleted reference to the id.
    display: dict[str, str] = {}
    placed: dict[str, list[str]] = {}
    for name, _masters, cell in cell_layers(plugins, key):
        if cell is None:
            continue
        for ref in cell.references:
            if ref.deleted:
                continue
            rid = ref.id.lower()
            display.setdefault(rid, ref.id)
            names = placed.setdefault(rid, [])
            if name not in names:
                names.append(name)
    return {
        rid: {
            "id": display[rid],
            "type": index.types.get(rid, ""),
            "model": index.meshes.get(rid, ""),
            "definedBy": defined.get(rid, []),
            "placedBy": names,
        }
        for rid, names in placed.items()
    }


def preview_cell(
    plugins: Sequence[LoadedPlugin],
    key: CellKey,
    load_mesh: Callable[[str], list[Mesh] | None],
) -> tuple[list[Placement], CellAudit, BuiltScene]:
    """Resolve a cell and assemble its scene.

    Args:
        plugins: The parsed load order.
        key: The cell to preview.
        load_mesh: Resolves a model path to its world meshes (see
            :func:`wraithguard.scene.build.build_scene`).

    Returns:
        ``(placements, audit, scene)``.
    """
    model_index = build_model_index(record for plugin in plugins for record in plugin.records)
    layers = cell_layers(plugins, key)
    placements, audit = resolve_cell(layers, model_index)
    scene = build_scene(placements, load_mesh)
    return placements, audit, scene


#: The eight grid offsets around a cell, in reading order (row by row, skipping
#: the centre). An exterior cell's neighbours; an interior has none.
_ADJACENT_OFFSETS: tuple[tuple[int, int], ...] = (
    (-1, 1), (0, 1), (1, 1),
    (-1, 0), (1, 0),
    (-1, -1), (0, -1), (1, -1),
)  # fmt: skip


def preview_cell_instanced(
    plugins: Sequence[LoadedPlugin],
    key: CellKey,
    load_mesh: Callable[[str], list[Mesh] | None],
    *,
    include_adjacent: bool = False,
) -> tuple[list[Placement], CellAudit, InstancedCell]:
    """Resolve a cell and assemble it for *instanced* drawing.

    Same resolution as :func:`preview_cell`, but the scene is grouped by model
    (see :func:`wraithguard.scene.build.build_instanced`) instead of baked, so a
    cell that reuses meshes hundreds of times stays small enough to serve and
    render.

    Args:
        plugins: The parsed load order.
        key: The cell to preview.
        load_mesh: Resolves a model path to its model-space meshes.
        include_adjacent: For an exterior cell, also assemble the eight
            neighbouring cells (their statics and terrain), so a merge seam or a
            floater reads against the ground next door. They are tagged
            ``adjacent`` and never framed on -- the camera still lands in the
            picked cell -- so the viewer can draw them behind a toggle. Ignored
            for an interior, which has no grid neighbours.

    Returns:
        ``(placements, audit, instanced_cell)`` -- the placements and audit are
        the *picked* cell's alone; adjacent cells are context, not part of what
        the load order does to this cell.
    """
    model_index = build_model_index(record for plugin in plugins for record in plugin.records)
    layers = cell_layers(plugins, key)
    placements, audit = resolve_cell(layers, model_index)
    cell = build_instanced(placements, load_mesh)
    if key.grid is not None:
        _add_terrain(cell, plugins, key.grid)
        if include_adjacent:
            _add_adjacent_cells(cell, plugins, key.grid, model_index, load_mesh)
    else:
        _add_interior_water(cell, layers, placements)
    return placements, audit, cell


def _add_adjacent_cells(
    cell: InstancedCell,
    plugins: Sequence[LoadedPlugin],
    grid: tuple[int, int],
    model_index: ModelIndex,
    load_mesh: Callable[[str], list[Mesh] | None],
) -> None:
    """Append the eight neighbouring exterior cells as context, mutating ``cell``.

    Every reference and every terrain vertex is already placed in absolute world
    units, so a neighbour needs no offset -- resolving it drops it exactly where
    it sits next to the picked cell. Each neighbour's groups are tagged
    ``adjacent`` and its terrain is *not* framed on, so the camera stays on the
    picked cell while the surrounding ground gives it context.

    Args:
        cell: The picked cell's scene, mutated in place.
        plugins: The parsed load order.
        grid: The picked exterior cell's ``(x, y)``.
        model_index: The load order's model index, reused across neighbours
            rather than rebuilt eight times.
        load_mesh: Resolves a model path to its model-space meshes.
    """
    gx, gy = grid
    for dx, dy in _ADJACENT_OFFSETS:
        neighbour_grid = (gx + dx, gy + dy)
        neighbour_key = CellKey(grid=neighbour_grid)
        layers = cell_layers(plugins, neighbour_key)
        placements, _audit = resolve_cell(layers, model_index)
        neighbour = build_instanced(placements, load_mesh)
        # Frame on a neighbour's terrain too: terrain is bounded (exactly its
        # cell), so including it just widens the view to the whole 3x3 the user
        # asked for, rather than leaving eight cells off-screen around a centred
        # one. Only strays among the *statics* are kept out of the framing.
        _add_terrain(neighbour, plugins, neighbour_grid, focus=True)
        for group in neighbour.groups:
            group.adjacent = True
            cell.groups.append(group)
        cell.drawn += neighbour.drawn
        for model in neighbour.missing_models:
            if model not in cell.missing_models:
                cell.missing_models.append(model)


def _winning_cell(layers: Sequence[tuple[str, Sequence[str], Cell | None]]) -> Cell | None:
    """The last plugin's version of the cell (what the game would load)."""
    for _name, _masters, record in reversed(list(layers)):
        if record is not None:
            return record
    return None


def _add_interior_water(
    cell: InstancedCell,
    layers: Sequence[tuple[str, Sequence[str], Cell | None]],
    placements: Sequence[Placement],
) -> None:
    """Add an interior cell's water plane, if it declares one.

    An interior's water sits at the height its ``WHGT`` gives (or 0 when the
    flag is set but no height is stored). Interiors have no grid, so the plane is
    sized to the objects in the cell -- a generous span past their extent -- which
    keeps it covering the pool without a real boundary to read from.

    Args:
        cell: The scene to add to, mutated in place.
        layers: The per-plugin cell records.
        placements: The cell's placements (for the plane's extent).
    """
    record = _winning_cell(layers)
    if record is None or CellFlags.HAS_WATER not in record.data.cell_flags:
        return
    height = record.water_height if record.water_height is not None else 0.0
    xs = [p.transform.translation[0] for p in placements]
    ys = [p.transform.translation[1] for p in placements]
    pad = 512.0
    x0 = (min(xs) if xs else -pad) - pad
    y0 = (min(ys) if ys else -pad) - pad
    x1 = (max(xs) if xs else pad) + pad
    y1 = (max(ys) if ys else pad) + pad
    _append_water(cell, height, x0, y0, x1, y1)


def _append_water(
    cell: InstancedCell, height: float, x0: float, y0: float, x1: float, y1: float
) -> None:
    """Append a water plane group (drawn once, at identity, not framed on)."""
    mesh = water_mesh(height, x0, y0, x1, y1)
    identity = list(matrix4_columns(Transform()))
    cell.groups.append(
        InstancedGroup(model=mesh.name, meshes=[mesh], matrices=identity, record_type="Water")
    )
    cell.drawn += 1


def _winning_landscape(
    plugins: Sequence[LoadedPlugin], grid: tuple[int, int]
) -> tuple[Landscape, LoadedPlugin] | None:
    """The winning ``LAND`` for a grid *and the plugin that provides it*.

    The owning plugin matters because a ``LAND``'s ``VTEX`` texture indices are
    resolved against **that plugin's** ``LTEX`` numbering, not a global one (see
    :func:`_landscape_textures`).
    """
    winning: tuple[Landscape, LoadedPlugin] | None = None
    for plugin in plugins:
        for record in plugin.records:
            if isinstance(record, Landscape) and record.grid == grid:
                winning = (record, plugin)
    return winning


def find_landscape(plugins: Sequence[LoadedPlugin], grid: tuple[int, int]) -> Landscape | None:
    """The winning ``LAND`` record for an exterior grid, or ``None``.

    Last plugin in load order wins, mirroring how the game resolves a cell's
    landscape when nothing has been merged. Merge Lands may have produced a
    single combined ``LAND``; that plugin loads last and so wins here too.

    Args:
        plugins: The parsed load order.
        grid: The exterior cell's ``(x, y)``.

    Returns:
        The landscape record, or ``None`` when no plugin provides one.
    """
    found = _winning_landscape(plugins, grid)
    return found[0] if found is not None else None


def _landscape_textures(plugins: Sequence[LoadedPlugin], owner: LoadedPlugin) -> dict[int, str]:
    """The ``LTEX`` index -> file map a ``LAND`` from ``owner`` resolves against.

    Land-texture (``LTEX``) indices are **per plugin**: every plugin numbers its
    own from zero, so index 5 in Morrowind.esm and index 5 in a big landmass mod
    (Tamriel Rebuilt reuses 0..327, right over the vanilla range) are unrelated
    textures. A ``LAND``'s ``VTEX`` grid refers to the numbering of the plugin
    that authored it, so it must be resolved against **only that plugin and its
    masters** -- exactly what the engine does with its ``(index, plugin)`` land
    texture lookup. Collapsing every plugin's ``LTEX`` into one last-wins table
    (which is what a naive global map does) makes a later mod's textures bleed
    over an earlier cell's terrain -- the wrong-ground bug this avoids.

    Args:
        plugins: The parsed load order (masters load before their dependents).
        owner: The plugin whose ``LAND`` is being drawn.

    Returns:
        ``{ltex_index: texture_file}`` from ``owner`` and its masters only, later
        (nearer ``owner``) winning -- the game's resolution order.
    """
    masters = {name.lower() for name in owner.masters}
    scope: list[object] = []
    for plugin in plugins:
        if plugin is owner or plugin.name.lower() in masters:
            scope.extend(plugin.records)
        if plugin is owner:
            break  # the masters all load before the owner; nothing after matters
    return landscape_textures(scope)


def _add_terrain(
    cell: InstancedCell,
    plugins: Sequence[LoadedPlugin],
    grid: tuple[int, int],
    *,
    focus: bool = True,
) -> None:
    """Append the exterior cell's terrain to a built scene, if it has any.

    The terrain is drawn once (an instance group with a single identity
    transform), split into one *blended* layer per land texture so the textures
    fade into one another at cell boundaries the way Morrowind draws them (see
    :func:`~wraithguard.scene.terrain.terrain_blend_meshes`). A malformed height
    grid is skipped rather than failing the whole preview -- the statics are worth
    showing on their own.

    Args:
        cell: The scene to add to, mutated in place.
        plugins: The parsed load order.
        grid: The exterior cell's ``(x, y)``.
        focus: Whether the camera should frame on this terrain. True for the
            picked cell (the camera lands in it); False for a neighbour, which is
            context and must not pull the view off the picked cell.
    """
    found = _winning_landscape(plugins, grid)
    if found is None:
        return
    land, owner = found
    if not has_terrain(land):
        return
    # Resolve this LAND's texture indices against its own plugin + masters, not a
    # global table -- LTEX indices collide across plugins (see _landscape_textures).
    texture_paths = _landscape_textures(plugins, owner)
    try:
        meshes = terrain_blend_meshes(land, texture_paths)
    except (HeightEncodeError, struct.error):
        return
    if not meshes:
        return
    identity = list(matrix4_columns(Transform()))
    gx, gy = grid
    cell.groups.append(
        InstancedGroup(
            model=f"terrain_{gx}_{gy}",
            meshes=meshes,
            matrices=identity,
            record_type="Terrain",
            focus=focus,
        )
    )
    cell.drawn += 1
    # Sea, only where the ground actually dips below it: a flat z=0 plane under a
    # dry inland cell would just hang under the terrain, so it is added only when
    # some vertex is below sea level (a coast, a river mouth, a swamp). Every
    # sub-mesh shares the cell's vertex grid, so the first one carries them all.
    if any(vertex[2] < SEA_LEVEL for vertex in meshes[0].vertices):
        _append_water(
            cell,
            SEA_LEVEL,
            gx * LAND_CELL_UNITS,
            gy * LAND_CELL_UNITS,
            (gx + 1) * LAND_CELL_UNITS,
            (gy + 1) * LAND_CELL_UNITS,
        )
