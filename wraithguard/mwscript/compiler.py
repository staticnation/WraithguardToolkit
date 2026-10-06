"""Compile Morrowind script source to bytecode, the way MWEdit does.

A port of MWEdit's compiler (``mwedit/script_compile.cc`` and
``script_compile_ex.cc``, MIT, Copyright 2025 Walrus Tech; originally Dave
Humphrey). It is table-driven: :data:`~wraithguard.mwscript.compiler_data.TABLES`
is the grammar, each row naming the token it expects and the routines that
check it (``Parse*``) and write its bytes (``Output*``). Those routines are
ported one for one, under their MWEdit names, so the two can be read side by
side; the aim is the same bytes MWEdit writes, which the tests check against
MWEdit's own test plugin.

MWEdit's quirks are kept where they change the output (the reference written
for a quoted ``"id"->Function`` is one), because a compiler that "fixes" them
writes scripts that differ from the ones already in people's plugins.

Not ported: MWEdit's custom (MWSE/MWE) function list, so ``FUNCTIONX`` is never
produced; the ``setx``/``ifx``/``whilex`` blocks are ported but untested.

The Rust backend has the same port (``native/src/mwscript``); :class:`ScriptCompiler`
uses it when it is built and this one when it is not. This module stays the reference
the Rust one is held to.

Usage::

    from wraithguard.mwscript.compiler import ScriptCompiler

    compiler = ScriptCompiler([esm, esp])     # the load order, read once
    result = compiler.compile(text)
    if result.ok:
        result.data, result.var_data, result.header()

Copyright (c) 2026 StaticNation.
"""

# The nested ifs follow MWEdit's, so the two read side by side.
# ruff: noqa: SIM102

from __future__ import annotations

import codecs
import struct
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final

