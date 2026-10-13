"""Script source goes into the pool with CRLF line ends, however it was typed."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.patch.editor import EditorSession, bare_lf_lines, crlf
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path

PLUGINS: dict[str, list[dict[str, Any]]] = {
    "Morrowind.esm": [
        {"type": "Script", "id": "TestScript", "text": "begin TestScript\r\nend\r\n"},
        {"type": "DialogueInfo", "id": "101", "script_text": "", "text": "Hello."},
    ],
}
_TAGS = {"SCPT": "Script", "INFO": "DialogueInfo"}


def _session(tmp_path: Path) -> EditorSession:
    """A session over :data:`PLUGINS`."""
    return EditorSession(
        [(n, tmp_path / n) for n in PLUGINS],
        PatchQueue(),
        read=lambda p, tag: [r for r in PLUGINS[p.name] if r["type"] == _TAGS.get(tag)],
    )


def test_every_line_end_becomes_crlf():
    assert crlf("a\nb\r\nc\rd") == "a\r\nb\r\nc\r\nd"
    assert crlf("a\r\n") == "a\r\n"


def test_script_text_and_result_scripts_are_stored_crlf(tmp_path):
    s = _session(tmp_path)
    script = s.find("SCPT", "TestScript")
    info = s.find("INFO", "101")
    assert script is not None and info is not None
    s.set_field(script, "text", "begin TestScript\nset x to 1 ; a comment\nend\n")
    s.set_field(info, "script_text", "Journal MS_Q 10\nGoodbye")
    texts = {c.path: c.value for k in s.queue.fields for c in s.queue.fields[k]}
    assert texts["text"] == "begin TestScript\r\nset x to 1 ; a comment\r\nend\r\n"
    assert texts["script_text"] == "Journal MS_Q 10\r\nGoodbye"
    # Typing the same text back with LF is no change at all.
    s.set_field(script, "text", "begin TestScript\nend\n")
    assert not any(c.path == "text" for k in s.queue.fields for c in s.queue.fields[k])


def test_bare_lf_lines_are_found():
    assert bare_lf_lines("a\r\nb\nc\r\nd\n") == [2, 4]
    assert bare_lf_lines("a\r\nb") == []


def test_a_plugins_lf_script_is_warned_about(tmp_path):
    PLUGINS["Morrowind.esm"].append(
        {"type": "Script", "id": "LfScript", "text": "begin LfScript\nend\n"}
    )
    try:
        s = _session(tmp_path)
        lf = s.find("SCPT", "LfScript")
        ok = s.find("SCPT", "TestScript")
        assert lf is not None and ok is not None
        warned = [f for f in s.script_view(lf)["findings"] if "CRLF" in f["message"]]
        assert warned and warned[0]["line"] == 1 and warned[0]["message"].startswith("2 line(s)")
        assert not any("CRLF" in f["message"] for f in s.script_view(ok)["findings"])
        # Text typed in the window is not the plugin's: no warning.
        assert not any(
            "CRLF" in f["message"] for f in s.script_view(lf, "begin LfScript\nend\n")["findings"]
        )
    finally:
        PLUGINS["Morrowind.esm"].pop()


class _NoCompiler:
    """Compiles every result cleanly, so only the line-end warning is left."""

    def check_result(self, text: str, speaker: str) -> list[Any]:
        return []

    def script_locals(self, speaker: str) -> None:
        return None


def test_a_plugins_lf_result_script_is_warned_about(tmp_path):
    s = _session(tmp_path)
    compiler: Any = _NoCompiler()
    stored = s.result_check("Journal MS_Q 10\nGoodbye\n", "", compiler, stored=True)
    assert [m["line"] for m in stored["messages"]] == [1]
    assert "2 line(s) end in LF, not CRLF" in stored["messages"][0]["message"]
    # As typed it is stored with CRLF anyway: no warning; nor for a CRLF result.
    assert s.result_check("Journal MS_Q 10\nGoodbye\n", "", compiler)["messages"] == []
    crlf_result = s.result_check("Journal MS_Q 10\r\nGoodbye\r\n", "", compiler, stored=True)
    assert crlf_result["messages"] == []
