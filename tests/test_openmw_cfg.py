"""The openmw.cfg reader: the shared cases (``tests/fixtures/openmw_cfg_cases.json``,
which ``viewer-shell/ordercore`` runs too), through the Rust reader and the fallback."""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

from wraithguard import openmw_cfg

if TYPE_CHECKING:
    from collections.abc import Callable

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "openmw_cfg_cases.json").read_text(encoding="utf-8")
)["cases"]


def _readers() -> list[Callable[..., dict[str, Any]]]:
    """The fallback, and the Rust reader when the backend is built."""
    out: list[Callable[..., dict[str, Any]]] = [openmw_cfg.read_cfg_py]
    try:
        import wraithguard_native
    except ImportError:
        return out
    if hasattr(wraithguard_native, "openmw_cfg"):
        out.append(openmw_cfg.read_cfg)
    return out


def _build(root: Path, case: dict[str, Any]) -> None:
    """Make a case's folders and files under ``root``."""
    for d in case["dirs"]:
        (root / d).mkdir(parents=True, exist_ok=True)
    for name, text in case["files"].items():
        p = root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")


@pytest.mark.parametrize("reader", _readers(), ids=lambda r: r.__name__)
@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path, case, reader):
    _build(tmp_path, case)
    tokens = case.get("tokens", {})

    def rel(p: str | None) -> str | None:
        """A path relative to the case's folder, '/' separated."""
        return None if p is None else Path(p).relative_to(tmp_path).as_posix()

    got = reader(
        tmp_path / case["cfg"],
        tmp_path / tokens["userdata"] if "userdata" in tokens else None,
        tmp_path / tokens["userconfig"] if "userconfig" in tokens else None,
    )
    want = case["expect"]
    assert [rel(p) for p in got["data"]] == want["data"]
    assert [rel(p) for p in got["missing_data"]] == want["missing_data"]
    for key in ("content", "groundcover", "archives", "plugins"):
        assert got[key] == want[key], key
    assert [rel(p) for p in got["files"]] == want["files"]
    assert len(got["warnings"]) == want["warnings"], got["warnings"]
    if "user_data" in want:
        assert rel(got["user_data"]) == want["user_data"]
    if want["located"]:
        names = [n for n, _p in want["located"]]
        found = dict(openmw_cfg.locate([Path(p) for p in got["data"]], names))
        assert [[n, rel(found[n])] for n in names] == want["located"]


def test_quoted_paths_unescape():
    assert openmw_cfg.path_text('"a &" b && c" tail') == 'a " b & c'
    assert openmw_cfg.path_text("plain & path") == "plain & path"


def test_the_flatpak_sandbox_has_its_own_folders():
    t = openmw_cfg.host_tokens(Path("/home/me/.var/app/org.openmw.OpenMW/config/openmw/openmw.cfg"))
    assert t["?userdata?"] == Path("/home/me/.var/app/org.openmw.OpenMW/data/openmw")
    assert t["?userconfig?"] == Path("/home/me/.var/app/org.openmw.OpenMW/config/openmw")
