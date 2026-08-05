"""From a field diff to a per-plugin answer.

The scanner says "these disagree", which is the wrong resolution for the
decision being made. Five plugins editing one record with nothing lost looks
identical to five where the third one's work is discarded, and only the second
is worth opening. Rolling the per-field judgement up to the record and then to
the plugin is what turns a list of conflicts into "this mod loses 38 records".
"""

from __future__ import annotations

from typing import Any, Final

from wraithguard.patch.status import ConflictAll, ConflictThis
from wraithguard.patch.summary import (
    ALL_TAGS,
    field_statuses,
    group_by_plugin,
    record_plugin_statuses,
    record_status,
    row_tag_updates,
    survey,
    tally,
)

#: Three plugins editing one record, in load order.
PLUGINS: Final[list[str]] = ["Base.esm", "Mid.esp", "Last.esp"]

#: ``name`` is untouched, ``script`` is contested, ``weight`` is agreed on.
PER: Final[dict[str, dict[str, Any]]] = {
    "Base.esm": {"name": "Iron Cuirass", "script": "", "weight": 30},
    "Mid.esp": {"name": "Iron Cuirass", "script": "mid_script", "weight": 18},
    "Last.esp": {"name": "Iron Cuirass", "script": "last_script", "weight": 18},
}

KEYS: Final[list[str]] = ["name", "script", "weight"]


def statuses() -> list:
    """The three fields judged."""
    return field_statuses(KEYS, PER, PLUGINS)


class TestJudgingEachField:
    """One field at a time, across every plugin defining the record."""

    def test_an_agreed_field_is_no_conflict(self) -> None:
        """All three say the same thing."""
        assert statuses()[0].overall is ConflictAll.NO_CONFLICT

    def test_a_discarded_edit_is_a_conflict(self) -> None:
        """``mid_script`` is neither the original nor the winner. It is gone."""
        assert statuses()[1].overall is ConflictAll.CONFLICT

    def test_two_plugins_agreeing_against_the_master_lose_nothing(self) -> None:
        """Both changed the weight to 18, so neither lost anything."""
        assert statuses()[2].overall is ConflictAll.OVERRIDE_BENIGN

    def test_each_plugin_gets_its_own_verdict_on_the_field(self) -> None:
        """The two axes disagree here, which is the reason there are two."""
        assert statuses()[1].per_plugin == (
            ConflictThis.MASTER,
            ConflictThis.CONFLICT_LOSES,
            ConflictThis.CONFLICT_WINS,
        )

    def test_the_key_is_carried_through(self) -> None:
        """The panel labels rows by it, so it has to survive the judgement."""
        assert [status.key for status in statuses()] == KEYS

    def test_no_fields_judged_is_not_a_crash(self) -> None:
        """An identical record produces an empty diff."""
        assert field_statuses([], PER, PLUGINS) == []


class TestAbsentFields:
    """A field one plugin's record does not have at all."""

    def test_absence_is_read_as_absence_not_as_none(self) -> None:
        """``None`` is a value a record may hold; conflating them would call a
        real edit a no-op, which is the quietest kind of wrong.
        """
        per = {"A.esm": {"x": None}, "B.esp": {}}
        assert (
            field_statuses(["x"], per, ["A.esm", "B.esp"])[0].overall is ConflictAll.OVERRIDE_BENIGN
        )

    def test_absence_can_be_skipped_instead(self) -> None:
        """For optional subrecords, silence is not a deletion."""
        per = {"A.esm": {"x": None}, "B.esp": {}}
        got = field_statuses(["x"], per, ["A.esm", "B.esp"], skip_absent=True)
        assert got[0].overall is ConflictAll.ONLY_ONE

    def test_a_plugin_missing_from_the_diff_is_absent_not_a_crash(self) -> None:
        """The diff is best-effort and may not cover every plugin."""
        got = field_statuses(["x"], {"A.esm": {"x": 1}}, ["A.esm", "Gone.esp"])
        assert got[0].per_plugin[1] is ConflictThis.UNKNOWN


class TestRollingUpARecord:
    """A record is as bad as its worst field."""

    def test_a_record_takes_its_worst_field(self) -> None:
        """One discarded edit makes the record worth opening."""
        assert record_status(statuses()) is ConflictAll.CONFLICT

    def test_a_record_with_no_fields_is_not_a_conflict(self) -> None:
        """Otherwise every identical record would light up."""
        assert record_status([]) is ConflictAll.ONLY_ONE

    def test_each_plugin_takes_its_worst_field(self) -> None:
        """``Mid.esp`` agrees about the name and loses the script."""
        got = record_plugin_statuses(statuses(), PLUGINS)
        assert got["Mid.esp"] is ConflictThis.CONFLICT_LOSES

    def test_the_winner_is_not_reported_as_losing(self) -> None:
        """It wins the script and matches on the rest."""
        got = record_plugin_statuses(statuses(), PLUGINS)
        assert got["Last.esp"] is ConflictThis.CONFLICT_WINS

    def test_every_plugin_is_accounted_for(self) -> None:
        """A missing key would silently drop a mod from the summary."""
        assert set(record_plugin_statuses(statuses(), PLUGINS)) == set(PLUGINS)


