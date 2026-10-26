r"""Regenerate ``tests/fixtures/predicate_cases.json`` from the Python predicate code.

The inputs are written here (and a few hundred more made from a fixed seed); the answers are what
``wraithguard.rules.expressions`` and ``wraithguard.rules.predicates`` give. ``ordercore::predicates``
(``viewer-shell/ordercore``) runs the same file, so the two cannot drift.

Three families:

* ``expressions``: a predicate body, its tokens, its tree and each top-level node described;
* ``versions``: ``format_version`` of a string;
* ``checks``: rule text and a load order through ``check_predicates``, with plugin files on disk.
  The files are real: tiny TES3 plugins of a given size and header description are written to a
  temporary folder and the Python reads them. What it read (``usable``, and each plugin's size and
  description) is recorded with the case, and the Rust side is given exactly that.

A check the Rust side may decline (``Unsupported``: a plugin name or ``[DESC]`` regex it does not
read) is not wrong; ``must_support`` marks the checks it has to read.

Run: ``python tools/gen_predicate_cases.py``.
"""

from __future__ import annotations

import json
import struct
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "predicate_cases.json"

POOL: list[str] = [
    "Alpha.esp",
    "Beta.esp",
    "Gamma.esm",
    "Delta Mod.esp",
    "Eps 1.2.esp",
    "Zeta v2.esp",
    "Eta.omwaddon",
    "Kappa.esp",
    "Lambda 3.0b.esp",
    "Mu.esp",
    "Café.esp",
]
PATTERNS: list[str] = ["Theta*.esp", "Iota?.esp", "Eps <VER>.esp", "*.esm", "Al*.esp", "Theta1.esp"]
DESCRIPTIONS: list[str] = [
    "",
    "Version 1.2",
    "v2.0b",
    "ver 3",
    "A mod by someone",
    "Tribunal patch version 1.0",
    "Café mod \u2013 v1.5",
    "v 1.0.2.3",
    "Version: abc",
    "mod \u2018quoted\u2019 version 4_5",
]
REGEXES: list[str] = [
    "Version",
    "v2",
    "mod",
    "[Tt]ribunal",
    "(abc",
    ".*Caf.",
    "x{2}",
    "^A",
    "b$",
    "(?i)mod",
    r"\d\.\d",
    "version|ver",
    "[",
]
#: What the Rust side declines in a hand-written check: a name with syntax in it, a regex it does
#: not read, a size too big for 64 bits.
DECLINED: tuple[str, ...] = ("A|B", "(?i)", "99999999999999999999999")
MESSAGES: list[str] = ["", "needs the base", "do not mix these", "see the readme", "Caf\u00e9 note"]

EXPRESSIONS: list[str] = [
    "",
    "A.esp",
    "A.esp B.esp",
    "[ALL A.esp B.esp]",
    "[ANY A.esp B.esp]",
    "[NOT A.esp]",
    "[ALL A.esp [NOT B.esp] C.esm]",
    "[ALL\n  A.esp\n  [NOT B.esp]\n  C.esm]",
    "[ANY [ALL A.esp B.esp] [ALL C.esp D.esp]]",
    "[VER = 1.0 A.esp]",
    "[VER < 2.0b Mod.esp]",
    "[ver>1_2-3 Mod.esp]",
    "[VER =1.0A.esp]",
    "[VER = 1.0]",
    "[VER = x Mod.esp]",
    "[SIZE 1234 Mod.esp]",
    "[SIZE !1234 Mod With Spaces.esm]",
    "[SIZE 12 Mod.esps]",
    "[DESC /[Tt]ribunal/ Foo.esp]",
    "[DESC !/abc/ Foo.esp]",
    "[DESC / Foo.esp]",
    "[MWSE-LUA /x/ Foo.esp]",
    " /msg/ A.esp",
    "Foo ALL A.esp",
    "A.esp ] B.esp",
    "[ A.esp",
    "[[A.esp]]",
    "[]",
    "[] A.esp []",
    "[A.esp B.esp] [C.esp]",
    "Theta*.esp Iota?.esp Eps <VER>.esp",
    "x.esp.esp A.espx B.omwaddon C.omwscripts D.omwgame",
    "Café.esp [ALL Naïve.esm]",
    "[all a.esp] [any b.esp] [not c.esp]",
    "[DESC A.esp]",
    "[ANY NOT A.esp]",
    # names holding a bracketed part, as mlox reads them
    "[ANY Morrowind Rebirth [Main].ESP DN-GDRv<VER>-Rebirth.esp]",
    "[Official]Master Index.esp [ANY [Official]Siege at Firemoth.esp B.esp]",
    "Dragonbone Armor [v.1.2].esp Vivec - Area of Effect (Remiros) [Replacer].esp",
    "[NOT [Official]Entertainers.esp]",
    "A [B.esp] C.esp",
    "A [ALL B] C.esp",
    "A [ VER x] C.esp",
    "A [x C.esp",
    "[ALL A.esp [Main] B.esp]",
    "All Boats.esp [ANY Not Here.esp]",
]

