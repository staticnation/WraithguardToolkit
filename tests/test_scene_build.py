"""Tests for ``wraithguard.scene.build`` -- model-space instanced assembly.

The mesh loader is injected, so no NIF/VFS IO: a fake loader returns known
model-space meshes and we check grouping, placement matrices, and concurrency.
"""

from __future__ import annotations

import math
from dataclasses import replace

from wraithguard.nif.geometry import Mesh, Transform
from wraithguard.scene.build import build_instanced, matrix4_columns
from wraithguard.scene.resolve import Placement, reference_transform


def _mesh() -> Mesh:
    return Mesh(name="s", vertices=[(1.0, 0.0, 0.0)], triangles=[(0, 0, 0)])


def _placed(model: str, translation: tuple[float, float, float]) -> Placement:
    return Placement(
        ref_id="r",
        model=model,
        transform=Transform(translation=translation),
        kind="placed",
    )


def _placed_typed(model: str, record_type: str) -> Placement:
    """A placed reference carrying a record type (for group categorisation)."""
    return Placement(
        ref_id="r", model=model, transform=Transform(), kind="placed", record_type=record_type
    )


def test_a_repeatedly_missing_model_is_recorded_once() -> None:
    # The same unloadable model placed twice must not list twice (the second hit
    # takes the "already recorded" branch).
    placements = [_placed("gone.nif", (0.0, 0.0, 0.0)), _placed("gone.nif", (5.0, 0.0, 0.0))]
    scene = build_instanced(placements, lambda _p: None)
    assert scene.missing_models == ["gone.nif"]


def test_parallel_workers_match_the_serial_build() -> None:
    """Warming the parse on a thread pool must produce the identical cell."""
    placements = [
        _placed("rock.nif", (1.0, 0.0, 0.0)),
        _placed("crate.nif", (2.0, 0.0, 0.0)),
        _placed("rock.nif", (3.0, 0.0, 0.0)),  # repeat -> one group, two instances
        _placed("gone.nif", (4.0, 0.0, 0.0)),  # unloadable -> missing
    ]

    def loader(model: str) -> list[Mesh] | None:
        return None if model == "gone.nif" else [_mesh()]

    serial = build_instanced(placements, loader, workers=1)
    parallel = build_instanced(placements, loader, workers=4)

    assert [g.model for g in parallel.groups] == [g.model for g in serial.groups]
    assert [g.matrices for g in parallel.groups] == [g.matrices for g in serial.groups]
    assert parallel.drawn == serial.drawn
    assert parallel.missing_models == serial.missing_models


class TestMatrix4Columns:
    """The column-major 4x4 a THREE.Matrix4 / InstancedMesh expects."""

    def test_identity_transform_is_the_identity_matrix(self) -> None:
        assert matrix4_columns(Transform()) == (
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        )  # fmt: skip

    def test_translation_lands_in_the_last_column(self) -> None:
        cols = matrix4_columns(Transform(translation=(10.0, 20.0, 30.0)))
        assert cols[12:15] == (10.0, 20.0, 30.0)
        assert cols[15] == 1.0

    def test_uniform_scale_multiplies_the_rotation_block(self) -> None:
        cols = matrix4_columns(Transform(scale=2.0))
        assert (cols[0], cols[5], cols[10]) == (2.0, 2.0, 2.0)

    def test_it_matches_applying_the_transform_to_a_point(self) -> None:
        # Building the matrix and multiplying a point (column-major) must equal
        # Transform.apply -- the two ways the same placement gets positioned.
        transform = reference_transform((3.0, -4.0, 5.0), (0.3, -0.7, 1.1), 1.5)
        cols = matrix4_columns(transform)
        point = (2.0, 1.0, -0.5)
        # column-major m: element(row r, col c) = cols[c*4 + r]
        expect = transform.apply(point)
        got = tuple(
            cols[0 * 4 + r] * point[0]
            + cols[1 * 4 + r] * point[1]
            + cols[2 * 4 + r] * point[2]
            + cols[3 * 4 + r]  # translation column, w = 1
            for r in range(3)
        )
        assert all(math.isclose(g, e, abs_tol=1e-6) for g, e in zip(got, expect))


