"""The page's reading of the engine's answers: commands that pass on text must be listed."""

from __future__ import annotations

import re
from pathlib import Path

CORE = Path(__file__).resolve().parent.parent / "viewer-shell" / "ui" / "src" / "03_core.js"
CONFLICTS = Path(__file__).resolve().parent.parent / "wraithguard" / "gui" / "conflicts.py"


def _text_commands() -> set[str]:
    """The commands ``Engine.call`` hands back as they are, from the page's source."""
    found = re.search(r"TEXT:new Set\(\[(.*?)\]\)", CORE.read_text(encoding="utf-8"), re.S)
    assert found, "Engine.TEXT is gone from 03_core.js"
    return set(re.findall(r"'([a-z_]+)'", found.group(1)))


def test_the_open_record_command_is_read_as_text() -> None:
    """Wraithguard answers ``wg_open_record`` with ``ok``: reading it as JSON is an error.

    The click on a quest or record link in the viewer's inspector reported "the engine's
    answer to wg_open_record did not read (Unexpected token 'o', "ok" is not valid JSON)"
    although the conflict viewer had opened.
    """
    assert "wg_open_record" in _text_commands()
    source = CONFLICTS.read_text(encoding="utf-8")
    assert 'Payload(b"ok", "text/plain; charset=utf-8")' in source  # the answer it passes on


def test_the_post_passthrough_is_still_read_as_text() -> None:
    assert {"wg_post", "viewer_profile_read"} <= _text_commands()
