r"""Regenerate ``tests/fixtures/export_cases.json`` from the Python export code.

The inputs are written here (and many more made from a fixed seed); the answers are what
``wraithguard.configurator`` and ``wraithguard_toolkit`` give. ``ordercore::export``
(``viewer-shell/ordercore``) runs the same file, so the two cannot drift.

Families:

* ``lines``: the ``data=`` line helpers of ``cfglines.py`` over strings, and the matchers
  (``configurator_remove_matches``, ``curated_covers``, ``orphan_cfg_entries``,
  ``detect_data_quoting``, ``find_anchor_index``);
* ``inserts``: ``insert_data_paths`` (the lines and the notes it prints);
* ``tomls``: ``generate_customizations_toml`` (the text and the warnings it prints);
* ``cfgs``: ``read_cfg`` of a cfg's text, and ``render_cfg_lines`` of plans over it;
* ``removals``: ``plugins_needing_removal`` and ``data_paths_needing_removal``;
* ``writes``: ``write_plan`` end to end over a temp folder: the files left behind (a ``.bak``
  carries the fixed stamp) and what it reports having written.

The TOML is written by Python in text mode, so on Windows its lines end in ``\r\n``; the cases
record it with ``\n`` and the Rust side is told which ending to write.

Run: ``python tools/gen_export_cases.py``.
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from types import SimpleNamespace
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "export_cases.json"
STAMP = "20261022-120000"


class _FixedClock(datetime):
    """``datetime`` that always says it is 2026-10-22 12:00:00."""

    @classmethod
    def now(cls, tz: Any = None) -> _FixedClock:
        return cls(2026, 10, 22, 12, 0, 0)


class Rng:
    """A small generator of its own, so the file does not move with Python's ``random``."""

    def __init__(self, seed: int) -> None:
        """Start from ``seed``."""
        self.state = (seed * 2654435761 + 12345) % (1 << 32)

    def below(self, n: int) -> int:
        """A number from 0 up to, not including, ``n``."""
        self.state = (self.state * 1664525 + 1013904223) % (1 << 32)
        return (self.state >> 8) % n

    def chance(self, num: int, den: int) -> bool:
        """Whether a ``num`` in ``den`` chance came up."""
        return self.below(den) < num

    def pick(self, items: list[str]) -> str:
        """One of the items."""
        return items[self.below(len(items))]

    def some(self, items: list[str], num: int = 1, den: int = 2) -> list[str]:
        """Each item kept with a ``num`` in ``den`` chance."""
        return [x for x in items if self.chance(num, den)]

    def shuffled(self, items: list[str]) -> list[str]:
        """The items in another order."""
        out = list(items)
        for i in range(len(out) - 1, 0, -1):
            j = self.below(i + 1)
            out[i], out[j] = out[j], out[i]
        return out


def _out(fn: Any, *args: Any, **kwargs: Any) -> tuple[Any, list[str]]:
    """The value of ``fn`` and the lines it printed."""
    buffer = io.StringIO()
    with contextlib.redirect_stdout(buffer):
        value = fn(*args, **kwargs)
    return value, buffer.getvalue().splitlines()


# ---------------------------------------------------------------- lines

STRINGS: list[str] = [
    "",
    " ",
    "C:\\Mods\\Cat\\Mod",
    "C:/Mods/Cat/Mod/",
    '"C:\\Mods\\A & B"',
    "'C:\\Mods\\Quoted'",
    "A && B",
    'say &"hi&" & bye',
    "trailing &",
    "&",
    "&&&",
    '"',
    '""',
    "'",
    "''",
    '"unclosed',
    'unopened"',
    "  spaced out  ",
    "a'b",
    "a'''b",
    "a\nb",
    "a'b\nc",
    "a'''b\nc\"d\\e",
    "back\\slash",
    "Data Files",
    "D:\\Morrowind\\Data Files\\",
    "d:/morrowind/DATA FILES",
    "Data Files Extra",
    "data=C:\\Mods\\A",
    '  Data = "C:\\Mods\\A & B"  ',
    "DATA=bare path",
    "data=",
    "data = ",
    "content=X.esp",
    "#data=C:\\x",
    "data=C:\\Mods\\A\u00e9",
    "caf\u00e9/\u00c9COLE",
    "\u0130stanbul\\\u03a3",
    "\t\u00a0tab\u2003",
]

RAW_LINES: list[list[str]] = [
    [],
    ["data=C:\\A", "data=C:\\B"],
    ['data="C:\\A"', 'data="C:\\B"'],
    ['data="C:\\A"', "data=C:\\B"],
    ['data="C:\\A"', "data=C:\\B", "content=X.esp", "# data=zzz"],
    ["content=X.esp"],
]

