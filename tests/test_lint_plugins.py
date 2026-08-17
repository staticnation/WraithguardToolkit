"""lint_plugins / _lint_one_plugin: the orchestration around the per-record
lint checks already pinned in test_lint_helpers.py.

What's new here is everything that only shows up across a full plugin or a
full load order: which plugins get skipped before ever being opened
(.omwscripts, vanilla masters, merge artifacts), the two load-order-wide
accumulators (interior_first / pathgrids) that only resolve into a
[NO PATHGRID] warning after every plugin has been seen, the is_custom gate
that keeps [HEADER] and [EXP-DEP] off curated files, and the EVLGMST/EXP-DEP
message assembly itself.
"""

from __future__ import annotations

from pathlib import Path

from conftest import interior_cell, rec, sub, write_plugin, zstr

import wraithguard_toolkit as core
from wraithguard.plugins import PluginFileIndex


def _index(data_dir: Path) -> PluginFileIndex:
    return PluginFileIndex([str(data_dir)])


def _gmst_record(name: str, tag: str, raw_value: bytes) -> bytes:
    return rec("GMST", sub("NAME", zstr(name)) + sub(tag, raw_value))


def _header_with_gaps(*, author: str = "", description: str = "") -> bytes:
    """A TES3 record whose header has a blank author and/or description --
    write_plugin always fills both in, so this bypasses it to build one with
    gaps directly."""
    import struct

    hedr = (
        struct.pack("<fi", 1.3, 0)
        + zstr(author, 32)
        + zstr(description, 256)
        + struct.pack("<i", 0)
    )
    return rec("TES3", sub("HEDR", hedr))


