r"""Regenerate ``tests/fixtures/pyre_cases.json`` from Python's ``re``.

``ordercore::pyre`` (``viewer-shell/ordercore``) matches the part of Python's regular expressions
the rule code uses, by hand. Each case is a pattern, its flags, a text, and what Python's ``re``
gives: that the pattern is an error, or its ``search`` and ``match`` (the span and the span of
each group, ``null`` for a group that did not take part). The Rust side may *decline* a pattern it
does not read (``must_support`` says which it must read); it may not answer differently from
Python.

The cases are written here (the six expressions the rule code compiles, over the texts they see,
and patterns around each feature) and made from a fixed seed with a generator of its own, so the
file does not move with Python's ``random``.

Run: ``python tools/gen_pyre_cases.py``.
"""

from __future__ import annotations

import json
import re
import warnings
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "pyre_cases.json"

VER = r"(\d+(?:[_.-]?\d+)*[a-zA-Z]?)"
FUNC_FORMS = (
    r"\[\s*VER\b[^\]\n]*\]|"
    r"\[\s*SIZE\b[^\]\n]*\]|"
    r"\[\s*DESC\s*!?\s*/[^/\n]*/[^\]\n]*\]|"
    r"\[\s*MWSE-LUA\s*!?\s*/[^/\n]*/[^\]\n]*\]"
)
TOKEN = (
    FUNC_FORMS + r"|\[|\]|\bALL\b|\bANY\b|\bNOT\b|/[^/]+/|"
    r"[^\[\]\n]+?\.(?:esp|esm|omwaddon|omwgame|omwscripts)\*?"
)
TOP = r"^\[(Order|NearStart|NearEnd|Requires|Conflict|Patch|Note|Version)\b([^\]\n]*)\]"
RE_VER = r"^\[\s*VER\s*([=<>])\s*" + VER + r"\s*([^\]]+?)\s*\]$"
RE_SIZE = r"^\[\s*SIZE\s*(!?)(\d+)\s+(\S.*?\.(?:es[mp]|omwaddon|omwgame|omwscripts)\b)\s*\]$"
RE_DESC = r"^\[\s*DESC\s*(!?)/([^/]+)/\s+([^\]]+?)\s*\]$"
RE_LUA = r"^\[\s*MWSE-LUA\s*(!?)/([^/]+)/\s+([^\]]+?)\s*\]$"

#: ``(pattern, ignore_case, multiline)`` the rule code compiles.
REAL: list[tuple[str, bool, bool]] = [
    (TOKEN, True, False),
    (TOP, True, True),
    (RE_VER, True, False),
    (RE_SIZE, True, False),
    (RE_DESC, True, False),
    (RE_LUA, True, False),
]

REAL_TEXTS: list[str] = [
    "",
    "A.esp",
    "[ALL A.esp [NOT B.esp] C.esm]",
    "Foo ALL A.esp",
    " /msg/ A.esp",
    "[VER = 1.0 A.esp]",
    "[VER = 1.0]",
    "[VER < 2.0b Mod.esp]",
    "[ver>1_2-3 Mod.esp]",
    "[VER =1.0A.esp]",
    "[VER = 1.0.0.5 Mod.esp ]",
    "[VER = x Mod.esp]",
    "[SIZE 1234 Mod.esp]",
    "[SIZE !1234 Mod With Spaces.esm]",
    "[SIZE 12 Mod.esps]",
    "[DESC /[Tt]ribunal/ Foo.esp]",
    "[DESC !/abc/ Foo.esp]",
    "[DESC / Foo.esp]",
    "[DESC /a/b/ Foo.esp]",
    "[MWSE-LUA /x/ Foo.esp]",
    "[DESC /x/ Foo.esp] [VER = 1 A.esp] B.esp",
    "[ALL\n  A.esp\n  [NOT B.esp]\n  C.esm]",
    "[Order]\nA.esp\n[NOTE]\nB.esp",
    "see [Order]\n[order] x\n[Requires\n[Conflict] msg\n[Note something] a\n[VERSION 0.1]",
    "[NearEnd]  \n[Patch Foo]",
    "[Orderly]\n[Order2]\n[Note]",
    "x.esp.esp *.esp A?.esp A.espx A.esm* B.omwaddon",
    "Café.esp [ALL Naïve.esm]",
    "[DESC /é/ Café.esp]",
    "ALL\u0131ANY ALL\u017f ANY\u212a NOT",
    "[V\u0130R = 1 A.esp] [DE\u017fC /x/ A.esp]",
    "Mod\u017f.esp B\u212a.ESM",
    "[SIZE 12 Mod.esp]\n",
    "a\u2028b.esp\u00a0c.esp",
]

