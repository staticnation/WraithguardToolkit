"""Tests for turning a load order into plugin records.

The failure these pin shipped: a real 27-master load order stopped at
``could not read master distant_seafloor_2.00.esm``, with no way to tell
whether the file was missing, unreadable, or simply somewhere else. Both
halves of that -- searching one folder, and discarding the reason -- are
covered here.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from wraithguard.land.landmass import Landmass
from wraithguard.land.meta import MetaError, load_meta, write_merged_marker
from wraithguard.land.pipeline import MergedCell, MergeOutcome
from wraithguard.land.service import (
    MergeServiceError,
    _contributors,
    _records_via,
    _split_order,
    build_merged_lands,
    resolve_plugin,
)

if TYPE_CHECKING:
    from pathlib import Path


class TestResolvePlugin:
    """A load order spans many data folders, spelled inconsistently."""

    def test_finds_a_plugin_in_a_later_folder(self, tmp_path: Path) -> None:
        """The master may not be in the folder the first plugin was found in."""
        first = tmp_path / "vanilla"
        second = tmp_path / "mod"
        first.mkdir()
        second.mkdir()
        (second / "distant_seafloor.esm").write_bytes(b"")
        found = resolve_plugin("distant_seafloor.esm", [first, second])
        assert found == second / "distant_seafloor.esm"

    def test_matches_case_insensitively(self, tmp_path: Path) -> None:
        """``openmw.cfg`` and the filesystem routinely disagree on case."""
        tmp_path.joinpath("RepopulatedMorrowind.esm").write_bytes(b"")
        assert resolve_plugin("RepopulatedMorrowind.ESM", [tmp_path]) is not None

    def test_returns_none_when_absent(self, tmp_path: Path) -> None:
        """Not finding it is reported, not guessed at."""
        assert resolve_plugin("nothing.esm", [tmp_path]) is None

    def test_an_unreadable_folder_does_not_stop_the_search(self, tmp_path: Path) -> None:
        """A vanished data folder must not hide a plugin in the next one."""
        good = tmp_path / "good"
        good.mkdir()
        (good / "a.esm").write_bytes(b"")
        assert resolve_plugin("a.esm", [tmp_path / "gone", good]) is not None

    def test_prefers_the_earlier_folder(self, tmp_path: Path) -> None:
        """Search order is load order: the first data folder wins."""
        first = tmp_path / "one"
        second = tmp_path / "two"
        first.mkdir()
        second.mkdir()
        (first / "a.esm").write_bytes(b"")
        (second / "a.esm").write_bytes(b"")
        assert resolve_plugin("a.esm", [first, second]) == first / "a.esm"


class TestRecordsViaReportsWhy:
    """A master that will not convert stops the merge, so it must say why."""

    def test_a_missing_converter_is_named(self, tmp_path: Path) -> None:
        """The reason is returned rather than collapsed into an empty list."""
        records, failure = _records_via(
            str(tmp_path / "no-such-tes3conv"), tmp_path / "a.esm", tmp_path
        )
        assert records == []
        assert "tes3conv" in failure

    def test_a_converter_error_carries_its_message(self, tmp_path: Path) -> None:
        """tes3conv's own complaint is what the user needs to see."""
        script = tmp_path / "fake.sh"
        script.write_text('#!/bin/sh\necho "bad plugin" >&2\nexit 3\n', encoding="utf-8")
        script.chmod(0o755)
        records, failure = _records_via(str(script), tmp_path / "a.esm", tmp_path)
        assert records == []
        assert "exited 3" in failure
        assert "bad plugin" in failure

    def test_success_reports_no_failure(self, tmp_path: Path) -> None:
        """The happy path returns records and an empty reason."""
        script = tmp_path / "fake.sh"
        script.write_text('#!/bin/sh\nprintf \'[{"type":"Header"}]\' > "$2"\n', encoding="utf-8")
        script.chmod(0o755)
        records, failure = _records_via(str(script), tmp_path / "a.esm", tmp_path)
        assert failure == ""
        assert records == [{"type": "Header"}]


class TestSplitOrder:
    """Masters are the reference; mods are what gets merged onto it."""

    def test_extensions_decide(self) -> None:
        """``.esm`` and ``.omwgame`` are masters, everything else is a mod."""
        masters, mods = _split_order(["a.esm", "b.esp", "c.omwgame", "d.omwaddon"])
        assert masters == ["a.esm", "c.omwgame"]
        assert mods == ["b.esp", "d.omwaddon"]

    def test_case_does_not_matter(self) -> None:
        """A load order file may spell the extension either way."""
        masters, _mods = _split_order(["A.ESM"])
        assert masters == ["A.ESM"]


class TestMasterFailuresAreExplained:
    """The message a user sees when the merge stops at a master."""

    def test_a_master_in_no_folder_says_so(self, tmp_path: Path) -> None:
        """The old message could not distinguish this from a parse error."""
        with pytest.raises(MergeServiceError, match="not in any of the"):
            build_merged_lands(
                data_files=[tmp_path],
                load_order=["distant_seafloor.esm"],
                converter="tes3conv",
            )

    def test_no_masters_at_all_is_refused(self, tmp_path: Path) -> None:
        """Without a reference every mod looks like a total rewrite."""
        with pytest.raises(MergeServiceError, match="no masters"):
            build_merged_lands(data_files=[tmp_path], load_order=["a.esp"], converter="tes3conv")

    def test_no_data_folder_is_refused(self) -> None:
        """An empty folder list would silently find nothing."""
        with pytest.raises(MergeServiceError, match="no data folder"):
            build_merged_lands(data_files=[], load_order=["a.esm"], converter="tes3conv")


