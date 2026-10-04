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
            [RefEdit("Ebon Tower", "Tamriel_Data.esm", 15, {"id": "x"})],
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