class TestSkippedBeforeEverOpened:
    def test_an_omwscripts_file_is_never_opened(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        # A file that would raise (garbage bytes) if it were ever read.
        (data_dir / "MyMod.omwscripts").write_bytes(b"not a valid tes3 file")
        order = ["MyMod.omwscripts"]

        warnings, stats = core.lint_plugins(order, _index(data_dir))

        assert warnings == []
        assert stats["scanned"] == 0

    def test_a_plugin_not_found_by_the_index_is_skipped(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        order = ["Ghost.esp"]  # never written to disk

        warnings, stats = core.lint_plugins(order, _index(data_dir))

        assert warnings == []
        assert stats["scanned"] == 0

    def test_an_unreadable_plugin_is_counted_separately_from_scanned(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        path = write_plugin(data_dir / "Mine.esp")

        original_read_bytes = Path.read_bytes

        def _raises(self: Path) -> bytes:
            if self == path:
                raise OSError("simulated: permission denied")
            return original_read_bytes(self)

        monkeypatch.setattr(Path, "read_bytes", _raises)

        _warnings, stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert stats["unreadable"] == 1
        assert stats["scanned"] == 0

    def test_a_non_tes3_file_is_silently_skipped(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        (data_dir / "Mine.esp").write_bytes(b"not a TES3 file at all")

        warnings, stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert warnings == []
        assert stats["scanned"] == 0

    def test_a_progress_callback_is_invoked_per_scanned_plugin(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(data_dir / "Mine.esp")
        seen = []

        core.lint_plugins(
            ["Mine.esp"], _index(data_dir), progress=lambda pos, plugin: seen.append((pos, plugin))
        )

        assert seen == [(0, "Mine.esp")]


class TestFogbugAndNoPathgrid:
    def test_a_fog_bug_cell_produces_a_warning(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(data_dir / "Mine.esp", extra=interior_cell("Some Interior", fog=0.0))

        warnings, _stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert any("[FOGBUG]" in w and "Some Interior" in w for w in warnings)

    def test_a_new_interior_cell_with_no_pathgrid_anywhere_is_flagged(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(data_dir / "Mine.esp", extra=interior_cell("Some Interior", fog=1.0))

        warnings, _stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert any("[NO PATHGRID]" in w and "Some Interior" in w for w in warnings)

    def test_a_pathgrid_from_any_plugin_clears_the_warning(self, tmp_path: Path) -> None:
        # The pathgrid can come from a DIFFERENT plugin than the one that
        # introduced the cell -- that's the false positive this fixes.
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(data_dir / "A_Cell.esp", extra=interior_cell("Some Interior", fog=1.0))
        pgrd = sub("NAME", zstr("Some Interior")) + sub(
            "DATA", __import__("struct").pack("<iihBB", 0, 0, 0, 0, 0)
        )
        write_plugin(data_dir / "B_Pathgrid.esp", extra=rec("PGRD", pgrd))

        warnings, _stats = core.lint_plugins(["A_Cell.esp", "B_Pathgrid.esp"], _index(data_dir))

        assert not any("[NO PATHGRID]" in w for w in warnings)

    def test_the_first_plugin_to_introduce_a_cell_is_the_one_named(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(data_dir / "First.esp", extra=interior_cell("Shared Interior", fog=1.0))
        write_plugin(data_dir / "Second.esp", extra=interior_cell("Shared Interior", fog=1.0))

        warnings, _stats = core.lint_plugins(["First.esp", "Second.esp"], _index(data_dir))

        matches = [w for w in warnings if "[NO PATHGRID]" in w]
        assert len(matches) == 1
        assert "'First.esp'" in matches[0]


class TestEvilGmst:
    def test_an_evil_gmst_is_named_in_the_warning(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        evil_name, (tag, raw_value) = next(iter(core._EVIL_GMSTS.items()))
        write_plugin(data_dir / "Mine.esp", extra=_gmst_record(evil_name, tag, raw_value))

        warnings, _stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert any("[EVLGMST]" in w and evil_name in w for w in warnings)

    def test_the_same_name_with_a_different_value_produces_no_warning(self, tmp_path: Path) -> None:
        # Both name AND value must match -- a deliberate change to a game
        # setting is the plugin doing its job, not a stale CS default.
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        evil_name, (tag, _raw_value) = next(iter(core._EVIL_GMSTS.items()))
        write_plugin(
            data_dir / "Mine.esp",
            extra=_gmst_record(evil_name, tag, b"\x00\x00\x00\x00"),
        )

        warnings, _stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert warnings == []

    def test_a_name_not_in_the_table_produces_no_warning(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(
            data_dir / "Mine.esp",
            extra=_gmst_record("sSomeOrdinarySetting", "STRV", zstr("a normal value")),
        )

        warnings, _stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert warnings == []


class TestHeaderGate:
    def test_a_custom_plugin_with_a_blank_header_is_flagged(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        (data_dir / "Mine.esp").write_bytes(_header_with_gaps())

        warnings, _stats = core.lint_plugins(
            ["Mine.esp"], _index(data_dir), subset_names=["Mine.esp"]
        )

        assert any("[HEADER]" in w for w in warnings)

    def test_a_non_custom_plugin_with_a_blank_header_is_not_flagged(self, tmp_path: Path) -> None:
        # A curated/vanilla file's metadata is the list's business, not this
        # tool's -- the header check only applies to the user's own mods.
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        (data_dir / "Mine.esp").write_bytes(_header_with_gaps())

        warnings, _stats = core.lint_plugins(["Mine.esp"], _index(data_dir))

        assert warnings == []


class TestOriginTagging:
    def test_a_warning_includes_the_plugins_origin_tag_when_known(self, tmp_path: Path) -> None:
        data_dir = tmp_path / "Data Files"
        data_dir.mkdir()
        write_plugin(data_dir / "Mine.esp", extra=interior_cell("Some Interior", fog=0.0))

        warnings, _stats = core.lint_plugins(
            ["Mine.esp"], _index(data_dir), origins={"mine.esp": "MyList"}
        )

        assert any("[MyList]" in w for w in warnings)
