"""Chained inserts versus one ``insertBlock``: do they produce the same cfg?

The emitter writes one ``[[Customizations.insert]]`` per plugin, each anchored
on the plugin inserted before it. momw-configurator also accepts ``insertBlock``
-- a multi-line string inserted after a single anchor, one line per entry, in
order.

Switching changes every TOML this tool emits, so "the tests still pass" is not
enough: the question is whether the two forms *apply identically*. This module
answers it by running both through
:func:`~mlox_subset.configurator.simulate_configurator_apply` -- the faithful
re-implementation of the Configurator's ``cfg/custom.go`` -- against the same
cfg and comparing the result line for line.

It also pins the reason the change is worth making rather than merely tidier.
``after`` matches by **substring**, and the Go code treats more than one match
as fatal for the whole run. Chaining on plugin names gives one such chance per
plugin; a single block gives one, on an anchor already known to be unique.
"""

from __future__ import annotations

import pytest

from mlox_subset.configurator import simulate_configurator_apply
from mlox_subset.configurator.emit import _anchor_is_unique, _pick_anchor, _subset_runs

#: A cfg shaped like a real one: a curated block of content, then data paths.
CFG = [
    'data="C:/games/Morrowind/Data Files"',
    "content=Morrowind.esm",
    "content=Tribunal.esm",
    "content=Bloodmoon.esm",
    "content=Patch for Purists.esp",
]


def chained_toml(anchor: str, values: list[str], list_name: str = "total-overhaul") -> str:
    """Render the insert form this tool emits today.

    Each entry is anchored on the one inserted before it, so the run depends on
    every previously inserted line being findable and unambiguous.

    Args:
        anchor: The line the first insert attaches to.
        values: The values to insert, in order.
        list_name: The customization's list name.

    Returns:
        TOML text.
    """
    blocks = [f"[[Customizations]]\nlistName = '{list_name}'\n"]
    previous = anchor
    for value in values:
        blocks.append(f"[[Customizations.insert]]\ninsert = '{value}'\nafter = '{previous}'\n")
        previous = value
    return "\n".join(blocks)


def block_toml(anchor: str, values: list[str], list_name: str = "total-overhaul") -> str:
    """Render the same insertions as a single ``insertBlock``.

    Args:
        anchor: The line the block attaches to.
        values: The values to insert, in order.
        list_name: The customization's list name.

    Returns:
        TOML text.
    """
    body = "\n".join(values)
    return (
        f"[[Customizations]]\nlistName = '{list_name}'\n\n"
        f'[[Customizations.insert]]\ninsertBlock = """{body}"""\nafter = \'{anchor}\'\n'
    )


def applied(toml_text: str, cfg: list[str] | None = None) -> tuple[list[str] | None, list[str]]:
    """Apply a TOML to a cfg the way the Configurator would.

    Args:
        toml_text: The customizations.
        cfg: The starting cfg lines; the module's sample by default.

    Returns:
        The resulting lines (``None`` when the run aborts) and any errors.
    """
    lines, errors, _notes = simulate_configurator_apply(
        list(cfg if cfg is not None else CFG), toml_text, "total-overhaul"
    )
    return lines, errors


