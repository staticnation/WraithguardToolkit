r"""Regenerate ``tests/fixtures/pattern_cases.json`` from the Python pattern code.

The inputs are written here; the answers are what ``wraithguard.sort.graph.expand_pattern``,
``wraithguard.rules.pattern_has_meta`` and ``wraithguard.versions.format_version`` give.
``ordercore::patterns`` (``viewer-shell/ordercore``) runs the same file, so the two cannot
drift.

A case marked ``"supported": false`` is one the Rust side declines (it answers ``Unsupported``
and the caller keeps the Python): a pattern with a ``*``, ``?`` or ``<VER>`` that also holds a
character mlox passes through to the regular expression (``[ ] { } | \ ^ $``), and a version
holding a non-ASCII character. The Python answer is recorded for them all the same.

Run: ``python tools/gen_pattern_cases.py``.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "pattern_cases.json"

#: What a pattern is looked up in (a load order, so in load order).
POOL: list[str] = [
    "Morrowind.esm",
    "Tribunal.esm",
    "Better Bodies.esp",
    "Better Bodies 2.2.esp",
    "Better Bodies 10.esp",
    "BetterBodies.esp",
    "Tamriel_Data.esm",
    "TR_Mainland.esm",
    "TR_Data.esm",
    "MOD.ESP",
    "a.esp",
    "ab.esp",
    "abc.esp",
    "Mod (v2).esp",
    "Mod+.esp",
    "Mod v1.2.3a.esp",
    "Mod v1.2.3.4.esp",
    "x-1.0.esp",
    "x_1_0.esp",
    "Café.esp",
    "CAFÉ.ESP",
    "Moe's Mod.esp",
    "A & B.esp",
    "name.with.dots.esp",
    "Mod [x].esp",
    "1.0",
    "2",
    "1_2_3",
    "1..2",
    "1.",
    "a1",
]

#: Characters mlox leaves to the regular expression; with a wildcard they make a pattern the
#: Rust side declines.
PASSED_THROUGH = "[]{}|\\^$"

PATTERNS: list[tuple[str, str]] = [
    ("plain", "Morrowind.esm"),
    ("plain in another case", "morrowind.ESM"),
    ("plain, absent", "Nothing.esp"),
    ("plain with parentheses", "Mod (v2).esp"),
    ("plain with a plus", "Mod+.esp"),
    ("plain with dots", "name.with.dots.esp"),
    ("plain finds the first only", "mod.esp"),
    ("plain, non-ASCII, in another case", "café.esp"),
    ("plain with brackets is literal", "Mod [x].esp"),
    ("plain with an apostrophe and spaces", "Moe's Mod.esp"),
    ("plain with an ampersand", "A & B.esp"),
    ("star in the middle", "Better Bodies*.esp"),
    ("star first", "*.esm"),
    ("a lone star", "*"),
    ("two stars", "**.esp"),
    ("stars with underscores", "T*_*.esm"),
    ("a star after a dot", "A.*"),
    ("a star and a dot", "*.*"),
    ("star matches nothing", "Morrowind*.esm"),
    ("star after a parenthesis", "Mod (*"),
    ("star after a plus", "Mod+*"),
    ("star, another case", "better bodies*.ESP"),
    ("question mark matches none or one", "a?.esp"),
    ("a lone question mark first", "?.esp"),
    ("two question marks", "a??.esp"),
    ("question mark and star", "?*.esm"),
    ("question mark in non-ASCII", "caf?.esp"),
    ("star in non-ASCII", "CAF*.ESP"),
    ("version", "Better Bodies <VER>.esp"),
    ("version in any case", "better bodies <ver>.esp"),
    ("version with a prefix", "Mod v<VER>.esp"),
    ("version after a dash", "x-<VER>.esp"),
    ("version after an underscore", "x_<VER>.esp"),
    ("a lone version", "<VER>"),
    ("version then star", "<VER>*"),
    ("star then version", "*<VER>"),
    ("version then question mark", "<VER>?"),
    ("two versions", "<VER>.<VER>"),
    ("star inside brackets", "[Mod]*.esp"),
    ("alternation", "a|b*"),
    ("backslash", "x\\y*"),
    ("caret", "^A*"),
    ("dollar", "A*$"),
    ("repeat count", "a{2}*"),
    ("class with a version", "[abc]<VER>"),
]

VERSIONS: list[str] = [
    "1.0",
    "1.0a",
    "1.0A",
    "1.0ab",
    "1.2.3",
    "1.2.3.4",
    "1.2.3.4.5",
    "1.2.3.4_5",
    "1.2.3.4-5",
    "1.2.3.-5",
    "1.0.0.0.0",
    "1.0.0.0_0",
    "2",
    "",
    "abc",
    "a1",
    "1..2",
    "1.",
    ".1",
    "1.2a.3",
    "1.2.3a",
    "5a",
    "10a",
    "01.002.0003",
    "00000",
    "1_2_3",
    "1-2-3",
    " 1.0",
    "1.0 ",
    "1 .0",
    "+1.2",
    "1.+2",
    "1.-2",
    "99999999999999999999.1",
    "1.0\t",
    "1_000.2",
    "\u0663.\u0661",  # Arabic-Indic digits: Python reads them, the Rust side declines
    "1.0é",
]


def _has_meta(pattern: str) -> bool:
    return "*" in pattern or "?" in pattern or "<ver>" in pattern.lower()


def cases() -> dict[str, list[dict[str, object]]]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    from wraithguard.rules import pattern_has_meta
    from wraithguard.sort.graph import expand_pattern
    from wraithguard.versions import format_version

    patterns: list[dict[str, object]] = []
    for name, pattern in PATTERNS:
        meta = pattern_has_meta(pattern)
        if meta != _has_meta(pattern):
            msg = f"the wildcard test differs from pattern_has_meta on {pattern!r}"
            raise SystemExit(msg)
        patterns.append(
            {
                "name": name,
                "pattern": pattern,
                "has_meta": meta,
                "supported": not (meta and any(c in pattern for c in PASSED_THROUGH)),
                "expand": expand_pattern(pattern, POOL),
            }
        )
    versions = [{"in": v, "out": format_version(v), "supported": v.isascii()} for v in VERSIONS]
    return {"patterns": patterns, "versions": versions}


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "mlox name patterns and version strings, answered by wraithguard.sort.graph."
            "expand_pattern, wraithguard.rules.pattern_has_meta and wraithguard.versions."
            "format_version; run by tests/test_pattern_cases.py and viewer-shell/ordercore/"
            "tests/pattern_cases.rs. Regenerate: python tools/gen_pattern_cases.py"
        ),
        "pool": POOL,
        **out,
    }
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"{len(out['patterns'])} patterns, {len(out['versions'])} versions -> {OUT.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
