"""wraithguard.patch.refedit: changed references, written as the engine merges cells."""

from __future__ import annotations

from typing import Any

import pytest

from wraithguard.patch.records import PatchError
from wraithguard.patch.refedit import RefEdit, cell_patch_record, next_new_index, winning_reference


def _header(*masters: str) -> dict[str, Any]:
    return {"type": "Header", "masters": [[m, 1] for m in masters]}


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


# Tamriel_Data's reference 15 is (1, 15) in A (one master) and (2, 15) in B (whose
# first master is OAAB): the same object - merge_to_master's own example.
PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Tamriel_Data.esm": [
        _header(),
        {
            "type": "Cell",
            "flags": "",
            "id": "Ebon Tower",
            "name": "Ebon Tower",
            "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
            "references": [_ref(0, 15, "iron dagger", 1.0), _ref(0, 16, "chair", 5.0)],
        },
    ],
    "OAAB.esm": [_header()],
    "A.esp": [
        _header("Tamriel_Data.esm"),
        {
            "type": "Cell",
            "flags": "",
            "id": "Ebon Tower",
            "name": "Ebon Tower",
            "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
            "references": [_ref(1, 15, "iron dagger", 2.0)],
        },
    ],
    "B.esp": [
        _header("OAAB.esm", "Tamriel_Data.esm"),
        {
            "type": "Cell",
            "flags": "",
            "id": "Ebon Tower",
            "name": "Ebon Tower",
            "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
            "water_height": 3.0,
            "references": [_ref(2, 15, "iron dagger", 3.0, scale=2.0), _ref(0, 1, "torch", 9.0)],
        },
    ],
}
ORDER = ["Tamriel_Data.esm", "OAAB.esm", "A.esp", "B.esp"]


def test_the_winning_reference_follows_each_plugins_own_master_list():
    versions = [
        (p, next(r for r in PLUGINS[p] if r["type"] == "Cell"))
        for p in ("Tamriel_Data.esm", "A.esp", "B.esp")
    ]
    masters = {
        "Tamriel_Data.esm": [],
        "A.esp": ["Tamriel_Data.esm"],
        "B.esp": ["OAAB.esm", "Tamriel_Data.esm"],
    }
    plugin, ref = winning_reference(versions, masters, "Tamriel_Data.esm", 15)
    assert plugin == "B.esp" and ref["translation"][0] == 3.0
    plugin, ref = winning_reference(versions, masters, "Tamriel_Data.esm", 16)
    assert plugin == "Tamriel_Data.esm"
    assert winning_reference(versions, masters, "B.esp", 1)[1]["id"] == "torch"
    assert winning_reference(versions, masters, "A.esp", 15) is None  # A created nothing


def test_the_patch_cell_carries_only_the_changed_references():
    masters = ["Tamriel_Data.esm", "OAAB.esm", "B.esp"]
    rec = cell_patch_record(
        [
            RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"translation": [7.0, 0.0, 0.0]}),
            RefEdit("ebon tower", "B.esp", 1, {"deleted": True}),
        ],
        PLUGINS,
        ORDER,
        masters,
    )
    assert rec["water_height"] == 3.0 and rec["name"] == "Ebon Tower"  # the winner's own fields
    dagger, torch = rec["references"]
    # Keyed for the patch: Tamriel_Data is its first master, B its third.
    assert (dagger["mast_index"], dagger["refr_index"]) == (1, 15)
    assert (
        dagger["translation"] == [7.0, 0.0, 0.0] and dagger["scale"] == 2.0
    )  # B's version, changed
    assert (torch["mast_index"], torch["refr_index"], torch["deleted"]) == (3, 1, True)
    assert len(rec["references"]) == 2  # not the chair: it is untouched


def test_refusals():
    with pytest.raises(PatchError, match="not among the patch's masters"):
        cell_patch_record(
            [RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {})], PLUGINS, ORDER, ["B.esp"]
        )
    with pytest.raises(PatchError, match="no reference"):
        cell_patch_record(
            [RefEdit("Ebon Tower", "Tamriel_Data.esm", 99, {})],
            PLUGINS,
            ORDER,
            ["Tamriel_Data.esm"],
        )
    with pytest.raises(PatchError, match="cannot be changed"):
        cell_patch_record(
            [RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"refr_index": 3})],
            PLUGINS,
            ORDER,
            ["Tamriel_Data.esm"],
        )
    with pytest.raises(PatchError, match="one cell"):
        cell_patch_record(
            [RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {}), RefEdit("Other", "x", 1, {})],
            PLUGINS,
            ORDER,
            ["Tamriel_Data.esm"],
        )


def test_next_new_index():
    assert next_new_index([]) == 1
    assert next_new_index(PLUGINS["B.esp"]) == 2
    assert next_new_index(PLUGINS["Tamriel_Data.esm"]) == 17


