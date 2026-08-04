"""The patch queue: what survives being changed your mind about.

Queuing a record is a decision, and decisions get revisited -- you pick a
winner, look at three more conflicts, then realise the first one should have
taken one field from somewhere else. The Patch Builder exists so that is
possible; these are the rules it enforces, tested without a display because
they are data rules rather than drawing ones.
"""

from __future__ import annotations

from typing import Final

from wraithguard.patch import FieldChoice, Selection
from wraithguard.patch.queue import PatchQueue, base_from_conflicts

#: A conflict list shaped like the one the window reads.
SHOWN: Final[list[dict]] = [
    {"type": "Cell", "id": "(7, 22)", "plugins": ["Castle.esp", "Other.esp"]},
    {"type": "Static", "id": "rock", "plugins": ["A.esp", "B.esp"]},
]


def base(record_type: str, key: str) -> str:
    """The plugin that currently wins, from the sample conflict list."""
    return base_from_conflicts(SHOWN, record_type, key)


class TestChangingYourMind:
    """Re-deciding replaces; it never accumulates."""

    def test_a_second_whole_choice_replaces_the_first(self) -> None:
        """Otherwise the patch would carry it twice and decide for you."""
        queue = PatchQueue()
        queue.add_whole(Selection("A.esp", "Static", "rock"))
        queue.add_whole(Selection("B.esp", "Static", "rock"))
        assert queue.selections == [Selection("B.esp", "Static", "rock")]

    def test_a_second_choice_for_one_field_replaces_it(self) -> None:
        """Same rule one level down."""
        queue = PatchQueue()
        queue.add_field("Cell", "(7, 22)", FieldChoice("data.flags", "Other.esp"))
        queue.add_field("Cell", "(7, 22)", FieldChoice("data.flags", "Castle.esp"))
        assert queue.fields[("Cell", "(7, 22)")] == [FieldChoice("data.flags", "Castle.esp")]

    def test_different_fields_accumulate(self) -> None:
        """That is the whole point of merging down."""
        queue = PatchQueue()
        queue.add_field("Cell", "(7, 22)", FieldChoice("data.flags", "Other.esp"))
        queue.add_field("Cell", "(7, 22)", FieldChoice("region", "Other.esp"))
        assert len(queue.fields[("Cell", "(7, 22)")]) == 2


class TestWholeAndMergedAreExclusive:
    """A record is taken whole or merged, never both."""

    def test_choosing_whole_drops_the_merge(self) -> None:
        """The later decision is the one meant."""
        queue = PatchQueue()
        queue.add_field("Static", "rock", FieldChoice("model", "B.esp"))
        queue.add_whole(Selection("A.esp", "Static", "rock"))
        assert ("Static", "rock") not in queue.fields
        assert len(queue) == 1

    def test_choosing_a_field_drops_the_whole_record(self) -> None:
        """And the same the other way round."""
        queue = PatchQueue()
        queue.add_whole(Selection("A.esp", "Static", "rock"))
        queue.add_field("Static", "rock", FieldChoice("model", "B.esp"))
        assert queue.selections == []
        assert len(queue) == 1


class TestTheBaseIsWhatCurrentlyWins:
    """So a merge reads as departures from what the load order already does."""

    def test_the_last_plugin_is_the_base(self) -> None:
        """Last in load order is what the game gives you today."""
        assert base("Cell", "(7, 22)") == "Other.esp"

    def test_a_record_no_longer_scanned_has_no_base(self) -> None:
        """Reported rather than guessed: the queue may outlive a rescan."""
        assert base("Cell", "(99, 99)") == ""

    def test_merges_carry_every_plugin_they_read(self) -> None:
        """Base and each field's source, or the references cannot be renumbered."""
        queue = PatchQueue()
        queue.add_field("Cell", "(7, 22)", FieldChoice("data.flags", "Castle.esp"))
        merge = queue.merges(base)[0]
        assert merge.plugins == {"Other.esp", "Castle.esp"}


class TestCounting:
    """The number on the button and in the summary."""

    def test_both_kinds_count(self) -> None:
        """One record each, however it was chosen."""
        queue = PatchQueue()
        queue.add_whole(Selection("A.esp", "Static", "rock"))
        queue.add_field("Cell", "(7, 22)", FieldChoice("region", "Other.esp"))
        assert len(queue) == 2

    def test_an_empty_queue_counts_nothing(self) -> None:
        """And the write button stays disabled on it."""
        assert len(PatchQueue()) == 0
