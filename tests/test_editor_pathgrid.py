"""wraithguard.patch.editor: path grids edited from the render window."""

from __future__ import annotations

import json
import struct
from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.esp.json import record_from_json
from wraithguard.patch.editor import EditorError, EditorSession
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.records import record_key

if TYPE_CHECKING:
    from pathlib import Path


def _conns(targets: list[int]) -> bytes:
    """The native reader's form: a u32 count, then the targets."""
    return struct.pack(f"<I{len(targets)}I", len(targets), *targets)


_PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Cell", "name": "Vault", "data": {"grid": [0, 0], "flags": "IS_INTERIOR"}},
        {"type": "Cell", "name": "Hall", "data": {"grid": [0, 0], "flags": "IS_INTERIOR"}},
        {"type": "Cell", "name": "", "data": {"grid": [3, -1], "flags": ""}},
        {
            "type": "PathGrid",
            "flags": "",
            "cell": "Vault",
            "data": {"grid": [0, 0], "granularity": 1024, "point_count": 3},
            "points": [
                {"location": [0, 0, 0], "auto_generated": 1, "connection_count": 1},
                {"location": [100, 0, 0], "auto_generated": 1, "connection_count": 2},
                {"location": [100, 100, 0], "auto_generated": 1, "connection_count": 1},
            ],
            "connections": _conns([1, 0, 2, 1]),
        },
        {
            "type": "PathGrid",
            "flags": "",
            "cell": "Hall",
            "data": {"grid": [0, 0], "granularity": 1024, "point_count": 0},
            "points": [],
            "connections": _conns([]),
        },
    ],
}

_TYPES = {"CELL": "Cell", "PGRD": "PathGrid"}


def _session(tmp_path: Path) -> EditorSession:
    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        return [r for r in _PLUGINS[path.name] if r["type"] == _TYPES[tag]]

    order = [(name, tmp_path / name) for name in _PLUGINS]
    return EditorSession(order, PatchQueue(), journal=tmp_path / "journal.json", read=read)


def test_interior_path_grids_are_keyed_by_their_cell():
    vault, hall = _PLUGINS["Morrowind.esm"][3:5]
    assert record_key(vault) == "Vault"
    assert record_key(hall) == "Hall"
    ext = {"type": "PathGrid", "cell": "Balmora", "data": {"grid": [-3, -2]}}
    assert record_key(ext) == "(-3, -2)"


def test_view_slices_connections_per_point(tmp_path):
    v = _session(tmp_path).pathgrid_view("int:Vault")
    assert v["points"] == [[0, 0, 0], [100, 0, 0], [100, 100, 0]]
    assert v["edges"] == [[0, 1], [1, 0], [1, 2], [2, 1]]
    assert v["plugins"] == ["Morrowind.esm"]
    assert not v["queued"]


def test_set_queues_points_links_and_count(tmp_path):
    s = _session(tmp_path)
    v = s.set_pathgrid(
        "int:Vault",
        [[0, 0, 0], [100, 0, 0], [100, 100, 0], [0, 100, 4.6]],
        [[0, 1], [1, 0], [2, 3], [3, 2], [1, 2], [2, 1]],
    )
    assert v["queued"]
    assert v["points"][3] == [0, 100, 5]
    assert sorted(map(tuple, v["edges"])) == [(0, 1), (1, 0), (1, 2), (2, 1), (2, 3), (3, 2)]
    choices = {c.path: c.value for c in s.queue.fields[("PathGrid", "Vault")]}
    assert choices["data.point_count"] == 4
    assert [p["connection_count"] for p in choices["points"]] == [1, 2, 2, 1]
    assert choices["connections"] == [1, 0, 2, 1, 3, 2]
    # The journal holds it (plain numbers, so it is JSON).
    doc = json.loads((tmp_path / "journal.json").read_text(encoding="utf-8"))
    assert doc["fields"][0]["key"] == "Vault"


def test_a_cell_without_one_gets_the_patchs_own(tmp_path):
    s = _session(tmp_path)
    v = s.set_pathgrid("3,-1", [[10, 10, 0], [500, 10, 0]], [[0, 1], [1, 0]])
    assert v["new"] and v["grid"] == [3, -1]
    made = s.queue.new_record("PathGrid", "(3, -1)")
    assert made is not None
    assert made.record["data"]["grid"] == [3, -1]
    assert made.source == "Morrowind.esm"
    # Edited again, it stays the patch's own record.
    v = s.set_pathgrid("3,-1", [[10, 10, 0]], [])
    assert v["points"] == [[10, 10, 0]] and v["edges"] == []
    # And it writes: the editor's list of links is what the record holds.
    rec = record_from_json(dict(s.queue.new_record("PathGrid", "(3, -1)").record))
    assert rec.connections == [] and len(rec.points) == 1
    s.revert_pathgrid("3,-1")
    assert s.queue.new_record("PathGrid", "(3, -1)") is None


def test_queued_connections_write_as_numbers():
    rec = record_from_json(
        {
            "type": "PathGrid",
            "flags": "",
            "cell": "Vault",
            "data": {"grid": [0, 0], "granularity": 1024, "point_count": 2},
            "points": [
                {"location": [0, 0, 0], "auto_generated": 1, "connection_count": 1},
                {"location": [1, 0, 0], "auto_generated": 1, "connection_count": 1},
            ],
            "connections": [1, 0],
        }
    )
    assert rec.connections == [1, 0]


@pytest.mark.parametrize(
    ("points", "edges"),
    [([[0, 0]], []), ([[0, 0, 0]], [[0, 0]]), ([[0, 0, 0]], [[0, 5]]), ([["a", 0, 0]], [])],
)
def test_bad_grids_are_refused(tmp_path, points, edges):
    with pytest.raises(EditorError):
        _session(tmp_path).set_pathgrid("int:Hall", points, edges)