REMOVE_PAIRS: list[tuple[str, str]] = [
    ("Wares.esp", "content=Better Wares.esp"),
    ("Wares.esp", "content=Wares.esp"),
    ("C:/Mods/A", "data=C:\\Mods\\A"),
    ("C:/Mods/A", 'data="C:\\Mods\\A"'),
    ("Mods/A", "data=C:\\Mods\\A"),
    ("/Mods/A", "data=C:\\Mods\\A"),
    ("C:/Mods/A", "data=D:\\Mods\\A"),
    ("Mods\\A", "data='C:/Mods/A'"),
    ("a/b", "no equals here"),
    ("x\\", "data=x/"),
    ("\u00e9:/x", "data=\u00e9:/x"),
    ("", "content="),
    ("/", "data=/"),
]

ORPHAN_CASES: list[dict[str, Any]] = [
    {
        "content": [
            "Morrowind.esm",
            "Curated.esp",
            "Clean_Dirty.esp",
            "Mine.esp",
            "Declared.esp",
            "Tribunal.esm",
        ],
        "data": [
            "data=C:\\Mods\\A",
            'data="D:\\Morrowind\\Data Files"',
            "data=C:\\Mods\\Declared",
            "content=zzz",
        ],
        "curated": ["curated.esp", "dirty.esp"],
        "cleaning": ["dirty.esp"],
        "declared_plugins": ["declared.esp"],
        "declared_data": ["c:/mods/declared"],
    },
    {
        "content": ["Clean_Mine.esp", "Clean_Clean_X.esp", "Clean_Listed.esp"],
        "data": [],
        "curated": ["listed.esp", "x.esp"],
        "cleaning": ["x.esp"],
        "declared_plugins": [],
        "declared_data": [],
    },
    {
        "content": [],
        "data": [],
        "curated": [],
        "cleaning": [],
        "declared_plugins": [],
        "declared_data": [],
    },
]


def _fuzz_string(r: Rng) -> str:
    pool = [*"abAB/\\ ._-:&\"'=\t", "data=", "Data Files", "C:", "\u00e9", "&&", "'''", "\n"]
    return "".join(r.pick(pool) for _ in range(r.below(12)))


def lines_cases() -> dict[str, Any]:
    """The cfglines helpers over strings and lists."""
    from wraithguard.configurator.apply import configurator_remove_matches
    from wraithguard.configurator.cfglines import (
        cfg_line_value,
        curated_covers,
        detect_data_quoting,
        escape_cfg_value,
        extract_data_path_value,
        find_anchor_index,
        format_data_line,
        is_base_data_path,
        normalize_data_path,
        orphan_cfg_entries,
        toml_value,
        unescape_cfg_value,
    )

    r = Rng(11)
    strings = [*STRINGS, *(_fuzz_string(r) for _ in range(300))]
    one = [
        {
            "s": s,
            "escape": escape_cfg_value(s),
            "unescape": unescape_cfg_value(s),
            "format_bare": format_data_line(s, False),
            "format_quoted": format_data_line(s, True),
            "normalize": normalize_data_path(s),
            "toml": toml_value(s),
            "extract": extract_data_path_value(s),
            "base": is_base_data_path(s),
            "cfg_value": cfg_line_value(s),
        }
        for s in strings
    ]
    quoting = [{"lines": ls, "quoted": detect_data_quoting(ls)} for ls in RAW_LINES]
    haystack = ["content=Wares.esp", "content=Better Wares.esp", "data=C:\\Mods\\A", ""]
    anchors = [
        {"lines": haystack, "anchor": a, "index": find_anchor_index(haystack, a)}
        for a in ["wares", "BETTER", "mods\\a", "zzz", "", "content="]
    ]
    removes = [
        {"value": v, "line": ln, "matches": configurator_remove_matches(v, ln)}
        for v, ln in REMOVE_PAIRS
    ]
    covers = []
    for name in [
        "Foo.esp",
        "FOO.ESP",
        "Clean_Foo.esp",
        "clean_clean_foo.esp",
        "Clean_Bar.esp",
        "clean_",
        "x",
    ]:
        cur, clean = {"foo.esp", "bar.esp"}, {"foo.esp"}
        covers.append(
            {
                "name": name,
                "curated": sorted(cur),
                "cleaning": sorted(clean),
                "covers": curated_covers(name, cur, clean),
            }
        )
    orphans = []
    for case in ORPHAN_CASES:
        got = orphan_cfg_entries(
            case["content"],
            case["data"],
            curated_lower=set(case["curated"]),
            needs_cleaning_lower=set(case["cleaning"]),
            declared_plugins_lower=set(case["declared_plugins"]),
            declared_data_norms=set(case["declared_data"]),
        )
        orphans.append({**case, "orphans": [list(got[0]), list(got[1])]})
    return {
        "strings": one,
        "quoting": quoting,
        "anchors": anchors,
        "removes": removes,
        "covers": covers,
        "orphans": orphans,
    }


# ---------------------------------------------------------------- inserts

