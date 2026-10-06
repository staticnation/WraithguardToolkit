"""The MWScript compiler against MWEdit's own output.

``tests/fixtures/mwedit/MwEditScriptTest.esp`` is MWEdit's compiler test plugin: 34
scripts covering every function family, if/elseif/else, while, set expressions,
MessageBox, object references and remote variables, each with the bytecode MWEdit
compiled. The port must reproduce every byte, the locals and the header.
"""

from __future__ import annotations

import struct
from pathlib import Path

import pytest

from wraithguard.mwscript.compiler import Compiler, compile_script, find_function
from wraithguard.mwscript.records import Record, RecordIndex, index_plugins
from wraithguard.mwscript.script_record import read_script_records

FIXTURE = Path(__file__).parent / "fixtures" / "mwedit" / "MwEditScriptTest.esp"
SCRIPTS = read_script_records(FIXTURE)
INDEX = index_plugins([FIXTURE])


def _schd(path: Path) -> dict[str, bytes]:
    """Each script's raw SCHD, by name."""
    data = path.read_bytes()
    out: dict[str, bytes] = {}
    pos = 0
    while pos + 16 <= len(data):
        tag = data[pos : pos + 4]
        (size,) = struct.unpack_from("<I", data, pos + 4)
        body = data[pos + 16 : pos + 16 + size]
        if tag == b"SCPT":
            q = 0
            while q + 8 <= len(body):
                (sub_size,) = struct.unpack_from("<I", body, q + 4)
                if body[q : q + 4] == b"SCHD":
                    payload = body[q + 8 : q + 8 + sub_size]
                    out[payload[:32].split(b"\x00")[0].decode("latin-1")] = payload
                q += 8 + sub_size
        pos += 16 + size
    return out


HEADERS = _schd(FIXTURE)


def test_fixture_has_the_corpus() -> None:
    """All 34 of MWEdit's test scripts are there, with bytecode."""
    assert len(SCRIPTS) == 34
    assert all(s.bytecode for s in SCRIPTS)


@pytest.mark.parametrize("script", SCRIPTS, ids=[s.name for s in SCRIPTS])
def test_matches_mwedit_byte_for_byte(script) -> None:
    """Same SCDT, same locals, same SCHD as MWEdit wrote."""
    result = compile_script(script.text, INDEX)
    assert result.ok, [m.text for m in result.errors]
    assert result.name == script.name
    assert result.data == script.bytecode
    assert [*result.shorts, *result.longs, *result.floats] == script.variables
    assert result.header() == HEADERS[script.name]


def _index(*records: Record) -> RecordIndex:
    index = RecordIndex()
    for rec in records:
        index.add(rec)
    return index


def test_line_endings_do_not_matter() -> None:
    """LF-only and CR-only text compile to the same bytes as CRLF."""
    script = next(s for s in SCRIPTS if s.name == "test_if1")
    crlf = compile_script(script.text, INDEX).data
    text = script.text.replace("\r\n", "\n")
    assert compile_script(text, INDEX).data == crlf
    assert compile_script(text.replace("\n", "\r"), INDEX).data == crlf


def test_return_and_end() -> None:
    """``return`` is 0x0124, ``end`` 0x0101."""
    result = compile_script("begin r\nreturn\nend\n", RecordIndex())
    assert result.ok
    assert result.data == b"\x24\x01\x01\x01"


def test_choice_writes_its_arguments_as_text() -> None:
    """Choice: opcode, a 2-byte length, then the arguments space-separated."""
    result = compile_script('begin c\nChoice "Yes", 1, "No", 2\nend\n', RecordIndex())
    assert result.ok
    body = b'"Yes" 1 "No" 2'
    assert result.data == b"\xc9\x10" + struct.pack("<H", len(body)) + body + b"\x01\x01"


def test_globals_take_the_records_case_and_pad_to_four() -> None:
    """A global is written as its record spells it, at least four bytes long."""
    index = _index(Record("GLOB", "Gx", glob_type="s"))
    result = compile_script("begin g\nset gx to 1\nend\n", index)
    assert result.ok
    # set, G + length 4 + "Gx" + 2 NULs of padding, then the expression's length and " 1".
    assert result.data == b"\x05\x01G\x04Gx\x00\x00\x02 1\x01\x01"


def test_unknown_id_is_an_error_by_default_and_a_warning_when_weak() -> None:
    """ERROR_BADID follows the message level, as in MWEdit."""
    text = "begin u\nnobody->Disable\nend\n"
    strict = compile_script(text, RecordIndex())
    assert not strict.ok
    assert strict.errors
    weak = compile_script(text, RecordIndex(), levels="weak")
    assert weak.ok
    assert any(m.level == "warning" for m in weak.messages)


def test_redeclared_local_fails() -> None:
    """A local declared twice stops the compile."""
    result = compile_script("begin d\nshort a\nshort a\nend\n", RecordIndex())
    assert not result.ok


def test_function_lookup_is_case_insensitive() -> None:
    """Function names match whatever their case."""
    func = find_function("getdistance")
    assert func is not None
    assert func.name == "GetDistance"
    assert func.opcode == 0x1001


