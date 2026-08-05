"""Conflict status on two axes: what a record suffers, and what a file does.

The distinction is the point. "These differ" makes the reader do the work; the
two questions a modder actually has are whether anything is being *lost*, and
what *this* file is doing about it, and those have different answers in the
same record all the time.

The judgement in both cases is one rule: with the versions in load order, the
first is the master's and the last is what the game uses, and nothing is lost
so long as every version equals one or the other. The moment one is neither,
somebody's edit is being discarded with no error and no log line.
"""

from __future__ import annotations

from wraithguard.patch.status import (
    ABSENT,
    ConflictAll,
    ConflictThis,
    conflict_all,
    conflict_this,
    worst_all,
    worst_this,
)


class TestWhatHappensToTheRecord:
    """:class:`ConflictAll` -- the question "is anything being lost here?"."""

    def test_one_definition_cannot_conflict(self) -> None:
        """The common case by a wide margin."""
        assert conflict_all(["a"]) is ConflictAll.ONLY_ONE

    def test_nothing_at_all_is_still_only_one(self) -> None:
        """An empty column should not read as a conflict."""
        assert conflict_all([]) is ConflictAll.ONLY_ONE

    def test_agreement_is_not_a_conflict(self) -> None:
        """Three files, one value. Nothing to report."""
        assert conflict_all(["a", "a", "a"]) is ConflictAll.NO_CONFLICT

    def test_everyone_agreeing_with_the_winner_loses_nothing(self) -> None:
        """The master is overridden, but by files that agree with each other."""
        assert conflict_all(["a", "b", "b"]) is ConflictAll.OVERRIDE_BENIGN

    def test_a_middle_file_agreeing_with_the_master_loses_nothing(self) -> None:
        """It re-stated the master's value; the last file changed it. No loss."""
        assert conflict_all(["a", "a", "b"]) is ConflictAll.OVERRIDE_BENIGN

    def test_a_third_value_is_a_real_conflict(self) -> None:
        """``b`` is neither the master's nor the winner's: that edit is gone."""
        assert conflict_all(["a", "b", "c"]) is ConflictAll.CONFLICT

    def test_the_order_is_the_input(self) -> None:
        """Same values, different load order, different answer -- by design."""
        assert conflict_all(["a", "b", "b"]) is ConflictAll.OVERRIDE_BENIGN
        assert conflict_all(["b", "b", "a"]) is ConflictAll.OVERRIDE_BENIGN
        assert conflict_all(["b", "a", "b"]) is ConflictAll.CONFLICT


class TestAbsenceIsNotEmptiness:
    """A field a record does not have is not a field set to nothing.

    yampt needed a magic byte string to say this because its values were C++
    strings. Keeping it a distinct object means no real value can collide with
    it, and the two readings stay available: absence as a value, or absence as
    "this file has nothing to say".
    """

    def test_absence_counted_as_a_value_is_a_change(self) -> None:
        """Removing a field is an edit, and by default it reads as one."""
        assert conflict_all(["a", ABSENT]) is ConflictAll.OVERRIDE_BENIGN

    def test_absence_skipped_leaves_only_one_definition(self) -> None:
        """Right for optional subrecords, where absent means "not mentioned"."""
        assert conflict_all(["a", ABSENT], skip_absent=True) is ConflictAll.ONLY_ONE

    def test_skipping_absence_moves_who_counts_as_master(self) -> None:
        """The first file that says anything is the one being overridden."""
        assert conflict_all([ABSENT, "a", "a"], skip_absent=True) is ConflictAll.NO_CONFLICT

    def test_skipping_absence_moves_who_counts_as_winner(self) -> None:
        """A trailing absence must not be mistaken for the winning value."""
        assert conflict_all(["a", "b", ABSENT], skip_absent=True) is ConflictAll.OVERRIDE_BENIGN

    def test_absent_everywhere_is_only_one(self) -> None:
        """No file mentions it, so there is nothing to judge."""
        assert conflict_all([ABSENT, ABSENT], skip_absent=True) is ConflictAll.ONLY_ONE

    def test_the_sentinel_names_itself(self) -> None:
        """A failing assertion should read, not print an object address."""
        assert repr(ABSENT) == "ABSENT"


