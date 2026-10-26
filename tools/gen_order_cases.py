r"""Regenerate ``tests/fixtures/order_cases.json`` from the Python sort.

The inputs are written here (and a few dozen more made from a fixed seed); the answers are what
``wraithguard.sort.engine.build_and_sort`` gives: the whole load order, and why each custom sits
where it does (``anchor_out``). ``ordercore::order`` (``viewer-shell/ordercore``) runs the same
file, so the two cannot drift.

A case is rule file texts (a file's index is its priority, as ``load_rule_blocks`` makes it: its
``[Order]`` blocks are chains, its ``[NearStart]``/``[NearEnd]`` names are the hint lists, in file
order), the curated order, the customs and the header masters. Rule texts are read with
``parse_mlox_text`` (``ordercore::rules`` on the Rust side), so 2a to 2d run together.

A case marked ``"supported": false`` is one the Rust side declines (``Unsupported``: a rule or hint
name with a wildcard and a character mlox leaves to the regular expression; see
``gen_pattern_cases.py``).

Everything here must be a case the Rust port can run: no curated order holding two spellings of
one name, and no two nodes that differ only by case (Python then orders their neighbours by set
order, which differs from process to process).

Run: ``python tools/gen_order_cases.py``.
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "order_cases.json"

#: Characters mlox leaves to the regular expression; with a wildcard they make a name the Rust
#: side declines.
# What ordercore declined in a pattern with a wildcard: nothing, since every character
# but the wildcards is plain text (wraithguard/rules/patterns.py).
PASSED_THROUGH = ""

Case = tuple[str, list[tuple[int, str]], list[str], list[str], dict[str, list[str]]]


def _o(*names: str) -> str:
    return "[Order]\n" + "\n".join(names) + "\n"


def _ns(*names: str) -> str:
    return "[NearStart]\n" + "\n".join(names) + "\n"


def _ne(*names: str) -> str:
    return "[NearEnd]\n" + "\n".join(names) + "\n"


#: ``(name, [(priority, rule text)], base, subset, masters)``.
HAND: list[Case] = [
    ("nothing at all", [], [], [], {}),
    (
        "a curated order and no customs comes back as it was",
        [],
        ["A.esm", "C.esp", "B.esp"],
        [],
        {},
    ),
    ("a new custom with no signal goes to the end", [], ["A.esp", "B.esp"], ["X.esp"], {}),
    (
        "new customs with no signal keep the order they were declared in",
        [],
        ["A.esp", "B.esp"],
        ["Z.esp", "X.esp", "Y.esp"],
        {},
    ),
    (
        "a custom already in the cfg keeps its place",
        [],
        ["A.esp", "X.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "customs already in the cfg keep their places",
        [],
        ["X.esp", "A.esp", "Y.esp", "B.esp", "Z.esp"],
        ["Z.esp", "X.esp", "Y.esp"],
        {},
    ),
    (
        "a custom master with no rule floats into the master block",
        [],
        ["Morrowind.esm", "A.esp"],
        ["Mine.esm"],
        {},
    ),
    (
        "a custom master and a custom plugin, both new",
        [],
        ["Morrowind.esm", "Tribunal.esm", "A.esp"],
        ["Late.esp", "Mine.esm"],
        {},
    ),
    (
        "right after a curated plugin by a rule",
        [(0, _o("B.esp", "X.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "right before a curated plugin by a rule",
        [(0, _o("X.esp", "C.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "after the later of two predecessors",
        [(0, _o("A.esp", "X.esp")), (0, _o("C.esp", "X.esp"))],
        ["A.esp", "B.esp", "C.esp", "D.esp"],
        ["X.esp"],
        {},
    ),
    (
        "an after signal wins over a before signal",
        [(0, _o("A.esp", "X.esp", "D.esp"))],
        ["A.esp", "B.esp", "C.esp", "D.esp"],
        ["X.esp"],
        {},
    ),
    (
        "the earliest of two successors",
        [(0, _o("X.esp", "D.esp")), (0, _o("X.esp", "C.esp"))],
        ["A.esp", "B.esp", "C.esp", "D.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a header master that is a master file is no position signal",
        [],
        ["Morrowind.esm", "A.esp", "B.esp"],
        ["X.esp"],
        {"x.esp": ["Morrowind.esm"]},
    ),
    (
        "a header master that is an ordinary plugin is",
        [],
        ["Morrowind.esm", "A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {"x.esp": ["A.esp"]},
    ),
    (
        "a custom after a custom after a curated plugin",
        [(0, _o("B.esp", "Y.esp", "X.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "three customs chained on one curated plugin",
        [(0, _o("A.esp", "X.esp", "Y.esp", "Z.esp"))],
        ["A.esp", "B.esp"],
        ["Z.esp", "Y.esp", "X.esp"],
        {},
    ),
    (
        "customs ordered among themselves, new, no curated neighbour",
        [(0, _o("X.esp", "Y.esp"))],
        ["A.esp", "B.esp"],
        ["Y.esp", "X.esp"],
        {},
    ),
    (
        "a chain of new customs, no curated neighbour",
        [(0, _o("X.esp", "Y.esp", "Z.esp"))],
        ["A.esp"],
        ["Z.esp", "X.esp", "Y.esp"],
        {},
    ),
    (
        "a new custom before a custom that is after a curated plugin",
        [(0, _o("B.esp", "Y.esp")), (0, _o("X.esp", "Y.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "customs in the cfg moved by rules",
        [(0, _o("C.esp", "X.esp")), (0, _o("Y.esp", "A.esp"))],
        ["X.esp", "A.esp", "Y.esp", "B.esp", "C.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "two customs that order each other and a curated plugin each side",
        [(0, _o("A.esp", "X.esp", "Y.esp", "C.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["Y.esp", "X.esp"],
        {},
    ),
    (
        "a refused rule leaves the order alone",
        [(0, _o("C.esp", "X.esp")), (0, _o("X.esp", "A.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a later file's rule wins and moves the custom",
        [(0, _o("A.esp", "X.esp")), (1, _o("C.esp", "X.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    ("a NearStart custom", [(0, _ns("X.esp"))], ["A.esp", "B.esp"], ["X.esp"], {}),
    ("a NearEnd custom", [(0, _ne("X.esp"))], ["A.esp", "B.esp"], ["X.esp", "Y.esp"], {}),
    (
        "NearStart and NearEnd together",
        [(0, _ns("Y.esp") + _ne("X.esp"))],
        ["A.esp", "B.esp"],
        ["X.esp", "Y.esp", "Z.esp"],
        {},
    ),
    (
        "NearEnd keeps the order it is listed in",
        [(0, _ne("C.esp", "B.esp", "A.esp"))],
        ["Base.esp"],
        ["A.esp", "B.esp", "C.esp"],
        {},
    ),
    (
        "NearStart keeps the order it is listed in",
        [(0, _ns("C.esp", "B.esp", "A.esp"))],
        ["Base.esp"],
        ["A.esp", "B.esp", "C.esp"],
        {},
    ),
    (
        "the first block to name a plugin fixes its rank",
        [(0, _ne("A.esp", "B.esp") + _ne("C.esp", "A.esp"))],
        ["Base.esp"],
        ["A.esp", "B.esp", "C.esp"],
        {},
    ),
    (
        "a plugin named by NearStart and NearEnd is a NearStart",
        [(0, _ns("X.esp") + _ne("X.esp", "Y.esp"))],
        ["Base.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "hints are read file after file",
        [(0, _ne("B.esp")), (1, _ne("A.esp"))],
        ["Base.esp"],
        ["A.esp", "B.esp"],
        {},
    ),
    (
        "a hint on a curated plugin changes nothing",
        [(0, _ns("B.esp") + _ne("A.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a hint on a plugin that is not installed changes nothing",
        [(0, _ns("Gone.esp") + _ne("Gone2.esp"))],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
    (
        "hints by wildcard",
        [(0, _ne("Z*.esp") + _ns("Q?.esp"))],
        ["A.esp"],
        ["Z1.esp", "Q.esp", "Qa.esp", "Z2.esp", "M.esp"],
        {},
    ),
    (
        "hints in another case",
        [(0, _ns("x.ESP") + _ne("y.esp"))],
        ["A.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "a hint overrides an anchor",
        [(0, _o("A.esp", "X.esp") + _ne("X.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a hint on a custom already in the cfg",
        [(0, _ns("X.esp"))],
        ["A.esp", "B.esp", "X.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "an edge wins over a NearStart hint",
        [(0, _ns("X.esp"))],
        ["Morrowind.esm", "A.esp", "B.esp"],
        ["X.esp"],
        {"x.esp": ["B.esp"]},
    ),
    (
        "an edge wins over a NearEnd hint",
        [(0, _ne("X.esp") + _o("X.esp", "B.esp"))],
        ["A.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a custom master with a NearEnd hint is still a master",
        [(0, _ne("Mine.esm"))],
        ["Morrowind.esm", "A.esp"],
        ["Mine.esm", "X.esp"],
        {},
    ),
    (
        "customs both ends: hints and anchors",
        [(0, _o("B.esp", "X.esp") + _ns("Y.esp") + _ne("Z.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp", "Y.esp", "Z.esp", "W.esp"],
        {},
    ),
    (
        "masters before everything even when positioned late",
        [(0, _o("C.esp", "Mine.esm"))],
        ["Morrowind.esm", "A.esp", "B.esp", "C.esp"],
        ["Mine.esm"],
        {},
    ),
    (
        "a custom master after a custom master",
        [],
        ["Morrowind.esm", "A.esp"],
        ["Y.esm", "X.esm"],
        {"y.esm": ["X.esm"]},
    ),
    (
        "a custom after a custom master is anchored by nothing",
        [(0, _o("Mine.esm", "X.esp"))],
        ["Morrowind.esm", "A.esp"],
        ["Mine.esm", "X.esp"],
        {},
    ),
    (
        "a custom already in the cfg with a before signal only",
        [(0, _o("X.esp", "B.esp"))],
        ["A.esp", "B.esp", "C.esp", "X.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a custom that must come after a custom that is itself unanchored",
        [(0, _o("X.esp", "Y.esp"))],
        ["A.esp", "B.esp"],
        ["X.esp", "Y.esp", "Z.esp"],
        {},
    ),
    (
        "two customs each after the other's curated plugin and each other",
        [(0, _o("A.esp", "X.esp")), (0, _o("B.esp", "Y.esp")), (0, _o("X.esp", "Y.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["Y.esp", "X.esp"],
        {},
    ),
    (
        "a custom after a custom, both in the cfg",
        [(0, _o("X.esp", "Y.esp"))],
        ["Y.esp", "A.esp", "X.esp", "B.esp"],
        ["Y.esp", "X.esp"],
        {},
    ),
    (
        "wildcards that reach customs and curated plugins",
        [(0, _o("A*.esp", "Mod*.esp", "Z.esp"))],
        ["A1.esp", "A2.esp", "Mod1.esp", "B.esp"],
        ["ModNew.esp", "Z.esp"],
        {},
    ),
    (
        "a version pattern",
        [(0, _o("Better Bodies <VER>.esp", "Patch.esp"))],
        ["Better Bodies 2.2.esp", "Other.esp"],
        ["Patch.esp"],
        {},
    ),
    (
        "customs named in another case",
        [(0, _o("b.ESP", "x.esp"))],
        ["A.esp", "B.esp", "C.esp"],
        ["X.ESP", "x.esp"],
        {},
    ),
    (
        "a custom in the cfg under another spelling",
        [(0, _o("B.esp", "mod.esp"))],
        ["A.esp", "Mod.esp", "B.esp"],
        ["MOD.ESP"],
        {},
    ),
    (
        "a long chain of new customs",
        [(0, _o(*[f"C{i:02d}.esp" for i in range(60)]))],
        ["A.esp", "B.esp"],
        [f"C{i:02d}.esp" for i in reversed(range(60))],
        {},
    ),
    (
        "a long chain of customs after a curated plugin",
        [(0, _o("A.esp", *[f"C{i:02d}.esp" for i in range(40)]))],
        ["A.esp", "B.esp", "D.esp"],
        [f"C{i:02d}.esp" for i in reversed(range(40))],
        {},
    ),
    (
        "a realistic fragment",
        [
            (
                0,
                "[Version 0.1]\n"
                + _o("Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm", "Tamriel_Data.esm")
                + _o("TR_Mainland.esm", "Better Bodies*.esp", "My Armor <VER>.esp")
                + "[Note]\nsee the [Order] section\n"
                + _ne("Merged.esp"),
            ),
            (1, _o("My Armor <VER>.esp", "Better Bodies*.esp") + _ns("Mine.esm")),
        ],
        [
            "Morrowind.esm",
            "Tribunal.esm",
            "Bloodmoon.esm",
            "Tamriel_Data.esm",
            "TR_Mainland.esm",
            "Better Bodies.esp",
            "Better Bodies 2.esp",
            "Weapons.esp",
            "Armor.esp",
        ],
        ["My Armor 1.2.esp", "Better Bodies Patch.esp", "Mine.esp", "Merged.esp", "Mine.esm"],
        {
            "my armor 1.2.esp": ["Morrowind.esm", "Tamriel_Data.esm", "Gone.esm"],
            "better bodies patch.esp": ["Better Bodies.esp"],
            "mine.esp": ["my armor 1.2.esp", "Weapons.esp"],
        },
    ),
    (
        "a wildcard hint with a character the expression would read is declined",
        [(0, _ns("Mod [x]*.esp"))],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a rule name that is declined declines the sort",
        [(0, _o("A.esp", "X*{2}.esp"))],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
]


class Rng:
    """A small generator of its own, so the file does not change with Python's ``random``."""

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

    def shuffled(self, items: list[str]) -> list[str]:
        """The items in another order."""
        out = list(items)
        for i in range(len(out) - 1, 0, -1):
            j = self.below(i + 1)
            out[i], out[j] = out[j], out[i]
        return out


