"""Turn an exterior cell's ``LAND`` record into a drawable terrain mesh.

An exterior cell carries its ground as a 65x65 grid of heights (plus per-vertex
colours and normals) rather than as placed objects. This decodes that grid into
one world-space :class:`~wraithguard.nif.geometry.Mesh` -- the heightmap
triangulated, positioned at the cell's world origin, and tinted by its vertex
colours -- so the terrain sits in the same absolute space as the cell's statics
and can be drawn by the same viewer.

Heights come back in world units from :func:`~wraithguard.land.heights` (the
decoder the Merge Lands feature already relies on). Three texture strategies live
here, all over the same geometry, in increasing fidelity:

* :func:`terrain_mesh` paints the cell's *dominant* land texture (the ``VTEX``
  frequency winner) over the whole surface -- one mesh, one texture. A first
  pass that reads as ground rather than a flat colour.
* :func:`terrain_meshes` *splats*: one sub-mesh per land texture, each carrying
  only the quads that texture paints. The right texture in the right place, but
  hard-edged at the seams.
* :func:`terrain_blend_meshes` *blends*: one full-cell layer per land texture,
  the base opaque and the rest faded in by a per-vertex coverage alpha, so the
  textures cross-fade at cell boundaries the way Morrowind draws them. This is
  the strategy the cell preview uses. The ``VTEX`` grid names one texture per
  16x16 cell; there is no stored per-texture vertex alpha, so the soft edge is
  reconstructed here from cell coverage rather than read from the file.

All three read the ``VTEX`` grid de-swizzled, the layout confirmed empirically in
:func:`wraithguard.tes3fields.landscape.decode_texture_indices`. The one
calibration point is that grid's orientation against the vertex grid: like
``VHGT`` and ``VCLR``, row 0 is taken as the south edge (the file's grids share an
orientation). If a previewed cell's ground reads mirrored or rotated, that
assumption -- in :func:`_texture_at` and :func:`_corner_weights` -- is where to
look.
"""

from __future__ import annotations

import itertools
import math
import struct
from collections import Counter
from typing import TYPE_CHECKING

