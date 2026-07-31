"""Chained inserts versus one ``insertBlock``: do they produce the same cfg?

The emitter writes one ``[[Customizations.insert]]`` per plugin, each anchored
on the plugin inserted before it. momw-configurator also accepts ``insertBlock``
-- a multi-line string inserted after a single anchor, one line per entry, in
order.

Switching changes every TOML this tool emits, so "the tests still pass" is not
enough: the question is whether the two forms *apply identically*. This module
answers it by running both through
:func:`~wraithguard.configurator.simulate_configurator_apply` -- the faithful
re-implementation of the Configurator's ``cfg/custom.go`` -- against the same
cfg and comparing the result line for line.

It also pins the reason the change is worth making rather than merely tidier.
``after`` matches by **substring**, and the Go code treats more than one match
as fatal for the whole run. Chaining on plugin names gives one such chance per
plugin; a single block gives one, on an anchor already known to be unique.
"""

from __future__ import annotations

import pytest

from wraithguard.configurator import simulate_configurator_apply
from wraithguard.configurator.emit import (
    _anchor_is_unique,
    _pick_anchor,
    _subset_runs,
    _widen_anchor,
    generate_customizations_toml,
)

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

    def test_an_ambiguous_predecessor_is_widened_rather_than_abandoned(self):
        """'Wares.esp' matches two lines; 'content=Wares.esp' matches one.

        The natural ``after`` reading is kept, because the ambiguity was in the
        bare name rather than in the line. Widening is preferred over falling
        back to the following plugin: same placement, but the anchor still
        names the plugin the run actually belongs behind.
        """
        assert _pick_anchor(self.ORDER, 3, 4, self.haystack) == ("after", "content=Wares.esp")

    def test_the_widened_anchor_places_the_run_identically(self):
        """Widening must not move anything -- it only disambiguates.

        Checked against the simulator rather than by reasoning, because a wider
        anchor matching a *different* line would place the run somewhere else
        entirely while every unit test still passed.
        """
        cfg = ["content=Better Wares.esp", "content=Wares.esp", "content=Tail.esp"]
        toml = (
            "[[Customizations]]\nlistName = 'total-overhaul'\n\n"
            "[[Customizations.insert]]\ninsert = 'Mine.esp'\nafter = 'content=Wares.esp'\n"
        )
        lines, errors = applied(toml, cfg)

        assert errors == []
        assert lines == [
            "content=Better Wares.esp",
            "content=Wares.esp",
            "content=Mine.esp",
            "content=Tail.esp",
        ]

    def test_both_neighbours_ambiguous_in_every_form_emits_the_natural_one(self):
        """Dropping the insert would be worse than a rebuild that says why.

        Duplicated lines are the case widening cannot fix: the whole line is
        just as ambiguous as the value in it. The existing ambiguity warning
        still fires, so the user is told which anchor was the problem.
        """
        order = ["Dup.esp", "Mine.esp", "Dup.esp"]
        haystack = ["content=Dup.esp", "content=Dup.esp", "content=Mine.esp"]

        assert _pick_anchor(order, 1, 2, haystack) == ("after", "Dup.esp")

    def test_a_run_covering_the_whole_order_has_no_anchor(self):
        """There is nothing to attach to; the emitter writes a warning instead."""
        assert _pick_anchor(["Mine.esp"], 0, 1, ["content=Mine.esp"]) == ("after", None)

    def test_uniqueness_is_substring_based_like_the_configurator(self):
        """Matching the Go semantics is the whole point of the check."""
        haystack = ["content=Wares.esp", "content=Better Wares.esp"]

        assert not _anchor_is_unique("Wares.esp", haystack)
        assert _anchor_is_unique("Better Wares.esp", haystack)


def same_anchor_toml(anchor: str, values: list[str], mode: str) -> str:
    """Render N separate inserts that all share one fixed anchor.

    The shape the data-path emitter used to produce -- distinct from
    :func:`chained_toml`, which anchors each insert on the previously inserted
    line.

    Args:
        anchor: The single line every insert attaches to.
        values: The values to insert, in the order written.
        mode: ``"after"`` or ``"before"``.

    Returns:
        TOML text.
    """
    blocks = ["[[Customizations]]\nlistName = 'total-overhaul'\n"]
    blocks.extend(
        f"[[Customizations.insert]]\ninsert = '{value}'\n{mode} = '{anchor}'\n" for value in values
    )
    return "\n".join(blocks)


