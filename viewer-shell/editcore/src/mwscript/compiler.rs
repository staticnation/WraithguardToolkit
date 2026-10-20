//! Morrowind script source to bytecode, the way MWEdit compiles it.
//!
//! A port of MWEdit's compiler (`mwedit/script_compile.cc` and `script_compile_ex.cc`,
//! MIT, Copyright 2025 Walrus Tech; originally Dave Humphrey), routine for routine and
//! under MWEdit's names, so the two read side by side. It is table-driven: `data::TABLES`
//! is the grammar (generated from MWEdit's source by `tools/gen_mwscript_compiler.py`),
//! each row naming the token it expects and the routines that check it (`Parse*`) and
//! write its bytes (`Output*`).
//!
//! `wraithguard/mwscript/compiler.py` is the same port in Python: the reference this one
//! is held to, and the fallback when the native module is not built. Both write MWEdit's
//! bytes for every script of its compiler test plugin (`tests/fixtures/mwedit/`).
//!
//! MWEdit's quirks are kept where they change the output (`InsertScriptDataRef`'s
//! arithmetic for a quoted ID is one): a compiler that "fixed" them would write scripts
//! that differ from the ones already in people's plugins. Text is bytes (Latin-1), as in
//! MWEdit; IDs compare with ASCII case folded, as `_stricmp` does.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::data::{self, Action, Row};
use super::records::{Record, RecordIndex};

// Result codes.
pub const OK: i32 = 0;
pub const ERROR: i32 = -1;
pub const BLOCKEND: i32 = 2;
pub const TABLEEND: i32 = 3;
// CheckFuncArg results.
const ARG_OK: i32 = 0;
const ARG_ERROR: i32 = -1;
const ARG_ENDTABLE: i32 = -2;

const ONE: u8 = 1;
const MANY: u8 = 2;
const OPT: u8 = 4;
const STOP: u8 = 8;

// Function flags.
const F_SHORTVAR: u32 = 0x1;
const F_ALLOWGLOBAL: u32 = 0x2;
const F_EXTRASHORT: u32 = 0x4;
const F_NOOPTOUT: u32 = 0x8;
const F_BLOODMOON: u32 = 0x400;
const F_TRIBUNAL: u32 = 0x800;
const F_DIALOGUE: u32 = 0x1000;
const F_BAD: u32 = 0x2000;
const F_VAR: u32 = 0x4000;
// Argument flags.
const A_BYTE: u64 = 0x1;
const A_SHORT: u64 = 0x2;
const A_LONG: u64 = 0x4;
const A_FLOAT: u64 = 0x8;
const A_NUMBER: u64 = 0xF;
const A_STRING: u64 = 0x10;
const A_ID: u64 = 0x20;
const A_XYZ: u64 = 0x40;
const A_EFFECT: u64 = 0x80;
const A_RESET: u64 = 0x100;
const A_ANIM: u64 = 0x200;
const A_OPTSTART: u64 = 0x400;
const A_OPTIONAL: u64 = 0x800;
const A_NOTREQ: u64 = 0x1000;
const A_CELLSTR: u64 = 0x2000;
const A_SHORTSTR: u64 = 0x4000;
const A_MANY: u64 = 0x8000;
const A_VARMASK: u64 = 0x3FF;
const A_SCRIPTID: u64 = 0x10000;
const A_SOUNDID: u64 = 0x20000;
const A_RACEID: u64 = 0x40000;
const A_JOURNALID: u64 = 0x80000;
const A_FACTIONID: u64 = 0x100000;
const A_ITEMID: u64 = 0x200000;
const A_REGIONID: u64 = 0x400000;
const A_TOPICID: u64 = 0x800000;
const A_CELLID: u64 = 0x1000000;
const A_EFFECTID: u64 = 0x2000000;
const A_LEVELCID: u64 = 0x4000000;
const A_LEVELIID: u64 = 0x8000000;
const A_SOULGEMID: u64 = 0x10000000;
const A_CREATUREID: u64 = 0x20000000;
const A_SPELLID: u64 = 0x40000000;
const A_NPCID: u64 = 0x80000000;
const A_IDMASK: u64 = 0xFFFF0000;
const A_NOOUTPUT: u64 = 0x100000000;
/// ESMSCR_FUNC_LOCALVAR: the extended-argument stack's tag for a local.
const A_LOCALVAR: i32 = 0x10000;

const OPCODE_MESSAGEBOX: u16 = 0x1000;
const MAX_FUNCARGS: usize = 13;
const MAX_MSGBUTTONS: i32 = 9;
const MAX_MSGARGS: i32 = 9;
const MAX_IFSTATEMENTS: i32 = 255;
const MAX_IFEXPRESSIONS: i32 = 255;
const DATA_SIZE: usize = 65535;
const MAX_LOCALVARS: usize = 255;
const VAR_MAXLENGTH: usize = 32;
const SPECIAL_LOCAL_INDEX: usize = 33;
const STACK_MAXTOKEN: usize = 63;
const EFFECT_MAX: i32 = 142;

// MWSE opcodes used by the extended blocks.
const OP_JUMPSHORT: i16 = 0x380A;
const OP_JUMPSHORTZERO: i16 = 0x380C;
const OP_POP: i16 = 0x380F;
const OP_PUSH: i16 = 0x3811;
const OP_PUSHS: i16 = 0x3813;
const OP_GETLOCAL: i16 = 0x3C00;
const OP_SETLOCAL: i16 = 0x3C02;
const OP_SETREF: i16 = 0x3C18;

const CHAR_EOL: u8 = b'\r';
const CHAR_COMMENT: u8 = b';';
const CHAR_STRING: u8 = b'"';
const CT_SPACE: u8 = 1;
const CT_PUNCT: u8 = 2;
const CT_DIGIT: u8 = 4;
const CT_SYMBOLF: u8 = 8;
const CT_SYMBOL: u8 = 16;

use super::data::{
    BEGINBLOCK, ENDTABLE, TOKEN_ADDOP as TK_ADDOP, TOKEN_CLOSEBRAC as TK_CLOSEBRAC, TOKEN_COMMA as TK_COMMA,
    TOKEN_EOL as TK_EOL, TOKEN_EOS as TK_EOS, TOKEN_FUNCOP as TK_FUNCOP, TOKEN_FUNCTION as TK_FUNCTION,
    TOKEN_GLOBALVAR as TK_GLOBALVAR, TOKEN_LOCALVAR as TK_LOCALVAR, TOKEN_MULOP as TK_MULOP, TOKEN_NUMBER as TK_NUMBER,
    TOKEN_OPENBRAC as TK_OPENBRAC, TOKEN_RELOP as TK_RELOP, TOKEN_RESET as TK_RESET, TOKEN_STRING as TK_STRING,
    TOKEN_SYMBOL as TK_SYMBOL, TOKEN_UNKNOWN as TK_UNKNOWN, TOKEN_VAROP as TK_VAROP, TOKEN_XYZ as TK_XYZ,
};

/// GetESMTokenName.
fn token_name(id: i32) -> &'static str {
    match id {
        -2 => "Begin Block",
        -1 => "End Table",
        1 => "Integer",
        2 => "Float",
        3 => "String",
        4 => "Symbol",
        5 => "Operator",
        6 => "End-of-Line",
        7 => "End-of-File",
        8 => "begin",
        9 => "end",
        10 => "Type",
        11 => "set",
        12 => "to",
        13 => "(",
        14 => ")",
        15 => "+ or -",
        16 => "* or /",
        17 => "Compare Operator",
        18 => "if",
        19 => "elseif",
        20 => "else",
        21 => "endif",
        22 => "while",
        23 => "endwhile",
        24 => "return",
        25 => "get",
        26 => "Number",
        27 => ",",
        28 => "->",
        29 => ".",
        30 => "X/Y/Z",
        31 => "Local Variable",
        32 => "Global Variable",
        33 => "Function",
        34 => "Object ID",
        35 => "reset",
        37 => "setx",
        38 => "ifx",
        39 => "whilex",
        _ => "Unknown",
    }
}

/// GetESMScriptResToken: a reserved word's token, or TOKEN_UNKNOWN.
fn reserved(word: &[u8]) -> i32 {
    match word.to_ascii_lowercase().as_slice() {
        b"begin" => data::TOKEN_BEGIN,
        b"end" => data::TOKEN_END,
        b"endwhile" => data::TOKEN_ENDWHILE,
        b"else" => data::TOKEN_ELSE,
        b"elseif" => data::TOKEN_ELSEIF,
        b"endif" => data::TOKEN_ENDIF,
        b"if" => data::TOKEN_IF,
        b"ifx" => data::TOKEN_IFX,
        b"long" | b"float" | b"short" => data::TOKEN_TYPEOP,
        b"return" => data::TOKEN_RETURN,
        b"set" => data::TOKEN_SET,
        b"setx" => data::TOKEN_SETX,
        b"to" => data::TOKEN_TO,
        b"get" => data::TOKEN_GET,
        b"while" => data::TOKEN_WHILE,
        b"whilex" => data::TOKEN_WHILEX,
        b"x" | b"y" | b"z" => data::TOKEN_XYZ,
        _ => TK_UNKNOWN,
    }
}

/// GetESMScriptOpToken.
fn op_token(op: &[u8]) -> i32 {
    match op {
        b"(" => TK_OPENBRAC,
        b")" => TK_CLOSEBRAC,
        b"+" | b"-" => TK_ADDOP,
        b"*" | b"/" => TK_MULOP,
        b">" | b"<" => TK_RELOP,
        b"." => TK_VAROP,
        b"," => TK_COMMA,
        b"->" => TK_FUNCOP,
        [b'=' | b'<' | b'!' | b'>', b'='] => TK_RELOP,
        _ => TK_UNKNOWN,
    }
}

// --- Message levels ----------------------------------------------------------------

const LEVEL_ERROR: i32 = -1;
const LEVEL_NONE: i32 = 0;
const LEVEL_WARNING: i32 = 1;

/// How strict the compile is: MWEdit's default, weak and strong message levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Levels {
    Default,
    Weak,
    Strong,
}

impl Levels {
    /// By name (`default`, `weak`, `strong`); anything else is the default.
    pub fn from_name(name: &str) -> Self {
        match name {
            "weak" => Levels::Weak,
            "strong" => Levels::Strong,
            _ => Levels::Default,
        }
    }

    /// FindMsgLevel.
    fn level(self, id: &str) -> i32 {
        use Levels::{Strong, Weak};
        let (e, n, w) = (LEVEL_ERROR, LEVEL_NONE, LEVEL_WARNING);
        match id {
            "ERROR_BADID" => match self {
                Weak => w,
                _ => e,
            },
            "WARNING_NOCOMMA" => n,
            "WARNING_BADFUNCARG" | "WARNING_BADFUNCVAR" => match self {
                Weak => n,
                _ => w,
            },
            "ERROR_BADLOCAL" | "ERROR_BADTOKEN" | "ERROR_TOOMANYARGS" => e,
            "ERROR_BADNUMBER" | "ERROR_MULTREF" | "ERROR_BADFUNCARG" => match self {
                Weak => w,
                _ => e,
            },
            "WARNING_NOSPACE" | "WARNING_BADVARSTART" => match self {
                Strong => w,
                _ => n,
            },
            "WARNING_IDCONFLICT" | "WARNING_LOCALVAR34" => match self {
                Weak => n,
                _ => w,
            },
            "WARNING_BADFUNCTION" => match self {
                Weak => n,
                Strong => e,
                _ => w,
            },
            "WARNING_EXTFUNC" => match self {
                Strong => e,
                _ => w,
            },
            "WARNING_EXPANSIONFUNC" => match self {
                Weak => w,
                _ => e,
            },
            "WARNING_DIALOGFUNC" => match self {
                Strong => w,
                _ => n,
            },
            _ => w,
        }
    }
}

// --- Function table ----------------------------------------------------------------