class TestTallyingAcrossRecords:
    """The plugin-level view a flat conflict list cannot give."""

    def test_losses_are_counted_per_plugin(self) -> None:
        """ "This mod loses 2 records" is the shape of an actionable answer."""
        counted = tally(
            [
                {"Mid.esp": ConflictThis.CONFLICT_LOSES},
                {"Mid.esp": ConflictThis.CONFLICT_LOSES},
                {"Mid.esp": ConflictThis.OVERRIDE_WINS},
            ]
        )
        assert counted["Mid.esp"].losing == 2

    def test_redundant_redefinitions_are_counted_separately(self) -> None:
        """Not junk by default: an unchanged dialogue copy may hold a position."""
        counted = tally([{"A.esp": ConflictThis.IDENTICAL_TO_MASTER}])
        assert counted["A.esp"].redundant == 1

    def test_a_plugin_with_no_losses_reports_zero_rather_than_missing(self) -> None:
        """A summary row that silently omits a number reads as a bug."""
        counted = tally([{"A.esp": ConflictThis.OVERRIDE_WINS}])
        assert counted["A.esp"].losing == 0

    def test_every_plugin_seen_gets_a_row(self) -> None:
        """Including ones that only ever agree."""
        counted = tally([{"A.esm": ConflictThis.MASTER, "B.esp": ConflictThis.IDENTICAL_TO_MASTER}])
        assert set(counted) == {"A.esm", "B.esp"}

    def test_nothing_to_tally_is_an_empty_summary(self) -> None:
        """Not an error and not a fabricated row."""
        assert tally([]) == {}


class TestSurveyingAWholeScan:
    """Every conflict judged in one pass, then counted per plugin."""

    def _conflict(self, key: str) -> dict[str, Any]:
        """One conflict as the scanner reports it."""
        return {"type": "Armor", "id": key, "plugins": PLUGINS}

    def test_each_record_gets_its_status(self) -> None:
        """Keyed by type and id, because ids collide across types."""
        got = survey([self._conflict("cuirass")], lambda _c: (KEYS, PER))
        assert got.records[("Armor", "cuirass")] is ConflictAll.CONFLICT

    def test_the_plugin_tally_accumulates_across_records(self) -> None:
        """The whole point: a per-mod number, not a per-record one."""
        got = survey(
            [self._conflict("a"), self._conflict("b")],
            lambda _c: (KEYS, PER),
        )
        assert got.plugins["Mid.esp"].losing == 2

    def test_a_record_that_cannot_be_read_is_counted_not_dropped(self) -> None:
        """A summary that quietly omits records is worse than one that admits
        what it could not see.
        """
        got = survey([self._conflict("a")], lambda _c: None)
        assert got.unreadable == 1
        assert got.records == {}

    def test_losers_come_back_worst_first(self) -> None:
        """Which is the order somebody would want to act in."""
        per_plugin = {
            "a": {"Bad.esp": ConflictThis.CONFLICT_LOSES},
            "b": {"Bad.esp": ConflictThis.CONFLICT_LOSES, "Mild.esp": ConflictThis.CONFLICT_LOSES},
        }
        got = survey(
            [{"type": "T", "id": key, "plugins": []} for key in per_plugin],
            lambda c: ([], {}),
        )
        got.plugins = tally(list(per_plugin.values()))
        assert [entry.plugin for entry in got.losing_plugins] == ["Bad.esp", "Mild.esp"]

    def test_plugins_that_never_lose_are_left_out_of_the_losers(self) -> None:
        """Otherwise the list is every mod and says nothing."""
        got = survey([self._conflict("a")], lambda _c: (["name"], PER))
        assert got.losing_plugins == []

    def test_an_empty_scan_surveys_to_nothing(self) -> None:
        """Not an error and not a fabricated row."""
        got = survey([], lambda _c: None)
        assert got.records == {} and got.plugins == {} and got.unreadable == 0


