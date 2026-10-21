"""Regenerate ``tests/fixtures/rule_cases.json`` from the Python rule parser.

The cases are the inputs written here; the answers are what
``wraithguard.rules.parser.parse_mlox_text`` gives. ``ordercore::rules``
(``viewer-shell/ordercore``) runs the same file, so the two cannot drift. Run it again
when a rule is added or changed, review the diff, and commit both the script and the file.

Run: ``python tools/gen_rule_cases.py``.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "rule_cases.json"

#: ``(name, rule file text)``. Everything here must be a case the Rust port can run: no
#: input that only differs in how Python folds the case of a non-ASCII letter.
INPUTS: list[tuple[str, str]] = [
    ("empty", ""),
    ("one order block", "[Order]\nMorrowind.esm\nTribunal.esm\nBloodmoon.esm\n"),
    ("keywords in any case", "[order]\nA.esp\nB.esp\n[NEARSTART]\nC.esp\n[NearEnd]\nD.esp\n"),
    (
        "other keywords end a block and are not read",
        "[Order]\nA.esp\n[Conflict]\nB.esp\n[Note]\nC.esp\n[Order]\nD.esp\n",
    ),
    (
        "a header may carry arguments",
        "[Order extra words]\nA.esp\n[Requires foo]\nB.esp\n[NearEnd 3]\nC.esp\n",
    ),
    ("not a header: the word runs on", "[Order]\nA.esp\n[Orderly]\nB.esp\n"),
    ("not a header: unclosed", "[Order\nA.esp\n[Order]\nB.esp\n"),
    ("not a header: prose mid-line", "[Order]\nA.esp\nsee the [Order] section\nB.esp\n"),
    ("the header's own line is body", "[Order] A.esp B.esp\nC.esp\n"),
    ("comments", "[Order] ; first\nA.esp ; the first\n; whole line\nB.esp;tight\n"),
    ("CRLF", "[Order]\r\nA.esp\r\nB.esp\r\n"),
    ("CR only", "[Order]\rA.esp\rB.esp\r"),
    (
        "vertical tab, form feed and the unit separators break lines",
        "[Order]\x0bA.esp\x0cB.esp\x1cC.esp\x1dD.esp\x1eE.esp",
    ),
    ("NEL and the Unicode separators break lines", "[Order]\u0085A.esp\u2028B.esp\u2029C.esp"),
    ("\\x1f is whitespace, not a break", "[Order]\nA.esp\x1fB.esp\n"),
    (
        "names with spaces and punctuation",
        "[Order]\nBetter Bodies - Clean (v2).esp\nTom & Jerry+.esm\n",
    ),
    ("wildcards and <VER>", "[Order]\nBetter Bodies*.esp\nMod <VER>.esp\nWhat?.esp\n"),
    ("a trailing star after the extension", "[Order]\nA.esp*\nB.esm*\n"),
    ("several names on a line", "[Order]\nA.esp B.esm C.omwaddon\n"),
    ("trailing words after a name are dropped", "[Order]\nA.esp is first\nB.esp, then\n"),
    (
        "the extensions, in any case",
        "[Order]\nA.ESP\nB.Esm\nC.omwaddon\nD.OMWGAME\nE.omwscripts\nF.esx\nG.omwaddons\n",
    ),
    ("an extension followed by more is not a name", "[Order]\nA.esp.bak\nB.espx\nC.esp\n"),
    ("a name can contain a dot", "[Order]\nA.b.c.esp\nv1.2 Mod.esm\n"),
    ("the shortest name wins", "[Order]\nA.esp.esp\nB.esp x.esp\n"),
    ("a non-breaking space ends a name", "[Order]\nA.esp\u00a0B.esp\n"),
    ("other Unicode whitespace around names", "[Order]\n\u3000A.esp\u2003B.esp\u202f\n"),
    ("non-ASCII names", "[Order]\nCafé Épique.esp\nÜbermod.esm\n"),
    ("phantom lines are counted", "[Order]\nA.esp\nsome words\n[DESC /x/ B.esp]\n\nC.esp\n"),
    ("a block with no names is dropped", "[Order]\nnothing here\n[NearStart]\nA.esp\n"),
    ("hints are not chains", "[NearStart]\nA.esp\nB.esp\n[NearEnd]\nC.esp\n"),
    ("a header only on its own line start", "  [Order]\nA.esp\n"),
    ("blank lines and indentation", "\n\n[Order]\n\n   A.esp   \n\t B.esp\t\n\n"),
    ("two blocks of one kind", "[Order]\nA.esp\n[Order]\nB.esp\nC.esp\n"),
    (
        "a realistic fragment",
        "[Version 0.1]\n[Order]\nMorrowind.esm\nTribunal.esm\nBloodmoon.esm\nTamriel_Data.esm\n"
        "[Note]\nsee the [Order] section\n[Conflict]\nA.esp\nB.esp\n"
        "[NearEnd]\nMerged Objects.esp\nMashed Lists.esp\n",
    ),
]


def cases() -> list[dict[str, object]]:
    """The cases, answered by the Python parser."""
    sys.path.insert(0, str(ROOT))
    from wraithguard.rules.parser import parse_mlox_text

    out: list[dict[str, object]] = []
    for name, text in INPUTS:
        blocks, skipped = parse_mlox_text(text)
        out.append(
            {
                "name": name,
                "text": text,
                "blocks": [[keyword, names] for keyword, names in blocks],
                "skipped": skipped,
            }
        )
    return out


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    doc = {
        "_about": (
            "mlox rule text -> ordering blocks, answered by wraithguard.rules.parser."
            "parse_mlox_text; run by tests/test_rule_cases.py and "
            "viewer-shell/ordercore/tests/rule_cases.rs. Regenerate: "
            "python tools/gen_rule_cases.py"
        ),
        "cases": cases(),
    }
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(doc['cases'])} cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
