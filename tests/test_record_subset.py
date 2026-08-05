"""Tes3ConvSession.record_subset: stream a plugin, keep only the wanted records.

The point of the method is memory -- it must return *exactly* what
``record_map`` would for the same keys, including the interior/exterior keying
of id-less cell-scoped records (path grids), and it must do so whether or not
the optional ``ijson`` streaming parser is installed. These tests pin both: the
streamed result equals the full-parse result, and the no-ijson fallback equals
the streamed one.

The session works on already-dumped JSON, so no tes3conv binary is needed: a
JSON file written into the dump dir is read back exactly as a converted plugin
would be.
"""

from __future__ import annotations

import json
import sys
from typing import TYPE_CHECKING

import pytest

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path
    from typing import Any

# A plugin with one of every keying case the method has to get right:
# a normal id record, an interior cell, that interior's id-less path grid
# (keyed by cell name only because the cell is interior), and an exterior cell
# (keyed by its coordinates).
RECORDS: list[dict[str, Any]] = [
    {"type": "TES3", "author": "header, keyed by nothing"},
    {"type": "Npc", "id": "bob", "level": 3},
    {"type": "Armor", "id": "cuirass", "weight": 30.5},
    {"type": "Cell", "id": "MyInterior", "data": {"flags": 1}},
    {"type": "PathGrid", "cell": "MyInterior", "data": {"grid": [0, 0]}, "points": [1, 2, 3]},
    {"type": "Cell", "data": {"flags": 0, "grid": [5, 6]}, "name": ""},
]


def _session(tmp_path: Path) -> tuple[core.Tes3ConvSession, str]:
    """A session whose dump dir already holds RECORDS as one plugin's JSON."""
    session = core.Tes3ConvSession(exe="unused", dump_dir=str(tmp_path), keep=True)
    (tmp_path / "Plugin.json").write_text(json.dumps(RECORDS), encoding="utf-8")
    return session, "Plugin.esp"


def _reference(session: core.Tes3ConvSession, path: str) -> dict[tuple[str, str], Any]:
    """What the whole-file reader returns -- the answer record_subset must match."""
    return session.record_map(path)


class TestItMatchesTheWholeFileReader:
    def test_a_normal_subset_is_identical(self, tmp_path: Path) -> None:
        session, path = _session(tmp_path)
        want = {("Npc", "bob"), ("Armor", "cuirass")}
        got = session.record_subset(path, want)
        ref = {k: v for k, v in _reference(session, path).items() if k in want}
        assert got == ref

    def test_interior_path_grid_is_keyed_and_returned(self, tmp_path: Path) -> None:
        # The case the sidecar interior lookup exists for: an id-less path grid
        # keyed by its interior cell's name, not by "(0, 0)".
        session, path = _session(tmp_path)
        want = {("PathGrid", "MyInterior")}
        got = session.record_subset(path, want)
        assert set(got) == want
        assert got[("PathGrid", "MyInterior")]["points"] == [1, 2, 3]

    def test_exterior_cell_is_keyed_by_coords(self, tmp_path: Path) -> None:
        session, path = _session(tmp_path)
        want = {("Cell", "(5, 6)")}
        got = session.record_subset(path, want)
        assert set(got) == want

    def test_every_key_at_once_matches_the_full_map(self, tmp_path: Path) -> None:
        session, path = _session(tmp_path)
        ref = _reference(session, path)
        got = session.record_subset(path, set(ref))
        assert got == ref


class TestEdges:
    def test_no_wanted_keys_reads_nothing(self, tmp_path: Path) -> None:
        session, path = _session(tmp_path)
        assert session.record_subset(path, set()) == {}

    def test_an_absent_key_is_simply_missing(self, tmp_path: Path) -> None:
        session, path = _session(tmp_path)
        got = session.record_subset(path, {("Npc", "nobody"), ("Npc", "bob")})
        assert set(got) == {("Npc", "bob")}

    def test_numbers_decode_as_json_would_not_decimal(self, tmp_path: Path) -> None:
        # use_float=True: a float stays a float, not a Decimal, so the value
        # compares and repr()s the same as every other read path.
        session, path = _session(tmp_path)
        got = session.record_subset(path, {("Armor", "cuirass")})
        weight = got[("Armor", "cuirass")]["weight"]
        assert isinstance(weight, float)
        assert weight == 30.5


class TestFallbackWithoutIjson:
    def test_without_ijson_it_still_matches(self, tmp_path: Path, monkeypatch: Any) -> None:
        # Make `import ijson` raise, forcing the whole-file fallback, and prove
        # it returns the same records as the streaming path.
        monkeypatch.setitem(sys.modules, "ijson", None)
        session, path = _session(tmp_path)
        want = {("Npc", "bob"), ("PathGrid", "MyInterior"), ("Cell", "(5, 6)")}
        got = session.record_subset(path, want)
        ref = {k: v for k, v in _reference(session, path).items() if k in want}
        assert got == ref


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
