r"""The command line's cases: ``tests/fixtures/cli_cases.json`` (port step 4.5, slice 1).

Each case is a setup made on disk and a command line; the answer is what
``wraithguard_toolkit.py`` gives for it: every line it prints, every line it logs, how it exits,
and the files the run leaves (the cfg and the TOML it writes, the CSVs, the cell map). The Rust
command line (``viewer-shell/cli``, the ``wraithguard`` binary) is run on the same cases by
``viewer-shell/cli/tests/cli_cases.rs`` and must give the same, byte for byte.

The sort's setups are ``tools/gen_plan_cases.py``'s; the scans' plugins are written here from
tes3conv-schema records by the native writer and kept in the file as base64 (``b64``, beside the
``records`` they came from, which only the reader of this file reads), so the Rust test needs
no writer. Paths are written with ``<root>`` for the setup's folder and ``/`` between names;
the clock is 2026-10-22 12:00:00 (``.bak`` stamps, the cell map's date).

Run: ``python tools/gen_cli_cases.py`` (needs the native module).
"""

from __future__ import annotations

import base64
import contextlib
import io
import json
import logging
import sys
import tempfile
from pathlib import Path
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "cli_cases.json"
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools"))

import gen_plan_cases as plan  # noqa: E402

# ---------------------------------------------------------------- the scans' plugins


def _header(
    *masters: tuple[str, int], author: str = "me", description: str = "a test"
) -> dict[str, Any]:
    return {
        "type": "Header",
        "flags": "",
        "version": 1.3,
        "file_type": "Esp",
        "author": author,
        "description": description,
        "num_objects": 0,
        "masters": [list(m) for m in masters],
    }


def _static(rid: str, mesh: str) -> dict[str, Any]:
    return {"type": "Static", "flags": "", "id": rid, "mesh": mesh}


def _exterior(x: int, y: int) -> dict[str, Any]:
    return {
        "type": "Cell",
        "flags": "",
        "name": "",
        "data": {"flags": "", "grid": [x, y]},
        "references": [],
    }


def _interior(name: str, fog: float) -> dict[str, Any]:
    return {
        "type": "Cell",
        "flags": "",
        "name": name,
        "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
        "atmosphere_data": {
            "ambient_color": [0, 0, 0, 0],
            "sunlight_color": [0, 0, 0, 0],
            "fog_color": [0, 0, 0, 0],
            "fog_density": fog,
        },
        "references": [],
    }


#: An empty packed field (bytecode, variables) as the tes3conv schema writes it.
EMPTY_PACKED = "KLUv/QBYIQAAAAAAAA=="


def _script(rid: str, text: str) -> dict[str, Any]:
    return {
        "type": "Script",
        "flags": "",
        "id": rid,
        "header": {
            "num_shorts": 0,
            "num_longs": 0,
            "num_floats": 0,
            "bytecode_length": 0,
            "variables_length": 0,
        },
        "variables": EMPTY_PACKED,
        "bytecode": EMPTY_PACKED,
        "text": text,
    }


GMST_EVIL = {
    "type": "GameSetting",
    "flags": "",
    "id": "fFleeDistance",
    "value": {"type": "Float", "data": 3000.0},
}

SCAN_PLUGINS: dict[str, list[dict[str, Any]]] = {
    "game/Morrowind.esm": [
        _header(description="the base"),
        _static("rock", "r.nif"),
        _static("tree", "t.nif"),
        _exterior(0, 0),
        _exterior(1, 1),
    ],
    "mods/A/Alpha.esp": [
        _header(("Morrowind.esm", 1), author="", description=""),
        _static("rock", "r2.nif"),
        GMST_EVIL,
        _exterior(1, 1),
        _exterior(2, -3),
        _interior("Vault", 0.0),
        _script("alpha_s", 'begin alpha_s\r\nPlaceItem "rock" 0 0 0 0\r\nend\r\n'),
    ],
    "mods/B/Beta.esp": [
        _header(("Morrowind.esm", 1), ("Alpha.esp", 1)),
        _static("rock", "r3.nif"),
        _static("tree", "t2.nif"),
        _exterior(1, 1),
        _interior("Vault", 0.5),
        _interior('Shrine <of> "Light"', 0.5),
    ],
    "mods/C/Clash.esp": [
        _header(("Morrowind.esm", 1)),
        _static("tree", "t3.nif"),
        _exterior(0, 0),
    ],
}

SCAN_CFG = """\
data="<root>/game"
data="<root>/mods/A"
data="<root>/mods/B"
data="<root>/mods/C"
content=Morrowind.esm
content=Clash.esp
"""

SCAN_FILES: dict[str, Any] = {
    "rules/mlox_base.txt": "[Order]\nAlpha.esp\nBeta.esp\n",
    "openmw.cfg": SCAN_CFG,
    "mods/A/meshes/x.nif": "not a mesh A",
    "mods/B/meshes/x.nif": "not a mesh, B",
    "mods/A/textures/same.dds": "identical bytes",
    "mods/B/textures/same.dds": "identical bytes",
    "mods/B/textures/only.dds": "one provider",
    "mods/C/textures/same.dds": "other bytes",
}


