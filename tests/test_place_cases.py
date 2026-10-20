"""Placed objects, new references, path grids, and the cells and land the patch makes:
the shared place cases (``tests/fixtures/place_cases.json``, which
``viewer-shell/editcore/tests/cases.rs`` runs through ``editcore::places`` too) through
:class:`~wraithguard.patch.editor.EditorSession`."""

from __future__ import annotations

import base64
import hashlib
import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.patch.editor import TAG_TO_TYPE, EditorError, EditorSession, queue_journal
from wraithguard.patch.newland import flat_land
from wraithguard.patch.queue import PatchQueue

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "place_cases.json").read_text(encoding="utf-8")
)["cases"]

#: A landscape's packed grids: base64 of the bytes, zstd'd where Python has a zstd
#: backend; the cases hold a digest of the bytes instead.
LAND_GRIDS = (
    "vertex_normals",
    "vertex_heights",
    "world_map_data",
    "vertex_colors",
    "texture_indices",
)


def _bytes(text: str) -> bytes:
    """A packed field's bytes."""
    raw = base64.b64decode(text)
    if raw.startswith(b"\x28\xb5\x2f\xfd"):
        from compression import zstd  # type: ignore[import-not-found,unused-ignore]  # 3.14

        raw = zstd.decompress(raw)
    return raw


def packed(record: dict[str, Any]) -> dict[str, Any]:
    """A record with each packed grid of a landscape as ``sha256:<digest>`` of its bytes."""
    if record.get("type") == "Landscape":
        for name in LAND_GRIDS:
            grid = record.get(name)
            if isinstance(grid, dict) and isinstance(grid.get("data"), str):
                grid["data"] = "sha256:" + hashlib.sha256(_bytes(grid["data"])).hexdigest()
    return record


def _relabel(value: Any, labels: dict[str, str]) -> Any:
    """``value`` with every new reference's editor name as its label."""
    if isinstance(value, str):
        return labels.get(value, value)
    if isinstance(value, list):
        return [_relabel(v, labels) for v in value]
    if isinstance(value, dict):
        return {k: _relabel(v, labels) for k, v in value.items()}
    return value


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """Apply a case's operations; what each gave, and the journal after them."""
    plugins = case["plugins"]
    by_plugin = {p["name"]: p for p in plugins}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        p = by_plugin[path.name]
        if tag == "TES3":
            return [{"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}]
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in p["records"] if r["type"] == want]

    s = EditorSession([(p["name"], tmp / p["name"]) for p in plugins], PatchQueue(), read=read)
    labels: dict[str, str] = {}
    uids: dict[str, str] = {}

    def label_new() -> None:
        """Give each new reference not yet seen the next label."""
        for n in s.queue.new_refs:
            if n.uid not in labels:
                labels[n.uid] = f"U{len(labels) + 1}"
                uids[labels[n.uid]] = n.uid

    def new_ref(label: str) -> Any:
        """The new reference a label names."""
        found = s.find_new("", uids[label])
        assert found is not None, label
        return found

    def ref(a: list[Any]) -> Any:
        """The placed object ``[cell, origin, index]`` names."""
        found = s.find_ref(a[0], a[1], a[2])
        assert found is not None, a
        return found

    results: list[Any] = []
    for op, *a in case["ops"]:
        try:
            if op == "ref_view":
                found = s.find_ref(a[0], a[1], a[2])
                out: Any = None if found is None else s.ref_view(found)
            elif op == "set_ref":
                s.set_ref_field(ref(a), a[3], a[4])
                out = "ok"
            elif op == "revert_ref":
                s.revert_ref(ref(a), a[3])
                out = "ok"
            elif op == "place":
                record = s.find(a[1], a[2])
                assert record is not None
                new = s.place(a[0], record, a[3], a[4])
                out = new.uid
            elif op == "place_many":
                items = []
                for tag, rid, translation, rotation, scale in a[1]:
                    record = s.find(tag, rid)
                    assert record is not None
                    items.append((record, translation, rotation, scale))
                out = [n.uid for n in s.place_many(a[0], items)]
            elif op == "new_view":
                out = s.new_view(new_ref(a[0]))
            elif op == "set_new":
                out = s.set_new_field(new_ref(a[0]), a[1], a[2]).cell
            elif op == "remove_new":
                s.remove_new(new_ref(a[0]))
                out = "ok"
            elif op == "new_cell":
                out = s.new_cell(a[0], absent=a[1])
            elif op == "new_land":
                out = s.new_land(a[0], height=a[1], texture=a[2], no_land=a[3], has_cell=a[4])
            elif op == "duplicate_cell":
                out = s.duplicate_cell(a[0], a[1])
            elif op == "pathgrid_view":
                out = s.pathgrid_view(a[0])
            elif op == "set_pathgrid":
                out = s.set_pathgrid(a[0], a[1], a[2])
            elif op == "revert_pathgrid":
                out = s.revert_pathgrid(a[0])
            elif op == "flat_land":
                out = packed(flat_land(a[0], a[1], a[2]))
            elif op == "pending":
                out = s.pending()
            elif op == "undo":
                out = s.undo()
            else:
                raise AssertionError(f"unknown operation {op}")
        except EditorError as exc:
            out = {"error": str(exc)}
        label_new()
        results.append(out)
    journal = queue_journal(s.queue)
    for made in journal["made"]:
        packed(made["record"])
    return _relabel({"results": results, "journal": journal}, labels)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
