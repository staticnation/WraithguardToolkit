"""Retaining animation key *values* from a NIF (the ``animation=True`` path).

The structure reader normally records only how many keys each animation group
has. These tests cover the opt-in path that also decodes the keys themselves:
the pure span decoders (built from crafted bytes so each interpolation branch is
hit deliberately), and a whole synthetic file parsed with ``animation=True`` so
the companion fields land on the right blocks. Without the flag, nothing changes
-- that is asserted too, because the scan over a mod folder must not start
paying for a viewer feature.
"""

from __future__ import annotations

import json
import re
import struct

from wraithguard.nif import read_nif_bytes
from wraithguard.nif.geometry import (
    Mesh,
    Transform,
    TransformAnimation,
    UVAnimation,
    model_shapes,
    world_meshes,
)
from wraithguard.nif.reader import (
    Block,
    NifFile,
    _decode_animation,
    _decode_key_group,
    _decode_keyframe,
)
from wraithguard.nif.viewer import _transform_anim_payload, _uv_anim_payload, build_viewer_page

HEADER = b"NetImmerse File Format, Version 4.0.0.2\n"
_VERSION = 0x04000002


def _text(value: str) -> bytes:
    """A length-prefixed, unterminated string, as a NIF stores one."""
    raw = value.encode("cp1252")
    return struct.pack("<I", len(raw)) + raw


def _nif(*blocks: tuple[str, bytes]) -> bytes:
    """Assemble a whole file from ``(type name, body)`` pairs."""
    out = [HEADER, struct.pack("<I", _VERSION), struct.pack("<I", len(blocks))]
    for type_name, body in blocks:
        out.append(_text(type_name))
        out.append(body)
    return b"".join(out)


def _float_group(keys: list[tuple[float, float]], mode: int = 1) -> bytes:
    """A ``count, mode, keys`` group of scalar (time, value) keys."""
    if not keys:
        return struct.pack("<I", 0)
    body = struct.pack("<I", len(keys)) + struct.pack("<I", mode)
    return body + b"".join(struct.pack("<ff", t, v) for t, v in keys)


def _vector_group(keys: list[tuple[float, tuple[float, float, float]]]) -> bytes:
    """A linear ``count, mode, keys`` group of (time, xyz) keys."""
    body = struct.pack("<I", len(keys)) + struct.pack("<I", 1)
    return body + b"".join(struct.pack("<ffff", t, *xyz) for t, xyz in keys)


def _quat_group(keys: list[tuple[float, tuple[float, float, float, float]]]) -> bytes:
    """A linear ``count, mode, keys`` group of (time, wxyz) quaternion keys."""
    body = struct.pack("<I", len(keys)) + struct.pack("<I", 1)
    return body + b"".join(struct.pack("<fffff", t, *wxyz) for t, wxyz in keys)


def _vis_array(keys: list[tuple[float, bool]]) -> bytes:
    """A ``count`` then that many (time f32, visible byte) keys."""
    body = struct.pack("<I", len(keys))
    return body + b"".join(struct.pack("<f", t) + struct.pack("<B", int(v)) for t, v in keys)


def _morph_target(
    weights: list[tuple[float, float]], verts: list[tuple[float, float, float]]
) -> bytes:
    """One morph target: a weight key group (mode always written) then its vertices."""
    body = struct.pack("<I", len(weights)) + struct.pack("<I", 1)  # count, mode (always)
    body += b"".join(struct.pack("<ff", t, w) for t, w in weights)
    body += b"".join(struct.pack("<fff", *v) for v in verts)
    return body


def _morph_data(targets: list[bytes], num_vertices: int) -> bytes:
    """A NiMorphData body: counts, relative flag, then the target table."""
    body = struct.pack("<I", len(targets)) + struct.pack("<I", num_vertices) + struct.pack("<B", 1)
    return body + b"".join(targets)


# --------------------------------------------------------------------------- #
# pure decoders
# --------------------------------------------------------------------------- #


def test_decode_scalar_key_group() -> None:
    """A float group decodes to (time, value) pairs and reports the end offset."""
    data = _float_group([(0.0, 1.0), (0.5, 2.0), (1.0, 3.0)])
    keys, end = _decode_key_group(data, 0, {1: 8}, 1)
    assert keys == [(0.0, 1.0), (0.5, 2.0), (1.0, 3.0)]
    assert end == len(data)