def _scan_files() -> dict[str, Any]:
    """The scan setup's files, its plugins written by the native writer."""
    import wraithguard_native

    files = dict(SCAN_FILES)
    for rel, records in SCAN_PLUGINS.items():
        raw = bytes(wraithguard_native.plugin_write(records))
        files[rel] = {"b64": base64.b64encode(raw).decode("ascii"), "records": records}
    return files


# ---------------------------------------------------------------- the cases


def _cases_in() -> list[tuple[str, dict[str, Any], list[str]]]:
    by_name = {name: (files, argv) for name, files, argv, _extra in plan._cases_in()}
    out: list[tuple[str, dict[str, Any], list[str]]] = []
    for name in (
        "a subset file, sorted with its folders",
        "a subset file, folders not sorted",
        "a customizations TOML with MOMW's file and a list",
        "orphans from the cfg, with a data-paths cache",
        "a missing MOMW file and cache",
        "nothing to sort",
        "a subset of only grass",
        "a scan written to the subset file",
        "a scan with nowhere to write",
        "a merged file gone stale, and an order the rules turn round",
    ):
        files, argv = by_name[name]
        out.append((name, files, list(argv)))
    files, argv = by_name["a subset file, sorted with its folders"]
    out.append(("a dry run of writing both", files, [*argv, "--dry-run"]))
    files, argv = by_name["a subset file, folders not sorted"]
    out.append(
        ("verbose, without the rule warnings", files, [*argv, "-v", "--no-predicate-warnings"])
    )
    scan = _scan_files()
    base = [
        "--cfg",
        "<root>/openmw.cfg",
        "--rules",
        "<root>/rules/mlox_base.txt",
        "--subset",
        "Alpha.esp",
        "Beta.esp",
    ]
    out.append(
        (
            "every scan, their files written",
            scan,
            [
                *base,
                "--check-conflicts",
                "--conflicts-out",
                "<root>/conflicts.csv",
                "--cell-map",
                "<root>/map.html",
                "--lint",
                "--resource-conflicts",
                "--resources-out",
                "<root>/resources.csv",
            ],
        )
    )
    out.append(
        (
            "conflicts of mine only, one plugin left out",
            scan,
            [
                *base,
                "--check-conflicts",
                "--conflicts-subset-only",
                "--exclude",
                "clash*",
                "--lint",
                "--cell-map",
                "<root>/map.html",
            ],
        )
    )
    out.append(
        (
            "a cell map and the lint with a substring left out",
            scan,
            [*base, "--cell-map", "<root>/map.html", "--lint", "--exclude", "alph"],
        )
    )
    return out


def _build(root: Path, files: dict[str, Any]) -> None:
    """``gen_plan_cases.make_setup``, with ``b64`` plugins written as they are."""
    rest = {k: v for k, v in files.items() if not (isinstance(v, dict) and "b64" in v)}
    plan.make_setup(root, rest)
    for rel, spec in files.items():
        if isinstance(spec, dict) and "b64" in spec:
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(base64.b64decode(spec["b64"]))


def _snapshot(root: Path) -> dict[str, bytes]:
    return {
        p.relative_to(root).as_posix(): p.read_bytes()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def run_case(name: str, files: dict[str, Any], argv: list[str]) -> dict[str, Any]:
    """Make the setup, run the CLI's ``main`` on ``argv`` and record what it did."""
    import wraithguard_toolkit as tk
    from wraithguard.viz import cellmap

    case: dict[str, Any] = {"name": name, "files": files, "argv": argv}
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        _build(root, files)
        before = _snapshot(root)
        out, err = io.StringIO(), io.StringIO()
        code = 0
        with (
            mock.patch.object(
                sys, "argv", ["wraithguard_toolkit.py", *plan._rooted(argv, str(root))]
            ),
            mock.patch.object(tk, "datetime", plan._FixedClock),
            mock.patch.object(cellmap, "datetime", plan._FixedClock),
            contextlib.redirect_stdout(out),
            contextlib.redirect_stderr(err),
        ):
            try:
                tk.main()
            except SystemExit as stop:
                if isinstance(stop.code, str):
                    err.write(stop.code + "\n")
                    code = 1
                else:
                    code = stop.code or 0
            finally:
                for h in list(logging.getLogger("wraithguard").handlers):
                    logging.getLogger("wraithguard").removeHandler(h)
        after = _snapshot(root)
        written: dict[str, str] = {}
        for rel, raw in after.items():
            if before.get(rel) != raw:
                written[rel] = plan.unrooted(raw.decode("utf-8").replace("\r\n", "\n"), str(root))
        case["stdout"] = plan.unrooted(out.getvalue().split("\n"), str(root))
        case["stderr"] = plan.unrooted(err.getvalue().split("\n"), str(root))
        case["exit"] = code
        case["written"] = written
    return case


def main() -> int:
    """Record every case into the fixture."""
    cases = [run_case(*c) for c in _cases_in()]
    about = (
        "The command line: setups made on disk and a command line, answered by "
        "wraithguard_toolkit.py (what it prints on stdout and stderr, how it exits, the files it "
        "leaves); run by viewer-shell/cli/tests/cli_cases.rs against the Rust binary. "
        "Regenerate: python tools/gen_cli_cases.py"
    )
    OUT.write_text(
        json.dumps({"_about": about, "cases": cases}, indent=1, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"{len(cases)} cases -> {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
