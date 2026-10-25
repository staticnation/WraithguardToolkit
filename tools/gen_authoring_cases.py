r"""Regenerate ``tests/fixtures/authoring_cases.json`` from the Python rule maker.

The inputs are written here (and many more made from a fixed seed); the answers are what
``wraithguard.rules.authoring``, ``wraithguard.rules.derive`` and the user-rule-file functions of
``wraithguard_toolkit`` give. ``ordercore::authoring`` and ``ordercore::derive``
(``viewer-shell/ordercore``) run the same file, so the two cannot drift.

Families:

* ``refs``: ``format_ref`` over citations;
* ``rules``: a rule as data, how it renders and every problem ``validate`` finds;
* ``proposals``: ``propose_all`` and its parts over master lists and conflict records (the
  Python's dicts arrive as ordered lists, because their order matters);
* ``files``: ``append_authored_rule`` and ``append_user_rule`` over a temp folder (the text
  returned or the refusal, and the file left behind);
* ``frozen``: ``order_rule_frozen_conflicts``.

Python writes the rules file in text mode, so on Windows its lines end in ``\r\n``; the cases record
it with ``\n`` and the Rust side is told which ending to write.

Run: ``python tools/gen_authoring_cases.py``.
"""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "authoring_cases.json"


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

    def pick(self, items: list[Any]) -> Any:
        """One of the items."""
        return items[self.below(len(items))]

    def some(self, items: list[Any], num: int = 1, den: int = 2) -> list[Any]:
        """Each item kept with a ``num`` in ``den`` chance."""
        return [x for x in items if self.chance(num, den)]


# ---------------------------------------------------------------- rules as data


def plugin(name: str) -> dict[str, Any]:
    """An expression: a plugin."""
    return {"t": "plugin", "name": name}


def desc(pattern: str, name: str, negated: bool = False) -> dict[str, Any]:
    """An expression: ``[DESC]``."""
    return {"t": "desc", "pattern": pattern, "plugin": name, "negated": negated}


def size(n: int, name: str, negated: bool = False) -> dict[str, Any]:
    """An expression: ``[SIZE]``."""
    return {"t": "size", "size": n, "plugin": name, "negated": negated}


def ver(op: str, version: str, name: str) -> dict[str, Any]:
    """An expression: ``[VER]``."""
    return {"t": "ver", "operator": op, "version": version, "plugin": name}


def group(op: str, *parts: dict[str, Any]) -> dict[str, Any]:
    """An expression: a group."""
    return {"t": "group", "operator": op, "parts": list(parts)}


def rule(kind: str, **kw: Any) -> dict[str, Any]:
    """A rule as the case file writes it."""
    base: dict[str, Any] = {
        "kind": kind,
        "plugins": [],
        "expressions": [],
        "message": "",
        "ref": "",
        "priority": 0,
        "section": "",
        "comment": "",
        "force_block": False,
    }
    return {**base, **kw}


def expr_from_json(e: dict[str, Any]) -> Any:
    """The Python expression a JSON one stands for."""
    from wraithguard.rules import authoring as a

    t = e["t"]
    if t == "plugin":
        return a.Plugin(e["name"])
    if t == "desc":
        return a.Desc(e["pattern"], e["plugin"], e["negated"])
    if t == "size":
        return a.Size(e["size"], e["plugin"], e["negated"])
    if t == "ver":
        return a.Ver(e["operator"], e["version"], e["plugin"])
    return a.Group(e["operator"], tuple(expr_from_json(p) for p in e["parts"]))


def expr_to_json(e: Any) -> dict[str, Any]:
    """The JSON of a Python expression."""
    from wraithguard.rules import authoring as a

    if isinstance(e, a.Plugin):
        return plugin(e.name)
    if isinstance(e, a.Desc):
        return desc(e.pattern, e.plugin, e.negated)
    if isinstance(e, a.Size):
        return size(e.size, e.plugin, e.negated)
    if isinstance(e, a.Ver):
        return ver(e.operator, e.version, e.plugin)
    return group(e.operator, *(expr_to_json(p) for p in e.parts))


