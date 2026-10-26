r"""Regenerate ``tests/fixtures/momw_cases.json`` from the Python MOMW code.

The inputs are written here; the answers are what ``wraithguard.momw`` and
``wraithguard.configurator.cfglines.curated_covers`` give. ``ordercore::momw``
(``viewer-shell/ordercore``) runs the same file, so the two cannot drift.

The ``yml`` cases are answered by ``ordercore::momw``'s reader (a YAML parser, through the
native module, which must be built): the entries, or ``refused`` for a file that is not
MOMW's. Those answers were read by hand when written; the rest are the Python's.

Run: ``python tools/gen_momw_cases.py``.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "momw_cases.json"

REALISTIC = """\
# MOMW's plugin-order.yml, abridged
- file_name: Morrowind.esm
  on_lists:
    - expanded-vanilla
    - i-heart-vanilla

- file_name: Tribunal.esm
  on_lists:
    - expanded-vanilla
    - i-heart-vanilla
- file_name: Foo.esp
  for_mod: Foo Mod
  needs_cleaning: true
  on_lists:
    - expanded-vanilla
  depends:
    - file_name: Bar.esp
      on_lists:
        - nope
- file_name: Bar.esp
  for_mod: "Bar Mod"
  needs_cleaning: false
  on_lists:
    - 'expanded-vanilla'
    - i-heart-vanilla
- file_name: Baz.esp
  on_lists:
    - i-heart-vanilla
