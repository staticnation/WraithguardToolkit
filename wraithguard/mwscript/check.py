"""Checks on a Morrowind script's source, for the editor's Script Edit window.

Not the compiler (that is :mod:`wraithguard.mwscript.compiler`) - but the mistakes a
compiler would stop on, and the ones it would let through that break a script in play, read off the text a
line at a time:

- the ``begin``/``end`` frame, and a ``begin`` name that is not the record's id;
- ``if``/``elseif``/``else``/``endif`` and ``while``/``endwhile`` out of balance;
- a variable declared twice, or ``set`` on a name that is neither a local, a global nor
  another object's variable (``obj.var``); ``set`` without ``to``;
- a statement that starts with a name the game has no function for (the opcode table,
  MWEdit's and MWSE's function lists) and that is not a variable.

Each finding is ``(line, level, message)``, lines from 1, ``level`` ``error`` (it will not
compile) or ``warning`` (it compiles, or the compiler is lenient, and probably is wrong).

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Final

from wraithguard.mwscript.opcodes import BY_NAME

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable

#: Words that open a statement without being functions.
KEYWORDS: Final[frozenset[str]] = frozenset(
    {
        "begin",
        "end",
        "short",
        "long",
        "float",
        "if",
        "elseif",
        "else",
        "endif",
        "while",
        "endwhile",
        "set",
        "return",
    }
)

_DECL = re.compile(r"^(short|long|float)\s+([A-Za-z_][\w]*)", re.IGNORECASE)
_WORD = re.compile(r"^\s*(?:\"[^\"]*\"|[\w.]+)\s*->\s*([A-Za-z_]\w*)|^\s*([A-Za-z_]\w*)")


@dataclass(frozen=True, slots=True)
class Finding:
    """One thing wrong with a script.

    Attributes:
        line: The line, from 1.
        level: ``error`` or ``warning``.
        message: What, in a sentence.
    """

    line: int
    level: str
    message: str


def _code(line: str) -> str:
    """A line without its comment (``;`` outside quotes), trimmed."""
    out, quoted = [], False
    for ch in line:
        if ch == '"':
            quoted = not quoted
        elif ch == ";" and not quoted:
            break
        out.append(ch)
    return "".join(out).strip()


def check_script(
    text: str,
    script_id: str = "",
    globals_: Iterable[str] = (),
) -> list[Finding]:
    """Check a script's source.

    Args:
        text: The source.
        script_id: The record's id (``begin`` should name it).
        globals_: The load order's global variables (``set`` may name one).

    Returns:
        The findings, in line order.
    """
    native = _native_check()
    if native is not None:
        return [
            Finding(int(n), str(level), str(msg))
            for n, level, msg in native(text, script_id, [str(g) for g in globals_])
        ]
    return check_script_py(text, script_id, globals_)


def _native_check() -> Callable[..., list[tuple[int, str, str]]] | None:
    """The Rust backend's checks (``native/src/mwscript/check.rs``), or None."""
    try:
        import wraithguard_native
    except ImportError:
        return None
    return getattr(wraithguard_native, "mwscript_check", None)


def check_script_py(
    text: str,
    script_id: str = "",
    globals_: Iterable[str] = (),
) -> list[Finding]:
    """:func:`check_script` in Python.

    The fallback without the Rust backend, and the reference the Rust one is held to.
    """
    found: list[Finding] = []
    known_globals = {g.lower() for g in globals_}
    local: dict[str, int] = {}
    stack: list[tuple[str, int, bool]] = []  # (block, line, seen else)
    began = ended = False

    def say(n: int, level: str, message: str) -> None:
        """Record a finding."""
        found.append(Finding(n, level, message))

    for n, raw in enumerate(text.replace("\r\n", "\n").split("\n"), start=1):
        code = _code(raw)
        if not code:
            continue
        low = code.lower()
        first = low.split()[0].rstrip(",")
        if ended:
            say(n, "warning", "text after 'end' is never run")
            ended = False  # once
        if not began:
            if first != "begin":
                say(n, "error", "a script starts with 'begin <name>'")
            else:
                name = code.split()[1] if len(code.split()) > 1 else ""
                if not name:
                    say(n, "error", "'begin' needs the script's name")
                elif script_id and name.strip('"').lower() != script_id.lower():
                    say(n, "warning", f"'begin {name}' does not name this script ({script_id})")
            began = True
            continue
        if first == "begin":
            say(n, "error", "a second 'begin'")
            continue
        if first == "end" and len(low.split()) <= 2 and "->" not in low:
            for block, at, _else in stack:
                say(at, "error", f"'{block}' is never closed")
            stack.clear()
            ended = True
            continue
        decl = _DECL.match(code)
        if decl:
            var = decl.group(2).lower()
            if var in local:
                say(n, "warning", f"{decl.group(2)} is declared again (first on line {local[var]})")
            else:
                local[var] = n
            continue
        if first in ("if", "while"):
            stack.append((first, n, False))
            continue
        if first in ("elseif", "else"):
            if not stack or stack[-1][0] != "if":
                say(n, "error", f"'{first}' without an 'if'")
            elif stack[-1][2]:
                say(n, "error", f"'{first}' after 'else'")
            elif first == "else":
                stack[-1] = (stack[-1][0], stack[-1][1], True)
            continue
        if first in ("endif", "endwhile"):
            want = "if" if first == "endif" else "while"
            if not stack or stack[-1][0] != want:
                say(n, "error", f"'{first}' without a '{want}'")
            else:
                stack.pop()
            continue
        if first == "set":
            m = re.match(r"set\s+(\"[^\"]*\"\.\w+|[\w.]+)\s+to\b", code, re.IGNORECASE)
            if not m:
                say(n, "error", "'set' is 'set <variable> to <value>'")
                continue
            var = m.group(1).lower()
            if "." not in var and var not in local and var not in known_globals:
                say(n, "warning", f"{m.group(1)} is not a local variable or a global")
            continue
        if first == "return":
            continue
        w = _WORD.match(code)
        name = (w.group(1) or w.group(2)).lower() if w else ""
        if name and name not in BY_NAME and name not in KEYWORDS and name not in local:
            say(n, "warning", f"{name} is not a function the game knows")
    if not began:
        say(1, "error", "the script is empty")
    elif not ended and not found_end(text):
        for block, at, _else in stack:
            say(at, "error", f"'{block}' is never closed")
        say(max(1, len(text.splitlines())), "error", "the script has no 'end'")
    found.sort(key=lambda f: f.line)
    return found


def found_end(text: str) -> bool:
    """Whether a script has an ``end`` line (comments aside)."""
    for raw in text.splitlines():
        code = _code(raw).lower().split()
        if code and code[0] == "end" and len(code) <= 2:
            return True
    return False


__all__ = ["KEYWORDS", "Finding", "check_script", "check_script_py", "found_end"]
