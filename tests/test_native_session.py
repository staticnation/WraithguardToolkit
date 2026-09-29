"""The native conflict session (:class:`wraithguard_toolkit.NativeEspSession`).

The session the conflict scan, cell map, field diff and Merged Lands read through:
the Rust backend hands each plugin's records to Python as tes3conv-schema dicts, with
no JSON written or parsed and no sidecar files. These build a plugin with the esp
*writer*, read it back through the session, and check each surface the callers
depend on.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import wraithguard_toolkit as core
from wraithguard.esp import (
    Cell,
    CellData,
    GameSetting,
    Header,
    Landscape,
    Weapon,
    write_plugin,
)
from wraithguard.esp.flags import CellFlags

if TYPE_CHECKING:
    from pathlib import Path

    import pytest


def _plugin(path: Path) -> str:
    """Write a small varied plugin with the esp writer and return its path.

    Args:
        path: The file to write.

    Returns:
        The path as a string.
    """
    records = [
        Header(),
        Weapon(id="the_sword", name="The Sword"),
        GameSetting(id="fJumpBase", value=1.5),
        Cell(data=CellData(cell_flags=CellFlags.HAS_WATER)),
        Landscape(grid=(3, 4)),
    ]
    path.write_bytes(write_plugin(records))
    return str(path)


class TestNativeSession:
    """Reading a plugin back through the native session, without tes3conv."""

    def test_engine_is_named_native(self, tmp_path: Path) -> None:
        """The engine label distinguishes it from tes3conv in scan output."""
        session = core.NativeEspSession(dump_dir=str(tmp_path), keep=True)
        assert session.engine_name == "native"

    def test_records_include_the_header_and_content(self, tmp_path: Path) -> None:
        """``records`` yields every record as a tes3conv-shaped dict."""
        session = core.NativeEspSession(dump_dir=str(tmp_path), keep=True)
        records = session.records(_plugin(tmp_path / "mine.esp"))
        types = [r.get("type") for r in records]
        assert types[0] == "Header"
        assert {"Weapon", "GameSetting", "Cell", "Landscape"} <= set(types)

    def test_record_map_keys_by_type_and_id(self, tmp_path: Path) -> None:
        """``record_map`` is what field-level diffing indexes by."""
        session = core.NativeEspSession(dump_dir=str(tmp_path), keep=True)
        mapping = session.record_map(_plugin(tmp_path / "mine.esp"))
        assert ("Weapon", "the_sword") in mapping
        assert mapping[("Weapon", "the_sword")]["name"] == "The Sword"

    def test_record_keys_drive_conflict_detection(self, tmp_path: Path) -> None:
        """``record_keys`` is the compact list the conflict scan compares."""
        session = core.NativeEspSession(dump_dir=str(tmp_path), keep=True)
        keys = {(rtype, rid) for rtype, rid, *_ in session.record_keys(_plugin(tmp_path / "m.esp"))}
        assert ("GameSetting", "fJumpBase") in keys

    def test_landscape_records_are_available_for_merging(self, tmp_path: Path) -> None:
        """Merged Lands reads terrain through ``landscape_records``."""
        session = core.NativeEspSession(dump_dir=str(tmp_path), keep=True)
        land = session.landscape_records(_plugin(tmp_path / "mine.esp"))
        assert any(r.get("type") == "Landscape" for r in land)

    def test_nothing_is_written_to_disk(self, tmp_path: Path) -> None:
        """No JSON spool and no sidecars: the records never leave memory."""
        out = tmp_path / "spool"
        session = core.NativeEspSession(dump_dir=str(out), keep=True)
        path = _plugin(tmp_path / "mine.esp")
        session.records(path)
        session.record_keys(path)
        session.cells(path)
        session.landscape_records(path)
        assert list(out.iterdir()) == []

    def test_keys_are_read_once_while_the_plugin_is_unchanged(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Keys and cells come from memory until the plugin changes on disk."""
        session = core.NativeEspSession(dump_dir=str(tmp_path / "s"), keep=True)
        path = _plugin(tmp_path / "mine.esp")
        first = session.record_keys(path)
        calls: list[object] = []
        real = session._parse
        monkeypatch.setattr(session, "_parse", lambda *a, **k: calls.append(a) or real(*a, **k))
        assert session.record_keys(path) == first
        assert session.cells(path)
        assert calls == []

    def test_a_subset_parses_only_what_is_asked_for(self, tmp_path: Path) -> None:
        """``record_subset`` returns exactly the wanted records, keyed as the map keys them."""
        session = core.NativeEspSession(dump_dir=str(tmp_path / "s"), keep=True)
        path = _plugin(tmp_path / "mine.esp")
        got = session.record_subset(path, {("Weapon", "the_sword"), ("Weapon", "nope")})
        assert list(got) == [("Weapon", "the_sword")]
        assert got[("Weapon", "the_sword")] == session.record_map(path)[("Weapon", "the_sword")]

    def test_values_have_the_json_shapes(self, tmp_path: Path) -> None:
        """Lists, not tuples, and an f32 as its shortest decimal - as json.load gave."""
        session = core.NativeEspSession(dump_dir=str(tmp_path / "s"), keep=True)
        gmst = session.record_map(_plugin(tmp_path / "mine.esp"))[("GameSetting", "fJumpBase")]
        land = session.landscape_records(str(tmp_path / "mine.esp"))
        header = session.records(str(tmp_path / "mine.esp"))[0]
        assert gmst["value"] == {"type": "Float", "data": 1.5}
        assert header["version"] == 1.3  # the f32 1.3, not 1.2999999523162842
        assert isinstance(next(r for r in land if r["type"] == "Landscape")["grid"], list)

    def test_rust_keys_match_the_python_keys(self, tmp_path: Path) -> None:
        """``plugin_keys`` gives exactly what ``_keys_and_cells`` gives over the records."""
        import wraithguard_native

        path = _plugin(tmp_path / "mine.esp")
        data = (tmp_path / "mine.esp").read_bytes()
        keys, cells = core._keys_and_cells(wraithguard_native.plugin_records(data, None, True))
        rust_keys, rust_cells = wraithguard_native.plugin_keys(data)
        assert list(rust_keys) == [tuple(k) for k in keys]
        assert list(rust_cells) == [tuple(c) for c in cells]
        session = core.NativeEspSession(dump_dir=str(tmp_path / "s"), keep=True)
        assert session.record_keys(path) == list(rust_keys)

    def test_packed_arrays_arrive_as_raw_bytes(self, tmp_path: Path) -> None:
        """Landscape grids come unpacked - bytes, not zstd'd base64 - and still decode."""
        from wraithguard.tes3fields.landscape import decode_vertex_heights

        session = core.NativeEspSession(dump_dir=str(tmp_path / "s"), keep=True)
        land = session.landscape_records(_plugin(tmp_path / "mine.esp"))
        rec = next(r for r in land if r["type"] == "Landscape")
        heights = rec["vertex_heights"]["data"]
        assert isinstance(heights, bytes)
        assert len(heights) == 65 * 65
        assert len(decode_vertex_heights(heights)) == 65

    def test_an_unreadable_plugin_returns_no_records(self, tmp_path: Path) -> None:
        """A file that isn't a valid plugin at all fails read_plugin, not a crash."""
        bad = tmp_path / "corrupt.esp"
        bad.write_bytes(b"not a plugin")
        session = core.NativeEspSession(dump_dir=str(tmp_path), keep=True)

        assert session.records(str(bad)) == []
