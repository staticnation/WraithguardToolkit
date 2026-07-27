"""Regenerate ``mlox_subset/mwscript/opcodes.py``.

Licence policy: this project copies no GPL source, so the table is built from
MWEdit's ``Functions.dat`` (MIT) only. Compiler-internal opcodes -- which no
function table lists -- are re-derived from a corpus of real compiled scripts
instead, making them observations about the game's own data rather than a copy
of anyone's source. See ``tests/test_mwscript.py::TestOpcodeTable``.

A second, optional input is ``customfunctions.dat``: the MWSE / MW-Enhanced
function definitions MWEdit reads alongside its own table. It is *not* MWSE
source -- it is the data file the MWSE updater installs, in MWEdit's own text
format -- so the no-GPL-source policy is unaffected.

That file names parameter types symbolically (``Long | String``) where
``Functions.dat`` uses hex flag words. The mapping between the two was not
copied from any header: the two files describe 106 of the same functions, so
correlating those gives each symbolic name's bit value directly, with no
ambiguity (every name resolved to exactly one value). The result agrees with the
``FLAG_*`` constants the disassembler already carries, which is the check that
the derivation is right.

Where the two tables disagree the existing ``Functions.dat`` entry is kept and
the disagreement printed, with one documented exception: see ``CORRECTIONS``.

Usage:
    python tools/gen_opcodes.py ../MWEdit-dev/data/Functions.dat [customfunctions.dat]
"""

from __future__ import annotations

import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "mlox_subset/mwscript/opcodes.py"

#: Parameter type names as ``customfunctions.dat`` spells them, mapped to the
#: flag bits the disassembler decodes with. Derived by correlating the 106
#: functions both input files describe (see the module docstring), not copied.
#: ``Ref`` is a long by the file's own documentation ("same as long").
SYMBOLIC_FLAGS: dict[str, int] = {
    "byte": 0x0001,
    "short": 0x0002,
    "long": 0x0004,
    "ref": 0x0004,
    "float": 0x0008,
    "string": 0x0010,
    "id": 0x0020,
    "optional": 0x0800,
    "many": 0x8000,
    "none": 0x0000,
}

#: Entries where the shipped tables are demonstrably wrong, corrected against
#: UESP's per-function documentation. Kept explicit and tiny: a table that
#: silently "fixes" its own inputs is a table nobody can check.
#:
#: ``0x3C33`` -- MWEdit's ``Functions.dat`` gives ``XFileWriteFloat`` a single
#: float operand, with no filename. Two independent sources say otherwise:
#: ``customfunctions.dat`` lists two parameters, and UESP documents the syntax as
#: ``xFileWriteFloat filename (string), value (float)``. Its three siblings
#: (``XFileWriteShort``/``Long``/``String``) all take the filename first, so the
#: omission is a typo rather than a real asymmetry. Left uncorrected, every call
#: to it decodes one operand short and desynchronises the rest of the stream.
#:
#: The filename is a *string* (``0x10``), not ``Long | String`` (``0x14``), which
#: is also why the other 25 disagreements in this family are resolved MWEdit's
#: way: UESP documents the parameter as a string, and a wrong string decode is
#: caught by ``_plausible_identifier`` while a wrong 4-byte read is not.
CORRECTIONS: dict[int, tuple[str, tuple[int, ...]]] = {
    0x3C33: ("XFileWriteFloat", (0x10, 0x8)),
}

#: Opcodes the compiler emits that appear in no function table. Values derived
#: by correlating real bytecode against its own source text, not copied.
#: ``_SetReference``: emitted for ``id->Func``; 0x010C carried the
#: length-prefixed target id in 200/200 observed cases, with no rival value.
CORPUS_DERIVED: dict[int, tuple[str, tuple[int, ...]]] = {
    0x010C: ("_SetReference", (0x20,)),
}


