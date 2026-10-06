"""wraithguard.mwscript.check: what the Script Edit window flags in a script's source."""

from __future__ import annotations

from wraithguard.mwscript.check import check_script

GOOD = """begin payme
short done
float timer ; a comment
if ( done == 0 )
    set timer to timer + GetSecondsPassed
    if ( timer > 5 )
        player->AddItem "gold_001" 5
        set done to 1
        set GlobalDays to 1
        set "fargoth".myvar to 2
    elseif ( timer < 0 )
        return
    else
        MessageBox "Wait"
    endif
endif
end payme
"""


def _msgs(text: str, **kw: object) -> list[tuple[int, str, str]]:
    return [(f.line, f.level, f.message) for f in check_script(text, **kw)]  # type: ignore[arg-type]


def test_a_good_script_has_nothing():
    assert _msgs(GOOD, script_id="PayMe", globals_=["globaldays"]) == []


def test_frame_and_name():
    assert _msgs("short x\nend")[0][1:] == ("error", "a script starts with 'begin <name>'")
    msgs = _msgs("begin other\nend", script_id="payme")
    assert msgs == [(1, "warning", "'begin other' does not name this script (payme)")]
    assert ("error", "the script has no 'end'") in [m[1:] for m in _msgs("begin x\nshort y")]
    assert (3, "warning", "text after 'end' is never run") in _msgs("begin x\nend\nset y to 1")
    assert _msgs("")[0][2] == "the script is empty"


def test_blocks_out_of_balance():
    msgs = _msgs("begin x\nif ( 1 )\nwhile ( 1 )\nendif\nend")
    assert (4, "error", "'endif' without a 'if'") in msgs
    assert (2, "error", "'if' is never closed") in msgs
    assert (3, "error", "'while' is never closed") in msgs
    msgs = _msgs("begin x\nif ( 1 )\nelse\nelseif ( 2 )\nendif\nelse\nend")
    assert (4, "error", "'elseif' after 'else'") in msgs
    assert (6, "error", "'else' without an 'if'") in msgs


def test_variables_and_functions():
    msgs = _msgs("begin x\nshort a\nlong A\nset b to 1\nset a 2\nFlyToTheMoon\nend")
    assert (3, "warning", "A is declared again (first on line 2)") in msgs
    assert (4, "warning", "b is not a local variable or a global") in msgs
    assert (5, "error", "'set' is 'set <variable> to <value>'") in msgs
    assert (6, "warning", "flytothemoon is not a function the game knows") in msgs
    assert not any(m[0] == 4 for m in _msgs("begin x\nshort a\nset a to 1\nend"))


def test_session_script_view(tmp_path):
    from wraithguard.patch.editor import EditorError, EditorSession
    from wraithguard.patch.queue import PatchQueue

    recs = {
        "SCPT": [{"type": "Script", "id": "payme", "text": "begin payme\nset gdays to 1\nend"}],
        "GLOB": [{"type": "GlobalVariable", "id": "GDays"}],
        "STAT": [{"type": "Static", "id": "rock"}],
    }
    s = EditorSession(
        [("Morrowind.esm", tmp_path / "Morrowind.esm")],
        PatchQueue(),
        read=lambda _p, tag: recs.get(tag, []),
    )
    found = s.find("SCPT", "payme")
    assert found is not None
    view = s.script_view(found)
    assert view["findings"] == [] and view["compiled"] is False  # gdays is a global
    view = s.script_view(found, "begin payme\nset nope to 1\n")
    assert [f["level"] for f in view["findings"]] == ["warning", "error"]
    rock = s.find("STAT", "rock")
    import pytest

    with pytest.raises(EditorError, match="not a script"):
        s.script_view(rock)  # type: ignore[arg-type]


def test_rust_checks_match_the_python_ones():
    """The Rust backend (when built) finds what this module finds, word for word."""
    import pytest

    from wraithguard.mwscript.check import _native_check, check_script_py
    from wraithguard.mwscript.script_record import read_script_records

    native = _native_check()
    if native is None:
        pytest.skip("the native module is not built")
    from pathlib import Path

    fixture = Path(__file__).parent / "fixtures" / "mwedit" / "MwEditScriptTest.esp"
    texts = [s.text for s in read_script_records(fixture)]
    texts += [
        "begin x\nshort a\nlong A\nset b to 1\nset a 2\nFlyToTheMoon\nend",
        "begin x\nif ( 1 )\nelse\nelse\nwhile ( 1 )\nend",
        "set a to 1",
        "",
        "begin x\nend\nDisable",
    ]
    for text in texts:
        want = [(f.line, f.level, f.message) for f in check_script_py(text, "x", ["GDays"])]
        got = [tuple(f) for f in native(text, "x", ["GDays"])]
        assert got == want, text