class TestTheOutputIsMarkedAsGenerated:
    """A merged plugin must not be merged again on the next run.

    ``Merged Lands.esp`` is an ``.esp`` that edits every cell it wrote and
    loads last. Without a marker beside it, a second run reads it as a mod and
    reconciles its terrain as one more opinion -- a merge of a merge. Nothing
    fails; the terrain just drifts further from what any author wrote each time.
    """

    def test_the_marker_is_written_and_read_back(self, tmp_path: Path) -> None:
        """The sidecar round-trips through the loader that consumes it."""
        plugin = tmp_path / "Merged Lands.esp"
        plugin.write_bytes(b"")
        marker = write_merged_marker(plugin)
        assert marker.name == "Merged Lands.mergedlands.toml"
        assert load_meta(plugin).is_previous_merge

    def test_a_plugin_without_a_marker_is_not_a_previous_merge(self, tmp_path: Path) -> None:
        """The default must stay permissive; every ordinary mod lacks a sidecar."""
        plugin = tmp_path / "some_mod.esp"
        plugin.write_bytes(b"")
        assert not load_meta(plugin).is_previous_merge

    def test_an_unwritable_location_is_reported(self, tmp_path: Path) -> None:
        """A missing marker is a trap for the next run, so it is not silent."""
        plugin = tmp_path / "gone" / "Merged Lands.esp"
        with pytest.raises(MetaError, match="marks"):
            write_merged_marker(plugin)


class TestDeclaredMasters:
    """The merged file declares what it read, not the whole load order."""

    def _outcome(self, editors: list[str]) -> MergeOutcome:
        """A one-cell outcome edited by ``editors``."""
        outcome = MergeOutcome()
        outcome.cells[(0, 0)] = MergedCell(coords=(0, 0), editors=list(editors))
        return outcome

    def _reference(self, source: str) -> Landmass:
        """A reference landmass whose only cell came from ``source``."""
        landmass = Landmass(name="reference")
        landmass.sources[(0, 0)] = source
        return landmass

    def test_only_contributing_plugins_are_declared(self) -> None:
        """Twenty-seven masters in the order does not mean twenty-seven here."""
        order = ["Morrowind.esm", "Unused.esm", "a.esp", "b.esp"]
        declared = _contributors(
            self._outcome(["a.esp"]), [], self._reference("Morrowind.esm"), order
        )
        assert declared == ["Morrowind.esm", "a.esp"]

    def test_the_master_behind_the_cell_is_declared(self) -> None:
        """A LAND record overrides one; the file it overrides must be named."""
        declared = _contributors(
            self._outcome(["a.esp"]),
            [],
            self._reference("Bloodmoon.esm"),
            ["Bloodmoon.esm", "a.esp"],
        )
        assert "Bloodmoon.esm" in declared

    def test_texture_sources_are_declared(self) -> None:
        """A texture's file name came from somewhere, and that is a dependency."""
        declared = _contributors(
            self._outcome([]),
            ["Tamriel_Data.esm"],
            self._reference("Morrowind.esm"),
            ["Morrowind.esm", "Tamriel_Data.esm"],
        )
        assert declared == ["Morrowind.esm", "Tamriel_Data.esm"]

    def test_load_order_decides_the_order(self) -> None:
        """Masters are declared in the order they load, not alphabetically."""
        order = ["z.esm", "a.esp"]
        declared = _contributors(self._outcome(["a.esp"]), [], self._reference("z.esm"), order)
        assert declared == ["z.esm", "a.esp"]

    def test_a_contributor_missing_from_the_order_is_still_declared(self) -> None:
        """Omitting a real dependency is worse than declaring one out of order."""
        declared = _contributors(
            self._outcome(["ghost.esp"]), [], self._reference("Morrowind.esm"), ["Morrowind.esm"]
        )
        assert declared == ["Morrowind.esm", "ghost.esp"]


class TestNonPluginsAreNotMerged:
    """A load order may name things that are not plugins.

    ``.omwscripts`` is OpenMW's list of Lua scripts to attach. It holds no
    records, it belongs in the load order, and it is not something the merge
    can read. Reporting it under "could not be read" is noise that looks like
    a problem the user needs to fix.
    """

    def test_omwscripts_is_neither_master_nor_mod(self) -> None:
        """It is dropped before either list is built."""
        masters, mods = _split_order(
            ["Morrowind.esm", "a.esp", "Natural Wildlife.omwscripts", "b.omwaddon"]
        )
        assert masters == ["Morrowind.esm"]
        assert mods == ["a.esp", "b.omwaddon"]

    def test_the_case_it_is_written_in_does_not_matter(self) -> None:
        """Load order files are not consistent about case."""
        _masters, mods = _split_order(["A.OMWSCRIPTS"])
        assert mods == []

    def test_real_plugins_are_untouched(self) -> None:
        """The filter must not eat anything that does hold records."""
        masters, mods = _split_order(["a.esm", "b.esp", "c.omwaddon", "d.omwgame"])
        assert masters == ["a.esm", "d.omwgame"]
        assert mods == ["b.esp", "c.omwaddon"]
