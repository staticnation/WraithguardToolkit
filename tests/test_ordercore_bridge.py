"""The calls into ``ordercore`` through the native module (``wraithguard/ordercore_bridge.py``,
port step 4.2 slice 2j): every native function a Python body defers to exists, so a misspelt
name cannot quietly leave Python answering; the generators' switch back to the Python bodies;
and ordercore's English messages put back into the user's language."""

from __future__ import annotations

import re
from pathlib import Path

import pytest

from wraithguard import ordercore_bridge

ROOT = Path(__file__).resolve().parent.parent
SOURCES = [*sorted((ROOT / "wraithguard").rglob("*.py")), ROOT / "wraithguard_toolkit.py"]

#: ``WG_ORDERCORE=python``: the Python bodies answer, so there is no native path to check.
needs_native = pytest.mark.skipif(ordercore_bridge.FORCE_PYTHON, reason="WG_ORDERCORE=python")


def _names() -> set[str]:
    found: set[str] = set()
    for path in SOURCES:
        text = path.read_text(encoding="utf-8")
        found.update(re.findall(r"(?:native|call|_native|_ported)\(\s*\"(order_[a-z_]+)\"", text))
    return found


def test_every_native_function_python_defers_to_is_there() -> None:
    native = pytest.importorskip("wraithguard_native")
    names = _names()
    assert len(names) >= 25
    assert sorted(n for n in names if not hasattr(native, n)) == []


@needs_native
def test_python_only_answers_from_the_python_bodies_and_comes_back() -> None:
    pytest.importorskip("wraithguard_native")
    assert ordercore_bridge.native("order_parse_rules") is not None
    with ordercore_bridge.python_only():
        assert ordercore_bridge.native("order_parse_rules") is None
    assert ordercore_bridge.native("order_parse_rules") is not None


@needs_native
def test_a_decline_falls_back_to_the_python_body() -> None:
    native = pytest.importorskip("wraithguard_native")

    def declines() -> None:
        raise native.OrderUnsupportedError("x")

    native.order_test_declines = declines
    try:
        assert ordercore_bridge.call("order_test_declines", lambda: "python") == "python"
    finally:
        delattr(native, "order_test_declines")


@needs_native
def test_other_errors_are_not_swallowed() -> None:
    native = pytest.importorskip("wraithguard_native")

    def fails() -> None:
        raise ValueError("refused")

    native.order_test_fails = fails
    try:
        with pytest.raises(ValueError, match="refused"):
            ordercore_bridge.call("order_test_fails", lambda: "python")
    finally:
        delattr(native, "order_test_fails")


def test_a_message_is_rendered_again_from_its_template(monkeypatch: pytest.MonkeyPatch) -> None:
    from wraithguard import i18n

    monkeypatch.setattr(i18n, "gettext", lambda t: t.replace("NOTE", "HINWEIS"))
    template = "NOTE: '%(value)s' has %(count)d of '%(value)s'."
    got = ordercore_bridge.translated("NOTE: 'a b' has 3 of 'a b'.", [template])
    assert got == "HINWEIS: 'a b' has 3 of 'a b'."
    assert ordercore_bridge.translated("NOTE: 'a' has 3 of 'b'.", [template]) == (
        "NOTE: 'a' has 3 of 'b'."
    )
    assert ordercore_bridge.translated("something else", [template]) == "something else"