VERSIONS: list[str] = [
    "",
    "1",
    "1.0",
    "1.0a",
    "1.2.3",
    "1.2.3.4",
    "1.2.3.4.5",
    "01.002.0003",
    "2_1",
    "3-4-5",
    "3.4b",
    "3.4B",
    "10.20.30.40a",
    "99999.1",
    "123456.7",
    "1..2",
    ".1",
    "1.",
    "a",
    "1.2.3a",
    "1.2.3.4a",
    "007",
    "0",
    "1x2",
    "1.2x",
    "\u0661.\u0662",
    "1.\u0662",
    " 1.2",
    "+1.2",
    "1.2\u017f",
]


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


def tes3(description: str, size: int) -> bytes:
    """A minimal TES3 plugin: the header record, padded to ``size`` bytes."""
    raw = description.encode("cp1252", "replace")[:255]
    hedr = struct.pack("<fI", 1.3, 0) + b"author".ljust(32, b"\0") + raw.ljust(256, b"\0")
    hedr += struct.pack("<I", 0)
    sub = b"HEDR" + struct.pack("<I", len(hedr)) + hedr
    record = b"TES3" + struct.pack("<I", len(sub)) + b"\0" * 8 + sub
    return record.ljust(max(size, len(record)), b"\0")


# ---- rule texts, written by hand

#: ``(name, rules text, load order, origins, disk)``; ``disk`` is ``None`` (no data folders),
#: or ``{plugin: (size, description)}`` (the folder, possibly empty).
Check = tuple[str, str, list[str], dict[str, str], dict[str, tuple[int, str]] | None]

NODISK: dict[str, tuple[int, str]] = {}

