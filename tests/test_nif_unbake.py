"""The un-baking migration's safety net (UNBAKE_MIGRATION.md, Stage 1).

``model_shapes`` keeps each shape's vertices in its own local space and carries
the composed node transform in ``node_world``; ``world_meshes`` bakes that in.
These pin the one invariant the whole migration rests on: **baking a model-space
shape lands its vertices exactly where the old world-space path put them**, so
every current caller (bounds, terrain, water, the conflict diff, the viewer) is
byte-identical until it is deliberately moved.
"""

from __future__ import annotations

import json
import re

from wraithguard.nif.geometry import Mesh, Transform, bake_mesh, model_shapes, world_meshes
from wraithguard.nif.reader import Block, NifFile
from wraithguard.nif.viewer import build_viewer_page

_VERSION = 0x04000002


def _node(
    children: list[int],
    *,
    translation: tuple[float, float, float] = (0.0, 0.0, 0.0),
    scale: float = 1.0,
) -> dict:
    """NiNode fields with an identity rotation and the given placement."""
    return {
        "children_links": children,
        "controller": -1,
        "rotation_m3": [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        "translation_xyz": translation,
        "scale": scale,
    }


def _shape_under_transformed_node() -> NifFile:
    """A shape under a root node that scales x2 and shifts +10 on X."""
    blocks = [
        Block(0, "NiNode", _node(children=[1], translation=(10.0, 0.0, 0.0), scale=2.0)),
        Block(1, "NiTriShape", {"data": 2, "properties_links": [], "controller": -1}),
        Block(
            2,
            "NiTriShapeData",
            {
                "vertices_xyz": [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)],
                "triangles_indices": [(0, 1, 2)],
            },
        ),
    ]
    return NifFile(_VERSION, len(blocks), blocks)


def test_model_shapes_keep_local_vertices_and_carry_node_world() -> None:
    """The shape's vertices stay local; its node transform rides in node_world."""
    shapes = model_shapes(_shape_under_transformed_node())
    assert len(shapes) == 1
    shape = shapes[0]
    # Root rotation is dropped but its translation and scale are kept.
    assert shape.node_world.scale == 2.0
    assert shape.node_world.translation == (10.0, 0.0, 0.0)
    # Vertices are untransformed -- the shape's own coordinates.
    assert shape.vertices == [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)]


def test_world_meshes_bakes_node_world_into_vertices() -> None:
    """The baked path lands vertices at node_world.apply(local), node_world reset."""
    baked = world_meshes(_shape_under_transformed_node())
    assert len(baked) == 1
    mesh = baked[0]
    assert mesh.node_world == Transform()  # folded in, so identity
    # v*2 + (10,0,0): the +10 lands on every vertex's X.
    assert mesh.vertices == [(12.0, 0.0, 0.0), (10.0, 2.0, 0.0), (10.0, 0.0, 2.0)]


def test_baking_a_model_shape_equals_the_world_mesh() -> None:
    """The invariant: bake(model_shape) is exactly world_meshes, position for position."""
    parsed = _shape_under_transformed_node()
    from_model = [bake_mesh(shape) for shape in model_shapes(parsed)]
    direct = world_meshes(parsed)
    assert [m.vertices for m in from_model] == [m.vertices for m in direct]
    assert all(m.node_world == Transform() for m in from_model)


def test_bake_mesh_is_a_noop_on_an_identity_shape() -> None:
    """A shape already in world space (identity node_world) is returned unchanged."""
    blocks = [
        Block(0, "NiNode", _node(children=[1])),
        Block(1, "NiTriShape", {"data": 2, "properties_links": [], "controller": -1}),
        Block(
            2,
            "NiTriShapeData",
            {"vertices_xyz": [(3.0, 4.0, 5.0)], "triangles_indices": [(0, 0, 0)]},
        ),
    ]
    shape = model_shapes(NifFile(_VERSION, len(blocks), blocks))[0]
    assert shape.node_world == Transform()
    assert bake_mesh(shape) is shape  # untouched, same object


def test_viewer_payload_carries_model_space_node_world() -> None:
    """A model-space mesh ships its node transform as a 4x4 for the page to fold."""
    mesh = Mesh(
        name="crate",
        vertices=[(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)],
        triangles=[(0, 1, 2)],
        node_world=Transform(scale=2.0, translation=(10.0, 0.0, 0.0)),
    )
    page = build_viewer_page([("crate", [mesh])])
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    # scale 2 on the diagonal, +10 on X in the translation column (column-major).
    assert scenes[0]["meshes"][0]["nodeWorld"] == [
        2.0, 0, 0, 0,
        0, 2.0, 0, 0,
        0, 0, 2.0, 0,
        10.0, 0, 0, 1,
    ]  # fmt: skip


def test_baked_mesh_payload_carries_identity_node_world() -> None:
    """A baked mesh's node_world is identity, so the fold is a no-op on the page."""
    mesh = Mesh(name="baked", vertices=[(1.0, 2.0, 3.0)], triangles=[(0, 0, 0)])
    page = build_viewer_page([("baked", [mesh])])
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    assert scenes[0]["meshes"][0]["nodeWorld"] == [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