INSERTS: list[tuple[str, list[str], list[dict[str, Any]]]] = [
    ("nothing", [], []),
    ("an insert with no lines", [], [{"value": "C:\\New"}]),
    ("after an anchor", ["data=C:\\A", "data=C:\\B"], [{"value": "C:\\New", "after": "c:\\a"}]),
    ("before an anchor", ["data=C:\\A", "data=C:\\B"], [{"value": "C:\\New", "before": "C:\\B"}]),
    (
        "after beats before",
        ["data=C:\\A", "data=C:\\B"],
        [{"value": "N", "after": "C:\\A", "before": "C:\\B"}],
    ),
    ("an empty anchor is none", ["data=C:\\A"], [{"value": "N", "after": "", "before": ""}]),
    ("no anchor", ["data=C:\\A"], [{"value": "C:\\Loose"}]),
    ("an anchor that is not there", ["data=C:\\A"], [{"value": "N", "after": "zzz"}]),
    (
        "a duplicate of an existing path",
        ["data=C:\\A", "data=C:\\B"],
        [{"value": "c:/a/", "after": "C:\\B"}],
    ),
    (
        "a duplicate within the run",
        ["data=C:\\A"],
        [{"value": "N", "after": "A"}, {"value": "n\\", "after": "A"}],
    ),
    (
        "several on one anchor keep their order, before and after",
        ["data=C:\\A", "data=C:\\B"],
        [
            {"value": "P1", "after": "A"},
            {"value": "P2", "after": "A"},
            {"value": "Q1", "before": "A"},
            {"value": "Q2", "before": "A"},
            {"value": "R", "after": "B"},
        ],
    ),
    (
        "the first anchor match wins",
        ["data=C:\\Mods\\A", "data=C:\\Mods\\AB"],
        [{"value": "N", "after": "mods\\a"}],
    ),
    (
        "quoted lines make quoted inserts",
        ['data="C:\\A"', 'data="C:\\B"'],
        [{"value": "C:\\N & M", "after": "A"}],
    ),
    (
        "bare lines make bare inserts",
        ["data=C:\\A", "data=C:\\B"],
        [{"value": '"C:\\N"', "after": "A"}],
    ),
    ("a tie is bare", ['data="C:\\A"', "data=C:\\B"], [{"value": "N", "after": "A"}]),
    (
        "lines that are not data lines",
        ["content=X.esp", "data=C:\\A"],
        [{"value": "N", "after": "content"}],
    ),
    ("an empty value", ["data=C:\\A"], [{"value": "", "after": "A"}]),
    (
        "every kind at once",
        ["data=C:\\A", "data=C:\\B"],
        [
            {"value": "C:\\A"},
            {"value": "X1"},
            {"value": "X2", "after": "nope"},
            {"value": "X3", "before": "B"},
        ],
    ),
]


def insert_cases() -> list[dict[str, Any]]:
    """``insert_data_paths`` over hand-written and made-up lines."""
    from wraithguard.configurator.datapaths import insert_data_paths

    todo = list(INSERTS)
    for seed in range(1, 61):
        r = Rng(seed)
        paths = [
            "C:\\Mods\\A",
            "C:\\Mods\\A\\Sub",
            "C:\\Mods\\AB",
            "D:\\Other\\B",
            "C:\\Mods\\Caf\u00e9",
        ]
        quoted = r.below(3)
        lines = [
            (f'data="{p}"' if quoted == 0 or (quoted == 1 and r.below(2)) else f"data={p}")
            for p in r.some(paths, 2, 3)
        ]
        if r.below(3) == 0:
            lines.insert(r.below(len(lines) + 1), "content=X.esp")
        inserts = []
        for _ in range(r.below(5)):
            item: dict[str, Any] = {
                "value": r.pick([*paths, "E:\\New1", "E:\\New2", "e:/new1/", ""])
            }
            roll = r.below(4)
            if roll == 0:
                item["after"] = r.pick(["a", "ab", "other", "nope", "", "Sub"])
            elif roll == 1:
                item["before"] = r.pick(["a", "ab", "other", "nope", ""])
            inserts.append(item)
        todo.append((f"made up, seed {seed}", lines, inserts))
    out = []
    for name, lines, inserts in todo:
        norm = [
            {"value": i["value"], "after": i.get("after"), "before": i.get("before")}
            for i in inserts
        ]
        got, notes = _out(insert_data_paths, list(lines), [dict(i) for i in inserts])
        out.append(
            {
                "name": name,
                "lines": lines,
                "inserts": norm,
                "result": [list(g) for g in got],
                "notes": notes,
            }
        )
    return out


# ---------------------------------------------------------------- tomls

CONTENT_POOL = [
    "A.esp",
    "B.esp",
    "Better A.esp",
    "C.esm",
    "D.esp",
    "Wares.esp",
    "Better Wares.esp",
    "Mine.esp",
    "Mine2.esp",
    "Caf\u00e9.esp",
]
DATA_POOL = [
    "C:\\Mods\\A",
    "C:\\Mods\\A\\Sub",
    "C:\\Mods\\AB",
    "D:\\Other\\B",
    "D:\\Data Files",
    "D:\\Data Files\\Addons",
    "C:\\Mods\\It's",
    "C:\\Mods\\New1",
    "C:\\Mods\\New2",
]


