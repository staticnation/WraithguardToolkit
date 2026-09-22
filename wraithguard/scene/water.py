"""A water surface for a cell preview.

Morrowind draws water at a fixed height: sea level (``0``) across the exterior,
and a per-cell height inside an interior that declares one (its ``WHGT``). This
builds a semi-transparent surface at that height so a preview reads "there is
water here, up to here" -- which is what a "did the merge leave the ground poking
through the sea" or "is this pool at the right level" question needs.

The surface is a subdivided grid carrying the
:attr:`~wraithguard.nif.geometry.Mesh.water` flag. The viewer draws it with a
custom water shader: gentle low swells in the vertices (kept small so they never
rise to clip the shore or a rock standing in the water), fine per-pixel ripples in
the fragment shader, a Fresnel sky reflection (sampling Morrowind's own sky
texture) and a sun glint -- the reflective, moving surface reads as water where a
flat blue quad reads as glass. The subdivision is what gives the swell vertices to
ride; the fine ripple detail is per-pixel, so it costs no extra geometry.
"""

from __future__ import annotations

from wraithguard.nif.geometry import Mesh

#: Exterior water height in world units -- Morrowind's sea sits at z = 0.
SEA_LEVEL = 0.0

#: The water's look: a dim blue, half-transparent so the ground reads through it.
#: A fallback tint only -- the viewer's water shader computes its own colour from
#: reflection and depth; this shows only if that shader fails to compile.
_WATER_RGBA = (0.16, 0.34, 0.52, 1.0)
_WATER_OPACITY = 0.5

#: Grid cells per side of the water surface. Enough to carry the gentle swell
#: smoothly; the fine ripples are per-pixel in the shader, not in the mesh, so
#: this stays modest -- one plane, ~64x64 quads, drawn once.
_WATER_SUBDIVISIONS = 64

#: How far the water skirt hangs below the surface, in world units -- a shallow
#: translucent wall at the cell edge so the sea does not read as a floating sheet.
_SKIRT_DROP = 1024.0

#: The skirt bottom is dimmer than the surface, so it reads as depth.
_SKIRT_BOTTOM_SHADE = 0.6


def water_mesh(
    height: float, x0: float, y0: float, x1: float, y1: float, *, name: str = "water"
) -> Mesh:
    """A subdivided water surface at ``height`` spanning the given XY rectangle.

    Args:
        height: The water surface height (world z).
        x0: Minimum x.
        y0: Minimum y.
        x1: Maximum x.
        y1: Maximum y.
        name: The mesh name (shown in the viewer's shape list).

    Returns:
        A grid :class:`~wraithguard.nif.geometry.Mesh` at ``height``, flagged as
        water, wound counter-clockwise seen from above. The blue vertex colour and
        blending are a fallback used only if the water shader fails to compile.
    """
    n = _WATER_SUBDIVISIONS
    span_x = x1 - x0
    span_y = y1 - y0
    vertices: list[tuple[float, float, float]] = []
    for row in range(n + 1):
        fy = row / n
        for col in range(n + 1):
            fx = col / n
            vertices.append((x0 + fx * span_x, y0 + fy * span_y, height))
    triangles: list[tuple[int, int, int]] = []
    stride = n + 1
    for row in range(n):
        for col in range(n):
            v00 = row * stride + col
            v10 = v00 + 1
            v01 = v00 + stride
            v11 = v01 + 1
            triangles.append((v00, v10, v11))
            triangles.append((v00, v11, v01))
    return Mesh(
        name=name,
        vertices=vertices,
        triangles=triangles,
        vertex_colors=[_WATER_RGBA] * len(vertices),
        alpha_blend=True,
        opacity=_WATER_OPACITY,
        water=True,
    )


#: Each water edge and the two rectangle corners it spans (as ``(x, y)`` picks
#: from ``x0/y0/x1/y1``), plus the neighbouring grid offset it faces -- so a
#: block's inner edges can be left open, matching the terrain skirt.
_WATER_EDGES: dict[str, tuple[tuple[int, int], tuple[int, int], tuple[int, int]]] = {
    # edge: (corner a, corner b, neighbour offset); corner is (x_index, y_index)
    "south": ((0, 0), (1, 0), (0, -1)),
    "east": ((1, 0), (1, 1), (1, 0)),
    "north": ((1, 1), (0, 1), (0, 1)),
    "west": ((0, 1), (0, 0), (-1, 0)),
}

#: The four water edges in a stable order.
ALL_EDGES: tuple[str, ...] = ("south", "east", "north", "west")


def water_skirt_mesh(
    height: float,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    *,
    edges: tuple[str, ...] = ALL_EDGES,
    drop: float = _SKIRT_DROP,
    name: str = "water_skirt",
) -> Mesh:
    """A translucent wall down the water's edges, so it does not look afloat.

    The named edges drop from the surface rectangle to ``drop`` below it, tinted
    the water colour and dimmed toward the bottom. Drawn as a plain translucent
    surface (not the animated water plane): a vertical face has no swell to ride,
    and a flat tint reads correctly as the side of a body of water.

    Args:
        height: The water surface height (world z).
        x0: Minimum x.
        y0: Minimum y.
        x1: Maximum x.
        y1: Maximum y.
        edges: Which of ``south``/``east``/``north``/``west`` to wall.
        drop: How far below the surface the skirt hangs.
        name: The mesh name (shown in the viewer's shape list).

    Returns:
        A :class:`~wraithguard.nif.geometry.Mesh` of the requested edge walls.
    """
    bottom = height - drop
    xs = (x0, x1)
    ys = (y0, y1)
    vertices: list[tuple[float, float, float]] = []
    triangles: list[tuple[int, int, int]] = []
    for edge in edges:
        (ax_i, ay_i), (bx_i, by_i), _neighbour = _WATER_EDGES[edge]
        ax, ay, bx, by = xs[ax_i], ys[ay_i], xs[bx_i], ys[by_i]
        base = len(vertices)
        vertices.extend([(ax, ay, height), (bx, by, height), (bx, by, bottom), (ax, ay, bottom)])
        triangles.append((base, base + 1, base + 2))
        triangles.append((base, base + 2, base + 3))
    s = _SKIRT_BOTTOM_SHADE
    dim = (_WATER_RGBA[0] * s, _WATER_RGBA[1] * s, _WATER_RGBA[2] * s, _WATER_RGBA[3])
    vertex_colors = [_WATER_RGBA, _WATER_RGBA, dim, dim] * len(edges)
    return Mesh(
        name=name,
        vertices=vertices,
        triangles=triangles,
        vertex_colors=vertex_colors,
        alpha_blend=True,
        opacity=_WATER_OPACITY,
    )