def parse_functions_dat(path: Path) -> dict[int, tuple[str, tuple[int, ...]]]:
    """Parse MWEdit's ``Functions.dat`` into ``{opcode: (name, params)}``.

    The file is a sequence of blocks::

        Function = Activate
            Options = 0x8
            Opcode = 0x1017
            Param1 = 0x820, "player"
        End

    ``ParamN`` keys are read in numeric order; the flag word before the comma
    is the operand's encoding, and the quoted label is discarded.

    Args:
        path: Location of ``Functions.dat``.

    Returns:
        Every function block that declared both a name and an opcode.
    """
    text = path.read_text(encoding="latin-1")
    table: dict[int, tuple[str, tuple[int, ...]]] = {}
    name: str | None = None
    opcode: int | None = None
    params: dict[int, int] = {}

    def flush() -> None:
        """Commit the block just parsed, if it was complete."""
        if name and opcode is not None:
            table[opcode] = (name, tuple(params[k] for k in sorted(params)))

    for raw_line in text.splitlines():
        line = raw_line.strip()
        if line.lower() == "end":
            flush()
            name, opcode, params = None, None, {}
            continue
        key, _, value = line.partition("=")
        key, value = key.strip().lower(), value.strip()
        if not value:
            continue
        try:
            if key == "function":
                flush()
                name, opcode, params = value, None, {}
            elif key == "opcode":
                opcode = int(value, 16)
            elif key.startswith("param") and key[5:].isdigit():
                params[int(key[5:])] = int(value.split(",")[0].strip(), 16)
        except ValueError:
            continue  # a malformed field should not abort the whole parse
    flush()
    return table


def parse_custom_functions(path: Path) -> dict[int, tuple[str, tuple[int, ...]]]:
    """Parse ``customfunctions.dat`` into ``{opcode: (name, params)}``.

    A different dialect of the same idea::

        function
            Name = XAddItem
            Options = MWSE | AllowGlobal
            Param1 = Long | String
            Opcode = 0x3c28
        end

    Blocks open with a bare ``function`` and the name is a ``Name`` key; ``#``
    starts a comment anywhere on a line; parameter types are symbolic and
    ``|``-separated. An unrecognised type name contributes no bits rather than
    aborting the block, so one new keyword in a future release costs one
    function, not the file.

    Args:
        path: Location of ``customfunctions.dat``.

    Returns:
        Every function block that declared both a name and an opcode.
    """
    table: dict[int, tuple[str, tuple[int, ...]]] = {}
    name: str | None = None
    opcode: int | None = None
    params: dict[int, int] = {}

    def flush() -> None:
        """Commit the block just parsed, if it was complete."""
        if name and opcode is not None:
            table[opcode] = (name, tuple(params[k] for k in sorted(params)))

    for raw_line in path.read_text(encoding="latin-1").splitlines():
        line = raw_line.split("#")[0].strip()
        if not line:
            continue
        if line.lower() == "end":
            flush()
            name, opcode, params = None, None, {}
            continue
        if line.lower() == "function":
            flush()
            name, opcode, params = None, None, {}
            continue
        key, _, value = line.partition("=")
        key, value = key.strip().lower(), value.strip()
        if not value:
            continue
        if key == "name":
            name = value
        elif key == "opcode":
            try:
                opcode = int(value, 16)
            except ValueError:
                opcode = None  # a malformed opcode drops the block, not the file
        elif key.startswith("param") and key[5:].isdigit():
            flags = 0
            for token in value.split("|"):
                flags |= SYMBOLIC_FLAGS.get(token.strip().lower(), 0)
            params[int(key[5:])] = flags
    flush()
    return table


def merge_custom(
    table: dict[int, tuple[str, tuple[int, ...]]],
    custom: dict[int, tuple[str, tuple[int, ...]]],
) -> tuple[set[int], list[str]]:
    """Add the custom functions that the main table does not already describe.

    Existing entries always win. The two files agree on the 106 opcodes they
    share except for two renames and 26 differing operand shapes, and the
    entries already in the table are the ones the corpus and the test suite
    have been run against -- so a disagreement is reported for a human to judge
    rather than silently applied.

    Args:
        table: The table from ``Functions.dat``; extended in place.
        custom: The table from ``customfunctions.dat``.

    Returns:
        The opcodes added, and one line per disagreement found.
    """
    added: set[int] = set()
    notes: list[str] = []
    for opcode, (name, params) in sorted(custom.items()):
        if opcode not in table:
            table[opcode] = (name, params)
            added.add(opcode)
            continue
        existing_name, existing_params = table[opcode]
        if existing_name.lower() != name.lower():
            notes.append(f"0x{opcode:04X}: name {existing_name} kept, custom says {name}")
        elif existing_params != params:
            notes.append(
                f"0x{opcode:04X}: {existing_name} operands "
                f"{[hex(p) for p in existing_params]} kept, custom says {[hex(p) for p in params]}"
            )
    return added, notes