/// One built-in function (MWEdit's `esmscrfuncinfo_t`).
#[derive(Clone, Copy, Debug)]
pub struct Func {
    pub name: &'static [u8],
    pub opcode: u16,
    pub flags: u32,
    pub var: [u64; 13],
}

fn func_map() -> &'static HashMap<Vec<u8>, Func> {
    static MAP: OnceLock<HashMap<Vec<u8>, Func>> = OnceLock::new();
    MAP.get_or_init(|| {
        data::FUNCTIONS
            .iter()
            .map(|&(name, opcode, flags, _ret, var)| (name.to_ascii_lowercase(), Func { name, opcode, flags, var }))
            .collect()
    })
}

/// A built-in function by name, case aside.
pub fn find_function(name: &[u8]) -> Option<Func> {
    func_map().get(&name.to_ascii_lowercase()).copied()
}

// --- Custom (MWSE / MW-Enhanced) functions ---------------------------------------------

/// Extended functions by ASCII-lowercased name.
pub type CustomFunctions = HashMap<Vec<u8>, Func>;

const F_MWSE: u32 = 0x8000;
const F_MWE: u32 = 0x10000;

/// MWEdit's `customfunctions.dat` function options. "ShortVar" maps to ALLOWGLOBAL, as
/// MWEdit has it (`mw_custom_func.cc`); extended functions do not read either when written.
fn func_option(word: &[u8]) -> u32 {
    match word.to_ascii_lowercase().as_slice() {
        b"shortvar" | b"allowglobal" => F_ALLOWGLOBAL,
        b"extrashort" => F_EXTRASHORT,
        b"nooptout" => F_NOOPTOUT,
        b"bloodmoon" => F_BLOODMOON,
        b"tribunal" => F_TRIBUNAL,
        b"dialogue" => F_DIALOGUE,
        b"bad" => F_BAD,
        b"allowvar" => F_VAR,
        b"mwse" | b"extended" => F_MWSE,
        b"mwe" | b"mwenhanced" => F_MWE,
        _ => 0,
    }
}

/// MWEdit's `customfunctions.dat` argument options.
fn arg_option(word: &[u8]) -> u64 {
    match word.to_ascii_lowercase().as_slice() {
        b"byte" => A_BYTE,
        b"short" => A_SHORT,
        b"long" | b"ref" => A_LONG,
        b"float" => A_FLOAT,
        b"number" => A_NUMBER,
        b"string" => A_STRING,
        b"id" => A_ID,
        b"xyz" => A_XYZ,
        b"effect" => A_EFFECT,
        b"reset" => A_RESET,
        b"animation" => A_ANIM,
        b"optstart" => A_OPTSTART,
        b"optional" => A_OPTIONAL,
        b"notrequired" => A_NOTREQ,
        b"cellstring" => A_CELLSTR,
        b"shortstring" => A_SHORTSTR,
        b"many" => A_MANY,
        b"scriptid" => A_SCRIPTID,
        b"soundid" => A_SOUNDID,
        b"raceid" => A_RACEID,
        b"journalid" => A_JOURNALID,
        b"factionid" => A_FACTIONID,
        b"itemid" => A_ITEMID,
        b"regionid" => A_REGIONID,
        b"topicid" => A_TOPICID,
        b"cellid" => A_CELLID,
        b"effectid" => A_EFFECTID,
        b"levelcreatureid" => A_LEVELCID,
        b"levelitemid" => A_LEVELIID,
        b"soulgemid" => A_SOULGEMID,
        b"creatureid" => A_CREATUREID,
        b"spellid" => A_SPELLID,
        b"npcid" => A_NPCID,
        _ => 0,
    }
}

/// MWEdit's SeperateVarValueQ with `=` and `#`: the name, and the value when there is one.
fn var_value(line: &[u8]) -> (&[u8], Option<&[u8]>) {
    let (mut sep, mut start, mut end, mut quoted) = (None, 0, line.len(), false);
    for (i, &c) in line.iter().enumerate() {
        if c == b'=' && sep.is_none() {
            sep = Some(i);
            start = i + 1;
        } else if sep.is_some() && c == b'"' {
            if quoted {
                end = i;
                break;
            }
            quoted = true;
            start = i + 1;
        } else if !quoted && c == b'#' {
            end = i;
            break;
        }
    }
    match sep {
        None => (line[..end].trim_ascii(), None),
        Some(s) => (line[..s].trim_ascii(), Some(line[start..end.max(start)].trim_ascii())),
    }
}

fn flag_words<T: std::ops::BitOr<Output = T> + Default>(value: &[u8], f: impl Fn(&[u8]) -> T) -> T {
    value.split(|&c| c == b'|').map(|w| f(w.trim_ascii())).fold(T::default(), |a, b| a | b)
}

/// Parses MWEdit's `customfunctions.dat` (`function` ... `end` blocks of `Name`,
/// `Options`, `Param1`-`Param12` and `Opcode`), as `ReadMwCustomFunctions` reads it.
/// A name is kept for the life of the process (they are few, read once per file).
pub fn parse_custom_functions(text: &[u8]) -> CustomFunctions {
    let mut out = CustomFunctions::new();
    let mut lines = text.split(|&c| c == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l));
    while let Some(raw) = lines.next() {
        let (word, value) = var_value(raw);
        if value.is_some() || !word.eq_ignore_ascii_case(b"function") {
            continue;
        }
        let (mut name, mut opcode, mut flags, mut var) = (Vec::new(), 0u16, 0u32, [0u64; 13]);
        for inner in lines.by_ref() {
            let (key, value) = var_value(inner);
            let Some(value) = value else {
                if key.eq_ignore_ascii_case(b"end") {
                    break;
                }
                continue;
            };
            let key = key.to_ascii_lowercase();
            match key.as_slice() {
                b"name" => name = value.to_vec(),
                b"opcode" => {
                    let t = String::from_utf8_lossy(value);
                    let t = t.trim();
                    let parsed = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                        Some(hex) => u32::from_str_radix(hex, 16),
                        None => t.parse::<u32>(),
                    };
                    opcode = parsed.map_or(0, |v| v as u16);
                }
                b"options" => flags = flag_words(value, func_option),
                k if k.starts_with(b"param") => {
                    if let Ok(n) = std::str::from_utf8(&k[5..]).unwrap_or("").parse::<usize>()
                        && (1..=12).contains(&n)
                    {
                        var[n - 1] = flag_words(value, arg_option);
                    }
                }
                _ => {}
            }
        }
        if !name.is_empty() {
            let key = name.to_ascii_lowercase();
            let name: &'static [u8] = Box::leak(name.into_boxed_slice());
            out.insert(key, Func { name, opcode, flags, var });
        }
    }
    out
}

/// The extended functions MWEdit ships (its `customfunctions.dat`, generated into `data`).
pub fn default_custom() -> &'static CustomFunctions {
    static MAP: OnceLock<CustomFunctions> = OnceLock::new();
    MAP.get_or_init(|| {
        data::CUSTOM_FUNCTIONS
            .iter()
            .map(|&(name, opcode, flags, var)| (name.to_ascii_lowercase(), Func { name, opcode, flags, var }))
            .collect()
    })
}

/// FindESMEffectID: a magic effect's index from its ID (`sEffectJump`).
fn find_effect(name: &[u8]) -> i32 {
    data::EFFECT_IDS.iter().position(|e| e.eq_ignore_ascii_case(name)).map_or(-1, |i| i as i32)
}

// --- C library behaviour the output depends on -------------------------------------

fn c_isspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

fn c_ispunct(c: u8) -> bool {
    matches!(c, 33..=47 | 58..=64 | 91..=96 | 123..=126)
}