class TestWhatEachFileIsDoing:
    """:class:`ConflictThis` -- one status per file, in load order."""

    def test_the_first_file_is_the_master(self) -> None:
        """Everything after it is an override of it."""
        assert conflict_this(["a", "b"])[0] is ConflictThis.MASTER

    def test_a_lone_definition_is_just_the_master(self) -> None:
        """Nothing overrides it, so nothing else to say."""
        assert conflict_this(["a"]) == [ConflictThis.MASTER]

    def test_restating_the_master_is_identical_to_master(self) -> None:
        """The ITM record. Usually junk -- but not always, see the anchors."""
        assert conflict_this(["a", "a"])[1] is ConflictThis.IDENTICAL_TO_MASTER

    def test_an_uncontested_change_wins(self) -> None:
        """One file changed it and nothing later disagreed."""
        assert conflict_this(["a", "b"])[1] is ConflictThis.OVERRIDE_WINS

    def test_the_loser_is_named_and_so_is_the_winner(self) -> None:
        """The status worth hunting for: ``b``'s edit never reaches the game."""
        assert conflict_this(["a", "b", "c"]) == [
            ConflictThis.MASTER,
            ConflictThis.CONFLICT_LOSES,
            ConflictThis.CONFLICT_WINS,
        ]

    def test_agreeing_with_the_winner_is_not_losing(self) -> None:
        """Two files made the same change; neither lost anything."""
        assert conflict_this(["a", "b", "b"])[1] is ConflictThis.OVERRIDE_WINS

    def test_an_absent_file_says_nothing_when_skipped(self) -> None:
        """It gets no status rather than a guessed one."""
        assert conflict_this(["a", ABSENT, "b"], skip_absent=True)[1] is ConflictThis.UNKNOWN

    def test_absent_where_the_master_was_absent_is_no_change(self) -> None:
        """Both say nothing, so the second changed nothing."""
        got = conflict_this([ABSENT, ABSENT])
        assert got[1] is ConflictThis.IDENTICAL_TO_MASTER

    def test_a_file_lacking_the_field_is_not_its_master(self) -> None:
        """Being first is not authority over a field you do not have."""
        assert conflict_this([ABSENT, "a"])[0] is ConflictThis.UNKNOWN

    def test_every_file_gets_exactly_one_status(self) -> None:
        """The list is positional; a short one would mislabel the tree."""
        assert len(conflict_this(["a", "b", "c", "d"])) == 4


class TestRollingUp:
    """A plugin is coloured by the worst thing in it, on each axis."""

    def test_a_group_takes_its_worst_record(self) -> None:
        """One real conflict makes the whole branch worth opening."""
        assert (
            worst_all([ConflictAll.NO_CONFLICT, ConflictAll.CONFLICT, ConflictAll.ONLY_ONE])
            is ConflictAll.CONFLICT
        )

    def test_an_empty_group_is_not_a_conflict(self) -> None:
        """A filtered-out branch should not light up."""
        assert worst_all([]) is ConflictAll.ONLY_ONE

    def test_losing_outranks_winning(self) -> None:
        """A file whose edit survives needs no attention; one whose edit
        vanished does. Declaration order would have said the opposite.
        """
        assert (
            worst_this([ConflictThis.CONFLICT_WINS, ConflictThis.CONFLICT_LOSES])
            is ConflictThis.CONFLICT_LOSES
        )

    def test_being_the_master_outranks_being_a_copy_of_it(self) -> None:
        """Between two quiet statuses, the meaningful one shows."""
        assert (
            worst_this([ConflictThis.IDENTICAL_TO_MASTER, ConflictThis.MASTER])
            is ConflictThis.MASTER
        )

    def test_an_empty_roll_up_is_unknown(self) -> None:
        """Nothing beneath it means nothing to say about it."""
        assert worst_this([]) is ConflictThis.UNKNOWN

    def test_every_status_can_be_ranked(self) -> None:
        """A missing entry would raise at paint time, in the GUI, on the one
        record that had it. Pinned here instead.
        """
        assert all(worst_this([status]) is status for status in ConflictThis)