def _frozenset_literal(name: str, opcodes: set[int]) -> list[str]:
    """Emit a typed ``frozenset`` constant.

    Args:
        name: The constant's name.
        opcodes: Its members.

    Returns:
        The source lines. An empty set is written as ``frozenset()`` rather than
        ``frozenset({})``, which reads as an empty *dict* to anyone skimming.
    """
    if not opcodes:
        return [f"{name}: Final[frozenset[int]] = frozenset()"]
    members = ", ".join(f"0x{o:04X}" for o in sorted(opcodes))
    return [
        f"{name}: Final[frozenset[int]] = frozenset(",
        "    {" + members + "}",
        ")",
    ]


def main(argv: list[str]) -> int:
    """Write the generated module. Returns a process exit code."""
    if len(argv) not in (2, 3):
        print(__doc__)
        return 2
    table = parse_functions_dat(Path(argv[1]))
    if not table:
        print("no functions parsed -- is that really Functions.dat?", file=sys.stderr)
        return 1
    extended: set[int] = set()
    if len(argv) == 3:
        custom = parse_custom_functions(Path(argv[2]))
        if not custom:
            print("no functions parsed from the custom table", file=sys.stderr)
            return 1
        extended, notes = merge_custom(table, custom)
        for note in notes:
            print(f"  kept existing: {note}", file=sys.stderr)
    for opcode, entry in CORRECTIONS.items():
        if opcode in table and table[opcode] != entry:
            print(f"  corrected: 0x{opcode:04X} -> {entry[0]}{entry[1]}", file=sys.stderr)
        table[opcode] = entry
    for opcode, entry in CORPUS_DERIVED.items():
        table.setdefault(opcode, entry)

    lines = [
        '"""Morrowind script opcodes and their operand shapes.',
        "",
        "GENERATED FILE -- do not edit by hand. See ``tools/gen_opcodes.py``.",
        "",
        "Sources:",
        "",
        "* MWEdit ``data/Functions.dat`` -- names and the parameter flag words",
        "  that give each function's operand shape. Copyright 2025 Walrus Tech,",
        "  MIT-licensed, so safe to derive from.",
        "* MWSE's ``customfunctions.dat`` -- the MWSE / MW-Enhanced function",
        "  definitions, in MWEdit's own text format. This is the data file the",
        "  MWSE updater installs, not MWSE source, so the no-GPL-source policy",
        "  is unaffected. Only opcodes the main table does not already describe",
        "  are taken from it.",
        "* UESP's per-function MWSE pages -- for the one entry where both tables",
        "  are wrong or disagree. Documentation, not source; see ``CORRECTIONS``",
        "  in the generator for the evidence behind each.",
        "* A corpus of real compiled scripts -- for the compiler-internal opcodes",
        "  that no function table lists. These were measured, not copied: an",
        "  opcode's value is a fact about the game's data files.",
        "",
        "Deliberately *not* used: MWSE's ``OpCodes.h``. It is GPLv2, and this",
        "project's standing policy is to copy no GPL source (see CREDITS.md).",
        "MWSE was consulted only to confirm agreement, which cost nothing: the",
        "MWSE-only functions never appeared in the corpus, and the one internal",
        "opcode that matters was re-derived independently.",
        "",
        "Note: ``else``/``endif`` have no opcodes -- the compiler emits jumps.",
        '"""',
        "",
        "from __future__ import annotations",
        "",
        "from typing import Final",
        "",
        "#: Opcode -> (name, parameter flag words).",
        "FUNCTIONS: Final[dict[int, tuple[str, tuple[int, ...]]]] = {",
    ]
    for opcode in sorted(table):
        name, params = table[opcode]
        args = ", ".join(hex(p) for p in params) + ("," if len(params) == 1 else "")
        lines.append(f'    0x{opcode:04X}: ("{name}", ({args})),')
    lines += [
        "}",
        "",
        "#: Opcodes the compiler emits itself; their names appear in no source",
        "#: text, so name-based filtering must never exclude them.",
        *_frozenset_literal("INTERNAL", set(CORPUS_DERIVED)),
        "",
        "#: Opcodes that come from the MWSE / MW-Enhanced table rather than the",
        "#: base game's. A script using one of these needs that runtime installed,",
        "#: which is worth saying out loud in a disassembly.",
        *_frozenset_literal("EXTENDED", extended),
        "",
        "#: Lowercased name -> opcode.",
        "BY_NAME: Final[dict[str, int]] = {",
        "    name.lower(): opcode for opcode, (name, _params) in FUNCTIONS.items()",
        "}",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(
        f"wrote {len(table)} opcodes ({len(CORPUS_DERIVED)} corpus-derived, "
        f"{len(extended)} from the MWSE table, {len(CORRECTIONS)} corrected) -> {OUT}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