def test_decode_empty_group_has_no_mode_word() -> None:
    """An empty group is just its zero count -- the mode word is omitted."""
    data = struct.pack("<I", 0)
    keys, end = _decode_key_group(data, 0, {1: 8}, 1)
    assert keys == []
    assert end == 4


def test_decode_bezier_key_reads_value_before_tangents() -> None:
    """A Bézier float key (width 16) still yields the value right after the time."""
    # time, value, in-tan, out-tan -- only (time, value) is retained.
    data = struct.pack("<I", 1) + struct.pack("<I", 2) + struct.pack("<ffff", 2.0, 9.0, 0.1, 0.2)
    keys, _end = _decode_key_group(data, 0, {2: 16}, 1)
    assert keys == [(2.0, 9.0)]


def test_decode_keyframe_quaternion_translation_scale() -> None:
    """A keyframe body decodes rotation (w,x,y,z), translation and scale keys."""
    body = (
        _quat_group([(0.0, (1.0, 0.0, 0.0, 0.0)), (1.0, (0.0, 0.0, 0.0, 1.0))])
        + _vector_group([(0.0, (10.0, 20.0, 30.0))])
        + _float_group([(0.0, 1.0), (1.0, 2.0)])
    )
    decoded = _decode_keyframe(body, 0)
    assert decoded["rotation"] == [
        (0.0, (1.0, 0.0, 0.0, 0.0)),
        (1.0, (0.0, 0.0, 0.0, 1.0)),
    ]
    assert decoded["translation"] == [(0.0, (10.0, 20.0, 30.0))]
    assert decoded["scale"] == [(0.0, 1.0), (1.0, 2.0)]


def test_decode_keyframe_euler_rotation() -> None:
    """Euler rotation (mode 4) yields the axis order and three per-axis groups."""
    body = (
        struct.pack("<I", 1)  # rotation key count (non-zero triggers the mode read)
        + struct.pack("<I", 4)  # EULER_KEY
        + struct.pack("<i", 0)  # axis order XYZ
        + _float_group([(0.0, 0.25)])  # x
        + _float_group([(0.0, 0.5)])  # y
        + _float_group([(0.0, 0.75)])  # z
        + struct.pack("<I", 0)  # translation: empty
        + struct.pack("<I", 0)  # scale: empty
    )
    decoded = _decode_keyframe(body, 0)
    assert decoded["rotation"]["euler_axis_order"] == 0
    assert decoded["rotation"]["euler"] == [[(0.0, 0.25)], [(0.0, 0.5)], [(0.0, 0.75)]]


def test_decode_animation_vis_keys() -> None:
    """Visibility keys decode to (time, bool) with the byte read as truthiness."""
    data = _vis_array([(0.0, True), (0.5, False), (1.0, True)])
    keys = _decode_animation("vis_key_array", data, 0, len(data), {})
    assert keys == [(0.0, True), (0.5, False), (1.0, True)]


# --------------------------------------------------------------------------- #
# whole-file integration
# --------------------------------------------------------------------------- #


def test_uv_data_values_retained_only_with_animation_flag() -> None:
    """NiUVData keeps its offset/tiling keys under companions when asked, not otherwise."""
    body = (
        _float_group([(0.0, 0.0), (1.0, 1.0)])  # u offset
        + _float_group([(0.0, 0.0)])  # v offset
        + struct.pack("<I", 0)  # u tiling: empty
        + struct.pack("<I", 0)  # v tiling: empty
    )
    data = _nif(("NiUVData", body))

    plain = read_nif_bytes(data)
    assert plain.blocks[0].fields["u_keys"] == 2  # count, as before
    assert "u_keys_values" not in plain.blocks[0].fields

    animated = read_nif_bytes(data, animation=True)
    fields = animated.blocks[0].fields
    assert fields["u_keys"] == 2  # count still there
    assert fields["u_keys_values"] == [(0.0, 0.0), (1.0, 1.0)]
    assert fields["v_keys_values"] == [(0.0, 0.0)]
    assert fields["u_scale_keys_values"] == []