def block_anchor_toml(anchor: str, values: list[str], mode: str) -> str:
    """Render the same insertions as one ``insertBlock`` on the same anchor.

    Args:
        anchor: The line the block attaches to.
        values: The values to insert, in order.
        mode: ``"after"`` or ``"before"``.

    Returns:
        TOML text.
    """
    body = "\n".join(values)
    return (
        "[[Customizations]]\nlistName = 'total-overhaul'\n\n"
        f"[[Customizations.insert]]\ninsertBlock = '''{body}'''\n{mode} = '{anchor}'\n"
    )


class TestManyInsertsOnOneFixedAnchor:
    """The pattern the data-path emitter used, which nothing here modelled.

    Two forms were tested before this: chaining each insert on the previous
    line, and one ``insertBlock``. The data-path emitter did neither -- it wrote
    N separate inserts all naming the *same* frozen anchor. That has its own
    ordering semantics, and getting them wrong reverses a run silently.
    """

    ANCHOR = "Patch for Purists.esp"
    VALUES = ["First.esp", "Second.esp", "Third.esp"]

    @staticmethod
    def content_of(toml_text: str) -> list[str]:
        """Apply a TOML and return just the content lines.

        Args:
            toml_text: The customizations.

        Returns:
            The resulting ``content=`` lines.
        """
        lines, errors = applied(toml_text)
        assert errors == []
        assert lines is not None
        return [line for line in lines if line.startswith("content=")]

    def test_repeated_after_anchors_come_out_reversed(self) -> None:
        """Each insert lands immediately after the same line, so order inverts.

        This is why the old emitter wrote its ``after`` runs backwards. Pinned
        because it is the reason the reversal existed, and therefore the reason
        it had to be *removed* when the form changed.
        """
        got = self.content_of(same_anchor_toml(self.ANCHOR, self.VALUES, "after"))

        assert got[-3:] == ["content=Third.esp", "content=Second.esp", "content=First.esp"]

    def test_repeated_before_anchors_keep_their_order(self) -> None:
        """The mirror case does not invert, which is why only one was reversed."""
        got = self.content_of(same_anchor_toml(self.ANCHOR, self.VALUES, "before"))

        assert got[-4:-1] == ["content=First.esp", "content=Second.esp", "content=Third.esp"]

    @pytest.mark.parametrize("mode", ["after", "before"])
    def test_a_block_keeps_its_own_order_either_way(self, mode: str) -> None:
        """A block is placed as a unit, so neither direction inverts it.

        The whole point: carrying the ``after`` reversal into the block form
        would silently write the run backwards.

        Args:
            mode: The anchor direction.
        """
        got = self.content_of(block_anchor_toml(self.ANCHOR, self.VALUES, mode))
        mine = [line for line in got if line != "content=" + self.ANCHOR]

        assert mine[-3:] == ["content=First.esp", "content=Second.esp", "content=Third.esp"]

    def test_the_block_equals_the_old_form_written_the_old_way(self) -> None:
        """The migration is only safe if the results are identical.

        ``after`` compared against the *reversed* chained form, because that is
        what the emitter actually wrote -- comparing against the forward form
        would compare the block to a bug.
        """
        old = same_anchor_toml(self.ANCHOR, list(reversed(self.VALUES)), "after")
        new = block_anchor_toml(self.ANCHOR, self.VALUES, "after")

        assert applied(old) == applied(new)

    def test_the_before_form_needed_no_reversal(self) -> None:
        """And so converts to a block with the values exactly as they were."""
        old = same_anchor_toml(self.ANCHOR, self.VALUES, "before")
        new = block_anchor_toml(self.ANCHOR, self.VALUES, "before")

        assert applied(old) == applied(new)


