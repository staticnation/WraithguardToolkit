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

    Args:
        vertex: The vertex's row or column index (0..64).

    Returns:
        ``(cell_low, cell_high, frac)``: the two ``VTEX`` cell indices (clamped to
        0..15) this vertex falls between, and the 0..1 blend toward the high one.
    """
    coord = (vertex - 2) / 4.0
    low = math.floor(coord)
    frac = coord - low
    cell_low = min(max(low, 0), TEXTURE_SIZE - 1)
    cell_high = min(max(low + 1, 0), TEXTURE_SIZE - 1)
    return cell_low, cell_high, frac


def _vertex_texture_weights(cell_tex: list[list[str]], row: int, col: int) -> dict[str, float]:
    """The blend weight of each land texture at one vertex, summing to 1.

    Bilinearly samples the four ``VTEX`` cells around the vertex (see
    :func:`_corner_weights`) and accumulates each corner's weight onto the
    texture that corner's cell paints. Deep inside a region every corner shares
    the texture, so it gets weight ~1; at a boundary the weight splits, which is
    the soft edge. Unpainted corners collect under ``""``.

    Args:
        cell_tex: The 16x16 resolved texture grid from :func:`_cell_textures`.
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
        texture = cell_tex[cy][cx]
        weights[texture] = weights.get(texture, 0.0) + weight
    return weights


def terrain_blend_meshes(
    land: Landscape, texture_paths: Mapping[int, str] | None = None
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

    # Order the layers most-used first, so the base (drawn opaque underneath) is
    # the texture that covers the most ground -- the fewest seams to blend over.
    counts: Counter[str] = Counter()
    for line in cell_tex:
        for texture in line:
            if texture:
                counts[texture] += 1
    layers = [texture for texture, _n in counts.most_common()]
    if len(layers) <= 1:
        # Nothing to blend: one opaque textured (or plain) mesh.
        return [terrain_mesh(land, texture_paths)]

    gx, gy = land.grid
    vertices = _height_vertices(land)
    base_colors = _colors_for(land)
    uvs = _grid_uvs()

    # Per-vertex, the coverage weight of every layer at that vertex.
    per_vertex = [
        _vertex_texture_weights(cell_tex, row, col)
        for row in range(LAND_SIZE)
        for col in range(LAND_SIZE)
    ]
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