def rule_from_json(r: dict[str, Any]) -> Any:
    """The Python rule a JSON one stands for."""
    from wraithguard.rules.authoring import Rule

    return Rule(
        kind=r["kind"],
        plugins=list(r["plugins"]),
        expressions=[expr_from_json(e) for e in r["expressions"]],
        message=r["message"],
        ref=r["ref"],
        priority=r["priority"],
        section=r["section"],
        comment=r["comment"],
        force_block=r["force_block"],
    )


def rule_to_json(r: Any) -> dict[str, Any]:
    """The JSON of a Python rule."""
    return rule(
        r.kind,
        plugins=list(r.plugins),
        expressions=[expr_to_json(e) for e in r.expressions],
        message=r.message,
        ref=r.ref,
        priority=r.priority,
        section=r.section,
        comment=r.comment,
        force_block=r.force_block,
    )


LONG = "x" * 61
NAMES = [
    "A.esp",
    "B.esm",
    "Some Mod.omwaddon",
    "C.OMWGAME",
    "D.omwscripts",
    "Caf\u00e9.esp",
    "E*.esp",
    "F?.esp",
    "G <VER>.esp",
]
BAD_NAMES = [
    "",
    "  ",
    "noext",
    "bad;name.esp",
    "br[acket].esp",
    "tab\tname.esp",
    "x.esp\ny.esp",
    ".esp",
    "a.espx",
    "a.esp*",
    "a.esp**",
    "\u00e9.txt",
    "a.esp\n",
]

