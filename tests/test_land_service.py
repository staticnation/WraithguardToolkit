"""Tests for turning a load order into plugin records.

The failure these pin shipped: a real 27-master load order stopped at
``could not read master distant_seafloor_2.00.esm``, with no way to tell
whether the file was missing, unreadable, or simply somewhere else. Both
halves of that -- searching one folder, and discarding the reason -- are
covered here.
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from wraithguard.land.diff import LandData
from wraithguard.land.landmass import Landmass
from wraithguard.land.merge import ConflictStrategy
from wraithguard.land.meta import (
    LAYER_NAMES,
    META_TYPES,
    STRATEGY_NAMES,
    MergeSettings,
    MetaError,
    PluginMeta,
    load_meta,
    parse_meta,
    render_meta,
    strategy_display_name,
    strategy_from_name,
    write_merged_marker,
    write_meta,
    write_patch_template,
    write_settings,
)
from wraithguard.land.pipeline import MergedCell, MergeOutcome
from wraithguard.land.service import (
    MergeServiceError,
    _contributors,
    _describe_meta,
    _records_via,
    _split_order,
    build_merged_lands,
    resolve_plugin,
)

# The two converter-subprocess tests below stand a shell script in for tes3conv
# and run it. Windows' CreateProcess cannot execute a script directly (no
# shebang, and .bat/.py are not valid argv[0]s), so there is no portable "fake
# executable" to drop; the subprocess error/success handling is exercised on
# POSIX, where a chmod +x script is a real executable. The code under test is
# platform-independent -- only the stand-in is not.
_posix_only = pytest.mark.skipif(
    os.name != "posix", reason="needs an executable stand-in for tes3conv (POSIX script)"
)


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

    def test_the_last_data_folder_wins(self, tmp_path: Path) -> None:
        """OpenMW resolves a shared name to the latest ``data=`` folder.

        A later ``data=`` line shadows an earlier one, so a mod split across
        ``00 Core`` and ``01 Patch`` -- both shipping ``a.esm``, patch second --
        must resolve to the patch copy. Returning the first match read the
        pre-patch file, which is the load-order bug this pins against.
        """
        core = tmp_path / "00 Core"
        patch = tmp_path / "01 Patch"
        core.mkdir()
        patch.mkdir()
        (core / "a.esm").write_bytes(b"")
        (patch / "a.esm").write_bytes(b"")
        # directories arrive in load order (earliest first); the patch wins.
        assert resolve_plugin("a.esm", [core, patch]) == patch / "a.esm"

    def test_case_insensitive_collision_still_takes_the_later_folder(self, tmp_path: Path) -> None:
        """Last-wins holds even when the two copies differ only in case."""
        core = tmp_path / "00 Core"
        patch = tmp_path / "01 Patch"
        core.mkdir()
        patch.mkdir()
        (core / "Dwemer Airship_Exterior.ESP").write_bytes(b"")
        (patch / "Dwemer Airship_Exterior.esp").write_bytes(b"")
        found = resolve_plugin("Dwemer Airship_Exterior.esp", [core, patch])
        assert found == patch / "Dwemer Airship_Exterior.esp"


class TestRecordsViaReportsWhy:
    """A master that will not convert stops the merge, so it must say why."""

    def test_a_missing_converter_is_named(self, tmp_path: Path) -> None:
        """The reason is returned rather than collapsed into an empty list."""
        records, failure = _records_via(
            str(tmp_path / "no-such-tes3conv"), tmp_path / "a.esm", tmp_path
        )
        assert records == []
        assert "tes3conv" in failure

    @_posix_only
    def test_a_converter_error_carries_its_message(self, tmp_path: Path) -> None:
        """tes3conv's own complaint is what the user needs to see."""
        script = tmp_path / "fake.sh"
        script.write_text('#!/bin/sh\necho "bad plugin" >&2\nexit 3\n', encoding="utf-8")
        script.chmod(0o755)
        records, failure = _records_via(str(script), tmp_path / "a.esm", tmp_path)
        assert records == []
        assert "exited 3" in failure
        assert "bad plugin" in failure

    @_posix_only
    def test_success_reports_no_failure(self, tmp_path: Path) -> None:
        """The happy path returns records and an empty reason."""
        script = tmp_path / "fake.sh"
        script.write_text('#!/bin/sh\nprintf \'[{"type":"Header"}]\' > "$2"\n', encoding="utf-8")
        script.chmod(0o755)
        records, failure = _records_via(str(script), tmp_path / "a.esm", tmp_path)
        assert failure == ""
        assert records == [{"type": "Header"}]

    def test_the_converter_call_suppresses_the_console_window(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """A --noconsole build must not flash a window per plugin converted.

        The Merged Lands run shells out to tes3conv once per landscape plugin;
        without the no-window flags each opened a console window. The fix threads
        ``no_window_kwargs()`` into the call -- this pins that it reaches
        ``subprocess.run``.
        """
        import types

        import wraithguard.land.service as svc

        seen: dict[str, object] = {}

        def fake_run(argv: list[str], **kwargs: object) -> types.SimpleNamespace:
            seen.update(kwargs)
            Path(argv[2]).write_text("[]", encoding="utf-8")  # the JSON target
            return types.SimpleNamespace(returncode=0, stdout="", stderr="")

        monkeypatch.setattr(svc, "has_landscape", lambda *_a, **_k: True)
        monkeypatch.setattr(svc, "no_window_kwargs", lambda: {"creationflags": 0x08000000})
        monkeypatch.setattr(svc.subprocess, "run", fake_run)

        _records_via("tes3conv", tmp_path / "a.esm", tmp_path)
        assert seen.get("creationflags") == 0x08000000


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

    def test_the_marker_is_no_longer_blank_inside(self, tmp_path: Path) -> None:
        """It spells out every layer, so the file is self-describing."""
        plugin = tmp_path / "Merged Lands.esp"
        plugin.write_bytes(b"")
        marker = write_merged_marker(plugin)
        text = marker.read_text(encoding="utf-8")
        for name in LAYER_NAMES:
            assert f"[{name}]" in text
        # Belt-and-braces: a marker excludes every layer.
        meta = load_meta(plugin)
        assert meta.is_previous_merge
        assert meta.allowed_layers() == LandData.NONE  # nothing included


class TestWritingSidecars:
    """render_meta is the inverse of parse_meta, and the writers use it."""

    def test_render_round_trips_through_the_parser(self, tmp_path: Path) -> None:
        """What the writer produces must parse back to the same settings."""
        meta = PluginMeta(
            meta_type="Patch",
            layers={
                "height_map": MergeSettings(included=False),
                "world_map_data": MergeSettings(conflict_strategy=ConflictStrategy.OVERWRITE),
            },
        )
        reparsed = parse_meta(_parse(render_meta(meta), tmp_path))
        assert reparsed.settings_for("height_map").included is False
        assert (
            reparsed.settings_for("world_map_data").conflict_strategy is ConflictStrategy.OVERWRITE
        )

    def test_explicit_writes_every_layer_default_omits_them(self, tmp_path: Path) -> None:
        """Explicit is for humans; skip-default matches Merged Lands' own output."""
        meta = PluginMeta(meta_type="Patch")
        explicit = render_meta(meta, explicit=True)
        minimal = render_meta(meta, explicit=False)
        for name in LAYER_NAMES:
            assert f"[{name}]" in explicit
            assert f"[{name}]" not in minimal

    def test_curvature_is_preserved_our_own_strategy(self, tmp_path: Path) -> None:
        """The strategy we add beyond Merged Lands must survive a round trip."""
        meta = PluginMeta(
            meta_type="Patch",
            layers={"height_map": MergeSettings(conflict_strategy=ConflictStrategy.CURVATURE)},
        )
        assert 'conflict_strategy = "Curvature"' in render_meta(meta)
        reparsed = parse_meta(_parse(render_meta(meta), tmp_path))
        assert reparsed.settings_for("height_map").conflict_strategy is ConflictStrategy.CURVATURE

    def test_patch_template_is_a_full_editable_patch(self, tmp_path: Path) -> None:
        """A template shows every knob at its default and parses cleanly."""
        plugin = tmp_path / "SomeMod.esp"
        plugin.write_bytes(b"")
        sidecar = write_patch_template(plugin)
        assert sidecar.name == "SomeMod.mergedlands.toml"
        text = sidecar.read_text(encoding="utf-8")
        assert "conflict_strategy" in text  # the header lists the options
        for name in LAYER_NAMES:
            assert f"[{name}]" in text
        meta = load_meta(plugin)  # parses without error
        assert meta.meta_type == "Patch"
        assert meta.allowed_layers() != LandData.NONE  # defaults include everything

    def test_write_meta_reports_an_unwritable_path(self, tmp_path: Path) -> None:
        """A write into a missing directory fails loudly."""
        plugin = tmp_path / "gone" / "SomeMod.esp"
        with pytest.raises(MetaError):
            write_meta(plugin, PluginMeta())

    def test_write_settings_round_trips_gui_choices(self, tmp_path: Path) -> None:
        """What the editor writes parses back to the same PluginMeta.

        The GUI editor builds a PluginMeta from its controls and calls
        write_settings; this pins that the chosen strategy and dropped layer
        survive the write/read round trip and that the header is included.
        """
        plugin = tmp_path / "SomeMod.esp"
        plugin.write_bytes(b"")
        meta = PluginMeta(
            meta_type="Patch",
            layers={
                "texture_indices": MergeSettings(included=False),
                "world_map_data": MergeSettings(conflict_strategy=ConflictStrategy.OVERWRITE),
            },
        )
        sidecar = write_settings(plugin, meta)
        assert "conflict_strategy" in sidecar.read_text(encoding="utf-8")  # header present
        reloaded = load_meta(plugin)
        assert reloaded.meta_type == "Patch"
        assert reloaded.settings_for("texture_indices").included is False
        assert (
            reloaded.settings_for("world_map_data").conflict_strategy is ConflictStrategy.OVERWRITE
        )

    def test_public_strategy_and_meta_constants_match_the_schema(self) -> None:
        """The GUI's dropdowns read these, so they must stay in step with the
        parser: every name round-trips through the value and back."""
        assert set(META_TYPES) == {"Auto", "Patch", "MergedLands"}
        assert STRATEGY_NAMES[0] == "Auto"  # the default is offered first
        for name in STRATEGY_NAMES:
            assert strategy_display_name(strategy_from_name(name)) == name

    def test_describe_meta_names_every_layer_and_its_state(self) -> None:
        """The verbose log line spells out on/off and the strategy per layer.

        A troubleshooting log has to show *why* a plugin merged as it did, which
        means naming each layer's setting, not just that a sidecar was present.
        """
        from wraithguard.land.merge import ConflictStrategy

        meta = PluginMeta(
            meta_type="Patch",
            layers={
                "texture_indices": MergeSettings(included=False),
                "world_map_data": MergeSettings(conflict_strategy=ConflictStrategy.OVERWRITE),
            },
        )
        line = _describe_meta(meta)
        assert "meta_type=Patch" in line
        assert "texture_indices: OFF/" in line  # excluded layer flagged
        assert "world_map_data: on/Overwrite" in line  # strategy named
        assert "height_map: on/Auto" in line  # a default layer is still shown


def _parse(text: str, tmp_path: Path) -> dict:
    """Parse rendered TOML back into a document, via a temp file."""
    scratch = tmp_path / "roundtrip.mergedlands.toml"
    scratch.write_text(text, encoding="utf-8")
    from wraithguard.land.meta import _load_toml

    return _load_toml(scratch)


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