"""

YML_INPUTS: list[tuple[str, str]] = [
    ("empty", ""),
    ("only comments and blank lines", "# one\n\n   # indented\n\t\n"),
    ("one entry", "- file_name: Morrowind.esm\n  on_lists:\n    - expanded-vanilla\n"),
    ("a realistic fragment", REALISTIC),
    (
        "quotes are YAML's",
        '- file_name: "Foo.esp"\n  for_mod: \'"x"\'\n  on_lists:\n    - "a"\n    - \'b\'\n'
        "    - \"'c'\"\n- file_name: 'Bar.esp'\n",
    ),
    (
        "needs_cleaning as YAML's booleans",
        "- file_name: A.esp\n  needs_cleaning: true\n- file_name: B.esp\n  needs_cleaning: True\n"
        "- file_name: C.esp\n  needs_cleaning: TRUE\n- file_name: D.esp\n  needs_cleaning: yes\n"
        "- file_name: E.esp\n  needs_cleaning: Yes\n- file_name: H.esp\n  needs_cleaning: false\n"
        "- file_name: I.esp\n  needs_cleaning: no\n- file_name: J.esp\n  needs_cleaning:\n",
    ),
    ("needs_cleaning as a number is refused", "- file_name: F.esp\n  needs_cleaning: 1\n"),
    ("needs_cleaning as a quoted true is true", '- file_name: K.esp\n  needs_cleaning: "true"\n'),
    ("needs_cleaning as a word is refused", "- file_name: L.esp\n  needs_cleaning: maybe\n"),
    (
        "an entry with no file_name is refused",
        "- for_mod: Only\n  on_lists:\n    - a\n- file_name: Kept.esp\n",
    ),
    ("an empty file_name is refused", '- file_name: ""\n  for_mod: x\n- file_name: Kept.esp\n'),
    ("a duplicate file_name is refused", "- file_name: A.esp\n  file_name: B.esp\n"),
    ("a duplicate for_mod is refused", "- file_name: A.esp\n  for_mod: One\n  for_mod: Two\n"),
    ("file_name after another key on the dash line", "- for_mod: M\n  file_name: A.esp\n"),
    (
        "depends is passed over, with what is nested in it",
        "- file_name: A.esp\n  depends:\n    - file_name: Dep.esp\n    - other\n"
        "  on_lists:\n    - l1\n",
    ),
    (
        "other blocks are passed over",
        "- file_name: A.esp\n  tags:\n    - x\n    - y\n  on_lists:\n    - l1\n",
    ),
    (
        "a duplicate on_lists is refused",
        "- file_name: A.esp\n  on_lists:\n    - l1\n  depends:\n    - d\n  on_lists:\n    - l2\n",
    ),
    (
        "lines indented less than a list item are its continuation",
        "- file_name: A.esp\n  on_lists:\n   - three\n    - four\n      - six\n",
    ),
    ("an empty list item is refused", "- file_name: A.esp\n  on_lists:\n    - real\n    -\n"),
    ("empty strings are list items", "- file_name: A.esp\n  on_lists:\n    - \"\"\n    - ''\n"),
    ("on_lists as a number is refused", "- file_name: A.esp\n  on_lists: 5\n"),
    ("a list with no items", "- file_name: A.esp\n  on_lists:\n- file_name: B.esp\n"),
    ("a flow list is read", "- file_name: A.esp\n  on_lists: [a, b]\n"),
    ("a trailing comment is not in the value", "- file_name: A.esp # note\n"),
    (
        "CRLF line ends",
        "- file_name: A.esp\r\n  needs_cleaning: yes\r\n  on_lists:\r\n    - l\r\n",
    ),
    ("CR line ends", "- file_name: A.esp\r  on_lists:\r    - l\r"),
    ("a byte-order mark is passed over", "﻿- file_name: A.esp\n- file_name: B.esp\n"),
    ("tabs as indentation are refused", "- file_name: A.esp\n\ton_lists:\n\t\t- l\n"),
    (
        "unknown keys are passed over",
        "- file_name: A.esp\n  version: 1.2\n  author: Me\n  on_lists:\n    - l\n",
    ),
    ("a key indented deeper is refused", "- file_name: A.esp\n    for_mod: deep\n"),
    ("a colon and space in a plain value is refused", "- file_name: A.esp\n  for_mod: a: b\n"),
    ("a quoted colon is a value", '- file_name: A.esp\n  for_mod: "a: b"\n'),
    ("names with spaces and parentheses", "- file_name: Better Bodies (v2).esp\n"),
    ("a non-breaking space is refused", "- file_name:\u00a0A.esp\n"),
    ("no space after the dash is refused", "-file_name: A.esp\n- file_name: B.esp\n"),
    ("a line with no colon is refused", "- file_name: A.esp\n  stray text\n  on_lists:\n    - l\n"),
    ("quotes inside a value", '- file_name: A.esp\n  for_mod: He said "hi"\n'),
    ("a mapping at the top is refused", "stray: 1\n- file_name: A.esp\n"),
    ("an entry that is not a mapping is refused", "- just words\n- file_name: A.esp\n"),
    ("an unclosed quote is refused", '- file_name: "A.esp\n'),
    ("a file_name that is a number is its text", "- file_name: 7\n"),
    ("for_mod null is none", "- file_name: A.esp\n  for_mod: ~\n"),
]

DRIFT: list[tuple[str, list[str], list[str]]] = [
    ("consistent", ["A.esm", "B.esm", "C.esm"], ["A.esm", "B.esm", "C.esm"]),
    ("a swap is reported once", ["B.esm", "A.esm", "C.esm"], ["A.esm", "B.esm", "C.esm"]),
    ("only the first drift is reported", ["C.esm", "B.esm", "A.esm"], ["A.esm", "B.esm", "C.esm"]),
    (
        "the user's own plugins between are not compared",
        ["A.esm", "Mine.esp", "B.esm", "Other.esp", "C.esm"],
        ["A.esm", "B.esm", "C.esm"],
    ),
    ("names compare without regard to case", ["b.ESM", "a.esm"], ["A.esm", "B.esm"]),
    ("nothing in common", ["X.esp", "Y.esp"], ["A.esm", "B.esm"]),
    ("no curated list", ["A.esm", "B.esm"], []),
    ("no cfg", [], ["A.esm", "B.esm"]),
    (
        "a later duplicate in the list takes the rank",
        ["A.esm", "B.esm"],
        ["A.esm", "B.esm", "A.esm"],
    ),
    (
        "an earlier rank after a later is drift even when equal",
        ["B.esm", "B.esm"],
        ["A.esm", "B.esm"],
    ),
    ("the cfg repeats a plugin", ["A.esm", "B.esm", "A.esm"], ["A.esm", "B.esm"]),
]

COVERS: list[tuple[str, str, list[str], list[str]]] = [
    ("on the list", "Foo.esp", ["foo.esp"], []),
    ("on the list, any case", "FOO.ESP", ["foo.esp"], []),
    ("not on the list", "Mine.esp", ["foo.esp"], []),
    ("a clean_ copy of a needs-cleaning list plugin", "Clean_Foo.esp", ["foo.esp"], ["foo.esp"]),
    ("a clean_ copy that is not flagged", "Clean_Foo.esp", ["foo.esp"], []),
    ("a clean_ copy of a plugin not on the list", "Clean_Foo.esp", [], ["foo.esp"]),
    ("the yml already names the clean_ copy", "Clean_Foo.esp", ["clean_foo.esp"], []),
    ("only one clean_ is stripped", "clean_clean_foo.esp", ["foo.esp"], ["foo.esp"]),
    ("clean_ alone", "clean_", ["foo.esp"], ["foo.esp"]),
    ("nothing on the list", "Foo.esp", [], []),
]


def _sorted(items: object) -> list[str]:
    return sorted(items)  # type: ignore[call-overload,no-any-return]


def cases() -> dict[str, Any]:
    """The cases, answered by the Python bodies (not ordercore's, which they check)."""
    sys.path.insert(0, str(ROOT))
    from wraithguard import ordercore_bridge

    with ordercore_bridge.python_only():
        return _cases()


def _cases() -> dict[str, Any]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    import wraithguard_native

    from wraithguard.configurator.cfglines import curated_covers
    from wraithguard.momw import (
        base_order_matches_yml,
        curated_for_list,
        needs_cleaning_set,
        post_sort_findings,
    )

    yml: list[dict[str, Any]] = []
    parsed: dict[str, Any] = {}
    for name, text in YML_INPUTS:
        try:
            entries = [dict(e) for e in wraithguard_native.order_momw_parse(text.encode("utf-8"))]
        except ValueError:
            yml.append({"name": name, "text": text, "refused": True})
            continue
        parsed[name] = entries
        yml.append({"name": name, "text": text, "entries": entries})
    realistic = parsed["a realistic fragment"]

    lists = []
    for name, entries, list_name in [
        ("a list", realistic, "expanded-vanilla"),
        ("a list in another case", realistic, "EXPANDED-Vanilla"),
        ("another list", realistic, "i-heart-vanilla"),
        ("a list nothing is on", realistic, "nope"),
        ("no list name", realistic, ""),
        ("no entries", [], "expanded-vanilla"),
        ("quoted names", parsed["quotes are YAML's"], "a"),
    ]:
        got_set, got_order = curated_for_list(entries, list_name)  # type: ignore[arg-type]
        lists.append(
            {
                "name": name,
                "entries": entries,
                "list": list_name,
                "set": _sorted(got_set),
                "order": got_order,
            }
        )

    cleaning = [
        {
            "name": name,
            "entries": entries,
            "set": _sorted(needs_cleaning_set(entries)),  # type: ignore[arg-type]
        }
        for name, entries in [
            ("a realistic fragment", realistic),
            ("every form", parsed["needs_cleaning as YAML's booleans"]),
            ("none", []),
        ]
    ]

    drift = [
        {
            "name": n,
            "base": b,
            "curated": c,
            "warnings": base_order_matches_yml(b, c),
        }
        for n, b, c in DRIFT
    ]

    covers = [
        {
            "name": n,
            "plugin": p,
            "curated": c,
            "needs_cleaning": nc,
            "expect": curated_covers(p, set(c), set(nc)),
        }
        for n, p, c, nc in COVERS
    ]

    base = ["Morrowind.esm", "Tribunal.esm", "Foo.esp", "Mine.esp", "Baz.esp", "Clean_Foo.esp"]
    curated, order = curated_for_list(realistic, "expanded-vanilla")  # type: ignore[arg-type]
    findings: list[dict[str, Any]] = []
    for name, final, base_names, entries, cur, cur_order, declared, list_name in [
        (
            "a sorted order",
            ["Morrowind.esm", "Foo.esp", "Mine.esp"],
            base,
            realistic,
            curated,
            order,
            ["mine.esp"],
            "expanded-vanilla",
        ),
        ("no sort ran", None, base, realistic, curated, order, ["mine.esp"], "expanded-vanilla"),
        (
            "an empty sort result falls back to the cfg",
            [],
            base,
            realistic,
            curated,
            order,
            ["mine.esp"],
            "expanded-vanilla",
        ),
        ("no entries", ["Foo.esp"], base, [], curated, order, [], "expanded-vanilla"),
        ("no list name", ["Foo.esp"], base, realistic, curated, order, [], None),
        ("an empty list name", ["Foo.esp"], base, realistic, curated, order, [], ""),
        ("no curated set", ["Foo.esp"], base, realistic, set(), [], [], "expanded-vanilla"),
        ("an orphan", ["Foo.esp"], base, realistic, curated, order, [], "expanded-vanilla"),
        (
            "all declared",
            ["Foo.esp"],
            base,
            realistic,
            curated,
            order,
            ["mine.esp", "baz.esp"],
            "expanded-vanilla",
        ),
        (
            "drift in the cfg",
            ["Foo.esp"],
            ["Tribunal.esm", "Morrowind.esm", "Foo.esp"],
            realistic,
            curated,
            order,
            [],
            "expanded-vanilla",
        ),
        (
            "every kind at once",
            ["Foo.esp", "Bar.esp"],
            ["Tribunal.esm", "Morrowind.esm", "Foo.esp", "Mine.esp"],
            realistic,
            curated,
            order,
            [],
            "expanded-vanilla",
        ),
    ]:
        findings.append(
            {
                "name": name,
                "final_order": final,
                "base": base_names,
                "entries": entries,
                "curated": _sorted(cur),
                "curated_order": cur_order,
                "declared": declared,
                "list": list_name,
                "warnings": post_sort_findings(
                    final, base_names, entries, set(cur), cur_order, set(declared), list_name  # type: ignore[arg-type]
                ),
            }
        )
    return {
        "yml": yml,
        "lists": lists,
        "cleaning": cleaning,
        "drift": drift,
        "covers": covers,
        "findings": findings,
    }


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    body = cases()
    doc = {
        "_about": (
            "MOMW's plugin-order.yml (ordercore::momw's YAML reader: the entries, or refused for "
            "a file that is not MOMW's), the curated lists and the post-sort findings, answered "
            "by wraithguard.momw; run by "
            "tests/test_momw_cases.py and viewer-shell/ordercore/tests/momw_cases.rs. "
            "Regenerate: python tools/gen_momw_cases.py"
        ),
        **body,
    }
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    counts = {k: len(v) for k, v in body.items()}
    print(f"{counts} -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
