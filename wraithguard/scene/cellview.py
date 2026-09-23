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
from wraithguard.scene.terrain import (
    ALL_EDGES,
    EDGE_NEIGHBOUR,
    SKIRT_DROP,
    cell_texture_grid,
    has_terrain,
    landscape_textures,
    terrain_blend_meshes,
    terrain_skirt_mesh,
)
from wraithguard.scene.water import SEA_LEVEL, water_mesh, water_skirt_mesh
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

#: What :func:`_add_terrain` records per built cell for the skirt pass: the
#: landscape, its resolved texture paths, whether it dips below sea level, and the
#: cell's lowest vertex height (so a block of skirts can share one flat base).
_Ground = tuple[Landscape, dict[int, str], bool, float]


def preview_cell_instanced(
    plugins: Sequence[LoadedPlugin],
    key: CellKey,
    load_mesh: Callable[[str], list[Mesh] | None],
    *,
    include_adjacent: bool = False,
    workers: int = 1,
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
        workers: How many threads to parse the cell's unique models on. ``>1``
            parses them concurrently (worth it only on a free-threaded
            interpreter); ``load_mesh`` must then be thread-safe.

    Returns:
        ``(placements, audit, instanced_cell)`` -- the placements and audit are
        the *picked* cell's alone; adjacent cells are context, not part of what
        the load order does to this cell.
    """
    model_index = build_model_index(record for plugin in plugins for record in plugin.records)
    layers = cell_layers(plugins, key)
    placements, audit = resolve_cell(layers, model_index)
    cell = build_instanced(placements, load_mesh, workers=workers)
    if key.grid is not None:
        ground: dict[tuple[int, int], _Ground] = {}
        # Resolved neighbour texture grids, shared across every cell's terrain so a
        # grid is decoded once however many cells border it (cross-cell blending).
        tex_cache: dict[tuple[int, int], list[list[str]] | None] = {}
        _add_terrain(cell, plugins, key.grid, ground=ground, tex_cache=tex_cache)
        if include_adjacent:
            _add_adjacent_cells(
                cell, plugins, key.grid, model_index, load_mesh, ground=ground, tex_cache=tex_cache
            )
        _add_exterior_skirts(cell, key.grid, ground, include_adjacent=include_adjacent)
    else:
        _add_interior_water(cell, layers, placements)
    return placements, audit, cell


def _add_adjacent_cells(
    cell: InstancedCell,
    plugins: Sequence[LoadedPlugin],
    grid: tuple[int, int],
    model_index: ModelIndex,
    load_mesh: Callable[[str], list[Mesh] | None],
    *,
    ground: dict[tuple[int, int], _Ground] | None = None,
    tex_cache: dict[tuple[int, int], list[list[str]] | None] | None = None,
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
        ground: When given, each neighbour's terrain records
            ``grid -> (land, texture_paths, has_water)`` into it for the skirt pass.
        tex_cache: Shared cache of resolved neighbour texture grids, for cross-cell
            blending (see :func:`_add_terrain`).
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
        _add_terrain(
            neighbour, plugins, neighbour_grid, focus=True, ground=ground, tex_cache=tex_cache
        )
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
    # An interior is a single cell, so its water gets a full skirt around all four
    # edges (it has no neighbours to share an edge with).
    _append_skirt(cell, water_skirt_mesh(height, x0, y0, x1, y1), "Water")


def _append_water(
    cell: InstancedCell, height: float, x0: float, y0: float, x1: float, y1: float
) -> None:
    """Append a water plane group (drawn once, at identity, not framed on).

    The edge skirt is added separately (see :func:`_add_exterior_skirts` and
    :func:`_add_interior_water`), so it can wrap the outer perimeter of whatever
    is on screen rather than every cell's boundary.
    """
    mesh = water_mesh(height, x0, y0, x1, y1)
    identity = list(matrix4_columns(Transform()))
    cell.groups.append(
        InstancedGroup(model=mesh.name, meshes=[mesh], matrices=identity, record_type="Water")
    )
    cell.drawn += 1


def _append_skirt(
    cell: InstancedCell,
    mesh: Mesh,
    record_type: str,
    *,
    adjacent: bool = False,
    skirt_alone: bool = False,
) -> None:
    """Append a skirt mesh as its own group (drawn once, at identity, not framed on)."""
    if not mesh.triangles:
        return
    identity = list(matrix4_columns(Transform()))
    cell.groups.append(
        InstancedGroup(
            model=mesh.name,
            meshes=[mesh],
            matrices=identity,
            record_type=record_type,
            adjacent=adjacent,
            skirt_alone=skirt_alone,
        )
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


def _resolve_cell_tex(
    plugins: Sequence[LoadedPlugin],
    grid: tuple[int, int],
    cache: dict[tuple[int, int], list[list[str]] | None],
) -> list[list[str]] | None:
    """The 16x16 land-texture grid for one exterior grid, cached, or ``None``.

    Resolves the winning ``LAND`` and its owner's ``LTEX`` map (the same
    per-plugin scoping :func:`_add_terrain` uses), so a neighbour's textures blend
    against the numbering they were authored in.
    """
    if grid in cache:
        return cache[grid]
    grid_tex: list[list[str]] | None = None
    found = _winning_landscape(plugins, grid)
    if found is not None:
        land, owner = found
        if has_terrain(land):
            grid_tex = cell_texture_grid(land, _landscape_textures(plugins, owner))
    cache[grid] = grid_tex
    return grid_tex


def _neighbour_texture_grids(
    plugins: Sequence[LoadedPlugin],
    grid: tuple[int, int],
    cache: dict[tuple[int, int], list[list[str]] | None],
) -> dict[tuple[int, int], list[list[str]]]:
    """The eight surrounding cells' texture grids, keyed by ``(dx, dy)`` offset.

    Absent (no terrain) neighbours are omitted; the blend then repeats this cell's
    own edge on that side.
    """
    gx, gy = grid
    result: dict[tuple[int, int], list[list[str]]] = {}
    for dx, dy in _ADJACENT_OFFSETS:
        neighbour = _resolve_cell_tex(plugins, (gx + dx, gy + dy), cache)
        if neighbour is not None:
            result[(dx, dy)] = neighbour
    return result


def _add_terrain(
    cell: InstancedCell,
    plugins: Sequence[LoadedPlugin],
    grid: tuple[int, int],
    *,
    focus: bool = True,
    ground: dict[tuple[int, int], _Ground] | None = None,
    tex_cache: dict[tuple[int, int], list[list[str]] | None] | None = None,
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
        ground: When given, records ``grid -> (land, texture_paths, has_water)``
            for every cell that got terrain, so the skirt pass can wrap the outer
            perimeter without decoding the heights a second time.
        tex_cache: When given, the eight neighbours' texture grids are resolved
            (through it, cached per grid) and fed to the blend so the terrain
            textures blend across the cell seam instead of clamping at it.
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
    neighbours = (
        _neighbour_texture_grids(plugins, grid, tex_cache) if tex_cache is not None else None
    )
    try:
        meshes = terrain_blend_meshes(land, texture_paths, neighbours)
        if not meshes:
            return
    except (HeightEncodeError, struct.error):
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
    heights = [vertex[2] for vertex in meshes[0].vertices]
    has_water = any(z < SEA_LEVEL for z in heights)
    if has_water:
        _append_water(
            cell,
            SEA_LEVEL,
            gx * LAND_CELL_UNITS,
            gy * LAND_CELL_UNITS,
            (gx + 1) * LAND_CELL_UNITS,
            (gy + 1) * LAND_CELL_UNITS,
        )
    if ground is not None:
        ground[grid] = (land, texture_paths, has_water, min(heights))


def _neighbour(grid: tuple[int, int], edge: str) -> tuple[int, int]:
    """The grid on the far side of one of a cell's edges."""
    dx, dy = EDGE_NEIGHBOUR[edge]
    return (grid[0] + dx, grid[1] + dy)