class TestTheDataEmitterEndToEnd:
    """What the emitter actually writes, applied to a cfg.

    The classes above prove things about the *forms*. This one runs the real
    :func:`generate_customizations_toml` and puts its output through the
    simulator, because a correct understanding of the forms and a correct
    emitter are separate claims -- and the gap between them is where a run gets
    silently reversed.
    """

    FROZEN = 'data="E:\\Mods\\overhaul\\PatchForPurists"'
    NESTED = 'data="E:\\Mods\\overhaul\\UvirithsLegacy\\Data Files"'
    NESTED_SUB = 'data="E:\\Mods\\overhaul\\UvirithsLegacy\\Data Files\\Addons"'

    @staticmethod
    def emit(tuples):
        """Emit a customizations TOML for a data-path layout.

        Args:
            tuples: The ``(line, is_new, value)`` triples.

        Returns:
            The emitted TOML text.
        """
        return generate_customizations_toml(
            {},
            ["Morrowind.esm"],
            set(),
            {"Morrowind.esm": "Morrowind.esm"},
            list_name="total-overhaul",
            data_result_tuples=tuples,
        )

    @staticmethod
    def data_paths(lines):
        """The data path values from a cfg, in order.

        Args:
            lines: The cfg lines.

        Returns:
            Each ``data=`` value, unquoted.
        """
        return [
            line.split("=", 1)[1].strip().strip('"') for line in lines if line.startswith("data=")
        ]

    def test_a_run_at_the_end_keeps_its_order(self) -> None:
        """The case a one-element run cannot check.

        A run with no frozen line after it anchors ``after`` the preceding one.
        Chained inserts had to be written backwards to survive that; a block
        must not be. With three entries the two are distinguishable, which with
        one entry they are not.
        """
        mine = ["E:\\Mods\\custom\\A", "E:\\Mods\\custom\\B", "E:\\Mods\\custom\\C"]
        tuples = [(self.FROZEN, False, "E:\\Mods\\overhaul\\PatchForPurists")]
        tuples += [(f'data="{p}"', True, p) for p in mine]

        cfg = [self.FROZEN]
        lines, errors, _notes = simulate_configurator_apply(
            cfg, self.emit(tuples), "total-overhaul"
        )

        assert errors == []
        assert lines is not None
        assert self.data_paths(lines) == ["E:\\Mods\\overhaul\\PatchForPurists", *mine]

    def test_a_run_before_a_frozen_line_keeps_its_order(self) -> None:
        """The mirror case, which anchors the other way."""
        mine = ["E:\\Mods\\custom\\A", "E:\\Mods\\custom\\B", "E:\\Mods\\custom\\C"]
        tuples = [(f'data="{p}"', True, p) for p in mine]
        tuples += [(self.FROZEN, False, "E:\\Mods\\overhaul\\PatchForPurists")]

        cfg = [self.FROZEN]
        lines, errors, _notes = simulate_configurator_apply(
            cfg, self.emit(tuples), "total-overhaul"
        )

        assert errors == []
        assert lines is not None
        assert self.data_paths(lines) == [*mine, "E:\\Mods\\overhaul\\PatchForPurists"]

    def test_a_run_between_two_frozen_lines_anchors_forward(self) -> None:
        """Which neighbour is chosen when both are available and unique.

        Both place the run identically, so nothing about the resulting cfg
        distinguishes them -- only the anchor written into the file differs.
        Pinned anyway, because that anchor is what a later rebuild depends on,
        and "it does not matter" is how a preference changes unnoticed.
        """
        tuples = [
            (self.FROZEN, False, "E:\\Mods\\overhaul\\PatchForPurists"),
            ('data="E:\\Mods\\custom\\A"', True, "E:\\Mods\\custom\\A"),
            (self.NESTED, False, "E:\\Mods\\overhaul\\UvirithsLegacy\\Data Files"),
        ]
        emitted = self.emit(tuples)

        assert "before = " in emitted
        assert "after = " not in emitted

    def test_one_run_becomes_one_entry(self) -> None:
        """Not one per path, which is what produced a 2,229-line file."""
        mine = [f"E:\\Mods\\custom\\M{i}" for i in range(6)]
        tuples = [(f'data="{p}"', True, p) for p in mine]
        tuples += [(self.FROZEN, False, "E:\\Mods\\overhaul\\PatchForPurists")]

        emitted = self.emit(tuples)

        assert emitted.count("[[Customizations.insert]]") == 1
        assert "insertBlock" in emitted

    def test_a_nested_frozen_path_does_not_abort_the_run(self) -> None:
        """The reported failure, driven through the real emitter.

        The anchor the emitter would naturally choose is a strict prefix of
        another real cfg line, which the Configurator treats as fatal.
        """
        tuples = [
            ('data="E:\\Mods\\custom\\A"', True, "E:\\Mods\\custom\\A"),
            (self.NESTED, False, "E:\\Mods\\overhaul\\UvirithsLegacy\\Data Files"),
            (self.NESTED_SUB, False, "E:\\Mods\\overhaul\\UvirithsLegacy\\Data Files\\Addons"),
        ]
        cfg = [self.NESTED, self.NESTED_SUB]
        lines, errors, _notes = simulate_configurator_apply(
            cfg, self.emit(tuples), "total-overhaul"
        )

        assert lines is not None, f"the emitted TOML aborted the run: {errors}"
        assert errors == []
        assert self.data_paths(lines)[0] == "E:\\Mods\\custom\\A"