/// C `atof`: the longest numeric prefix, 0.0 if none.
fn atof(text: &[u8]) -> f64 {
    let n = text.len();
    let mut i = 0;
    while i < n && c_isspace(text[i]) {
        i += 1;
    }
    let start = i;
    if i < n && matches!(text[i], b'+' | b'-') {
        i += 1;
    }
    let digits = i;
    while i < n && text[i].is_ascii_digit() {
        i += 1;
    }
    if i < n && text[i] == b'.' {
        i += 1;
        while i < n && text[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == digits || &text[digits..i] == b"." {
        return 0.0;
    }
    if i < n && matches!(text[i], b'e' | b'E') {
        let mut j = i + 1;
        if j < n && matches!(text[j], b'+' | b'-') {
            j += 1;
        }
        if j < n && text[j].is_ascii_digit() {
            while j < n && text[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    std::str::from_utf8(&text[start..i]).ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
}

/// C `atol`/`atoi` (32-bit, saturating like MSVC's).
fn atol(text: &[u8]) -> i32 {
    let n = text.len();
    let mut i = 0;
    while i < n && c_isspace(text[i]) {
        i += 1;
    }
    let mut sign = 1i64;
    if i < n && matches!(text[i], b'+' | b'-') {
        if text[i] == b'-' {
            sign = -1;
        }
        i += 1;
    }
    let mut value = 0i64;
    while i < n && text[i].is_ascii_digit() {
        value = (value * 10 + i64::from(text[i] - b'0')).min(1 << 40);
        i += 1;
    }
    (value * sign).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// MWEdit's IsStringFloat: digits and whitespace with at most one '.'.
fn is_string_float(text: &[u8]) -> bool {
    let mut seen_dot = false;
    for &c in text {
        if c == b'.' && !seen_dot {
            seen_dot = true;
            continue;
        }
        if !(c_isspace(c) || c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// MWEdit's UnquoteString: the text between the first two quotes, if any.
fn unquote(text: &[u8]) -> &[u8] {
    let Some(first) = text.iter().position(|&c| c == b'"') else { return text };
    let rest = &text[first + 1..];
    match rest.iter().position(|&c| c == b'"') {
        Some(second) => &rest[..second],
        None => rest,
    }
}

/// The quotes off a token (`"id"` -> `id`), as MWEdit's `Copy(token + 1, len - 2)`.
fn inner(token: &[u8]) -> &[u8] {
    if token.len() >= 2 { &token[1..token.len() - 1] } else { &[] }
}

/// Latin-1 bytes as text, for messages.
fn text(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

// --- Results ---------------------------------------------------------------------------

/// A compiler error or warning: line and column from 1, `error` or `warning`, the text.
#[derive(Clone, Debug)]
pub struct Message {
    pub line: usize,
    pub column: usize,
    pub level: &'static str,
    pub text: String,
}

/// What a compile produced.
#[derive(Clone, Debug)]
pub struct CompileResult {
    /// The compile succeeded (MWEdit's `Compile() >= 0`).
    pub ok: bool,
    /// The script's name, from its `begin` line.
    pub name: Vec<u8>,
    /// The bytecode (`SCDT`).
    pub data: Vec<u8>,
    /// Local names: shorts, longs, floats, in declaration order.
    pub locals: [Vec<Vec<u8>>; 3],
    /// Errors and warnings, in the order found.
    pub messages: Vec<Message>,
}

impl CompileResult {
    /// The `SCVR` block: every local's name, NUL-terminated, shorts first.
    pub fn var_data(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for name in self.locals.iter().flatten() {
            out.extend_from_slice(name);
            out.push(0);
        }
        out
    }

    /// The 52-byte `SCHD`: name, the three local counts and the two sizes.
    pub fn header(&self) -> Vec<u8> {
        let mut out = vec![0u8; 32];
        let n = self.name.len().min(31);
        out[..n].copy_from_slice(&self.name[..n]);
        for v in [
            self.locals[0].len(),
            self.locals[1].len(),
            self.locals[2].len(),
            self.data.len(),
            self.var_data().len(),
        ] {
            out.extend_from_slice(&(v as u32).to_le_bytes());
        }
        out
    }
}

/// An expression stack entry (`esmscrstack_t`).
struct StackEntry {
    token_id: i32,
    token: Vec<u8>,
}

/// An if/while block's statement-count slot (`esmscrifblock_t`).
struct IfBlock {
    pos: usize,
    start_count: i32,
}

// --- The compiler -------------------------------------------------------------------------

/// One compile of one script (MWEdit's `CEsmScriptCompile`).
pub struct Compiler<'r> {
    records: &'r RecordIndex,
    levels: Levels,
    allow_tribunal: bool,
    allow_bloodmoon: bool,
    custom: &'r CustomFunctions,
    allow_extended: bool,

    text: Vec<u8>,
    pos: usize,
    token: Vec<u8>,
    last_token: Vec<u8>,
    token_id: i32,
    line: usize,
    char_pos: usize,
    token_parsed: bool,
    line_has_ref: bool,
    last_token_negative: bool,
    last_set_negative: bool,

    cur_func: Option<Func>,
    num_func_args: i32,
    last_func_arg: i32,
    func_arg_index: usize,

    messages: Vec<Message>,
    locals: [Vec<Vec<u8>>; 3],

    ref_object: Vec<u8>,
    arg_x_stack: Vec<StackEntry>,
    let_queue: Vec<StackEntry>,
    x_if_stack: Vec<IfBlock>,

    expr_stack: Vec<StackEntry>,

    buf: Vec<u8>,
    size: usize,
    last_line_pos: usize,
    last_if_pos: i64,
    last_set_pos: i64,
    is_empty_if: bool,
    last_set_symbol: bool,
    output_func_id_ref: bool,
    last_msgbox_but: i64,
    last_msgbox_args: i64,
    num_msg_buttons: i32,
    num_msg_args: i32,
    many_arg_pos: i32,
    last_func_arg_symbol: bool,
    func_opt_count: i32,
    func_opt_pos: usize,
    statement_count: i32,
    if_stack: Vec<IfBlock>,

    name: Vec<u8>,
}

type Entry = &'static Row;

impl<'r> Compiler<'r> {
    /// A compile against `records`, at MWEdit's message `levels`, with or without the
    /// expansions' functions (Tribunal also picks the animation group numbering).
    pub fn new(records: &'r RecordIndex, levels: Levels, tribunal: bool, bloodmoon: bool) -> Self {
        Self::with_custom(records, levels, tribunal, bloodmoon, default_custom(), true)
    }

    /// As `new`, with the extended (MWSE / MW-Enhanced) functions to know (see
    /// `parse_custom_functions`) and whether to use them without a warning.
    pub fn with_custom(
        records: &'r RecordIndex,
        levels: Levels,
        tribunal: bool,
        bloodmoon: bool,
        custom: &'r CustomFunctions,
        extended: bool,
    ) -> Self {
        Compiler {
            records,
            levels,
            allow_tribunal: tribunal,
            allow_bloodmoon: bloodmoon,
            custom,
            allow_extended: extended,
            text: Vec::new(),
            pos: 0,
            token: Vec::new(),
            last_token: Vec::new(),
            token_id: TK_UNKNOWN,
            line: 0,
            char_pos: 0,
            token_parsed: true,
            line_has_ref: false,
            last_token_negative: false,
            last_set_negative: false,
            cur_func: None,
            num_func_args: 0,
            last_func_arg: 0,
            func_arg_index: 0,
            messages: Vec::new(),
            locals: Default::default(),
            ref_object: Vec::new(),
            arg_x_stack: Vec::new(),
            let_queue: Vec::new(),
            x_if_stack: Vec::new(),
            expr_stack: Vec::new(),
            buf: vec![0u8; DATA_SIZE],
            size: 0,
            last_line_pos: 0,
            last_if_pos: -1,
            last_set_pos: 0,
            is_empty_if: false,
            last_set_symbol: false,
            output_func_id_ref: false,
            last_msgbox_but: -1,
            last_msgbox_args: -1,
            num_msg_buttons: 0,
            num_msg_args: 0,
            many_arg_pos: 0,
            last_func_arg_symbol: false,
            func_opt_count: 0,
            func_opt_pos: 0,
            statement_count: 0,
            if_stack: Vec::new(),
            name: Vec::new(),
        }
    }

    /// Compiles `source` (Latin-1; line ends `\r\n`, `\n` or `\r`).
    pub fn compile(mut self, source: &[u8]) -> CompileResult {
        let mut norm = Vec::with_capacity(source.len() + 16);
        let mut i = 0;
        while i < source.len() {
            match source[i] {
                0 => break,
                b'\r' => {
                    norm.extend_from_slice(b"\r\n");
                    if source.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                }
                b'\n' => norm.extend_from_slice(b"\r\n"),
                c => norm.push(c),
            }
            i += 1;
        }
        self.text = norm;
        self.pos = 0;
        self.token_parsed = true;
        let result = self.parse_table(data::MAIN_BLOCK);
        let size = self.size;
        CompileResult {
            ok: result >= 0,
            name: std::mem::take(&mut self.name),
            data: self.buf[..size].to_vec(),
            locals: std::mem::take(&mut self.locals),
            messages: std::mem::take(&mut self.messages),
        }
    }

    // --- Buffer -----------------------------------------------------------------------

    /// The character at the parse position plus `off` (NUL past either end).
    fn ch(&self, off: isize) -> u8 {
        let i = self.pos as isize + off;
        if i < 0 { 0 } else { self.text.get(i as usize).copied().unwrap_or(0) }
    }

    fn ctype(c: u8) -> u8 {
        data::CHAR_TYPES[c as usize]
    }

    /// AddScriptData.
    fn add(&mut self, bytes: &[u8]) -> bool {
        if self.size + bytes.len() >= DATA_SIZE {
            self.add_error(format!("Maximum compiled script size {DATA_SIZE} exceeded!"));
            return false;
        }
        self.buf[self.size..self.size + bytes.len()].copy_from_slice(bytes);
        if self.last_line_pos == self.size {
            self.last_line_pos += bytes.len();
        }
        self.size += bytes.len();
        true
    }

    fn add_short(&mut self, v: i64) -> bool {
        self.add(&(v as i16).to_le_bytes())
    }

    /// OutputToken.
    fn output_token(&mut self, v: i64) -> i32 {
        self.add_short(v);
        0
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        if at + bytes.len() <= self.buf.len() {
            self.buf[at..at + bytes.len()].copy_from_slice(bytes);
        }
    }

    /// InsertScriptDataRef, MWEdit's arithmetic included: for a quoted ID the line is
    /// shifted by the quoted length but the unquoted one is written, so two stale bytes
    /// stay between the reference and the line and its last two bytes fall off the end.
    fn insert_script_data_ref(&mut self, ident: &[u8]) -> bool {
        let mut size = ident.len();
        if self.size + size >= DATA_SIZE {
            self.add_error(format!("Maximum compiled script size {DATA_SIZE} exceeded!"));
            return false;
        }
        let start = self.last_line_pos;
        if self.size >= start {
            let moved = self.buf[start..self.size].to_vec();
            self.put(start + size + 3, &moved);
        }
        self.put(start, &0x010Cu16.to_le_bytes());
        if ident.first() == Some(&b'"') {
            size = size.saturating_sub(2);
            let n = size & 0xFF;
            self.buf[start + 2] = n as u8;
            let src = &ident[1..(1 + n).min(ident.len())];
            self.put(start + 3, src);
        } else {
            let n = size & 0xFF;
            self.buf[start + 2] = n as u8;
            self.put(start + 3, &ident[..n]);
        }
        if self.last_set_pos >= start as i64 {
            self.last_set_pos += size as i64 + 3;
        }
        if self.last_if_pos >= start as i64 {
            self.last_if_pos += size as i64 + 3;
        }
        self.size += size + 3;
        true
    }

    /// A global's name: `prefix`, a length byte and the name, padded to four bytes.
    fn add_global(&mut self, prefix: &[u8]) {
        let name = self.token.clone();
        self.add(prefix);
        if name.len() < 4 {
            self.add(&[4]);
            self.add(&name);
            self.add(&vec![0u8; 4 - name.len()]);
        } else {
            self.add(&[(name.len() & 0xFF) as u8]);
            self.add(&name);
        }
    }

    // --- Messages -----------------------------------------------------------------------

    fn record(&mut self, level: &'static str, msg: String) {
        self.messages.push(Message { line: self.line + 1, column: self.char_pos + 1, level, text: msg });
    }

    /// AddError.
    fn add_error(&mut self, msg: String) {
        self.record("error", msg);
    }

    /// AddMessage: false when the message is an error that stops the compile.
    fn add_message(&mut self, id: &str, msg: String) -> bool {
        match self.levels.level(id) {
            LEVEL_NONE => true,
            LEVEL_WARNING => {
                self.record("warning", msg);
                true
            }
            _ => {
                self.add_error(msg);
                false
            }
        }
    }

    /// AssertToken.
    fn assert_token(&mut self, token: i32) -> i32 {
        if self.token_id != token {
            let msg = format!(
                "Syntax Error: Expected '{}' but found '{}' ({})!",
                token_name(token),
                token_name(self.token_id),
                text(&self.token)
            );
            self.add_message("ERROR_BADTOKEN", msg);
            return ERROR;
        }
        OK
    }

    // --- Records --------------------------------------------------------------------------

    /// IsSymbolID: whether an object has this ID, and the ID in the record's case.
    fn is_symbol_id(&self, token: &[u8]) -> Option<Vec<u8>> {
        let name = if token.first() == Some(&b'"') {
            let t = &token[1..token.len().min(128)];
            t.strip_suffix(b"\"").unwrap_or(t)
        } else {
            token
        };
        self.records.find(name).map(|r| r.id.clone())
    }

    /// CheckFuncID1: whether an ID argument names a record of a type it accepts.
    fn check_func_id1(&mut self, flags: u64, id: &[u8], optional: bool) -> bool {
        let name: Vec<u8> = if id.first() == Some(&b'"') {
            let t = &id[1..id.len().min(256)];
            t.strip_suffix(b"\"").unwrap_or(t).to_vec()
        } else {
            id[..id.len().min(255)].to_vec()
        };
        let r = self.records;
        let typed = |bit: u64, t: &[u8; 4]| flags & bit != 0 && r.find_type(&name, t).is_some();
        if typed(A_EFFECTID, b"MGEF")
            || typed(A_LEVELCID, b"LEVC")
            || typed(A_LEVELIID, b"LEVI")
            || typed(A_CREATUREID, b"CREA")
            || typed(A_NPCID, b"NPC_")
            || typed(A_SPELLID, b"SPEL")
            || typed(A_TOPICID, b"DIAL")
            || typed(A_REGIONID, b"REGN")
            || (flags & A_ITEMID != 0 && r.find_carryable(&name).is_some())
            || typed(A_FACTIONID, b"FACT")
            || typed(A_RACEID, b"RACE")
            || typed(A_SOUNDID, b"SOUN")
            || typed(A_SCRIPTID, b"SCPT")
        {
            return true;
        }
        if flags & A_CELLID != 0 && (flags & A_CELLSTR != 0 || r.find_type(&name, b"CELL").is_some()) {
            return true;
        }
        if flags & A_JOURNALID != 0 && r.find_type(&name, b"DIAL").is_some_and(|d| d.dial_type == 4) {
            return true;
        }
        if flags & A_SOULGEMID != 0
            && r.find_type(&name, b"MISC").is_some()
            && name.len() >= 13
            && name[..13].eq_ignore_ascii_case(b"misc_soulgem_")
        {
            return true;
        }
        let rec = r.find(&name).is_some();
        if flags & A_IDMASK == 0 && rec {
            return true;
        }
        if !rec && !optional && !self.add_message("ERROR_BADID", format!("The object ID '{}' is not valid!", text(&name)))
        {
            return false;
        }
        if !optional {
            let msg = format!("ID '{}' is not a {} type!", text(&name), id_type_name(flags));
            if !self.add_message("ERROR_BADID", msg) {
                return false;
            }
        }
        false
    }

    /// GetFuncArgRefType: the two bytes naming an ID argument's kind.
    fn func_arg_ref_type(&self, flags: u64) -> &'static [u8] {
        if self.cur_func.is_some_and(|f| f.opcode == 0x114E) {
            return b" r";
        }
        for (bit, code) in [
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
        ] {
            if flags & bit != 0 {
                return code;
            }
        }
        b" r"
    }

    /// GetAnimGroupID.
    fn anim_group_id(&self, name: &[u8]) -> i16 {
        data::ANIM_GROUPS
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map_or(-1, |&(_, new, old)| if self.allow_tribunal { new } else { old })
    }

    // --- Locals ---------------------------------------------------------------------------

    /// FindLocalVar.
    fn find_local_var(&self, name: &[u8]) -> bool {
        self.locals.iter().flatten().any(|v| v.eq_ignore_ascii_case(name))
    }

    /// FindLocalVarIndex: (1-based index, type char) or (-1, 0).
    fn find_local_var_index(&mut self, name: &[u8]) -> (i64, u8) {
        for (k, kind) in b"slf".iter().copied().enumerate() {
            if let Some(i) = self.locals[k].iter().position(|v| v.eq_ignore_ascii_case(name)) {
                if i == SPECIAL_LOCAL_INDEX {
                    let word = ["short", "long", "float"][k];
                    let msg = format!(
                        "Use of 34th local {word} variable '{}' may result in errors and is not recommended!",
                        text(name)
                    );
                    self.add_message("WARNING_LOCALVAR34", msg);
                }
                return (i as i64 + 1, kind);
            }
        }
        (-1, 0)
    }

    /// The type byte MWEdit would write for an unknown local (its stack garbage).
    fn kind_byte(kind: u8) -> u8 {
        if kind == 0 { 0xCC } else { kind }
    }

    /// AddLocalVar.
    fn add_local_var(&mut self, name: &[u8], k: usize) -> bool {
        if name.len() > VAR_MAXLENGTH {
            let msg =
                format!("Local variable name exceeds the maximum length of {VAR_MAXLENGTH} ({})!", text(name));
            self.add_message("ERROR_BADLOCAL", msg);
            return false;
        }
        if self.locals[k].len() >= MAX_LOCALVARS {
            let word = ["short", "long", "float"][k];
            self.add_message("ERROR_BADLOCAL", format!("Exceeded the maximum number of local {word} variables (255)!"));
            return false;
        }
        self.locals[k].push(name.to_vec());
        true
    }

    // --- Tokenizer ------------------------------------------------------------------------

    /// GetNextToken.
    fn get_next_token(&mut self) -> i32 {
        self.last_token = self.token.clone();
        if Self::ctype(self.ch(0)) & CT_SPACE != 0 {
            self.skip_white_space();
        }
        if self.ch(0) == CHAR_COMMENT {
            self.skip_comment();
        }
        self.token_parsed = false;
        let c = self.ch(0);
        if c == CHAR_EOL {
            self.token_id = TK_EOL;
            self.token.clear();
            self.char_pos = 0;
            self.line += 1;
            self.pos += 1;
            self.line_has_ref = false;
            self.last_line_pos = self.size;
        } else if c == 0 {
            self.token_id = TK_EOS;
            self.token.clear();
        } else if c == CHAR_STRING {
            return self.get_string_token();
        } else if Self::ctype(c) & CT_SYMBOLF != 0 {
            return self.get_symbol_token();
        } else if Self::ctype(c) & CT_DIGIT != 0 {
            return self.get_number_token();
        } else if Self::ctype(c) & CT_PUNCT != 0 {
            return self.get_operator_token();
        } else {
            self.add_message("ERROR_BADTOKEN", format!("Unknown token type character '{}' found!", c as char));
            return ERROR;
        }
        OK
    }

    /// SkipTokenWhiteSpace.
    fn skip_white_space(&mut self) {
        loop {
            self.pos += 1;
            self.char_pos += 1;
            if Self::ctype(self.ch(0)) & CT_SPACE == 0 {
                break;
            }
        }
    }

    /// SkipTokenComment.
    fn skip_comment(&mut self) {
        loop {
            self.pos += 1;
            self.char_pos += 1;
            if matches!(self.ch(0), CHAR_EOL | 0) {
                break;
            }
        }
    }

    fn advance_number(&mut self) {
        loop {
            self.pos += 1;
            self.char_pos += 1;
            let c = self.ch(0);
            if c == 0 || c_isspace(c) || c_ispunct(c) {
                break;
            }
        }
    }

    /// GetNumberToken.
    fn get_number_token(&mut self) -> i32 {
        let start = self.pos;
        self.advance_number();
        if self.ch(0) == b'.' {
            self.advance_number();
        }
        self.token_id = TK_NUMBER;
        self.token = self.text[start..self.pos].to_vec();
        if !is_string_float(&self.token) {
            let msg = format!("Invalid number string '{}' found!", text(&self.token));
            if !self.add_message("ERROR_BADNUMBER", msg) {
                return ERROR;
            }
        }
        OK
    }

    /// GetOperatorToken.
    fn get_operator_token(&mut self) -> i32 {
        let two = [self.ch(0), self.ch(1)];
        self.token_id = if two[1] != 0 { op_token(&two) } else { TK_UNKNOWN };
        if self.token_id != TK_UNKNOWN {
            self.token = two.to_vec();
            self.pos += 2;
            self.char_pos += 2;
        } else {
            self.token_id = op_token(&two[..1]);
            self.token = vec![two[0]];
            self.pos += 1;
            self.char_pos += 1;
        }
        if self.token_id == TK_VAROP || self.token_id == TK_FUNCOP {
            if self.line_has_ref {
                self.add_message("ERROR_MULTREF", "Multiple object references found on line!".into());
                return ERROR;
            }
            self.line_has_ref = true;
        } else if self.token_id == TK_OPENBRAC || self.token_id == TK_CLOSEBRAC {
            if !c_isspace(self.ch(-2)) || !c_isspace(self.ch(0)) {
                self.add_message(
                    "WARNING_NOSPACE",
                    "Open/closing bracket missing space characters on one/both sides!".into(),
                );
            }
        } else if self.token_id == TK_RELOP {
            let back = if self.token.len() == 1 { -2 } else { -3 };
            if (!c_isspace(self.ch(back)) || !c_isspace(self.ch(0)))
                && !self.add_message(
                    "WARNING_NOSPACE",
                    "Comparison operator missing space characters on one/both sides!".into(),
                )
            {
                return ERROR;
            }
        }
        OK
    }

    /// GetStringToken.
    fn get_string_token(&mut self) -> i32 {
        let start = self.pos;
        let start_pos = self.char_pos;
        loop {
            self.pos += 1;
            self.char_pos += 1;
            if matches!(self.ch(0), b'"' | 0 | CHAR_EOL) {
                break;
            }
        }
        if self.ch(0) != b'"' {
            let msg =
                format!("Bad String, no terminating \" found on line for string starting at position {start_pos}!");
            self.add_message("ERROR_BADTOKEN", msg);
            return ERROR;
        }
        self.pos += 1;
        self.char_pos += 1;
        self.token = self.text[start..self.pos].to_vec();
        self.token_id = TK_STRING;
        OK
    }

    /// GetSymbolToken.
    fn get_symbol_token(&mut self) -> i32 {
        let start = self.pos;
        loop {
            self.pos += 1;
            self.char_pos += 1;
            if Self::ctype(self.ch(0)) & CT_SYMBOL == 0 {
                break;
            }
        }
        self.token = self.text[start..self.pos].to_vec();
        self.token_id = TK_SYMBOL;
        let res = reserved(&self.token);
        if res != TK_UNKNOWN {
            self.token_id = res;
            return OK;
        }
        if self.find_local_var(&self.token) {
            self.token_id = TK_LOCALVAR;
            return OK;
        }
        let records = self.records;
        if let Some(glob) = records.get_global(&self.token) {
            self.token = glob.id.clone();
            self.token_id = TK_GLOBALVAR;
            return OK;
        }
        if let Some(func) = find_function(&self.token) {
            if self.cur_func.is_some() {
                let msg = format!("Cannot call the function '{}' from within another function!", text(&self.token));
                self.add_message("ERROR_BADTOKEN", msg);
                return ERROR;
            }
            self.cur_func = Some(func);
            self.num_func_args = 0;
            self.last_func_arg = 0;
            self.func_arg_index = 0;
            self.num_msg_buttons = 0;
            self.num_msg_args = 0;
            self.many_arg_pos = 0;
            self.token_id = TK_FUNCTION;
            if func.flags & F_BLOODMOON != 0 && !self.allow_bloodmoon {
                let msg = format!("The Bloodmoon function '{}' may not be supported!", text(&self.token));
                if !self.add_message("WARNING_EXPANSIONFUNC", msg) {
                    return ERROR;
                }
            }
            if func.flags & F_TRIBUNAL != 0 && !self.allow_tribunal {
                let msg = format!("The Tribunal function '{}' may not be supported!", text(&self.token));
                if !self.add_message("WARNING_EXPANSIONFUNC", msg) {
                    return ERROR;
                }
            }
            return OK;
        }
        if let Some(func) = self.custom.get(&self.token.to_ascii_lowercase()).copied() {
            self.cur_func = Some(func);
            self.num_func_args = 0;
            self.last_func_arg = 0;
            self.func_arg_index = 0;
            self.num_msg_buttons = 0;
            self.num_msg_args = 0;
            self.many_arg_pos = 0;
            self.token_id = data::TOKEN_FUNCTIONX;
            if !self.allow_extended {
                let msg = format!("Extended function '{}' may not be supported!", text(&self.token));
                if !self.add_message("WARNING_EXTFUNC", msg) {
                    return ERROR;
                }
            }
        }
        OK
    }

    // --- Table engine -----------------------------------------------------------------------

    /// ParseTable.
    fn parse_table(&mut self, table: usize) -> i32 {
        let rows: &'static [Row] = data::TABLES[table];
        let mut index = 0;
        while rows[index].token != ENDTABLE {
            let entry = &rows[index];
            if entry.flags & MANY != 0 {
                let result = self.parse_table_many(entry);
                if result != OK {
                    return result;
                }
            } else if entry.flags & ONE != 0 {
                let result = self.parse_table_one(entry);
                if result == TABLEEND || result == BLOCKEND {
                    return OK;
                }
                if result != OK {
                    return result;
                }
            }
            index += 1;
        }
        let end = &rows[index];
        if let Some(out) = end.output {
            self.call(out);
        }
        if end.flags & STOP != 0 {
            if let Some(parse) = end.parse {
                let result = self.call(parse);
                if result != OK {
                    return result;
                }
            }
            if end.flags & OPT != 0 {
                return BLOCKEND;
            }
            let msg = format!("Unknown token '{}'({}) found!", text(&self.token), token_name(self.token_id));
            self.add_message("ERROR_BADTOKEN", msg);
            return ERROR;
        }
        OK
    }

    /// ParseTableMany.
    fn parse_table_many(&mut self, entry: Entry) -> i32 {
        let mut result = OK;
        while result == OK {
            result = self.parse_table_one(entry);
        }
        if result == BLOCKEND { OK } else { result }
    }

    /// ParseTableOne.
    fn parse_table_one(&mut self, entry: Entry) -> i32 {
        let mut result = OK;
        let mut output = true;
        if self.token_parsed {
            result = self.get_next_token();
            if result != OK {
                return result;
            }
        }
        if entry.token == BEGINBLOCK {
            if let Some(parse) = entry.parse {
                result = self.call(parse);
            }
            if let Some(sub) = entry.sub
                && result == OK
            {
                result = self.parse_table(sub);
            }
            if result == OK && entry.flags & STOP != 0 {
                result = TABLEEND;
            }
        } else if entry.token == self.token_id {
            self.token_parsed = true;
            if let Some(out) = entry.output {
                self.call(out);
            }
            if let Some(parse) = entry.parse {
                result = self.call(parse);
            }
            if let Some(sub) = entry.sub
                && result == OK
            {
                result = self.parse_table(sub);
            }
            if result == OK && entry.flags & STOP != 0 {
                result = TABLEEND;
            }
            output = false;
        } else if entry.flags & MANY != 0 {
            result = BLOCKEND;
        } else if entry.flags & OPT != 0 {
            output = false;
            result = OK;
        } else {
            result = self.assert_token(entry.token);
            if result == OK {
                output = false;
            }
        }
        if output
            && result == OK
            && let Some(out) = entry.output
        {
            self.call(out);
        }
        result
    }

    /// Runs a table row's routine.
    fn call(&mut self, action: Action) -> i32 {
        use Action as A;
        match action {
            A::OutputElse => self.output_else(),
            A::OutputElseIf => self.output_else_if(),
            A::OutputEnd => {
                self.statement_count += 1;
                self.output_token(0x0101)
            }
            A::OutputEndIf => self.output_end_if(),
            A::OutputEndWhile => self.output_end_while(),
            A::OutputExprStack | A::OutputIfLeftExprStack => self.output_expr_stack(),
            A::OutputFuncArgGlobal => self.output_func_arg_global(),
            A::OutputFuncArgLocal => self.output_func_arg_local(),
            A::OutputFuncArgNum => self.output_func_arg_num(),
            A::OutputFuncArgReset => self.output_func_arg_reset(),
            A::OutputFuncArgXYZ => {
                let c = self.token.first().copied().unwrap_or(0).to_ascii_uppercase();
                self.add(&[c]);
                0
            }
            A::OutputFuncOp => {
                let ident = self.last_token.clone();
                self.write_func_op(&ident)
            }
            A::OutputFuncXBlock => self.output_func_x_block(),
            A::OutputFunction => self.output_function(),
            A::OutputIf => {
                self.open_condition(0x0106);
                self.last_set_negative = false;
                self.last_func_arg_symbol = false;
                self.last_set_symbol = false;
                0
            }
            A::OutputIfFinish => self.finish_condition(MAX_IFEXPRESSIONS, "IF"),
            A::OutputIfFunction => {
                self.add(b" X");
                self.output_function()
            }
            A::OutputIfGlobal => {
                self.add_global(b" G");
                0
            }
            A::OutputIfLocal => {
                let token = self.token.clone();
                let (index, kind) = self.find_local_var_index(&token);
                self.add_short(0x20 + (i64::from(kind) << 8));
                self.add_short(index);
                0
            }
            A::OutputIfRAddOp | A::OutputIfRNumber => {
                let t = self.token.clone();
                self.add(&t);
                0
            }
            A::OutputIfRGlobal => {
                self.add_global(b"G");
                self.add(&[0]);
                0
            }
            A::OutputIfRLocal => {
                let token = self.token.clone();
                let (index, kind) = self.find_local_var_index(&token);
                self.add(&[Self::kind_byte(kind)]);
                self.add_short(index);
                self.add(&[0]);
                0
            }
            A::OutputIfRelOp => {
                let mut op = vec![b' '];
                op.extend_from_slice(&self.token[..self.token.len().min(13)]);
                self.add(&op);
                self.is_empty_if = false;
                0
            }
            A::OutputIfRightExprStack => self.output_if_right_expr_stack(),
            A::OutputLetEnd => self.output_let_end(),
            A::OutputLineFunction => {
                self.statement_count += 1;
                self.output_function()
            }
            A::OutputOneExpr => 0,
            A::OutputReturn => {
                self.statement_count += 1;
                self.output_token(0x0124)
            }
            A::OutputScriptName => {
                self.name = self.token.clone();
                0
            }
            A::OutputSet => self.output_set(),
            A::OutputSetEnd => self.output_set_end(),
            A::OutputSetGlobal => self.output_set_global(),
            A::OutputSetLocal => {
                let token = self.token.clone();
                let (index, kind) = self.find_local_var_index(&token);
                self.add(&[Self::kind_byte(kind)]);
                self.add_short(index);
                self.add(&[0]);
                self.last_set_pos = self.size as i64 - 1;
                0
            }
            A::OutputWhile => {
                self.open_condition(0x010A);
                self.last_set_negative = false;
                OK
            }
            A::OutputWhileFinish => self.finish_condition(255, "WHILE"),
            A::OutputXElse => self.output_x_else(),
            A::OutputXElseIf => {
                self.output_x_else();
                self.output_x_if()
            }
            A::OutputXEndIf => {
                if let Some(block) = self.x_if_stack.pop() {
                    let at = (self.size as i16).to_le_bytes();
                    self.put(block.pos, &at);
                }
                0
            }
            A::OutputXEndWhile => self.output_x_end_while(),
            A::OutputXIf => self.output_x_if(),
            A::OutputXWhile => {
                self.x_if_stack.push(IfBlock { pos: self.size, start_count: 0 });
                self.x_test();
                0
            }
            A::ParseCheckFuncArg => self.parse_check_func_arg(),
            A::ParseFuncAddOp => {
                self.last_token_negative = self.token == b"-";
                0
            }
            A::ParseFuncArg1 => self.parse_func_arg1(),
            A::ParseFuncArgX => self.parse_func_arg_x(),
            A::ParseFuncComma => self.parse_func_comma(),
            A::ParseFuncEnd => self.parse_func_end(),
            A::ParseFuncRef => {
                self.token_parsed = true;
                self.parse_table(data::LEFT_FUNC_BLOCK)
            }
            A::ParseFunction => {
                if self.cur_func.is_some_and(|f| f.opcode == 0x10C9) {
                    self.parse_choice_function()
                } else {
                    OK
                }
            }
            A::ParseFunctionLine => {
                self.statement_count += 1;
                OK
            }
            A::ParseIfRExprStart => {
                self.last_set_symbol = true;
                self.last_set_pos = self.size as i64 - 1;
                self.expr_stack.clear();
                OK
            }
            A::ParseIfVarRef => self.parse_var_ref1(true),
            A::ParseLineStringID => {
                self.statement_count += 1;
                self.parse_string_id()
            }
            A::ParseLineSymbolID => {
                self.statement_count += 1;
                self.parse_symbol_id()
            }
            A::ParseNewLocalVar => self.parse_new_local_var(),
            A::ParsePushToken => self.parse_push_token(),
            A::ParseRExprStart => {
                self.expr_stack.clear();
                OK
            }
            A::ParseSetFirstAddOp => {
                self.last_set_negative = self.token == b"-";
                0
            }
            A::ParseStringIDIf => self.string_global(A::OutputIfGlobal),
            A::ParseStringIDPush => self.string_global(A::ParsePushToken),
            A::ParseStringIDSet => self.string_global(A::OutputSetGlobal),
            A::ParseSymbolID => self.parse_symbol_id(),
            A::ParseVarRef => self.parse_var_ref1(false),
            A::PushFuncOp => {
                self.ref_object = self.last_token.clone();
                0
            }
            A::PushFuncXArgLocal => {
                let token = self.token[..self.token.len().min(STACK_MAXTOKEN)].to_vec();
                self.arg_x_stack.push(StackEntry { token_id: A_LOCALVAR, token });
                0
            }
            A::PushFuncXArgNum => self.push_func_x_arg_num(),
            A::PushFuncXArgString => {
                let t = inner(&self.token);
                let token = t[..t.len().min(STACK_MAXTOKEN)].to_vec();
                self.arg_x_stack.push(StackEntry { token_id: A_STRING as i32, token });
                0
            }
            A::PushLetLocal => {
                let token = self.token[..self.token.len().min(STACK_MAXTOKEN)].to_vec();
                self.let_queue.push(StackEntry { token_id: A_LOCALVAR, token });
                0
            }
        }
    }

    // --- Parse routines --------------------------------------------------------------------

    /// ParseNewLocalVar.
    fn parse_new_local_var(&mut self) -> i32 {
        let k = match self.token.to_ascii_lowercase().as_slice() {
            b"short" => 0,
            b"long" => 1,
            b"float" => 2,
            _ => {
                let msg = format!("Unknown type operator '{}'!", text(&self.token));
                self.add_message("ERROR_BADTOKEN", msg);
                return ERROR;
            }
        };
        let result = self.get_next_token();
        if result != OK {
            return result;
        }
        let t = text(&self.token);
        match self.token_id {
            TK_SYMBOL => {}
            TK_GLOBALVAR => {
                let msg = format!("Local variable '{t}' conflicts with a global variable.");
                if !self.add_message("WARNING_IDCONFLICT", msg) {
                    return ERROR;
                }
            }
            TK_LOCALVAR => {
                self.add_message("ERROR_BADLOCAL", format!("Local variable '{t}' has already been defined!"));
                return ERROR;
            }
            _ => {
                self.add_message("ERROR_BADLOCAL", format!("Invalid local variable name '{t}'!"));
                return ERROR;
            }
        }
        if self.token.first() == Some(&b'_')
            && !self.add_message(
                "WARNING_BADVARSTART",
                format!("Local/Global variables should not start with an '_' ({t})."),
            )
        {
            return ERROR;
        }
        let name = self.token.clone();
        if !self.add_local_var(&name, k) {
            return ERROR;
        }
        let result = self.get_next_token();
        if result != OK {
            return result;
        }
        let result = self.assert_token(TK_EOL);
        if result != OK {
            return result;
        }
        self.token_parsed = true;
        self.statement_count += 1;
        OK
    }

    /// ParseVarRef1: `id.variable`, as a set target or in an if/while expression.
    fn parse_var_ref1(&mut self, in_if: bool) -> i32 {
        let mut var_token = self.last_token.clone();
        if var_token.first() == Some(&b'"') {
            var_token = inner(&var_token).to_vec();
        }
        let result = self.get_next_token();
        if result != OK {
            return result;
        }
        if !matches!(self.token_id, TK_SYMBOL | TK_LOCALVAR | TK_GLOBALVAR | TK_FUNCTION) {
            let msg = format!("Expecting an object variable name but found '{}'!", token_name(self.token_id));
            self.add_message("ERROR_BADTOKEN", msg);
            return ERROR;
        }
        let records = self.records;
        let Some(found) = records.find(&var_token) else {
            self.add_message("ERROR_BADID", format!("Unknown object id '{}' found!", text(&var_token)));
            return ERROR;
        };
        let mut rec: &Record = found;
        if in_if {
            if self.last_set_negative {
                self.add(b" -1");
            }
            self.add(b" ");
        }
        if &rec.rtype != b"SCPT" {
            if rec.script.is_empty() {
                self.add_message("ERROR_BADID", format!("Object '{}' has no script assigned!", text(&var_token)));
                return ERROR;
            }
            let script_name = rec.script.clone();
            let Some(script) = records.find(&script_name) else {
                self.add_message("ERROR_BADID", format!("Invalid script name '{}' found!", text(&script_name)));
                return ERROR;
            };
            rec = script;
            self.add(b"r");
        } else {
            self.add(b"m");
        }
        let n = var_token.len() & 0xFF;
        self.add(&[n as u8]);
        self.add(&var_token[..n]);
        let (index, kind) = if &rec.rtype == b"SCPT" { rec.find_local(&self.token) } else { (-1, 0) };
        if index <= 0 {
            let msg = format!("Object '{}' has no local variable '{}'!", text(&var_token), text(&self.token));
            self.add_message("ERROR_BADID", msg);
            return ERROR;
        }
        self.add(&[kind]);
        self.add_short(i64::from(index));
        if in_if {
            if self.last_set_negative {
                self.add(b" *");
                self.last_set_negative = false;
            }
        } else {
            self.add(&[0]);
        }
        self.last_set_pos = self.size as i64 - 1;
        self.token_parsed = true;
        OK
    }

    /// ParseStringID: a quoted object ID.
    fn parse_string_id(&mut self) -> i32 {
        let found = self.is_symbol_id(inner(&self.token)).is_some();
        self.token_parsed = true;
        if !found && !self.add_message("ERROR_BADID", format!("Unknown object ID {} found!", text(&self.token))) {
            return ERROR;
        }
        OK
    }

    /// ParseStringIDSet/If/Push: a quoted global (written by `then`), else an object ID.
    fn string_global(&mut self, then: Action) -> i32 {
        let name = inner(&self.token).to_vec();
        if self.records.get_global(&name).is_none() {
            return self.parse_string_id();
        }
        self.token = name;
        self.token_id = TK_GLOBALVAR;
        self.call(then);
        TABLEEND
    }

    /// ParseSymbolID: an object ID, its case taken from the record.
    fn parse_symbol_id(&mut self) -> i32 {
        match self.is_symbol_id(&self.token) {
            Some(id) => self.token = id,
            None => {
                let msg = format!("Unknown object ID '{}' found!", text(&self.token));
                if !self.add_message("ERROR_BADID", msg) {
                    return ERROR;
                }
            }
        }
        self.token_parsed = true;
        OK
    }

    /// The current function (the grammar only asks inside a call).
    fn func(&self) -> Func {
        self.cur_func.expect("a function is being compiled")
    }

    /// Refusal shared by CheckFuncArg's cases: `Some(code)` to return, `None` to go on.
    fn refuse(&mut self, no_message: bool, optional: bool, id: &str, msg: String) -> Option<i32> {
        if no_message {
            return Some(ARG_ERROR);
        }
        if optional {
            return Some(ARG_ENDTABLE);
        }
        if !self.add_message(id, msg) {
            return Some(ARG_ERROR);
        }
        None
    }

    /// CheckFuncArg: whether the current token fits the current argument.
    fn check_func_arg(&mut self) -> i32 {
        let Some(func) = self.cur_func.filter(|_| self.func_arg_index < MAX_FUNCARGS) else {
            let msg = format!("Exceeded the maximum of {MAX_FUNCARGS} function arguments!");
            self.add_message("ERROR_TOOMANYARGS", msg);
            return ARG_ERROR;
        };
        let flags = func.var[self.func_arg_index];
        let nm = flags & A_MANY != 0;
        let op = flags & A_OPTIONAL != 0;
        let n = self.num_func_args;
        let tok = text(&self.token);
        if flags & A_VARMASK == 0 {
            if !self.add_message("ERROR_BADFUNCARG", format!("Function does not accept argument #{n}!")) {
                return ARG_ERROR;
            }
            return ARG_OK;
        }
        if flags & A_OPTSTART != 0 {
            self.func_opt_pos = self.size;
            self.func_opt_count = self.num_func_args;
            self.add(&[0, 0]);
        }
        macro_rules! refuse {
            ($id:expr, $msg:expr) => {
                if let Some(code) = self.refuse(nm, op, $id, $msg) {
                    return code;
                }
            };
        }
        match self.token_id {
            TK_RESET => {
                if flags & A_RESET == 0 {
                    refuse!("WARNING_BADFUNCARG", format!("Function does not accept 'reset' for argument #{n}!"));
                }
            }
            TK_NUMBER => {
                if flags & A_EFFECT != 0 {
                    let v = atol(&self.token);
                    if !(0..EFFECT_MAX).contains(&v) {
                        refuse!("ERROR_BADFUNCARG", format!("Invalid magic effect id '{tok}'!"));
                    } else {
                        self.output_effect(v);
                        self.last_func_arg_symbol = false;
                        self.last_set_symbol = false;
                    }
                } else if flags & A_RESET != 0 {
                } else if flags & A_NUMBER == 0 {
                    refuse!("ERROR_BADFUNCARG", format!("Function does not accept a number for argument #{n}!"));
                } else if func.opcode == OPCODE_MESSAGEBOX
                    && self.many_arg_pos == 0
                    && !self
                        .add_message("ERROR_BADFUNCARG", "MessageBox function does not accept a number variable!".into())
                {
                    return ARG_ERROR;
                }
            }
            TK_STRING => {
                if flags & A_ID != 0 {
                    let token = self.token.clone();
                    if !self.check_func_id1(flags, &token, false) {
                        return ARG_ERROR;
                    }
                    let name = inner(&self.token).to_vec();
                    self.id_arg(&name);
                } else if flags & A_ANIM != 0 {
                    let temp = self.token[..self.token.len().min(255)].to_vec();
                    let group = unquote(&temp).to_vec();
                    let anim = self.anim_group_id(&group);
                    if anim < 0 {
                        refuse!("ERROR_BADFUNCARG", format!("The '{}' is not a valid animation group!", text(&group)));
                    }
                    self.add_short(i64::from(anim));
                } else if flags & A_STRING == 0 {
                    refuse!("ERROR_BADFUNCARG", format!("Function does not accept a string for argument #{n}!"));
                } else {
                    self.output_func_arg_string();
                }
            }
            TK_XYZ => {
                if flags & A_XYZ == 0 {
                    refuse!(
                        "ERROR_BADFUNCARG",
                        format!("Function does not accept an X/Y/Z symbol for argument #{n}!")
                    );
                }
            }
            TK_SYMBOL => {
                if flags & A_EFFECT != 0 {
                    let effect = find_effect(&self.token);
                    if effect < 0 {
                        refuse!("ERROR_BADFUNCARG", format!("Invalid magic effect id '{tok}'!"));
                    } else {
                        self.output_effect(effect);
                        self.last_func_arg_symbol = false;
                        self.last_set_symbol = true;
                    }
                } else if flags & A_ANIM != 0 {
                    let token = self.token.clone();
                    let anim = self.anim_group_id(&token);
                    if anim < 0 {
                        refuse!("ERROR_BADFUNCARG", format!("The symbol '{tok}' is not a valid animation group!"));
                    }
                    self.add_short(i64::from(anim));
                } else if flags & A_RESET != 0 {
                    if self.token.eq_ignore_ascii_case(b"reset") {
                        self.output_func_arg_reset();
                    } else if !self
                        .add_message("ERROR_BADFUNCARG", format!("The '{tok}' is not a valid reset parameter!"))
                    {
                        return ARG_ERROR;
                    }
                } else if flags & A_ID == 0 {
                    refuse!("ERROR_BADFUNCARG", format!("Function does not accept a symbol for argument #{n}!"));
                } else {
                    let token = self.token.clone();
                    if !self.check_func_id1(flags, &token, false) {
                        return ARG_ERROR;
                    }
                    self.id_arg(&token);
                }
            }
            TK_GLOBALVAR | TK_LOCALVAR => {
                if self.token_id == TK_GLOBALVAR && func.flags & F_ALLOWGLOBAL == 0 {
                    refuse!("WARNING_BADFUNCVAR", "Function does not accept global variables!".into());
                }
                if flags & A_RESET != 0 {
                    if self.token.eq_ignore_ascii_case(b"reset") {
                        self.output_func_arg_reset();
                    } else if !self.add_message(
                        "WARNING_BADFUNCVAR",
                        format!("Function does not accept variable ({tok}) for a reset parameter!"),
                    ) {
                        return ARG_ERROR;
                    }
                } else if func.flags & F_VAR == 0 {
                    refuse!("WARNING_BADFUNCVAR", "Function does not accept variable input!".into());
                } else if flags & A_NUMBER == 0 {
                    refuse!("ERROR_BADFUNCARG", format!("Function does not accept a number for argument #{n}!"));
                }
            }
            other => {
                refuse!(
                    "ERROR_BADFUNCARG",
                    format!("Unknown function argument #{n} '{tok}'({}) found !", token_name(other))
                );
            }
        }
        ARG_OK
    }

    fn missing_comma(&mut self) -> bool {
        if self.last_func_arg == self.num_func_args {
            self.num_func_args += 1;
            let msg = format!("Missing ',' between arguments {} and {}!", self.num_func_args - 1, self.num_func_args);
            return self.add_message("WARNING_NOCOMMA", msg);
        }
        true
    }

    /// ParseFuncArg1: one function argument.
    fn parse_func_arg1(&mut self) -> i32 {
        let mut check_msg_args = false;
        self.token_parsed = true;
        if !self.missing_comma() {
            return ERROR;
        }
        let mut result = self.check_func_arg();
        let Some(func) = self.cur_func.filter(|_| self.func_arg_index < MAX_FUNCARGS) else {
            return ARG_ERROR;
        };
        if func.var[self.func_arg_index] & A_MANY != 0 {
            if result == ARG_ERROR {
                self.many_arg_pos += 1;
                self.func_arg_index += 1;
                if func.opcode == OPCODE_MESSAGEBOX {
                    check_msg_args = true;
                    self.count_msg_arg();
                }
                result = self.check_func_arg();
                if result == ARG_ERROR {
                    let msg = format!(
                        "Unknown or invalid function argument #{} '{}'({}) found !",
                        self.num_func_args,
                        text(&self.token),
                        token_name(self.token_id)
                    );
                    self.add_message("ERROR_BADFUNCARG", msg);
                    return ERROR;
                }
            } else if func.opcode == OPCODE_MESSAGEBOX {
                check_msg_args = true;
                self.count_msg_arg();
            }
            if check_msg_args {
                if self.many_arg_pos == 0
                    && self.num_msg_args > MAX_MSGARGS
                    && !self.add_message(
                        "ERROR_BADFUNCARG",
                        format!("Exceeded the maximum of {MAX_MSGARGS} MessageBox variables!"),
                    )
                {
                    return ERROR;
                }
                if self.many_arg_pos == 1
                    && self.num_msg_buttons > MAX_MSGBUTTONS
                    && !self.add_message(
                        "ERROR_BADFUNCARG",
                        format!("Exceeded the maximum of {MAX_MSGBUTTONS} MessageBox buttons!"),
                    )
                {
                    return ERROR;
                }
            }
            self.last_func_arg = self.num_func_args;
            return OK;
        }
        if result == ARG_ERROR {
            return ERROR;
        }
        if result == ARG_ENDTABLE {
            return TABLEEND;
        }
        self.last_func_arg = self.num_func_args;
        self.func_arg_index += 1;
        OK
    }

    fn count_msg_arg(&mut self) {
        if self.many_arg_pos == 0 {
            self.num_msg_args += 1;
        } else if self.many_arg_pos == 1 {
            self.num_msg_buttons += 1;
        }
    }

    /// ParseFuncArgX: one extended-function argument (checked only; written later).
    fn parse_func_arg_x(&mut self) -> i32 {
        self.token_parsed = true;
        if !self.missing_comma() {
            return ERROR;
        }
        if self.cur_func.is_none() || self.func_arg_index >= MAX_FUNCARGS {
            return ARG_ERROR;
        }
        self.last_func_arg = self.num_func_args;
        self.func_arg_index += 1;
        OK
    }

    /// ParseChoiceFunction: `Choice "text", 1, "text", 2`, written as text.
    fn parse_choice_function(&mut self) -> i32 {
        let mut output_size = 0usize;
        let mut arg_count = 0;
        let size_offset = self.size;
        self.add(&[0, 0]);
        loop {
            if self.token_parsed {
                let result = self.get_next_token();
                if result != OK {
                    return result;
                }
            }
            match self.token_id {
                TK_EOL => {
                    self.token_parsed = true;
                    break;
                }
                TK_COMMA => self.token_parsed = true,
                TK_NUMBER | TK_STRING => {
                    if self.token_id == TK_NUMBER && self.token.contains(&b'.') {
                        let msg = format!("Invalid float number '{}' in Choice function call!", text(&self.token));
                        self.add_message("ERROR_BADNUMBER", msg);
                    }
                    self.token_parsed = true;
                    let t = self.token.clone();
                    output_size += t.len();
                    if arg_count > 0 {
                        output_size += 1;
                        self.add(b" ");
                    }
                    arg_count += 1;
                    self.add(&t);
                }
                _ => {
                    let msg = format!("Invalid argument '{}' in Choice function call!", text(&self.token));
                    self.add_message("ERROR_BADFUNCARG", msg);
                    self.token_parsed = true;
                }
            }
        }
        if output_size > 65535 {
            self.add_message(
                "ERROR_TOOMANYARGS",
                "Arguments in Choice function exceed maximum length of 65535 bytes!".into(),
            );
            return ERROR;
        }
        self.put(size_offset, &(output_size as u16).to_le_bytes());
        // Wraithguard: the call is over at the end of its line. MWEdit leaves its function
        // set here (only ParseFuncEnd clears it), so any function on a later line - a
        // `StartScript` after a `Choice` in a dialogue result - was refused as "called from
        // within another function"; the Construction Set accepts it, and so does this.
        self.cur_func = None;
        TABLEEND
    }

    /// ParseCheckFuncArg: before each argument, end the call if the function takes no more.
    fn parse_check_func_arg(&mut self) -> i32 {
        let Some(func) = self.cur_func else { return OK };
        if self.num_func_args > MAX_FUNCARGS as i32 || self.func_arg_index > MAX_FUNCARGS {
            return OK;
        }
        let flags = func.var.get(self.func_arg_index).copied().unwrap_or(0);
        if flags == 0 {
            if self.token_id == TK_COMMA {
                let msg = format!("Function does not accept argument #{}!", self.func_arg_index + 1);
                if !self.add_message("WARNING_BADFUNCARG", msg) {
                    return ERROR;
                }
            }
            let result = self.parse_func_end();
            if result < 0 {
                return result;
            }
            return TABLEEND;
        }
        OK
    }

    /// ParseFuncComma: a comma between arguments (an empty one skips an optional one).
    fn parse_func_comma(&mut self) -> i32 {
        if let Some(func) = self.cur_func
            && self.last_func_arg != self.num_func_args
        {
            if self.func_arg_index < MAX_FUNCARGS && func.var[self.func_arg_index] & A_OPTIONAL == 0 {
                let msg = format!("Function argument #{} is not optional!", self.func_arg_index + 1);
                if !self.add_message("WARNING_BADFUNCARG", msg) {
                    return ERROR;
                }
            }
            if self.func_arg_index < MAX_FUNCARGS && func.var[self.func_arg_index] & A_MANY == 0 {
                self.func_arg_index += 1;
            }
            self.last_func_arg += 1;
        }
        self.num_func_args += 1;
        OK
    }

    /// ParseFuncEnd: defaults for omitted arguments, then the call's tail.
    fn parse_func_end(&mut self) -> i32 {
        let Some(func) = self.cur_func else { return OK };
        if self.num_func_args == 0 {
            self.num_func_args = 1;
        }
        let fname = text(func.name);
        if func.flags & F_BAD != 0
            && !self.add_message("WARNING_BADFUNCTION", format!("The function '{fname}' is known to not work in Morrowind!"))
        {
            self.cur_func = None;
            return ERROR;
        }
        if func.flags & F_DIALOGUE != 0
            && !self.add_message("WARNING_DIALOGFUNC", format!("The function '{fname}' only works in dialogue results!"))
        {
            self.cur_func = None;
            return ERROR;
        }
        for &arg in func.var.iter().skip(self.func_arg_index) {
            if arg & A_VARMASK == 0 {
                break;
            }
            if arg & A_OPTIONAL == 0 && arg & A_NOTREQ == 0 {
                if !self.add_message("ERROR_BADFUNCARG", "Missing required argument(s) for function call!".into()) {
                    self.cur_func = None;
                    return ERROR;
                }
                self.last_func_arg_symbol = false;
                break;
            }
            if arg & A_RESET != 0 {
                let v = if func.opcode == 0x10F9 && self.func_arg_index > 3 { 1 } else { 0 };
                self.add(&[v]);
            } else if arg & A_BYTE != 0 || (arg & A_ID != 0 && func.flags & F_NOOPTOUT == 0) {
                self.add(&[0]);
            }
            self.last_func_arg_symbol = false;
            if arg & A_OPTSTART != 0 {
                self.add(&[0, 0]);
            }
        }
        if self.func_opt_count > 0 && self.func_opt_pos > 0 {
            let count = (self.num_func_args - self.func_opt_count) as i16;
            self.put(self.func_opt_pos, &count.to_le_bytes());
        }
        self.output_func_end();
        self.cur_func = None;
        OK
    }

    /// ParsePushToken: an expression token. Operands are written, operators stacked.
    fn parse_push_token(&mut self) -> i32 {
        let top_id = self.expr_stack.last().map_or(TK_UNKNOWN, |e| e.token_id);
        let mut push = false;
        let tid = self.token_id;
        if self.last_set_negative && tid != TK_NUMBER {
            self.add(b" -1");
        }
        match tid {
            TK_OPENBRAC => {
                push = true;
                if self.last_set_negative {
                    self.last_set_negative = false;
                    self.expr_stack.push(StackEntry { token_id: TK_MULOP, token: b"*".to_vec() });
                    self.last_set_symbol = false;
                }
            }
            TK_CLOSEBRAC => {
                while let Some(entry) = self.expr_stack.pop() {
                    if entry.token_id == TK_OPENBRAC {
                        break;
                    }
                    self.add(b" ");
                    self.add(&entry.token);
                }
            }
            TK_ADDOP | TK_MULOP => {
                self.last_set_symbol = false;
                self.is_empty_if = false;
                let pops = |id: i32| if tid == TK_ADDOP { id == TK_ADDOP || id == TK_MULOP } else { id == TK_MULOP };
                if pops(top_id) {
                    while self.expr_stack.last().is_some_and(|e| pops(e.token_id)) {
                        let entry = self.expr_stack.pop().expect("checked");
                        self.add(b" ");
                        self.add(&entry.token);
                    }
                }
                push = true;
            }
            TK_NUMBER => {
                self.add(b" ");
                if self.last_set_negative {
                    self.add(b"-");
                    self.last_set_negative = false;
                }
                let t = self.token.clone();
                self.add(&t);
                self.last_set_symbol = false;
                self.is_empty_if = false;
            }
            TK_LOCALVAR => {
                let token = self.token.clone();
                let (index, kind) = self.find_local_var_index(&token);
                self.add(&[b' ', Self::kind_byte(kind)]);
                self.add_short(index);
            }
            TK_GLOBALVAR => self.add_global(b" G"),
            TK_FUNCTION => {
                let opcode = self.func().opcode;
                self.add(b" X");
                self.add_short(i64::from(opcode));
                self.last_func_arg_symbol = false;
                if self.last_set_negative {
                    self.last_set_negative = false;
                    self.expr_stack.push(StackEntry { token_id: TK_MULOP, token: b"*".to_vec() });
                    self.last_set_symbol = false;
                }
            }
            _ => {
                let mut t = vec![b' '];
                t.extend_from_slice(&self.token);
                self.add(&t);
            }
        }
        if self.last_set_negative {
            self.last_set_negative = false;
            self.add(b" *");
            self.last_set_symbol = false;
        }
        if push {
            let token = self.token[..self.token.len().min(STACK_MAXTOKEN)].to_vec();
            self.expr_stack.push(StackEntry { token_id: tid, token });
        }
        self.token_parsed = true;
        OK
    }

    // --- Output routines ----------------------------------------------------------------------

    fn open_condition(&mut self, opcode: i64) {
        let line_pos = self.size;
        self.output_token(opcode);
        self.statement_count += 1;
        self.output_token(0);
        self.last_if_pos = self.size as i64 - 1;
        self.is_empty_if = true;
        self.output_func_id_ref = true;
        self.last_line_pos = line_pos;
    }

    /// OutputElseIf.
    fn output_else_if(&mut self) -> i32 {
        if self.update_last_if_block() == ERROR {
            return ERROR;
        }
        self.open_condition(0x0108);
        0
    }

    /// OutputElse.
    fn output_else(&mut self) -> i32 {
        if self.update_last_if_block() == ERROR {
            return ERROR;
        }
        let line_pos = self.size;
        self.output_token(0x0107);
        self.statement_count += 1;
        self.if_stack.push(IfBlock { pos: self.size, start_count: self.statement_count });
        self.add(&[0]);
        self.last_line_pos = line_pos;
        0
    }

    /// OutputEndIf.
    fn output_end_if(&mut self) -> i32 {
        if self.update_last_if_block() == ERROR {
            return ERROR;
        }
        self.statement_count += 1;
        self.output_token(0x0109)
    }

    /// OutputEndWhile.
    fn output_end_while(&mut self) -> i32 {
        if self.update_last_if_block() == ERROR {
            return ERROR;
        }
        self.statement_count += 1;
        self.output_token(0x010B)
    }

    /// UpdateLastIfBlock: fill in the open block's statement count.
    fn update_last_if_block(&mut self) -> i32 {
        let Some(block) = self.if_stack.pop() else { return OK };
        let count = self.statement_count - block.start_count;
        if count > MAX_IFSTATEMENTS {
            self.add_error(format!("Exceeded the maximum of {MAX_IFSTATEMENTS} statements within an IF block!"));
            return ERROR;
        }
        self.buf[block.pos] = count as u8;
        OK
    }

    /// OutputIfFinish / OutputWhileFinish: fill in the expression length.
    fn finish_condition(&mut self, limit: i32, what: &str) -> i32 {
        if self.last_if_pos < 0 {
            return -1;
        }
        let mut size = self.size as i64 - self.last_if_pos - 1;
        if size > i64::from(limit) {
            self.add_error(format!("Compiled {what} expression length exceeds {limit} bytes!"));
            return -1;
        }
        if self.is_empty_if {
            self.add(&[0]);
            size += 1;
        }
        let at = self.last_if_pos as usize;
        self.buf[at] = size as u8;
        self.if_stack.push(IfBlock { pos: at.saturating_sub(1), start_count: self.statement_count });
        self.last_if_pos = -1;
        self.output_func_id_ref = false;
        0
    }

    /// OutputSet.
    fn output_set(&mut self) -> i32 {
        let line_pos = self.size;
        self.last_set_symbol = true;
        self.last_func_arg_symbol = false;
        self.output_func_id_ref = true;
        self.last_set_negative = false;
        let result = self.output_token(0x0105);
        self.last_line_pos = line_pos;
        self.statement_count += 1;
        result
    }

    /// OutputSetGlobal.
    fn output_set_global(&mut self) -> i32 {
        self.add_global(b"G");
        self.add(&[0]);
        self.last_set_pos = self.size as i64 - 1;
        0
    }

    /// OutputSetEnd: fill in the expression length.
    fn output_set_end(&mut self) -> i32 {
        if self.last_set_pos < 0 {
            return -1;
        }
        let mut size = self.size as i64 - self.last_set_pos - 1;
        if size > 255 {
            self.add_error("Compiled SET expression length exceeds 255 bytes!".into());
            return ERROR;
        }
        if self.last_set_symbol && !self.last_func_arg_symbol {
            self.add(&[0]);
            size += 1;
        }
        let at = self.last_set_pos as usize;
        self.buf[at] = size as u8;
        self.output_func_id_ref = false;
        0
    }

    /// OutputExprStack: write the operators left on the stack.
    fn output_expr_stack(&mut self) -> i32 {
        while let Some(entry) = self.expr_stack.pop() {
            if entry.token_id == TK_OPENBRAC || entry.token_id == TK_CLOSEBRAC {
                break;
            }
            self.add(b" ");
            self.add(&entry.token);
        }
        0
    }

    /// OutputIfRightExprStack.
    fn output_if_right_expr_stack(&mut self) -> i32 {
        let result = self.output_expr_stack();
        if self.last_set_pos >= 0 && self.size as i64 - self.last_set_pos - 1 > 255 {
            self.add_error("Compiled IF expression length exceeds 255 bytes!".into());
            return ERROR;
        }
        if self.last_set_symbol && !self.last_func_arg_symbol {
            self.add(&[0]);
        }
        result
    }

    /// OutputFunction: a function's opcode (after any stored `id->`).
    fn output_function(&mut self) -> i32 {
        let opcode = self.func().opcode;
        self.output_stored_func_op();
        self.add_short(i64::from(opcode));
        self.last_msgbox_args = -1;
        self.last_msgbox_but = -1;
        self.num_msg_args = 0;
        self.num_msg_buttons = 0;
        self.func_opt_count = 0;
        self.last_func_arg_symbol = false;
        0
    }

    /// OutputFuncArgReset.
    fn output_func_arg_reset(&mut self) -> i32 {
        self.add(&[5]);
        0
    }

    /// OutputFuncArgNum: a numeric argument, written as the argument's type.
    fn output_func_arg_num(&mut self) -> i32 {
        let mut f = atof(&self.token) as f32;
        let mut l = atol(&self.token);
        let mut s = atol(&self.token) as i16;
        let mut b = s as i8;
        if self.last_token_negative {
            f = -f;
            l = l.wrapping_neg();
            s = s.wrapping_neg();
            b = b.wrapping_neg();
            self.last_token_negative = false;
        }
        if self.num_func_args <= 0 {
            self.num_func_args = 1;
            self.last_func_arg = 0;
        }
        match self.cur_func {
            None => {
                self.add(&f.to_le_bytes());
            }
            Some(func) => {
                let arg = func.var.get(self.func_arg_index).copied().unwrap_or(0);
                if arg & A_NOOUTPUT != 0 {
                    self.func_opt_count += 1;
                } else if arg & A_FLOAT != 0 {
                    self.add(&f.to_le_bytes());
                } else if arg & A_LONG != 0 {
                    self.add(&l.to_le_bytes());
                } else if arg & A_SHORT != 0 {
                    self.add(&s.to_le_bytes());
                } else if arg & A_RESET != 0 {
                    self.add(&[1]);
                } else if arg & A_BYTE != 0 {
                    self.add(&b.to_le_bytes());
                }
            }
        }
        self.last_func_arg_symbol = false;
        0
    }

    fn arg_is_reset(&self, func: &Func) -> bool {
        self.func_arg_index < MAX_FUNCARGS && func.var[self.func_arg_index] & A_RESET != 0
    }

    /// OutputFuncArgGlobal.
    fn output_func_arg_global(&mut self) -> i32 {
        let func = self.func();
        if self.arg_is_reset(&func) {
            return 0;
        }
        if func.flags & F_SHORTVAR == 0 {
            self.add(b" ");
        }
        self.add_global(b"G");
        self.last_func_arg_symbol = true;
        if func.opcode != OPCODE_MESSAGEBOX {
            self.add(&[0]);
        }
        0
    }

    /// OutputFuncArgLocal.
    fn output_func_arg_local(&mut self) -> i32 {
        let func = self.func();
        if self.arg_is_reset(&func) {
            return 0;
        }
        let token = self.token.clone();
        let (index, kind) = self.find_local_var_index(&token);
        if func.flags & F_SHORTVAR == 0 {
            self.add(b" ");
        }
        self.add(&[Self::kind_byte(kind)]);
        self.add_short(index);
        self.last_func_arg_symbol = true;
        if func.opcode != OPCODE_MESSAGEBOX {
            self.add(&[0]);
        }
        0
    }

    /// OutputFuncArgStrSym / OutputFuncArgSym: an ID argument, quoted or bare.
    fn id_arg(&mut self, name: &[u8]) {
        let func = self.func();
        if self.output_func_id_ref {
            let flags = func.var.get(self.func_arg_index).copied().unwrap_or(0);
            let code = self.func_arg_ref_type(flags);
            self.add(code);
        }
        let n = name.len() & 0xFF;
        self.add(&[n as u8]);
        self.add(&name[..n]);
        self.last_func_arg_symbol = false;
    }

    /// OutputFuncArgString: a string argument (MessageBox's text and buttons included).
    fn output_func_arg_string(&mut self) {
        let func = self.func();
        let t = inner(&self.token).to_vec();
        if func.opcode == OPCODE_MESSAGEBOX {
            if self.num_func_args == 1 {
                self.add(&(t.len() as u16).to_le_bytes());
                self.add(&t);
                self.add(&[0]);
                self.last_msgbox_args = self.size as i64 - 1;
                self.last_msgbox_but = -1;
            } else {
                if self.last_msgbox_but < 0 {
                    self.add(&[0]);
                    self.last_msgbox_but = self.size as i64 - 1;
                }
                let length = (t.len() + 1) & 0xFF;
                self.add(&[length as u8]);
                self.add(&t[..length.saturating_sub(1).min(t.len())]);
                self.add(&[0]);
            }
        } else if func.flags & F_SHORTVAR == 0
            && func.var.get(self.func_arg_index).copied().unwrap_or(0) & A_SHORTSTR == 0
        {
            self.add(&(t.len() as u16).to_le_bytes());
            self.add(&t);
        } else {
            let n = t.len() & 0xFF;
            self.add(&[n as u8]);
            self.add(&t[..n]);
        }
    }

    /// OutputFuncEnd: a call's tail (EXTRASHORT's -1, MessageBox's counts).
    fn output_func_end(&mut self) {
        let func = self.func();
        if func.flags & F_EXTRASHORT != 0 {
            self.add_short(-1);
        }
        if func.opcode == OPCODE_MESSAGEBOX {
            if self.last_msgbox_args < 0 {
                self.add(&[0, 0]);
            } else {
                let args_at = self.last_msgbox_args as usize;
                self.buf[args_at] = self.num_msg_args as u8;
                if self.last_msgbox_but < 0 {
                    self.add(&[0]);
                } else {
                    let but_at = self.last_msgbox_but as usize;
                    self.buf[but_at] = self.num_msg_buttons as u8;
                }
            }
        }
    }

    /// OutputEffect: a magic effect index, one byte for SHORTVAR functions, else two.
    fn output_effect(&mut self, effect: i32) {
        if self.func().flags & F_SHORTVAR != 0 {
            self.add(&[effect as u8]);
        } else {
            self.add_short(i64::from(effect));
        }
    }

    // --- Object references (script_compile_ex.cc) ----------------------------------------

    /// OutputStoredFuncOp: write a remembered `id->`.
    fn output_stored_func_op(&mut self) {
        let ident = std::mem::take(&mut self.ref_object);
        if !ident.is_empty() {
            self.write_func_op(&ident);
        }
    }

    /// WriteFuncOp: a local holding a reference (MWSE), else the ID before the line.
    fn write_func_op(&mut self, ident: &[u8]) -> i32 {
        let (index, kind) = self.find_local_var_index(ident);
        if index >= 0 {
            let words = [OP_PUSHS, (index - 1) as i16, OP_PUSHS, i16::from(kind), OP_GETLOCAL, OP_SETREF];
            self.add_words(&words);
        } else {
            self.insert_script_data_ref(ident);
        }
        0
    }

    fn add_words(&mut self, words: &[i16]) {
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.add(&bytes);
    }

    // --- MWSE blocks (setx / ifx / whilex), ported but untested ---------------------------

    /// OutputLetEnd: store the `setx` results.
    fn output_let_end(&mut self) -> i32 {
        let queue = std::mem::take(&mut self.let_queue);
        for entry in queue {
            let (index, kind) = self.find_local_var_index(&entry.token);
            self.add_words(&[OP_PUSHS, (index - 1) as i16, OP_PUSHS, i16::from(kind), OP_SETLOCAL]);
        }
        0
    }

    fn x_test(&mut self) {
        let token = self.token.clone();
        let (index, kind) = self.find_local_var_index(&token);
        self.add_words(&[
            OP_PUSHS,
            (index - 1) as i16,
            OP_PUSHS,
            i16::from(kind),
            OP_GETLOCAL,
            OP_POP,
            4,
            OP_JUMPSHORTZERO,
            0,
        ]);
    }

    /// OutputXIf: `ifx ( local )`.
    fn output_x_if(&mut self) -> i32 {
        self.x_if_stack.push(IfBlock { pos: self.size + 16, start_count: 0 });
        self.x_test();
        0
    }

    /// OutputXElse.
    fn output_x_else(&mut self) -> i32 {
        let size = self.size;
        let Some(block) = self.x_if_stack.last_mut() else { return 0 };
        let at = block.pos;
        block.pos = size + 2;
        self.put(at, &((size + 4) as i16).to_le_bytes());
        self.add_words(&[OP_JUMPSHORT, 0]);
        0
    }

    /// OutputXEndWhile.
    fn output_x_end_while(&mut self) -> i32 {
        if let Some(block) = self.x_if_stack.pop() {
            self.add_words(&[OP_JUMPSHORT, block.pos as i16]);
            let at = (self.size as i16).to_le_bytes();
            self.put(block.pos + 16, &at);
        }
        0
    }

    /// PushFuncXArgNum: an extended-function numeric argument.
    fn push_func_x_arg_num(&mut self) -> i32 {
        let arg = self.func().var.get(self.func_arg_index).copied().unwrap_or(0) & 0xFFFF_FFFF;
        let kind = if arg & A_FLOAT != 0 {
            A_FLOAT
        } else if arg & (A_LONG | A_SHORT) != 0 {
            A_LONG
        } else {
            arg
        };
        if self.last_token_negative {
            self.token.insert(0, b'-');
            self.last_token_negative = false;
        }
        if self.num_func_args <= 0 {
            self.num_func_args = 1;
            self.last_func_arg = 0;
        }
        let token = self.token[..self.token.len().min(STACK_MAXTOKEN)].to_vec();
        self.arg_x_stack.push(StackEntry { token_id: kind as i32, token });
        self.last_func_arg_symbol = false;
        0
    }

    /// OutputFuncXBlock: an extended call, its arguments pushed last-first, then the opcode.
    fn output_func_x_block(&mut self) -> i32 {
        while let Some(entry) = self.arg_x_stack.pop() {
            match entry.token_id {
                A_LOCALVAR => {
                    let (index, kind) = self.find_local_var_index(&entry.token);
                    self.add_words(&[OP_PUSHS, (index - 1) as i16, OP_PUSHS, i16::from(kind), OP_GETLOCAL]);
                    self.last_func_arg_symbol = true;
                }
                k if k == A_FLOAT as i32 => {
                    let mut out = OP_PUSH.to_le_bytes().to_vec();
                    out.extend_from_slice(&(atof(&entry.token) as f32).to_le_bytes());
                    self.add(&out);
                }
                k if k == A_LONG as i32 || k == A_SHORT as i32 => {
                    let mut out = OP_PUSH.to_le_bytes().to_vec();
                    out.extend_from_slice(&atol(&entry.token).to_le_bytes());
                    self.add(&out);
                }
                k if k == A_STRING as i32 => {
                    let t = &entry.token;
                    let n = t.len() & 0xFF;
                    let padded = if n % 2 == 1 { n } else { n + 1 };
                    let pos = self.size + 8;
                    let mut out: Vec<u8> = [OP_PUSHS, pos as i16, OP_JUMPSHORT, (pos + padded + 1) as i16]
                        .iter()
                        .flat_map(|w| w.to_le_bytes())
                        .collect();
                    out.push(n as u8);
                    out.extend_from_slice(&t[..n]);
                    if padded > n {
                        out.push(0);
                    }
                    self.add(&out);
                }
                _ => {}
            }
        }
        let opcode = self.func().opcode;
        self.output_stored_func_op();
        self.add_short(i64::from(opcode));
        self.last_msgbox_args = -1;
        self.last_msgbox_but = -1;
        self.num_msg_args = 0;
        self.num_msg_buttons = 0;
        self.func_opt_count = 0;
        self.last_func_arg_symbol = false;
        0
    }
}

/// GetFuncArgIDType.
fn id_type_name(flags: u64) -> &'static str {
    if flags & A_IDMASK == 0 {
        return "None";
    }
    if flags & A_NPCID != 0 {
        return if flags & A_CREATUREID != 0 { "NPC/Creature" } else { "NPC" };
    }
    for (bit, name) in [
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
    ] {
        if flags & bit != 0 {
            return name;
        }
    }
    "Unknown"
}

/// Compiles one script's source against `records` (MWEdit's own extended functions).
pub fn compile(source: &[u8], records: &RecordIndex, levels: Levels, tribunal: bool, bloodmoon: bool) -> CompileResult {
    Compiler::new(records, levels, tribunal, bloodmoon).compile(source)
}