HAND: list[Check] = [
    ("nothing", "", [], {}, None),
    (
        "a note fires on any of what it lists, and is about those present",
        "[Note modder's resources]\nA.esp\nB.esp\nC.esp\n",
        ["B.esp", "Other.esp"],
        {},
        None,
    ),
    (
        "a note none of whose plugins is present",
        "[Note x]\nA.esp\nB.esp\n",
        ["Other.esp"],
        {},
        None,
    ),
    (
        "a conflict between names holding brackets",
        "[Conflict one of these]\n[Official]Master Index.esp\nMorrowind Rebirth [Main].ESP\nMaster Index.esp\n",
        ["[Official]Master Index.esp", "Morrowind Rebirth [Main].ESP"],
        {},
        None,
    ),
    (
        "a requires with its dependency missing",
        "[Requires needs the base]\nPatch.esp\nBase.esp\n",
        ["Patch.esp"],
        {},
        None,
    ),
    (
        "a requires with its dependency present",
        "[Requires needs the base]\nPatch.esp\nBase.esp\n",
        ["Patch.esp", "Base.esp"],
        {},
        None,
    ),
    (
        "a requires whose target is absent",
        "[Requires x]\nPatch.esp\nBase.esp\n",
        ["Other.esp"],
        {},
        None,
    ),
    (
        "a requires missing two, one a group",
        "[Requires both]\nPatch.esp\nA.esp\n[ANY B.esp C.esp]\n",
        ["Patch.esp"],
        {},
        None,
    ),
    (
        "a conflict with both present",
        "[Conflict do not mix]\nA.esp\nB.esp\n",
        ["A.esp", "B.esp"],
        {},
        None,
    ),
    ("a conflict with one present", "[Conflict do not mix]\nA.esp\nB.esp\n", ["A.esp"], {}, None),
    (
        "a conflict of groups",
        "[Conflict groups]\n[ALL A.esp B.esp]\n[ALL C.esp D.esp]\n",
        ["A.esp", "B.esp", "C.esp", "D.esp"],
        {},
        None,
    ),
    (
        "a conflict names the plugins with their origins",
        "[Conflict mix]\nA.esp\nB*.esp\n",
        ["A.esp", "B1.esp", "B2.esp"],
        {"a.esp": "customizations.toml -> 'total'", "b2.esp": "subset file (x.txt)"},
        None,
    ),
    ("a note that holds", "[Note about it]\nA.esp\nB.esp\n", ["A.esp", "B.esp"], {}, None),
    ("a note that does not", "[Note about it]\nA.esp\nB.esp\n", ["A.esp"], {}, None),
    ("a note with a negation", "[Note n]\n[ALL A.esp [NOT B.esp]]\n", ["A.esp"], {}, None),
    (
        "a patch present, its original missing",
        "[Patch p]\nPatch.esp\nOrig.esp\n",
        ["Patch.esp"],
        {},
        None,
    ),
    (
        "a patch missing, its original present",
        "[Patch p]\nPatch.esp\nOrig.esp\n",
        ["Orig.esp"],
        {},
        None,
    ),
    ("a patch with both", "[Patch p]\nPatch.esp\nOrig.esp\n", ["Patch.esp", "Orig.esp"], {}, None),
    (
        "a patch over several originals",
        "[Patch p]\nPatch.esp\nO1.esp\nO2.esp\n",
        ["O1.esp", "O2.esp"],
        {},
        None,
    ),
    (
        "a message of indented lines and a comment",
        "[Requires]\n  first line\n  second line ; not this\nPatch.esp ; a comment\nBase.esp\n",
        ["Patch.esp"],
        {},
        None,
    ),
    (
        "an indented line inside an open bracket is logic",
        "[Note n]\n[ALL A.esp\n\t[NOT B.esp]\n\tC.esm]\n",
        ["A.esp", "C.esm"],
        {},
        None,
    ),
    (
        "keywords in any case, and ones that are not predicates",
        "[note first]\nA.esp\n[REQUIRES second]\nA.esp\nB.esp\n[Order]\nA.esp\nB.esp\n[Version 1.0]\n",
        ["A.esp"],
        {},
        None,
    ),
    (
        "a header that is not at a line start is prose",
        "[Note real]\nA.esp\n  see the [Requires x] section\n",
        ["A.esp"],
        {},
        None,
    ),
    (
        "blocks one after another",
        "[Note one]\nA.esp\n[Note two]\nB.esp\n[Note three]\nC.esp\n",
        ["A.esp", "B.esp"],
        {},
        None,
    ),
    ("a wildcard", "[Note w]\nTheta*.esp\n", ["Theta1.esp", "Theta22.esp", "Other.esp"], {}, None),
    ("a question mark", "[Note q]\nIota?.esp\n", ["Iota.esp", "Iotaa.esp", "Iotaab.esp"], {}, None),
    (
        "a version pattern",
        "[Note v]\nBetter <VER>.esp\n",
        ["Better 2.2.esp", "Better.esp"],
        {},
        None,
    ),
    ("names in another case", "[Note c]\nALPHA.ESP\n", ["alpha.esp"], {}, None),
    (
        "a name with a character the expression reads as syntax",
        "[Note s]\nA|B.esp\n",
        ["A|B.esp"],
        {},
        None,
    ),
    # --- [VER]
    (
        "a version read from the header",
        "[Note v]\n[VER = 1.2 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "Version 1.2")},
    ),
    (
        "a version in the file name when the header has none",
        "[Note v]\n[VER = 1.2 Mod 1.2.esp]\n",
        ["Mod 1.2.esp"],
        {},
        {"Mod 1.2.esp": (500, "no version here")},
    ),
    (
        "a version less than",
        "[Note v]\n[VER < 2.0 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "v1.5")},
    ),
    (
        "a version greater than, not met",
        "[Note v]\n[VER > 2.0 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "v1.5")},
    ),
    (
        "a version that cannot be known: equals is assumed",
        "[Note v]\n[VER = 9.9 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "nothing")},
    ),
    (
        "a version that cannot be known: less than is not",
        "[Note v]\n[VER < 9.9 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "nothing")},
    ),
    (
        "a version with no data folders",
        "[Note v]\n[VER = 1.0 Mod 1.0.esp]\n",
        ["Mod 1.0.esp"],
        {},
        None,
    ),
    (
        "a version of a plugin that is not active",
        "[Note v]\n[VER = 1.0 Gone.esp]\n",
        ["Mod.esp"],
        {},
        None,
    ),
    (
        "a version with a letter",
        "[Note v]\n[VER > 1.0 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "v1.0a")},
    ),
    (
        "a version quirk: the version gives back a digit to leave a plugin",
        "[Note v]\n[VER = 1.0]\n",
        ["Mod.esp"],
        {},
        NODISK,
    ),
    (
        "a requires on a version",
        "[Requires need new]\nPatch.esp\n[VER > 2.0 Base.esp]\n",
        ["Patch.esp", "Base.esp"],
        {},
        {"Base.esp": (500, "Version 1.0")},
    ),
    # --- [SIZE]
    (
        "a size that matches",
        "[Note s]\n[SIZE 1234 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (1234, "")},
    ),
    (
        "a size that does not",
        "[Note s]\n[SIZE 1234 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (999, "")},
    ),
    ("a negated size", "[Note s]\n[SIZE !1234 Mod.esp]\n", ["Mod.esp"], {}, {"Mod.esp": (999, "")}),
    (
        "a negated size that is the size",
        "[Note s]\n[SIZE !1234 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (1234, "")},
    ),
    ("a size with no data folders", "[Note s]\n[SIZE 1234 Mod.esp]\n", ["Mod.esp"], {}, None),
    ("a size, the folder empty", "[Note s]\n[SIZE 1234 Mod.esp]\n", ["Mod.esp"], {}, NODISK),
    (
        "a size, the folder readable but not holding it",
        "[Note s]\n[SIZE 1234 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Other.esp": (1234, "")},
    ),
    (
        "a size of a wildcard: one of two",
        "[Note s]\n[SIZE 1234 Mod*.esp]\n",
        ["Mod1.esp", "Mod2.esp"],
        {},
        {"Mod1.esp": (999, ""), "Mod2.esp": (1234, "")},
    ),
    (
        "a size that is huge",
        "[Note s]\n[SIZE 99999999999999999999999 Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "")},
    ),
    # --- [DESC]
    (
        "a description that matches",
        "[Note d]\n[DESC /[Tt]ribunal/ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "Tribunal patch")},
    ),
    (
        "a description that does not",
        "[Note d]\n[DESC /Bloodmoon/ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "Tribunal patch")},
    ),
    (
        "a negated description",
        "[Note d]\n[DESC !/Bloodmoon/ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "Tribunal patch")},
    ),
    ("a description with no data folders", "[Note d]\n[DESC /x/ Mod.esp]\n", ["Mod.esp"], {}, None),
    (
        "a description with a regex Python rejects",
        "[Note d]\n[DESC /(abc/ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "abc")},
    ),
    (
        "a negated description with a regex Python rejects",
        "[Note d]\n[DESC !/(abc/ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "abc")},
    ),
    (
        "a description with a non-ASCII character",
        "[Note d]\n[DESC /Caf./ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "Caf\u00e9 \u2013 v1.5")},
    ),
    (
        "a description never reached is never read",
        "[Note d]\n[ALL Absent.esp [DESC /(?i)x/ Mod.esp]]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "x")},
    ),
    (
        "a MWSE-Lua test never holds",
        "[Note m]\n[MWSE-LUA /x/ Mod.esp]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "")},
    ),
    (
        "a negated MWSE-Lua test",
        "[Note m]\n[NOT [MWSE-LUA /x/ Mod.esp]]\n",
        ["Mod.esp"],
        {},
        {"Mod.esp": (500, "")},
    ),
    (
        "a patch gated on a description",
        "[Patch for the tribunal one]\nPatch.esp\n[DESC /Tribunal/ Base.esp]\n",
        ["Patch.esp", "Base.esp"],
        {},
        {"Base.esp": (500, "Bloodmoon")},
    ),
    (
        "an unrecognised bracketed form never holds",
        "[Note u]\n[FOO bar Mod.esp]\n",
        ["Mod.esp"],
        {},
        NODISK,
    ),
    ("a /message/ is true and names nothing", "[Note m]\n/hello/\nA.esp\n", ["A.esp"], {}, None),
]