from wraithguard.mwscript.compiler_data import (
    ANIM_GROUPS,
    CHAR_TYPES,
    EFFECT_IDS,
    FUNCTIONS,
    TABLES,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence
    from pathlib import Path

    from wraithguard.mwscript.records import Record, RecordIndex

# --- Constants (MWEdit's script_defs.h / script_compile.h) -------------------

#: Result codes.
OK: Final = 0
ERROR: Final = -1
BLOCKEND: Final = 2
TABLEEND: Final = 3
#: CheckFuncArg results.
_ARG_OK: Final = 0
_ARG_ERROR: Final = -1
_ARG_ENDTABLE: Final = -2

ONE, MANY, OPT, STOP = 1, 2, 4, 8

# Function flags.
F_SHORTVAR = 0x1
F_ALLOWGLOBAL = 0x2
F_EXTRASHORT = 0x4
F_NOOPTOUT = 0x8
F_BLOODMOON = 0x400
F_TRIBUNAL = 0x800
F_DIALOGUE = 0x1000
F_BAD = 0x2000
F_VAR = 0x4000
# Argument flags.
A_BYTE = 0x1
A_SHORT = 0x2
A_LONG = 0x4
A_FLOAT = 0x8
A_NUMBER = 0xF
A_STRING = 0x10
A_ID = 0x20
A_XYZ = 0x40
A_EFFECT = 0x80
A_RESET = 0x100
A_ANIM = 0x200
A_OPTSTART = 0x400
A_OPTIONAL = 0x800
A_NOTREQ = 0x1000
A_CELLSTR = 0x2000
A_SHORTSTR = 0x4000
A_MANY = 0x8000
A_VARMASK = 0x3FF
A_SCRIPTID = 0x10000
A_SOUNDID = 0x20000
A_RACEID = 0x40000
A_JOURNALID = 0x80000
A_FACTIONID = 0x100000
A_ITEMID = 0x200000
A_REGIONID = 0x400000
A_TOPICID = 0x800000
A_CELLID = 0x1000000
A_EFFECTID = 0x2000000
A_LEVELCID = 0x4000000
A_LEVELIID = 0x8000000
A_SOULGEMID = 0x10000000
A_CREATUREID = 0x20000000
A_SPELLID = 0x40000000
A_NPCID = 0x80000000
A_IDMASK = 0xFFFF0000
A_NOOUTPUT = 0x100000000
A_LOCALVAR = 0x10000  # ESMSCR_FUNC_LOCALVAR, the extended-argument stack tag

OPCODE_MESSAGEBOX: Final = 0x1000
MAX_FUNCARGS: Final = 13
MAX_MSGBUTTONS: Final = 9
MAX_MSGARGS: Final = 9
MAX_IFSTATEMENTS: Final = 255
MAX_IFEXPRESSIONS: Final = 255
DATA_SIZE: Final = 65535
MAX_LOCALVARS: Final = 255
VAR_MAXLENGTH: Final = 32
SPECIAL_LOCAL_INDEX: Final = 33
STACK_MAXTOKEN: Final = 63
EFFECT_MAX: Final = 142

# MWSE opcodes used by the extended blocks.
OP_JUMPSHORT = 0x380A
OP_JUMPSHORTZERO = 0x380C
OP_POP = 0x380F
OP_PUSH = 0x3811
OP_PUSHS = 0x3813
OP_GETLOCAL = 0x3C00
OP_SETLOCAL = 0x3C02
OP_SETREF = 0x3C18

CHAR_EOL = "\r"
CHAR_COMMENT = ";"
CHAR_STRING = '"'
CT_SPACE, CT_PUNCT, CT_DIGIT, CT_SYMBOLF, CT_SYMBOL = 1, 2, 4, 8, 16

# Token IDs.
T = {
    "BEGINBLOCK": -2,
    "ENDTABLE": -1,
    "TOKEN_UNKNOWN": 0,
    "TOKEN_INTEGER": 1,
    "TOKEN_FLOAT": 2,
    "TOKEN_STRING": 3,
    "TOKEN_SYMBOL": 4,
    "TOKEN_OPERATOR": 5,
    "TOKEN_EOL": 6,
    "TOKEN_EOS": 7,
    "TOKEN_BEGIN": 8,
    "TOKEN_END": 9,
    "TOKEN_TYPEOP": 10,
    "TOKEN_SET": 11,
    "TOKEN_TO": 12,
    "TOKEN_OPENBRAC": 13,
    "TOKEN_CLOSEBRAC": 14,
    "TOKEN_ADDOP": 15,
    "TOKEN_MULOP": 16,
    "TOKEN_RELOP": 17,
    "TOKEN_IF": 18,
    "TOKEN_ELSEIF": 19,
    "TOKEN_ELSE": 20,
    "TOKEN_ENDIF": 21,
    "TOKEN_WHILE": 22,
    "TOKEN_ENDWHILE": 23,
    "TOKEN_RETURN": 24,
    "TOKEN_GET": 25,
    "TOKEN_NUMBER": 26,
    "TOKEN_COMMA": 27,
    "TOKEN_FUNCOP": 28,
    "TOKEN_VAROP": 29,
    "TOKEN_XYZ": 30,
    "TOKEN_LOCALVAR": 31,
    "TOKEN_GLOBALVAR": 32,
    "TOKEN_FUNCTION": 33,
    "TOKEN_SYMBOLID": 34,
    "TOKEN_RESET": 35,
    "TOKEN_FUNCTIONX": 36,
    "TOKEN_SETX": 37,
    "TOKEN_IFX": 38,
    "TOKEN_WHILEX": 39,
}
TK_UNKNOWN = T["TOKEN_UNKNOWN"]
TK_STRING = T["TOKEN_STRING"]
TK_SYMBOL = T["TOKEN_SYMBOL"]
TK_EOL = T["TOKEN_EOL"]
TK_EOS = T["TOKEN_EOS"]
TK_OPENBRAC = T["TOKEN_OPENBRAC"]
TK_CLOSEBRAC = T["TOKEN_CLOSEBRAC"]
TK_ADDOP = T["TOKEN_ADDOP"]
TK_MULOP = T["TOKEN_MULOP"]
TK_RELOP = T["TOKEN_RELOP"]
TK_NUMBER = T["TOKEN_NUMBER"]
TK_COMMA = T["TOKEN_COMMA"]
TK_FUNCOP = T["TOKEN_FUNCOP"]
TK_VAROP = T["TOKEN_VAROP"]
TK_XYZ = T["TOKEN_XYZ"]
TK_LOCALVAR = T["TOKEN_LOCALVAR"]
TK_GLOBALVAR = T["TOKEN_GLOBALVAR"]
TK_FUNCTION = T["TOKEN_FUNCTION"]
TK_RESET = T["TOKEN_RESET"]

_TOKEN_NAMES: Final[dict[int, str]] = {
    -2: "Begin Block",
    -1: "End Table",
    0: "Unknown",
    1: "Integer",
    2: "Float",
    3: "String",
    4: "Symbol",
    5: "Operator",
    6: "End-of-Line",
    7: "End-of-File",
    8: "begin",
    9: "end",
    10: "Type",
    11: "set",
    12: "to",
    13: "(",
    14: ")",
    15: "+ or -",
    16: "* or /",
    17: "Compare Operator",
    18: "if",
    19: "elseif",
    20: "else",
    21: "endif",
    22: "while",
    23: "endwhile",
    24: "return",
    25: "get",
    26: "Number",
    27: ",",
    28: "->",
    29: ".",
    30: "X/Y/Z",
    31: "Local Variable",
    32: "Global Variable",
    33: "Function",
    34: "Object ID",
    35: "reset",
    36: "Unknown",
    37: "setx",
    38: "ifx",
    39: "whilex",
}

#: Reserved words (MWEdit's GetESMScriptResToken).
_RESERVED: Final[dict[str, int]] = {
    "begin": T["TOKEN_BEGIN"],
    "end": T["TOKEN_END"],
    "endwhile": T["TOKEN_ENDWHILE"],
    "else": T["TOKEN_ELSE"],
    "elseif": T["TOKEN_ELSEIF"],
    "endif": T["TOKEN_ENDIF"],
    "if": T["TOKEN_IF"],
    "ifx": T["TOKEN_IFX"],
    "long": T["TOKEN_TYPEOP"],
    "float": T["TOKEN_TYPEOP"],
    "return": T["TOKEN_RETURN"],
    "set": T["TOKEN_SET"],
    "short": T["TOKEN_TYPEOP"],
    "setx": T["TOKEN_SETX"],
    "to": T["TOKEN_TO"],
    "get": T["TOKEN_GET"],
    "while": T["TOKEN_WHILE"],
    "whilex": T["TOKEN_WHILEX"],
    "x": T["TOKEN_XYZ"],
    "y": T["TOKEN_XYZ"],
    "z": T["TOKEN_XYZ"],
}

# --- Message levels -----------------------------------------------------------

LEVEL_ERROR, LEVEL_NONE, LEVEL_WARNING = -1, 0, 1

_LEVEL_SETS: Final[dict[str, dict[str, int]]] = {
    "default": {
        "ERROR_BADID": LEVEL_ERROR,
        "WARNING_NOCOMMA": LEVEL_NONE,
        "WARNING_BADFUNCARG": LEVEL_WARNING,
        "WARNING_BADFUNCVAR": LEVEL_WARNING,
        "ERROR_BADLOCAL": LEVEL_ERROR,
        "ERROR_BADTOKEN": LEVEL_ERROR,
        "ERROR_BADNUMBER": LEVEL_ERROR,
        "ERROR_MULTREF": LEVEL_ERROR,
        "WARNING_NOSPACE": LEVEL_NONE,
        "WARNING_IDCONFLICT": LEVEL_WARNING,
        "WARNING_BADVARSTART": LEVEL_NONE,
        "WARNING_BADFUNCTION": LEVEL_WARNING,
        "ERROR_BADFUNCARG": LEVEL_ERROR,
        "WARNING_EXTFUNC": LEVEL_WARNING,
        "ERROR_TOOMANYARGS": LEVEL_ERROR,
        "WARNING_EXPANSIONFUNC": LEVEL_ERROR,
        "WARNING_LOCALVAR34": LEVEL_WARNING,
        "WARNING_DIALOGFUNC": LEVEL_NONE,
    },
    "weak": {
        "ERROR_BADID": LEVEL_WARNING,
        "WARNING_NOCOMMA": LEVEL_NONE,
        "WARNING_BADFUNCARG": LEVEL_NONE,
        "WARNING_BADFUNCVAR": LEVEL_NONE,
        "ERROR_BADLOCAL": LEVEL_ERROR,
        "ERROR_BADTOKEN": LEVEL_ERROR,
        "ERROR_BADNUMBER": LEVEL_WARNING,
        "ERROR_MULTREF": LEVEL_WARNING,
        "WARNING_NOSPACE": LEVEL_NONE,
        "WARNING_IDCONFLICT": LEVEL_NONE,
        "WARNING_BADVARSTART": LEVEL_NONE,
        "WARNING_BADFUNCTION": LEVEL_NONE,
        "ERROR_BADFUNCARG": LEVEL_WARNING,
        "WARNING_EXTFUNC": LEVEL_WARNING,
        "ERROR_TOOMANYARGS": LEVEL_ERROR,
        "WARNING_EXPANSIONFUNC": LEVEL_WARNING,
        "WARNING_LOCALVAR34": LEVEL_NONE,
        "WARNING_DIALOGFUNC": LEVEL_NONE,
    },
    "strong": {
        "ERROR_BADID": LEVEL_ERROR,
        "WARNING_NOCOMMA": LEVEL_NONE,
        "WARNING_BADFUNCARG": LEVEL_WARNING,
        "WARNING_BADFUNCVAR": LEVEL_WARNING,
        "ERROR_BADLOCAL": LEVEL_ERROR,
        "ERROR_BADTOKEN": LEVEL_ERROR,
        "ERROR_BADNUMBER": LEVEL_ERROR,
        "ERROR_MULTREF": LEVEL_ERROR,
        "WARNING_NOSPACE": LEVEL_WARNING,
        "WARNING_IDCONFLICT": LEVEL_WARNING,
        "WARNING_BADVARSTART": LEVEL_WARNING,
        "WARNING_BADFUNCTION": LEVEL_ERROR,
        "ERROR_BADFUNCARG": LEVEL_ERROR,
        "WARNING_EXTFUNC": LEVEL_ERROR,
        "ERROR_TOOMANYARGS": LEVEL_ERROR,
        "WARNING_EXPANSIONFUNC": LEVEL_ERROR,
        "WARNING_LOCALVAR34": LEVEL_WARNING,
        "WARNING_DIALOGFUNC": LEVEL_WARNING,
    },
}


# --- Function table -------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class FuncInfo:
    """One built-in function (MWEdit's ``esmscrfuncinfo_t``)."""

    name: str
    opcode: int
    flags: int
    ret: int
    var: tuple[int, ...]


_FUNCS: Final[dict[str, FuncInfo]] = {
    name.lower(): FuncInfo(name, op, flags, ret, args) for name, op, flags, ret, args in FUNCTIONS
}
_EFFECTS: Final[dict[str, int]] = {}
for _i, _eid in enumerate(EFFECT_IDS):
    _EFFECTS.setdefault(_eid.lower(), _i)
_ANIMS: Final[dict[str, tuple[int, int]]] = {}
for _name, _new, _old in ANIM_GROUPS:
    _ANIMS.setdefault(_name.lower(), (_new, _old))


def find_function(name: str) -> FuncInfo | None:
    """A built-in function by name (case-insensitive)."""
    return _FUNCS.get(name.lower())


# --- Custom (MWSE / MW-Enhanced) functions -------------------------------------------

F_MWSE = 0x8000
F_MWE = 0x10000

_FUNC_OPTIONS: Final[dict[str, int]] = {
    "none": 0,
    # MWEdit maps "ShortVar" to ALLOWGLOBAL (mw_custom_func.cc); kept, as it is what MWEdit
    # compiles with. Extended functions do not read either flag when written.
    "shortvar": F_ALLOWGLOBAL,
    "allowglobal": F_ALLOWGLOBAL,
    "extrashort": F_EXTRASHORT,
    "nooptout": F_NOOPTOUT,
    "bloodmoon": F_BLOODMOON,
    "tribunal": F_TRIBUNAL,
    "dialogue": F_DIALOGUE,
    "bad": F_BAD,
    "allowvar": F_VAR,
    "mwse": F_MWSE,
    "extended": F_MWSE,
    "mwe": F_MWE,
    "mwenhanced": F_MWE,
}

_ARG_OPTIONS: Final[dict[str, int]] = {
    "none": 0,
    "byte": 0x1,
    "short": 0x2,
    "long": 0x4,
    "ref": 0x4,
    "float": 0x8,
    "number": 0xF,
    "string": 0x10,
    "id": 0x20,
    "xyz": 0x40,
    "effect": 0x80,
    "reset": 0x100,
    "animation": 0x200,
    "optstart": 0x400,
    "optional": 0x800,
    "notrequired": 0x1000,
    "cellstring": 0x2000,
    "shortstring": 0x4000,
    "many": 0x8000,
    "scriptid": 0x10000,
    "soundid": 0x20000,
    "raceid": 0x40000,
    "journalid": 0x80000,
    "factionid": 0x100000,
    "itemid": 0x200000,
    "regionid": 0x400000,
    "topicid": 0x800000,
    "cellid": 0x1000000,
    "effectid": 0x2000000,
    "levelcreatureid": 0x4000000,
    "levelitemid": 0x8000000,
    "soulgemid": 0x10000000,
    "creatureid": 0x20000000,
    "spellid": 0x40000000,
    "npcid": 0x80000000,
}


def _var_value(line: str) -> tuple[str, str | None]:
    """MWEdit's SeperateVarValueQ with ``=`` and ``#``: (name, value or None).

    The first ``=`` splits name from value; ``#`` outside quotes ends the line; a quoted
    value is taken between its quotes.
    """
    sep = -1
    start, end, quoted = 0, len(line), False
    for i, ch in enumerate(line):
        if ch == "=" and sep < 0:
            sep, start = i, i + 1
        elif sep >= 0 and ch == '"':
            if quoted:
                end = i
                break
            quoted, start = True, i + 1
        elif not quoted and ch == "#":
            end = i
            break
    if sep < 0:
        return line[:end].strip(), None
    return line[:sep].strip(), line[start:end].strip()


def _flags(value: str, table: Mapping[str, int]) -> int:
    """A ``A | B`` flag string's bits, by the table's lower-case names."""
    out = 0
    for part in value.split("|"):
        out |= table.get(part.strip().lower(), 0)
    return out


def parse_custom_functions(text: str) -> dict[str, FuncInfo]:
    """MWEdit's ``customfunctions.dat`` (MWSE / MW-Enhanced functions), by lower-case name.

    The format: ``function`` ... ``end`` blocks of ``Name``, ``Options``, ``Return``,
    ``Param1``-``Param12`` and ``Opcode`` lines, ``#`` comments. Read as MWEdit reads it
    (``ReadMwCustomFunctions``); unknown words are ignored rather than refused.
    """
    out: dict[str, FuncInfo] = {}
    lines = iter(text.splitlines())
    for raw in lines:
        name, value = _var_value(raw)
        if value is not None or name.lower() != "function":
            continue
        fname, opcode, flags = "", 0, 0
        args = [0] * MAX_FUNCARGS
        for inner in lines:
            key, val = _var_value(inner)
            if val is None:
                if key.lower() == "end":
                    break
                continue
            k = key.lower()
            if k == "name":
                fname = val
            elif k == "opcode":
                try:
                    opcode = int(val, 0) & 0xFFFF
                except ValueError:
                    opcode = 0
            elif k == "options":
                flags = _flags(val, _FUNC_OPTIONS)
            elif k.startswith("param") and k[5:].isdigit() and 1 <= int(k[5:]) <= 12:
                args[int(k[5:]) - 1] = _flags(val, _ARG_OPTIONS)
        if fname:
            out[fname.lower()] = FuncInfo(fname, opcode, flags, 0, tuple(args))
    return out


def _default_custom() -> dict[str, FuncInfo]:
    """MWEdit's own extended functions (MWSE / MW-Enhanced), by lower-case name."""
    from wraithguard.mwscript.compiler_data import CUSTOM_FUNCTIONS

    return {
        name.lower(): FuncInfo(name, op, flags, 0, args)
        for name, op, flags, args in CUSTOM_FUNCTIONS
    }


# --- C library behaviour the output depends on -----------------------------------


def _c_isspace(ch: str) -> bool:
    """C's ``isspace`` in the C locale, as MWEdit's lexer tests it."""
    return ch in " \t\n\v\f\r"


def _c_ispunct(ch: str) -> bool:
    """C's ``ispunct`` in the C locale, as MWEdit's lexer tests it."""
    o = ord(ch)
    return 33 <= o <= 47 or 58 <= o <= 64 or 91 <= o <= 96 or 123 <= o <= 126


def _c_isdigit(ch: str) -> bool:
    """C's ``isdigit``: an ASCII digit."""
    return "0" <= ch <= "9"


def _atof(text: str) -> float:
    """C ``atof``: the longest numeric prefix, 0.0 if none."""
    i, n = 0, len(text)
    while i < n and _c_isspace(text[i]):
        i += 1
    start = i
    if i < n and text[i] in "+-":
        i += 1
    digits = i
    while i < n and _c_isdigit(text[i]):
        i += 1
    if i < n and text[i] == ".":
        i += 1
        while i < n and _c_isdigit(text[i]):
            i += 1
    if i == digits or text[digits:i] == ".":
        return 0.0
    if i < n and text[i] in "eE":
        j = i + 1
        if j < n and text[j] in "+-":
            j += 1
        if j < n and _c_isdigit(text[j]):
            while j < n and _c_isdigit(text[j]):
                j += 1
            i = j
    return float(text[start:i])


def _atol(text: str) -> int:
    """C ``atol``/``atoi`` (32-bit, saturating like MSVC's)."""
    i, n = 0, len(text)
    while i < n and _c_isspace(text[i]):
        i += 1
    sign = 1
    if i < n and text[i] in "+-":
        sign = -1 if text[i] == "-" else 1
        i += 1
    value = 0
    while i < n and _c_isdigit(text[i]):
        value = value * 10 + ord(text[i]) - 48
        i += 1
    value *= sign
    return max(-0x80000000, min(0x7FFFFFFF, value))


def _wrap(value: int, bits: int) -> int:
    """Truncate to a signed ``bits``-bit C integer."""
    mask = (1 << bits) - 1
    value &= mask
    return value - (1 << bits) if value >> (bits - 1) else value


def _is_string_float(text: str) -> bool:
    """MWEdit's IsStringFloat: digits and whitespace with at most one '.'."""
    seen_dot = False
    for ch in text:
        if ch == "." and not seen_dot:
            seen_dot = True
            continue
        if not (_c_isspace(ch) or _c_isdigit(ch)):
            return False
    return True


def _unquote(text: str) -> str:
    """MWEdit's UnquoteString: the text between the first two quotes, if any."""
    first = text.find('"')
    if first < 0:
        return text
    second = text.find('"', first + 1)
    return text[first + 1 : second] if second >= 0 else text[first + 1 :]


def _bytes(text: str) -> bytes:
    """Text as the bytes a plugin holds (one byte a character)."""
    return text.encode("latin-1", "replace")


# --- Results ----------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class Message:
    """A compiler error or warning.

    Attributes:
        line: Source line, from 1.
        column: Character within the line, from 1.
        level: ``error`` or ``warning``.
        text: The message.
    """

    line: int
    column: int
    level: str
    text: str


@dataclass(slots=True)
class CompileResult:
    """What a compile produced.

    Attributes:
        ok: The compile succeeded (MWEdit's ``Compile() >= 0``).
        name: The script's name, from its ``begin`` line.
        data: The bytecode (``SCDT``).
        shorts: Local shorts, in declaration order (likewise ``longs``, ``floats``).
        messages: Errors and warnings, in the order found.
    """

    ok: bool
    name: str
    data: bytes
    shorts: list[str] = field(default_factory=list)
    longs: list[str] = field(default_factory=list)
    floats: list[str] = field(default_factory=list)
    messages: list[Message] = field(default_factory=list)

    @property
    def var_data(self) -> bytes:
        """The ``SCVR`` block: every local's name, NUL-terminated, shorts first."""
        return b"".join(_bytes(n) + b"\x00" for n in (*self.shorts, *self.longs, *self.floats))

    def header(self) -> bytes:
        """The 52-byte ``SCHD``: name, the three local counts and the two sizes."""
        name = _bytes(self.name)[:31].ljust(32, b"\x00")
        return name + struct.pack(
            "<5I",
            len(self.shorts),
            len(self.longs),
            len(self.floats),
            len(self.data),
            len(self.var_data),
        )

    @property
    def errors(self) -> list[Message]:
        """The errors only."""
        return [m for m in self.messages if m.level == "error"]


@dataclass(slots=True)
class _Stack:
    """An expression stack entry (``esmscrstack_t``)."""

    token_id: int
    token: str


@dataclass(slots=True)
class _IfBlock:
    """An if/while block's statement-count slot (``esmscrifblock_t``)."""

    pos: int
    start_count: int


# --- The compiler ---------------------------------------------------------------------


class Compiler:
    """One compile of one script (MWEdit's ``CEsmScriptCompile``).

    Args:
        records: The load order's records, for globals, object IDs and other
            scripts' locals.
        levels: ``default``, ``weak`` or ``strong``: how strict MWEdit's
            message levels are.
        tribunal: Allow Tribunal functions (and the Tribunal animation IDs).
        bloodmoon: Allow Bloodmoon functions.
        custom: The extended (MWSE / MW-Enhanced) functions, from
            :func:`parse_custom_functions`; None for the set MWEdit ships.
        extended: Allow extended functions without a warning.
    """

    def __init__(
        self,
        records: RecordIndex,
        *,
        levels: str = "default",
        tribunal: bool = True,
        bloodmoon: bool = True,
        custom: Mapping[str, FuncInfo] | None = None,
        extended: bool = True,
    ) -> None:
        """Set up an empty compile."""
        self.records = records
        self.levels = _LEVEL_SETS[levels]
        self.allow_tribunal = tribunal
        self.allow_bloodmoon = bloodmoon
        self.custom = custom if custom is not None else _default_custom()
        self.allow_extended = extended

        self.text = ""
        self.pos = 0
        self.token = ""
        self.last_token = ""
        self.token_id = TK_UNKNOWN
        self.line = 0
        self.char_pos = 0
        self.token_parsed = True
        self.line_has_ref = False
        self.line_ref_token = ""
        self.last_token_negative = False
        self.last_set_negative = False

        self.cur_func: FuncInfo | None = None
        self.num_func_args = 0
        self.last_func_arg = 0
        self.func_arg_index = 0

        self.messages: list[Message] = []
        self.num_errors = 0
        self.num_warnings = 0

        self.shorts: list[str] = []
        self.longs: list[str] = []
        self.floats: list[str] = []

        self.ref_object = ""
        self.arg_x_stack: list[_Stack] = []
        self.let_queue: list[_Stack] = []
        self.x_if_stack: list[_IfBlock] = []

        self.expr_stack: list[_Stack] = []
        self.num_output_expr = 0

        self.buf = bytearray(DATA_SIZE)
        self.size = 0
        self.last_line_pos = 0
        self.last_if_pos = -1
        self.last_set_pos = 0
        self.num_set_expr = 0
        self.is_empty_if = False
        self.last_set_symbol = False
        self.output_func_id_ref = False
        self.last_msgbox_but = -1
        self.last_msgbox_args = -1
        self.num_msg_buttons = 0
        self.num_msg_args = 0
        self.many_arg_pos = 0
        self.last_func_arg_symbol = False
        self.func_opt_count = 0
        self.func_opt_pos = 0
        self.statement_count = 0
        self.if_stack: list[_IfBlock] = []

        self.name = ""
        self._tables: dict[str, tuple[tuple[int, int, str | None, str | None, str | None], ...]] = {
            name: tuple((T[tok], flags, sub, parse, out) for tok, flags, sub, parse, out in rows)
            for name, rows in TABLES.items()
        }

    def _need_func(self) -> FuncInfo:
        """The function being compiled (the grammar only calls here inside a call)."""
        if self.cur_func is None:
            raise RuntimeError("no current function")
        return self.cur_func

    # --- Entry ---------------------------------------------------------------------

    def compile(self, text: str) -> CompileResult:
        r"""Compile ``text`` (line ends ``\r\n``, ``\n`` or ``\r``)."""
        text = text.replace("\r\n", "\n").replace("\r", "\n").replace("\n", "\r\n")
        self.text = text.split("\x00", 1)[0]
        self.pos = 0
        self.token_parsed = True
        result = self.parse_table("l_MainBlock")
        return CompileResult(
            ok=result >= 0,
            name=self.name,
            data=bytes(self.buf[: self.size]),
            shorts=list(self.shorts),
            longs=list(self.longs),
            floats=list(self.floats),
            messages=list(self.messages),
        )

    # --- Buffer ----------------------------------------------------------------------

    def ch(self, offset: int = 0) -> str:
        """The character at the parse position plus ``offset`` (NUL past the end)."""
        i = self.pos + offset
        return self.text[i] if 0 <= i < len(self.text) else "\x00"

    def char_type(self, ch: str) -> int:
        """The character's class bits."""
        o = ord(ch)
        return CHAR_TYPES[o] if o < 256 else 0

    def add(self, data: bytes) -> bool:
        """AddScriptData."""
        if self.size + len(data) >= DATA_SIZE:
            self.add_error(f"Maximum compiled script size {DATA_SIZE} exceeded!")
            return False
        self.buf[self.size : self.size + len(data)] = data
        if self.last_line_pos == self.size:
            self.last_line_pos += len(data)
        self.size += len(data)
        return True

    def add_short(self, value: int) -> bool:
        """Two little-endian bytes."""
        return self.add(struct.pack("<h", _wrap(value, 16)))

    def output_token(self, value: int) -> int:
        """OutputToken."""
        self.add_short(value)
        return 0

    def insert_script_data_ref(self, ident: str) -> bool:
        """InsertScriptDataRef, MWEdit's arithmetic included.

        For a quoted ID MWEdit shifts the line by the quoted length but writes
        the unquoted one, so two stale bytes stay between the reference and the
        line and the line's last two bytes fall off its end. Kept as is.
        """
        data = _bytes(ident)
        size = len(data)
        if self.size + size >= DATA_SIZE:
            self.add_error(f"Maximum compiled script size {DATA_SIZE} exceeded!")
            return False
        start = self.last_line_pos
        if self.size >= start:
            moved = bytes(self.buf[start : self.size])
            self.buf[start + size + 3 : start + size + 3 + len(moved)] = moved
        self.buf[start : start + 2] = struct.pack("<h", 0x010C)
        if data[:1] == b'"':
            size -= 2
            n = size & 0xFF
            self.buf[start + 2] = n
            self.buf[start + 3 : start + 3 + n] = data[1 : 1 + n]
        else:
            n = size & 0xFF
            self.buf[start + 2] = n
            self.buf[start + 3 : start + 3 + n] = data[:n]
        if self.last_set_pos >= start:
            self.last_set_pos += size + 3
        if self.last_if_pos >= start:
            self.last_if_pos += size + 3
        self.size += size + 3
        return True

    def add_global(self, prefix: bytes) -> None:
        """A global's name: ``prefix``, a length byte and the name, padded to 4."""
        name = _bytes(self.token)
        self.add(prefix)
        if len(name) < 4:
            self.add(bytes([4]))
            self.add(name)
            self.add(b"\x00" * (4 - len(name)))
        else:
            self.add(bytes([len(name) & 0xFF]))
            self.add(name)

    # --- Messages ---------------------------------------------------------------------

    def _record(self, level: str, text: str) -> None:
        """Keep a message at the current line and column."""
        self.messages.append(Message(self.line + 1, self.char_pos + 1, level, text))

    def add_error(self, text: str) -> None:
        """AddError."""
        self._record("error", text)
        self.num_errors += 1

    def add_message(self, msg_id: str, text: str) -> bool:
        """AddMessage: False when the message is an error that stops the compile."""
        level = self.levels.get(msg_id, LEVEL_WARNING)
        if level == LEVEL_NONE:
            return True
        if level == LEVEL_WARNING:
            self._record("warning", text)
            self.num_warnings += 1
            return True
        self.add_error(text)
        return False

    def assert_token(self, token: int) -> int:
        """AssertToken."""
        if self.token_id != token:
            self.add_message(
                "ERROR_BADTOKEN",
                f"Syntax Error: Expected '{_TOKEN_NAMES.get(token, 'Unknown')}' but found "
                f"'{_TOKEN_NAMES.get(self.token_id, 'Unknown')}' ({self.token})!",
            )
            return ERROR
        return OK

    # --- Records ----------------------------------------------------------------------

    def find_record(self, rec_id: str) -> Record | None:
        """FindRecord."""
        return self.records.find(rec_id)

    def get_global(self, name: str) -> Record | None:
        """GetGlobal."""
        return self.records.get_global(name)

    def is_symbol_id(self, token: str) -> tuple[bool, str]:
        """IsSymbolID: whether an object has this ID, and the ID in the record's case."""
        name = token
        if token[:1] == '"':
            name = token[1:128]
            if name.endswith('"'):
                name = name[:-1]
        rec = self.find_record(name)
        if rec is not None:
            return True, rec.id
        return False, token

    def check_func_id1(self, flags: int, rec_id: str, optional: bool) -> bool:
        """CheckFuncID1: whether an ID argument names a record of a type it accepts."""
        name = rec_id
        if name[:1] == '"':
            name = name[1:256]
            if name.endswith('"'):
                name = name[:-1]
        else:
            name = name[:255]
        r = self.records
        checks: tuple[tuple[int, str], ...] = (
            (A_EFFECTID, "MGEF"),
            (A_LEVELCID, "LEVC"),
            (A_LEVELIID, "LEVI"),
            (A_CREATUREID, "CREA"),
            (A_NPCID, "NPC_"),
            (A_SPELLID, "SPEL"),
            (A_TOPICID, "DIAL"),
            (A_REGIONID, "REGN"),
        )
        for bit, rec_type in checks:
            if flags & bit and r.find_type(name, rec_type) is not None:
                return True
        if flags & A_ITEMID and r.find_carryable(name) is not None:
            return True
        for bit, rec_type in (
            (A_FACTIONID, "FACT"),
            (A_RACEID, "RACE"),
            (A_SOUNDID, "SOUN"),
            (A_SCRIPTID, "SCPT"),
        ):
            if flags & bit and r.find_type(name, rec_type) is not None:
                return True
        if flags & A_CELLID:
            if flags & A_CELLSTR:
                return True
            if r.find_type(name, "CELL") is not None:
                return True
        if flags & A_JOURNALID:
            dial = r.find_type(name, "DIAL")
            if dial is not None and dial.dial_type == 4:
                return True
        if flags & A_SOULGEMID:
            if r.find_type(name, "MISC") is not None and name[:13].lower() == "misc_soulgem_":
                return True
        rec = self.find_record(name)
        if flags & A_IDMASK == 0 and rec is not None:
            return True
        if rec is None and not optional:
            if not self.add_message("ERROR_BADID", f"The object ID '{name}' is not valid!"):
                return False
        if not optional and not self.add_message(
            "ERROR_BADID", f"ID '{name}' is not a {_id_type_name(flags)} type!"
        ):
            return False
        return False

    def get_func_arg_ref_type(self, flags: int) -> bytes:
        """GetFuncArgRefType: the two bytes naming an ID argument's kind."""
        if self.cur_func is not None and self.cur_func.opcode == 0x114E:
            return b" r"
        for bit, code in (
            (A_CELLID, b" c"),
            (A_FACTIONID, b" t"),
            (A_JOURNALID, b" d"),
            (A_RACEID, b" a"),
            (A_SOUNDID, b" b"),
            (A_ITEMID, b" o"),
            (A_SPELLID, b" o"),
            (A_CREATUREID, b" o"),
            (A_NPCID, b" o"),
            (A_SCRIPTID, b" m"),
        ):
            if flags & bit:
                return code
        return b" r"

    def get_anim_group_id(self, name: str) -> int:
        """GetAnimGroupID."""
        found = _ANIMS.get(name.lower())
        if found is None:
            return -1
        return found[0] if self.allow_tribunal else found[1]

    # --- Locals -------------------------------------------------------------------------

    def find_local_var(self, name: str) -> bool:
        """FindLocalVar."""
        low = name.lower()
        return any(v.lower() == low for v in (*self.shorts, *self.longs, *self.floats))

    def find_local_var_index(self, name: str) -> tuple[int, str]:
        """FindLocalVarIndex: (1-based index, type char) or (-1, '')."""
        low = name.lower()
        for kind, names in (("s", self.shorts), ("l", self.longs), ("f", self.floats)):
            for index, var in enumerate(names):
                if var.lower() == low:
                    if index == SPECIAL_LOCAL_INDEX:
                        word = {"s": "short", "l": "long", "f": "float"}[kind]
                        self.add_message(
                            "WARNING_LOCALVAR34",
                            f"Use of 34th local {word} variable '{name}' may result in errors "
                            "and is not recommended!",
                        )
                    return index + 1, kind
        return -1, ""

    def add_local_var(self, name: str, kind: str) -> bool:
        """AddLocalVar."""
        if len(name) > VAR_MAXLENGTH:
            self.add_message(
                "ERROR_BADLOCAL",
                f"Local variable name exceeds the maximum length of {VAR_MAXLENGTH} ({name})!",
            )
            return False
        names = {"s": self.shorts, "l": self.longs, "f": self.floats}[kind]
        if len(names) >= MAX_LOCALVARS:
            word = {"s": "short", "l": "long", "f": "float"}[kind]
            self.add_message(
                "ERROR_BADLOCAL", f"Exceeded the maximum number of local {word} variables (255)!"
            )
            return False
        names.append(name)
        return True

    # --- Tokenizer ----------------------------------------------------------------------

    def get_next_token(self) -> int:
        """GetNextToken."""
        self.last_token = self.token
        if self.char_type(self.ch()) & CT_SPACE:
            self.skip_white_space()
        if self.ch() == CHAR_COMMENT:
            self.skip_comment()
        self.token_parsed = False
        c = self.ch()
        if c == CHAR_EOL:
            self.token_id = TK_EOL
            self.token = ""
            self.char_pos = 0
            self.line += 1
            self.pos += 1
            self.line_has_ref = False
            self.last_line_pos = self.size
        elif c == "\x00":
            self.token_id = TK_EOS
            self.token = ""
        elif c == CHAR_STRING:
            return self.get_string_token()
        elif self.char_type(c) & CT_SYMBOLF:
            return self.get_symbol_token()
        elif self.char_type(c) & CT_DIGIT:
            return self.get_number_token()
        elif self.char_type(c) & CT_PUNCT:
            return self.get_operator_token()
        else:
            self.add_message("ERROR_BADTOKEN", f"Unknown token type character '{c}' found!")
            return ERROR
        return OK

    def skip_white_space(self) -> None:
        """SkipTokenWhiteSpace."""
        while True:
            self.pos += 1
            self.char_pos += 1
            if not self.char_type(self.ch()) & CT_SPACE:
                break

    def skip_comment(self) -> None:
        """SkipTokenComment."""
        while True:
            self.pos += 1
            self.char_pos += 1
            if self.ch() in (CHAR_EOL, "\x00"):
                break

    def get_number_token(self) -> int:
        """GetNumberToken."""
        start = self.pos

        def advance() -> None:
            """Step past the current character, keeping the line and column."""
            while True:
                self.pos += 1
                self.char_pos += 1
                c = self.ch()
                if c == "\x00" or _c_isspace(c) or _c_ispunct(c):
                    break

        advance()
        if self.ch() == ".":
            advance()
        self.token_id = TK_NUMBER
        self.token = self.text[start : self.pos]
        if not _is_string_float(self.token) and not self.add_message(
            "ERROR_BADNUMBER", f"Invalid number string '{self.token}' found!"
        ):
            return ERROR
        return OK

    def get_operator_token(self) -> int:
        """GetOperatorToken."""
        two = self.ch() + self.ch(1)
        self.token_id = _op_token(two) if self.ch(1) != "\x00" else TK_UNKNOWN
        if self.token_id != TK_UNKNOWN:
            self.token = two
            self.pos += 2
            self.char_pos += 2
        else:
            self.token_id = _op_token(self.ch())
            self.token = self.ch()
            self.pos += 1
            self.char_pos += 1
        if self.token_id in (TK_VAROP, TK_FUNCOP):
            if self.line_has_ref:
                self.add_message("ERROR_MULTREF", "Multiple object references found on line!")
                return ERROR
            self.line_has_ref = True
            self.line_ref_token = self.last_token
        elif self.token_id in (TK_OPENBRAC, TK_CLOSEBRAC):
            if not _c_isspace(self.ch(-2)) or not _c_isspace(self.ch()):
                self.add_message(
                    "WARNING_NOSPACE",
                    "Open/closing bracket missing space characters on one/both sides!",
                )
        elif self.token_id == TK_RELOP:
            back = -2 if len(self.token) == 1 else -3
            if not _c_isspace(self.ch(back)) or not _c_isspace(self.ch()):
                if not self.add_message(
                    "WARNING_NOSPACE",
                    "Comparison operator missing space characters on one/both sides!",
                ):
                    return ERROR
        return OK

    def get_string_token(self) -> int:
        """GetStringToken."""
        start = self.pos
        start_pos = self.char_pos
        while True:
            self.pos += 1
            self.char_pos += 1
            if self.ch() in ('"', "\x00", CHAR_EOL):
                break
        if self.ch() != '"':
            self.add_message(
                "ERROR_BADTOKEN",
                f'Bad String, no terminating " found on line for string starting at position {start_pos}!',
            )
            return ERROR
        self.pos += 1
        self.char_pos += 1
        self.token = self.text[start : self.pos]
        self.token_id = TK_STRING
        return OK

    def get_symbol_token(self) -> int:
        """GetSymbolToken."""
        start = self.pos
        while True:
            self.pos += 1
            self.char_pos += 1
            if not self.char_type(self.ch()) & CT_SYMBOL:
                break
        self.token = self.text[start : self.pos]
        self.token_id = TK_SYMBOL
        reserved = _RESERVED.get(self.token.lower())
        if reserved is not None:
            self.token_id = reserved
            return OK
        if self.find_local_var(self.token):
            self.token_id = TK_LOCALVAR
            return OK
        glob = self.get_global(self.token)
        if glob is not None:
            self.token = glob.id
            self.token_id = TK_GLOBALVAR
            return OK
        func = find_function(self.token)
        if func is not None:
            if self.cur_func is not None:
                self.add_message(
                    "ERROR_BADTOKEN",
                    f"Cannot call the function '{self.token}' from within another function!",
                )
                return ERROR
            self.cur_func = func
            self.num_func_args = 0
            self.last_func_arg = 0
            self.func_arg_index = 0
            self.num_msg_buttons = 0
            self.num_msg_args = 0
            self.many_arg_pos = 0
            self.token_id = TK_FUNCTION
            if func.flags & F_BLOODMOON and not self.allow_bloodmoon:
                if not self.add_message(
                    "WARNING_EXPANSIONFUNC",
                    f"The Bloodmoon function '{self.token}' may not be supported!",
                ):
                    return ERROR
            if func.flags & F_TRIBUNAL and not self.allow_tribunal:
                if not self.add_message(
                    "WARNING_EXPANSIONFUNC",
                    f"The Tribunal function '{self.token}' may not be supported!",
                ):
                    return ERROR
            return OK
        func = self.custom.get(self.token.lower())
        if func is not None:
            self.cur_func = func
            self.num_func_args = 0
            self.last_func_arg = 0
            self.func_arg_index = 0
            self.num_msg_buttons = 0
            self.num_msg_args = 0
            self.many_arg_pos = 0
            self.token_id = T["TOKEN_FUNCTIONX"]
            if not self.allow_extended and not self.add_message(
                "WARNING_EXTFUNC", f"Extended function '{self.token}' may not be supported!"
            ):
                return ERROR
        return OK

    # --- Table engine ---------------------------------------------------------------------

    def _call(self, name: str | None) -> int:
        """Run the named action (none: OK), returning its result."""
        if name is None:
            return OK
        method: Callable[[], int] = getattr(self, _method_name(name))
        return method()

    def parse_table(self, name: str) -> int:
        """ParseTable."""
        table = self._tables[name]
        index = 0
        while table[index][0] != T["ENDTABLE"]:
            entry = table[index]
            if entry[1] & MANY:
                result = self.parse_table_many(entry)
                if result != OK:
                    return result
            elif entry[1] & ONE:
                result = self.parse_table_one(entry)
                if result in (TABLEEND, BLOCKEND):
                    return OK
                if result != OK:
                    return result
            index += 1
        end = table[index]
        if end[4] is not None:
            self._call(end[4])
        if end[1] & STOP:
            if end[3] is not None:
                result = self._call(end[3])
                if result != OK:
                    return result
            if end[1] & OPT:
                return BLOCKEND
            self.add_message(
                "ERROR_BADTOKEN",
                f"Unknown token '{self.token}'({_TOKEN_NAMES.get(self.token_id, 'Unknown')}) found!",
            )
            return ERROR
        return OK

    def parse_table_many(self, entry: tuple[int, int, str | None, str | None, str | None]) -> int:
        """ParseTableMany."""
        result = OK
        while result == OK:
            result = self.parse_table_one(entry)
        return OK if result == BLOCKEND else result

    def parse_table_one(self, entry: tuple[int, int, str | None, str | None, str | None]) -> int:
        """ParseTableOne."""
        token_id, flags, sub, parse, output = entry
        result = OK
        do_output = True
        if self.token_parsed:
            result = self.get_next_token()
            if result != OK:
                return result
        if token_id == T["BEGINBLOCK"]:
            if parse is not None:
                result = self._call(parse)
            if sub is not None and result == OK:
                result = self.parse_table(sub)
            if result == OK and flags & STOP:
                result = TABLEEND
        elif token_id == self.token_id:
            self.token_parsed = True
            if output is not None:
                self._call(output)
            if parse is not None:
                result = self._call(parse)
            if sub is not None and result == OK:
                result = self.parse_table(sub)
            if result == OK and flags & STOP:
                result = TABLEEND
            do_output = False
        elif flags & MANY:
            result = BLOCKEND
        elif flags & OPT:
            do_output = False
            result = OK
        else:
            result = self.assert_token(token_id)
            if result == OK:
                do_output = False
        if do_output and result == OK and output is not None:
            self._call(output)
        return result

    # --- Parse routines ---------------------------------------------------------------------

    def ParseNewLocalVar(self) -> int:  # noqa: N802 - MWEdit's names, for side-by-side reading
        """Declare a local: ``short``/``long``/``float`` and a name."""
        kind = {"short": "s", "long": "l", "float": "f"}.get(self.token.lower())
        if kind is None:
            self.add_message("ERROR_BADTOKEN", f"Unknown type operator '{self.token}'!")
            return ERROR
        result = self.get_next_token()
        if result != OK:
            return result
        if self.token_id == TK_SYMBOL:
            pass
        elif self.token_id == TK_GLOBALVAR:
            if not self.add_message(
                "WARNING_IDCONFLICT",
                f"Local variable '{self.token}' conflicts with a global variable.",
            ):
                return ERROR
        elif self.token_id == TK_LOCALVAR:
            self.add_message(
                "ERROR_BADLOCAL", f"Local variable '{self.token}' has already been defined!"
            )
            return ERROR
        else:
            self.add_message("ERROR_BADLOCAL", f"Invalid local variable name '{self.token}'!")
            return ERROR
        if self.token[:1] == "_" and not self.add_message(
            "WARNING_BADVARSTART",
            f"Local/Global variables should not start with an '_' ({self.token}).",
        ):
            return ERROR
        if not self.add_local_var(self.token, kind):
            return ERROR
        result = self.get_next_token()
        if result != OK:
            return result
        result = self.assert_token(TK_EOL)
        if result != OK:
            return result
        self.token_parsed = True
        self.statement_count += 1
        return OK

    def ParseFuncRef(self) -> int:  # noqa: N802
        """``id->Function`` on the left of an expression."""
        self.token_parsed = True
        return self.parse_table("l_LeftFuncBlock")

    def ParseVarRef(self) -> int:  # noqa: N802
        """``id.variable`` as a set target."""
        return self.parse_var_ref1(in_if=False)

    def ParseIfVarRef(self) -> int:  # noqa: N802
        """``id.variable`` in an if/while expression."""
        return self.parse_var_ref1(in_if=True)

    def parse_var_ref1(self, *, in_if: bool) -> int:
        """ParseVarRef1."""
        var_token = self.last_token
        if var_token[:1] == '"':
            var_token = var_token[1:-1]
        result = self.get_next_token()
        if result != OK:
            return result
        if self.token_id not in (TK_SYMBOL, TK_LOCALVAR, TK_GLOBALVAR, TK_FUNCTION):
            self.add_message(
                "ERROR_BADTOKEN",
                f"Expecting an object variable name but found '{_TOKEN_NAMES.get(self.token_id, 'Unknown')}'!",
            )
            return ERROR
        rec = self.find_record(var_token)
        if rec is None:
            self.add_message("ERROR_BADID", f"Unknown object id '{var_token}' found!")
            return ERROR
        if in_if:
            if self.last_set_negative:
                self.add(b" -1")
            self.add(b" ")
        if rec.type != "SCPT":
            if not rec.script:
                self.add_message("ERROR_BADID", f"Object '{var_token}' has no script assigned!")
                return ERROR
            script = self.find_record(rec.script)
            if script is None:
                self.add_message("ERROR_BADID", f"Invalid script name '{rec.script}' found!")
                return ERROR
            rec = script
            self.add(b"r")
        else:
            self.add(b"m")
        name = _bytes(var_token)
        length = len(name) & 0xFF
        if name[:1] == b'"':
            length = (length - 2) & 0xFF
            self.add(bytes([length]))
            self.add(name[1 : 1 + length])
        else:
            self.add(bytes([length]))
            self.add(name[:length])
        index, kind = rec.find_local(self.token) if rec.type == "SCPT" else (-1, "")
        if index <= 0:
            self.add_message(
                "ERROR_BADID", f"Object '{var_token}' has no local variable '{self.token}'!"
            )
            return ERROR
        self.add(_bytes(kind))
        self.add_short(index)
        if in_if:
            if self.last_set_negative:
                self.add(b" *")
                self.last_set_negative = False
            self.last_set_pos = self.size - 1
        else:
            self.add(b"\x00")
            self.last_set_pos = self.size - 1
        self.token_parsed = True
        return OK

    def ParseStringID(self) -> int:  # noqa: N802
        """A quoted object ID."""
        found, _ = self.is_symbol_id(self.token[1:-1])
        self.token_parsed = True
        if not found and not self.add_message(
            "ERROR_BADID", f"Unknown object ID {self.token} found!"
        ):
            return ERROR
        return OK

    def ParseLineStringID(self) -> int:  # noqa: N802
        """A quoted object ID opening a line."""
        self.statement_count += 1
        return self.ParseStringID()

    def _string_global(self, then: Callable[[], int]) -> int | None:
        """A quoted global (``"name"``) handed to ``then``, or None when it names none."""
        inner = self.token[1:-1]
        glob = self.get_global(inner)
        if glob is None:
            return None
        self.token = inner
        self.token_id = TK_GLOBALVAR
        then()
        return TABLEEND

    def ParseStringIDSet(self) -> int:  # noqa: N802
        """A quoted name as a set target: a global, else an object ID."""
        hit = self._string_global(self.OutputSetGlobal)
        return hit if hit is not None else self.ParseStringID()

    def ParseStringIDIf(self) -> int:  # noqa: N802
        """A quoted name opening an if expression."""
        hit = self._string_global(self.OutputIfGlobal)
        return hit if hit is not None else self.ParseStringID()

    def ParseStringIDPush(self) -> int:  # noqa: N802
        """A quoted name inside an expression."""
        hit = self._string_global(self.ParsePushToken)
        return hit if hit is not None else self.ParseStringID()

    def ParsePushStringID(self) -> int:  # noqa: N802
        """ParsePushStringID."""
        result = self.ParseStringID()
        if result < OK:
            return result
        self.add(b" ")
        return result

    def ParseSymbolID(self) -> int:  # noqa: N802
        """An object ID (its case taken from the record)."""
        found, token = self.is_symbol_id(self.token)
        self.token = token
        if not found and not self.add_message(
            "ERROR_BADID", f"Unknown object ID '{self.token}' found!"
        ):
            return ERROR
        self.token_parsed = True
        return OK

    def ParsePushSymbolID(self) -> int:  # noqa: N802
        """ParsePushSymbolID."""
        result = self.ParseSymbolID()
        if result < OK:
            return result
        self.add(b" ")
        return result

    def ParseLineSymbolID(self) -> int:  # noqa: N802
        """An object ID opening a line."""
        self.statement_count += 1
        return self.ParseSymbolID()

    def ParseFunctionLine(self) -> int:  # noqa: N802
        """An extended function opening a line."""
        self.statement_count += 1
        return OK

    def check_func_arg(self) -> int:
        """CheckFuncArg: whether the current token fits the current argument."""
        func = self.cur_func
        if func is None or self.func_arg_index >= MAX_FUNCARGS:
            self.add_message(
                "ERROR_TOOMANYARGS", f"Exceeded the maximum of {MAX_FUNCARGS} function arguments!"
            )
            return _ARG_ERROR
        flags = func.var[self.func_arg_index]
        no_message = bool(flags & A_MANY)
        optional = bool(flags & A_OPTIONAL)

        def refuse(msg_id: str, text: str) -> int | None:
            """Report a bad argument (unless quiet), or end an optional one."""
            if no_message:
                return _ARG_ERROR
            if optional:
                return _ARG_ENDTABLE
            if not self.add_message(msg_id, text):
                return _ARG_ERROR
            return None

        if flags & A_VARMASK == 0:
            if not self.add_message(
                "ERROR_BADFUNCARG", f"Function does not accept argument #{self.num_func_args}!"
            ):
                return _ARG_ERROR
            return _ARG_OK
        if flags & A_OPTSTART:
            self.func_opt_pos = self.size
            self.func_opt_count = self.num_func_args
            self.add(b"\x00\x00")
        tid = self.token_id
        n = self.num_func_args
        if tid == TK_RESET:
            if flags & A_RESET == 0:
                hit = refuse(
                    "WARNING_BADFUNCARG", f"Function does not accept 'reset' for argument #{n}!"
                )
                if hit is not None:
                    return hit
        elif tid == TK_NUMBER:
            if flags & A_EFFECT:
                value = _atol(self.token)
                if value < 0 or value >= EFFECT_MAX:
                    hit = refuse("ERROR_BADFUNCARG", f"Invalid magic effect id '{self.token}'!")
                    if hit is not None:
                        return hit
                else:
                    self.OutputEffect(value)
                    self.last_func_arg_symbol = False
                    self.last_set_symbol = False
            elif flags & A_RESET:
                pass
            elif flags & A_NUMBER == 0:
                hit = refuse(
                    "ERROR_BADFUNCARG", f"Function does not accept a number for argument #{n}!"
                )
                if hit is not None:
                    return hit
            elif func.opcode == OPCODE_MESSAGEBOX and self.many_arg_pos == 0:
                if not self.add_message(
                    "ERROR_BADFUNCARG", "MessageBox function does not accept a number variable!"
                ):
                    return _ARG_ERROR
        elif tid == TK_STRING:
            if flags & A_ID:
                if not self.check_func_id1(flags, self.token, optional=False):
                    return _ARG_ERROR
                self.OutputFuncArgStrSym()
            elif flags & A_ANIM:
                temp = self.token[:255]
                anim = self.get_anim_group_id(_unquote(temp))
                if anim < 0:
                    hit = refuse(
                        "ERROR_BADFUNCARG",
                        f"The '{_unquote(temp)}' is not a valid animation group!",
                    )
                    if hit is not None:
                        return hit
                self.add_short(anim)
            elif flags & A_STRING == 0:
                hit = refuse(
                    "ERROR_BADFUNCARG", f"Function does not accept a string for argument #{n}!"
                )
                if hit is not None:
                    return hit
            else:
                self.OutputFuncArgString()
        elif tid == TK_XYZ:
            if flags & A_XYZ == 0:
                hit = refuse(
                    "ERROR_BADFUNCARG",
                    f"Function does not accept an X/Y/Z symbol for argument #{n}!",
                )
                if hit is not None:
                    return hit
        elif tid == TK_SYMBOL:
            if flags & A_EFFECT:
                effect = _EFFECTS.get(self.token.lower(), -1)
                if effect < 0:
                    hit = refuse("ERROR_BADFUNCARG", f"Invalid magic effect id '{self.token}'!")
                    if hit is not None:
                        return hit
                else:
                    self.OutputEffect(effect)
                    self.last_func_arg_symbol = False
                    self.last_set_symbol = True
            elif flags & A_ANIM:
                anim = self.get_anim_group_id(self.token)
                if anim < 0:
                    hit = refuse(
                        "ERROR_BADFUNCARG",
                        f"The symbol '{self.token}' is not a valid animation group!",
                    )
                    if hit is not None:
                        return hit
                self.add_short(anim)
            elif flags & A_RESET:
                if self.token.lower() == "reset":
                    self.OutputFuncArgReset()
                elif not self.add_message(
                    "ERROR_BADFUNCARG", f"The '{self.token}' is not a valid reset parameter!"
                ):
                    return _ARG_ERROR
            elif flags & A_ID == 0:
                hit = refuse(
                    "ERROR_BADFUNCARG", f"Function does not accept a symbol for argument #{n}!"
                )
                if hit is not None:
                    return hit
            else:
                if not self.check_func_id1(flags, self.token, optional=False):
                    return _ARG_ERROR
                self.OutputFuncArgSym()
        elif tid in (TK_GLOBALVAR, TK_LOCALVAR):
            if tid == TK_GLOBALVAR and func.flags & F_ALLOWGLOBAL == 0:
                hit = refuse("WARNING_BADFUNCVAR", "Function does not accept global variables!")
                if hit is not None:
                    return hit
            if flags & A_RESET:
                if self.token.lower() == "reset":
                    self.OutputFuncArgReset()
                elif not self.add_message(
                    "WARNING_BADFUNCVAR",
                    f"Function does not accept variable ({self.token}) for a reset parameter!",
                ):
                    return _ARG_ERROR
            elif func.flags & F_VAR == 0:
                hit = refuse("WARNING_BADFUNCVAR", "Function does not accept variable input!")
                if hit is not None:
                    return hit
            elif flags & A_NUMBER == 0:
                hit = refuse(
                    "ERROR_BADFUNCARG", f"Function does not accept a number for argument #{n}!"
                )
                if hit is not None:
                    return hit
        else:
            hit = refuse(
                "ERROR_BADFUNCARG",
                f"Unknown function argument #{n} '{self.token}'({_TOKEN_NAMES.get(tid, 'Unknown')}) found !",
            )
            if hit is not None:
                return hit
        return _ARG_OK

    def ParseFuncArg1(self) -> int:  # noqa: N802
        """One function argument."""
        check_msg_args = False
        self.token_parsed = True
        if self.last_func_arg == self.num_func_args:
            self.num_func_args += 1
            if not self.add_message(
                "WARNING_NOCOMMA",
                f"Missing ',' between arguments {self.num_func_args - 1} and {self.num_func_args}!",
            ):
                return ERROR
        result = self.check_func_arg()
        func = self.cur_func
        if func is None or self.func_arg_index >= MAX_FUNCARGS:
            return _ARG_ERROR
        if func.var[self.func_arg_index] & A_MANY:
            if result == _ARG_ERROR:
                self.many_arg_pos += 1
                self.func_arg_index += 1
                if func.opcode == OPCODE_MESSAGEBOX:
                    check_msg_args = True
                    if self.many_arg_pos == 0:
                        self.num_msg_args += 1
                    elif self.many_arg_pos == 1:
                        self.num_msg_buttons += 1
                result = self.check_func_arg()
                if result == _ARG_ERROR:
                    self.add_message(
                        "ERROR_BADFUNCARG",
                        f"Unknown or invalid function argument #{self.num_func_args} '{self.token}'"
                        f"({_TOKEN_NAMES.get(self.token_id, 'Unknown')}) found !",
                    )
                    return ERROR
            elif func.opcode == OPCODE_MESSAGEBOX:
                check_msg_args = True
                if self.many_arg_pos == 0:
                    self.num_msg_args += 1
                elif self.many_arg_pos == 1:
                    self.num_msg_buttons += 1
            if check_msg_args:
                if self.many_arg_pos == 0 and self.num_msg_args > MAX_MSGARGS:
                    if not self.add_message(
                        "ERROR_BADFUNCARG",
                        f"Exceeded the maximum of {MAX_MSGARGS} MessageBox variables!",
                    ):
                        return ERROR
                if self.many_arg_pos == 1 and self.num_msg_buttons > MAX_MSGBUTTONS:
                    if not self.add_message(
                        "ERROR_BADFUNCARG",
                        f"Exceeded the maximum of {MAX_MSGBUTTONS} MessageBox buttons!",
                    ):
                        return ERROR
            self.last_func_arg = self.num_func_args
            return OK
        if result == _ARG_ERROR:
            return ERROR
        if result == _ARG_ENDTABLE:
            return TABLEEND
        self.last_func_arg = self.num_func_args
        self.func_arg_index += 1
        return OK

    def ParseFuncArgX(self) -> int:  # noqa: N802
        """One extended-function argument (checked only; output comes later)."""
        self.token_parsed = True
        if self.last_func_arg == self.num_func_args:
            self.num_func_args += 1
            if not self.add_message(
                "WARNING_NOCOMMA",
                f"Missing ',' between arguments {self.num_func_args - 1} and {self.num_func_args}!",
            ):
                return ERROR
        if self.cur_func is None or self.func_arg_index >= MAX_FUNCARGS:
            return _ARG_ERROR
        self.last_func_arg = self.num_func_args
        self.func_arg_index += 1
        return OK

    def ParseFunction(self) -> int:  # noqa: N802
        """A function opening a line (``Choice`` takes its own path)."""
        if self.cur_func is not None and self.cur_func.opcode == 0x10C9:
            return self.parse_choice_function()
        return OK

    def parse_choice_function(self) -> int:
        """ParseChoiceFunction: ``Choice "text", 1, "text", 2``."""
        output_size = 0
        arg_count = 0
        size_offset = self.size
        self.add(b"\x00\x00")
        while True:
            if self.token_parsed:
                result = self.get_next_token()
                if result != OK:
                    return result
            tid = self.token_id
            if tid == TK_EOL:
                self.token_parsed = True
                break
            if tid == TK_COMMA:
                self.token_parsed = True
                continue
            if tid in (TK_NUMBER, TK_STRING):
                if tid == TK_NUMBER and "." in self.token:
                    self.add_message(
                        "ERROR_BADNUMBER",
                        f"Invalid float number '{self.token}' in Choice function call!",
                    )
                self.token_parsed = True
                data = _bytes(self.token)
                output_size += len(data)
                if arg_count > 0:
                    output_size += 1
                    self.add(b" ")
                arg_count += 1
                self.add(data)
                continue
            self.add_message(
                "ERROR_BADFUNCARG", f"Invalid argument '{self.token}' in Choice function call!"
            )
            self.token_parsed = True
        if output_size > 65535:
            self.add_message(
                "ERROR_TOOMANYARGS",
                "Arguments in Choice function exceed maximum length of 65535 bytes!",
            )
            return ERROR
        self.buf[size_offset : size_offset + 2] = struct.pack("<H", output_size & 0xFFFF)
        # Wraithguard: the call is over at the end of its line. MWEdit leaves its function
        # set here (only ParseFuncEnd clears it), so a function on any later line - a
        # StartScript after a Choice in a dialogue result - was refused as "called from
        # within another function"; the Construction Set accepts it, and so does this.
        self.cur_func = None
        return TABLEEND

    def ParseCheckFuncArg(self) -> int:  # noqa: N802
        """Before each argument: end the call when the function takes no more."""
        func = self.cur_func
        if func is None:
            return OK
        if self.num_func_args > MAX_FUNCARGS or self.func_arg_index > MAX_FUNCARGS:
            return OK
        flags = func.var[self.func_arg_index] if self.func_arg_index < MAX_FUNCARGS else 0
        if flags == 0:
            if self.token_id == TK_COMMA and not self.add_message(
                "WARNING_BADFUNCARG",
                f"Function does not accept argument #{self.func_arg_index + 1}!",
            ):
                return ERROR
            result = self.ParseFuncEnd()
            if result < 0:
                return result
            return TABLEEND
        return OK

    def ParseFuncComma(self) -> int:  # noqa: N802
        """A comma between arguments (an empty one skips an optional argument)."""
        func = self.cur_func
        if func is not None and self.last_func_arg != self.num_func_args:
            if (
                self.func_arg_index < MAX_FUNCARGS
                and func.var[self.func_arg_index] & A_OPTIONAL == 0
            ) and not self.add_message(
                "WARNING_BADFUNCARG",
                f"Function argument #{self.func_arg_index + 1} is not optional!",
            ):
                return ERROR
            if self.func_arg_index < MAX_FUNCARGS and func.var[self.func_arg_index] & A_MANY == 0:
                self.func_arg_index += 1
            self.last_func_arg += 1
        self.num_func_args += 1
        return OK

    def ParseFuncEnd(self) -> int:  # noqa: N802
        """The end of a call: defaults for omitted arguments, then the call's tail."""
        func = self.cur_func
        if func is None:
            return OK
        if self.num_func_args == 0:
            self.num_func_args = 1
        if func.flags & F_BAD and not self.add_message(
            "WARNING_BADFUNCTION",
            f"The function '{func.name}' is known to not work in Morrowind!",
        ):
            self.cur_func = None
            return ERROR
        if func.flags & F_DIALOGUE and not self.add_message(
            "WARNING_DIALOGFUNC", f"The function '{func.name}' only works in dialogue results!"
        ):
            self.cur_func = None
            return ERROR
        for index in range(self.func_arg_index, MAX_FUNCARGS):
            arg = func.var[index]
            if arg & A_VARMASK == 0:
                break
            if arg & A_OPTIONAL == 0 and arg & A_NOTREQ == 0:
                if not self.add_message(
                    "ERROR_BADFUNCARG", "Missing required argument(s) for function call!"
                ):
                    self.cur_func = None
                    return ERROR
                self.last_func_arg_symbol = False
                break
            if arg & A_RESET:
                if func.opcode == 0x10F9 and self.func_arg_index > 3:
                    self.add(b"\x01")
                else:
                    self.add(b"\x00")
            elif arg & A_BYTE or (arg & A_ID and func.flags & F_NOOPTOUT == 0):
                self.add(b"\x00")
            self.last_func_arg_symbol = False
            if arg & A_OPTSTART:
                self.add(b"\x00\x00")
        if self.func_opt_count > 0 and self.func_opt_pos > 0:
            count = self.num_func_args - self.func_opt_count
            self.buf[self.func_opt_pos : self.func_opt_pos + 2] = struct.pack(
                "<h", _wrap(count, 16)
            )
        self.OutputFuncEnd()
        self.cur_func = None
        return OK

    def ParsePushToken(self) -> int:  # noqa: N802
        """An expression token: operands are written, operators stacked (shunting-yard)."""
        top = self.expr_stack[-1] if self.expr_stack else None
        top_id = top.token_id if top is not None else TK_UNKNOWN
        push = False
        self.num_set_expr += 1
        tid = self.token_id
        if self.last_set_negative and tid != TK_NUMBER:
            self.add(b" -1")
        if tid == TK_OPENBRAC:
            push = True
            if self.last_set_negative:
                self.last_set_negative = False
                self.expr_stack.append(_Stack(TK_MULOP, "*"))
                self.last_set_symbol = False
        elif tid == TK_CLOSEBRAC:
            while self.expr_stack:
                entry = self.expr_stack.pop()
                if entry.token_id == TK_OPENBRAC:
                    break
                self.add(b" " + _bytes(entry.token))
        elif tid == TK_ADDOP:
            self.last_set_symbol = False
            self.is_empty_if = False
            if top_id in (TK_ADDOP, TK_MULOP):
                while self.expr_stack and self.expr_stack[-1].token_id in (TK_ADDOP, TK_MULOP):
                    self.add(b" " + _bytes(self.expr_stack.pop().token))
            push = True
        elif tid == TK_MULOP:
            self.last_set_symbol = False
            self.is_empty_if = False
            if top_id == TK_MULOP:
                while self.expr_stack and self.expr_stack[-1].token_id == TK_MULOP:
                    self.add(b" " + _bytes(self.expr_stack.pop().token))
            push = True
        elif tid == TK_NUMBER:
            self.add(b" ")
            if self.last_set_negative:
                self.add(b"-")
                self.last_set_negative = False
            self.add(_bytes(self.token))
            self.last_set_symbol = False
            self.is_empty_if = False
        elif tid == TK_LOCALVAR:
            index, kind = self.find_local_var_index(self.token)
            self.add(b" ")
            self.add(_bytes(kind) if kind else b"\xcc")
            self.add_short(index)
        elif tid == TK_GLOBALVAR:
            self.add_global(b" G")
        elif tid == TK_FUNCTION:
            func = self._need_func()
            self.add(b" X")
            self.add_short(func.opcode)
            self.last_func_arg_symbol = False
            if self.last_set_negative:
                self.last_set_negative = False
                self.expr_stack.append(_Stack(TK_MULOP, "*"))
                self.last_set_symbol = False
        else:
            self.add(b" " + _bytes(self.token))
        if self.last_set_negative:
            self.last_set_negative = False
            self.add(b" *")
            self.last_set_symbol = False
        if push:
            self.expr_stack.append(_Stack(tid, self.token[:STACK_MAXTOKEN]))
        self.token_parsed = True
        return OK

    def ParseIfRExprStart(self) -> int:  # noqa: N802
        """The start of either side of an if/while comparison."""
        self.last_set_symbol = True
        self.last_set_pos = self.size - 1
        self.ParseRExprStart()
        return OK

    def ParseRExprStart(self) -> int:  # noqa: N802
        """The start of an expression."""
        self.expr_stack.clear()
        self.num_output_expr = 0
        return OK

    def ParseFuncAddOp(self) -> int:  # noqa: N802
        """A sign before a numeric argument."""
        self.last_token_negative = self.token == "-"  # noqa: S105 - a minus sign
        return 0

    def ParseSetFirstAddOp(self) -> int:  # noqa: N802
        """A sign before an expression's first factor."""
        self.last_set_negative = self.token == "-"  # noqa: S105 - a minus sign
        return 0

    # --- Output routines ----------------------------------------------------------------

    def OutputScriptName(self) -> int:  # noqa: N802
        """The ``begin`` line's name."""
        self.name = self.token
        return 0

    def OutputEnd(self) -> int:  # noqa: N802
        """``end``."""
        self.statement_count += 1
        return self.output_token(0x0101)

    def OutputReturn(self) -> int:  # noqa: N802
        """``return``."""
        self.statement_count += 1
        return self.output_token(0x0124)

    def OutputObjRef(self) -> int:  # noqa: N802
        """The object-reference opcode."""
        return self.output_token(0x010C)

    def OutputOneExpr(self) -> int:  # noqa: N802
        """A no-op in MWEdit (its body is disabled)."""
        return 0

    def _open_condition(self, opcode: int) -> int:
        """Start an if/while: its opcode and a size slot filled in when it closes."""
        line_pos = self.size
        self.output_token(opcode)
        self.statement_count += 1
        self.output_token(0)
        self.last_if_pos = self.size - 1
        self.is_empty_if = True
        self.output_func_id_ref = True
        self.last_line_pos = line_pos
        return 0

    def OutputIf(self) -> int:  # noqa: N802
        """``if``: opcode, statement count, expression length (both filled in later)."""
        self._open_condition(0x0106)
        self.last_set_negative = False
        self.last_func_arg_symbol = False
        self.last_set_symbol = False
        return 0

    def OutputElseIf(self) -> int:  # noqa: N802
        """``elseif``."""
        if self.update_last_if_block() == ERROR:
            return ERROR
        return self._open_condition(0x0108)

    def OutputWhile(self) -> int:  # noqa: N802
        """``while``."""
        self._open_condition(0x010A)
        self.last_set_negative = False
        return OK

    def OutputElse(self) -> int:  # noqa: N802
        """``else``."""
        if self.update_last_if_block() == ERROR:
            return ERROR
        line_pos = self.size
        self.output_token(0x0107)
        self.statement_count += 1
        self.if_stack.append(_IfBlock(self.size, self.statement_count))
        self.add(b"\x00")
        self.last_line_pos = line_pos
        return 0

    def OutputEndIf(self) -> int:  # noqa: N802
        """``endif``."""
        if self.update_last_if_block() == ERROR:
            return ERROR
        self.statement_count += 1
        return self.output_token(0x0109)

    def OutputEndWhile(self) -> int:  # noqa: N802
        """``endwhile``."""
        if self.update_last_if_block() == ERROR:
            return ERROR
        self.statement_count += 1
        return self.output_token(0x010B)

    def update_last_if_block(self) -> int:
        """UpdateLastIfBlock: fill in the open block's statement count."""
        if not self.if_stack:
            return OK
        block = self.if_stack.pop()
        count = self.statement_count - block.start_count
        if count > MAX_IFSTATEMENTS:
            self.add_error(
                f"Exceeded the maximum of {MAX_IFSTATEMENTS} statements within an IF block!"
            )
            return ERROR
        self.buf[block.pos] = count & 0xFF
        return OK

    def _finish_condition(self, limit: int, what: str) -> int:
        """Fill in the open condition's size, refusing one past ``limit``."""
        if self.last_if_pos < 0:
            return -1
        size = self.size - self.last_if_pos - 1
        if size > limit:
            self.add_error(f"Compiled {what} expression length exceeds {limit} bytes!")
            return -1
        if self.is_empty_if:
            self.add(b"\x00")
            size += 1
        self.buf[self.last_if_pos] = size & 0xFF
        self.if_stack.append(_IfBlock(self.last_if_pos - 1, self.statement_count))
        self.last_if_pos = -1
        self.output_func_id_ref = False
        return 0

    def OutputIfFinish(self) -> int:  # noqa: N802
        """The ``)`` closing an if/elseif."""
        return self._finish_condition(MAX_IFEXPRESSIONS, "IF")

    def OutputWhileFinish(self) -> int:  # noqa: N802
        """The ``)`` closing a while."""
        return self._finish_condition(255, "WHILE")

    def OutputIfGlobal(self) -> int:  # noqa: N802
        """A global opening an if expression."""
        self.add_global(b" G")
        return 0

    def OutputIfLocal(self) -> int:  # noqa: N802
        """A local opening an if expression."""
        index, kind = self.find_local_var_index(self.token)
        self.add_short(0x20 + ((ord(kind) if kind else 0) << 8))
        self.add_short(index)
        return 0

    def OutputIfRelOp(self) -> int:  # noqa: N802
        """The comparison operator."""
        self.add(b" " + _bytes(self.token)[:13])
        self.is_empty_if = False
        return 0

    def OutputIfRAddOp(self) -> int:  # noqa: N802
        """OutputIfRAddOp."""
        self.add(_bytes(self.token))
        return 0

    def OutputIfRNumber(self) -> int:  # noqa: N802
        """OutputIfRNumber."""
        self.add(_bytes(self.token))
        return 0

    def OutputIfRLocal(self) -> int:  # noqa: N802
        """OutputIfRLocal."""
        index, kind = self.find_local_var_index(self.token)
        self.add(_bytes(kind) if kind else b"\xcc")
        self.add_short(index)
        self.add(b"\x00")
        return 0

    def OutputIfRGlobal(self) -> int:  # noqa: N802
        """OutputIfRGlobal."""
        self.add_global(b"G")
        self.add(b"\x00")
        return 0

    def OutputIfEmpty(self) -> int:  # noqa: N802
        """OutputIfEmpty."""
        self.add(b"\x00")
        return 0

    def OutputSet(self) -> int:  # noqa: N802
        """``set``."""
        line_pos = self.size
        self.num_set_expr = 0
        self.last_set_symbol = True
        self.last_func_arg_symbol = False
        self.output_func_id_ref = True
        self.last_set_negative = False
        result = self.output_token(0x0105)
        self.last_line_pos = line_pos
        self.statement_count += 1
        return result

    def OutputSetLocal(self) -> int:  # noqa: N802
        """A local as the set target."""
        index, kind = self.find_local_var_index(self.token)
        self.add(_bytes(kind) if kind else b"\xcc")
        self.add_short(index)
        self.add(b"\x00")
        self.last_set_pos = self.size - 1
        return 0

    def OutputSetGlobal(self) -> int:  # noqa: N802
        """A global as the set target."""
        self.add_global(b"G")
        self.add(b"\x00")
        self.last_set_pos = self.size - 1
        return 0

    def OutputSetEnd(self) -> int:  # noqa: N802
        """The end of a set line: fill in the expression length."""
        if self.last_set_pos < 0:
            return -1
        size = self.size - self.last_set_pos - 1
        if size > 255:
            self.add_error("Compiled SET expression length exceeds 255 bytes!")
            return ERROR
        if self.last_set_symbol and not self.last_func_arg_symbol:
            self.add(b"\x00")
            size += 1
        self.buf[self.last_set_pos] = size & 0xFF
        self.output_func_id_ref = False
        return 0

    def OutputExprStack(self) -> int:  # noqa: N802
        """Write the operators left on the stack."""
        while self.expr_stack:
            entry = self.expr_stack.pop()
            if entry.token_id in (TK_OPENBRAC, TK_CLOSEBRAC):
                break
            self.add(b" " + _bytes(entry.token))
        return 0

    def OutputIfLeftExprStack(self) -> int:  # noqa: N802
        """OutputIfLeftExprStack."""
        return self.OutputExprStack()

    def OutputIfRightExprStack(self) -> int:  # noqa: N802
        """OutputIfRightExprStack."""
        result = self.OutputExprStack()
        if self.last_set_pos >= 0:
            size = self.size - self.last_set_pos - 1
            if size > 255:
                self.add_error("Compiled IF expression length exceeds 255 bytes!")
                return ERROR
        if self.last_set_symbol and not self.last_func_arg_symbol:
            self.add(b"\x00")
        return result

    def OutputIfFunction(self) -> int:  # noqa: N802
        """A function opening an if expression."""
        self.add(b" X")
        return self.OutputFunction()

    def OutputLineFunction(self) -> int:  # noqa: N802
        """A function opening a line."""
        self.statement_count += 1
        return self.OutputFunction()

    def OutputFunction(self) -> int:  # noqa: N802
        """A function's opcode (after any stored ``id->`` reference)."""
        func = self._need_func()
        self.OutputStoredFuncOp()
        self.add_short(func.opcode)
        self.last_msgbox_args = -1
        self.last_msgbox_but = -1
        self.num_msg_args = 0
        self.num_msg_buttons = 0
        self.func_opt_count = 0
        self.last_func_arg_symbol = False
        return 0

    def OutputFuncArgReset(self) -> int:  # noqa: N802
        """``reset``."""
        self.add(b"\x05")
        return 0

    def OutputFuncArgXYZ(self) -> int:  # noqa: N802
        """``X``, ``Y`` or ``Z``."""
        self.add(_bytes(self.token[0].upper()))
        return 0

    def OutputFuncArgNum(self) -> int:  # noqa: N802
        """A numeric argument, written as the argument's type."""
        f_value = _atof(self.token)
        l_value = _atol(self.token)
        s_value = _wrap(_atol(self.token), 16)
        b_value = _wrap(s_value, 8)
        if self.last_token_negative:
            f_value, l_value = -f_value, _wrap(-l_value, 32)
            s_value, b_value = _wrap(-s_value, 16), _wrap(-b_value, 8)
            self.last_token_negative = False
        if self.num_func_args <= 0:
            self.num_func_args = 1
            self.last_func_arg = 0
        func = self.cur_func
        arg = (
            func.var[self.func_arg_index]
            if func is not None and self.func_arg_index < MAX_FUNCARGS
            else 0
        )
        if func is None:
            self.add(struct.pack("<f", f_value))
        elif arg & A_NOOUTPUT:
            self.func_opt_count += 1
        elif arg & A_FLOAT:
            self.add(struct.pack("<f", f_value))
        elif arg & A_LONG:
            self.add(struct.pack("<i", l_value))
        elif arg & A_SHORT:
            self.add(struct.pack("<h", s_value))
        elif arg & A_RESET:
            self.add(b"\x01")
        elif arg & A_BYTE:
            self.add(struct.pack("<b", b_value))
        self.last_func_arg_symbol = False
        return 0

    def _var_arg_prefix(self) -> bool:
        """Whether the current argument takes a local-variable prefix."""
        func = self._need_func()
        if 0 <= self.func_arg_index < MAX_FUNCARGS and func.var[self.func_arg_index] & A_RESET:
            return False
        if func.flags & F_SHORTVAR == 0:
            self.add(b" ")
        return True

    def OutputFuncArgGlobal(self) -> int:  # noqa: N802
        """A global as an argument."""
        if not self._var_arg_prefix():
            return 0
        self.add_global(b"G")
        self.last_func_arg_symbol = True
        func = self._need_func()
        if func.opcode != OPCODE_MESSAGEBOX:
            self.add(b"\x00")
        return 0

    def OutputFuncArgLocal(self) -> int:  # noqa: N802
        """A local as an argument."""
        func = self._need_func()
        if 0 <= self.func_arg_index < MAX_FUNCARGS and func.var[self.func_arg_index] & A_RESET:
            return 0
        index, kind = self.find_local_var_index(self.token)
        if func.flags & F_SHORTVAR == 0:
            self.add(b" ")
        self.add(_bytes(kind) if kind else b"\xcc")
        self.add_short(index)
        self.last_func_arg_symbol = True
        if func.opcode != OPCODE_MESSAGEBOX:
            self.add(b"\x00")
        return 0

    def _id_arg(self, name: bytes) -> None:
        """Write an id argument: its reference type when asked, length and name."""
        func = self._need_func()
        if self.output_func_id_ref:
            self.add(self.get_func_arg_ref_type(func.var[self.func_arg_index]))
        n = len(name) & 0xFF
        self.add(bytes([n]))
        self.add(name[:n])
        self.last_func_arg_symbol = False

    def OutputFuncArgStrSym(self) -> int:  # noqa: N802
        """A quoted ID as an argument."""
        self._id_arg(_bytes(self.token)[1:-1])
        return 0

    def OutputFuncArgSym(self) -> int:  # noqa: N802
        """A bare ID as an argument."""
        self._id_arg(_bytes(self.token))
        return 0

    def OutputFuncArgString(self) -> int:  # noqa: N802
        """A string argument (MessageBox's text and buttons included)."""
        func = self._need_func()
        text = _bytes(self.token)[1:-1]
        if func.opcode == OPCODE_MESSAGEBOX:
            if self.num_func_args == 1:
                self.add(struct.pack("<H", len(text) & 0xFFFF))
                self.add(text)
                self.add(b"\x00")
                self.last_msgbox_args = self.size - 1
                self.last_msgbox_but = -1
            else:
                if self.last_msgbox_but < 0:
                    self.add(b"\x00")
                    self.last_msgbox_but = self.size - 1
                length = (len(text) + 1) & 0xFF
                self.add(bytes([length]))
                self.add(text[: max(length - 1, 0)])
                self.add(b"\x00")
        elif func.flags & F_SHORTVAR == 0 and func.var[self.func_arg_index] & A_SHORTSTR == 0:
            self.add(struct.pack("<H", len(text) & 0xFFFF))
            self.add(text)
        else:
            n = len(text) & 0xFF
            self.add(bytes([n]))
            self.add(text[:n])
        return 0

    def OutputFuncEnd(self) -> int:  # noqa: N802
        """A call's tail: EXTRASHORT's -1, MessageBox's counts."""
        func = self._need_func()
        if func.flags & F_EXTRASHORT:
            self.add_short(-1)
        if func.opcode == OPCODE_MESSAGEBOX:
            if self.last_msgbox_args < 0:
                self.add(b"\x00\x00")
            elif self.last_msgbox_but < 0:
                self.buf[self.last_msgbox_args] = self.num_msg_args & 0xFF
                self.add(b"\x00")
            else:
                self.buf[self.last_msgbox_args] = self.num_msg_args & 0xFF
                self.buf[self.last_msgbox_but] = self.num_msg_buttons & 0xFF
        return 0

    def OutputEffect(self, effect: int) -> int:  # noqa: N802
        """A magic effect index: one byte for SHORTVAR functions, else two."""
        func = self._need_func()
        if func.flags & F_SHORTVAR:
            return int(self.add(bytes([effect & 0xFF])))
        return int(self.add_short(effect))

    # --- Object references (script_compile_ex.cc) ----------------------------------------

    def PushFuncOp(self) -> int:  # noqa: N802
        """Remember the ``id`` of ``id->`` until the function is written."""
        self.ref_object = self.last_token
        return 0

    def OutputFuncOp(self) -> int:  # noqa: N802
        """Write ``id->`` now (expressions)."""
        self.write_func_op(self.last_token)
        return 0

    def OutputStoredFuncOp(self) -> int:  # noqa: N802
        """Write a remembered ``id->``."""
        if self.ref_object:
            self.write_func_op(self.ref_object)
        self.ref_object = ""
        return 0

    def write_func_op(self, ident: str) -> int:
        """WriteFuncOp: a local holding a reference (MWSE), else the ID before the line."""
        index, kind = self.find_local_var_index(ident)
        if index >= 0:
            self.add(
                struct.pack(
                    "<6h",
                    OP_PUSHS,
                    _wrap(index - 1, 16),
                    OP_PUSHS,
                    ord(kind),
                    OP_GETLOCAL,
                    OP_SETREF,
                )
            )
        else:
            self.insert_script_data_ref(ident)
        return 0

    # --- MWSE blocks (setx / ifx / whilex), ported but untested ------------------------

    def PushLetLocal(self) -> int:  # noqa: N802
        """A ``setx`` target."""
        self.let_queue.append(_Stack(A_LOCALVAR, self.token[:STACK_MAXTOKEN]))
        return 0

    def OutputLetEnd(self) -> int:  # noqa: N802
        """Store the ``setx`` results."""
        while self.let_queue:
            entry = self.let_queue.pop(0)
            index, kind = self.find_local_var_index(entry.token)
            self.add(
                struct.pack(
                    "<5h",
                    OP_PUSHS,
                    _wrap(index - 1, 16),
                    OP_PUSHS,
                    ord(kind) if kind else 0,
                    OP_SETLOCAL,
                )
            )
        return 0

    def _x_test(self) -> None:
        """Write the test of a local variable as MWEdit lays it out."""
        index, kind = self.find_local_var_index(self.token)
        self.add(
            struct.pack(
                "<9h",
                OP_PUSHS,
                _wrap(index - 1, 16),
                OP_PUSHS,
                ord(kind) if kind else 0,
                OP_GETLOCAL,
                OP_POP,
                4,
                OP_JUMPSHORTZERO,
                0,
            )
        )

    def OutputXIf(self) -> int:  # noqa: N802
        """``ifx ( local )``."""
        self.x_if_stack.append(_IfBlock(self.size + 16, 0))
        self._x_test()
        return 0

    def OutputXElse(self) -> int:  # noqa: N802
        """``else`` of an ``ifx``."""
        if self.x_if_stack:
            block = self.x_if_stack[-1]
            self.buf[block.pos : block.pos + 2] = struct.pack("<h", _wrap(self.size + 4, 16))
            block.pos = self.size + 2
            self.add(struct.pack("<2h", OP_JUMPSHORT, 0))
        return 0

    def OutputXElseIf(self) -> int:  # noqa: N802
        """``elseif`` of an ``ifx``."""
        self.OutputXElse()
        return self.OutputXIf()

    def OutputXEndIf(self) -> int:  # noqa: N802
        """``endif`` of an ``ifx``."""
        if self.x_if_stack:
            block = self.x_if_stack.pop()
            self.buf[block.pos : block.pos + 2] = struct.pack("<h", _wrap(self.size, 16))
        return 0

    def OutputXWhile(self) -> int:  # noqa: N802
        """``whilex ( local )``."""
        self.x_if_stack.append(_IfBlock(self.size, 0))
        self._x_test()
        return 0

    def OutputXEndWhile(self) -> int:  # noqa: N802
        """``endwhile`` of a ``whilex``."""
        if self.x_if_stack:
            block = self.x_if_stack.pop()
            self.add(struct.pack("<2h", OP_JUMPSHORT, _wrap(block.pos, 16)))
            self.buf[block.pos + 16 : block.pos + 18] = struct.pack("<h", _wrap(self.size, 16))
        return 0

    def PushFuncXArgLocal(self) -> int:  # noqa: N802
        """An extended-function local argument."""
        self.arg_x_stack.append(_Stack(A_LOCALVAR, self.token[:STACK_MAXTOKEN]))
        return 0

    def PushFuncXArgNum(self) -> int:  # noqa: N802
        """An extended-function numeric argument."""
        func = self._need_func()
        arg = func.var[self.func_arg_index] & 0xFFFFFFFF
        kind = A_FLOAT if arg & A_FLOAT else A_LONG if arg & (A_LONG | A_SHORT) else arg
        if self.last_token_negative:
            self.token = "-" + self.token
            self.last_token_negative = False
        if self.num_func_args <= 0:
            self.num_func_args = 1
            self.last_func_arg = 0
        self.arg_x_stack.append(_Stack(kind, self.token[:STACK_MAXTOKEN]))
        self.last_func_arg_symbol = False
        return 0

    def PushFuncXArgString(self) -> int:  # noqa: N802
        """An extended-function string argument."""
        self.arg_x_stack.append(_Stack(A_STRING, self.token[1:-1][:STACK_MAXTOKEN]))
        return 0

    def OutputFuncXBlock(self) -> int:  # noqa: N802
        """Write an extended call: its arguments pushed last-first, then the opcode."""
        while self.arg_x_stack:
            entry = self.arg_x_stack.pop()
            if entry.token_id == A_LOCALVAR:
                index, kind = self.find_local_var_index(entry.token)
                self.add(
                    struct.pack(
                        "<5h",
                        OP_PUSHS,
                        _wrap(index - 1, 16),
                        OP_PUSHS,
                        ord(kind) if kind else 0,
                        OP_GETLOCAL,
                    )
                )
                self.last_func_arg_symbol = True
            elif entry.token_id in (A_FLOAT, A_LONG, A_SHORT):
                if entry.token_id & A_FLOAT:
                    self.add(struct.pack("<hf", OP_PUSH, _atof(entry.token)))
                else:
                    self.add(struct.pack("<hi", OP_PUSH, _atol(entry.token)))
            elif entry.token_id == A_STRING:
                text = _bytes(entry.token)
                n = len(text) & 0xFF
                padded = n if n % 2 else n + 1
                pos = self.size + 8
                out = struct.pack(
                    "<hhhh", OP_PUSHS, _wrap(pos, 16), OP_JUMPSHORT, _wrap(pos + padded + 1, 16)
                )
                out += bytes([n]) + text[:n] + (b"\x00" if padded > n else b"")
                self.add(out)
        func = self._need_func()
        self.OutputStoredFuncOp()
        self.add_short(func.opcode)
        self.last_msgbox_args = -1
        self.last_msgbox_but = -1
        self.num_msg_args = 0
        self.num_msg_buttons = 0
        self.func_opt_count = 0
        self.last_func_arg_symbol = False
        return 0


def _method_name(name: str) -> str:
    """An action's method name (MWEdit's own spelling is kept)."""
    return name


def _op_token(text: str) -> int:
    """GetESMScriptOpToken."""
    if len(text) == 1:
        return {
            "(": TK_OPENBRAC,
            ")": TK_CLOSEBRAC,
            "+": TK_ADDOP,
            "-": TK_ADDOP,
            "*": TK_MULOP,
            "/": TK_MULOP,
            ">": TK_RELOP,
            "<": TK_RELOP,
            ".": TK_VAROP,
            ",": TK_COMMA,
        }.get(text, TK_UNKNOWN)
    if len(text) == 2:
        if text == "->":
            return TK_FUNCOP
        if text[0] in "=<!>" and text[1] == "=":
            return TK_RELOP
    return TK_UNKNOWN


def _id_type_name(flags: int) -> str:
    """GetFuncArgIDType."""
    if flags & A_IDMASK == 0:
        return "None"
    if flags & A_NPCID:
        return "NPC/Creature" if flags & A_CREATUREID else "NPC"
    for bit, name in (
        (A_CELLID, "Cell"),
        (A_EFFECTID, "Effect"),
        (A_LEVELCID, "Level Creature"),
        (A_LEVELIID, "Level Item"),
        (A_CREATUREID, "Creature"),
        (A_SPELLID, "Spell"),
        (A_SOULGEMID, "Soulgem"),
        (A_TOPICID, "Topic"),
        (A_REGIONID, "Region"),
        (A_ITEMID, "Carryable"),
        (A_FACTIONID, "Faction"),
        (A_JOURNALID, "Journal"),
        (A_RACEID, "Race"),
        (A_SOUNDID, "Sound"),
        (A_SCRIPTID, "Script"),
    ):
        if flags & bit:
            return name
    return "Unknown"


def compile_script(text: str, records: RecordIndex, **options: object) -> CompileResult:
    """Compile one script's source.

    Args:
        text: The source.
        records: The load order's records (see :mod:`wraithguard.mwscript.records`).
        **options: ``levels``, ``tribunal``, ``bloodmoon`` (see :class:`Compiler`).

    Returns:
        The bytecode, locals and messages.
    """
    return Compiler(records, **options).compile(text)  # type: ignore[arg-type]


# --- The front door: the Rust compiler when built, this port when not ----------------


def plugin_bytes(text: str) -> bytes:
    """Text as a plugin stores it: Windows-1252, and Latin-1 for what that cannot hold."""
    return text.encode("cp1252", errors="wg-latin1")


def _latin1_fallback(err: UnicodeError) -> tuple[bytes, int]:
    """Encode what Windows-1252 cannot as its Latin-1 byte (``?`` past U+00FF)."""
    if not isinstance(err, UnicodeEncodeError):
        raise err
    bad = err.object[err.start : err.end]
    return bytes(ord(c) if ord(c) < 256 else 0x3F for c in bad), err.end


codecs.register_error("wg-latin1", _latin1_fallback)


def _text(value: object) -> str:
    """A backend string as text (bytes are the plugin's, Windows-1252)."""
    return plugin_text(bytes(value)) if isinstance(value, (bytes, bytearray)) else str(value)


def plugin_text(data: bytes) -> str:
    """Plugin bytes as text: Windows-1252, Latin-1 for the five bytes it leaves undefined."""
    return "".join(
        bytes([b]).decode("cp1252") if b not in (0x81, 0x8D, 0x8F, 0x90, 0x9D) else chr(b)
        for b in data
    )


#: Pool records for :class:`ScriptCompiler`: ``(tag, id, script, dial_type, shorts,
#: longs, floats)`` (``script`` empty and ``dial_type`` -1 where they do not apply).
Extra = tuple[str, str, str, int, list[str], list[str], list[str]]


def _native_compiler() -> type | None:
    """The Rust backend's ScriptCompiler, or None when it is not built."""
    try:
        import wraithguard_native
    except ImportError:
        return None
    found = getattr(wraithguard_native, "ScriptCompiler", None)
    return found if isinstance(found, type) else None


class ScriptCompiler:
    """A load order read once, to compile scripts against.

    Uses the Rust backend (``native/src/mwscript``) when it is built and this module's
    port when it is not; both write MWEdit's bytes. ``extra`` lays the patch pool's
    records over the plugins' (a global or script the patch adds), so a script compiles
    against what the patch will be.

    Args:
        plugins: The load order's plugin files, earliest first.
        extra: Pool records, as :data:`Extra`.
        native: Use the Rust backend if it is built (False: always this port).
    """

    def __init__(
        self,
        plugins: Sequence[Path | str],
        extra: Sequence[Extra] = (),
        *,
        custom: str | None = None,
        native: bool = True,
    ) -> None:
        """Read the load order.

        ``custom`` is a ``customfunctions.dat``'s text (the extended functions); None
        for the set MWEdit ships.
        """
        backend = _native_compiler() if native else None
        self.native = backend is not None
        self._custom: Mapping[str, FuncInfo] | None = None
        if backend is not None:
            self._impl: object = backend(
                [str(p) for p in plugins],
                list(extra),
                None if custom is None else plugin_bytes(custom),
            )
            return
        from wraithguard.mwscript.records import Record, index_plugins

        index = index_plugins(plugins)
        for tag, rec_id, script, dial_type, shorts, longs, floats in extra:
            index.add(
                Record(
                    tag,
                    rec_id,
                    script=script,
                    dial_type=dial_type,
                    locals={"s": shorts, "l": longs, "f": floats},
                )
            )
        self._impl = index
        if custom is not None:
            self._custom = parse_custom_functions(custom)

    def script_locals(self, rec_id: str) -> tuple[list[str], list[str], list[str]] | None:
        """A script's locals, or those of the script an object carries, by type.

        Returns ``(shorts, longs, floats)``, or None when there is no such script.
        """
        if self.native:
            got = self._impl.script_locals(rec_id)  # type: ignore[attr-defined]
            if got is None:
                return None
            shorts, longs, floats = ([plugin_text(n) for n in names] for names in got)
            return shorts, longs, floats
        index: RecordIndex = self._impl  # type: ignore[assignment]
        rec = index.find(rec_id)
        if rec is not None and rec.type != "SCPT":
            rec = index.find(rec.script) if rec.script else None
        if rec is None or rec.type != "SCPT":
            return None
        return (
            list(rec.locals.get("s", [])),
            list(rec.locals.get("l", [])),
            list(rec.locals.get("f", [])),
        )

    def check_result(self, text: str, speaker: str = "") -> list[Message]:
        """Compile a dialogue response's result script, for its errors.

        Morrowind compiles a result when the response fires, so nothing is stored; this
        is the check the Construction Set makes on save. The text is compiled as the
        body of a script that has the speaker's script's locals (a result reads and sets
        them by bare name); the messages' lines are the result's own.

        Args:
            text: The result script.
            speaker: The responding actor's id, for its script's locals ("" for none).

        Returns:
            The compiler's errors and warnings.
        """
        locals_ = self.script_locals(speaker) if speaker else None
        head = ["begin _dialogue_result"]
        if locals_ is not None:
            for kind, names in zip(("short", "long", "float"), locals_, strict=True):
                head += [f"{kind} {name}" for name in names]
        body = text.replace("\r\n", "\n").replace("\r", "\n").rstrip("\n")
        result = self.compile("\n".join([*head, body, "end", ""]))
        offset, last = len(head), len(head) + body.count("\n") + 1
        return [
            Message(min(m.line, last) - offset, m.column, m.level, m.text)
            for m in result.messages
            if m.line > offset
        ]

    def compile_record(
        self,
        rec_id: str,
        text: str,
        *,
        levels: str = "default",
        tribunal: bool = True,
        bloodmoon: bool = True,
        extended: bool = True,
    ) -> str | None:
        """The ``SCPT`` record a save writes, as the editor's record JSON, or None.

        Only the Rust backend builds it (it writes ``variables`` and ``bytecode`` in the
        tes3 crate's own form, compressed as the patch stores them); without it, or when
        the source does not compile, None.
        """
        if not self.native:
            return None
        made = self._impl.compile_record(  # type: ignore[attr-defined]
            rec_id, text, levels, tribunal, bloodmoon, extended
        )
        return made if isinstance(made, str) else None

    def compile(
        self,
        text: str,
        *,
        levels: str = "default",
        tribunal: bool = True,
        bloodmoon: bool = True,
        extended: bool = True,
    ) -> CompileResult:
        """Compile one script's source (see :class:`Compiler` for the options)."""
        if not self.native:
            index: RecordIndex = self._impl  # type: ignore[assignment]
            return Compiler(
                index,
                levels=levels,
                tribunal=tribunal,
                bloodmoon=bloodmoon,
                custom=self._custom,
                extended=extended,
            ).compile(text)
        got: dict[str, Any] = self._impl.compile(  # type: ignore[attr-defined]
            plugin_bytes(text), levels, tribunal, bloodmoon, extended
        )
        return CompileResult(
            ok=bool(got["ok"]),
            name=plugin_text(got["name"]),
            data=bytes(got["data"]),
            shorts=[plugin_text(n) for n in got["shorts"]],
            longs=[plugin_text(n) for n in got["longs"]],
            floats=[plugin_text(n) for n in got["floats"]],
            messages=[
                Message(int(line), int(col), _text(level), _text(msg))
                for line, col, level, msg in got["messages"]
            ],
        )


#: Where MWEdit and the MWSE updater put the extended function list.
CUSTOM_FUNCTIONS_FILE: Final = "customfunctions.dat"


def find_custom_functions(plugins: Sequence[Path | str]) -> str | None:
    """A ``customfunctions.dat`` near the load order, read.

    Looked for beside a plugin, and in the folder above its data folder (the game's,
    where MWSE's updater writes one).

    Returns None when there is none (the compiler then knows MWEdit's own set).
    """
    from pathlib import Path

    seen: set[Path] = set()
    for plugin in plugins:
        folder = Path(plugin).parent
        for place in (folder, folder.parent):
            if place in seen:
                continue
            seen.add(place)
            candidate = place / CUSTOM_FUNCTIONS_FILE
            try:
                if candidate.is_file():
                    return candidate.read_text(encoding="cp1252", errors="replace")
            except OSError:
                continue
    return None