def _add_exterior_skirts(
    cell: InstancedCell,
    focused_grid: tuple[int, int],
    ground: dict[tuple[int, int], _Ground],
    *,
    include_adjacent: bool,
) -> None:
    """Skirt the outer perimeter of the drawn terrain -- the one cell or the block.

    Two skirts are built so the plinth always hugs whatever is on screen: the
    picked cell's *full* skirt, shown only while neighbours are hidden; and, when
    neighbours were built, a *block* skirt that walls only the outer edges of the
    whole drawn block (an edge whose neighbour is also drawn is internal and left
    open), shown while neighbours are visible. The viewer swaps between them with
    the "Adjacent cells" toggle (see :attr:`InstancedGroup.skirt_alone`).

    Only terrain is skirted here. An exterior cell's sea plane spans exactly the
    cell, so its edge sits flush against the (opaque, taller) land skirt already --
    a water skirt there would only z-fight it. Interiors, which have no land skirt,
    get their water skirt in :func:`_add_interior_water`.

    Every skirt shares one flat ``bottom`` (the lowest point of the drawn terrain,
    less :data:`~wraithguard.scene.terrain.SKIRT_DROP`), so a block of plinths sits
    on one base rather than each dropping to its own uneven depth.

    Args:
        cell: The scene to add to, mutated in place.
        focused_grid: The picked exterior cell's ``(x, y)``.
        ground: ``grid -> (land, texture_paths, has_water, min_z)`` for every drawn
            cell.
        include_adjacent: Whether neighbouring cells were built.
    """
    if not ground:
        return
    terrain_grids = set(ground)
    bottom = min(min_z for (_land, _tex, _water, min_z) in ground.values()) - SKIRT_DROP

    # The lone-cell skirt: the picked cell's full plinth, shown when neighbours are
    # hidden (or there are none). Always added so toggling neighbours off restores it.
    focused = ground.get(focused_grid)
    if focused is not None:
        land, _texture_paths, _has_water, _min_z = focused
        skirt = terrain_skirt_mesh(land, bottom=bottom)
        _append_skirt(cell, skirt, "Terrain", skirt_alone=True)

    if not include_adjacent:
        return

    # The block skirt: only the outer edges of the whole drawn block, tagged
    # adjacent so it shows exactly when the neighbours do.
    for g, (land, _texture_paths, _has_water, _min_z) in ground.items():
        t_edges = tuple(e for e in ALL_EDGES if _neighbour(g, e) not in terrain_grids)
        if t_edges:
            skirt = terrain_skirt_mesh(land, edges=t_edges, bottom=bottom)
            _append_skirt(cell, skirt, "Terrain", adjacent=True)
