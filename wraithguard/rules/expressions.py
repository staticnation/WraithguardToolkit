"""Tokenise and parse the mlox predicate language.

``[Requires]``, ``[Conflict]`` and ``[Note]`` rule bodies are written in a
small lisp-like language: bracketed ``ALL``/``ANY``/``NOT`` groups over plugin
patterns, plus atomic function forms that test a plugin's version, file size or
header description.

This module covers the front half of that pipeline -- text to tokens to AST,
and rendering an AST back to readable text. It is deliberately free of any
dependency on plugin files: evaluating an AST needs to read real plugin
metadata, so that half lives with the code that can do so.

The tokeniser's one subtlety is worth stating plainly, because it is easy to
"simplify" and break: ``DESC`` and ``MWSE-LUA`` forms carry a ``/regex/`` that
may itself contain ``]`` (``[DESC /[Tt]ribunal/ Foo.esp]``). Those forms must
therefore consume the ``/.../`` part explicitly *before* looking for the
closing bracket. Naive bracket matching splits the token in the middle of the
regex and produces nonsense. ``VER`` and ``SIZE`` bodies cannot contain
brackets, so they are matched more simply.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import TYPE_CHECKING, Final

from wraithguard.i18n import gettext as _
from wraithguard.logging_setup import get_logger

if TYPE_CHECKING:
    from collections.abc import Sequence

#: Diagnostics about the run (not the user's report) go through here.
_LOG = get_logger(__name__)

#: The atomic function forms, captured whole so each becomes a single leaf node
#: the evaluator can dispatch on rather than being split into ``[`` + junk.
_FUNC_FORMS: Final = (
    r"\[\s*VER\b[^\]\n]*\]|"
    r"\[\s*SIZE\b[^\]\n]*\]|"
    r"\[\s*DESC\s*!?\s*/[^/\n]*/[^\]\n]*\]|"
    r"\[\s*MWSE-LUA\s*!?\s*/[^/\n]*/[^\]\n]*\]"
)

#: The atomic forms, matched at a ``[``.
_FUNC_FORM_RE: Final = re.compile(_FUNC_FORMS, re.IGNORECASE)

#: A logic keyword, standing alone.
_KEYWORD_RE: Final = re.compile(r"\bALL\b|\bANY\b|\bNOT\b", re.IGNORECASE)

#: A ``/message/`` string.
_MESSAGE_RE: Final = re.compile(r"/[^/]+/")

#: The plugin extensions a name ends in.
_EXT: Final = r"\.(?:esp|esm|omwaddon|omwgame|omwscripts)"

#: A plugin name, ending in an extension (and an optional ``*``). It may hold a bracketed
#: part, as mlox reads one (``Morrowind Rebirth [Main].ESP``, ``[Official]Master
#: Index.esp``), when the part opens no function or group and holds no extension; any
#: other bracket ends the name, so a group's own brackets are never taken into one.
_NAME_RE: Final = re.compile(
    r"(?:[^\[\]\n]|\[(?!(?:ALL|ANY|NOT|DESC|VER|SIZE|MWSE-LUA)\b|\s*(?:DESC|VER|SIZE|MWSE-LUA)\b)"
    r"(?:(?!" + _EXT + r")[^\[\]\n])*\])+?" + _EXT + r"\*?",
    re.IGNORECASE,
)

#: Logic operators recognised at the head of a group.
_OPERATORS: Final = frozenset({"ALL", "ANY", "NOT", "DESC"})


def tokenize_mlox_logic(text: str) -> list[str]:
    """Split a predicate body into tokens.

    Read left to right, trying at each place, in this order: a whole atomic form, a plugin
    name that starts with a bracketed part (``[Official]Master Index.esp``), a bracket, a
    logic keyword, a ``/message/``, a plugin name; a place that starts none of them is
    passed over. A name may carry a bracketed part inside it as mlox reads one
    (``Morrowind Rebirth [Main].ESP``); before that, such a name came apart into empty
    groups and a stray ``.ESP``.

    Args:
        text: The body of a ``[Requires]``/``[Conflict]``/``[Note]`` block.

    Returns:
        Tokens in source order: brackets, logic keywords, ``/message/``
        strings, plugin patterns, and whole atomic function forms. Empty
        tokens are dropped.
    """
    tokens: list[str] = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        m: re.Match[str] | None = None
        if c == "[":
            m = _FUNC_FORM_RE.match(text, i) or _NAME_RE.match(text, i)
            if m is None:
                tokens.append(c)
                i += 1
                continue
        elif c == "]":
            tokens.append(c)
            i += 1
            continue
        else:
            m = _KEYWORD_RE.match(text, i) or _MESSAGE_RE.match(text, i) or _NAME_RE.match(text, i)
        if m is None:
            i += 1
            continue
        tokens.append(m.group())
        i = m.end()
    return [token.strip() for token in tokens if token.strip()]


def parse_mlox_lisp(tokens: list[str]) -> list:
    """Build a nested-list AST from bracketed tokens.

    Warning:
        This **consumes** ``tokens`` via ``pop(0)`` -- recursion relies on the
        shared list being drained. Callers that need the tokens afterwards must
        pass a copy.

    Args:
        tokens: Output of :func:`tokenize_mlox_logic`.

    Returns:
        A nested list AST. Unbalanced input degrades gracefully rather than
        raising: a missing ``]`` simply ends the current group at end of input.
    """
    if not tokens:
        return []
    node: list = []
    while tokens:
        token = tokens.pop(0)
        if token == "[":  # noqa: S105 - a parser token, not a secret
            node.append(parse_mlox_lisp(tokens))
        elif token == "]":  # noqa: S105 - a parser token, not a secret
            return node
        else:
            node.append(token)
    return node


def describe_node(node: object) -> str:
    """Render an AST node back to a short human-readable string.

    Used to name a *missing* dependency in a ``[Requires]`` warning: a plugin
    that is not installed cannot be looked up in the active set to recover its
    real name, so the rule's own text is the only thing left to show.

    A group with no explicit operator is described as ``ANY``, matching how the
    evaluator treats it.

    Args:
        node: A string leaf or a nested list from :func:`parse_mlox_lisp`.

    Returns:
        A readable rendering, or ``"?"`` for anything unrecognised.
    """
    if isinstance(node, str):
        return node
    if isinstance(node, list) and node:
        if isinstance(node[0], str) and node[0].upper() in _OPERATORS:
            operator, rest = node[0].upper(), node[1:]
        else:
            operator, rest = "ANY", node
        return f"{operator}({', '.join(describe_node(item) for item in rest)})"
    return "?"


def load_rules_raw_text(rule_paths: Sequence[str | Path]) -> str:
    """Concatenate the raw text of every rule file.

    The predicate evaluator needs the *original* bodies -- uncommented and
    unsplit -- so it can see ``DESC`` message text that the ordering parser
    strips. File discovery matches :func:`~wraithguard.rules.parser.load_rule_blocks`
    so both see the same files in the same order.

    Args:
        rule_paths: Rule files or directories of ``*.txt`` rule files.

    Returns:
        Every file's text joined by newlines. Unreadable files are reported on
        stderr and skipped rather than aborting the run.
    """
    chunks: list[str] = []
    for raw_path in rule_paths:
        path = Path(raw_path)
        files = [path] if path.is_file() else sorted(path.glob("*.txt"))
        for rule_file in files:
            try:
                chunks.append(rule_file.read_text(encoding="utf-8-sig", errors="replace"))
            except Exception as exc:  # noqa: PERF203, BLE001
                # Deliberately broad, and deliberately inside the loop: rule
                # files are untrusted downloads, and isolating each file is the
                # point -- hoisting the try out would let one bad file discard
                # every file after it.
                _LOG.warning(
                    _("could not read rule file %(file)s for predicate checks: %(error)s"),
                    {"file": rule_file, "error": exc},
                )
    return "\n".join(chunks)
