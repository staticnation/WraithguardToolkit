"""The viewer editor's placed objects: found as the load order resolves them, changed
into the patch pool (wraithguard.patch.editor, wraithguard.gui.editorlink)."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch.editor import EditorError, EditorSession, coerce_ref, restore_queue
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path


def _ref(mast: int, refr: int, rid: str, x: float, **more: Any) -> dict[str, Any]:
    return {
        "mast_index": mast,
        "refr_index": refr,
        "id": rid,
        "temporary": False,
        "translation": [x, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        **more,
    }


def _cell(*refs: dict[str, Any]) -> dict[str, Any]:
    return {
        "type": "Cell",
        "flags": "",
        "id": "Ebon Tower",
        "name": "Ebon Tower",
        "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
        "references": list(refs),
    }


# Tamriel_Data's reference 15 is (1, 15) in A and (2, 15) in B, whose first master is
# OAAB: one object, B's version winning.
PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Tamriel_Data.esm": [
        {"type": "Header", "masters": []},
        _cell(_ref(0, 15, "iron dagger", 1.0), _ref(0, 16, "chair", 5.0)),
    ],
    "OAAB.esm": [{"type": "Header", "masters": []}],
    "A.esp": [
        {"type": "Header", "masters": [["Tamriel_Data.esm", 1]]},
        _cell(_ref(1, 15, "iron dagger", 2.0)),
    ],
    "B.esp": [
        {"type": "Header", "masters": [["OAAB.esm", 1], ["Tamriel_Data.esm", 1]]},
        _cell(_ref(2, 15, "iron dagger", 3.0, scale=1.5)),
    ],
}


def _session(tmp_path: Path, reads: list | None = None) -> EditorSession:
    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        if reads is not None:
            reads.append((path.name, tag))
        want = {"TES3": "Header", "CELL": "Cell"}[tag]
        return [r for r in PLUGINS[path.name] if r["type"] == want]

    order = [(name, tmp_path / name) for name in PLUGINS]
    return EditorSession(order, PatchQueue(), journal=tmp_path / "journal.json", read=read)


def test_find_ref_resolves_the_load_orders_winner(tmp_path):
    reads: list = []
    s = _session(tmp_path, reads)
    found = s.find_ref("ebon tower", "tamriel_data.esm", 15, ["B.esp", "A.esp", "Tamriel_Data.esm"])
    assert found is not None
    assert found.origin == "Tamriel_Data.esm"  # as the load order spells it
    assert found.cell == "Ebon Tower"
    assert found.winner == "B.esp" and found.ref["translation"][0] == 3.0
    assert found.plugins == ("Tamriel_Data.esm", "A.esp", "B.esp")
    assert ("OAAB.esm", "CELL") not in reads  # only the plugins the viewer named
    assert s.find_ref("Ebon Tower", "Tamriel_Data.esm", 99) is None
    assert s.find_ref("Nowhere", "Tamriel_Data.esm", 15) is None


def test_ref_view_set_revert_and_pending(tmp_path):
    s = _session(tmp_path)
    found = s.find_ref("Ebon Tower", "Tamriel_Data.esm", 15)
    assert found is not None
    view = s.ref_view(found)
    fields = {f["path"]: f for f in view["fields"]}
    assert view["id"] == "iron dagger" and view["winner"] == "B.esp"
    assert fields["scale"]["value"] == 1.5 and fields["lock_level"]["value"] is None
    assert fields["destination"]["kind"] == "door"

    s.set_ref_field(found, "translation", [7, "8", 9.5])
    s.set_ref_field(found, "deleted", True)
    s.set_ref_field(found, "scale", 1.5)  # what it already is: nothing to queue
    (edit,) = s.queue.ref_edits
    assert edit.changes == {"translation": [7.0, 8.0, 9.5], "deleted": True}
    assert edit.plugins == found.plugins
    view = {f["path"]: f for f in s.ref_view(found)["fields"]}
    assert view["translation"]["queued"] == [7.0, 8.0, 9.5]
    assert len(s.queue) == 1

    (pend,) = s.pending()
    assert pend["type"] == "Reference"
    assert pend["ref"] == {
        "cell": "Ebon Tower",
        "origin": "Tamriel_Data.esm",
        "refr": 15,
        "plugins": ["Tamriel_Data.esm", "A.esp", "B.esp"],
    }
    assert {c["path"] for c in pend["changes"]} == {"translation", "deleted"}

    # the journal brings it back
    again = PatchQueue()
    assert restore_queue(again, tmp_path / "journal.json") == 1
    assert again.ref_edits[0].changes == edit.changes

    s.revert_ref(found, "deleted")
    assert s.queue.ref_edits[0].changes == {"translation": [7.0, 8.0, 9.5]}
    s.set_ref_field(found, "translation", [3.0, 0.0, 0.0])  # back to the winner's
    assert s.queue.ref_edits == [] and not (tmp_path / "journal.json").exists()


def test_coerce_ref_types_and_limits():
    assert coerce_ref("rotation", [0, 0, "1.5"]) == [0.0, 0.0, 1.5]
    assert coerce_ref("lock_level", "50") == 50
    assert coerce_ref("owner", "") is None  # cleared
    assert coerce_ref("lock_level", "") is None
    assert coerce_ref("deleted", "yes") is True
    assert coerce_ref("moved_cell", [-2, "9"]) == [-2, 9]
    assert coerce_ref("destination", {"translation": [1, 2, 3], "cell": " Balmora "}) == {
        "translation": [1.0, 2.0, 3.0],
        "rotation": [0.0, 0.0, 0.0],
        "cell": "Balmora",
    }
    assert coerce_ref("destination", None) is None  # an ordinary door
    for name, bad in [
        ("translation", [1, 2]),
        ("translation", [1, 2, float("nan")]),
        ("scale", 3.0),
        ("lock_level", 1.5),
        ("destination", {"cell": "x"}),
        ("destination", {"translation": [1, 2, 3], "cell": 4}),
        ("id", "x"),
        ("refr_index", 1),
    ]:
        with pytest.raises(EditorError):
            coerce_ref(name, bad)


class _Host:
    """The mixin's endpoints, with the "UI thread" the calling one."""

    def __init__(self, session: EditorSession) -> None:
        self._editor_session = session
        self.refreshed = 0

    def _schedule_ui(self, _delay: int, fn: Any, *args: Any) -> None:
        fn(*args)

    def refresh_patch_views(self) -> None:
        self.refreshed += 1