def test_keyframe_data_values_retained_in_file() -> None:
    """A NiKeyframeData block parsed with animation=True carries decoded keys."""
    body = (
        _quat_group([(0.0, (1.0, 0.0, 0.0, 0.0))])
        + _vector_group([(0.0, (1.0, 2.0, 3.0)), (2.0, (4.0, 5.0, 6.0))])
        + _float_group([(0.0, 1.0)])
    )
    animated = read_nif_bytes(_nif(("NiKeyframeData", body)), animation=True)
    keys = animated.blocks[0].fields["keyframe_data_values"]
    assert keys["rotation"] == [(0.0, (1.0, 0.0, 0.0, 0.0))]
    assert keys["translation"] == [(0.0, (1.0, 2.0, 3.0)), (2.0, (4.0, 5.0, 6.0))]
    assert keys["scale"] == [(0.0, 1.0)]


def test_vis_data_values_retained_in_file() -> None:
    """A NiVisData block keeps its visibility keys under the companion field."""
    animated = read_nif_bytes(
        _nif(("NiVisData", _vis_array([(0.0, True), (1.0, False)]))), animation=True
    )
    assert animated.blocks[0].fields["vis_keys_values"] == [(0.0, True), (1.0, False)]


# --------------------------------------------------------------------------- #
# geometry extraction (world_meshes attaches the UV animation to the shape)
# --------------------------------------------------------------------------- #


