"""_yml_post_sort_warnings: the plugin-order.yml sanity notes printed after
the sort -- needs-cleaning, orphan detection, and curated-order drift.

A pure function over plain data, no filesystem involved. needs_cleaning_set
and base_order_matches_yml (both from wraithguard.momw) already have
coverage via test_momw.py; this pins the orchestration around them instead:
which of the three checks run when, what lands in yml_warnings vs. what's
only printed, and the early-exit when there's nothing to check against.
"""

from __future__ import annotations

from typing import Any

import wraithguard_toolkit as core


def _entry(
    file_name: str, *, needs_cleaning: bool = False, on_lists: list[str] | None = None
) -> dict[str, Any]:
    return {
        "file_name": file_name,
        "for_mod": None,
        "on_lists": on_lists or [],
        "needs_cleaning": needs_cleaning,
    }


class TestNoYmlEntries:
    def test_nothing_happens_at_all(self, capsys) -> None:
        warnings: list[str] = []

        result = core._yml_post_sort_warnings(
            ["A.esp"], ["A.esp"], [], warnings, set(), [], set(), None
        )

        assert result is None
        assert warnings == []
        assert capsys.readouterr().out == ""


class TestNeedsCleaning:
    def test_a_flagged_plugin_in_final_order_is_warned_about(self, capsys) -> None:
        entries = [_entry("Dirty.esp", needs_cleaning=True)]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Dirty.esp"], ["Dirty.esp"], entries, warnings, set(), [], set(), None
        )

        assert any("[NEEDS CLEANING]" in w and "Dirty.esp" in w for w in warnings)

    def test_falls_back_to_base_order_names_when_final_order_is_none(self, capsys) -> None:
        entries = [_entry("Dirty.esp", needs_cleaning=True)]
        warnings: list[str] = []

        core._yml_post_sort_warnings(None, ["Dirty.esp"], entries, warnings, set(), [], set(), None)

        assert any("[NEEDS CLEANING]" in w for w in warnings)

    def test_a_clean_plugin_gets_no_warning(self, capsys) -> None:
        entries = [_entry("Clean.esp", needs_cleaning=False)]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Clean.esp"], ["Clean.esp"], entries, warnings, set(), [], set(), None
        )

        assert warnings == []
        assert "No plugin-order.yml warnings." in capsys.readouterr().out

    def test_runs_even_without_a_list_name(self, capsys) -> None:
        # Needs-cleaning is independent of curation -- it fires with no
        # --list-name given at all, unlike orphan/order-drift below.
        entries = [_entry("Dirty.esp", needs_cleaning=True)]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Dirty.esp"], ["Dirty.esp"], entries, warnings, set(), [], set(), None
        )

        assert len(warnings) == 1


class TestOrphanDetection:
    def test_an_uncurated_undeclared_plugin_is_flagged_orphan(self, capsys) -> None:
        entries = [_entry("Curated.esp", on_lists=["total-overhaul"])]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Curated.esp", "Stray.esp"],
            ["Curated.esp", "Stray.esp"],
            entries,
            warnings,
            {"curated.esp"},
            ["Curated.esp"],
            set(),  # nothing declared by the user
            "total-overhaul",
        )

        assert any("[ORPHAN]" in w and "Stray.esp" in w for w in warnings)
        assert not any("Curated.esp" in w and "[ORPHAN]" in w for w in warnings)

    def test_a_declared_plugin_is_not_flagged_orphan(self, capsys) -> None:
        entries = [_entry("Curated.esp", on_lists=["total-overhaul"])]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Curated.esp", "MyMod.esp"],
            ["Curated.esp", "MyMod.esp"],
            entries,
            warnings,
            {"curated.esp"},
            ["Curated.esp"],
            {"mymod.esp"},  # the user declared this one themselves
            "total-overhaul",
        )

        assert warnings == []

    def test_no_list_name_means_no_orphan_check_even_with_a_curated_set(self, capsys) -> None:
        entries = [_entry("Curated.esp", on_lists=["x"])]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Stray.esp"], ["Stray.esp"], entries, warnings, {"curated.esp"}, [], set(), None
        )

        assert warnings == []

    def test_an_empty_curated_set_means_no_orphan_check_even_with_a_list_name(self, capsys) -> None:
        entries = [_entry("A.esp")]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Stray.esp"], ["Stray.esp"], entries, warnings, set(), [], set(), "total-overhaul"
        )

        assert warnings == []


class TestOrderDrift:
    def test_base_order_matches_yml_findings_are_folded_in(self, capsys) -> None:
        entries = [_entry("First.esp", on_lists=["x"]), _entry("Second.esp", on_lists=["x"])]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Second.esp", "First.esp"],  # cfg has them backwards
            ["Second.esp", "First.esp"],
            entries,
            warnings,
            {"first.esp", "second.esp"},
            ["First.esp", "Second.esp"],  # canonical order
            set(),
            "total-overhaul",
        )

        assert any("[LIST ORDER]" in w for w in warnings)


class TestReporting:
    def test_all_findings_share_one_warnings_list_and_get_printed(self, capsys) -> None:
        entries = [_entry("Dirty.esp", needs_cleaning=True, on_lists=["x"])]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Dirty.esp", "Stray.esp"],
            ["Dirty.esp", "Stray.esp"],
            entries,
            warnings,
            {"dirty.esp"},
            ["Dirty.esp"],
            set(),
            "total-overhaul",
        )

        out = capsys.readouterr().out
        assert f"{len(warnings)} PLUGIN-ORDER.YML WARNING(S)" in out
        assert "[NEEDS CLEANING]" in out
        assert "[ORPHAN]" in out

    def test_the_caller_supplied_list_is_mutated_in_place(self, capsys) -> None:
        entries = [_entry("Dirty.esp", needs_cleaning=True)]
        warnings: list[str] = []

        core._yml_post_sort_warnings(
            ["Dirty.esp"], ["Dirty.esp"], entries, warnings, set(), [], set(), None
        )

        # Not a copy -- the same list object the caller passed in.
        assert len(warnings) == 1