def _host(tmp_path: Path) -> Any:
    from wraithguard.gui.editorlink import EditorLinkMixin

    cls = type("Host", (_Host, EditorLinkMixin), {})
    return cls(_session(tmp_path))


def _post(handler: Any, **body: Any) -> Any:
    return json.loads(handler(json.dumps(body).encode("utf-8")).body)


def test_ref_endpoints(tmp_path):
    host = _host(tmp_path)
    req = {"cell": "Ebon Tower", "origin": "Tamriel_Data.esm", "refr": 15}
    assert _post(host._on_edit_ref, **req)["winner"] == "B.esp"
    view = _post(host._on_edit_ref_set, **req, path="lock_level", value=40)
    assert {f["path"]: f for f in view["fields"]}["lock_level"]["queued"] == 40
    assert host.refreshed == 1
    view = _post(host._on_edit_ref_revert, **req)
    assert all("queued" not in f for f in view["fields"])
    with pytest.raises(ValueError, match="bad refr"):
        host._on_edit_ref(json.dumps({**req, "refr": "15"}).encode())
    with pytest.raises(ValueError, match="No plugin"):
        host._on_edit_ref(json.dumps({**req, "refr": 99}).encode())
    with pytest.raises(EditorError):
        host._on_edit_ref_set(json.dumps({**req, "path": "scale", "value": 9}).encode())


def test_a_cleared_field_is_written_absent(tmp_path):
    """A cleared optional field (None) leaves the written reference without it."""
    native = pytest.importorskip("wraithguard_native")
    from wraithguard.land.emit import build_plugin
    from wraithguard.patch.refedit import RefEdit, cell_patch_record

    masters = ["Tamriel_Data.esm", "OAAB.esm", "B.esp"]
    edit = RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"scale": None, "lock_level": 30})
    rec = cell_patch_record([edit], PLUGINS, list(PLUGINS), masters)
    doc = build_plugin([rec], [(m, 100) for m in masters], description="test")
    back = native.plugin_records(native.plugin_records_bytes(json.dumps(doc)))
    (cell,) = [r for r in back if r["type"] == "Cell"]
    (ref,) = cell["references"]
    assert "scale" not in ref and ref["lock_level"] == 30


