"""Rebuilding the order the engine will read a topic in.

The engine reads a topic top down and speaks the first response whose filters
match, so position is priority. Each response names the one it follows, and the
list is rebuilt plugin by plugin in load order.

The rule that does the damage is the one for a broken chain: a response whose
declared predecessor is nowhere to be found goes to the **end** of the topic.
It does not error and it does not vanish from the file -- it is simply tested
last, after everything else has had a chance to match, which for a greeting or
a filtered line usually means never.

That is why an author inserting a line mid-topic drags the neighbouring
responses in unchanged: those copies are what keep the chain findable.
"""

from __future__ import annotations

from typing import Any

from wraithguard.patch.dialogue import (
    Response,
    moved,
    orphans,
    positions,
    responses_by_topic,
    shifts,
    topic_order,
)


def keys(order: list) -> list[str]:
    """The resolved topic as a list of ids, for readable assertions."""
    return [placed.key for placed in order]


class TestBuildingTheOrder:
    """One plugin, no overrides: the ordinary case."""

    def test_a_chain_resolves_in_order(self) -> None:
        """Each response follows the one it names."""
        order = topic_order(
            [
                Response("a"),
                Response("b", prev="a"),
                Response("c", prev="b"),
            ]
        )
        assert keys(order) == ["a", "b", "c"]

    def test_an_empty_predecessor_means_the_top(self) -> None:
        """ "Place this at the top of the greetings" is exactly this case."""
        order = topic_order([Response("a"), Response("top", prev="")])
        assert keys(order) == ["top", "a"]

    def test_a_line_inserted_mid_topic_lands_mid_topic(self) -> None:
        """The whole reason positions can be authored at all."""
        order = topic_order(
            [
                Response("a"),
                Response("b", prev="a"),
                Response("new", prev="a"),
            ]
        )
        assert keys(order) == ["a", "new", "b"]

    def test_nothing_at_all_resolves_to_nothing(self) -> None:
        """An empty topic is not an error."""
        assert topic_order([]) == []


class TestABrokenChainGoesLast:
    """The failure mode, and the reason the anchors exist."""

    def test_an_unknown_predecessor_sends_a_line_to_the_end(self) -> None:
        """Not an error, not a removal -- just last, and so tested last."""
        order = topic_order(
            [
                Response("a"),
                Response("b", prev="a"),
                Response("lost", prev="nowhere"),
            ]
        )
        assert keys(order) == ["a", "b", "lost"]

    def test_the_orphan_is_named(self) -> None:
        """Reporting it is the difference between "may be wrong" and "is last"."""
        order = topic_order([Response("a"), Response("lost", prev="nowhere")])
        assert orphans(order) == ["lost"]

    def test_a_whole_chain_is_not_orphaned(self) -> None:
        """No false alarms, or the real one stops being read."""
        order = topic_order([Response("a"), Response("b", prev="a")])
        assert orphans(order) == []

    def test_the_first_response_is_not_an_orphan(self) -> None:
        """It declares no predecessor, which is not the same as a missing one."""
        assert orphans(topic_order([Response("a")])) == []


class TestOverridesFromLaterPlugins:
    """Several plugins defining the same response, in load order."""

    def test_redefining_without_moving_keeps_the_place(self) -> None:
        """Editing a line's text must not reorder the topic."""
        order = topic_order(
            [
                Response("a", plugin="One.esp"),
                Response("b", prev="a", plugin="One.esp"),
                Response("b", prev="a", plugin="Two.esp"),
            ]
        )
        assert keys(order) == ["a", "b"]

    def test_every_plugin_defining_a_response_is_recorded(self) -> None:
        """The last one is what the game uses, so the order of these matters."""
        order = topic_order(
            [
                Response("a", plugin="One.esp"),
                Response("a", plugin="Two.esp"),
            ]
        )
        assert order[0].plugins == ["One.esp", "Two.esp"]

    def test_relinking_a_response_moves_it(self) -> None:
        """A later plugin saying "this follows c now" is obeyed."""
        order = topic_order(
            [
                Response("a"),
                Response("b", prev="a"),
                Response("c", prev="b"),
                Response("b", prev="c", plugin="Later.esp"),
            ]
        )
        assert keys(order) == ["a", "c", "b"]

    def test_a_move_to_the_top_is_obeyed(self) -> None:
        """Emptying ``prev_id`` is how a mod claims priority in a greeting."""
        order = topic_order(
            [
                Response("a"),
                Response("b", prev="a"),
                Response("b", prev="", plugin="Later.esp"),
            ]
        )
        assert keys(order) == ["b", "a"]