def fuzz(seed: int, big: bool) -> Case:
    """A made-up setup: curated plugins, customs (some in the cfg), rules, masters and hints."""
    r = Rng(seed)
    nb = (14 + r.below(18)) if big else (3 + r.below(10))
    nc = (10 + r.below(12)) if big else (2 + r.below(6))
    nmasters = 1 + r.below(3)
    base = [f"B{i:02d}.{'esm' if i < nmasters else 'esp'}" for i in range(nb)]
    customs: list[str] = []
    for j in range(nc):
        ext = "esm" if r.chance(1, 7) else "esp"
        customs.append(f"C{j:02d}.{ext}")
    in_cfg = [c for c in customs if r.chance(1, 3)]
    for c in in_cfg:
        base.insert(nmasters + r.below(len(base) - nmasters + 1), c)
    # the customs as declared: shuffled, some spelt in another case (the cfg's spelling wins)
    shuffled = r.shuffled(customs)
    declared = [
        c.replace(".esp", ".ESP") if c in in_cfg and r.chance(1, 4) else c for c in shuffled
    ]

    pool = [*base, *customs, "Gone1.esp", "Gone2.esp", "Gone3.esm"]
    wild = ["B0*.esp", "C*.esp", "C0?.esp", "*.esm", "B1*.esp", "C1*.esp"]

    def token() -> str:
        roll = r.below(10)
        if roll == 0:
            return r.pick(wild)
        name = r.pick(pool)
        return name.lower() if roll == 1 else name

    rules: list[tuple[int, str]] = []
    for priority in range(1 + r.below(3)):
        text = ""
        for _ in range(1 + r.below(5 if big else 4)):
            kind = r.below(10)
            names = [token() for _ in range(2 + r.below(4))]
            if kind < 7:
                text += _o(*names)
            elif kind < 9:
                text += _ns(*names[:2])
            else:
                text += _ne(*names[:2])
        rules.append((priority, text))

    masters: dict[str, list[str]] = {}
    for c in customs:
        if r.chance(1, 2):
            listed = [r.pick(pool) for _ in range(1 + r.below(2))]
            masters[c.lower()] = listed
    return (f"made up, seed {seed}{', big' if big else ''}", rules, base, declared, masters)


