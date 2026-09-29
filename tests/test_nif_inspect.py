"""The mesh viewer's block panel and field editor (wraithguard.nif.inspect / .edit)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from wraithguard.nif.edit import NifEditError, apply_edits
from wraithguard.nif.inspect import inspect_mesh

ANIM = Path(__file__).parent / "fixtures" / "gardenfell_anim" / "anim.nif"


def _field(panel: dict, block: int, name: str) -> dict:
    return next(f for f in panel["blocks"][str(block)]["fields"] if f["name"] == name)


def _block_of(panel: dict, type_name: str) -> int:
    return next(int(k) for k, b in panel["blocks"].items() if b["type"] == type_name)


def test_inspect_gives_tree_and_fields_as_json() -> None:
    out = inspect_mesh(ANIM.read_bytes())
    assert out is not None
    json.dumps(out)  # what goes into the viewer's extra file
    assert out["complete"] is True
    assert out["tree"][0]["index"] == 0
    assert out["tree"][0]["type"] == "NiNode"
    blocks = out["blocks"]
    assert set(blocks) == {str(i) for i in range(len(blocks))}
    root = _field(out, 0, "name")
    assert root["editable"] is True and root["value"] == "Anim"
    # Links read as the block index they point at.
    assert _field(out, 0, "children[0]")["kind"] == "link"


def test_every_block_is_in_the_tree_once() -> None:
    out = inspect_mesh(ANIM.read_bytes())
    assert out is not None
    seen: list[int] = []

    def walk(nodes: list) -> None:
        for n in nodes:
            seen.append(n["index"])
            walk(n["children"])

    walk(out["tree"])
    assert sorted(seen) == list(range(len(out["blocks"])))


def test_edits_from_the_panel_apply() -> None:
    data = ANIM.read_bytes()
    out = inspect_mesh(data)
    assert out is not None
    tex = _block_of(out, "NiSourceTexture")
    edited = apply_edits(
        data,
        [
            {"op": "set_field", "block": 0, "name": "name", "value": "Renamed"},
            {"op": "set_field", "block": 0, "name": "translation", "value": "1, 2, 3.5"},
            {"op": "set_field", "block": 0, "name": "scale", "value": 0.5},
            {"op": "set_field", "block": tex, "name": "source", "value": "textures\\new.dds"},
        ],
    )
    after = inspect_mesh(edited)
    assert after is not None
    assert _field(after, 0, "name")["value"] == "Renamed"
    assert _field(after, 0, "translation")["value"] == "1.0, 2.0, 3.5"
    assert _field(after, 0, "scale")["value"] == 0.5
    assert (
        _field(after, _block_of(after, "NiSourceTexture"), "source")["value"] == "textures\\new.dds"
    )


def test_a_field_the_editor_cannot_set_is_refused() -> None:
    data = ANIM.read_bytes()
    with pytest.raises(NifEditError):
        apply_edits(
            data, [{"op": "set_field", "block": 0, "name": "rotation.x_axis", "value": "1,0,0"}]
        )
    with pytest.raises(NifEditError):
        apply_edits(data, [{"op": "set_field", "block": 999, "name": "name", "value": "x"}])
    with pytest.raises(NifEditError):
        apply_edits(data, [{"op": "set_field", "block": 0, "name": "scale", "value": "big"}])


def test_garbage_is_none() -> None:
    assert inspect_mesh(b"not a nif") is None
