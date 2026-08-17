"""scan_mod_directories: walking a mods folder for data paths and plugins.

Behind --scan-dir and the GUI's folder-scan button. Had zero direct test
coverage. The two behaviors worth pinning are the asset-folder-or-plugin
match rule (either is enough) and the pruning: once a folder matches, its
subfolders are never descended into, so a mod manager's per-mod folder
doesn't get double-counted by whatever's nested inside it.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path


class TestMatchingRules:
    def test_an_asset_folder_alone_is_a_match(self, tmp_path: Path) -> None:
        mod = tmp_path / "GrassMod"
        (mod / "textures").mkdir(parents=True)

        lines, n_folders, n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 1
        assert n_plugins == 0
        assert str(mod) in lines

    def test_a_plugin_alone_is_a_match(self, tmp_path: Path) -> None:
        mod = tmp_path / "QuestMod"
        mod.mkdir()
        (mod / "MyQuest.esp").write_bytes(b"")

        lines, n_folders, n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 1
        assert n_plugins == 1
        assert "MyQuest.esp" in lines

    def test_a_folder_with_neither_is_not_a_match(self, tmp_path: Path) -> None:
        (tmp_path / "NotAMod" / "readme").mkdir(parents=True)
        (tmp_path / "NotAMod" / "notes.txt").write_text("hi", encoding="utf-8")

        lines, n_folders, _n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 0
        assert lines == []

    def test_asset_folder_name_matching_is_case_insensitive(self, tmp_path: Path) -> None:
        mod = tmp_path / "GrassMod"
        (mod / "Textures").mkdir(parents=True)

        _lines, n_folders, _n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 1

    def test_plugin_extension_matching_is_case_insensitive(self, tmp_path: Path) -> None:
        mod = tmp_path / "QuestMod"
        mod.mkdir()
        (mod / "MyQuest.ESP").write_bytes(b"")

        _lines, _n_folders, n_plugins = core.scan_mod_directories(tmp_path)

        assert n_plugins == 1

    def test_a_folder_with_both_gets_its_path_and_its_plugins(self, tmp_path: Path) -> None:
        mod = tmp_path / "FullMod"
        (mod / "meshes").mkdir(parents=True)
        (mod / "FullMod.esp").write_bytes(b"")

        lines, n_folders, n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 1
        assert n_plugins == 1
        assert str(mod) in lines
        assert "FullMod.esp" in lines


class TestPruning:
    def test_a_matched_folders_subfolders_are_not_scanned_separately(self, tmp_path: Path) -> None:
        mod = tmp_path / "OuterMod"
        (mod / "meshes").mkdir(parents=True)
        nested = mod / "docs" / "InnerLookingFolder"
        (nested / "textures").mkdir(parents=True)
        (nested / "Inner.esp").write_bytes(b"")

        lines, n_folders, n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 1  # only OuterMod -- pruned before reaching InnerLookingFolder
        assert n_plugins == 0
        assert "Inner.esp" not in lines

    def test_sibling_mod_folders_are_each_counted(self, tmp_path: Path) -> None:
        for name in ("ModA", "ModB", "ModC"):
            (tmp_path / name / "meshes").mkdir(parents=True)

        _lines, n_folders, _n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 3

    def test_an_unmatched_folder_still_lets_its_children_be_found(self, tmp_path: Path) -> None:
        # e.g. a top-level "Mods" container folder that holds real mods but
        # is not itself one -- walking must continue past it.
        container = tmp_path / "Mods"
        (container / "RealMod" / "meshes").mkdir(parents=True)

        lines, n_folders, _n_plugins = core.scan_mod_directories(tmp_path)

        assert n_folders == 1
        assert str(container / "RealMod") in lines


class TestOutputFormatting:
    def test_plugins_are_sorted_case_insensitively_under_their_folder(self, tmp_path: Path) -> None:
        mod = tmp_path / "Bundle"
        mod.mkdir()
        for name in ("Zebra.esp", "alpha.esp", "Mike.esp"):
            (mod / name).write_bytes(b"")

        lines, _n_folders, n_plugins = core.scan_mod_directories(tmp_path)

        assert n_plugins == 3
        idx = lines.index(str(mod))
        assert lines[idx + 1 : idx + 4] == ["alpha.esp", "Mike.esp", "Zebra.esp"]

    def test_each_matched_folder_is_followed_by_a_blank_separator_line(
        self, tmp_path: Path
    ) -> None:
        (tmp_path / "ModA" / "meshes").mkdir(parents=True)
        (tmp_path / "ModB" / "meshes").mkdir(parents=True)

        lines, _n_folders, _n_plugins = core.scan_mod_directories(tmp_path)

        assert lines.count("") == 2

    def test_output_path_writes_the_same_lines_newline_joined(self, tmp_path: Path) -> None:
        (tmp_path / "ModA" / "meshes").mkdir(parents=True)
        out = tmp_path / "subset.txt"

        lines, _n_folders, _n_plugins = core.scan_mod_directories(tmp_path, out)

        assert out.exists()
        assert out.read_text(encoding="utf-8") == "\n".join(lines) + "\n"

    def test_no_output_path_writes_nothing(self, tmp_path: Path) -> None:
        (tmp_path / "ModA" / "meshes").mkdir(parents=True)

        core.scan_mod_directories(tmp_path, output_path=None)

        assert list(tmp_path.glob("*.txt")) == []

    def test_an_empty_start_folder_writes_an_empty_file_not_a_bare_newline(
        self, tmp_path: Path
    ) -> None:
        empty = tmp_path / "Empty"
        empty.mkdir()
        out = tmp_path / "subset.txt"

        lines, n_folders, n_plugins = core.scan_mod_directories(empty, out)

        assert lines == []
        assert n_folders == 0 and n_plugins == 0
        assert out.read_text(encoding="utf-8") == ""