def _blocks(
    rules: list[tuple[int, str]],
) -> tuple[list[tuple[list[str], int]], list[str], list[str]]:
    """``load_rule_blocks``'s three lists, from rule file texts."""
    from wraithguard.rules.parser import parse_mlox_text

    order: list[tuple[list[str], int]] = []
    nearstart: list[str] = []
    nearend: list[str] = []
    for priority, text in rules:
        blocks, _skipped = parse_mlox_text(text)
        for keyword, names in blocks:
            if keyword == "order":
                order.append((names, priority))
            elif keyword == "nearstart":
                nearstart.extend(names)
            else:
                nearend.extend(names)
    return order, nearstart, nearend


def _supported(
    blocks: list[tuple[list[str], int]], nearstart: list[str], nearend: list[str]
) -> bool:
    from wraithguard.rules import pattern_has_meta

    names = [n for chain, _priority in blocks for n in chain] + nearstart + nearend
    return not any(pattern_has_meta(n) and any(c in n for c in PASSED_THROUGH) for n in names)


def answer(
    rules: list[tuple[int, str]],
    base: list[str],
    subset: list[str],
    masters: dict[str, list[str]],
) -> dict[str, Any]:
    """What the Python sort gives for one case."""
    from wraithguard.sort.engine import build_and_sort

    order, nearstart, nearend = _blocks(rules)
    anchors: dict[str, tuple[str, str | None]] = {}
    with contextlib.redirect_stdout(io.StringIO()):  # the report of what was refused
        result = build_and_sort(
            list(base),
            list(subset),
            order,
            masters=masters,
            nearstart=nearstart,
            nearend=nearend,
            anchor_out=anchors,
        )
    return {
        "supported": _supported(order, nearstart, nearend),
        "order": result,
        "anchors": {k: [how, anchor] for k, (how, anchor) in sorted(anchors.items())},
    }


def cases() -> list[dict[str, Any]]:
    """The cases, answered by the Python bodies (not ordercore's, which they check)."""
    sys.path.insert(0, str(ROOT))
    from wraithguard import ordercore_bridge

    with ordercore_bridge.python_only():
        return _cases()


def _cases() -> list[dict[str, Any]]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    every: list[Case] = [*HAND]
    every += [fuzz(seed, big=False) for seed in range(1, 61)]
    every += [fuzz(seed, big=True) for seed in range(101, 113)]
    out = []
    for name, rules, base, subset, masters in every:
        out.append(
            {
                "name": name,
                "rules": [{"priority": p, "text": t} for p, t in rules],
                "base": base,
                "subset": subset,
                "masters": masters,
                **answer(rules, base, subset, masters),
            }
        )
    return out


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "The sort's order: where each custom plugin lands among the curated ones, answered "
            "by wraithguard.sort.engine.build_and_sort; run by tests/test_order_cases.py and "
            "viewer-shell/ordercore/tests/order_cases.rs. Regenerate: python "
            "tools/gen_order_cases.py"
        ),
        "cases": out,
    }
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(out)} cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
