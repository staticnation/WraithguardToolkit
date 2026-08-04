"""A carried-through ``replace`` block has to say that it is carried through.

A user asked publicly whether a ``[[Customizations.replace]]`` at the bottom of
their exported file was normal. It was theirs -- written by hand to reconcile a
plugin momw names ``CORE PATCH`` with the ``BASE PATCH`` they actually have
installed. Regenerating the file moved it, so it turned up somewhere it had
never been, with nothing attached to say where it came from, looking exactly
like something this tool had invented. It took a Discord thread and two people
reading a plugin_order.yml to establish that nothing was wrong.

The comment costs six lines and answers it in place. It also states a real
limitation that was previously invisible: momw-configurator's ``replace``
inherits the position of ``source``, so when mlox wants the plugin somewhere
else, this tool cannot express that and leaves it where it is.
"""

from __future__ import annotations

from typing import Final

from wraithguard.configurator.emit import _replace_notes

#: A sorted order with the reported plugin in it.
ORDER: Final[list[str]] = [
    "Morrowind.esm",
    "Tribunal.esm",
    "Bloodmoon.esm",
    "Finished Morrowind - BASE PATCH - Part 1.esp",
    "Finished Morrowind - BASE PATCH - Part 4 - More Audio Dialogue for Service NPCs.ESP",
    "Immersive Mournhold.esp",
]

#: The block exactly as it appears in the reported file.
REPORTED: Final[dict[str, str]] = {
    "source": "Finished Morrowind - CORE PATCH - Part 4 - More Audio Dialogue for Service NPCs.ESP",
    "dest": "Finished Morrowind - BASE PATCH - Part 4 - More Audio Dialogue for Service NPCs.ESP",
}


class TestItSaysWhoseBlockItIs:
    """The question that was actually asked: did the tool write this?"""

    def test_it_says_the_block_is_the_user_s(self) -> None:
        """The first line, because it is the first thing to know."""
        assert "Yours: carried through unchanged" in _replace_notes(REPORTED, ORDER)[0]

    def test_it_says_the_tool_never_writes_one(self) -> None:
        """Rules out the tool as the author, rather than merely not claiming it."""
        assert any("never writes a replace" in line for line in _replace_notes(REPORTED, ORDER))

    def test_every_line_is_a_comment(self) -> None:
        """Anything else would change what the Configurator applies."""
        assert all(line.startswith("#") for line in _replace_notes(REPORTED, ORDER))


class TestItSaysWhereMloxWantsIt:
    """The limitation: we cannot move one of these, so the user must decide."""

    def test_the_position_is_given(self) -> None:
        """A position is actionable; "it might be wrong" is not."""
        notes = "\n".join(_replace_notes(REPORTED, ORDER))
        assert "position 5 of 6" in notes

    def test_the_preceding_plugin_is_named(self) -> None:
        """Easier to act on than an index, and it is what an insert would use."""
        notes = "\n".join(_replace_notes(REPORTED, ORDER))
        assert "after: Finished Morrowind - BASE PATCH - Part 1.esp" in notes

    def test_the_limitation_is_explained(self) -> None:
        """Without the reason, the position reads as a bug report about us."""
        notes = "\n".join(_replace_notes(REPORTED, ORDER))
        assert "inherits the position of source" in notes

    def test_the_first_plugin_gets_no_after_line(self) -> None:
        """There is nothing before it to name."""
        first = {"source": "x.esp", "dest": "Morrowind.esm"}
        assert not any("after:" in line for line in _replace_notes(first, ORDER))


class TestItStaysQuietWhenItHasNothingToSay:
    """Commentary that is not true is worse than none."""

    def test_a_data_path_replace_gets_no_position(self) -> None:
        """Most replaces swap folders, which have no place in the plugin order."""
        notes = _replace_notes(
            {
                "source": "Performance/ProjectAtlas/01 Textures - MET",
                "dest": "{{.ModBaseDir}}/starter/Performance/ProjectAtlas/01 Textures - Vanilla",
            },
            ORDER,
        )
        assert notes  # it is still the user's block, and that is worth saying
        assert not any("position" in line for line in notes)

    def test_a_block_with_no_dest_says_nothing(self) -> None:
        """A malformed entry gets no invented commentary."""
        assert _replace_notes({"source": "x.esp"}, ORDER) == []

    def test_a_block_with_no_source_says_nothing(self) -> None:
        """Both halves are needed before anything can be claimed about it."""
        assert _replace_notes({"dest": "Morrowind.esm"}, ORDER) == []

    def test_an_empty_order_still_names_the_owner(self) -> None:
        """With nothing sorted there is no position, but it is still theirs."""
        notes = _replace_notes(REPORTED, [])
        assert notes
        assert not any("position" in line for line in notes)


class TestMatchingIsCaseInsensitive:
    """Load orders and TOML files disagree about case constantly."""

    def test_a_differently_cased_dest_is_still_found(self) -> None:
        """``.ESP`` against ``.esp`` must not lose the position line."""
        rep = {"source": "x.esp", "dest": "IMMERSIVE MOURNHOLD.ESP"}
        assert any("position 6 of 6" in line for line in _replace_notes(rep, ORDER))