def test_a_door_destination_is_written(tmp_path):
    """A destination set on a reference: DODT, and DNAM only for an interior."""
    native = pytest.importorskip("wraithguard_native")
    from wraithguard.land.emit import build_plugin
    from wraithguard.patch.refedit import RefEdit, cell_patch_record

    masters = ["Tamriel_Data.esm", "OAAB.esm", "B.esp"]
    out = {}
    for cell in ("Ebon Tower, Upper", ""):
        dest = coerce_ref(
            "destination", {"translation": [1, 2, 3], "rotation": [0, 0, 1.5], "cell": cell}
        )
        edit = RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"destination": dest})
        rec = cell_patch_record([edit], PLUGINS, list(PLUGINS), masters)
        doc = build_plugin([rec], [(m, 100) for m in masters], description="test")
        back = native.plugin_records(native.plugin_records_bytes(json.dumps(doc)))
        (c,) = [r for r in back if r["type"] == "Cell"]
        out[cell] = c["references"][0]["destination"]
    assert out["Ebon Tower, Upper"]["cell"] == "Ebon Tower, Upper"
    assert out[""]["cell"] == "" and out[""]["translation"] == [1.0, 2.0, 3.0]
    assert out[""]["rotation"][2] == pytest.approx(1.5)


def _exterior(*refs: dict[str, Any], grid: tuple[int, int]) -> dict[str, Any]:
    return {
        "type": "Cell",
        "flags": "",
        "id": "",
        "name": "",
        "data": {"flags": "", "grid": list(grid)},
        "references": list(refs),
    }


def test_moving_across_an_exterior_edge_sets_moved_cell(tmp_path):
    plugins = {
        "Morrowind.esm": [
            {"type": "Header", "masters": []},
            {"type": "Static", "id": "rock", "mesh": "x/rock.nif"},
            _exterior(_ref(0, 7, "rock", -2 * 8192 + 100.0), grid=(-2, -9)),
            _exterior(grid=(-1, -9)),
        ]
    }

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        want = {"TES3": "Header", "CELL": "Cell", "STAT": "Static"}[tag]
        return [r for r in plugins[path.name] if r["type"] == want]

    s = EditorSession(
        [("Morrowind.esm", tmp_path / "Morrowind.esm")], PatchQueue(), tmp_path / "j.json", read
    )
    found = s.find_ref("(-2, -9)", "Morrowind.esm", 7)
    assert found is not None
    y = -9 * 8192 + 10.0
    s.set_ref_field(found, "translation", [-8192 + 50.0, y, 0])
    assert s.queue.ref_edits[0].changes["moved_cell"] == [-1, -9]
    s.set_ref_field(found, "translation", [-2 * 8192 + 300.0, y, 0])  # back home
    assert "moved_cell" not in s.queue.ref_edits[0].changes
    s.set_ref_field(found, "translation", [-8192 + 50.0, y, 0])
    s.revert_ref(found, "translation")
    assert s.queue.ref_edits == []

    # A new reference moves to the cell it now stands in.
    rock = s.find("STAT", "rock")
    assert rock is not None
    new = s.place("(-2, -9)", rock, [-2 * 8192 + 10.0, y, 0])
    new = s.set_new_field(new, "translation", [-8192 + 10.0, y, 0])
    assert new.cell == "(-1, -9)" and new.plugins == ("Morrowind.esm",)
    assert [n.cell for n in s.queue.new_refs] == ["(-1, -9)"]
    with pytest.raises(EditorError, match="no plugin"):
        s.set_new_field(new, "translation", [9 * 8192.0, y, 0])


def test_a_reference_moved_in_from_another_cell_is_found_at_home(tmp_path):
    """The viewer finds it where it stands; its record is in the cell it left."""
    moved = _ref(0, 5, "rock", 8192 + 10.0, moved_cell=[1, 0])
    plugins = {
        "Morrowind.esm": [
            {"type": "Header", "masters": []},
            _exterior(moved, grid=(0, 0)),
            _exterior(grid=(1, 0)),
        ]
    }

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        want = {"TES3": "Header", "CELL": "Cell"}.get(tag)
        return [r for r in plugins[path.name] if r["type"] == want]

    s = EditorSession([("Morrowind.esm", tmp_path / "Morrowind.esm")], PatchQueue(), None, read)
    found = s.find_ref("(1, 0)", "Morrowind.esm", 5, ["Morrowind.esm"])
    assert found is not None and found.cell == "(0, 0)"
    # Moved further along: still keyed at home, moved_cell following it.
    s.set_ref_field(found, "translation", [2 * 8192 + 5.0, 10.0, 0])
    assert s.queue.ref_edits[0].cell == "(0, 0)"
    assert s.queue.ref_edits[0].changes["moved_cell"] == [2, 0]
    assert s.find_ref("(1, 0)", "Morrowind.esm", 99) is None