class TestBuildInstanced:
    """Grouping placements by model instead of baking each one."""

    def test_repeated_model_becomes_one_group_with_many_matrices(self) -> None:
        calls: list[str] = []

        def loader(path: str) -> list[Mesh]:
            calls.append(path)
            return [_mesh()]

        placements = [
            _placed("rock.nif", (0.0, 0.0, 0.0)),
            _placed("rock.nif", (5.0, 0.0, 0.0)),
            _placed("rock.nif", (9.0, 0.0, 0.0)),
        ]
        cell = build_instanced(placements, loader)
        assert calls == ["rock.nif"]  # loaded once for three placements
        assert len(cell.groups) == 1
        group = cell.groups[0]
        assert group.count == 3
        assert len(group.matrices) == 48  # 3 instances * 16 floats
        assert cell.drawn == 3
        # Vertices are NOT baked -- they stay in model space.
        assert group.meshes[0].vertices == [(1.0, 0.0, 0.0)]

    def test_distinct_models_are_separate_groups_in_first_seen_order(self) -> None:
        placements = [
            _placed("b.nif", (0.0, 0.0, 0.0)),
            _placed("a.nif", (0.0, 0.0, 0.0)),
            _placed("b.nif", (1.0, 0.0, 0.0)),
        ]
        cell = build_instanced(placements, lambda _p: [_mesh()])
        assert [g.model for g in cell.groups] == ["b.nif", "a.nif"]
        assert [g.count for g in cell.groups] == [2, 1]

    def test_an_unloadable_model_is_recorded_not_fatal(self) -> None:
        cell = build_instanced([_placed("gone.nif", (0.0, 0.0, 0.0))], lambda _p: None)
        assert cell.groups == []
        assert cell.drawn == 0
        assert cell.missing_models == ["gone.nif"]

    def test_non_drawable_placements_are_skipped(self) -> None:
        placements = [
            Placement(ref_id="a", model="", transform=Transform(), kind="no_mesh_record"),
            Placement(ref_id="b", model="x.nif", transform=Transform(), kind="actor"),
        ]
        cell = build_instanced(placements, lambda _p: [_mesh()])
        assert cell.groups == [] and cell.drawn == 0

    def test_the_matrix_carries_the_placement_translation(self) -> None:
        cell = build_instanced([_placed("rock.nif", (7.0, 8.0, 9.0))], lambda _p: [_mesh()])
        matrices = cell.groups[0].matrices
        assert matrices[12:15] == [7.0, 8.0, 9.0]

    def test_a_repeatedly_missing_model_is_recorded_once(self) -> None:
        placements = [_placed("gone.nif", (0.0, 0.0, 0.0)), _placed("gone.nif", (5.0, 0.0, 0.0))]
        cell = build_instanced(placements, lambda _p: None)
        assert cell.missing_models == ["gone.nif"]  # not listed twice

    def test_an_emitter_file_is_recategorised_as_emitter(self) -> None:
        emitter = replace(_mesh(), emitter=True)
        cell = build_instanced([_placed_typed("mist.nif", "Static")], lambda _p: [emitter])
        assert cell.groups[0].record_type == "Emitter"

    def test_a_light_that_emits_stays_a_light(self) -> None:
        # A torch carries a flame particle but reads as a light first.
        flame = replace(_mesh(), emitter=True)
        cell = build_instanced([_placed_typed("torch.nif", "Light")], lambda _p: [flame])
        assert cell.groups[0].record_type == "Light"

    def test_a_plain_static_keeps_its_record_type(self) -> None:
        cell = build_instanced([_placed_typed("rock.nif", "Static")], lambda _p: [_mesh()])
        assert cell.groups[0].record_type == "Static"
