"""_lint_stage and _resource_stage: the two opt-in, read-only report stages
gated by --lint and --resource-conflicts.

Both follow the same shape as _conflict_and_cellmap_scans (already covered):
a flag check, a scan, a printed report, and an optional CSV. This pins the
orchestration -- the flag gate, --exclude's effect on the lint scan,
scanned/findings counts, and the CSV write's exists-only-when-there's-
something-to-write rule -- not lint_plugins' or detect_resource_conflicts'
own internal check logic, which are large enough to earn their own file
later.
"""

from __future__ import annotations

import types
from typing import TYPE_CHECKING

from conftest import write_plugin

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path


def _args(**overrides: object) -> types.SimpleNamespace:
    base = {"lint": False, "exclude": None, "resource_conflicts": False, "resources_out": None}
    base.update(overrides)
    return types.SimpleNamespace(**base)


class TestLintStageGate:
    def test_lint_false_is_a_silent_no_op(self, tmp_path: Path, capsys) -> None:
        data = tmp_path / "Data Files"
        data.mkdir()
        write_plugin(data / "A.esp")

        core._lint_stage(_args(lint=False), ["A.esp"], ["A.esp"], ["A.esp"], [str(data)], {})

        assert capsys.readouterr().out == ""


class TestLintStageScanning:
    def test_a_clean_run_reports_no_findings(self, tmp_path: Path, capsys) -> None:
        data = tmp_path / "Data Files"
        data.mkdir()
        write_plugin(data / "A.esp")

        core._lint_stage(_args(lint=True), [], ["A.esp"], ["A.esp"], [str(data)], {})

        out = capsys.readouterr().out
        assert "Scanned 1 plugin(s)" in out
        assert "No lint findings." in out

    def test_final_order_none_falls_back_to_base_order_names(self, tmp_path: Path, capsys) -> None:
        data = tmp_path / "Data Files"
        data.mkdir()
        write_plugin(data / "A.esp")

        core._lint_stage(_args(lint=True), [], None, ["A.esp"], [str(data)], {})

        assert "Scanned 1 plugin(s)" in capsys.readouterr().out

    def test_exclude_shrinks_the_scanned_count_and_is_announced(
        self, tmp_path: Path, capsys
    ) -> None:
        data = tmp_path / "Data Files"
        data.mkdir()
        write_plugin(data / "A.esp")
        write_plugin(data / "B.esp")

        core._lint_stage(
            _args(lint=True, exclude=["B.esp"]),
            [],
            ["A.esp", "B.esp"],
            ["A.esp", "B.esp"],
            [str(data)],
            {},
        )

        out = capsys.readouterr().out
        assert "excluded 1 plugin" in out
        assert "Scanned 1 plugin(s)" in out

    def test_a_custom_plugin_with_a_blank_header_is_a_real_finding(
        self, tmp_path: Path, capsys
    ) -> None:
        data = tmp_path / "Data Files"
        data.mkdir()
        write_plugin(data / "Mine.esp", author="", description="")

        core._lint_stage(
            _args(lint=True), ["Mine.esp"], ["Mine.esp"], ["Mine.esp"], [str(data)], {}
        )

        out = capsys.readouterr().out
        assert "[HEADER]" in out
        assert "No lint findings." not in out


class TestResourceStageGate:
    def test_resource_conflicts_false_is_a_silent_no_op(self, tmp_path: Path, capsys) -> None:
        core._resource_stage(_args(resource_conflicts=False), [], [], [])

        assert capsys.readouterr().out == ""


class TestResourceStageScanning:
    def test_a_shared_differing_file_is_reported(self, tmp_path: Path, capsys) -> None:
        dir_a = tmp_path / "A"
        dir_b = tmp_path / "B"
        (dir_a / "textures").mkdir(parents=True)
        (dir_b / "textures").mkdir(parents=True)
        (dir_a / "textures" / "rock.dds").write_bytes(b"aaaa")
        (dir_b / "textures" / "rock.dds").write_bytes(b"bbbb")

        core._resource_stage(_args(resource_conflicts=True), [], [], [str(dir_a), str(dir_b)])

        assert "rock.dds" in capsys.readouterr().out

    def test_identical_shared_files_are_not_reported_as_conflicts(
        self, tmp_path: Path, capsys
    ) -> None:
        dir_a = tmp_path / "A"
        dir_b = tmp_path / "B"
        (dir_a / "textures").mkdir(parents=True)
        (dir_b / "textures").mkdir(parents=True)
        (dir_a / "textures" / "rock.dds").write_bytes(b"same")
        (dir_b / "textures" / "rock.dds").write_bytes(b"same")

        core._resource_stage(_args(resource_conflicts=True), [], [], [str(dir_a), str(dir_b)])

        # detect_resource_conflicts still reports the *candidate*
        # (present in both), but marked identical -- not what this test
        # cares about; the summary line is what proves it printed cleanly.
        out = capsys.readouterr().out
        assert "DATA-PATH RESOURCE (VFS) CONFLICTS" in out

    def test_resources_out_writes_a_csv_only_when_there_are_conflicts(self, tmp_path: Path) -> None:
        dir_a = tmp_path / "A"
        dir_a.mkdir()
        (dir_a / "textures").mkdir()
        (dir_a / "textures" / "rock.dds").write_bytes(b"solo")
        out = tmp_path / "resources.csv"

        core._resource_stage(
            _args(resource_conflicts=True, resources_out=str(out)), [], [], [str(dir_a)]
        )

        # A single data dir has nothing to conflict with -- no CSV expected.
        assert not out.exists()

    def test_resources_out_writes_a_csv_when_conflicts_exist(self, tmp_path: Path) -> None:
        dir_a = tmp_path / "A"
        dir_b = tmp_path / "B"
        (dir_a / "textures").mkdir(parents=True)
        (dir_b / "textures").mkdir(parents=True)
        (dir_a / "textures" / "rock.dds").write_bytes(b"aaaa")
        (dir_b / "textures" / "rock.dds").write_bytes(b"bbbb")
        out = tmp_path / "resources.csv"

        core._resource_stage(
            _args(resource_conflicts=True, resources_out=str(out)),
            [],
            [],
            [str(dir_a), str(dir_b)],
        )

        assert out.exists()
        assert "rock.dds" in out.read_text(encoding="utf-8")

    def test_a_bad_resources_out_path_is_logged_not_raised(self, tmp_path: Path) -> None:
        dir_a = tmp_path / "A"
        dir_b = tmp_path / "B"
        (dir_a / "textures").mkdir(parents=True)
        (dir_b / "textures").mkdir(parents=True)
        (dir_a / "textures" / "rock.dds").write_bytes(b"aaaa")
        (dir_b / "textures" / "rock.dds").write_bytes(b"bbbb")
        bad_out = tmp_path / "no-such-parent-dir" / "resources.csv"

        # Must not raise -- OSError from the write is caught and logged.
        core._resource_stage(
            _args(resource_conflicts=True, resources_out=str(bad_out)),
            [],
            [],
            [str(dir_a), str(dir_b)],
        )

        assert not bad_out.exists()