class TestSayingHowFarSomethingMoved:
    """The point of the module: a number, not a warning."""

    def test_positions_are_one_based(self) -> None:
        """Because they are read out to a person, not indexed into."""
        assert positions(topic_order([Response("a"), Response("b", prev="a")])) == {"a": 1, "b": 2}

    def test_a_move_is_reported_with_both_positions(self) -> None:
        """ "Fourth becomes ninth" is actionable; "may have moved" is not."""
        before = topic_order([Response("a"), Response("b", prev="a"), Response("c", prev="b")])
        after = topic_order(
            [
                Response("a"),
                Response("b", prev="a"),
                Response("c", prev="b"),
                Response("c", prev="", plugin="Patch.esp"),
            ]
        )
        assert moved(before, after) == [("c", 3, 1), ("a", 1, 2), ("b", 2, 3)]

    def test_an_unchanged_topic_reports_nothing(self) -> None:
        """Silence has to mean something, so it must be reliable."""
        order = [Response("a"), Response("b", prev="a")]
        assert moved(topic_order(order), topic_order(order)) == []

    def test_a_response_that_only_appears_afterwards_is_not_a_move(self) -> None:
        """Adding a line is not moving it, and conflating them buries the moves."""
        before = topic_order([Response("a")])
        after = topic_order([Response("a"), Response("new", prev="a")])
        assert moved(before, after) == []

    def test_a_line_pushed_to_the_end_shows_as_the_move_it_is(self) -> None:
        """The broken-chain case, measured rather than warned about."""
        before = topic_order([Response("a"), Response("mid", prev="a"), Response("z", prev="mid")])
        after = topic_order(
            [
                Response("a"),
                Response("mid", prev="a"),
                Response("z", prev="mid"),
                Response("mid", prev="gone", plugin="Patch.esp"),
            ]
        )
        assert ("mid", 2, 3) in moved(before, after)


class TestGroupingResponsesUnderTheirTopics:
    """A response belongs to the last ``Dialogue`` read before it."""

    def _records(self) -> list[dict[str, Any]]:
        """Two topics with two responses each, in file order."""
        return [
            {"type": "Dialogue", "id": "Greeting 5"},
            {"type": "DialogueInfo", "id": "g1", "prev_id": ""},
            {"type": "DialogueInfo", "id": "g2", "prev_id": "g1"},
            {"type": "Dialogue", "id": "new shoes"},
            {"type": "DialogueInfo", "id": "s1", "prev_id": ""},
        ]

    def test_each_response_lands_under_its_own_topic(self) -> None:
        """Attaching one to the wrong topic silently rewrites two topics."""
        got = responses_by_topic(self._records(), "Mod.esp")
        assert {topic: [r.key for r in rows] for topic, rows in got.items()} == {
            "Greeting 5": ["g1", "g2"],
            "new shoes": ["s1"],
        }

    def test_the_plugin_is_recorded_on_every_response(self) -> None:
        """The resolver uses it to say which file supplies the winning line."""
        got = responses_by_topic(self._records(), "Mod.esp")
        assert all(r.plugin == "Mod.esp" for rows in got.values() for r in rows)

    def test_a_response_before_any_topic_is_dropped(self) -> None:
        """An orphan in the source is the source's problem. Inventing a topic
        for it here would hide exactly the defect worth seeing.
        """
        got = responses_by_topic([{"type": "DialogueInfo", "id": "loose"}], "Mod.esp")
        assert got == {}

    def test_records_without_dialogue_produce_nothing(self) -> None:
        """Most plugins have no dialogue at all."""
        assert responses_by_topic([{"type": "Static", "id": "rock"}]) == {}