def test_it_writes_and_reads_back_as_a_plugin(tmp_path):
    """Through the Rust backend: the cell, the one reference, its key and its change."""
    native = pytest.importorskip("wraithguard_native")
    from wraithguard.land.emit import build_plugin

    masters = ["Tamriel_Data.esm", "OAAB.esm", "B.esp"]
    rec = cell_patch_record(
        [RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"rotation": [0.0, 0.0, 1.5]})],
        PLUGINS,
        ORDER,
        masters,
    )
    import json

    doc = build_plugin([rec], [(m, 100) for m in masters], description="test")
    data = native.plugin_records_bytes(json.dumps(doc))
    back = native.plugin_records(data)
    assert [m[0] for m in back[0]["masters"]] == masters
    (cell,) = [r for r in back if r["type"] == "Cell"]
    (ref,) = cell["references"]
    assert (ref["mast_index"], ref["refr_index"], ref["id"]) == (1, 15, "iron dagger")
    assert ref["rotation"][2] == pytest.approx(1.5) and ref["scale"] == pytest.approx(2.0)


def test_merge_to_master_applies_the_patch_as_the_game_would(tmp_path):
    """greatness7's merge_to_master (embedded in the backend) merges the load order with
    the patch last: the changed reference changes, nothing else in the cell does."""
    native = pytest.importorskip("wraithguard_native")
    import json

    from wraithguard.patch.records import master_names

    def write(name: str, records: list[dict[str, Any]], masters: list[str]) -> None:
        header = {
            "type": "Header",
            "flags": "",
            "version": 1.3,
            "author": "",
            "description": "",
            "file_type": "Esm" if name.endswith(".esm") else "Esp",
            "num_objects": len(records),
            "masters": [[m, (tmp_path / m).stat().st_size] for m in masters],
        }
        (tmp_path / name).write_bytes(native.plugin_records_bytes(json.dumps([header, *records])))

    for name in ORDER:
        recs = PLUGINS[name]
        write(name, [r for r in recs if r["type"] != "Header"], master_names(recs))
    masters = ["Tamriel_Data.esm", "OAAB.esm", "B.esp"]
    rec = cell_patch_record(
        [RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"translation": [42.0, 0.0, 0.0]})],
        PLUGINS,
        ORDER,
        masters,
    )
    write("Patch.esp", [rec], masters)
    merged = native.plugin_records(
        native.merge_load_order([str(tmp_path / n) for n in [*ORDER, "Patch.esp"]])
    )
    (cell,) = [r for r in merged if r["type"] == "Cell"]
    by_id = {r["id"]: r for r in cell["references"]}
    assert set(by_id) == {"iron dagger", "chair", "torch"}  # one dagger: the same key everywhere
    assert by_id["iron dagger"]["translation"][0] == pytest.approx(42.0)  # the patch's change
    assert by_id["iron dagger"]["scale"] == pytest.approx(2.0)  # B's scale, kept
    assert by_id["chair"]["translation"][0] == pytest.approx(5.0)  # untouched
    assert by_id["torch"]["translation"][0] == pytest.approx(9.0)  # B's own, untouched
    assert cell.get("water_height") == pytest.approx(3.0)  # the cell's own fields, unchanged


# -- cell keys: interiors by name, exteriors by grid ------------------------------------

_CELLS = [
    {
        "type": "Cell",
        "flags": "",
        "name": "Lamptown, Cellar",
        "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
        "references": [],
    },
    {
        "type": "Cell",
        "flags": "",
        "name": "Lamptown, Attic",
        "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
        "references": [],
    },
    {
        "type": "Cell",
        "flags": "",
        "name": "Coast Watch",
        "data": {"flags": "", "grid": [0, 0]},
        "references": [],
    },
    {
        "type": "Cell",
        "flags": "",
        "name": "Seyda Neen",
        "data": {"flags": "", "grid": [-2, -9]},
        "references": [],
    },
    {
        "type": "Cell",
        "flags": "",
        "name": "Seyda Neen",
        "data": {"flags": "", "grid": [-3, -9]},
        "references": [],
    },
    {
        "type": "Cell",
        "flags": "",
        "name": "",
        "data": {"flags": "", "grid": [5, 5]},
        "references": [],
    },
]


def test_cells_are_keyed_as_the_engine_keys_them():
    from wraithguard.patch.records import record_key

    assert [record_key(c) for c in _CELLS] == [
        "Lamptown, Cellar",
        "Lamptown, Attic",
        "(0, 0)",
        "(-2, -9)",
        "(-3, -9)",
        "(5, 5)",
    ]


def test_find_record_takes_a_conflict_name_but_refuses_an_ambiguous_one():
    from wraithguard.patch.records import find_record

    assert find_record(_CELLS, "Cell", "Lamptown, Attic", "x.esp")["name"] == "Lamptown, Attic"
    assert find_record(_CELLS, "Cell", "(0, 0)", "x.esp")["name"] == "Coast Watch"  # not a room
    assert find_record(_CELLS, "Cell", "Coast Watch", "x.esp")["data"]["grid"] == [0, 0]
    assert find_record(_CELLS, "Cell", "(-3, -9)", "x.esp")["data"]["grid"] == [-3, -9]
    with pytest.raises(PatchError, match=r"\(-2, -9\), \(-3, -9\)"):
        find_record(_CELLS, "Cell", "Seyda Neen", "x.esp")
    assert find_record(_CELLS, "Cell", "Nowhere", "x.esp") is None


