"""An ID renamed inside script source, for Search & Replace.

The Construction Set's Search & Replace leaves scripts alone, because a changed text
without a recompile leaves the bytecode Morrowind.exe runs naming the old ID. With the
compiler (:mod:`wraithguard.mwscript.compiler`) the editor can recompile, so scripts
and dialogue results are changed too: every place the ID stands as a word, bare or
quoted, the way the compiler reads one - not inside a longer word, a different quoted
ID, or a comment.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from wraithguard.mwscript.compiler_data import CHAR_TYPES

_SYMBOL = 16


def _is_symbol(ch: str) -> bool:
    """Whether a character is part of an id, as the compiler's table has it."""
    o = ord(ch)
    return o < 256 and bool(CHAR_TYPES[o] & _SYMBOL)


def replace_id(text: str, old: str, new: str) -> tuple[str, int]:
    """``text`` with the ID ``old`` (any case) made ``new`` wherever it stands as one.

    A bare word is replaced when the whole word is ``old``; a quoted string when its
    whole content is. ``new`` is quoted where it has to be (a space, a dot, a
    hyphen...), as the compiler would need it. Comments (``;`` to the line's end) are
    left as written.

    Args:
        text: Script source, or a dialogue result.
        old: The ID to replace.
        new: The ID to put in its place.

    Returns:
        The new text and how many places changed.
    """
    low = old.lower()
    needs_quotes = not new or not all(_is_symbol(c) for c in new) or not _is_symbol(new[0])
    bare_new = f'"{new}"' if needs_quotes else new
    out: list[str] = []
    count = 0
    i, n = 0, len(text)
    while i < n:
        ch = text[i]
        if ch == ";":
            end = text.find("\n", i)
            end = n if end < 0 else end
            out.append(text[i:end])
            i = end
        elif ch == '"':
            end = text.find('"', i + 1)
            line_end = text.find("\n", i + 1)
            if end < 0 or (0 <= line_end < end):
                out.append(ch)
                i += 1
                continue
            inner = text[i + 1 : end]
            if inner.lower() == low:
                out.append(f'"{new}"')
                count += 1
            else:
                out.append(text[i : end + 1])
            i = end + 1
        elif _is_symbol(ch):
            j = i
            while j < n and _is_symbol(text[j]):
                j += 1
            word = text[i:j]
            if word.lower() == low:
                out.append(bare_new)
                count += 1
            else:
                out.append(word)
            i = j
        else:
            out.append(ch)
            i += 1
    return "".join(out), count