def _block(r: Rng) -> dict[str, Any]:
    block: dict[str, Any] = {}
    if r.below(5):
        block["listName"] = r.pick(["total-overhaul", "my list", "", "It's"])
    for key in ("removeData", "removeContent", "removeFallback", "removeGroundcover"):
        if r.below(3) == 0:
            block[key] = [
                r.pick([*CONTENT_POOL, "wares.esp", "C:/Mods/A", "Sub"])
                for _ in range(1 + r.below(3))
            ]
    if r.below(4) == 0:
        block["replace"] = [
            {
                k: v
                for k, v in (("source", r.pick(CONTENT_POOL)), ("dest", r.pick(CONTENT_POOL)))
                if r.below(5)
            }
            for _ in range(1 + r.below(2))
        ]
    if r.below(4) == 0:
        block["append"] = [
            {"append": "fallback=X,1"} if r.below(2) else {"appendBlock": "line one\nline 'two'"}
            for _ in range(1 + r.below(2))
        ]
    return block


def _toml_args(r: Rng) -> dict[str, Any]:
    order = r.shuffled(r.some(CONTENT_POOL, 2, 3))
    subset = r.some(order, 1, 2)
    subset = [s.lower() if r.below(8) == 0 else s for s in subset]
    original_values = {n: r.pick(["Orig " + n, n.lower()]) for n in order if r.below(5) == 0}
    result: list[list[Any]] | None = None
    raw: list[dict[str, Any]] | None = None
    kind = r.below(4)
    chosen = r.some(DATA_POOL, 2, 3)
    users = r.some(chosen, 1, 2)
    if kind in (0, 1) and chosen:
        quoted = r.below(2)
        result = []
        for p in r.shuffled(chosen):
            line = f'data="{p}"' if quoted else f"data={p}"
            is_new = r.below(3) == 0
            result.append([line, is_new, p if is_new and r.below(4) else None])
    elif kind == 2:
        raw = [
            {
                k: v
                for k, v in (
                    ("value", p),
                    ("after", r.pick(["A", "AB", "Data Files"])),
                    ("before", r.pick(["B"])),
                )
                if k == "value" or r.below(3) == 0
            }
            for p in chosen
        ]
    anchors: dict[str, list[Any]] = {}
    for n in subset:
        if r.below(2):
            how = r.pick(["after", "before", "nearstart", "nearend", "none"])
            anchors[n.lower()] = [how, r.pick(CONTENT_POOL) if how in ("after", "before") else None]
    return {
        "original_blocks": [_block(r) for _ in range(r.below(3))],
        "final_content_order": order,
        "subset": subset,
        "original_content_values": original_values,
        "data_result": result,
        "raw_data_inserts": raw,
        "replace_dest_names": r.some(order, 1, 6),
        "user_data_values": users,
        "list_name": r.pick(["", "forced", "total-overhaul"]) if r.below(3) == 0 else None,
        "remove_content": r.some(CONTENT_POOL, 1, 6),
        "remove_data": r.some(DATA_POOL, 1, 6),
        "custom_anchors": anchors,
        "new_groundcover": r.some(["Grass.esp", "Grass2.esp"], 1, 4),
    }


