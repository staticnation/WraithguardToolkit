"""The Build patch and Test in OpenMW links' decisions: the shared cases
(``tests/fixtures/buildjob_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::buildjob`` and ``editcore::testrun`` too) through
:mod:`wraithguard.patch.buildjob` and :mod:`wraithguard.patch.testrun`.

A case's paths are written with ``{tmp}`` for a folder of its own, made with the case's
``dirs`` and ``files`` in it; the answers are written back the same way, with ``/``
between a path's parts.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from wraithguard.patch import buildjob, testrun

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "buildjob_cases.json").read_text(encoding="utf-8")
)["cases"]


def _in(value: Any, tmp: Path) -> Any:
    """``{tmp}`` made the case's folder, everywhere in a value."""
    return json.loads(json.dumps(value).replace("{tmp}", tmp.as_posix()))


def _out(value: Any, tmp: Path) -> Any:
    """The case's folder written back as ``{tmp}``, a path's separators as ``/`` (the
    answers read the same on Windows)."""
    text = json.dumps(value).replace("\\\\", "/")
    return json.loads(text.replace(tmp.as_posix(), "{tmp}"))


def answer(case: dict[str, Any], tmp: Path) -> Any:
    """What one case's call gives."""
    kind, a = case["kind"], _in(case["args"], tmp)
    if kind == "target":
        return str(buildjob.patch_target(a["req"]))
    if kind == "check":
        return buildjob.build_check(buildjob.patch_target(a["req"]), a["order"])
    if kind == "preview":
        return str(buildjob.preview_target(a["req"]))
    if kind == "mode":
        return buildjob.build_mode(a["req"], Path(a["target"]), a.get("running"))
    if kind == "next":
        return buildjob.new_state(a.get("running"))
    if kind == "status":
        return buildjob.build_status(a.get("state"), a["req"])
    if kind == "setups":
        settings = a["settings"]
        changed = buildjob.update_setups(settings, a["req"])
        return {"settings": settings, "changed": changed}
    if kind == "setup":
        return testrun.clean_setup(a["raw"])
    if kind == "launch":
        argv, script = testrun.launch_args(
            Path(a["exe"]), Path(a["plugin"]), a.get("cell"), a.get("setup")
        )
        return {"argv": argv, "script": script}
    if kind == "test":
        exe = Path(a["exe"]) if a.get("exe") else None
        found, cell, setup = buildjob.test_request(a["req"], a.get("given"), exe, a["settings"])
        return {"exe": str(found), "cell": cell, "setup": setup}
    if kind == "settings":
        file = tmp / "openmw_test.json"
        if "file" in a:
            file.write_text(a["file"], encoding="utf-8")
        return testrun.load_settings_from(file)
    raise AssertionError(f"unknown kind {kind}")


def run(case: dict[str, Any], tmp: Path) -> Any:
    """A case's answer, or ``{error}``."""
    for d in case.get("dirs", []):
        (tmp / d).mkdir(parents=True, exist_ok=True)
    for f in case.get("files", []):
        (tmp / f).parent.mkdir(parents=True, exist_ok=True)
        (tmp / f).write_bytes(b"")
    try:
        got = answer(case, tmp)
    except ValueError as exc:
        got = {"error": str(exc)}
    return _out(got, tmp)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]