class TestTheTwoFormsAgree:
    """The property that has to hold before the emitter can change."""

    @pytest.mark.parametrize(
        "values",
        [
            ["MyNewQuest.esp"],
            ["A.esp", "B.esp"],
            ["A.esp", "B.esp", "C.esp", "D.esp", "E.esp"],
        ],
    )
    def test_content_inserts_produce_the_same_cfg(self, values: list[str]) -> None:
        """One plugin, two, or a handful -- the same lines either way.

        Args:
            values: The plugins to insert.
        """
        anchor = "Patch for Purists.esp"
        chained, chained_errs = applied(chained_toml(anchor, values))
        block, block_errs = applied(block_toml(anchor, values))

        assert chained_errs == [] and block_errs == []
        assert chained == block

    def test_order_is_preserved(self) -> None:
        """A block inserts top-to-bottom, which is what chaining achieved."""
        values = ["First.esp", "Second.esp", "Third.esp"]
        lines, errors = applied(block_toml("Patch for Purists.esp", values))

        assert errors == []
        assert lines is not None
        assert [line for line in lines if line.startswith("content=")] == [
            "content=Morrowind.esm",
            "content=Tribunal.esm",
            "content=Bloodmoon.esm",
            "content=Patch for Purists.esp",
            "content=First.esp",
            "content=Second.esp",
            "content=Third.esp",
        ]

    def test_the_prefix_comes_from_the_anchor(self) -> None:
        """Anchoring on a data= line inserts data= lines, not content=."""
        values = ["C:/mods/MyMod", "C:/mods/OtherMod"]
        lines, errors = applied(block_toml("C:/games/Morrowind/Data Files", values))

        assert errors == []
        assert lines is not None
        assert 'data="C:/mods/MyMod"' in lines or "data=C:/mods/MyMod" in lines
        assert not any(line.startswith("content=C:/mods") for line in lines)

    def test_data_inserts_produce_the_same_cfg(self) -> None:
        """The data half of an export, both ways."""
        anchor = "C:/games/Morrowind/Data Files"
        values = ["C:/mods/MyMod", "C:/mods/OtherMod"]
        chained, chained_errs = applied(chained_toml(anchor, values))
        block, block_errs = applied(block_toml(anchor, values))

        assert chained_errs == [] and block_errs == []
        assert chained == block

    def test_an_empty_block_changes_nothing(self) -> None:
        """Nothing to insert must not corrupt the file or error."""
        lines, _errors = applied(block_toml("Patch for Purists.esp", []))

        assert lines is None or [line for line in lines if line.startswith("content=")] == [
            "content=Morrowind.esm",
            "content=Tribunal.esm",
            "content=Bloodmoon.esm",
            "content=Patch for Purists.esp",
        ]


class TestWhyTheBlockFormIsSafer:
    """Not merely tidier: chaining multiplies an existing failure mode.

    ``after`` matches by **substring** and more than one match aborts the whole
    run -- the Go code returns a nil cfg. Chaining anchors each insert on the
    *previously inserted plugin name*, so every plugin is another anchor that
    can turn out to be ambiguous. A block anchors once.

    The first attempt at this test asserted the wrong construction and passed
    for the wrong reason, which the harness caught: the collision does not come
    from two inserted names, it comes from an inserted name being a substring of
    a line **already in the cfg**. ``Wares.esp`` and ``Better Wares.esp`` is an
    ordinary Morrowind pairing, and the second contains the first.
    """

    #: The curated list already ships "Better Wares.esp"; the user adds
    #: "Wares.esp". Chaining then anchors on a name that matches both lines.
    SUBSTRING_CFG = [
        'data="C:/games/Morrowind/Data Files"',
        "content=Bloodmoon.esm",
        "content=Better Wares.esp",
    ]

    VALUES = ["Wares.esp", "Next.esp"]

    def test_chaining_through_an_ambiguous_name_aborts_the_run(self) -> None:
        """The whole cfg is abandoned, not just the one insert."""
        lines, errors = applied(chained_toml("Bloodmoon.esm", self.VALUES), self.SUBSTRING_CFG)

        assert lines is None, "expected the ambiguous chained anchor to abort"
        assert any("multiple matches" in err.lower() for err in errors), errors
        assert any("Wares.esp" in err for err in errors), errors

    def test_the_block_form_survives_the_same_load_order(self) -> None:
        """One anchor, and it is one we can check is unique."""
        lines, errors = applied(block_toml("Bloodmoon.esm", self.VALUES), self.SUBSTRING_CFG)

        assert errors == [], errors
        assert lines is not None
        assert [line for line in lines if line.startswith("content=")] == [
            "content=Bloodmoon.esm",
            "content=Wares.esp",
            "content=Next.esp",
            "content=Better Wares.esp",
        ]

    def test_an_ambiguous_first_anchor_still_aborts_either_way(self) -> None:
        """The block form is not a cure, and should not be sold as one.

        If the anchor the emitter picks is itself ambiguous, both forms fail
        identically. What the block form removes is the *additional* exposure
        from chaining, not the exposure itself.
        """
        cfg = [
            'data="C:/games/Morrowind/Data Files"',
            "content=Bloodmoon.esm",
            "content=Bloodmoon.esm Patch.esp",
        ]
        chained, chained_errs = applied(chained_toml("Bloodmoon.esm", ["A.esp"]), cfg)
        block, block_errs = applied(block_toml("Bloodmoon.esm", ["A.esp"]), cfg)

        assert chained is None and block is None
        assert chained_errs == block_errs