from wraithguard.esp.flags import LandscapeFlags
from wraithguard.land.heights import decode_heights_from_deltas
from wraithguard.land.textures import ltex_of
from wraithguard.nif.geometry import Mesh
from wraithguard.tes3fields.landscape import (
    LAND_CELL_UNITS,
    LAND_NUM_VERTS,
    LAND_SIZE,
    NUM_TEXTURES,
    TEXTURE_SIZE,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from wraithguard.esp.records.landscape import Landscape

#: World units between adjacent land vertices (64 gaps span one cell).
_SPACING = LAND_CELL_UNITS / (LAND_SIZE - 1)

#: How many times a land texture tiles across one cell. Morrowind paints a
#: texture per 16x16 ``VTEX`` cell (a 4-quad block), so one tile per four quads
#: is the natural rate. A calibration point -- the *rate* only decides how coarse
#: the ground reads, not which texture goes where.
_UV_SCALE = 1.0 / 4.0

#: How far below the lowest edge vertex the terrain skirt hangs, in world units.
#: A cell is 8192 units across; ~1024 reads as a solid plinth without dominating.
SKIRT_DROP = 1024.0

#: UV units per world unit, so the skirt's texture tiles at the same scale as the
#: cell surface (whose grid UVs step ``_UV_SCALE`` every ``_SPACING`` units).
_SKIRT_UV = _UV_SCALE / _SPACING

#: The texture the skirt is painted with -- Morrowind's default land texture, so
#: every plinth reads as the same neutral ground regardless of what the cell
#: paints on top. Resolved like any base texture (``.dds`` preferred, ``.tga``
#: fallback), and ignored in favour of the vertex colours if it cannot be found.
SKIRT_TEXTURE = "_land_default.dds"

#: Quads per ``VTEX`` cell along one edge. 64 quads span the cell and ``VTEX`` is
#: 16x16, so each painted texture cell covers a 4x4 block of quads.
_QUADS_PER_VTEX = (LAND_SIZE - 1) // TEXTURE_SIZE


def landscape_textures(records: object) -> dict[int, str]:
    """Map every ``LTEX`` index to its texture file, last plugin winning.

    Args:
        records: An iterable of parsed records (across the load order, in order).

    Returns:
        ``{ltex_index: texture_file}`` for every landscape texture seen.
    """
    from wraithguard.esp.records.landscapetexture import LandscapeTexture

    paths: dict[int, str] = {}
    for record in records:  # type: ignore[attr-defined]
        if isinstance(record, LandscapeTexture) and record.file_name:
            paths[record.index] = record.file_name
    return paths


def _dominant_texture(vtex: bytes, paths: Mapping[int, str]) -> str:
    """The most-painted land texture in a cell's ``VTEX`` grid, or ``""``.

    A first pass at exterior ground: one texture for the whole cell rather than a
    per-region splat. It is the frequency winner, so it needs none of the ``VTEX``
    positional layout -- just the counts -- which keeps it robust while the
    per-block version is worked out.

    Args:
        vtex: The record's 16x16 ``VTEX`` bytes (256 unsigned shorts).
        paths: ``LTEX`` index -> texture file.

    Returns:
        The winning texture file, or ``""`` when nothing is painted or resolved.
    """
    if len(vtex) < 512:
        return ""
    counts: Counter[str] = Counter()
    for value in struct.unpack("<256H", vtex[:512]):
        index = ltex_of(value)
        if index is not None and index in paths:
            counts[paths[index]] += 1
    winner = counts.most_common(1)
    return winner[0][0] if winner else ""


def has_terrain(land: Landscape) -> bool:
    """Whether a ``LAND`` record actually carries a height grid to draw."""
    return LandscapeFlags.USES_VERTEX_HEIGHTS_AND_NORMALS in land.landscape_flags


def _vertex_colors(raw: bytes) -> list[tuple[float, float, float, float]]:
    """VCLR bytes (65x65 RGB, 0-255) as per-vertex RGBA floats, or empty.

    Args:
        raw: The record's ``vertex_colors`` bytes.

    Returns:
        One RGBA tuple (0-1) per vertex, or ``[]`` when the grid is absent or
        the wrong size -- the all-or-nothing rule the mesh's ``vertex_colors``
        field requires.
    """
    if len(raw) < LAND_NUM_VERTS * 3:
        return []
    return [
        (raw[i * 3] / 255.0, raw[i * 3 + 1] / 255.0, raw[i * 3 + 2] / 255.0, 1.0)
        for i in range(LAND_NUM_VERTS)
    ]


def _height_vertices(land: Landscape) -> list[tuple[float, float, float]]:
    """The cell's 65x65 vertices in world space, row-major from the south edge.

    Args:
        land: The landscape record.

    Returns:
        One ``(x, y, z)`` per vertex, at the cell's world origin.

    Raises:
        HeightEncodeError: If the height grid is malformed.
        struct.error: If the height bytes are the wrong length.
    """
    gx, gy = land.grid
    deltas = list(struct.unpack(f"<{LAND_NUM_VERTS}b", land.vertex_heights))
    heights = decode_heights_from_deltas(land.vertex_heights_offset, deltas)
    origin_x = gx * LAND_CELL_UNITS
    origin_y = gy * LAND_CELL_UNITS
    return [
        (origin_x + col * _SPACING, origin_y + row * _SPACING, heights[row][col])
        for row in range(LAND_SIZE)
        for col in range(LAND_SIZE)
    ]


def _quad_corners(row: int, col: int) -> tuple[tuple[int, int, int], tuple[int, int, int]]:
    """The two triangles of one quad, wound counter-clockwise seen from +Z.

    Winding this way means a one-sided render still shows the ground from the
    sky. The indices are into the row-major 65x65 vertex grid.

    Args:
        row: The quad's south-edge row (0..63).
        col: The quad's west-edge column (0..63).

    Returns:
        Two vertex-index triples.
    """
    v00 = row * LAND_SIZE + col
    v10 = v00 + 1
    v01 = v00 + LAND_SIZE
    v11 = v01 + 1
    return (v00, v10, v11), (v00, v11, v01)


def _grid_uvs() -> list[tuple[float, float]]:
    """Per-vertex UVs tiling a land texture across the cell (see :data:`_UV_SCALE`)."""
    return [
        (col * _UV_SCALE, row * _UV_SCALE) for row in range(LAND_SIZE) for col in range(LAND_SIZE)
    ]


def _colors_for(land: Landscape) -> list[tuple[float, float, float, float]]:
    """The cell's per-vertex colours, only when the record says it carries them.

    The field otherwise defaults to zeros of the right length, which would paint
    the whole cell black rather than leave it unshaded.
    """
    if LandscapeFlags.USES_VERTEX_COLORS in land.landscape_flags:
        return _vertex_colors(land.vertex_colors)
    return []


def _deswizzled_vtex(vtex: bytes) -> list[list[int]]:
    """The 16x16 ``VTEX`` grid in visual order, or ``[]`` when too short.

    ``VTEX`` is stored as sixteen 4x4 blocks, not row-major; this undoes that so
    ``grid[y][x]`` is the texture value at texture-cell ``(x, y)``. Mirrors
    :func:`wraithguard.tes3fields.landscape.decode_texture_indices`, whose
    docstring records the empirical confirmation of the layout -- reproduced here
    rather than called because that decoder expects a tes3conv-encoded field,
    while a ``LAND`` record holds the raw 512 ``VTEX`` bytes.

    Args:
        vtex: The record's raw ``VTEX`` bytes (256 unsigned shorts).

    Returns:
        16 rows of 16 raw ``VTEX`` values (``ltex_index + 1``; 0 = none), or
        ``[]`` when the field is too short to be a grid.
    """
    if len(vtex) < 2 * NUM_TEXTURES:
        return []
    flat = struct.unpack(f"<{NUM_TEXTURES}H", vtex[: 2 * NUM_TEXTURES])
    grid = [[0] * TEXTURE_SIZE for _ in range(TEXTURE_SIZE)]
    for k, value in enumerate(flat):
        x2, y2, x1, y1 = k % 4, (k // 4) % 4, (k // 16) % 4, k // 64
        grid[y1 * 4 + y2][x1 * 4 + x2] = value
    return grid


def _texture_at(grid: list[list[int]], row: int, col: int, texture_paths: Mapping[int, str]) -> str:
    """The texture file painted on the quad at vertex ``(row, col)``, or ``""``.

    A quad maps to the ``VTEX`` cell that contains it (:data:`_QUADS_PER_VTEX`
    quads per cell along each edge). ``VTEX`` row 0 is taken as the south edge, to
    match the vertex grid built by :func:`_height_vertices` -- the calibration
    point noted in the module docstring.

    Args:
        grid: The de-swizzled ``VTEX`` grid from :func:`_deswizzled_vtex`.
        row: The quad's south-edge vertex row (0..63).
        col: The quad's west-edge vertex column (0..63).
        texture_paths: ``LTEX`` index -> texture file.

    Returns:
        The texture file, or ``""`` when nothing is painted here or the painted
        index resolves to no known texture.
    """
    vy = row // _QUADS_PER_VTEX
    vx = col // _QUADS_PER_VTEX
    index = ltex_of(grid[vy][vx])
    if index is None:
        return ""
    return texture_paths.get(index, "")


def terrain_mesh(land: Landscape, texture_paths: Mapping[int, str] | None = None) -> Mesh:
    """Build a single world-space terrain mesh for one exterior ``LAND`` record.

    The whole cell as one mesh, painted with its *dominant* land texture -- the
    simple strategy. :func:`terrain_meshes` splats per region instead; this one
    is kept for callers and tests that want a single mesh and for the plain,
    untextured geometry.

    Args:
        land: The cell's landscape record (call :func:`has_terrain` first).
        texture_paths: ``LTEX`` index -> texture file (see
            :func:`landscape_textures`). When given and the cell paints textures,
            the terrain takes its most-used land texture tiled across the surface.
            Omitted, the terrain is untextured and reads by its vertex colours and
            shading alone.

    Returns:
        A :class:`~wraithguard.nif.geometry.Mesh` of the heightmap, placed at the
        cell's world origin, tinted by its vertex colours and textured with its
        dominant land texture when those are present.

    Raises:
        HeightEncodeError: If the height grid is malformed.
        struct.error: If the height bytes are the wrong length.
    """
    gx, gy = land.grid
    vertices = _height_vertices(land)
    triangles: list[tuple[int, int, int]] = []
    for row in range(LAND_SIZE - 1):
        for col in range(LAND_SIZE - 1):
            triangles.extend(_quad_corners(row, col))
    texture = ""
    uvs: list[tuple[float, float]] = []
    if texture_paths is not None and LandscapeFlags.USES_TEXTURES in land.landscape_flags:
        texture = _dominant_texture(land.texture_indices, texture_paths)
        if texture:
            uvs = _grid_uvs()
    return Mesh(
        name=f"terrain_{gx}_{gy}",
        vertices=vertices,
        triangles=triangles,
        uvs=uvs,
        texture=texture,
        vertex_colors=_colors_for(land),
    )


def terrain_meshes(land: Landscape, texture_paths: Mapping[int, str] | None = None) -> list[Mesh]:
    """Build one terrain mesh per land texture, splatting ``VTEX`` per region.

    Each sub-mesh shares the cell's full vertex grid but carries only the quads
    its texture paints, so the right texture lands in the right place. Quads whose
    ``VTEX`` cell is unpainted or resolves to no known texture fall into one
    untextured sub-mesh that reads by vertex colour, so no ground goes missing.

    Falls back to a single mesh (identical to :func:`terrain_mesh` untextured)
    when the record paints no textures, no paths are given, or nothing resolves --
    the honest plain-geometry view rather than an empty one.

    Args:
        land: The cell's landscape record (call :func:`has_terrain` first).
        texture_paths: ``LTEX`` index -> texture file (see
            :func:`landscape_textures`), or ``None`` for an untextured cell.

    Returns:
        One or more :class:`~wraithguard.nif.geometry.Mesh`, all at the cell's
        world origin. Ordered by first appearance of each texture, scanning the
        grid south-to-north, west-to-east.

    Raises:
        HeightEncodeError: If the height grid is malformed.
        struct.error: If the height bytes are the wrong length.
    """
    gx, gy = land.grid
    vertices = _height_vertices(land)
    colors = _colors_for(land)
    # No texture info: one plain mesh, same as terrain_mesh untextured. Returning
    # early here (rather than guarding the grid build) also narrows texture_paths
    # to non-None for the rest, which the splat loop needs.
    if texture_paths is None or LandscapeFlags.USES_TEXTURES not in land.landscape_flags:
        return [terrain_mesh(land, texture_paths)]
    grid = _deswizzled_vtex(land.texture_indices)
    if not grid:
        return [terrain_mesh(land, texture_paths)]

    # Bucket each quad's two triangles under the texture it is painted with,
    # keeping first-seen order so the output is deterministic.
    buckets: dict[str, list[tuple[int, int, int]]] = {}
    for row in range(LAND_SIZE - 1):
        for col in range(LAND_SIZE - 1):
            texture = _texture_at(grid, row, col, texture_paths)
            buckets.setdefault(texture, []).extend(_quad_corners(row, col))

    # Nothing resolved to a real texture: one plain mesh, not a texture-less
    # "splat" that would differ from terrain_mesh's untextured output for no gain.
    if set(buckets) == {""}:
        return [terrain_mesh(land)]

    uvs = _grid_uvs()
    meshes: list[Mesh] = []
    for order, (texture, triangles) in enumerate(buckets.items()):
        meshes.append(
            Mesh(
                name=f"terrain_{gx}_{gy}_{order}",
                vertices=vertices,
                triangles=triangles,
                # The untextured bucket reads by vertex colour, so it carries no
                # UVs -- a texture-less mesh with UVs would only mislead.
                uvs=uvs if texture else [],
                texture=texture,
                vertex_colors=colors,
            )
        )
    return meshes


def _cell_textures(grid: list[list[int]], texture_paths: Mapping[int, str]) -> list[list[str]]:
    """Resolve each ``VTEX`` cell to its texture file (``""`` when unpainted).

    Args:
        grid: The de-swizzled 16x16 ``VTEX`` grid from :func:`_deswizzled_vtex`.
        texture_paths: ``LTEX`` index -> texture file.

    Returns:
        A 16x16 grid of texture files, ``""`` where a cell is unpainted or its
        index resolves to no known texture.
    """
    resolved: list[list[str]] = []
    for row in grid:
        line: list[str] = []
        for value in row:
            index = ltex_of(value)
            line.append(texture_paths.get(index, "") if index is not None else "")
        resolved.append(line)
    return resolved


def _corner_weights(vertex: int) -> tuple[int, int, float]:
    """Map a vertex index (0..64) to the two ``VTEX`` cells it lies between.

    A ``VTEX`` cell spans four quads, so its *centre* sits at vertex ``4j + 2``.
    Placing the samples at cell centres is what makes a uniform region stay
    uniform (all four surrounding cells share its texture, so every weight lands
    on that one) and only ramps within roughly one cell of a boundary -- the soft
    transition Morrowind draws, rather than a gradient across every cell.

    The indices are **not** clamped to 0..15: a vertex on the cell's outer edge
    falls between cell 0 (or 15) and the cell *beyond* the boundary (-1 or 16). The
    caller samples that out-of-range cell from the neighbouring cell's edge (see
    :func:`_extended_texture_grid`), so the blend crosses the cell seam instead of
    stopping dead at it.

    Args:
        vertex: The vertex's row or column index (0..64).

    Returns:
        ``(cell_low, cell_high, frac)``: the two ``VTEX`` cell indices this vertex
        falls between (each in -1..16) and the 0..1 blend toward the high one.
    """
    coord = (vertex - 2) / 4.0
    low = math.floor(coord)
    frac = coord - low
    return low, low + 1, frac


def _vertex_texture_weights(extended: list[list[str]], row: int, col: int) -> dict[str, float]:
    """The blend weight of each land texture at one vertex, summing to 1.

    Bilinearly samples the four ``VTEX`` cells around the vertex (see
    :func:`_corner_weights`) and accumulates each corner's weight onto the
    texture that corner's cell paints. Deep inside a region every corner shares
    the texture, so it gets weight ~1; at a boundary the weight splits, which is
    the soft edge. Unpainted corners collect under ``""``.

    Args:
        extended: The 18x18 texture grid from :func:`_extended_texture_grid` -- the
            cell's own 16x16 plus a one-cell border from its neighbours, so an
            edge vertex blends across the cell seam. Cell index ``c`` (``-1..16``)
            is stored at ``extended[c + 1]``.
        row: The vertex's row (0..64).
        col: The vertex's column (0..64).

    Returns:
        ``{texture_file: weight}``, weights summing to 1 (an unpainted share, if
        any, sits under the ``""`` key).
    """
    cx0, cx1, fx = _corner_weights(col)
    cy0, cy1, fy = _corner_weights(row)
    corners = (
        (cx0, cy0, (1.0 - fx) * (1.0 - fy)),
        (cx1, cy0, fx * (1.0 - fy)),
        (cx0, cy1, (1.0 - fx) * fy),
        (cx1, cy1, fx * fy),
    )
    weights: dict[str, float] = {}
    for cx, cy, weight in corners:
        texture = extended[cy + 1][cx + 1]  # +1: the border cell -1 sits at index 0
        weights[texture] = weights.get(texture, 0.0) + weight
    return weights


def cell_texture_grid(
    land: Landscape, texture_paths: Mapping[int, str] | None
) -> list[list[str]] | None:
    """The cell's 16x16 resolved land-texture grid, or ``None`` if it has none.

    A small public wrapper so a caller (the cell view) can build a neighbour's
    texture grid to feed :func:`terrain_blend_meshes` for cross-cell blending.

    Args:
        land: The neighbour's landscape record.
        texture_paths: That landscape owner's ``LTEX`` index -> file map.

    Returns:
        The 16x16 grid (``""`` where unpainted/unresolved), or ``None`` when the
        cell paints no textures or carries no ``VTEX``.
    """
    if texture_paths is None or LandscapeFlags.USES_TEXTURES not in land.landscape_flags:
        return None
    grid = _deswizzled_vtex(land.texture_indices)
    if not grid:
        return None
    return _cell_textures(grid, texture_paths)


def _extended_texture_grid(
    cell_tex: list[list[str]],
    neighbours: Mapping[tuple[int, int], list[list[str]]] | None,
) -> list[list[str]]:
    """The cell's 16x16 texture grid with a one-cell border from its neighbours.

    Returns an 18x18 grid indexed so that ``VTEX`` cell ``c`` (``-1..16``) lives
    at ``[c + 1]``. The interior is the cell's own grid; the border ring is the
    adjacent edge (or corner) cell of the neighbour on that side, which is what
    lets an edge vertex's blend reach across the cell seam. A missing neighbour
    (no terrain there, or not resolved) falls back to repeating the cell's own
    edge, which reproduces the old hard-clamped edge -- no seam-crossing, but no
    artifact either.

    Args:
        cell_tex: The cell's own 16x16 resolved texture grid.
        neighbours: ``(dx, dy) -> 16x16 grid`` for the eight surrounding cells
            (``dy`` positive north, matching the vertex grid), any subset present.

    Returns:
        An 18x18 grid of texture files.
    """
    n = TEXTURE_SIZE
    neigh = neighbours or {}
    ext = [["" for _ in range(n + 2)] for _ in range(n + 2)]
    for er in range(n + 2):
        cr = er - 1
        for ec in range(n + 2):
            cc = ec - 1
            if 0 <= cr < n and 0 <= cc < n:
                ext[er][ec] = cell_tex[cr][cc]
                continue
            dx = -1 if cc < 0 else (1 if cc >= n else 0)
            dy = -1 if cr < 0 else (1 if cr >= n else 0)
            grid = neigh.get((dx, dy))
            if grid is not None:
                ext[er][ec] = grid[cr % n][cc % n]
            else:
                ext[er][ec] = cell_tex[min(max(cr, 0), n - 1)][min(max(cc, 0), n - 1)]
    return ext


def terrain_blend_meshes(
    land: Landscape,
    texture_paths: Mapping[int, str] | None = None,
    neighbours: Mapping[tuple[int, int], list[list[str]]] | None = None,
) -> list[Mesh]:
    """Build blended terrain layers, one per land texture, soft at the seams.

    Reproduces how Morrowind draws exterior ground: the ``VTEX`` grid names one
    texture per 16x16 cell, and the engine fades between neighbouring cells'
    textures at their shared edge rather than cutting hard. Each distinct texture
    becomes a full-cell layer here; the base layer (the most-used texture) is
    drawn opaque, and every other layer is faded in over it (see
    :func:`_vertex_texture_weights` for the per-vertex coverage).

    The alpha a layer carries is *not* its raw coverage. Under "over" compositing,
    a layer at raw coverage leaves the rest of the pixel showing whatever is
    beneath it -- so the opaque base bleeds through wherever the layers above it do
    not sum to one, muddying every seam that three or more textures share. Instead
    each layer's alpha is its coverage divided by the cumulative coverage of
    itself and every layer below it. That ratio telescopes: composited bottom-up,
    each texture ends up contributing exactly its own coverage weight, and the base
    shows only where it truly belongs. The layer directly above the base becomes
    opaque wherever the base texture is absent, so nothing leaks and no ground goes
    missing.

    Falls back to a single mesh when the cell paints zero or one texture, or when
    no paths are given -- there is nothing to blend, and a lone opaque layer is
    simpler and identical in result.

    Args:
        land: The cell's landscape record (call :func:`has_terrain` first).
        texture_paths: ``LTEX`` index -> texture file, or ``None`` for untextured.
        neighbours: ``(dx, dy) -> 16x16 texture grid`` for the surrounding cells
            (from :func:`cell_texture_grid`), so edge vertices blend across the cell
            seam toward the neighbour's texture. ``None`` clamps at the edge as
            before -- correct for a genuinely isolated cell.

    Returns:
        One :class:`~wraithguard.nif.geometry.Mesh` per land texture, each the
        full cell surface, ordered base-first. The base carries
        :attr:`~wraithguard.nif.geometry.Mesh.blend_layer` ``0`` and full alpha;
        later layers carry ``1, 2, ...`` and their cumulative-normalized alpha.

    Raises:
        HeightEncodeError: If the height grid is malformed.
        struct.error: If the height bytes are the wrong length.
    """
    if texture_paths is None or LandscapeFlags.USES_TEXTURES not in land.landscape_flags:
        return [terrain_mesh(land, texture_paths)]
    grid = _deswizzled_vtex(land.texture_indices)
    if not grid:
        return [terrain_mesh(land, texture_paths)]
    cell_tex = _cell_textures(grid, texture_paths)
    # A one-cell border from the neighbours, so an edge vertex's blend crosses the
    # cell seam toward the neighbour's texture instead of clamping to this cell's.
    extended = _extended_texture_grid(cell_tex, neighbours)

    gx, gy = land.grid
    vertices = _height_vertices(land)
    base_colors = _colors_for(land)
    uvs = _grid_uvs()

    # Per-vertex, the coverage weight of every layer at that vertex.
    per_vertex = [
        _vertex_texture_weights(extended, row, col)
        for row in range(LAND_SIZE)
        for col in range(LAND_SIZE)
    ]

    # Order the layers most-covered first, so the base (drawn opaque underneath) is
    # the texture over the most ground -- the fewest seams to blend over. Totalled
    # from the per-vertex weights (not raw cell counts) so a neighbour's texture
    # that only bleeds in at the shared edge still earns its own thin layer.
    totals: dict[str, float] = {}
    for weights in per_vertex:
        for texture, weight in weights.items():
            if texture:
                totals[texture] = totals.get(texture, 0.0) + weight
    layers = sorted(totals, key=lambda texture: -totals[texture])
    if len(layers) <= 1:
        # Nothing to blend: one opaque textured (or plain) mesh.
        return [terrain_mesh(land, texture_paths)]
    triangles: list[tuple[int, int, int]] = []
    for row in range(LAND_SIZE - 1):
        for col in range(LAND_SIZE - 1):
            triangles.extend(_quad_corners(row, col))

    # The cumulative coverage of the layers drawn so far, per vertex. Each layer's
    # alpha is its own coverage over this running sum, which telescopes under
    # "over" compositing to give every texture exactly its coverage (see docstring).
    cumulative = [0.0] * len(per_vertex)
    meshes: list[Mesh] = []
    for order, texture in enumerate(layers):
        # RGB carries the cell's own vertex colour (or white when it has none); A
        # carries this layer's cumulative-normalized coverage. The base is forced
        # opaque so the ground is never a hole -- everything above fades in over it.
        rgba: list[tuple[float, float, float, float]] = []
        for index, weights in enumerate(per_vertex):
            weight = weights.get(texture, 0.0)
            cumulative[index] += weight
            covered = cumulative[index]
            # The base is opaque; each layer above takes its share of the light
            # still coming through everything drawn beneath it.
            alpha = 1.0
            if order != 0:
                alpha = weight / covered if covered > 0.0 else 0.0
            if base_colors:
                r, g, b, _a = base_colors[index]
            else:
                r = g = b = 1.0
            rgba.append((r, g, b, alpha))
        meshes.append(
            Mesh(
                name=f"terrain_{gx}_{gy}_L{order}",
                vertices=vertices,
                triangles=triangles,
                uvs=uvs,
                texture=texture,
                vertex_colors=rgba,
                blend_layer=order,
            )
        )
    return meshes


#: The four cell edges, and the neighbouring grid offset each one faces. Used to
#: skirt only a block's *outer* edges: an edge whose neighbour is also drawn is
#: internal and left open.
EDGE_NEIGHBOUR: dict[str, tuple[int, int]] = {
    "south": (0, -1),
    "north": (0, 1),
    "east": (1, 0),
    "west": (-1, 0),
}

#: The four edges in a stable order.
ALL_EDGES: tuple[str, ...] = ("south", "east", "north", "west")


def _edge_indices(edge: str) -> list[int]:
    """Row-major grid indices along one cell edge, in a consistent direction."""
    n = LAND_SIZE
    if edge == "south":
        return list(range(n))  # row 0, west to east
    if edge == "east":
        return [r * n + (n - 1) for r in range(n)]  # col 64, south to north
    if edge == "north":
        return [(n - 1) * n + c for c in range(n - 1, -1, -1)]  # row 64, east to west
    return [r * n for r in range(n - 1, -1, -1)]  # col 0, north to south


def terrain_skirt_mesh(
    land: Landscape,
    *,
    edges: tuple[str, ...] = ALL_EDGES,
    drop: float = SKIRT_DROP,
    bottom: float | None = None,
) -> Mesh:
    """A vertical wall down the cell's terrain edges, so it does not look afloat.

    The named edges are dropped straight down to a common bottom a little below
    the lowest point, forming a plinth. Painted with :data:`SKIRT_TEXTURE` (tiled
    at the surface's scale) at full brightness -- no vertex shading, so the wall
    reads as an even band of ground.

    Args:
        land: The cell's landscape record (call :func:`has_terrain` first).
        edges: Which of ``south``/``east``/``north``/``west`` to wall -- all four
            for a lone cell, only the outer ones for a cell inside a drawn block.
        drop: How far below this cell's lowest edge vertex the skirt hangs, when
            ``bottom`` is not given.
        bottom: An explicit world-z for the skirt's base. Pass a value shared
            across a block of cells so their plinths line up on one flat base
            instead of each dropping to its own depth.

    Returns:
        A :class:`~wraithguard.nif.geometry.Mesh` of the requested edge walls, at
        the cell's world origin.

    Raises:
        HeightEncodeError: If the height grid is malformed.
        struct.error: If the height bytes are the wrong length.
    """
    gx, gy = land.grid
    grid = _height_vertices(land)
    if bottom is None:
        bottom = min(vertex[2] for vertex in grid) - drop

    vertices: list[tuple[float, float, float]] = []
    triangles: list[tuple[int, int, int]] = []
    uvs: list[tuple[float, float]] = []
    distance = 0.0
    for edge in edges:
        for start, end in itertools.pairwise(_edge_indices(edge)):
            top_a, top_b = grid[start], grid[end]
            segment = math.hypot(top_b[0] - top_a[0], top_b[1] - top_a[1])
            u0, u1 = distance * _SKIRT_UV, (distance + segment) * _SKIRT_UV
            distance += segment
            base = len(vertices)
            vertices.extend(
                [top_a, top_b, (top_b[0], top_b[1], bottom), (top_a[0], top_a[1], bottom)]
            )
            uvs.extend(
                [
                    (u0, (top_a[2] - bottom) * _SKIRT_UV),
                    (u1, (top_b[2] - bottom) * _SKIRT_UV),
                    (u1, 0.0),
                    (u0, 0.0),
                ]
            )
            triangles.append((base, base + 1, base + 2))
            triangles.append((base, base + 2, base + 3))

    return Mesh(
        name=f"terrain_skirt_{gx}_{gy}",
        vertices=vertices,
        triangles=triangles,
        uvs=uvs,
        # The plinth reads as a consistent band of ground rather than whatever the
        # cell's top texture happens to be. The viewer resolves this like any base
        # texture (DDS-first). No vertex colours -- the wall is drawn unshaded.
        texture=SKIRT_TEXTURE,
    )