def _expression(r: Rng, depth: int = 0) -> str:
    roll = r.below(14)
    if roll < 5 or depth > 2:
        return r.pick(POOL if r.below(4) else PATTERNS)
    plugin = r.pick(POOL if r.below(3) else PATTERNS)
    if roll == 5:
        return f"[VER {r.pick(['=', '<', '>'])} {r.pick(['1.0', '1.2', '2.0b', '3', '1.0.0.1', '4_5'])} {plugin}]"
    if roll == 6:
        return f"[SIZE {r.pick(['', '!'])}{r.pick(['400', '1234', '5000'])} {plugin}]"
    if roll == 7:
        return f"[DESC {r.pick(['', '!'])}/{r.pick(REGEXES)}/ {plugin}]"
    if roll == 8:
        return f"[MWSE-LUA /x/ {plugin}]"
    inner = " ".join(_expression(r, depth + 1) for _ in range(1 + r.below(3)))
    return f"[{r.pick(['ALL', 'ANY', 'NOT', 'ALL', 'ANY', ''])} {inner}]".replace("[ ", "[")


def _rules(r: Rng) -> str:
    text = ""
    for _ in range(1 + r.below(4)):
        keyword = r.pick(
            ["Conflict", "Requires", "Note", "Patch", "Note", "Requires", "Order", "Version"]
        )
        if r.below(5) == 0:
            keyword = keyword.upper() if r.below(2) else keyword.lower()
        text += f"[{keyword}{' ' + r.pick(MESSAGES) if r.below(2) else ''}]\n"
        if r.below(4) == 0:
            text += f"  {r.pick(MESSAGES) or 'message'}\n"
        for _ in range(1 + r.below(4)):
            line = _expression(r)
            if r.below(5) == 0:
                line += " ; a comment"
            text += line + "\n"
            if r.below(6) == 0:
                text += f"\t{r.pick(MESSAGES) or 'indented'}\n"
    return text