class TestGroupingByPlugin:
    """The shape a load order has: file, kind of thing, thing.

    The flat list answers "what conflicts". This answers "what does *this mod*
    touch", which is the question actually being asked while deciding what to
    do about a mod.
    """

    def _c(self, kind: str, key: str, *plugins: str) -> dict[str, Any]:
        """One conflict as the scanner reports it."""
        return {"type": kind, "id": key, "plugins": list(plugins)}

    def test_a_plugin_gets_a_branch_for_every_record_it_touches(self) -> None:
        """Including records where it is not the winner."""
        got = group_by_plugin([self._c("Armor", "cuirass", "Base.esm", "Mod.esp")])
        assert {branch.plugin for branch in got} == {"Base.esm", "Mod.esp"}

    def test_records_are_grouped_by_type(self) -> None:
        """A mod that edits 900 cells and 3 weapons should read as exactly that."""
        got = group_by_plugin(
            [
                self._c("Armor", "a", "Mod.esp"),
                self._c("Cell", "(1, 2)", "Mod.esp"),
                self._c("Armor", "b", "Mod.esp"),
            ]
        )
        assert {kind: len(rows) for kind, rows in got[0].groups.items()} == {"Armor": 2, "Cell": 1}

    def test_the_total_is_counted_for_the_branch(self) -> None:
        """Shown on the plugin row, so it must not need recomputing to display."""
        got = group_by_plugin([self._c("Armor", "a", "Mod.esp"), self._c("Cell", "c", "Mod.esp")])
        assert got[0].records == 2

    def test_branches_follow_the_load_order(self) -> None:
        """Order is meaning here as everywhere else: later overrides earlier."""
        got = group_by_plugin(
            [self._c("Armor", "a", "Late.esp", "Early.esm")],
            order=["Early.esm", "Late.esp"],
        )
        assert [branch.plugin for branch in got] == ["Early.esm", "Late.esp"]

    def test_a_plugin_missing_from_the_order_still_appears(self) -> None:
        """A scan can outlive the order it was taken from; dropping it hides work."""
        got = group_by_plugin([self._c("Armor", "a", "Stray.esp")], order=["Other.esm"])
        assert [branch.plugin for branch in got] == ["Stray.esp"]

    def test_the_order_is_matched_case_insensitively(self) -> None:
        """Load orders and record headers disagree about case constantly."""
        got = group_by_plugin(
            [self._c("Armor", "a", "LATE.ESP", "early.esm")],
            order=["Early.esm", "Late.esp"],
        )
        assert [branch.plugin for branch in got] == ["early.esm", "LATE.ESP"]

    def test_an_empty_scan_groups_to_nothing(self) -> None:
        """Not an error and not a fabricated branch."""
        assert group_by_plugin([]) == []


class TestTheDisplayContract:
    """The tag names and wording live with the model, not the window."""

    def test_every_status_has_a_tag_and_an_explanation(self) -> None:
        """A missing entry would raise at paint time, on one record, in the GUI."""
        assert set(ALL_TAGS) == set(ConflictAll)

    def test_the_tags_are_distinct(self) -> None:
        """Two statuses sharing a tag would colour identically and mislead."""
        names = [tag for tag, _ in ALL_TAGS.values()]
        assert len(set(names)) == len(names)

    def test_the_conflict_wording_says_what_is_lost(self) -> None:
        """ "Conflict" alone is what the current viewer already fails to explain."""
        assert "discarded" in ALL_TAGS[ConflictAll.CONFLICT][1]


class TestRowTagUpdates:
    """The pure core the conflict list's paced recolour applies."""

    def _rows(self) -> list[dict[str, Any]]:
        return [
            {"type": "Npc", "id": "bob"},
            {"type": "Armor", "id": "cuirass", "involves_subset": True},
            {"type": "Weapon", "id": "unjudged"},
        ]

    def test_every_judged_row_is_returned_in_order(self) -> None:
        """Position is the row's identity, so order and index must be exact."""
        records = {
            ("Npc", "bob"): ConflictAll.CONFLICT,
            ("Armor", "cuirass"): ConflictAll.OVERRIDE_BENIGN,
        }
        updates = row_tag_updates(self._rows(), records)
        assert [(i, s) for i, s, _ in updates] == [
            (0, ConflictAll.CONFLICT),
            (1, ConflictAll.OVERRIDE_BENIGN),
        ]

    def test_an_unjudged_row_is_left_out_not_cleared(self) -> None:
        """A row the survey never reached keeps whatever tag it had."""
        records = {("Npc", "bob"): ConflictAll.CONFLICT}
        indices = [i for i, _, _ in row_tag_updates(self._rows(), records)]
        assert indices == [0]

    def test_it_carries_the_subset_flag(self) -> None:
        """The subset star must survive recolour, so the paint keeps its tag."""
        records = {
            ("Npc", "bob"): ConflictAll.CONFLICT,
            ("Armor", "cuirass"): ConflictAll.OVERRIDE_BENIGN,
        }
        flags = {i: sub for i, _, sub in row_tag_updates(self._rows(), records)}
        assert flags == {0: False, 1: True}

    def test_no_records_paints_nothing(self) -> None:
        """An unjudged list must not touch a single row."""
        assert row_tag_updates(self._rows(), {}) == []
