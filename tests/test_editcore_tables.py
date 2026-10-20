"""The Rust Editor's tables (``viewer-shell/editcore/data/fields.json`` and
``opcodes.json``) are the Python ones: run ``tools/gen_editcore_tables.py`` again when this
fails."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def test_the_json_is_the_python_tables() -> None:
    spec = importlib.util.spec_from_file_location(
        "gen_editcore_tables", ROOT / "tools" / "gen_editcore_tables.py"
    )
    assert spec is not None and spec.loader is not None
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    have = json.loads((ROOT / "viewer-shell/editcore/data/fields.json").read_text("utf-8"))
    assert have == gen.tables()
    have = json.loads((ROOT / "viewer-shell/editcore/data/opcodes.json").read_text("utf-8"))
    assert have == gen.opcodes()