def test_a_whole_interior_cell_is_carried_not_the_first_room():
    from wraithguard.patch.records import Selection, collect

    rooms = {"Mod.esp": [{"type": "Header", "masters": []}, *_CELLS]}
    (got,) = collect(
        [Selection(plugin="Mod.esp", record_type="Cell", key="Lamptown, Attic")], rooms, ["Mod.esp"]
    )
    assert got["name"] == "Lamptown, Attic"


# -- through the pool and the patch writer ---------------------------------------------


def test_the_pool_keeps_journals_and_counts_reference_edits(tmp_path):
    from wraithguard.patch.editor import restore_queue, save_queue
    from wraithguard.patch.queue import PatchQueue

    q = PatchQueue()
    q.add_ref_edit(
        RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"translation": [1.0, 2.0, 3.0]}, ("A.esp",))
    )
    q.add_ref_edit(RefEdit("ebon tower", "tamriel_data.esm", 15, {"scale": 1.5}, ("B.esp",)))
    (e,) = q.ref_edits
    assert e.changes == {"translation": [1.0, 2.0, 3.0], "scale": 1.5} and e.plugins == (
        "A.esp",
        "B.esp",
    )
    q.add_ref_edit(RefEdit("Ebon Tower", "B.esp", 1, {"deleted": True}))
    assert len(q) == 1  # one cell
    j = tmp_path / "j.json"
    save_queue(q, j)
    again = PatchQueue()
    assert restore_queue(again, j) == 2
    assert {x.ident for x in again.ref_edits} == {x.ident for x in q.ref_edits}
    again.remove_ref_edit("EBON TOWER", "Tamriel_Data.esm", 15, "scale")
    assert next(x for x in again.ref_edits if x.refr_index == 15).changes == {
        "translation": [1.0, 2.0, 3.0]
    }
    again.remove_ref_edit("Ebon Tower", "B.esp", 1)
    again.remove_ref_edit("Ebon Tower", "Tamriel_Data.esm", 15, "translation")
    assert not again.ref_edits and not len(again)


def test_build_record_patch_writes_the_changed_references(tmp_path):
    pytest.importorskip("wraithguard_native")
    from wraithguard.patch.service import build_record_patch

    edits = [
        RefEdit(
            "Ebon Tower",
            "Tamriel_Data.esm",
            16,
            {"translation": [6.0, 0.0, 0.0]},
            ("Tamriel_Data.esm", "A.esp", "B.esp"),
        )
    ]
    out = tmp_path / "Patch.esp"
    sizes = dict.fromkeys(ORDER, 1000)
    result = build_record_patch([], PLUGINS, ORDER, sizes, "", out, ref_edits=edits)
    assert result.masters == ["Tamriel_Data.esm"]
    import wraithguard_native

    back = wraithguard_native.plugin_records(out.read_bytes())
    (cell,) = [r for r in back if r["type"] == "Cell"]
    assert [(r["mast_index"], r["refr_index"], r["id"]) for r in cell["references"]] == [
        (1, 16, "chair")
    ]
    # The same cell queued whole as well: the change goes into that record.
    from wraithguard.patch.records import Selection

    result = build_record_patch(
        [Selection(plugin="B.esp", record_type="Cell", key="Ebon Tower")],
        PLUGINS,
        ORDER,
        sizes,
        "",
        out,
        ref_edits=edits,
    )
    back = wraithguard_native.plugin_records(out.read_bytes())
    (cell,) = [r for r in back if r["type"] == "Cell"]
    got = {r["id"]: r for r in cell["references"]}
    assert set(got) == {"iron dagger", "torch", "chair"}
    assert got["chair"]["translation"][0] == pytest.approx(6.0)


def test_a_reference_can_place_another_object():
    """Search & Replace: the same key, another object - the engine merges it so."""
    native = pytest.importorskip("wraithguard_native")
    import json

    from wraithguard.land.emit import build_plugin

    masters = ["Tamriel_Data.esm"]
    rec = cell_patch_record(
        [RefEdit("Ebon Tower", "Tamriel_Data.esm", 16, {"id": "T_Chair02"})],
        PLUGINS,
        ORDER,
        masters,
    )
    doc = build_plugin([rec], [(m, 100) for m in masters], description="test")
    (cell,) = [
        r
        for r in native.plugin_records(native.plugin_records_bytes(json.dumps(doc)))
        if r["type"] == "Cell"
    ]
    assert [(r["mast_index"], r["refr_index"], r["id"]) for r in cell["references"]] == [
        (1, 16, "T_Chair02")
    ]
