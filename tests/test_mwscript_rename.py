"""An ID renamed inside script source (Search & Replace in scripts)."""

from __future__ import annotations

from wraithguard.mwscript.rename import replace_id


def test_bare_and_quoted_ids_are_replaced() -> None:
    """Whole words and whole quoted strings, any case."""
    text = 'Misc_01->Disable\nAddItem "misc_01", 1\nset x to misc_01.state\n'
    out, n = replace_id(text, "misc_01", "gold_001")
    assert n == 3
    assert out == 'gold_001->Disable\nAddItem "gold_001", 1\nset x to gold_001.state\n'


def test_longer_words_other_strings_and_comments_stay() -> None:
    """Not inside a longer word, another string, or a comment."""
    text = 'misc_011->Disable ; misc_01 here\nMessageBox "misc_01 is gone"\n'
    out, n = replace_id(text, "misc_01", "gold_001")
    assert n == 0
    assert out == text


def test_a_new_id_that_needs_quotes_gets_them() -> None:
    """A bare use becomes quoted when the new ID has a space or a dot."""
    out, n = replace_id("misc_01->Enable\n", "misc_01", "my lamp.01")
    assert n == 1
    assert out == '"my lamp.01"->Enable\n'