def fuzz(seed: int) -> Check:
    """A made-up setup: rule text, load order, origins and plugin files."""
    r = Rng(seed)
    text = _rules(r)
    pool = [*POOL, "Theta1.esp", "Theta2.esp", "Iota.esp", "Iotaa.esp"]
    order = [n for n in pool if r.chance(1, 2)]
    order = [n.lower() if r.below(6) == 0 else n for n in order]
    origins = {n.lower(): f"origin {i}" for i, n in enumerate(order) if r.below(4) == 0}
    kind = r.below(6)
    disk: dict[str, tuple[int, str]] | None
    if kind == 0:
        disk = None
    elif kind == 1:
        disk = {}
    else:
        disk = {
            n: (int(r.pick(["400", "1234", "5000"])), r.pick(DESCRIPTIONS))
            for n in pool
            if r.chance(2, 3)
        }
    return (f"made up, seed {seed}", text, order, origins, disk)


def answer(
    text: str,
    order: list[str],
    origins: dict[str, str],
    disk: dict[str, tuple[int, str]] | None,
) -> dict[str, Any]:
    """What ``check_predicates`` gives over real plugin files, and what it read from them."""
    from wraithguard.plugins import PluginFileIndex, read_plugin_description
    from wraithguard.rules.predicates import check_predicates

    with tempfile.TemporaryDirectory() as tmp:
        data_dirs: list[str] | None = None
        if disk is not None:
            for name, (size, description) in disk.items():
                (Path(tmp) / name).write_bytes(tes3(description, size))
            data_dirs = [tmp]
        index = PluginFileIndex(data_dirs)
        files: dict[str, dict[str, Any]] = {}
        for name in disk or {}:
            path = index.find(name)
            if path is None:
                msg = f"{name} was written but not found"
                raise RuntimeError(msg)
            files[name.lower()] = {
                "size": path.stat().st_size,
                "description": read_plugin_description(path),
            }
        warnings = check_predicates(text, order, origins, data_dirs)
        return {"usable": index.usable, "files": files, "warnings": warnings}


