"""_apply_plugin_order_yml: the --plugin-order-yml / --list-name stage of
compute_plan.

wraithguard.momw's own parsing (parse_plugin_order_yml, curated_for_list) is
already pinned by test_momw.py. This tests the orchestration wrapper around
it -- guarding a bad/missing file down to a warning instead of aborting the
run, the "no --list-name" note, the "list not found" warning, and the actual
job: dropping curated plugins out of the subset so this tool never reorders
what a curated list already owns.

Reuses test_momw.py's exact yml fixture (A.esp on total-overhaul +
i-heart-vanilla, B.esp on total-overhaul only) rather than inventing a new
one, so both files stay honest about the same real MOMW shape.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path

_YML = """\
# comment, ignored
- not-a-mapping
- for_mod: orphan with no filename
- file_name: A.esp
  for_mod: Mod A
  needs_cleaning: true
  on_lists:
    - total-overhaul
    - i-heart-vanilla
- file_name: B.esp
  on_lists:
    - total-overhaul
  depends:
    - file_name: nested-should-be-ignored.esp
"""


def _write_yml(tmp_path: Path) -> Path:
    path = tmp_path / "plugin-order.yml"
    path.write_text(_YML, encoding="utf-8")
    return path


def _args(tmp_path: Path, *, plugin_order_yml: Path | None = None, list_name: str | None = None):
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text('data="C:/Data Files"\ncontent=Morrowind.esm\n', encoding="utf-8")
    rules = tmp_path / "mlox_base.txt"
    rules.write_text("", encoding="utf-8")
    argv = ["--cfg", str(cfg), "--rules", str(rules)]
    if plugin_order_yml is not None:
        argv += ["--plugin-order-yml", str(plugin_order_yml)]
    if list_name is not None:
        argv += ["--list-name", list_name]
    return core.build_arg_parser().parse_args(argv)


class TestNoYmlGiven:
    def test_subset_and_every_result_field_pass_through_unchanged(self, tmp_path: Path) -> None:
        args = _args(tmp_path)
        subset = ["MyMod.esp", "OtherMod.esp"]

        (
            result_subset,
            yml_entries,
            curated_set,
            curated_order,
            yml_warnings,
            declared_lower,
            list_name,
        ) = core._apply_plugin_order_yml(args, subset)

        assert result_subset == subset
        assert yml_entries == []
        assert curated_set == set()
        assert curated_order == []
        assert yml_warnings == []
        assert declared_lower == {"mymod.esp", "othermod.esp"}
        assert list_name is None


class TestYmlWithoutListName:
    def test_entries_load_but_nothing_is_curated(self, tmp_path: Path, capsys) -> None:
        yml = _write_yml(tmp_path)
        args = _args(tmp_path, plugin_order_yml=yml)

        result_subset, yml_entries, curated_set, _order, _warn, _decl, _list_name = (
            core._apply_plugin_order_yml(args, ["A.esp"])
        )

        assert len(yml_entries) == 2  # A.esp, B.esp -- the two well-formed entries
        assert curated_set == set()
        assert result_subset == ["A.esp"]  # nothing dropped without a list to check against
        assert "no list name given" in capsys.readouterr().out


class TestYmlWithListName:
    def test_a_curated_plugin_is_dropped_from_the_subset(self, tmp_path: Path) -> None:
        yml = _write_yml(tmp_path)
        args = _args(tmp_path, plugin_order_yml=yml, list_name="total-overhaul")

        result_subset, _entries, curated_set, curated_order, warnings, _decl, list_name = (
            core._apply_plugin_order_yml(args, ["A.esp", "MyMod.esp"])
        )

        assert curated_set == {"a.esp", "b.esp"}
        assert curated_order == ["A.esp", "B.esp"]
        assert result_subset == ["MyMod.esp"]  # A.esp is the curated list's job now
        assert list_name == "total-overhaul"
        assert any("[REDUNDANT]" in w and "A.esp" in w for w in warnings)

    def test_matching_is_case_insensitive(self, tmp_path: Path) -> None:
        yml = _write_yml(tmp_path)
        args = _args(tmp_path, plugin_order_yml=yml, list_name="total-overhaul")

        result_subset, *_rest = core._apply_plugin_order_yml(args, ["a.ESP"])

        assert result_subset == []

    def test_a_subset_with_nothing_curated_is_left_alone(self, tmp_path: Path) -> None:
        yml = _write_yml(tmp_path)
        args = _args(tmp_path, plugin_order_yml=yml, list_name="total-overhaul")

        result_subset, _entries, _curated, _order, warnings, _decl, _name = (
            core._apply_plugin_order_yml(args, ["MyMod.esp"])
        )

        assert result_subset == ["MyMod.esp"]
        assert warnings == []

    def test_a_list_name_not_in_the_yml_warns_and_curates_nothing(
        self, tmp_path: Path, capsys
    ) -> None:
        yml = _write_yml(tmp_path)
        args = _args(tmp_path, plugin_order_yml=yml, list_name="does-not-exist")

        result_subset, _entries, curated_set, _order, warnings, _decl, _name = (
            core._apply_plugin_order_yml(args, ["A.esp"])
        )

        assert curated_set == set()
        assert result_subset == ["A.esp"]
        assert warnings == []
        assert "no plugins found for list" in capsys.readouterr().out


class TestUnreadableYml:
    def test_a_missing_file_downgrades_to_a_warning_not_a_crash(
        self, tmp_path: Path, capsys
    ) -> None:
        missing = tmp_path / "does-not-exist.yml"
        args = _args(tmp_path, plugin_order_yml=missing, list_name="total-overhaul")

        result_subset, yml_entries, curated_set, _order, _warn, _decl, _name = (
            core._apply_plugin_order_yml(args, ["A.esp"])
        )

        assert yml_entries == []
        assert curated_set == set()
        assert result_subset == ["A.esp"]  # the run continues, nothing curated
        assert "could not read plugin-order.yml" in capsys.readouterr().out
