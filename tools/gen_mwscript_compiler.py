"""Regenerate ``wraithguard/mwscript/compiler_data.py``.

The MWScript compiler (``wraithguard/mwscript/compiler.py``) is a port of
MWEdit's table-driven compiler. Its data -- the parse tables, the built-in
function table, the magic effect IDs, the animation groups and the character
classes -- is read here from MWEdit's source rather than typed in by hand, so a
regeneration against a newer MWEdit picks up its fixes.

MWEdit is MIT-licensed (Copyright 2025 Walrus Tech; originally by Dave
Humphrey), the same licence already relied on for ``opcodes.py``.

Inputs, relative to the MWEdit checkout given on the command line:

- ``mwedit/script_compile.cc`` - the 53 parse tables and ``l_EsmScrCharTypes``.
- ``mwedit/script_funcs.cc`` - ``g_ScriptFunctions`` and ``g_ScriptFuncAlpha``.
- ``mwedit/script_defs.h`` - the flag values the function table is written in.
- ``game/morrowind/defs.cc`` - ``l_Effects`` and ``l_AnimData``.
- ``data/customfunctions.dat`` - the MWSE / MW-Enhanced functions MWEdit ships, the
  compiler's default extended set (a user's own file can replace it at run time).

Usage:
    python tools/gen_mwscript_compiler.py ../MWEdit-dev
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable

OUT = Path(__file__).resolve().parent.parent / "wraithguard/mwscript/compiler_data.py"

TABLE_FLAGS = {"ESTF_ONE": 1, "ESTF_MANY": 2, "ESTF_OPT": 4, "ESTF_STOP": 8}


def _strip_comments(src: str) -> str:
    """Drop C and C++ comments (none of the inputs has ``//`` inside a string)."""
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def _defines(src: str) -> dict[str, str]:
    """``#define NAME value`` lines, value as written."""
    out: dict[str, str] = {}
    for m in re.finditer(r"^\s*#define\s+(\w+)\s+(.+?)\s*$", src, re.M):
        out[m.group(1)] = m.group(2)
    return out


def _evaluator(defs: dict[str, str]) -> Callable[[str], int]:
    """Return a function evaluating a C integer expression over ``defs``."""
    cache: dict[str, int] = {}

    def value(expr: str) -> int:
        expr = expr.strip()
        if expr in cache:
            return cache[expr]

        def name(m: re.Match[str]) -> str:
            word = m.group(0)
            if word in defs:
                return f"({value(defs[word])})"
            raise KeyError(word)

        text = re.sub(r"0[xX][0-9a-fA-F]+|\d+", lambda m: str(int(m.group(0), 0)), expr)
        text = re.sub(r"[A-Za-z_]\w*", name, text)
        result = int(eval(text, {"__builtins__": {}}))  # noqa: S307 - our own C constants
        cache[expr] = result
        return result

    return value


def _braced(body: str) -> list[str]:
    """The top-level ``{...}`` groups in an initialiser body."""
    out, depth, start = [], 0, 0
    for i, ch in enumerate(body):
        if ch == "{":
            if depth == 0:
                start = i + 1
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                out.append(body[start:i])
    return out


def _array(src: str, decl: str) -> str:
    """The initialiser body of the array declared as ``decl``."""
    m = re.search(re.escape(decl) + r"\s*=\s*\{", src)
    if m is None:
        raise SystemExit(f"{decl!r} not found")
    depth, i = 1, m.end()
    while depth:
        depth += {"{": 1, "}": -1}.get(src[i], 0)
        i += 1
    return src[m.end() : i - 1]


def parse_tables(src: str) -> dict[str, list[tuple[str, int, str | None, str | None, str | None]]]:
    """``l_*`` parse tables: (token, flags, sub-table, parse func, output func)."""
    src = _strip_comments(src)
    tables: dict[str, list[tuple[str, int, str | None, str | None, str | None]]] = {}
    for m in re.finditer(r"esmscrparsetable_t\s+(l_\w+)\[\]\s*=\s*\{(.*?)\n\};", src, re.S):
        name, body = m.groups()
        rows = []
        for group in _braced(body):
            fields = [f.strip() for f in group.split(",") if f.strip()]
            fields += ["NULL"] * (5 - len(fields))
            token, flags, table, parse, output = fields[:5]
            flag_value = (
                0 if flags == "0" else sum(TABLE_FLAGS[f.strip()] for f in flags.split("|"))
            )

            def clean(text: str) -> str | None:
                return None if text == "NULL" else text.replace("&CEsmScriptCompile::", "")

            rows.append(
                (
                    token.replace("ESMSCR_", ""),
                    flag_value,
                    clean(table),
                    clean(parse),
                    clean(output),
                )
            )
        tables[name] = rows
    return tables


def parse_char_types(src: str) -> list[int]:
    """``l_EsmScrCharTypes``: one class bitmask per byte value."""
    body = _strip_comments(_array(src, "int l_EsmScrCharTypes[257]"))
    names = {
        "0": 0,
        "ESCT_SPACE": 1,
        "ESCT_PUNCT": 2,
        "ESCT_DIGIT": 4 | 16,
        "ESCT_SYMF": 8 | 16,
        "ESCT_SYM": 16,
    }
    values = [names[v.strip()] for v in body.split(",") if v.strip()]
    return (values + [0] * 257)[:256]


def parse_functions(
    funcs_src: str, defs_src: str
) -> tuple[list[tuple[str, int, int, int, list[int]]], list[int]]:
    """``g_ScriptFunctions`` rows and the ``g_ScriptFuncAlpha`` start indices (-1 = NULL)."""
    defs = _defines(_strip_comments(defs_src))
    defs.update(_defines(_strip_comments(funcs_src)))
    value = _evaluator(defs)
    rows = []
    for group in _braced(
        _strip_comments(_array(funcs_src, "esmscrfuncinfo_t g_ScriptFunctions[]"))
    ):
        fields = [f.strip() for f in group.split(",") if f.strip()]
        if fields[0] == "NULL":
            break
        name = re.fullmatch(r'_T\("([^"]*)"\)', fields[0])
        if name is None:
            raise SystemExit(f"bad function name {fields[0]!r}")
        nums = [value(f) for f in fields[1:]]
        nums += [0] * (16 - len(nums))
        rows.append((name.group(1), nums[0] & 0xFFFF, nums[1], nums[2], nums[3:16]))
    alpha = []
    for raw in _strip_comments(_array(funcs_src, "esmscrfuncinfo_t *g_ScriptFuncAlpha[26]")).split(
        ","
    ):
        entry = raw.strip()
        if not entry:
            continue
        m = re.fullmatch(r"&g_ScriptFunctions\[(\d+)\]", entry)
        alpha.append(int(m.group(1)) if m else -1)
    return rows, alpha


def parse_effects(src: str) -> list[str]:
    """``l_Effects`` IDs (the GMST-style names a script may use), in index order."""
    out = []
    for group in _braced(
        _strip_comments(_array(src, "esmeffectdata_t l_Effects[MWESM_MAX_EFFECTS]"))
    ):
        strings = re.findall(r'"([^"]*)"', group)
        out.append(strings[1])
    return out


def parse_anims(src: str) -> list[tuple[str, int, int]]:
    """``l_AnimData``: (group name, ID, pre-Tribunal ID)."""
    out = []
    for group in _braced(_strip_comments(_array(src, "esmanimdata_t l_AnimData[]"))):
        fields = [f.strip() for f in group.split(",") if f.strip()]
        if fields[0] == "NULL":
            break
        out.append((fields[0].strip('"'), int(fields[1], 0), int(fields[2], 0)))
    return out


def main(argv: list[str]) -> int:
    """Write the generated module. Returns a process exit code."""
    if len(argv) != 2:
        print(__doc__)
        return 2
    root = Path(argv[1])
    read = lambda rel: (root / rel).read_text(encoding="latin-1")  # noqa: E731
    compile_src = read("mwedit/script_compile.cc")
    tables = parse_tables(compile_src)
    chars = parse_char_types(compile_src)
    funcs, alpha = parse_functions(read("mwedit/script_funcs.cc"), read("mwedit/script_defs.h"))
    defs_cc = read("game/morrowind/defs.cc")
    effects = parse_effects(defs_cc)
    anims = parse_anims(defs_cc)
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from wraithguard.mwscript.compiler import parse_custom_functions

    custom = sorted(
        parse_custom_functions(read("data/customfunctions.dat")).values(),
        key=lambda f: f.name.lower(),
    )

    lines = [
        '"""Data for the MWScript compiler, generated from MWEdit\'s source.',
        "",
        "GENERATED FILE -- do not edit by hand. See ``tools/gen_mwscript_compiler.py``.",
        "",
        "Source: MWEdit (Copyright 2025 Walrus Tech; originally Dave Humphrey), MIT.",
        '"""',
        "",
        "# fmt: off",
        "",
        "from __future__ import annotations",
        "",
        "from typing import Final",
        "",
        "#: Character class per byte: 1 space, 2 operator, 4 digit, 8 symbol start, 16 symbol.",
        "CHAR_TYPES: Final[tuple[int, ...]] = (",
    ]
    lines.extend(
        "    " + ", ".join(str(v) for v in chars[i : i + 16]) + "," for i in range(0, 256, 16)
    )
    lines += [
        ")",
        "",
        "#: Parse tables: name -> rows of (token, flags, sub-table, parse func, output func).",
        "#: Flags: 1 one, 2 many, 4 optional, 8 stop.",
        "TABLES: Final[dict[str, tuple[tuple[str, int, str | None, str | None, str | None], ...]]] = {",
    ]
    for name, rows in tables.items():
        lines.append(f"    {name!r}: (")
        lines.extend(f"        {row!r}," for row in rows)
        lines.append("    ),")
    lines += [
        "}",
        "",
        "#: Built-in functions in MWEdit's (alphabetical) order:",
        "#: (name, opcode, function flags, return type, 13 argument flag words).",
        "FUNCTIONS: Final[tuple[tuple[str, int, int, int, tuple[int, ...]], ...]] = (",
    ]
    for name, opcode, flags, ret, args in funcs:
        lines.append(
            f"    ({name!r}, 0x{opcode:04X}, 0x{flags:X}, 0x{ret:X}, ({', '.join(hex(a) for a in args)})),"
        )
    lines += [
        ")",
        "",
        "#: Index into FUNCTIONS of the first name per letter A-Z (-1: none).",
        f"FUNC_ALPHA: Final[tuple[int, ...]] = ({', '.join(str(a) for a in alpha)})",
        "",
        "#: Magic effect IDs by effect index.",
        "EFFECT_IDS: Final[tuple[str, ...]] = (",
    ]
    lines.extend(f"    {e!r}," for e in effects)
    lines += [
        ")",
        "",
        "#: Animation groups: (name, ID, pre-Tribunal ID).",
        "ANIM_GROUPS: Final[tuple[tuple[str, int, int], ...]] = (",
    ]
    lines.extend(f"    {a!r}," for a in anims)
    lines += [
        ")",
        "",
        "#: MWEdit's extended (MWSE / MW-Enhanced) functions, from its customfunctions.dat:",
        "#: (name, opcode, function flags, 13 argument flag words).",
        "CUSTOM_FUNCTIONS: Final[tuple[tuple[str, int, int, tuple[int, ...]], ...]] = (",
    ]
    lines.extend(
        f"    ({f.name!r}, 0x{f.opcode:04X}, 0x{f.flags:X}, ({', '.join(hex(a) for a in f.var)})),"
        for f in custom
    )
    lines += [")", ""]
    OUT.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    tokens = {
        name.removeprefix("ESMSCR_"): int(value)
        for name, value in _defines(_strip_comments(read("mwedit/script_defs.h"))).items()
        if name.startswith("ESMSCR_") and re.fullmatch(r"-?\d+", value)
    }
    write_rust(tables, chars, funcs, effects, anims, tokens, custom)
    print(
        f"wrote {OUT} and {OUT_RS}: {len(tables)} tables, {len(funcs)} functions, "
        f"{len(effects)} effects, {len(anims)} anim groups"
    )
    return 0


OUT_RS = Path(__file__).resolve().parent.parent / "native/src/mwscript/data.rs"


def _rs_str(text: str) -> str:
    """A Rust byte-string literal."""
    return (
        'b"'
        + "".join(c if 32 <= ord(c) < 127 and c not in '"\\' else f"\\x{ord(c):02x}" for c in text)
        + '"'
    )


def write_rust(
    tables: dict[str, list[tuple[str, int, str | None, str | None, str | None]]],
    chars: list[int],
    funcs: list[tuple[str, int, int, int, list[int]]],
    effects: list[str],
    anims: list[tuple[str, int, int]],
    tokens: dict[str, int],
    custom: list[Any],
) -> None:
    """The same data as Rust, for ``native/src/mwscript``."""
    names = list(tables)
    actions = sorted({a for rows in tables.values() for r in rows for a in r[3:] if a})
    out = [
        "//! Data for the MWScript compiler, generated from MWEdit's source.",
        "//!",
        "//! GENERATED FILE -- do not edit by hand. See `tools/gen_mwscript_compiler.py`.",
        "//! Source: MWEdit (Copyright 2025 Walrus Tech; originally Dave Humphrey), MIT.",
        "",
        "#![allow(clippy::unreadable_literal)]",
        "",
        "/// A parse or output routine a table row names (MWEdit's method names).",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        "#[allow(clippy::enum_variant_names)]",
        "pub enum Action {",
    ]
    out += [f"    {a}," for a in actions]
    out += [
        "}",
        "",
        "/// One table row: the token it expects, its flags (1 one, 2 many, 4 optional, 8",
        "/// stop), the table it chains to, and its parse and output routines.",
        "pub struct Row {",
        "    pub token: i32,",
        "    pub flags: u8,",
        "    pub sub: Option<usize>,",
        "    pub parse: Option<Action>,",
        "    pub output: Option<Action>,",
        "}",
        "",
    ]
    for i, name in enumerate(names):
        out.append(f"pub const {_const(name)}: usize = {i};")
    out += ["", "/// The parse tables, by the indices above.", "pub static TABLES: &[&[Row]] = &["]
    for name in names:
        out.append(f"    // {name}")
        out.append("    &[")
        for token, flags, sub, parse, output in tables[name]:
            sub_rs = f"Some({_const(sub)})" if sub else "None"
            p_rs = f"Some(Action::{parse})" if parse else "None"
            o_rs = f"Some(Action::{output})" if output else "None"
            out.append(
                f"        Row {{ token: {tokens[token]}, flags: {flags}, sub: {sub_rs}, "
                f"parse: {p_rs}, output: {o_rs} }},"
            )
        out.append("    ],")
    out += ["];", ""]
    out += [f"pub const {k}: i32 = {v};" for k, v in sorted(tokens.items(), key=lambda kv: kv[1])]
    out += [
        "",
        "/// Character class per byte: 1 space, 2 operator, 4 digit, 8 symbol start, 16 symbol.",
    ]
    out.append("pub static CHAR_TYPES: [u8; 256] = [")
    out += ["    " + ", ".join(str(v) for v in chars[i : i + 16]) + "," for i in range(0, 256, 16)]
    out += [
        "];",
        "",
        "/// Built-in functions: (name, opcode, function flags, return type, argument flag words).",
        "pub static FUNCTIONS: &[(&[u8], u16, u32, u32, [u64; 13])] = &[",
    ]
    for name, opcode, flags, ret, args in funcs:
        out.append(
            f"    ({_rs_str(name)}, 0x{opcode:04X}, 0x{flags:X}, 0x{ret:X}, "
            f"[{', '.join(f'0x{a:X}' for a in args)}]),"
        )
    out += [
        "];",
        "",
        "/// Magic effect IDs by effect index.",
        "pub static EFFECT_IDS: &[&[u8]] = &[",
    ]
    out += [f"    {_rs_str(e)}," for e in effects]
    out += [
        "];",
        "",
        "/// Animation groups: (name, ID, pre-Tribunal ID).",
        "pub static ANIM_GROUPS: &[(&[u8], i16, i16)] = &[",
    ]
    out += [f"    ({_rs_str(n)}, {a}, {b})," for n, a, b in anims]
    out += [
        "];",
        "",
        "/// MWEdit's extended (MWSE / MW-Enhanced) functions: (name, opcode, flags, arguments).",
        "pub static CUSTOM_FUNCTIONS: &[(&[u8], u16, u32, [u64; 13])] = &[",
    ]
    out += [
        f"    ({_rs_str(f.name)}, 0x{f.opcode:04X}, 0x{f.flags:X}, [{', '.join(f'0x{a:X}' for a in f.var)}]),"
        for f in custom
    ]
    from wraithguard.mwscript.opcodes import BY_NAME

    out += [
        "];",
        "",
        "/// Every function name the game, MWSE and MW-Enhanced know (`opcodes.py`'s `BY_NAME`,",
        "/// lower case, sorted): what the source checks (`check`) call a known function.",
        "pub static KNOWN_NAMES: &[&[u8]] = &[",
    ]
    out += [f"    {_rs_str(name)}," for name in sorted(BY_NAME)]
    out += ["];", ""]
    OUT_RS.parent.mkdir(parents=True, exist_ok=True)
    OUT_RS.write_text("\n".join(out), encoding="utf-8", newline="\n")


def _const(table: str) -> str:
    """``l_MainBlock`` -> ``MAIN_BLOCK``."""
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", table.removeprefix("l_")).upper()


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