def _hand_tomls() -> list[tuple[str, dict[str, Any]]]:
    base: dict[str, Any] = {
        "original_blocks": [],
        "final_content_order": [],
        "subset": [],
        "original_content_values": {},
        "data_result": None,
        "raw_data_inserts": None,
        "replace_dest_names": [],
        "user_data_values": [],
        "list_name": None,
        "remove_content": [],
        "remove_data": [],
        "custom_anchors": {},
        "new_groundcover": [],
    }
    order = ["Morrowind.esm", "A.esp", "Mine.esp", "Mine2.esp", "B.esp"]
    return [
        ("nothing at all", dict(base)),
        ("a list name, nothing else", {**base, "list_name": "total-overhaul"}),
        ("one custom plugin", {**base, "final_content_order": order, "subset": ["Mine.esp"]}),
        (
            "a run of two",
            {**base, "final_content_order": order, "subset": ["Mine.esp", "Mine2.esp"]},
        ),
        ("two runs", {**base, "final_content_order": order, "subset": ["A.esp", "B.esp"]}),
        (
            "the only plugin, no anchor",
            {**base, "final_content_order": ["Mine.esp"], "subset": ["Mine.esp"]},
        ),
        (
            "at the very start",
            {**base, "final_content_order": ["Mine.esp", "A.esp"], "subset": ["Mine.esp"]},
        ),
        (
            "a name inside another widens to its whole line",
            {
                **base,
                "final_content_order": ["Wares.esp", "Better Wares.esp", "Mine.esp"],
                "subset": ["Mine.esp"],
            },
        ),
        (
            "both neighbours ambiguous: the natural anchor and a warning",
            {
                **base,
                "final_content_order": ["A.esp", "Mine.esp", "A.esp.esp"],
                "subset": ["Mine.esp"],
            },
        ),
        (
            "the original value is kept",
            {
                **base,
                "final_content_order": order,
                "subset": ["Mine.esp"],
                "original_content_values": {"Mine.esp": "Mine Orig.esp"},
            },
        ),
        (
            "why each is where it is",
            {
                **base,
                "final_content_order": order,
                "subset": ["Mine.esp", "Mine2.esp", "B.esp"],
                "custom_anchors": {
                    "mine.esp": ["after", "A.esp"],
                    "mine2.esp": ["nearend", None],
                    "b.esp": ["none", None],
                },
            },
        ),
        (
            "a replace breaks a run and is noted",
            {
                **base,
                "original_blocks": [
                    {"listName": "x", "replace": [{"source": "Old.esp", "dest": "Mine2.esp"}]}
                ],
                "final_content_order": order,
                "subset": ["Mine.esp", "Mine2.esp"],
                "replace_dest_names": ["Mine2.esp"],
            },
        ),
        (
            "a replace with no dest in the order",
            {
                **base,
                "original_blocks": [
                    {"replace": [{"source": "Old.esp", "dest": "Gone.esp"}, {"source": "S"}, {}]}
                ],
            },
        ),
        (
            "data paths sorted: a run before a frozen line",
            {
                **base,
                "data_result": [
                    ["data=C:\\Mods\\New1", True, "C:\\Mods\\New1"],
                    ["data=C:\\Mods\\New2", True, "C:\\Mods\\New2"],
                    ["data=C:\\Mods\\A", False, None],
                ],
            },
        ),
        (
            "data paths sorted: a run at the end anchors after",
            {
                **base,
                "data_result": [
                    ["data=C:\\Mods\\A", False, None],
                    ["data=C:\\Mods\\New1", True, "C:\\Mods\\New1"],
                ],
            },
        ),
        (
            "data paths: a path already in the cfg is still ours",
            {
                **base,
                "data_result": [
                    ["data=C:\\Mods\\A", False, None],
                    ["data=C:\\Mods\\B", False, None],
                ],
                "user_data_values": ["c:/mods/b/"],
            },
        ),
        (
            "data paths: a prefix of another widens to its line",
            {
                **base,
                "data_result": [
                    ['data="D:\\Data Files"', False, None],
                    ['data="D:\\Data Files\\Addons"', False, None],
                    ["data=C:\\Mods\\New1", True, "C:\\Mods\\New1"],
                ],
            },
        ),
        (
            "data paths: no frozen line at all",
            {**base, "data_result": [["data=C:\\Mods\\New1", True, "C:\\Mods\\New1"]]},
        ),
        (
            "data paths unsorted: passed through with their anchors",
            {
                **base,
                "raw_data_inserts": [
                    {"value": "C:\\Mods\\A", "after": "x"},
                    {"value": "C:\\Mods\\B", "before": "y"},
                    {"value": "C:\\Mods\\C"},
                ],
            },
        ),
        (
            "removals, de-duplicated without regard to case",
            {
                **base,
                "original_blocks": [
                    {
                        "listName": "x",
                        "removeContent": ["A.esp", "a.esp"],
                        "removeData": ["C:/Mods/A"],
                    }
                ],
                "remove_content": ["a.ESP", "B.esp"],
                "remove_data": ["c:/mods/a", "C:/Mods/B"],
                "final_content_order": ["A.esp", "Better A.esp"],
            },
        ),
        (
            "two blocks: inserts only in the first",
            {
                **base,
                "original_blocks": [
                    {"listName": "one"},
                    {"listName": "two", "append": [{"append": "x"}]},
                ],
                "final_content_order": order,
                "subset": ["Mine.esp"],
                "remove_content": ["C.esp"],
                "new_groundcover": ["Grass.esp"],
            },
        ),
        (
            "groundcover and hand-written appends",
            {
                **base,
                "original_blocks": [
                    {"append": [{"append": "fallback=A,1"}, {"appendBlock": "a\nb"}]}
                ],
                "new_groundcover": ["Grass.esp", "Grass2.esp"],
            },
        ),
        (
            "values with quotes and newlines",
            {
                **base,
                "original_blocks": [{"listName": "It's"}],
                "remove_content": ["it's", "a'''b"],
            },
        ),
    ]