#: Patterns for each feature, with texts they should meet. ``must_support``: read, not declined.
HAND_PATTERNS: list[str] = [
    "",
    "a",
    "abc",
    "a.c",
    "a*",
    "a+",
    "a?",
    "a{2}",
    "a{2,}",
    "a{,2}",
    "a{2,3}",
    "a{2,3}?",
    "a*?",
    "a+?b",
    "a??b",
    "(a)(b)?",
    "(a|b)+c",
    "(?:ab)*c",
    "a|b|",
    "|a",
    "()",
    "(a*)b",
    "^a",
    "a$",
    "^$",
    r"\bab\b",
    r"\Bab",
    r"\d+",
    r"\D+",
    r"\w+",
    r"\W+",
    r"\s+",
    r"\S+",
    "[abc]",
    "[^abc]",
    "[a-c]+",
    "[a-cx-z]+",
    "[]a]",
    "[^]a]",
    "[a-]",
    "[-a]",
    r"[\d.]+",
    r"[\w-]+",
    r"[^\s]+",
    r"[\]a]",
    r"[\\a]",
    r"\.",
    r"\(",
    r"a\+b",
    r"\n",
    r"\t",
    r"a\ b",
    r"\/",
    "a{",
    "a{x}",
    "a{1",
    "a}",
    "}",
    "]",
    "a]b",
    "{",
    "{a}",
    "a{1}{2}",
    "(a",
    "a)",
    ")",
    "*a",
    "+a",
    "?a",
    "a**",
    "a+*",
    "a*??",
    "^*",
    "$+",
    r"\b+",
    "[a",
    "[]",
    "[z-a]",
    r"[\d-z]",
    r"[a-\d]",
    r"\q",
    "a\\",
    "a{2,1}",
    "(?i)a",
    "(?=a)",
    "(?!a)",
    "(?<=a)b",
    "(?P<n>a)",
    r"(a)\1",
    "a*+",
    "a++",
    "a?+",
    "a{2}+",
    r"\x41",
    r"\u0041",
    r"\A",
    r"\Z",
    r"\0",
    "a{}",
    "a{,}",
    "a{1001}",
    "(a*)*",
    "(a|)*",
    "(a?)+",
    "(^)*",
    "é",
    "[é]",
    "[a-é]",
    "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)",
    "a|b|c|d",
    "(a|ab)(c|bcd)(d*)",
    "x*y*z*",
    "(?:a|b)*?c",
    ".*a.*b",
    "(.*)(.*)",
    "a.*?b.*?c",
]

HAND_TEXTS: list[str] = [
    "",
    "a",
    "ab",
    "abc",
    "aaaa",
    "abcd",
    "xabcx",
    "a b",
    "a\nb",
    "a\n",
    "ba",
    "12 ab 34",
    "ab_cd-9",
    "x.y(z)+",
    "a{1}",
    "a{",
    "a]b",
    "é",
    "ab\u0663",
    "\u0131\u0130\u017f\u212a",
]

#: Texts for the random patterns.
FUZZ_ALPHABET = [*"abAB1_ -.]\n", "é", "\u0131", "\u0663"]


class Rng:
    """A small generator of its own, so the file does not change with Python's ``random``."""

    def __init__(self, seed: int) -> None:
        """Start from ``seed``."""
        self.state = (seed * 2654435761 + 12345) % (1 << 32)

    def below(self, n: int) -> int:
        """A number from 0 up to, not including, ``n``."""
        self.state = (self.state * 1664525 + 1013904223) % (1 << 32)
        return (self.state >> 8) % n

    def pick(self, items: list[str]) -> str:
        """One of the items."""
        return items[self.below(len(items))]


