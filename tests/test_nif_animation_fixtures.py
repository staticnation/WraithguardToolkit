"""Animation extraction against real hand-crafted NIF bytes.

The other animation tests build ``Block`` objects by hand, which share the
reader's own idea of the format; these parse actual little NIF files (from
Gardenfell / GrassForge, MIT -- see ``fixtures/gardenfell_anim/SOURCE.md``) so a
layout bug that a hand-built block would hide shows up. Two real bugs were caught
this way: geometry lost under a ``NiCollisionSwitch``, and a particle
``NiTriShape`` drawn as a surface instead of a point cloud.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from wraithguard.nif.geometry import model_shapes
from wraithguard.nif.kf import load_kf
from wraithguard.nif.reader import read_nif_bytes

_FIXTURES = Path(__file__).parent / "fixtures" / "gardenfell_anim"


def _shapes(name: str) -> list:
    """Parse a fixture with animation retention and return its model-space shapes."""
    nif = read_nif_bytes((_FIXTURES / name).read_bytes(), geometry=True, animation=True)
    assert nif.stopped_reason == "", f"{name} did not fully parse: {nif.stopped_reason}"
    return model_shapes(nif)


def test_uvsets_nif_has_uv_animation() -> None:
    """uvsets.nif drives a scrolling/scaling texture (NiUVController)."""
    shapes = _shapes("uvsets.nif")
    assert shapes
    assert all(s.uv_anim is not None for s in shapes)


def test_anim_nif_has_node_keyframe_animation() -> None:
    """anim.nif sways a node (NiKeyframeController)."""
    shapes = _shapes("anim.nif")
    assert shapes
    assert any(s.transform_anim is not None for s in shapes)


def test_morph_nif_extracts_shapes_under_a_collision_switch() -> None:
    """morph.nif's shapes sit under a NiCollisionSwitch and must not be lost."""
    shapes = _shapes("morph.nif")
    # Two shapes, both with a morph animation -- the regression this guards is
    # the walk stopping at NiCollisionSwitch and returning zero shapes.
    assert len(shapes) == 2
    assert all(s.morph_anim is not None for s in shapes)
    assert all(s.morph_anim.targets for s in shapes)


def test_particle_move_nif_is_a_point_cloud() -> None:
    """A NiTriShape under a NiBSParticleNode is particle geometry, drawn as points."""
    shapes = _shapes("particle_move.nif")
    assert len(shapes) == 1
    assert shapes[0].points is True
    assert shapes[0].triangles == []  # points meshes drop their surface topology
    assert shapes[0].vertices  # the particle positions survive


def test_flap_kf_binds_node_animation_by_name() -> None:
    """flap.kf's tracks bind to flap.nif's bones, turning static shapes animated."""
    kf = load_kf(read_nif_bytes((_FIXTURES / "flap.kf").read_bytes(), animation=True))
    # The .kf names the bones it drives (B0, B1), each with a keyframe track.
    assert set(kf) == {"B0", "B1"}

    nif = read_nif_bytes((_FIXTURES / "flap.nif").read_bytes(), geometry=True, animation=True)
    # Without the .kf the shapes are static (the model carries no controllers);
    # binding the .kf by node name makes them animate.
    assert all(s.transform_anim is None for s in model_shapes(nif))
    assert all(s.transform_anim is not None for s in model_shapes(nif, kf))


def test_load_kf_on_a_plain_nif_is_empty() -> None:
    """A file that is not a keyframe sequence yields no tracks (not an error)."""
    nif = read_nif_bytes((_FIXTURES / "anim.nif").read_bytes(), animation=True)
    assert load_kf(nif) == {}


@pytest.mark.parametrize("name", ["morph.nif", "anim.nif", "uvsets.nif", "particle_move.nif"])
def test_fixtures_parse_without_stopping(name: str) -> None:
    """Each fixture parses to the end -- a layout gap would stop the reader early."""
    nif = read_nif_bytes((_FIXTURES / name).read_bytes(), geometry=True, animation=True)
    assert nif.stopped_reason == ""
    assert nif.stopped_at is None