def toml_case(name: str, args: dict[str, Any]) -> dict[str, Any]:
    """``generate_customizations_toml`` for one set of arguments."""
    from wraithguard.configurator.emit import generate_customizations_toml

    blocks = args["original_blocks"]
    original = {"Customizations": blocks} if blocks else None
    result = None if args["data_result"] is None else [tuple(t) for t in args["data_result"]]
    anchors = {k: tuple(v) for k, v in args["custom_anchors"].items()}
    text, warnings = _out(
        generate_customizations_toml,
        original,
        args["final_content_order"],
        set(args["subset"]),
        args["original_content_values"],
        result,
        args["raw_data_inserts"],
        set(args["replace_dest_names"]),
        user_data_values=args["user_data_values"],
        list_name=args["list_name"],
        remove_content=args["remove_content"],
        remove_data=args["remove_data"],
        custom_anchors=anchors,
        new_groundcover=args["new_groundcover"],
    )
    return {"name": name, **args, "text": text, "warnings": warnings}


# ---------------------------------------------------------------- cfgs, writes

CFG_TEXTS: list[tuple[str, str]] = [
    ("empty", ""),
    ("one content line", "content=A.esp\n"),
    (
        "a realistic cfg",
        '# comment\nfallback=Weather_Clear,1\ndata=C:\\Mods\\A\ndata="C:\\Mods\\B"\n'
        "content=Morrowind.esm\ncontent=A.esp\ngroundcover=Grass.esp\ncontent=B.esp\nencoding=win1252\n",
    ),
    ("no trailing newline", "data=C:\\A\ncontent=X.esp"),
    ("crlf line endings", "data=C:\\A\r\ncontent=X.esp\r\ncontent=Y.esp\r\n"),
    ("a byte order mark", "\ufeffdata=C:\\A\ncontent=X.esp\n"),
    ("blank lines and spaces", "\n  content = X.esp  \n\n  DATA =  C:\\A  \n\n"),
    ("odd line breaks", "content=X.esp\x0bdata=C:\\A\x0ccontent=Y.esp\u2028# end\n"),
    ("only other keys", "fallback=A,1\nencoding=win1252\n"),
    ("non-ASCII names", "data=C:\\Mods\\Caf\u00e9\ncontent=Caf\u00e9.esp\n"),
    (
        "grass already declared",
        'content=A.esp\ngroundcover=Grass.esp\ngroundcover = "C:/x/GRASS2.esp"\n',
    ),
]


def cfg_cases() -> list[dict[str, Any]]:
    """``read_cfg`` over cfg text."""
    from wraithguard_toolkit import read_cfg, read_groundcover_names

    out = []
    for name, text in CFG_TEXTS:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "openmw.cfg"
            path.write_bytes(text.encode("utf-8"))
            lines, cpos, corder, dpos, dorder = read_cfg(path)
        out.append(
            {
                "name": name,
                "text": text,
                "lines": lines,
                "content_positions": cpos,
                "content_order": [list(c) for c in corder],
                "data_positions": dpos,
                "data_order": dorder,
                "groundcover": read_groundcover_names(lines),
            }
        )
    return out


def make_plan(
    cfg_text: str,
    r: Rng,
    *,
    subset: list[str] | None = None,
    data_sorted: bool = True,
) -> dict[str, Any]:
    """A plan over a cfg, with orders and inserts made up (not computed by the planner)."""
    from wraithguard_toolkit import read_cfg

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "openmw.cfg"
        path.write_bytes(cfg_text.encode("utf-8"))
        lines, cpos, corder, dpos, dorder = read_cfg(path)
    names = [v for v, _ in corder]
    customs = subset if subset is not None else r.some(["Mine.esp", "Mine2.esp", "New.esp"], 2, 3)
    final = r.shuffled([*names, *[c for c in customs if c not in names]])
    inserts = [
        {"value": p, "after": r.pick(["", "A", "B"]) or None, "before": None}
        for p in r.some(["C:\\Mods\\New1", "C:\\Mods\\New2"], 2, 3)
    ]
    from wraithguard.configurator.datapaths import insert_data_paths

    data_result = None
    if data_sorted:
        with contextlib.redirect_stdout(io.StringIO()):
            data_result = insert_data_paths(list(dorder), [dict(i) for i in inserts])
    return {
        "lines": lines,
        "content_positions": cpos,
        "data_positions": dpos,
        "final_order": final,
        "data_result": data_result,
        "new_groundcover": r.some(["Grass.esp", "Grass3.esp"], 1, 2),
        "subset": customs,
        "curated_set": r.some([*(n.lower() for n in names), "x.esp"], 1, 2),
        "base_order_names": names,
        "data_order": dorder,
        "data_inserts": inserts,
        "raw_toml_data_inserts": (
            [{"value": "C:\\Mods\\Raw", "after": "A", "before": None}] if r.below(2) else []
        ),
        "orphan_data_paths": r.some(["C:\\Mods\\Orphan"], 1, 3),
        "original_toml_data": {"Customizations": [_block(r)]} if r.below(2) else None,
        "original_content_values": {},
        "replace_dest_names": [],
        "custom_anchors": {c.lower(): ["after", names[0]] for c in customs if names and r.below(2)},
        "predicate_warnings": [],
        "yml_warnings": [],
    }


