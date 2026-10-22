r"""Regenerate ``tests/fixtures/graph_cases.json`` from the Python sort graph code.

The inputs are written here; the answers are what ``wraithguard.sort.graph``
(``would_create_cycle``, ``is_master_file``) and ``wraithguard.sort.engine.build_edges`` /
``_unique_conflicts`` give. ``ordercore::graph`` (``viewer-shell/ordercore``) runs the same
file, so the two cannot drift.

A graph case is rule file texts (a file's index is its priority, as ``load_rule_blocks`` makes
it), the curated order, the customs, and the header masters; its rule texts are read with
``parse_mlox_text`` (``ordercore::rules`` on the Rust side), so 2a, 2b and 2c are exercised
together. The answer is the customs as canonicalised, the edges (sorted), the in-degrees above
zero, and the rejected edges in the order they were refused.

A case marked ``"supported": false`` is one the Rust side declines (it answers ``Unsupported``
and the caller keeps the Python): one of its rule names holds a wildcard and a character mlox
leaves to the regular expression (see ``gen_pattern_cases.py``).

Everything here must be a case the Rust port can run: no header master named for a plugin that
the curated order holds in two spellings (Python then picks one by set order, which differs from
process to process; the Rust side takes the later).

Run: ``python tools/gen_graph_cases.py``.
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "graph_cases.json"

#: Characters mlox leaves to the regular expression; with a wildcard they make a name the Rust
#: side declines.
PASSED_THROUGH = "[]{}|\\^$"

MASTER_NAMES: list[str] = [
    "Morrowind.esm",
    "TRIBUNAL.ESM",
    "Bloodmoon.Esm",
    "Game.omwgame",
    "Game.OMWGAME",
    "Mod.esp",
    "Mod.ESP",
    "Mod.omwaddon",
    "Mod.omwscripts",
    "Mod.esl",
    "esm",
    ".esm",
    ".omwgame",
    "Mod.esm.bak",
    "Mod.esmx",
    "Mod.omwgame.txt",
    "",
    "Café.ESM",
    "Mod esm",
    "ESM.esp",
]

#: ``(name, adjacency, start, target)``: would ``start -> target`` close a cycle.
CYCLES: list[tuple[str, dict[str, list[str]], str, str]] = [
    ("an empty graph", {}, "A", "B"),
    ("the reverse of an edge", {"A": ["B"], "B": []}, "B", "A"),
    ("the same direction as an edge", {"A": ["B"], "B": []}, "A", "B"),
    ("the end of a chain back to its start", {"A": ["B"], "B": ["C"], "C": []}, "C", "A"),
    ("the start of a chain on to its end", {"A": ["B"], "B": ["C"], "C": []}, "A", "C"),
    ("two nodes with no path", {"A": ["B"], "C": ["D"], "B": [], "D": []}, "B", "C"),
    ("a node to itself", {"A": []}, "A", "A"),
    (
        "a diamond closed at the bottom",
        {"A": ["B", "C"], "B": ["D"], "C": ["D"], "D": []},
        "D",
        "A",
    ),
    ("across a diamond's two arms", {"A": ["B", "C"], "B": ["D"], "C": ["D"], "D": []}, "B", "C"),
    ("an unrelated cycle does not loop", {"X": ["Y"], "Y": ["X"], "A": []}, "A", "X"),
    ("a cycle on the way to the start", {"X": ["Y"], "Y": ["X", "S"], "S": []}, "S", "X"),
    ("a target the graph does not hold", {"A": ["B"]}, "A", "Nowhere"),
    ("a start the graph does not hold", {"A": ["B"], "B": []}, "Nowhere", "A"),
    ("names differing by case are different nodes", {"A": ["B"], "B": []}, "b", "A"),
    (
        "a long chain",
        {f"P{i}": [f"P{i + 1}"] for i in range(40)} | {"P40": []},
        "P40",
        "P0",
    ),
]

#: ``(name, [(priority, rule text)], base, subset, masters)``.
GRAPHS: list[tuple[str, list[tuple[int, str]], list[str], list[str], dict[str, list[str]]]] = [
    ("nothing at all", [], [], [], {}),
    ("a curated chain and no customs", [], ["A.esm", "B.esp", "C.esp"], [], {}),
    ("a single curated plugin", [], ["A.esm"], [], {}),
    ("customs that are new are not chained", [], ["A.esm", "B.esp"], ["X.esp", "Y.esp"], {}),
    (
        "a custom already in the cfg is bridged over",
        [],
        ["A.esp", "C.esp", "B.esp"],
        ["C.esp"],
        {},
    ),
    (
        "customs at both ends of the cfg are bridged over",
        [],
        ["X.esp", "A.esp", "B.esp", "Y.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "a custom spelt another way is put on the cfg's spelling",
        [],
        ["A.esp", "Mod.esp", "B.esp"],
        ["mod.ESP"],
        {},
    ),
    (
        "customs that differ only by case are one",
        [],
        ["A.esp"],
        ["New.esp", "new.ESP", "NEW.esp", "Other.esp"],
        {},
    ),
    (
        "a custom named twice, once as the cfg has it",
        [],
        ["A.esp", "Mod.esp"],
        ["mod.esp", "Mod.esp"],
        {},
    ),
    (
        "non-ASCII names fold by case",
        [],
        ["Morrowind.esm", "Café.esp", "B.esp"],
        ["CAFÉ.ESP", "Nové.esp"],
        {},
    ),
    (
        "a curated name twice in the cfg",
        [],
        ["A.esp", "B.esp", "A.esp"],
        [],
        {},
    ),
    (
        "one header master",
        [],
        ["Morrowind.esm", "A.esp"],
        ["X.esp"],
        {"x.esp": ["Morrowind.esm"]},
    ),
    (
        "several header masters, one not installed",
        [],
        ["Morrowind.esm", "Tribunal.esm", "A.esp"],
        ["X.esp"],
        {"x.esp": ["Morrowind.esm", "Tamriel_Data.esm", "Tribunal.esm"]},
    ),
    (
        "a header master spelt another way",
        [],
        ["Morrowind.esm", "A.esp"],
        ["X.esp"],
        {"x.esp": ["MORROWIND.ESM"]},
    ),
    (
        "a custom listing itself as its master",
        [],
        ["A.esp"],
        ["X.esp"],
        {"x.esp": ["X.esp", "x.ESP"]},
    ),
    (
        "a custom after another custom",
        [],
        ["A.esp"],
        ["X.esp", "Y.esp"],
        {"y.esp": ["X.esp"]},
    ),
    (
        "masters of a curated plugin are not read",
        [],
        ["A.esm", "B.esp"],
        ["X.esp"],
        {"b.esp": ["A.esm"], "x.esp": ["A.esm"]},
    ),
    (
        "a masters key that is not lower case is not found",
        [],
        ["A.esm"],
        ["X.esp"],
        {"X.esp": ["A.esm"]},
    ),
    (
        "two customs each the other's master: the second is refused",
        [],
        ["A.esm"],
        ["X.esp", "Y.esp"],
        {"x.esp": ["Y.esp"], "y.esp": ["X.esp"]},
    ),
    (
        "masters held for a plugin that is not a custom",
        [],
        ["A.esm"],
        [],
        {"zzz.esp": ["A.esm"]},
    ),
    (
        "rules among curated plugins add nothing",
        [(0, "[Order]\nA.esp\nB.esp\nC.esp\n")],
        ["A.esp", "B.esp", "C.esp"],
        [],
        {},
    ),
    (
        "a rule through a custom",
        [(0, "[Order]\nA.esp\nX.esp\nB.esp\n")],
        ["A.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "an uninstalled plugin in a chain is bridged",
        [(0, "[Order]\nA.esp\nGone.esp\nX.esp\n")],
        ["A.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "uninstalled plugins at the ends and in the middle",
        [(0, "[Order]\nGone1.esp\nA.esp\nGone2.esp\nGone3.esp\nX.esp\nGone4.esp\n")],
        ["A.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a chain with one name left installed has no edge",
        [(0, "[Order]\nGone.esp\nX.esp\nAlso Gone.esp\n")],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a star takes every match",
        [(0, "[Order]\nA*.esp\nX.esp\n")],
        ["A1.esp", "A2.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a star on both sides makes every pair",
        [(0, "[Order]\nA*.esp\nX*.esp\n")],
        ["A1.esp", "A2.esp", "X1.esp"],
        ["X2.esp", "X3.esp"],
        {},
    ),
    (
        "a version pattern",
        [(0, "[Order]\nBetter Bodies <VER>.esp\nX.esp\n")],
        ["Better Bodies 2.2.esp", "Better Bodies.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a question mark",
        [(0, "[Order]\nX?.esp\nB.esp\n")],
        ["X.esp", "Xa.esp", "Xab.esp", "B.esp"],
        ["Q.esp"],
        {},
    ),
    (
        "a star that reaches a custom and a curated plugin: only the custom's edges",
        [(0, "[Order]\nA.esp\nMod*.esp\n")],
        ["A.esp", "Mod2.esp"],
        ["ModNew.esp"],
        {},
    ),
    (
        "two tokens that are one plugin make no self edge",
        [(0, "[Order]\nX.esp\nx*.esp\nB.esp\n")],
        ["B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a plain name in another case",
        [(0, "[Order]\nA.ESP\nx.esp\n")],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a plain name finds the first of two spellings only",
        [(0, "[Order]\nmod.esp\nX.esp\n")],
        ["Mod.esp", "MOD.ESP"],
        ["X.esp"],
        {},
    ),
    (
        "a wildcard reaches a custom master as well",
        [(0, "[Order]\n*.esm\nX.esp\n")],
        ["Morrowind.esm", "Tribunal.esm", "A.esp"],
        ["Mine.esm", "X.esp"],
        {},
    ),
    (
        "a name a rule repeats",
        [(0, "[Order]\nA.esp\nX.esp\nA.esp\nY.esp\n")],
        ["A.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "rules that agree",
        [(0, "[Order]\nX.esp\nY.esp\n"), (1, "[Order]\nX.esp\nY.esp\nZ.esp\n")],
        ["A.esp"],
        ["X.esp", "Y.esp", "Z.esp"],
        {},
    ),
    (
        "a later file wins a contradiction",
        [(0, "[Order]\nX.esp\nY.esp\n"), (1, "[Order]\nY.esp\nX.esp\n")],
        ["A.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "an earlier file wins when it is the later listed",
        [(1, "[Order]\nX.esp\nY.esp\n"), (0, "[Order]\nY.esp\nX.esp\n")],
        ["A.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "one file's blocks at one priority run in file order",
        [(0, "[Order]\nX.esp\nY.esp\n[Order]\nY.esp\nX.esp\n")],
        ["A.esp"],
        ["X.esp", "Y.esp"],
        {},
    ),
    (
        "a rule against the curated order is refused",
        [(0, "[Order]\nC.esp\nX.esp\n[Order]\nX.esp\nA.esp\n")],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "the same refusal twice is told once",
        [(0, "[Order]\nC.esp\nX.esp\n[Order]\nX.esp\nA.esp\n[Order]\nX.esp\nA.esp\n")],
        ["A.esp", "B.esp", "C.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a rule against a header master edge is refused",
        [(0, "[Order]\nX.esp\nBase.esm\n")],
        ["Base.esm", "A.esp"],
        ["X.esp"],
        {"x.esp": ["Base.esm"]},
    ),
    (
        "a rule through a cycle of three",
        [(0, "[Order]\nX.esp\nY.esp\n[Order]\nY.esp\nZ.esp\n[Order]\nZ.esp\nX.esp\n")],
        ["A.esp"],
        ["X.esp", "Y.esp", "Z.esp"],
        {},
    ),
    (
        "a rule drawn from a conditional entry",
        [(0, "[Order]\nA.esp\n[DESC /x/ Gone.esp]\nsome words\nX.esp\n")],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
    (
        "other keywords make no edges",
        [(0, "[Conflict]\nA.esp\nX.esp\n[Note]\nX.esp\nB.esp\n[NearStart]\nX.esp\nA.esp\n")],
        ["A.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a custom already in the cfg, moved by a rule",
        [(0, "[Order]\nB.esp\nX.esp\n")],
        ["A.esp", "X.esp", "B.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a custom in the cfg under another spelling, in a rule",
        [(0, "[Order]\nB.esp\nx.esp\n")],
        ["A.esp", "Xylo.esp", "B.esp", "X.esp"],
        ["x.esp"],
        {},
    ),
    (
        "a realistic fragment",
        [
            (
                0,
                "[Version 0.1]\n[Order]\nMorrowind.esm\nTribunal.esm\nBloodmoon.esm\n"
                "Tamriel_Data.esm\nTR_Mainland.esm\n[Note]\nsee the [Order] section\n"
                "[Order]\nTR_Mainland.esm\nBetter Bodies*.esp\nMy Armor <VER>.esp\n",
            ),
            (1, "[Order]\nMy Armor <VER>.esp\nBetter Bodies*.esp\n[NearEnd]\nMerged.esp\n"),
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
        ],
        ["My Armor 1.2.esp", "Better Bodies Patch.esp", "Mine.esp"],
        {
            "my armor 1.2.esp": ["Morrowind.esm", "Tamriel_Data.esm", "Gone.esm"],
            "better bodies patch.esp": ["Better Bodies.esp"],
            "mine.esp": ["my armor 1.2.esp", "Weapons.esp"],
        },
    ),
    (
        "a name with a bracket and a star is declined",
        [(0, "[Order]\nA.esp\nMod [x]*.esp\nX.esp\n")],
        ["A.esp", "Mod [x] 2.esp"],
        ["X.esp"],
        {},
    ),
    (
        "a name with a bracket and no wildcard is plain",
        [(0, "[Order]\nA.esp\nMod [x].esp\nX.esp\n")],
        ["A.esp", "Mod [x].esp"],
        ["X.esp"],
        {},
    ),
    (
        "a declined name in a block that would add no edge",
        [(0, "[Order]\na|b*.esp\nQ.esp\n")],
        ["A.esp"],
        ["X.esp"],
        {},
    ),
]


def _blocks(rules: list[tuple[int, str]]) -> list[tuple[list[str], int]]:
    """The ``(names, priority)`` chains of rule files, as ``load_rule_blocks`` makes them."""
    from wraithguard.rules.parser import parse_mlox_text

    out: list[tuple[list[str], int]] = []
    for priority, text in rules:
        blocks, _skipped = parse_mlox_text(text)
        out.extend((names, priority) for keyword, names in blocks if keyword == "order")
    return out


def _supported(blocks: list[tuple[list[str], int]]) -> bool:
    from wraithguard.rules import pattern_has_meta

    return not any(
        pattern_has_meta(name) and any(c in name for c in PASSED_THROUGH)
        for names, _priority in blocks
        for name in names
    )


def answer(
    rules: list[tuple[int, str]],
    base: list[str],
    subset: list[str],
    masters: dict[str, list[str]],
) -> dict[str, Any]:
    """What the Python sort code builds for one graph case."""
    from wraithguard.sort.engine import _unique_conflicts, build_edges

    blocks = _blocks(rules)
    with contextlib.redirect_stdout(io.StringIO()):  # the report of what was refused
        subset_out, adj, indeg, conflicts = build_edges(base, subset, blocks, masters)
    return {
        "supported": _supported(blocks),
        "subset_out": subset_out,
        "edges": sorted([a, b] for a, targets in adj.items() for b in targets),
        "indegree": sorted([n, k] for n, k in indeg.items() if k),
        "conflicts": [list(c) for c in conflicts],
        "unique_conflicts": [list(c) for c in _unique_conflicts(conflicts)],
    }


def cases() -> dict[str, list[dict[str, Any]]]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    from wraithguard.sort.graph import is_master_file, would_create_cycle

    names = [{"name": n, "is_master": is_master_file(n)} for n in MASTER_NAMES]
    cycles = [
        {
            "name": name,
            "adjacency": adjacency,
            "start": start,
            "target": target,
            "closes_cycle": would_create_cycle(adjacency, start, target),
        }
        for name, adjacency, start, target in CYCLES
    ]
    graphs = []
    for name, rules, base, subset, masters in GRAPHS:
        graphs.append(
            {
                "name": name,
                "rules": [{"priority": p, "text": t} for p, t in rules],
                "base": base,
                "subset": subset,
                "masters": masters,
                **answer(rules, base, subset, masters),
            }
        )
    return {"masters": names, "cycles": cycles, "graphs": graphs}


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "The sort's graph: master files, the cycle check and the edges built from the "
            "curated order, header masters and rule texts, answered by wraithguard.sort."
            "graph and wraithguard.sort.engine.build_edges; run by tests/test_graph_cases.py "
            "and viewer-shell/ordercore/tests/graph_cases.rs. Regenerate: python "
            "tools/gen_graph_cases.py"
        ),
        **out,
    }
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"{len(out['masters'])} names, {len(out['cycles'])} cycle checks, "
        f"{len(out['graphs'])} graphs -> {OUT.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