HAND_RULES: list[tuple[str, dict[str, Any]]] = [
    ("a good order rule", rule("Order", plugins=["A.esp", "B.esm"], ref="readme")),
    (
        "an order rule with comment and section",
        rule(
            "Order",
            plugins=[" A.esp ", "B.esm"],
            ref="r",
            comment="why\n\n  what  ",
            section="@@My Mod ",
        ),
    ),
    ("an order rule of one plugin", rule("Order", plugins=["A.esp"], ref="r")),
    (
        "an order rule listing a plugin twice",
        rule("Order", plugins=["A.esp", "a.ESP", "B.esm"], ref="r"),
    ),
    ("an order rule with nothing", rule("Order", ref="r")),
    ("a NearStart", rule("NearStart", plugins=["A.esp"], ref="r")),
    ("a NearEnd of nothing", rule("NearEnd", ref="r")),
    ("bad names in an order rule", rule("Order", plugins=BAD_NAMES, ref="r")),
    (
        "expansions in an order rule",
        rule("Order", plugins=["E*.esp", "F?.esp", "G <VER>.esp"], ref="r"),
    ),
    (
        "a short requires",
        rule(
            "Requires",
            expressions=[plugin("A.esp"), plugin("B.esm")],
            message="needs B",
            ref="readme",
        ),
    ),
    (
        "a requires with a highlight and a long message",
        rule(
            "Requires",
            expressions=[plugin("A.esp"), plugin("B.esm")],
            message=LONG,
            ref="r",
            priority=2,
        ),
    ),
    (
        "a requires with a bracket in the message",
        rule(
            "Requires",
            expressions=[plugin("A.esp"), plugin("B.esm")],
            message="see [here]",
            ref="r",
        ),
    ),
    (
        "a multi-line message",
        rule("Note", expressions=[plugin("A.esp")], message="one\n\n  two  \nthree", ref="r"),
    ),
    (
        "a forced block",
        rule("Note", expressions=[plugin("A.esp")], message="m", ref="r", force_block=True),
    ),
    (
        "a highlight with no message",
        rule("Note", expressions=[plugin("A.esp")], ref="r", priority=1),
    ),
    (
        "a url citation",
        rule(
            "Note",
            expressions=[plugin("A.esp")],
            message="m",
            ref="see   https://example.com/x  or www.y.z",
        ),
    ),
    (
        "a citation with no url",
        rule("Note", expressions=[plugin("A.esp")], message="m", ref="  the   readme "),
    ),
    ("no citation and no message", rule("Note", expressions=[plugin("A.esp")])),
    (
        "a conflict of two",
        rule("Conflict", expressions=[plugin("A.esp"), plugin("B.esm")], message="m", ref="r"),
    ),
    ("a conflict of one", rule("Conflict", expressions=[plugin("A.esp")], message="m", ref="r")),
    (
        "a requires of three",
        rule(
            "Requires",
            expressions=[plugin("A.esp"), plugin("B.esm"), plugin("C.OMWGAME")],
            message="m",
            ref="r",
        ),
    ),
    ("a note of nothing", rule("Note", message="m", ref="r")),
    (
        "a patch with a NOT",
        rule(
            "Patch",
            expressions=[plugin("A.esp"), group("NOT", plugin("B.esm"))],
            message="m",
            ref="r",
        ),
    ),
    (
        "a patch with a nested NOT",
        rule(
            "Patch",
            expressions=[
                group(
                    "ALL",
                    plugin("A.esp"),
                    group("ANY", group("NOT", plugin("C.OMWGAME")), plugin("D.omwscripts")),
                ),
                plugin("B.esm"),
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "a good patch",
        rule(
            "Patch",
            expressions=[plugin("A.esp"), group("ALL", plugin("B.esm"), plugin("C.OMWGAME"))],
            message="m",
            ref="r",
        ),
    ),
    ("an unknown kind", rule("Frobnicate", plugins=["A.esp"], ref="r")),
    ("the Version header", rule("Version", ref="r")),
    (
        "a bad highlight level",
        rule("Note", expressions=[plugin("A.esp")], message="m", ref="r", priority=7),
    ),
    (
        "a negative highlight level",
        rule("Note", expressions=[plugin("A.esp")], message="m", ref="r", priority=-1),
    ),
    (
        "a highlight on a requires",
        rule(
            "Requires",
            expressions=[plugin("A.esp"), plugin("B.esm")],
            message="m",
            ref="r",
            priority=3,
        ),
    ),
    (
        "a highlight on a conflict and a patch",
        rule(
            "Conflict",
            expressions=[plugin("A.esp"), plugin("B.esm")],
            message="m",
            ref="r",
            priority=1,
        ),
    ),
    (
        "groups with odd arity",
        rule(
            "Note",
            expressions=[
                group("NOT"),
                group("NOT", plugin("A.esp"), plugin("B.esm")),
                group("ALL"),
                group("ANY"),
                group("ALL", plugin("A.esp")),
                group("ANY", plugin("A.esp")),
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "a group with another operator",
        rule(
            "Note",
            expressions=[group("XOR", plugin("A.esp"), plugin("B.esm"))],
            message="m",
            ref="r",
        ),
    ),
    (
        "a long group breaks",
        rule(
            "Note",
            expressions=[
                group("ALL", *[plugin(f"Some Long Plugin Name {i}.esp") for i in range(8)])
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "a nested long group breaks",
        rule(
            "Note",
            expressions=[
                group(
                    "ANY",
                    group("ALL", *[plugin(f"Some Long Plugin Name {i}.esp") for i in range(5)]),
                    *[plugin(f"Other Name {i}.esp") for i in range(4)],
                )
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "version predicates",
        rule(
            "Note",
            expressions=[
                ver("<", "1.2.3a", "A.esp"),
                ver("=", "1_3a", "A.esp"),
                ver(">", "77g", "A.esp"),
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "bad version predicates",
        rule(
            "Note",
            expressions=[
                ver("~", "1.2", "A.esp"),
                ver("=", "x", "A.esp"),
                ver("<", "", "A.esp"),
                ver("=", " 1.2 ", "A.esp"),
                ver("=", "1.2.3.4.5", "A.esp"),
                ver("=", "1.2\n", "A.esp"),
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "size predicates",
        rule(
            "Note",
            expressions=[size(0, "A.esp"), size(123, "A.esp", True), size(-5, "A.esp")],
            message="m",
            ref="r",
        ),
    ),
    (
        "description predicates that compile",
        rule(
            "Note",
            expressions=[
                desc("[Tt]ribunal", "A.esp"),
                desc("abc", "B.esm", True),
                desc("x{2}", "A.esp"),
                desc(".*", "A.esp"),
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "a description regex Python rejects",
        rule("Note", expressions=[desc("(abc", "A.esp")], message="m", ref="r"),
    ),
    (
        "a description regex not read",
        rule("Note", expressions=[desc("(?i)abc", "A.esp")], message="m", ref="r"),
    ),
    (
        "bad plugin names inside expressions",
        rule(
            "Requires",
            expressions=[
                plugin("bad;name.esp"),
                group("ALL", plugin("noext"), size(1, "x[y].esp")),
            ],
            message="m",
            ref="r",
        ),
    ),
    ("a non-ASCII name that is fine", rule("Order", plugins=["Caf\u00e9.esp", "B.esm"], ref="r")),
    ("a non-ASCII name that is not", rule("Order", plugins=["Caf\u00e9.txt", "B.esm"], ref="r")),
    ("a name with a quote in it", rule("Order", plugins=["it's.txt", 'say "x".txt'], ref="r")),
    (
        "expansions in expressions are named once, sorted",
        rule(
            "Note",
            expressions=[
                plugin("Z*.esp"),
                plugin("A?.esp"),
                plugin("Z*.esp"),
                plugin("M <VER>.esp"),
            ],
            message="m",
            ref="r",
        ),
    ),
    (
        "a section and comment only",
        rule(
            "Note",
            expressions=[plugin("A.esp")],
            message="m",
            ref="r",
            section="Sec",
            comment="; already\n;;two",
        ),
    ),
    (
        "a priority-only block",
        rule("Note", expressions=[plugin("A.esp")], ref="r", priority=2, force_block=True),
    ),
]


def _fuzz_expr(r: Rng, depth: int = 0) -> dict[str, Any]:
    roll = r.below(12)
    name = r.pick([*NAMES, *r.some(BAD_NAMES, 1, 6)]) if r.below(4) else r.pick(NAMES)
    if roll < 4 or depth > 2:
        return plugin(name)
    if roll == 4:
        return ver(
            r.pick(["<", "=", ">", "~"]),
            r.pick(["1.2", "1_3a", "x", "", " 2 ", "77g", "1.2.3.4"]),
            name,
        )
    if roll == 5:
        return size(r.pick([0, 1, 4096, -1]), name, bool(r.below(2)))
    if roll == 6:
        return desc(
            r.pick(["abc", "[Tt]ribunal", "(abc", "x{2}", "", ".*", "a|b"]), name, bool(r.below(2))
        )
    parts = [_fuzz_expr(r, depth + 1) for _ in range(r.below(4))]
    return group(r.pick(["ALL", "ANY", "NOT", "ALL", "ANY", "XOR"]), *parts)


def fuzz_rule(seed: int) -> dict[str, Any]:
    """A made-up rule."""
    r = Rng(seed)
    kind = r.pick(
        [
            "Order",
            "NearStart",
            "NearEnd",
            "Note",
            "Requires",
            "Conflict",
            "Patch",
            "Note",
            "Requires",
            "Bogus",
        ]
    )
    names = [r.pick([*NAMES, *r.some(BAD_NAMES, 1, 8)]) for _ in range(r.below(5))]
    return rule(
        kind,
        plugins=names,
        expressions=[_fuzz_expr(r) for _ in range(r.below(4))],
        message=r.pick(["", "m", LONG, "a ] b", "l1\nl2", "  \n  ", "needs it"]),
        ref=r.pick(["", "readme", "https://x.y/z", "  see  www.a.b  "]),
        priority=r.pick([0, 0, 1, 2, 3, 4, -1]),
        section=r.pick(["", "", "Sec", "@Sec", "@@x "]),
        comment=r.pick(["", "", "note", "a\n\nb"]),
        force_block=bool(r.below(5) == 0),
    )


REFS: list[str] = [
    "",
    "   ",
    "readme",
    "  the   readme  ",
    "https://example.com/x",
    "see https://example.com/x for more",
    "www.example.com",
    "HTTP://UPPER.CASE/x",
    "no http here, just www in words",
    "x\u2028y\u00a0z",
    "tab\tand\nnewline",
    "ftp://not.counted",
    "httpx://nope",
]


def rule_case(name: str, r: dict[str, Any]) -> dict[str, Any]:
    """Render and validate one rule."""
    from wraithguard.rules.authoring import render_rule, validate

    py = rule_from_json(r)
    return {
        "name": name,
        "rule": r,
        "rendered": render_rule(py),
        "problems": [[p.severity, p.message, p.remedy] for p in validate(py)],
    }


# ---------------------------------------------------------------- proposals

MASTER_CASES: list[tuple[str, list[list[Any]], list[str], list[Any], int]] = [
    ("nothing", [], [], [], 1),
    (
        "a mod over a mod",
        [["B.esp", ["A.esp"]], ["C.esp", ["Morrowind.esm", "B.esp"]]],
        ["Morrowind.esm", "A.esp", "B.esp", "C.esp"],
        [],
        1,
    ),
    ("a master that is not active", [["B.esp", ["A.esp", "Gone.esp"]]], ["A.esp", "B.esp"], [], 1),
    ("a plugin that lists itself", [["B.esp", ["b.ESP", "A.esp"]]], ["A.esp", "B.esp"], [], 1),
    (
        "the base game only",
        [["B.esp", ["Morrowind.esm", "Tribunal.esm", "BLOODMOON.ESM"]]],
        ["B.esp"],
        [],
        1,
    ),
    (
        "a patch",
        [
            ["A and B Compat Patch.esp", ["A.esp", "B.esp"]],
            ["Plain.esp", ["A.esp", "B.esp"]],
            ["x-fix.esp", ["A.esp"]],
        ],
        ["A.esp", "B.esp"],
        [],
        1,
    ),
    (
        "patch words",
        [
            ["glue_it.esp", ["A.esp", "B.esp"]],
            ["Fixes.esp", ["A.esp", "B.esp"]],
            ["Compatibility.esp", ["A.esp", "B.esp"]],
        ],
        ["A.esp", "B.esp"],
        [],
        1,
    ),
    (
        "conflicts, the busiest first",
        [],
        [],
        [
            {"plugins": ["A.esp", "B.esp"], "type": "NPC_"},
            {"plugins": ["A.esp", "C.esp"], "type": "CELL"},
            {"plugins": ["A.esp", "C.esp"], "type": "CELL"},
            {"plugins": ["A.esp", "C.esp"], "type": "NPC_"},
        ],
        1,
    ),
    (
        "three plugins on one record",
        [],
        [],
        [{"plugins": ["A.esp", "B.esp", "C.esp"], "type": "STAT"}],
        1,
    ),
    (
        "a threshold",
        [],
        [],
        [
            {"plugins": ["A.esp", "B.esp"], "type": "X"},
            {"plugins": ["A.esp", "C.esp"], "type": "X"},
            {"plugins": ["A.esp", "C.esp"], "type": "X"},
        ],
        2,
    ),
    (
        "records that are not lists, and types that are not names",
        [],
        [],
        [
            {"plugins": "A.esp", "type": "X"},
            {"type": "X"},
            {"plugins": None, "type": "X"},
            {"plugins": 5, "type": "X"},
            {"plugins": ["A.esp", "B.esp"]},
            {"plugins": ["A.esp", "B.esp"], "type": None},
            {"plugins": ["A.esp", "B.esp"], "type": ""},
            {"plugins": ["A.esp", "B.esp"], "type": 0},
            {"plugins": ["A.esp", "B.esp"], "type": 7},
            {"plugins": ["A.esp", 3], "type": "Y"},
        ],
        1,
    ),
    (
        "ties keep the order first seen",
        [],
        [],
        [
            {"plugins": ["A.esp", "B.esp"], "type": t}
            for t in ("NPC_", "CELL", "STAT", "CELL", "NPC_", "DIAL")
        ],
        1,
    ),
    (
        "everything together, facts first",
        [["B.esp", ["A.esp"]], ["B Patch.esp", ["A.esp", "B.esp"]]],
        ["A.esp", "B.esp", "B Patch.esp"],
        [{"plugins": ["A.esp", "B.esp"], "type": "NPC_"}],
        1,
    ),
]


def _fuzz_masters(seed: int) -> tuple[str, list[list[Any]], list[str], list[Any], int]:
    r = Rng(seed)
    pool = [
        "A.esp",
        "B.esp",
        "C.esp",
        "D Patch.esp",
        "E-Fix.esp",
        "Morrowind.esm",
        "Tribunal.esm",
        "Gone.esp",
        "f_compat.esp",
        "Caf\u00e9.esp",
    ]
    masters = [[p, r.some(pool, 1, 3)] for p in r.some(pool, 2, 3)]
    active = r.some(pool, 2, 3)
    conflicts: list[Any] = []
    for _ in range(r.below(8)):
        roll = r.below(10)
        if roll == 0:
            conflicts.append({"plugins": r.pick(pool), "type": "X"})
        elif roll == 1:
            conflicts.append({"type": "X"})
        else:
            record: dict[str, Any] = {"plugins": r.some(pool, 1, 3)[:3] or ["A.esp", "B.esp"]}
            kind = r.pick(["NPC_", "CELL", "STAT", None, "", 0, 4])
            if r.below(8):
                record["type"] = kind
            conflicts.append(record)
    return (f"made up, seed {seed}", masters, active, conflicts, 1 + r.below(2))


def proposal_to_json(p: Any) -> dict[str, Any]:
    """The JSON of a proposal."""
    return {
        "rule": rule_to_json(p.rule),
        "confidence": p.confidence,
        "reason": p.reason,
        "evidence": p.evidence,
        "alternatives": [rule_to_json(a) for a in p.alternatives],
    }


def proposal_case(
    name: str, masters: list[list[Any]], active: list[str], conflicts: list[Any], minimum: int
) -> dict[str, Any]:
    """Every derive function over one observation."""
    from wraithguard.rules import derive

    by_plugin = {plugin_name: list(ms) for plugin_name, ms in masters}
    got = {
        "order": [proposal_to_json(p) for p in derive.order_from_masters(by_plugin, active)],
        "requires": [proposal_to_json(p) for p in derive.requires_from_masters(by_plugin, active)],
        "patch": [proposal_to_json(p) for p in derive.patch_candidates(by_plugin, active)],
        "from_conflicts": [
            proposal_to_json(p) for p in derive.order_candidates_from_conflicts(conflicts, minimum)
        ],
    }
    everything = derive.propose_all(by_plugin, active, conflicts, minimum)
    return {
        "name": name,
        "masters": masters,
        "active": active,
        "conflicts": conflicts,
        "minimum": minimum,
        **got,
        "all": [proposal_to_json(p) for p in everything],
        "needs_citation": [derive.needs_citation(p) for p in everything],
    }


# ---------------------------------------------------------------- files

HEADER_ONLY = "; mine\n"
FILES: list[tuple[str, bytes | None, dict[str, Any]]] = [
    ("a new file", None, {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")}),
    (
        "a file ending in a blank line",
        b"; mine\n[Order]\nX.esp\nY.esp\n\n",
        {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")},
    ),
    (
        "a file ending in a newline",
        b"; mine\n",
        {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")},
    ),
    (
        "a file with no final newline",
        b"; mine",
        {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")},
    ),
    ("an empty file", b"", {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")}),
    (
        "a crlf file",
        b"[Order]\r\nX.esp\r\nY.esp\r\n",
        {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")},
    ),
    (
        "a file with a byte order mark",
        b"\xef\xbb\xbf; mine\n",
        {"rule": rule("Order", plugins=["A.esp", "B.esm"], ref="r")},
    ),
    (
        "a message rule",
        b"; mine\n",
        {
            "rule": rule(
                "Requires",
                expressions=[plugin("A.esp"), plugin("B.esm")],
                message="needs B",
                ref="readme",
            )
        },
    ),
    (
        "a rule with errors is refused",
        b"; mine\n",
        {"rule": rule("Order", plugins=["A.esp"], ref="r")},
    ),
    (
        "a rule with several errors",
        b"; mine\n",
        {"rule": rule("Order", plugins=["bad;name.esp", "a.esp", "A.ESP"], priority=9)},
    ),
    ("warnings do not refuse", b"; mine\n", {"rule": rule("NearEnd", plugins=["A.esp"])}),
    (
        "a non-ASCII name",
        b"; mine\n",
        {"rule": rule("Order", plugins=["Caf\u00e9.esp", "B.esm"], ref="r")},
    ),
    (
        "a user rule",
        None,
        {"user": {"keyword": "Order", "names": [" A.esp ", "", "B.esm"], "comment": " why "}},
    ),
    (
        "a user rule in another case",
        b"; mine\n",
        {"user": {"keyword": " NEARSTART ", "names": ["A.esp"], "comment": None}},
    ),
    (
        "a user rule of an unknown kind",
        b"; mine\n",
        {"user": {"keyword": "Requires", "names": ["A.esp"], "comment": None}},
    ),
    (
        "a user rule of no names",
        b"; mine\n",
        {"user": {"keyword": "order", "names": ["", "  "], "comment": None}},
    ),
    (
        "a user rule of one name",
        b"; mine\n",
        {"user": {"keyword": "order", "names": ["A.esp"], "comment": None}},
    ),
    (
        "a user rule with a quote in the keyword",
        b"; mine\n",
        {"user": {"keyword": "it's", "names": ["A.esp"], "comment": None}},
    ),
]


def file_case(name: str, existing: bytes | None, what: dict[str, Any]) -> dict[str, Any]:
    """``append_authored_rule`` / ``append_user_rule`` over a temp folder."""
    import wraithguard_toolkit as tk

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "my_rules.txt"
        if existing is not None:
            path.write_bytes(existing)
        try:
            if "rule" in what:
                text = tk.append_authored_rule(path, rule_from_json(what["rule"]))
            else:
                u = what["user"]
                text = tk.append_user_rule(path, u["keyword"], u["names"], u["comment"])
            outcome: dict[str, Any] = {"text": text}
        except ValueError as exc:
            outcome = {"refused": str(exc)}
        after = path.read_bytes().decode("utf-8").replace("\r\n", "\n") if path.exists() else None
    return {
        "name": name,
        "existing": None if existing is None else existing.decode("utf-8"),
        **what,
        **outcome,
        "after": after,
    }


FROZEN: list[tuple[list[str], list[str], list[str]]] = [
    ([], [], []),
    (["B.esp", "A.esp"], ["A.esp", "B.esp"], ["a.esp", "b.esp"]),
    (["A.esp", "B.esp"], ["A.esp", "B.esp"], ["a.esp", "b.esp"]),
    (["B.esp", "A.esp", "C*.esp"], ["A.esp", "B.esp", "Cx.esp"], ["A.esp", "B.esp", "Cx.esp"]),
    (["B.esp", "A.esp"], ["A.esp", "B.esp"], ["a.esp"]),
    (["B.esp", "A.esp"], ["A.esp"], ["a.esp", "b.esp"]),
    (["c.esp", "B.ESP", "a.esp"], ["A.esp", "B.esp", "C.esp"], ["A.esp", "B.esp", "C.esp"]),
    (["B.esp", "A.esp", "B.esp"], ["A.esp", "B.esp"], ["a.esp", "b.esp"]),
    (["A <VER>.esp", "B.esp"], ["B.esp", "A 1.esp"], ["b.esp"]),
    (["A?.esp", "B.esp"], ["B.esp", "Ax.esp"], ["B.esp"]),
]


def cases() -> dict[str, Any]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    from wraithguard.rules.authoring import format_ref
    from wraithguard_toolkit import order_rule_frozen_conflicts

    rules = [rule_case(n, r) for n, r in HAND_RULES]
    rules += [rule_case(f"made up, seed {s}", fuzz_rule(s)) for s in range(1, 301)]
    proposals = [proposal_case(*c) for c in MASTER_CASES]
    proposals += [proposal_case(*_fuzz_masters(s)) for s in range(1, 81)]
    return {
        "refs": [{"ref": r, "formatted": format_ref(r)} for r in REFS],
        "rules": rules,
        "proposals": proposals,
        "files": [file_case(*f) for f in FILES],
        "frozen": [
            {
                "names": n,
                "final_order": f,
                "curated": c,
                "pairs": [list(p) for p in order_rule_frozen_conflicts(n, f, c)],
            }
            for n, f, c in FROZEN
        ],
    }


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "The rule maker: how a rule renders and validates, the proposals derived from what the "
            "tool observed, and the user's rule file, answered by wraithguard.rules.authoring, "
            "wraithguard.rules.derive and wraithguard_toolkit; run by tests/test_authoring_cases.py and "
            "viewer-shell/ordercore/tests/authoring_cases.rs. Regenerate: python tools/gen_authoring_cases.py"
        ),
        **out,
    }
    OUT.write_bytes(
        (json.dumps(doc, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
    )
    print(
        f"{len(out['refs'])} refs, {len(out['rules'])} rules, {len(out['proposals'])} proposals, "
        f"{len(out['files'])} files, {len(out['frozen'])} frozen -> {OUT.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