class TestRunGrouping:
    """Which plugins share a block."""

    def test_consecutive_subset_plugins_form_one_run(self):
        """The common case: everything the user added sorts together."""
        order = ["Morrowind.esm", "A.esp", "B.esp", "C.esp"]

        assert _subset_runs(order, {"a.esp", "b.esp", "c.esp"}, set()) == [(1, 4)]

    def test_a_curated_plugin_between_two_customs_splits_the_run(self):
        """Two blocks, because a frozen plugin sits between them."""
        order = ["A.esp", "Curated.esp", "B.esp"]

        assert _subset_runs(order, {"a.esp", "b.esp"}, set()) == [(0, 1), (2, 3)]

    def test_a_replace_target_breaks_a_run(self):
        """It is emitted as a replace, so it must not appear in an insert too."""
        order = ["A.esp", "Swapped.esp", "B.esp"]

        runs = _subset_runs(order, {"a.esp", "swapped.esp", "b.esp"}, {"Swapped.esp"})

        assert runs == [(0, 1), (2, 3)]

    def test_no_subset_plugins_means_no_runs(self):
        """A sort that inserted nothing must emit no insert blocks."""
        assert _subset_runs(["Morrowind.esm"], set(), set()) == []


class TestAnchorSelection:
    """The block's single anchor is chosen, so it can be chosen well.

    An ambiguous anchor is fatal to the whole Configurator run, and the run's
    position is identical whether it is expressed as ``after`` the line before
    or ``before`` the line after -- so there is no cost to preferring the one
    that is unique.
    """

    #: 'Wares.esp' is contained in 'Better Wares.esp', which is what makes the
    #: obvious anchor ambiguous.
    ORDER = ["Bloodmoon.esm", "Better Wares.esp", "Wares.esp", "Mine.esp", "Tail.esp"]

    @property
    def haystack(self):
        """The cfg lines anchors are matched against."""
        return [f"content={n}" for n in self.ORDER]

    def test_the_natural_anchor_is_used_when_it_is_unique(self):
        """Nothing clever when nothing is wrong: after the preceding line."""
        order = ["Morrowind.esm", "Patch.esp", "Mine.esp"]
        haystack = [f"content={n}" for n in order]

        assert _pick_anchor(order, 2, 3, haystack) == ("after", "Patch.esp")

    def test_an_ambiguous_predecessor_falls_back_to_before(self):
        """'Wares.esp' matches two lines, so anchor on the following one."""
        assert _pick_anchor(self.ORDER, 3, 4, self.haystack) == ("before", "Tail.esp")

    def test_both_neighbours_ambiguous_emits_the_natural_one(self):
        """Dropping the insert would be worse than a rebuild that says why.

        The existing ambiguity warning still fires, so the user is told.
        """
        order = ["A.esp", "XA.esp", "Mine.esp", "B.esp", "XB.esp"]
        haystack = [f"content={n}" for n in order]

        mode, anchor = _pick_anchor(order, 2, 3, haystack)

        assert (mode, anchor) == ("after", "XA.esp")

    def test_a_run_covering_the_whole_order_has_no_anchor(self):
        """There is nothing to attach to; the emitter writes a warning instead."""
        assert _pick_anchor(["Mine.esp"], 0, 1, ["content=Mine.esp"]) == ("after", None)

    def test_uniqueness_is_substring_based_like_the_configurator(self):
        """Matching the Go semantics is the whole point of the check."""
        haystack = ["content=Wares.esp", "content=Better Wares.esp"]

        assert not _anchor_is_unique("Wares.esp", haystack)
        assert _anchor_is_unique("Better Wares.esp", haystack)