def _identity_node(children: list[int], controller: int = -1) -> dict:
    """Fields for a NiNode with the given children and an optional controller."""
    return {
        "children_links": children,
        "controller": controller,
        "rotation_m3": [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        "translation_xyz": (0.0, 0.0, 0.0),
        "scale": 1.0,
    }


def test_world_meshes_attaches_uv_animation_from_node_controller() -> None:
    """A NiUVController on a parent node reaches the child shape's mesh."""
    blocks = [
        Block(0, "NiNode", _identity_node(children=[1], controller=2)),
        Block(1, "NiTriShape", {"data": 3, "properties_links": [], "controller": -1}),
        Block(2, "NiUVController", {"data": 4, "next_controller": -1}),
        Block(
            3,
            "NiTriShapeData",
            {
                "vertices_xyz": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
                "triangles_indices": [(0, 1, 2)],
            },
        ),
        Block(
            4,
            "NiUVData",
            {
                "u_keys_values": [(0.0, 0.0), (1.0, 0.5)],
                "v_keys_values": [],
                "u_scale_keys_values": [],
                "v_scale_keys_values": [],
            },
        ),
    ]
    meshes = world_meshes(NifFile(0x04000002, len(blocks), blocks))

    assert len(meshes) == 1
    assert meshes[0].uv_anim == UVAnimation(u_offset=((0.0, 0.0), (1.0, 0.5)))


def test_world_meshes_leaves_uv_animation_none_without_controller() -> None:
    """A shape with no UV controller carries no animation (the default path)."""
    blocks = [
        Block(0, "NiNode", _identity_node(children=[1])),
        Block(1, "NiTriShape", {"data": 2, "properties_links": [], "controller": -1}),
        Block(
            2,
            "NiTriShapeData",
            {
                "vertices_xyz": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
                "triangles_indices": [(0, 1, 2)],
            },
        ),
    ]
    meshes = world_meshes(NifFile(0x04000002, len(blocks), blocks))

    assert len(meshes) == 1
    assert meshes[0].uv_anim is None


# --------------------------------------------------------------------------- #
# viewer serialisation + playback wiring
# --------------------------------------------------------------------------- #


def test_uv_anim_payload_includes_only_nonempty_channels() -> None:
    """The payload carries [[t, v], ...] for populated channels and drops empty ones."""
    anim = UVAnimation(u_offset=((0.0, 0.0), (2.0, 1.0)), v_tiling=((0.0, 3.0),))
    assert _uv_anim_payload(anim) == {
        "uOffset": [[0.0, 0.0], [2.0, 1.0]],
        "vTiling": [[0.0, 3.0]],
    }
    assert _uv_anim_payload(None) is None


def test_viewer_page_embeds_uv_animation_and_loop() -> None:
    """A mesh with a UV animation ships its keys and the page's playback code."""
    mesh = Mesh(
        name="banner",
        vertices=[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
        triangles=[(0, 1, 2)],
        uv_anim=UVAnimation(u_offset=((0.0, 0.0), (1.0, 1.0))),
    )
    page = build_viewer_page([("banner", [mesh])])

    # The playback code and the loop trigger are present.
    assert "advanceUvAnimations" in page
    assert "uvAnimated.length" in page
    # The mesh's keys reached the scene payload.
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    assert scenes[0]["meshes"][0]["uvAnim"] == {"uOffset": [[0.0, 0.0], [1.0, 1.0]]}


# --------------------------------------------------------------------------- #
# transform (node keyframe) animation
# --------------------------------------------------------------------------- #


def test_world_meshes_attaches_transform_animation_from_node_controller() -> None:
    """A NiKeyframeController on a node reaches its child shape with above/rest/below."""
    blocks = [
        Block(0, "NiNode", _identity_node(children=[1])),
        Block(1, "NiNode", _identity_node(children=[2], controller=3)),
        Block(2, "NiTriShape", {"data": 4, "properties_links": [], "controller": -1}),
        Block(3, "NiKeyframeController", {"data": 5, "next_controller": -1}),
        Block(
            4,
            "NiTriShapeData",
            {
                "vertices_xyz": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
                "triangles_indices": [(0, 1, 2)],
            },
        ),
        Block(
            5,
            "NiKeyframeData",
            {
                "keyframe_data_values": {
                    "rotation": [(0.0, (1.0, 0.0, 0.0, 0.0))],
                    "translation": [(0.0, (0.0, 0.0, 0.0)), (1.0, (0.0, 0.0, 90.0))],
                    "scale": [],
                }
            },
        ),
    ]
    meshes = world_meshes(NifFile(0x04000002, len(blocks), blocks))

    assert len(meshes) == 1
    anim = meshes[0].transform_anim
    assert anim is not None
    assert anim.above == Transform()  # identity above the animated node
    assert anim.rest == Transform()  # the node's own transform is identity here
    assert anim.below == Transform()  # the shape itself has no extra transform
    assert anim.rotation == ((0.0, (1.0, 0.0, 0.0, 0.0)),)
    assert anim.translation == ((0.0, (0.0, 0.0, 0.0)), (1.0, (0.0, 0.0, 90.0)))
    assert anim.scale == ()


def test_transform_anim_carries_descendant_chain_in_below() -> None:
    """An animated ancestor is split from its child chain without an inverse."""

    def node(children, *, translation=(0.0, 0.0, 0.0), controller=-1):
        return {
            "children_links": children,
            "controller": controller,
            "rotation_m3": [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            "translation_xyz": translation,
            "scale": 1.0,
        }

    blocks = [
        Block(0, "NiNode", node([1], translation=(10.0, 0.0, 0.0))),
        Block(1, "NiNode", node([2], translation=(2.0, 0.0, 0.0), controller=3)),
        Block(
            2,
            "NiTriShape",
            {
                **node([], translation=(0.0, 5.0, 0.0)),
                "data": 5,
                "properties_links": [],
                "controller": -1,
            },
        ),
        Block(3, "NiKeyframeController", {"data": 4, "next_controller": -1}),
        Block(
            4,
            "NiKeyframeData",
            {
                "keyframe_data_values": {
                    "rotation": [],
                    "translation": [(0.0, (2.0, 0.0, 0.0)), (1.0, (4.0, 0.0, 0.0))],
                    "scale": [],
                }
            },
        ),
        Block(
            5,
            "NiTriShapeData",
            {
                "vertices_xyz": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
                "triangles_indices": [(0, 1, 2)],
            },
        ),
    ]
    meshes = model_shapes(NifFile(_VERSION, len(blocks), blocks))
    assert len(meshes) == 1
    anim = meshes[0].transform_anim
    assert anim is not None
    # Root is above the animated node; the child's local transform is below it.
    assert anim.above.translation == (10.0, 0.0, 0.0)
    assert anim.rest.translation == (2.0, 0.0, 0.0)
    assert anim.below.translation == (0.0, 5.0, 0.0)


def test_transform_anim_payload_serialises_matrices_and_keys() -> None:
    """The payload carries the direct above/rest/below split and key channels."""
    anim = TransformAnimation(
        above=Transform(),
        rest=Transform(),
        below=Transform(translation=(3.0, 4.0, 5.0)),
        translation=((0.0, (0.0, 0.0, 0.0)), (1.0, (0.0, 0.0, 90.0))),
    )
    out = _transform_anim_payload(anim)
    assert out is not None
    identity = [1.0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
    assert out["above"] == identity
    assert out["rest"] == identity
    assert out["below"] == [1.0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 3.0, 4.0, 5.0, 1]
    assert out["translation"] == [[0.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 90.0]]
    assert out["rotation"] == []
    assert out["scale"] == []
    assert _transform_anim_payload(None) is None


def test_viewer_page_embeds_transform_animation_and_loop() -> None:
    """A mesh with a node animation ships its keys and the page's playback code."""
    mesh = Mesh(
        name="banner",
        vertices=[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
        triangles=[(0, 1, 2)],
        transform_anim=TransformAnimation(
            above=Transform(),
            rest=Transform(),
            below=Transform(),
            rotation=((0.0, (1.0, 0.0, 0.0, 0.0)), (1.0, (0.0, 0.0, 0.0, 1.0))),
        ),
    )
    page = build_viewer_page([("banner", [mesh])])

    assert "advanceXformAnimations" in page
    assert "registerXformAnim" in page
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    assert scenes[0]["meshes"][0]["transformAnim"]["rotation"] == [
        [0.0, 1.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0],
    ]


# --------------------------------------------------------------------------- #
# visibility (blink) animation
# --------------------------------------------------------------------------- #


def test_world_meshes_attaches_visibility_animation() -> None:
    """A NiVisController on a node blinks its child shape on and off."""
    blocks = [
        Block(0, "NiNode", _identity_node(children=[1], controller=2)),
        Block(1, "NiTriShape", {"data": 3, "properties_links": [], "controller": -1}),
        Block(2, "NiVisController", {"data": 4, "next_controller": -1}),
        Block(
            3,
            "NiTriShapeData",
            {
                "vertices_xyz": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
                "triangles_indices": [(0, 1, 2)],
            },
        ),
        Block(4, "NiVisData", {"vis_keys_values": [(0.0, True), (0.5, False), (1.0, True)]}),
    ]
    meshes = world_meshes(NifFile(0x04000002, len(blocks), blocks))

    assert len(meshes) == 1
    assert meshes[0].vis_anim == ((0.0, True), (0.5, False), (1.0, True))


def test_viewer_page_embeds_visibility_animation_and_loop() -> None:
    """A mesh with a visibility animation ships its keys and the playback code."""
    mesh = Mesh(
        name="blinker",
        vertices=[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
        triangles=[(0, 1, 2)],
        vis_anim=((0.0, True), (0.5, False)),
    )
    page = build_viewer_page([("blinker", [mesh])])

    assert "advanceVisAnimations" in page
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    assert scenes[0]["meshes"][0]["visAnim"] == [[0.0, 1], [0.5, 0]]


# --------------------------------------------------------------------------- #
# morph (cloth ripple) animation
# --------------------------------------------------------------------------- #


def test_morph_data_targets_retained_in_file() -> None:
    """NiMorphData keeps each target's weight keys and vertices under a companion."""
    base = _morph_target([], [(0.0, 0.0, 0.0)])  # target 0: base, no weight keys
    delta = _morph_target([(0.0, 0.0), (1.0, 1.0)], [(0.5, 0.0, 0.0)])  # target 1
    data = _nif(("NiMorphData", _morph_data([base, delta], num_vertices=1)))

    animated = read_nif_bytes(data, animation=True)
    morphs = animated.blocks[0].fields["morphs_values"]
    assert len(morphs) == 2
    assert morphs[0] == {"weights": [], "vertices": [(0.0, 0.0, 0.0)]}
    assert morphs[1] == {"weights": [(0.0, 0.0), (1.0, 1.0)], "vertices": [(0.5, 0.0, 0.0)]}


def _tri_data_verts() -> dict:
    """NiTriShapeData fields with three vertices and one triangle."""
    return {
        "vertices_xyz": [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
        "triangles_indices": [(0, 1, 2)],
    }


def test_world_meshes_attaches_morph_animation() -> None:
    """A NiGeomMorpherController on a shape yields its delta targets (base dropped)."""
    blocks = [
        Block(0, "NiNode", _identity_node(children=[1])),
        Block(1, "NiTriShape", {"data": 2, "properties_links": [], "controller": 3}),
        Block(2, "NiTriShapeData", _tri_data_verts()),
        Block(3, "NiGeomMorpherController", {"data": 4, "next_controller": -1}),
        Block(
            4,
            "NiMorphData",
            {
                "morphs_values": [
                    {"weights": [], "vertices": [(0.0, 0.0, 0.0)] * 3},  # base
                    {
                        "weights": [(0.0, 0.0), (1.0, 1.0)],
                        "vertices": [(0.0, 0.0, 5.0), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)],
                    },
                ]
            },
        ),
    ]
    meshes = world_meshes(NifFile(0x04000002, len(blocks), blocks))

    assert len(meshes) == 1
    morph = meshes[0].morph_anim
    assert morph is not None
    assert len(morph.targets) == 1  # target 0 (base) is dropped
    assert morph.targets[0].weights == ((0.0, 0.0), (1.0, 1.0))
    assert morph.targets[0].deltas == ((0.0, 0.0, 5.0), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0))


def test_morph_skipped_when_vertex_count_disagrees() -> None:
    """A target whose vertex count differs from the shape is not blended."""
    blocks = [
        Block(0, "NiNode", _identity_node(children=[1])),
        Block(1, "NiTriShape", {"data": 2, "properties_links": [], "controller": 3}),
        Block(2, "NiTriShapeData", _tri_data_verts()),  # 3 vertices
        Block(3, "NiGeomMorpherController", {"data": 4, "next_controller": -1}),
        Block(
            4,
            "NiMorphData",
            {
                "morphs_values": [
                    {"weights": [], "vertices": [(0.0, 0.0, 0.0)]},  # 1 vertex, not 3
                    {"weights": [(0.0, 1.0)], "vertices": [(1.0, 0.0, 0.0)]},
                ]
            },
        ),
    ]
    assert world_meshes(NifFile(0x04000002, len(blocks), blocks))[0].morph_anim is None


def test_viewer_page_embeds_morph_animation_and_loop() -> None:
    """A mesh with a morph animation ships flattened deltas and the playback code."""
    from wraithguard.nif.geometry import MorphAnimation, MorphTarget

    mesh = Mesh(
        name="banner",
        vertices=[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
        triangles=[(0, 1, 2)],
        morph_anim=MorphAnimation(
            targets=(
                MorphTarget(
                    weights=((0.0, 0.0), (1.0, 1.0)),
                    deltas=((0.0, 0.0, 5.0), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
                ),
            )
        ),
    )
    page = build_viewer_page([("banner", [mesh])])

    assert "advanceMorphAnimations" in page
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    target = scenes[0]["meshes"][0]["morphAnim"]["targets"][0]
    assert target["weights"] == [[0.0, 0.0], [1.0, 1.0]]
    assert target["deltas"] == [0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]


# --------------------------------------------------------------------------- #
# particle clouds (mist / glowbugs)
# --------------------------------------------------------------------------- #


def test_world_meshes_makes_a_point_cloud_from_particles() -> None:
    """A NiParticles block becomes a points mesh (positions, no triangles)."""
    blocks = [
        Block(0, "NiBSParticleNode", _identity_node(children=[1])),
        Block(1, "NiParticles", {"data": 2, "properties_links": [], "controller": -1}),
        Block(
            2,
            "NiParticlesData",
            {"vertices_xyz": [(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)]},
        ),
    ]
    meshes = world_meshes(NifFile(0x04000002, len(blocks), blocks))

    clouds = [m for m in meshes if m.points]
    assert len(clouds) == 1
    assert clouds[0].triangles == []
    assert clouds[0].emitter is True
    assert clouds[0].vertices == [(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)]


def test_viewer_page_draws_points_and_drifts_them() -> None:
    """A points mesh ships points=True and the page's particle-drift code."""
    mesh = Mesh(name="mist", vertices=[(0.0, 0.0, 0.0), (1.0, 1.0, 1.0)], triangles=[], points=True)
    page = build_viewer_page([("mist", [mesh])])

    assert "advancePointsAnimations" in page
    assert "THREE.Points" in page
    match = re.search(r"var scenes = (\[.*?\]);", page, re.S)
    assert match
    scenes = json.loads(match.group(1).replace("<\\/", "</"))
    assert scenes[0]["meshes"][0]["points"] is True
