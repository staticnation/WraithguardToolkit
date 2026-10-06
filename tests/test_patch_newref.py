"""New references: placed by the editor, written as the patch's own ``(0, n)``."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.patch.editor import EditorError, EditorSession, restore_queue
from wraithguard.patch.queue import PatchQueue
from wraithguard.patch.records import PatchError
from wraithguard.patch.refedit import NewRef, RefEdit, place_new_refs

if TYPE_CHECKING:
    from pathlib import Path


def _ref(mast: int, refr: int, rid: str, x: float) -> dict[str, Any]:
    return {
        "mast_index": mast,
        "refr_index": refr,
        "id": rid,
        "temporary": True,
        "translation": [x, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
    }


def _cell(*refs: dict[str, Any]) -> dict[str, Any]:
    return {
        "type": "Cell",
        "flags": "",
        "id": "Ebon Tower",
        "name": "Ebon Tower",
        "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
        "water_height": 2.0,
        "references": list(refs),
    }


PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Tamriel_Data.esm": [
        {"type": "Header", "masters": []},
        {"type": "Static", "id": "T_Chair", "mesh": "x/chair.nif"},
        {"type": "Npc", "id": "T_Guard", "name": "Guard", "data": {"level": 1}, "flags": ""},
        _cell(_ref(0, 15, "iron dagger", 1.0)),
    ],
    "B.esp": [
        {"type": "Header", "masters": [["Tamriel_Data.esm", 1]]},
        {**_cell(_ref(1, 15, "iron dagger", 3.0)), "water_height": 5.0},
    ],
}
ORDER = list(PLUGINS)


def _new(uid: str, rid: str = "T_Chair", x: float = 10.0, **more: Any) -> NewRef:
    fields = {"id": rid, "translation": [x, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], **more}
    return NewRef("Ebon Tower", uid, fields, "Tamriel_Data.esm", ("Tamriel_Data.esm", "B.esp"))


def test_new_references_number_on_from_the_patchs_own():
    carried_cell = _cell(_ref(0, 4, "torch", 9.0), _ref(1, 15, "iron dagger", 7.0))
    records: list[dict[str, Any]] = [carried_cell]
    given = place_new_refs(records, [_new("a"), _new("b", x=20.0)], PLUGINS, ORDER)
    assert given == {"a": 5, "b": 6}
    assert len(records) == 1  # into the cell the patch carries
    added = [r for r in carried_cell["references"] if r["refr_index"] in (5, 6)]
    assert [(r["mast_index"], r["id"], r["translation"][0]) for r in added] == [
        (0, "T_Chair", 10.0),
        (0, "T_Chair", 20.0),
    ]


def test_a_cell_the_patch_lacks_is_made_from_the_winner_with_only_them():
    records: list[dict[str, Any]] = []
    place_new_refs(records, [_new("a", temporary=False)], PLUGINS, ORDER)
    (cell,) = records
    assert cell["water_height"] == 5.0  # B's: the winning version's fields
    assert [(r["mast_index"], r["refr_index"], r["temporary"]) for r in cell["references"]] == [
        (0, 1, False)
    ]


def test_refusals():
    with pytest.raises(PatchError, match="no plugin"):
        place_new_refs(
            [], [NewRef("Nowhere", "a", {"id": "x", "translation": [0, 0, 0]})], PLUGINS, ORDER
        )
    with pytest.raises(PatchError, match="no object or no position"):
        place_new_refs([], [NewRef("Ebon Tower", "a", {"id": "x"})], PLUGINS, ORDER)
    with pytest.raises(PatchError, match="not a field"):
        place_new_refs([], [_new("a", colour=1)], PLUGINS, ORDER)


def test_build_record_patch_writes_new_references_and_its_own(tmp_path):
    native = pytest.importorskip("wraithguard_native")
    from wraithguard.patch.service import build_record_patch

    out = tmp_path / "Patch.esp"
    sizes = dict.fromkeys(ORDER, 1000)
    result = build_record_patch([], PLUGINS, ORDER, sizes, "", out, new_refs=[_new("a")])
    assert result.masters == ["Tamriel_Data.esm"]
    back = native.plugin_records(out.read_bytes())
    (cell,) = [r for r in back if r["type"] == "Cell"]
    assert [(r["mast_index"], r["refr_index"], r["id"]) for r in cell["references"]] == [
        (0, 1, "T_Chair")
    ]

    # Next session: the patch is in the load order. Moving the reference it made, and
    # placing another, appending to the build: its own stays (0, 1), the next is (0, 2),
    # and the patch is never its own master.
    plugins = {**PLUGINS, "Patch.esp": back}
    order = [*ORDER, "Patch.esp"]
    sizes = dict.fromkeys(order, 1000)
    move = RefEdit("Ebon Tower", "Patch.esp", 1, {"translation": [55.0, 0.0, 0.0]}, tuple(order))
    result = build_record_patch(
        [],
        plugins,
        order,
        sizes,
        "",
        out,
        carried=back,
        ref_edits=[move],
        new_refs=[_new("b", x=30.0)],
    )
    assert result.masters == ["Tamriel_Data.esm"]
    again = native.plugin_records(out.read_bytes())
    (cell,) = [r for r in again if r["type"] == "Cell"]
    got = {(r["mast_index"], r["refr_index"]): r for r in cell["references"]}
    assert set(got) == {(0, 1), (0, 2)}
    assert got[(0, 1)]["translation"][0] == pytest.approx(55.0)
    assert got[(0, 2)]["translation"][0] == pytest.approx(30.0)


def _session(tmp_path: Path) -> EditorSession:
    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        want = {"TES3": "Header", "CELL": "Cell", "STAT": "Static", "NPC_": "Npc"}.get(tag)
        return [r for r in PLUGINS[path.name] if r["type"] == want]

    order = [(name, tmp_path / name) for name in PLUGINS]
    return EditorSession(order, PatchQueue(), journal=tmp_path / "journal.json", read=read)


def test_session_places_changes_and_removes(tmp_path):
    s = _session(tmp_path)
    chair = s.find("STAT", "t_chair")
    assert chair is not None
    new = s.place("ebon tower", chair, [1, 2, 3], plugins=["B.esp", "Tamriel_Data.esm"])
    assert new.cell == "Ebon Tower" and new.base_plugin == "Tamriel_Data.esm"
    assert new.plugins == ("Tamriel_Data.esm", "B.esp")
    assert new.fields["id"] == "T_Chair" and new.fields["temporary"] is True
    guard = s.find("NPC_", "t_guard")
    assert guard is not None
    assert s.place("Ebon Tower", guard, [0, 0, 0]).fields["temporary"] is False  # actors persist
    assert len(s.queue.new_refs) == 2 and len(s.queue) == 1  # one cell

    new = s.set_new_field(new, "scale", 1.5)
    view = {f["path"]: f for f in s.new_view(new)["fields"]}
    assert view["scale"]["value"] == 1.5 and view["translation"]["value"] == [1.0, 2.0, 3.0]
    with pytest.raises(EditorError, match="three numbers"):
        s.set_new_field(new, "translation", "")
    pend = [p for p in s.pending() if p["type"] == "NewReference"]
    assert {p["new"]["id"] for p in pend} == {"T_Chair", "T_Guard"}

    again = PatchQueue()
    assert restore_queue(again, tmp_path / "journal.json") == 2
    assert {n.fields["id"] for n in again.new_refs} == {"T_Chair", "T_Guard"}

    s.remove_new(new)
    assert [n.fields["id"] for n in s.queue.new_refs] == ["T_Guard"]
    with pytest.raises(EditorError, match="no plugin"):
        s.place("Nowhere", chair, [0, 0, 0])


class _Host:
    def __init__(self, session: EditorSession) -> None:
        self._editor_session = session
        self.refreshed = 0

    def _schedule_ui(self, _delay: int, fn: Any, *args: Any) -> None:
        fn(*args)

    def refresh_patch_views(self) -> None:
        self.refreshed += 1


def _post(handler: Any, **body: Any) -> Any:
    return json.loads(handler(json.dumps(body).encode("utf-8")).body)


def test_place_endpoints(tmp_path):
    from wraithguard.gui.editorlink import EditorLinkMixin

    host = type("Host", (_Host, EditorLinkMixin), {})(_session(tmp_path))
    v = _post(
        host._on_edit_place,
        cell="Ebon Tower",
        tag="STAT",
        id="T_Chair",
        translation=[5, 6, 7],
        rotation=[0, 0, 1],
        plugins=["Tamriel_Data.esm", "B.esp"],
        defined=["Tamriel_Data.esm"],
    )
    assert v["new"] and v["id"] == "T_Chair" and v["uid"].startswith("new-")
    req = {"cell": v["cell"], "uid": v["uid"]}
    assert _post(host._on_edit_new, **req)["uid"] == v["uid"]
    v = _post(host._on_edit_new_set, **req, path="translation", value=[8, 6, 7])
    assert {f["path"]: f for f in v["fields"]}["translation"]["value"] == [8.0, 6.0, 7.0]
    assert host._on_edit_new_remove(json.dumps(req).encode()).body == b"ok"
    with pytest.raises(ValueError, match="no longer in the patch"):
        host._on_edit_new(json.dumps(req).encode())
    with pytest.raises(ValueError, match="not a record"):
        host._on_edit_place(
            json.dumps(
                {"cell": "Ebon Tower", "tag": "STAT", "id": "nothing", "translation": [0, 0, 0]}
            ).encode()
        )


def test_duplicate_makes_a_record_of_the_patchs_own(tmp_path):
    s = _session(tmp_path)
    chair = s.find("STAT", "T_Chair")
    assert chair is not None
    copy = s.duplicate(chair, "  WG_Chair  ")
    assert copy.new and copy.key == "WG_Chair" and copy.winner == "(this patch)"
    assert s.find("STAT", "wg_chair") == copy  # found from the pool, any case
    s.set_field(copy, "mesh", "x/chair2.nif")
    made = s.queue.new_record("Static", "WG_Chair")
    assert made is not None and made.record["mesh"] == "x/chair2.nif"
    assert made.source == "Tamriel_Data.esm"
    assert s.view(s.find("STAT", "WG_Chair"))["new"] is True
    with pytest.raises(EditorError, match="already"):
        s.duplicate(chair, "t_chair")
    with pytest.raises(EditorError, match="already"):
        s.duplicate(chair, "WG_CHAIR")  # the patch's own counts
    with pytest.raises(EditorError, match="at most"):
        s.duplicate(chair, "x" * 32)
    with pytest.raises(EditorError, match="nothing to go back to"):
        s.revert(copy, "mesh")
    pend = [p for p in s.pending() if p.get("made")]
    assert pend[0]["id"] == "WG_Chair" and pend[0]["made"]["mesh"] == "x/chair2.nif"

    # placed, the copy needs no master of its own; the journal keeps both
    new = s.place("Ebon Tower", copy, [0, 0, 0])
    assert new.base_plugin == ""
    again = PatchQueue()
    assert restore_queue(again, tmp_path / "journal.json") == 2
    assert again.new_record("Static", "wg_chair") is not None

    s.revert(copy)
    assert s.queue.new_records == []


def test_build_record_patch_writes_made_records(tmp_path):
    native = pytest.importorskip("wraithguard_native")
    from wraithguard.patch.records import NewRecord
    from wraithguard.patch.service import build_record_patch

    made = NewRecord(
        "Static",
        "WG_Chair",
        {"type": "Static", "id": "WG_Chair", "flags": "", "mesh": "x/c.nif"},
        "B.esp",
    )
    out = tmp_path / "Patch.esp"
    result = build_record_patch(
        [], PLUGINS, ORDER, dict.fromkeys(ORDER, 1000), "", out, new_records=[made]
    )
    assert result.masters == ["Tamriel_Data.esm", "B.esp"]  # the source and its master
    back = native.plugin_records(out.read_bytes())
    assert [(r["type"], r["id"], r["mesh"]) for r in back if r["type"] == "Static"] == [
        ("Static", "WG_Chair", "x/c.nif")
    ]


def test_insert_makes_a_blank_record(tmp_path):
    s = _session(tmp_path)
    made = s.insert("stat", "WG_Rock")
    assert made.new and made.record["type"] == "Static" and made.record["id"] == "WG_Rock"
    assert made.record["mesh"] == ""  # every field at its default
    assert s.find("STAT", "wg_rock") is not None
    with pytest.raises(EditorError, match="already"):
        s.insert("STAT", "t_chair")
    with pytest.raises(EditorError, match="cannot be made"):
        s.insert("CELL", "x")
    with pytest.raises(EditorError, match="no record type"):
        s.insert("XXXX", "x")
    script = s.insert("SCPT", "wg_hello")
    assert script.record["text"].startswith("begin wg_hello")


def test_blank_records_write(tmp_path):
    """Every field a blank record has is one the backend writes."""
    native = pytest.importorskip("wraithguard_native")
    from wraithguard.land.emit import build_plugin

    s = _session(tmp_path)
    recs = [
        s.insert(tag, f"wg_{tag.lower().strip('_')}").record
        for tag in ("STAT", "NPC_", "SCPT", "LEVI", "CONT")
    ]
    doc = build_plugin(recs, [("Morrowind.esm", 1)], description="t")
    back = native.plugin_records(native.plugin_records_bytes(json.dumps(doc)))
    assert sorted(r["id"] for r in back if r.get("id")) == [
        "wg_cont",
        "wg_levi",
        "wg_npc",
        "wg_scpt",
        "wg_stat",
    ]


def test_a_copied_script_names_itself(tmp_path):
    from wraithguard.patch.editor import Found

    s = _session(tmp_path)
    src = Found(
        "SCPT",
        "Script",
        "payme",
        (
            (
                "Tamriel_Data.esm",
                {"type": "Script", "id": "payme", "text": "; hi\nBegin payme\nend payme"},
            ),
        ),
    )
    copy = s.duplicate(src, "payyou")
    assert copy.record["text"] == "; hi\nBegin payyou\nend payme"