def _atom(r: Rng, depth: int) -> str:
    roll = r.below(20)
    if roll < 6:
        return r.pick(list("abAB1_"))
    if roll == 6:
        return "."
    if roll == 7:
        return r.pick(["[ab]", "[^a]", "[a-b]", r"[\d_]", r"[^\s]", "[a-]", "[A-Za-z]"])
    if roll == 8:
        return r.pick([r"\d", r"\w", r"\s", r"\D", r"\W", r"\S"])
    if roll == 9:
        return r.pick(["^", "$", r"\b", r"\B"])
    if roll == 10:
        return r.pick([" ", "-", ".", r"\.", r"\-", "]", "}"])
    if roll < 15 and depth < 3:
        inner = _alt(r, depth + 1)
        return f"({inner})" if r.below(2) else f"(?:{inner})"
    return r.pick(list("abAB"))


def _piece(r: Rng, depth: int) -> str:
    atom = _atom(r, depth)
    roll = r.below(12)
    if roll < 6 or atom in ("^", "$") or atom.startswith(("\\b", "\\B")):
        return atom
    quant = ["*", "+", "?", "{2}", "{1,2}", "{,2}", "{2,}"][r.below(7)]
    lazy = "?" if r.below(4) == 0 else ""
    return atom + quant + lazy


def _cat(r: Rng, depth: int) -> str:
    return "".join(_piece(r, depth) for _ in range(1 + r.below(4)))


def _alt(r: Rng, depth: int) -> str:
    return "|".join(_cat(r, depth) for _ in range(1 + (r.below(3) == 0) + (r.below(6) == 0)))


def _noise(r: Rng) -> str:
    """A string of metacharacters, most of them not a pattern."""
    pool = [*"ab()[]{}|*+?.^$\\-,:d1", "\\d", "\\b", "(?:", "{2}", "[^"]
    return "".join(r.pick(pool) for _ in range(1 + r.below(7)))


def _text(r: Rng) -> str:
    return "".join(r.pick(FUZZ_ALPHABET) for _ in range(r.below(9)))


def _spans(m: re.Match[str] | None) -> list[list[int] | None] | None:
    if m is None:
        return None
    return [None if m.start(i) < 0 else [m.start(i), m.end(i)] for i in range(m.re.groups + 1)]


def answer(pattern: str, ignore_case: bool, multiline: bool, text: str) -> dict[str, Any]:
    """What Python's ``re`` gives for one case."""
    flags = (re.IGNORECASE if ignore_case else 0) | (re.MULTILINE if multiline else 0)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        try:
            rx = re.compile(pattern, flags)
        except re.error:
            return {"error": True}
        return {"error": False, "search": _spans(rx.search(text)), "match": _spans(rx.match(text))}


def cases() -> list[dict[str, Any]]:
    """The cases, answered by Python."""
    todo: list[tuple[str, bool, bool, str, bool]] = []
    for pattern, icase, multi in REAL:
        todo += [(pattern, icase, multi, t, True) for t in REAL_TEXTS]
    for pattern in HAND_PATTERNS:
        for icase in (False, True):
            todo += [(pattern, icase, False, t, False) for t in HAND_TEXTS]
    for seed in range(1, 401):
        r = Rng(seed)
        pattern = _noise(r) if r.below(8) == 0 else _alt(r, 0)
        icase, multi = r.below(4) == 0, r.below(5) == 0
        todo += [(pattern, icase, multi, _text(r), False) for _ in range(3)]
    return [
        {
            "pattern": p,
            "ignore_case": i,
            "multiline": m,
            "text": t,
            "real": real,
            **answer(p, i, m, t),
        }
        for p, i, m, t, real in todo
    ]


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "Python's re, as ordercore::pyre must answer it: the six expressions the rule code "
            "compiles (real) and patterns around each feature, with their search and match; run by "
            "tests/test_pyre_cases.py and viewer-shell/ordercore/tests/pyre_cases.rs. Regenerate: "
            "python tools/gen_pyre_cases.py"
        ),
        "cases": out,
    }
    OUT.write_text(
        json.dumps(doc, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8"
    )
    print(f"{len(out)} cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