def _plan_for_python(plan: dict[str, Any]) -> dict[str, Any]:
    out = dict(plan)
    out["data_result"] = (
        None if plan["data_result"] is None else [tuple(t) for t in plan["data_result"]]
    )
    out["replace_dest_names"] = set(plan["replace_dest_names"])
    out["custom_anchors"] = {k: tuple(v) for k, v in plan["custom_anchors"].items()}
    out["curated_set"] = set(plan["curated_set"])
    return out


def render_case(
    name: str, cfg_text: str, plan: dict[str, Any], final: list[str] | None, data: list[str] | None
) -> dict[str, Any]:
    """``render_cfg_lines`` of a plan."""
    from wraithguard_toolkit import render_cfg_lines

    got = render_cfg_lines(_plan_for_python(plan), final, data)
    return {
        "name": name,
        "cfg": cfg_text,
        "plan": plan,
        "final_order": final,
        "data_order": data,
        "lines": got,
    }


def write_case(
    name: str,
    cfg_text: str | None,
    toml_text: str | None,
    plan: dict[str, Any],
    opts: dict[str, Any],
) -> dict[str, Any]:
    """``write_plan`` end to end over a temp folder."""
    import wraithguard_toolkit as tk

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        if cfg_text is not None:
            (root / "openmw.cfg").write_bytes(cfg_text.encode("utf-8"))
        if toml_text is not None:
            (root / "customizations.toml").write_bytes(toml_text.encode("utf-8"))
        args = SimpleNamespace(
            cfg=root / "openmw.cfg",
            write_cfg=opts["write_cfg"],
            emit_toml=(root / "customizations.toml") if opts["emit_toml"] else None,
            dry_run=opts["dry_run"],
            no_backup=opts["no_backup"],
            sort_data_paths=opts["sort_data_paths"],
            list_name=opts.get("list_name"),
        )
        with mock.patch.object(tk, "datetime", _FixedClock):
            result, _printed = _out(
                tk.write_plan,
                args,
                _plan_for_python(plan),
                opts.get("final_order"),
                opts.get("data_order"),
                set(opts.get("disabled_plugins") or []) or None,
                set(opts.get("disabled_data") or []) or None,
            )
        after = {
            p.name: (
                p.read_bytes().decode("utf-8").replace("\r\n", "\n")
                if p.suffix == ".toml"
                else p.read_bytes().decode("utf-8")
            )
            for p in sorted(root.iterdir())
        }
        # a .bak of the TOML is Python's text-mode write read back: normalise it the same way
        after = {k: (v.replace("\r\n", "\n") if ".toml" in k else v) for k, v in after.items()}
    return {
        "name": name,
        "cfg": cfg_text,
        "toml": toml_text,
        "plan": plan,
        "opts": opts,
        "wrote_cfg": result["wrote_cfg"],
        "wrote_toml": result["wrote_toml"],
        "after": after,
    }


def removal_cases() -> dict[str, Any]:
    """The two removal rules."""
    from wraithguard_toolkit import data_paths_needing_removal, plugins_needing_removal

    plugins = []
    for disabled, curated, base in [
        (["A.esp", "b.esp", "Zed.esp"], ["a.esp"], ["B.esp"]),
        (["A.esp", "b.esp", "Zed.esp"], [], ["B.esp", "zed.esp"]),
        (["A.esp", "a.ESP"], ["a.esp"], []),
        ([], ["a.esp"], []),
        (["A.esp"], [], []),
    ]:
        plugins.append(
            {
                "disabled": disabled,
                "curated": curated,
                "base": base,
                "result": plugins_needing_removal(disabled, curated, base),
            }
        )
    data = []
    for disabled, ours, base in [
        (
            ["data=C:\\Mods\\A", 'data="C:\\Mods\\B"', "data=C:\\Mods\\C"],
            ["c:/mods/a"],
            ["c:/mods/a", "c:/mods/b"],
        ),
        (["C:\\Mods\\A", "garbage", "data="], [], ["c:/mods/a", "garbage"]),
        ([], ["x"], ["y"]),
    ]:
        data.append(
            {
                "disabled": disabled,
                "ours": sorted(ours),
                "base": sorted(base),
                "result": data_paths_needing_removal(disabled, set(ours), set(base)),
            }
        )
    return {"plugins": plugins, "data": data}