class TestWideningAnAmbiguousAnchor:
    """Widening an anchor to its whole cfg line, and when that fails.

    The Configurator matches anchors against whole lines with
    ``strings.Contains``, so a value that is a prefix of another value is
    ambiguous -- but the *line* it sits in often is not, because the line
    carries delimiters the value lacks.
    """

    #: Both are real lines from a real user's cfg, and the first is a strict
    #: prefix of the second. Choosing the first as an anchor is fatal.
    NESTED = [
        'data="E:\\OpenMW\\Mods\\total-overhaul\\PlayerHomes\\UvirithsLegacy\\Data Files"',
        'data="E:\\OpenMW\\Mods\\total-overhaul\\PlayerHomes\\UvirithsLegacy\\Data Files\\Addons"',
    ]
    NESTED_VALUE = "E:\\OpenMW\\Mods\\total-overhaul\\PlayerHomes\\UvirithsLegacy\\Data Files"

    def test_the_bare_path_is_ambiguous(self) -> None:
        """Establishing the problem before checking the fix."""
        assert not _anchor_is_unique(self.NESTED_VALUE, self.NESTED)

    def test_the_quoted_line_is_not(self) -> None:
        """The closing quote ends the match, which the bare path cannot do."""
        widened = _widen_anchor(self.NESTED_VALUE, self.NESTED[0], self.NESTED)

        assert widened == self.NESTED[0]
        assert _anchor_is_unique(widened, self.NESTED)

    def test_the_ambiguous_anchor_really_would_abort_the_run(self) -> None:
        """The cost of not widening, measured rather than asserted."""
        toml = (
            "[[Customizations]]\nlistName = 'total-overhaul'\n\n"
            "[[Customizations.insert]]\ninsert = 'E:\\Mods\\Mine'\n"
            f"before = '{self.NESTED_VALUE}'\n"
        )
        lines, errors = applied(toml, self.NESTED)

        assert lines is None, "an ambiguous anchor must be fatal, as the Go code makes it"
        assert any("FATAL" in e for e in errors)

    def test_the_widened_anchor_applies_cleanly(self) -> None:
        """And the same insert now lands instead of aborting."""
        toml = (
            "[[Customizations]]\nlistName = 'total-overhaul'\n\n"
            "[[Customizations.insert]]\ninsert = 'E:\\Mods\\Mine'\n"
            f"before = '{self.NESTED[0]}'\n"
        )
        lines, errors = applied(toml, self.NESTED)

        assert errors == []
        assert lines is not None
        assert any("Mine" in line for line in lines)

    def test_the_narrower_form_is_preferred_when_it_works(self) -> None:
        """A whole-line anchor everywhere would be noise; widen only on need."""
        haystack = ["content=Alpha.esp", "content=Beta.esp"]

        assert _widen_anchor("Alpha.esp", "content=Alpha.esp", haystack) == "Alpha.esp"

    def test_an_unquoted_nested_path_cannot_be_widened(self) -> None:
        """Honest about the limit: there is no delimiter to widen to.

        Without quotes the shorter line is a prefix of the longer one at every
        width, so the caller has to fall back to the other neighbour instead.
        """
        unquoted = [
            "data=E:\\Mods\\UvirithsLegacy\\Data Files",
            "data=E:\\Mods\\UvirithsLegacy\\Data Files\\Addons",
        ]

        assert _widen_anchor("E:\\Mods\\UvirithsLegacy\\Data Files", unquoted[0], unquoted) is None
