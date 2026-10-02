"""A Lua 5.1 tokenizer, with LuaJIT's and OpenMW's additions.

OpenMW runs LuaJIT (Lua 5.1 with ``goto`` and labels, and a few 5.2 escapes). The
tokens keep their exact source span, comments included, so the same pass serves
the parser (which skips comments) and syntax highlighting (which colours them).

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

KEYWORDS: frozenset[str] = frozenset(
    {
        "and",
        "break",
        "do",
        "else",
        "elseif",
        "end",
        "false",
        "for",
        "function",
        "goto",
        "if",
        "in",
        "local",
        "nil",
        "not",
        "or",
        "repeat",
        "return",
        "then",
        "true",
        "until",
        "while",
    }
)

# Longest first, so ``...`` wins over ``..`` and ``..`` over ``.``. The 5.3 integer
# and bitwise operators are tokenized (a script for a newer OpenMW may use them) and
# the parser then says they are not 5.1.
_OPERATORS = (
    "...",
    "..",
    "==",
    "~=",
    "<=",
    ">=",
    "::",
    "//",
    "<<",
    ">>",
    "+",
    "-",
    "*",
    "/",
    "%",
    "^",
    "#",
    "&",
    "~",
    "|",
    "<",
    ">",
    "=",
    "(",
    ")",
    "{",
    "}",
    "[",
    "]",
    ";",
    ":",
    ",",
    ".",
)

_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_NUMBER = re.compile(
    r"""
    0[xX](?:[0-9a-fA-F]*\.?[0-9a-fA-F]+|[0-9a-fA-F]+\.?)(?:[pP][+-]?[0-9]+)?
    | (?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?
    """,
    re.VERBOSE,
)
# LuaJIT's 64-bit integer and imaginary suffixes (1LL, 2ULL, 3i).
_NUM_SUFFIX = re.compile(r"(?:[uU]?[lL][lL]|[iI])")
_LONG_OPEN = re.compile(r"\[(=*)\[")
_SPACE = re.compile(r"[ \t\r\f\v]+")
_ESCAPES = {
    "n": "\n",
    "t": "\t",
    "r": "\r",
    "a": "\a",
    "b": "\b",
    "f": "\f",
    "v": "\v",
    "\\": "\\",
    '"': '"',
    "'": "'",
    "\n": "\n",
}


class LuaSyntaxError(ValueError):
    """Source that is not valid Lua, with where it went wrong.

    Attributes:
        line: The 1-based line of the problem.
        col: The 1-based column.
    """

    def __init__(self, message: str, line: int, col: int) -> None:
        """Build the error.

        Args:
            message: What is wrong.
            line: The 1-based line.
            col: The 1-based column.
        """
        super().__init__(f"line {line}:{col}: {message}")
        self.line = line
        self.col = col


@dataclass(frozen=True)
class Token:
    """One token.

    Attributes:
        kind: ``name``, ``keyword``, ``number``, ``string``, ``comment``, ``op`` or
            ``eof``.
        text: The exact source text.
        start: Its offset in the source.
        line: Its 1-based line.
        col: Its 1-based column.
        value: For a string, its decoded value; for a number, its text without a
            LuaJIT suffix; otherwise the text.
    """

    kind: str
    text: str
    start: int
    line: int
    col: int
    value: str

    @property
    def end(self) -> int:
        """The offset just past the token."""
        return self.start + len(self.text)


class _Cursor:
    """Position bookkeeping for :func:`tokenize`."""

    def __init__(self, src: str) -> None:
        """Start at the beginning of ``src``.

        Args:
            src: The source text.
        """
        self.src = src
        self.pos = 0
        self.line = 1
        self.line_start = 0

    def col(self, pos: int | None = None) -> int:
        """The 1-based column of ``pos`` (default: the cursor).

        Args:
            pos: An offset on the current line, or None for the cursor.

        Returns:
            The column.
        """
        return (self.pos if pos is None else pos) - self.line_start + 1

    def advance_to(self, end: int) -> None:
        """Move to ``end``, counting the newlines passed.

        Args:
            end: The new offset.
        """
        chunk = self.src[self.pos : end]
        n = chunk.count("\n")
        if n:
            self.line += n
            self.line_start = self.pos + chunk.rfind("\n") + 1
        self.pos = end

    def error(self, message: str) -> LuaSyntaxError:
        """An error at the cursor.

        Args:
            message: What is wrong.

        Returns:
            The error, for the caller to raise.
        """
        return LuaSyntaxError(message, self.line, self.col())


def _long_bracket_end(cur: _Cursor, start: int) -> tuple[int, int] | None:
    """Where a ``[==[ ... ]==]`` that opens at ``start`` ends.

    Args:
        cur: The cursor (for errors).
        start: The offset of the opening ``[``.

    Returns:
        ``(end, open_length)`` - the offset past the closing bracket and the length
        of the opening one - or None if no long bracket opens there.

    Raises:
        LuaSyntaxError: The bracket is never closed.
    """
    m = _LONG_OPEN.match(cur.src, start)
    if not m:
        return None
    close = "]" + m.group(1) + "]"
    end = cur.src.find(close, m.end())
    if end < 0:
        raise cur.error("unfinished long string or comment")
    return end + len(close), m.end() - start


def _long_value(text: str, open_len: int) -> str:
    """The contents of a long bracket, as Lua reads them.

    Args:
        text: The whole bracketed text.
        open_len: The length of its opening bracket.

    Returns:
        The contents; a first newline right after the opening bracket is dropped.
    """
    body = text[open_len : len(text) - open_len]
    if body.startswith("\r\n"):
        return body[2:]
    if body.startswith(("\n", "\r")):
        return body[1:]
    return body


def _read_string(cur: _Cursor) -> tuple[int, str]:
    """Read a quoted string starting at the cursor.

    Args:
        cur: The cursor, on the opening quote.

    Returns:
        ``(end, value)``: the offset past the closing quote, and the decoded value.

    Raises:
        LuaSyntaxError: A newline or the end of the source before the closing quote,
            or a bad escape.
    """
    src = cur.src
    quote = src[cur.pos]
    i = cur.pos + 1
    out: list[str] = []
    while True:
        if i >= len(src):
            raise cur.error("unfinished string")
        ch = src[i]
        if ch == quote:
            return i + 1, "".join(out)
        if ch == "\n":
            raise cur.error("unfinished string (newline before the closing quote)")
        if ch != "\\":
            out.append(ch)
            i += 1
            continue
        i += 1
        if i >= len(src):
            raise cur.error("unfinished string")
        esc = src[i]
        if esc in _ESCAPES:
            out.append(_ESCAPES[esc])
            i += 1
        elif esc == "\r":
            out.append("\n")
            i += 2 if src.startswith("\r\n", i) else 1
        elif esc.isdigit():
            m = re.match(r"[0-9]{1,3}", src[i:])
            digits = m.group(0) if m else esc
            code = int(digits)
            if code > 255:
                raise cur.error(f"escape \\{digits} is too large")
            out.append(chr(code))
            i += len(digits)
        elif esc == "x":
            m = re.match(r"[0-9a-fA-F]{2}", src[i + 1 :])
            if not m:
                raise cur.error("\\x needs two hexadecimal digits")
            out.append(chr(int(m.group(0), 16)))
            i += 3
        elif esc == "z":
            i += 1
            while i < len(src) and src[i] in " \t\r\n\f\v":
                i += 1
        elif esc == "u":
            m = re.match(r"\{([0-9a-fA-F]+)\}", src[i + 1 :])
            if not m:
                raise cur.error("\\u needs {hex digits}")
            out.append(chr(min(int(m.group(1), 16), 0x10FFFF)))
            i += 1 + len(m.group(0))
        else:
            raise cur.error(f"invalid escape \\{esc}")


def tokenize(src: str, *, comments: bool = False, teal: bool = False) -> list[Token]:
    """Split Lua source into tokens.

    Args:
        src: The source text. A leading ``#!`` line is skipped, as Lua does.
        comments: Keep comment tokens (for highlighting). The parser leaves them out.
        teal: Teal source: ``?`` (an optional parameter, ``x?: T``) is a token too.

    Returns:
        The tokens, ending with one ``eof`` token.

    Raises:
        LuaSyntaxError: Text that is not a Lua token.
    """
    cur = _Cursor(src)
    tokens: list[Token] = []
    if src.startswith("#"):
        nl = src.find("\n")
        cur.advance_to(len(src) if nl < 0 else nl)

    def emit(kind: str, end: int, value: str | None = None) -> None:
        """Record the token from the cursor to ``end`` and move past it.

        Args:
            kind: Its kind.
            end: The offset past it.
            value: Its value, when not the text itself.
        """
        text = src[cur.pos : end]
        tokens.append(
            Token(kind, text, cur.pos, cur.line, cur.col(), text if value is None else value)
        )
        cur.advance_to(end)

    n = len(src)
    while cur.pos < n:
        ch = src[cur.pos]
        if ch == "\n":
            cur.advance_to(cur.pos + 1)
            continue
        m = _SPACE.match(src, cur.pos)
        if m:
            cur.advance_to(m.end())
            continue
        if src.startswith("--", cur.pos):
            long = _long_bracket_end(cur, cur.pos + 2)
            if long:
                end = long[0]
            else:
                nl = src.find("\n", cur.pos)
                end = n if nl < 0 else nl
            if comments:
                emit("comment", end)
            else:
                cur.advance_to(end)
            continue
        m = _NAME.match(src, cur.pos)
        if m:
            word = m.group(0)
            emit("keyword" if word in KEYWORDS else "name", m.end())
            continue
        if ch.isdigit() or (ch == "." and cur.pos + 1 < n and src[cur.pos + 1].isdigit()):
            m = _NUMBER.match(src, cur.pos)
            if not m:
                raise cur.error("malformed number")
            end = m.end()
            sfx = _NUM_SUFFIX.match(src, end)
            if sfx:
                end = sfx.end()
            if end < n and (src[end].isalnum() or src[end] == "_"):
                raise cur.error(f"malformed number near {src[cur.pos : end + 1]!r}")
            emit("number", end, m.group(0))
            continue
        if ch in "\"'":
            end, value = _read_string(cur)
            emit("string", end, value)
            continue
        if ch == "[":
            long = _long_bracket_end(cur, cur.pos)
            if long:
                end, open_len = long
                emit("string", end, _long_value(src[cur.pos : end], open_len))
                continue
        if teal and ch == "?":
            emit("op", cur.pos + 1)
            continue
        for op in _OPERATORS:
            if src.startswith(op, cur.pos):
                emit("op", cur.pos + len(op))
                break
        else:
            raise cur.error(f"unexpected character {ch!r}")
    tokens.append(Token("eof", "", n, cur.line, cur.col(), ""))
    return tokens