def _write_cases() -> list[dict[str, Any]]:
    cfg = CFG_TEXTS[2][1]
    toml = "[[Customizations]]\nlistName = 'old'\n"
    out: list[dict[str, Any]] = []
    plan = make_plan(cfg, Rng(1), subset=["Mine.esp", "New.esp"])
    base_opts = {
        "write_cfg": True,
        "emit_toml": True,
        "dry_run": False,
        "no_backup": False,
        "sort_data_paths": True,
        "list_name": "total-overhaul",
    }
    out.append(write_case("both files, backed up", cfg, toml, plan, base_opts))
    out.append(
        write_case("both files, none existing", None, None, plan, {**base_opts, "write_cfg": False})
    )
    out.append(
        write_case("a dry run writes nothing", cfg, toml, plan, {**base_opts, "dry_run": True})
    )
    out.append(write_case("no backup", cfg, toml, plan, {**base_opts, "no_backup": True}))
    out.append(write_case("only the toml", cfg, toml, plan, {**base_opts, "write_cfg": False}))
    out.append(write_case("only the cfg", cfg, toml, plan, {**base_opts, "emit_toml": False}))
    out.append(
        write_case(
            "nothing asked for",
            cfg,
            toml,
            plan,
            {**base_opts, "write_cfg": False, "emit_toml": False},
        )
    )
    out.append(
        write_case("data not sorted", cfg, toml, plan, {**base_opts, "sort_data_paths": False})
    )
    out.append(
        write_case("a crlf cfg is written back with lf", CFG_TEXTS[4][1], toml, plan, base_opts)
    )
    out.append(write_case("a byte order mark is dropped", CFG_TEXTS[5][1], toml, plan, base_opts))
    out.append(write_case("no trailing newline gets one", CFG_TEXTS[3][1], None, plan, base_opts))
    out.append(
        write_case(
            "a hand-adjusted order",
            cfg,
            toml,
            plan,
            {
                **base_opts,
                "final_order": list(reversed(plan["final_order"])),
                "data_order": list(reversed(plan["data_order"])),
            },
        )
    )
    out.append(
        write_case(
            "opted-out items",
            cfg,
            toml,
            plan,
            {
                **base_opts,
                "disabled_plugins": ["A.esp", "Mine.esp"],
                "disabled_data": ["data=C:\\Mods\\A", "C:\\Mods\\Gone"],
            },
        )
    )
    for seed in range(2, 42):
        r = Rng(seed)
        text = r.pick([t for _, t in CFG_TEXTS])
        made = make_plan(text, r, data_sorted=bool(r.below(2)))
        opts = {
            "write_cfg": bool(r.below(4)),
            "emit_toml": bool(r.below(4)),
            "dry_run": r.below(5) == 0,
            "no_backup": r.below(4) == 0,
            "sort_data_paths": bool(r.below(2)),
            "list_name": r.pick(["", "forced", "total-overhaul"]) if r.below(2) else None,
            "disabled_plugins": r.some(made["base_order_names"], 1, 4),
            "disabled_data": r.some(made["data_order"], 1, 4),
        }
        out.append(
            write_case(
                f"made up, seed {seed}",
                text if r.below(5) else None,
                toml if r.below(2) else None,
                made,
                opts,
            )
        )
    return out


def _render_cases() -> list[dict[str, Any]]:
    out = []
    for name, text in CFG_TEXTS:
        r = Rng(len(name) + 3)
        for sorted_data in (True, False):
            plan = make_plan(text, r, data_sorted=sorted_data)
            out.append(
                render_case(
                    f"{name}, data {'sorted' if sorted_data else 'not sorted'}",
                    text,
                    plan,
                    None,
                    None,
                )
            )
        plan = make_plan(text, r)
        out.append(
            render_case(
                f"{name}, orders overridden",
                text,
                plan,
                list(reversed(plan["final_order"])),
                list(reversed(plan["data_order"])),
            )
        )
    return out


def cases() -> dict[str, Any]:
    """The cases, answered by the Python bodies (not ordercore's, which they check)."""
    sys.path.insert(0, str(ROOT))
    from wraithguard import ordercore_bridge

    with ordercore_bridge.python_only():
        return _cases()


def _cases() -> dict[str, Any]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    tomls = [toml_case(n, a) for n, a in _hand_tomls()]
    tomls += [toml_case(f"made up, seed {s}", _toml_args(Rng(s))) for s in range(1, 301)]
    return {
        "lines": lines_cases(),
        "inserts": insert_cases(),
        "tomls": tomls,
        "cfgs": cfg_cases(),
        "renders": _render_cases(),
        "removals": removal_cases(),
        "writes": _write_cases(),
    }


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "Export: the data= line helpers, insert_data_paths, generate_customizations_toml, the cfg "
            "splice and write_plan end to end, answered by wraithguard.configurator and "
            "wraithguard_toolkit; run by tests/test_export_cases.py and "
            "viewer-shell/ordercore/tests/export_cases.rs. Regenerate: python tools/gen_export_cases.py"
        ),
        **out,
    }
    OUT.write_bytes((json.dumps(doc, indent=1, ensure_ascii=False) + "\n").encode("utf-8"))
    print(
        f"{len(out['lines']['strings'])} strings, {len(out['inserts'])} inserts, {len(out['tomls'])} tomls, "
        f"{len(out['cfgs'])} cfgs, {len(out['renders'])} renders, {len(out['writes'])} writes -> {OUT.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