def _ast(node: object) -> object:
    return [_ast(n) for n in node] if isinstance(node, list) else node


def expression_case(text: str) -> dict[str, Any]:
    """Tokens, tree and descriptions of one predicate body."""
    from wraithguard.rules.expressions import describe_node, parse_mlox_lisp, tokenize_mlox_logic

    tokens = tokenize_mlox_logic(text)
    ast = parse_mlox_lisp(list(tokens))
    return {
        "text": text,
        "tokens": tokens,
        "ast": _ast(ast),
        "described": [describe_node(n) for n in ast],
    }


def cases() -> dict[str, Any]:
    """The cases, answered by the Python bodies (not ordercore's, which they check)."""
    sys.path.insert(0, str(ROOT))
    from wraithguard import ordercore_bridge

    with ordercore_bridge.python_only():
        return _cases()


def _cases() -> dict[str, Any]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    from wraithguard.versions import format_version

    r = Rng(7)
    texts = [*EXPRESSIONS]
    texts += [" ".join(_expression(r) for _ in range(1 + r.below(3))) for _ in range(150)]
    versions = [{"version": v, "formatted": format_version(v)} for v in VERSIONS]
    checks = []
    todo: list[tuple[Check, bool]] = [(c, True) for c in HAND]
    todo += [(fuzz(seed), False) for seed in range(1, 301)]
    for (name, text, order, origins, disk), must in todo:
        checks.append(
            {
                "name": name,
                "must_support": must and not any(t in text for t in DECLINED),
                "rules": text,
                "order": order,
                "origins": origins,
                "disk": None if disk is None else {n: list(v) for n, v in disk.items()},
                **answer(text, order, origins, disk),
            }
        )
    return {
        "expressions": [expression_case(t) for t in texts],
        "versions": versions,
        "checks": checks,
    }


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "The predicate rules: expressions, versions and check_predicates over plugin files, "
            "answered by wraithguard.rules; run by tests/test_predicate_cases.py and "
            "viewer-shell/ordercore/tests/predicate_cases.rs. Regenerate: python "
            "tools/gen_predicate_cases.py"
        ),
        **out,
    }
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"{len(out['expressions'])} expressions, {len(out['versions'])} versions, "
        f"{len(out['checks'])} checks -> {OUT.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
