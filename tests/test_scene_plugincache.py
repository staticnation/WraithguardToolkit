"""Tests for the filtered plugin parse and the cross-preview/-launch cache.

Real plugin bytes are built with :func:`write_plugin`, so the filter and the
cache are exercised against genuine records, not stand-ins.
"""

from __future__ import annotations

import pickle
from typing import TYPE_CHECKING

from wraithguard.esp import write_plugin
from wraithguard.esp.flags import CellFlags, ObjectFlags
from wraithguard.esp.plugin import read_plugin_filtered
from wraithguard.esp.record import UnknownRecord
from wraithguard.esp.records.cell import Cell, CellData
from wraithguard.esp.records.header import Header
from wraithguard.esp.records.static_ import Static
from wraithguard.scene import PREVIEW_RECORD_TAGS, LoadedPlugin, PluginParseCache, plugincache

if TYPE_CHECKING:
    from pathlib import Path

    import pytest


def _plugin_bytes(static_id: str = "rock") -> bytes:
    """A plugin with a header, one Static, one Cell, and a non-kept record."""
    return write_plugin(
        [
            Header(masters=[("Morrowind.esm", 999)]),
            Static(id=static_id, mesh="r.nif"),
            Cell(name="Test", data=CellData(cell_flags=CellFlags.IS_INTERIOR)),
            UnknownRecord(b"ZZZZ", ObjectFlags(0), b""),  # not in any keep set
        ]
    )


class TestReadPluginFiltered:
    def test_it_keeps_only_the_named_tags(self) -> None:
        records = read_plugin_filtered(_plugin_bytes(), frozenset({b"STAT", b"CELL"}))
        kinds = {type(r).__name__ for r in records}
        assert kinds == {"Static", "Cell"}  # header (TES3) and ZZZZ skipped

    def test_an_empty_keep_set_returns_nothing(self) -> None:
        assert read_plugin_filtered(_plugin_bytes(), frozenset()) == []

    def test_a_kept_tag_with_no_class_comes_back_as_an_unknown_record(self) -> None:
        # Keeping a tag no record class handles still yields it, verbatim.
        records = read_plugin_filtered(_plugin_bytes(), frozenset({b"ZZZZ"}))
        assert len(records) == 1
        assert isinstance(records[0], UnknownRecord)
        assert records[0].wire_tag == b"ZZZZ"

    def test_the_preview_tag_set_covers_cells_terrain_objects_and_actors(self) -> None:
        for tag in (b"CELL", b"LAND", b"LTEX", b"STAT", b"NPC_", b"CREA"):
            assert tag in PREVIEW_RECORD_TAGS

    def test_the_preview_filter_keeps_the_cell_and_the_static(self) -> None:
        records = read_plugin_filtered(_plugin_bytes(), PREVIEW_RECORD_TAGS)
        assert any(isinstance(r, Cell) for r in records)
        assert any(isinstance(r, Static) for r in records)
        assert all(not isinstance(r, UnknownRecord) for r in records)


class TestPluginParseCacheMemory:
    def test_it_parses_records_and_masters(self, tmp_path: Path) -> None:
        file = tmp_path / "A.esp"
        file.write_bytes(_plugin_bytes())
        loaded = PluginParseCache().load("A.esp", file)
        assert isinstance(loaded, LoadedPlugin)
        assert loaded.name == "A.esp"
        assert loaded.masters == ["Morrowind.esm"]
        assert any(isinstance(r, Cell) for r in loaded.records)

    def test_an_unchanged_file_returns_the_same_cached_object(self, tmp_path: Path) -> None:
        file = tmp_path / "A.esp"
        file.write_bytes(_plugin_bytes())
        cache = PluginParseCache()
        first = cache.load("A.esp", file)
        assert cache.load("A.esp", file) is first  # no re-parse

    def test_a_changed_file_is_reparsed(self, tmp_path: Path) -> None:
        file = tmp_path / "A.esp"
        file.write_bytes(_plugin_bytes("rock"))
        cache = PluginParseCache()
        first = cache.load("A.esp", file)
        file.write_bytes(_plugin_bytes("a_much_longer_id_so_the_size_differs"))
        second = cache.load("A.esp", file)
        assert second is not first  # the size (in the key) changed -> re-read

    def test_without_a_cache_dir_no_sidecar_is_written(self, tmp_path: Path) -> None:
        file = tmp_path / "A.esp"
        file.write_bytes(_plugin_bytes())
        PluginParseCache(cache_dir=None).load("A.esp", file)
        assert list(tmp_path.glob("*.plc")) == []


class TestPluginParseCacheSidecar:
    def test_a_second_cache_reads_the_sidecar_without_reparsing(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        source = tmp_path / "A.esp"
        source.write_bytes(_plugin_bytes())
        cache_dir = tmp_path / "cache"
        first = PluginParseCache(cache_dir=cache_dir).load("A.esp", source)
        assert list(cache_dir.glob("*.plc"))  # a sidecar was written

        # A fresh cache must not touch the parser: make it raise if it tries.
        def _boom(_data: bytes, _keep: object) -> list[object]:
            raise AssertionError("parsed instead of reading the sidecar")

        monkeypatch.setattr(plugincache, "read_plugin_filtered", _boom)
        second = PluginParseCache(cache_dir=cache_dir).load("A.esp", source)
        assert [type(r).__name__ for r in second.records] == [
            type(r).__name__ for r in first.records
        ]
        assert second.masters == first.masters

    def test_a_corrupt_sidecar_falls_back_to_parsing(self, tmp_path: Path) -> None:
        source = tmp_path / "A.esp"
        source.write_bytes(_plugin_bytes())
        cache_dir = tmp_path / "cache"
        PluginParseCache(cache_dir=cache_dir).load("A.esp", source)
        for sidecar in cache_dir.glob("*.plc"):
            sidecar.write_bytes(b"not a pickle")  # corrupt it
        # A fresh cache treats the corrupt sidecar as a miss and re-parses cleanly.
        loaded = PluginParseCache(cache_dir=cache_dir).load("A.esp", source)
        assert any(isinstance(r, Cell) for r in loaded.records)

    def test_a_stale_sidecar_version_is_ignored(self, tmp_path: Path) -> None:
        source = tmp_path / "A.esp"
        source.write_bytes(_plugin_bytes())
        cache_dir = tmp_path / "cache"
        cache = PluginParseCache(cache_dir=cache_dir)
        cache.load("A.esp", source)
        stat = source.stat()
        # Overwrite the sidecar with a record from an older format version.
        (sidecar,) = cache_dir.glob("*.plc")
        sidecar.write_bytes(
            pickle.dumps(
                {
                    "v": -1,
                    "size": stat.st_size,
                    "mtime": stat.st_mtime_ns,
                    "masters": [],
                    "records": [],
                }
            )
        )
        loaded = PluginParseCache(cache_dir=cache_dir).load("A.esp", source)
        assert any(isinstance(r, Cell) for r in loaded.records)  # re-parsed, not the empty record

    def test_a_cache_dir_that_cannot_be_created_is_survived(self, tmp_path: Path) -> None:
        # cache_dir is an existing *file*, so mkdir fails -- the parse must still
        # succeed, just without a sidecar to show for it.
        source = tmp_path / "A.esp"
        source.write_bytes(_plugin_bytes())
        blocker = tmp_path / "blocker"
        blocker.write_text("i am a file, not a directory")
        loaded = PluginParseCache(cache_dir=blocker).load("A.esp", source)
        assert any(isinstance(r, Cell) for r in loaded.records)