class TestWhatAPatchMovesAcrossAWholeLoadOrder:
    """``shifts``: resolve each affected topic with and without the patch."""

    def _sources(self) -> dict[str, list[dict[str, Any]]]:
        """Two plugins, one shared topic."""
        return {
            "Base.esm": [
                {"type": "Dialogue", "id": "rumours"},
                {"type": "DialogueInfo", "id": "a", "prev_id": ""},
                {"type": "DialogueInfo", "id": "b", "prev_id": "a"},
            ],
            "Mod.esp": [
                {"type": "Dialogue", "id": "rumours"},
                {"type": "DialogueInfo", "id": "c", "prev_id": "b"},
            ],
        }

    def test_a_patch_that_moves_a_line_reports_it(self) -> None:
        """Carrying ``c`` with an emptied ``prev_id`` puts it first."""
        carried: list[dict[str, Any]] = [
            {"type": "Dialogue", "id": "rumours"},
            {"type": "DialogueInfo", "id": "c", "prev_id": ""},
        ]
        got = shifts(self._sources(), ["Base.esm", "Mod.esp"], carried)
        assert ("rumours", "a", 1, 2) in got
        assert ("rumours", "c", 3, 1) in got

    def test_a_patch_that_moves_nothing_reports_nothing(self) -> None:
        """Silence must be reliable or the notes get skimmed."""
        carried: list[dict[str, Any]] = [
            {"type": "Dialogue", "id": "rumours"},
            {"type": "DialogueInfo", "id": "c", "prev_id": "b"},
        ]
        assert shifts(self._sources(), ["Base.esm", "Mod.esp"], carried) == []

    def test_a_topic_no_source_has_is_skipped(self) -> None:
        """There is no "before" to compare against, so there is no move."""
        carried: list[dict[str, Any]] = [
            {"type": "Dialogue", "id": "brand new"},
            {"type": "DialogueInfo", "id": "x", "prev_id": ""},
        ]
        assert shifts(self._sources(), ["Base.esm", "Mod.esp"], carried) == []

    def test_a_plugin_missing_from_the_sources_is_skipped(self) -> None:
        """The load order can name files the scan did not read."""
        got = shifts(self._sources(), ["Base.esm", "Gone.esp", "Mod.esp"], [])
        assert got == []

    def test_load_order_decides_the_before_picture(self) -> None:
        """Order is the input everywhere in this package, including here.

        Both plugins define ``x``, disagreeing about where it sits, so which is
        read last decides the topic -- and therefore what counts as a move. The
        first version of this test used the fixture above and passed for the
        wrong reason: that arrangement resolves to the same order either way,
        so it would not have caught the argument being ignored.
        """
        sources: dict[str, list[dict[str, Any]]] = {
            "Base.esm": [
                {"type": "Dialogue", "id": "rumours"},
                {"type": "DialogueInfo", "id": "x", "prev_id": ""},
                {"type": "DialogueInfo", "id": "y", "prev_id": "x"},
            ],
            "Mod.esp": [
                {"type": "Dialogue", "id": "rumours"},
                {"type": "DialogueInfo", "id": "x", "prev_id": "y"},
            ],
        }
        carried: list[dict[str, Any]] = [
            {"type": "Dialogue", "id": "rumours"},
            {"type": "DialogueInfo", "id": "x", "prev_id": ""},
        ]
        forward = shifts(sources, ["Base.esm", "Mod.esp"], carried)
        backward = shifts(sources, ["Mod.esp", "Base.esm"], carried)
        # Base last leaves x already at the top, so carrying it moves nothing.
        assert backward == []
        # Mod last had pushed x below y, so carrying it moves both.
        assert forward == [("rumours", "x", 2, 1), ("rumours", "y", 1, 2)]