def test_tribunal_functions_can_be_refused() -> None:
    """With Tribunal off, a Tribunal function is an error (MWEdit's default level)."""
    from wraithguard.mwscript.compiler_data import FUNCTIONS

    name = next(n for n, _op, flags, _ret, args in FUNCTIONS if flags & 0x800 and not args[0])
    text = f"begin t\n{name}\nend\n"
    assert Compiler(RecordIndex()).compile(text).ok
    assert not Compiler(RecordIndex(), tribunal=False).compile(text).ok


def test_script_compiler_python_port_matches() -> None:
    """The front door, held to this port, writes the same bytes as MWEdit."""
    from wraithguard.mwscript.compiler import ScriptCompiler

    compiler = ScriptCompiler([FIXTURE], native=False)
    assert not compiler.native
    for script in SCRIPTS:
        assert compiler.compile(script.text).data == script.bytecode


def test_script_compiler_takes_pool_records() -> None:
    """A global only the patch pool has still resolves (``extra``)."""
    from wraithguard.mwscript.compiler import ScriptCompiler

    text = "begin p\nset NewGlobal to 2\nend\n"
    assert not ScriptCompiler([FIXTURE], native=False).compile(text).ok
    extra = [("GLOB", "NewGlobal", "", -1, [], [], [])]
    result = ScriptCompiler([FIXTURE], extra, native=False).compile(text)
    assert result.ok
    assert b"NewGlobal" in result.data


def test_rust_compiler_matches_the_python_port() -> None:
    """The Rust backend (when built) writes what this port writes, byte for byte."""
    from wraithguard.mwscript.compiler import ScriptCompiler, _native_compiler

    if _native_compiler() is None:
        pytest.skip("the native module is not built")
    rust = ScriptCompiler([FIXTURE])
    assert rust.native
    for script in SCRIPTS:
        got = rust.compile(script.text)
        assert got.ok, [m.text for m in got.errors]
        assert got.data == script.bytecode, script.name
        assert got.header() == HEADERS[script.name]
    probe = "begin q\nshort a\nset a to ( GetDistance player ) * -2\nreturn\nend\n"
    py = ScriptCompiler([FIXTURE], native=False).compile(probe)
    assert rust.compile(probe).data == py.data


def test_extended_functions_mwedit_ships() -> None:
    """An MWSE function from MWEdit's own list: arguments pushed last-first, then the opcode."""
    result = compile_script('begin t\nXAddItem "misc_01", 10\nend\n', INDEX)
    assert result.ok
    assert result.data[:6] == b"\x11\x38\x0a\x00\x00\x00"  # PUSH long 10
    assert result.data[-4:] == b"\x28\x3c\x01\x01"  # XAddItem, end


def test_extended_functions_from_a_file() -> None:
    """A ``customfunctions.dat`` replaces MWEdit's set, read as MWEdit reads it."""
    from wraithguard.mwscript.compiler import ScriptCompiler, parse_custom_functions

    text = (
        "# comment\nfunction\n  Name = XMine  # trailing\n  Opcode = 0x3999\n  Param1 = Long\nend\n"
    )
    custom = parse_custom_functions(text)
    assert custom["xmine"].opcode == 0x3999
    got = ScriptCompiler([FIXTURE], custom=text, native=False).compile("begin t\nXMine 5\nend\n")
    assert got.data == b"\x11\x38\x05\x00\x00\x00\x99\x39\x01\x01"
    assert (
        not ScriptCompiler([FIXTURE], custom=text, native=False)
        .compile('begin t\nXAddItem "misc_01", 10\nend\n')
        .ok
    )


def test_extended_functions_can_warn() -> None:
    """``extended=False``: MWEdit's warning (a warning, by default, not an error)."""
    result = Compiler(INDEX, extended=False).compile('begin t\nXAddItem "misc_01", 10\nend\n')
    assert result.ok
    assert any("Extended function" in m.text for m in result.messages)


def test_dialogue_results_compile_against_the_speakers_locals() -> None:
    """A result reads its speaker's script's locals by bare name; lines are the result's."""
    from wraithguard.mwscript.compiler import ScriptCompiler

    compiler = ScriptCompiler([FIXTURE], native=False)
    assert compiler.script_locals("testnpc02") == (
        ["RemoteVar1", "RemoteVar2"],
        ["TestLong"],
        ["TestFloat"],
    )
    assert compiler.check_result("set RemoteVar1 to 2\nGoodbye", "testnpc02") == []
    errors = compiler.check_result('Goodbye\nset Nobody to 1\nJournal "journal_01" 10', "")
    assert [(m.line, m.level) for m in errors] == [(2, "error")]


def test_a_cells_references_do_not_name_the_cell() -> None:
    """Only a cell's first NAME is its id (its references carry NAMEs too)."""
    rec = INDEX.find("misc_01")
    assert rec is not None
    assert rec.type == "MISC"
    assert rec.script == "scripttest_d"


def test_a_function_after_choice_is_not_inside_it():
    """MWEdit leaves Choice's call open past its line; the CS does not (a result's
    `StartScript` after its `Choice` is fine)."""
    from wraithguard.mwscript.compiler import ScriptCompiler

    msgs = ScriptCompiler([], native=False).check_result(
        'Choice "Yes" 1 "No" 2\nGoodbye\nStopCombat'
    )
    assert not [m for m in msgs if "within another function" in m.text], msgs
