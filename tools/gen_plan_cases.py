r"""The setups of ``tests/fixtures/plan_cases.json``, and a runner for them.

Each case is a setup made on disk (a cfg, rule files, a subset file or a customizations TOML,
MOMW's file, a list's data-path cache, mods folders with plugins in them) and the command line
the run is given; the answer is what ``compute_plan`` and ``write_plan`` give: the plan, every
line printed and every warning logged, the files an export leaves, or why the run stops.

The answers were recorded from the Python sort before it was retired (port step 4.2 slice 2k,
2026-10-26), and ``ordercore::plan`` reproduced every one; the file is now the Rust's test
input (``viewer-shell/ordercore/tests/plan_cases.rs``) and the Python bridge's
(``tests/test_plan_cases.py``). Running this again re-records them from ordercore, so do it only
to add a case, and read the new answers.

Paths are written with ``<root>`` for the setup's folder and ``/`` between names.

Run: ``python tools/gen_plan_cases.py``.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import struct
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "plan_cases.json"


#: The ``.bak`` stamp an export's backups carry here.
STAMP = "20261022-120000"


class _FixedClock(datetime):
    """``datetime`` that always says it is 2026-10-22 12:00:00."""

    @classmethod
    def now(cls, tz: Any = None) -> _FixedClock:
        return cls(2026, 10, 22, 12, 0, 0)


# ---------------------------------------------------------------- the setups

RULES = """\
; a rule file in mlox's form
[Order]
Morrowind.esm
Tribunal.esm
Bloodmoon.esm

[Order]
Alpha.esp
Beta.esp

[Order]
Curated Two.esp
Curated One.esp

[Order]
Base1.esp
[ANY X.esp Y.esp]
Late Patch.esp

[NearEnd]
Late*.esp

[Requires]
Needy.esp
 Missing Master.esm

[Conflict]
Alpha.esp
Clash.esp

[Note]
 Beta is a test plugin.
Beta.esp
"""

USER_RULES = """\
[Order]
Gamma.esp
Alpha.esp
"""


def _plugin(*masters: tuple[str, int], mtime: int | None = None) -> dict[str, Any]:
    spec: dict[str, Any] = {"masters": [list(m) for m in masters]}
    if mtime is not None:
        spec["mtime"] = mtime
    return spec


BASE_MODS: dict[str, Any] = {
    "rules/mlox_base.txt": RULES,
    "rules/mlox_user.txt": USER_RULES,
    "rules/readme.md": "not a rule file",
    "game/Morrowind.esm": _plugin(),
    "game/Tribunal.esm": _plugin(("Morrowind.esm", 0)),
    "game/Curated One.esp": _plugin(("Morrowind.esm", 0)),
    "game/Curated Two.esp": _plugin(("Morrowind.esm", 0)),
    "mods/A/Alpha.esp": _plugin(("Morrowind.esm", 0), ("Tribunal.esm", 0)),
    "mods/B/Beta.esp": _plugin(("Alpha.esp", 0)),
    "mods/B/Meshes/b.nif": "nif",
    "mods/C/Clash.esp": _plugin(("Morrowind.esm", 0)),
    "mods/C/Gamma.esp": _plugin(),
    "mods/N/Needy.esp": _plugin(("Missing Master.esm", 0)),
    "mods/L/Late Patch.esp": _plugin(("Beta.esp", 0)),
    "mods/G/Vurt Grass.esp": _plugin(),
}

CFG = """\
# openmw.cfg
data="<root>/game"
data="<root>/mods/G"
content=Morrowind.esm
content=Tribunal.esm
content=Curated One.esp
content=Curated Two.esp
groundcover=Vurt Grass.esp
fallback=foo,bar
"""

SUBSET = """\
# my plugins
Alpha.esp
Beta.esp   # a note
<root>/mods/A
<root>/mods/B
groundcover=New Grass.esp
groundcover=notaplugin
readme
Vurt Grass.esp
"""

CUSTOMIZATIONS = """\
[[Customizations]]
listName = "test-list"

[[Customizations.insert]]
insert = "<root>/mods/C"
after = "<root>/game"

[[Customizations.insert]]
insertBlock = '''
Clash.esp
Gamma.esp
'''

[[Customizations.replace]]
source = "Curated One.esp"
dest = "Late Patch.esp"
"""

YML = """\
- file_name: Morrowind.esm
  on_lists: [test-list]
- file_name: Tribunal.esm
  on_lists: [test-list]
- file_name: Curated One.esp
  on_lists: [test-list]
  needs_cleaning: true
- file_name: Curated Two.esp
  on_lists: [test-list]
- file_name: Beta.esp
  on_lists: [test-list]
- file_name: Gamma.esp
  on_lists: [other-list]
"""

CACHE = "# data paths\ngame\nmods/G\nmods/Z\n"


def _cases_in() -> list[tuple[str, dict[str, Any], list[str], dict[str, Any]]]:
    """``(name, files, argv, extra attributes)``; an ``export`` in the extras is written out."""
    cfg = {**BASE_MODS, "openmw.cfg": CFG}
    rules = ["--rules", "<root>/rules/mlox_base.txt", "<root>/rules"]
    out: list[tuple[str, dict[str, Any], list[str], dict[str, Any]]] = []
    out.append(
        (
            "a subset file, sorted with its folders",
            {**cfg, "subset.txt": SUBSET},
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset-file",
                "<root>/subset.txt",
                "--sort-data-paths",
                "--groundcover",
                "Extra Grass.esp",
                "--write-cfg",
                "--emit-toml",
                "<root>/out.toml",
            ],
            {"export": {}},
        )
    )
    out.append(
        (
            "a subset file, folders not sorted",
            {**cfg, "subset.txt": SUBSET},
            ["--cfg", "<root>/openmw.cfg", *rules, "--subset-file", "<root>/subset.txt"],
            {"export": {}},
        )
    )
    out.append(
        (
            "a customizations TOML with MOMW's file and a list",
            {**cfg, "customizations.toml": CUSTOMIZATIONS, "plugin-order.yml": YML},
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--customizations",
                "<root>/customizations.toml",
                "--plugin-order-yml",
                "<root>/plugin-order.yml",
                "--list-name",
                "test-list",
                "--sort-data-paths",
                "--emit-toml",
                "<root>/out.toml",
                "--subset",
                "Beta.esp",
                "--write-cfg",
                "--no-backup",
            ],
            {
                "export": {
                    "final_order": [
                        "Morrowind.esm",
                        "Tribunal.esm",
                        "Curated One.esp",
                        "Late Patch.esp",
                        "Curated Two.esp",
                        "Gamma.esp",
                    ],
                    "data_order": [
                        'data="<root>/mods/C"',
                        'data="<root>/game"',
                        'data="<root>/mods/G"',
                    ],
                    "disabled_plugins": ["Clash.esp", "Curated Two.esp"],
                    "disabled_data": ['data="<root>/mods/G"'],
                }
            },
        )
    )
    out.append(
        (
            "MOMW's file without a list name",
            {**cfg, "plugin-order.yml": YML},
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset",
                "Alpha.esp",
                "Beta.esp",
                "--plugin-order-yml",
                "<root>/plugin-order.yml",
            ],
            {},
        )
    )
    out.append(
        (
            "a list that has nothing",
            {**cfg, "plugin-order.yml": YML},
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset",
                "Alpha.esp",
                "--plugin-order-yml",
                "<root>/plugin-order.yml",
                "--list-name",
                "nope",
            ],
            {},
        )
    )
    out.append(
        (
            "orphans from the cfg, with a data-paths cache",
            {
                **cfg,
                "openmw.cfg": CFG.replace(
                    "content=Curated Two.esp\n",
                    "content=Curated Two.esp\ncontent=Hand Added.esp\ncontent=clean_Curated One.esp\n",
                ).replace('data="<root>/mods/G"\n', 'data="<root>/mods/G"\ndata=<root>/hand\n'),
                "plugin-order.yml": YML,
                "cache.txt": CACHE,
                "hand/Hand Added.esp": _plugin(("Morrowind.esm", 0)),
            },
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset-from-cfg",
                "--plugin-order-yml",
                "<root>/plugin-order.yml",
                "--list-name",
                "test-list",
                "--data-paths-cache",
                "<root>/cache.txt",
                "--emit-toml",
                "<root>/out.toml",
            ],
            {"export": {}},
        )
    )
    out.append(
        (
            "orphans without a cache, listed missing paths",
            {
                **cfg,
                "openmw.cfg": CFG.replace(
                    "content=Tribunal.esm\n", "content=Tribunal.esm\ncontent=Mine.esp\n"
                ),
            },
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset-from-cfg",
                "--subset",
                "Gamma.esp",
                "-v",
            ],
            {},
        )
    )
    out.append(
        (
            "a missing MOMW file and cache",
            cfg,
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset",
                "Alpha.esp",
                "--plugin-order-yml",
                "<root>/nope.yml",
                "--data-paths-cache",
                "<root>/nope.txt",
            ],
            {},
        )
    )
    out.append(
        (
            "a cache against the cfg, listed",
            {**cfg, "cache.txt": "mods/G\ngame\nmods/Q\n"},
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset",
                "Alpha.esp",
                "--no-predicate-warnings",
                "--data-paths-cache",
                "<root>/cache.txt",
                "--verbose",
            ],
            {},
        )
    )
    out.append(("nothing to sort", cfg, ["--cfg", "<root>/openmw.cfg", *rules], {}))
    out.append(
        (
            "nothing to sort, shown for editing",
            cfg,
            ["--cfg", "<root>/openmw.cfg", *rules],
            {"allow_empty_sort": True},
        )
    )
    out.append(
        (
            "a subset of only grass",
            cfg,
            ["--cfg", "<root>/openmw.cfg", *rules, "--subset", "Vurt Grass.esp"],
            {},
        )
    )
    out.append(
        (
            "a scan written to the subset file",
            {**cfg, "scan/One/Late Patch.esp": _plugin(("Beta.esp", 0)), "scan/One/Textures": None},
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--scan-dir",
                "<root>/scan",
                "--subset-file",
                "<root>/scanned.txt",
                "--sort-data-paths",
            ],
            {},
        )
    )
    out.append(
        (
            "a scan with nowhere to write",
            cfg,
            ["--cfg", "<root>/openmw.cfg", *rules, "--scan-dir", "<root>/mods"],
            {},
        )
    )
    out.append(
        (
            "a scan held in memory",
            cfg,
            ["--cfg", "<root>/openmw.cfg", *rules],
            {
                "subset_lines": [
                    "<root>/mods/A",
                    "Alpha.esp",
                    "",
                    "<root>/mods/C",
                    "Clash.esp",
                    "groundcover=Mem Grass.esp",
                    "huh",
                ]
            },
        )
    )
    out.append(
        (
            "one folder in memory, not sorted",
            cfg,
            ["--cfg", "<root>/openmw.cfg", *rules],
            {"subset_lines": ["<root>/mods/A", "Alpha.esp"]},
        )
    )
    out.append(
        (
            "a merged file gone stale, and an order the rules turn round",
            {
                **cfg,
                "openmw.cfg": CFG.replace(
                    'data="<root>/mods/G"\n', 'data="<root>/mods/G"\ndata="<root>/merged"\n'
                ).replace(
                    "content=Curated Two.esp\n",
                    "content=Curated Two.esp\ncontent=delta-merged.omwaddon\n",
                ),
                "merged/delta-merged.omwaddon": _plugin(mtime=1_000_000),
                "game/Curated One.esp": _plugin(("Morrowind.esm", 0), mtime=2_000_000),
                "game/Curated Two.esp": _plugin(("Morrowind.esm", 0), mtime=500_000),
                "game/Morrowind.esm": _plugin(mtime=3_000_000),
                "game/Tribunal.esm": _plugin(("Morrowind.esm", 0), mtime=4_000_000),
                "game/z.omwscripts": "PLAYER: x.lua",
            },
            [
                "--cfg",
                "<root>/openmw.cfg",
                *rules,
                "--subset",
                "Gamma.esp",
                "Alpha.esp",
                "Needy.esp",
                "Clash.esp",
                "--write-cfg",
                "--emit-toml",
                "<root>/out.toml",
                "--dry-run",
            ],
            {"export": {}},
        )
    )
    return out


# ---------------------------------------------------------------- running a case


def _rooted(value: Any, root: str) -> Any:
    if isinstance(value, str):
        return value.replace("<root>", root)
    if isinstance(value, list):
        return [_rooted(v, root) for v in value]
    if isinstance(value, dict):
        return {k: _rooted(v, root) for k, v in value.items()}
    return value


def unrooted(value: Any, root: str) -> Any:
    """``value`` with the setup's folder as ``<root>`` and every backslash a slash."""
    if isinstance(value, str):
        doubled = root.replace("\\", "\\\\")
        return (
            value.replace(doubled, "<root>")
            .replace(root, "<root>")
            .replace("\\\\", "/")
            .replace("\\", "/")
        )
    if isinstance(value, (list, tuple)):
        return [unrooted(v, root) for v in value]
    if isinstance(value, dict):
        return {unrooted(k, root): unrooted(v, root) for k, v in value.items()}
    if isinstance(value, (set, frozenset)):
        return sorted(unrooted(v, root) for v in value)
    return value


def plugin_bytes(masters: list[list[Any]], *, data: bool = True, pad: int = 0) -> bytes:
    """A TES3 plugin with a header and the given ``[name, recorded size]`` masters."""
    body = b"HEDR" + struct.pack("<I", 300) + struct.pack("<fI", 1.3, 0) + b"\0" * 32 + b"\0" * 256
    body += struct.pack("<I", 0)
    for name, size in masters:
        raw = name.encode("cp1252") + b"\0"
        body += b"MAST" + struct.pack("<I", len(raw)) + raw
        if data:
            body += b"DATA" + struct.pack("<IQ", 8, size)
    return b"TES3" + struct.pack("<III", len(body), 0, 0) + body + b"\0" * pad


def build_tree(root: Path, files: dict[str, Any]) -> None:
    """Write ``files`` (``{relative path: spec}``) under ``root``: a plugin spec
    ``{"masters": [[name, size]], "data": bool, "pad": n}``, a string for a text file, or
    ``None`` for an empty folder."""
    for rel, spec in files.items():
        path = root / rel
        if spec is None:
            path.mkdir(parents=True, exist_ok=True)
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(spec, str):
            path.write_bytes(spec.encode("utf-8"))
        else:
            path.write_bytes(
                plugin_bytes(spec.get("masters", []), data=spec.get("data", True), pad=spec.get("pad", 0))
            )


def make_setup(root: Path, files: dict[str, Any]) -> None:
    """Write the setup: ``build_tree``'s files with ``<root>`` filled in, and their times."""
    filled = {k: _rooted(v, str(root)) for k, v in files.items()}
    build_tree(root, dict(filled))
    for rel, spec in filled.items():
        if isinstance(spec, dict) and "mtime" in spec:
            os.utime(root / rel, (spec["mtime"], spec["mtime"]))


def run_case(
    name: str, files: dict[str, Any], argv: list[str], extra: dict[str, Any]
) -> dict[str, Any]:
    """Make the setup, run ``compute_plan`` and record what it gave."""
    sys.path.insert(0, str(ROOT))
    import wraithguard_toolkit as tk

    case: dict[str, Any] = {"name": name, "files": files, "argv": argv, "extra": extra}
    with tempfile.TemporaryDirectory() as tmp:
        root = str(Path(tmp).resolve())
        make_setup(Path(root), files)
        args = tk.build_arg_parser().parse_args(_rooted(argv, root))
        for key, value in _rooted(extra, root).items():
            if key != "export":
                setattr(args, key, value)
        logged: list[str] = []

        def warning(msg: str, *a: Any) -> None:
            logged.append(msg % a[0] if a and isinstance(a[0], dict) else msg % a if a else msg)

        out = io.StringIO()
        with mock.patch.object(tk._LOG, "warning", warning), contextlib.redirect_stdout(out):
            try:
                plan = tk.compute_plan(args)
            except SystemExit as stop:
                plan = None
                case["refused"] = str(stop.code)
        case["printed"] = unrooted(out.getvalue().split("\n")[:-1], root)
        case["logged"] = unrooted(logged, root)
        if plan is not None:
            keep = (
                "subset", "lines", "content_positions", "data_positions", "data_order",
                "final_order", "data_result", "predicate_warnings", "yml_warnings",
                "master_warnings", "master_problem_plugins", "custom_anchors",
                "original_content_values", "replace_dest_names", "raw_toml_data_inserts",
                "data_inserts", "orphan_data_paths", "base_order_names", "curated_set",
                "new_groundcover", "scan_dirs", "custom_data_dirs",
            )  # fmt: skip
            case["plan"] = json.loads(json.dumps(unrooted({k: plan[k] for k in keep}, root)))
        if plan is not None and "export" in extra:
            case["exported"] = export_case(tk, args, plan, _rooted(extra["export"], root), root)
        if (Path(root) / "scanned.txt").exists():
            case["scanned"] = unrooted(
                (Path(root) / "scanned.txt").read_text(encoding="utf-8"), root
            )
    return case


def export_case(
    tk: Any, args: Any, plan: dict[str, Any], spec: dict[str, Any], root: str
) -> dict[str, Any]:
    """``write_plan`` on the plan: what it printed and returned, and the files it left."""
    before = {p.name for p in Path(root).iterdir()}
    out = io.StringIO()
    with (
        mock.patch.object(tk, "datetime", _FixedClock),
        contextlib.redirect_stdout(out),
    ):
        result = tk.write_plan(
            args,
            plan,
            spec.get("final_order"),
            spec.get("data_order"),
            spec.get("disabled_plugins"),
            spec.get("disabled_data"),
        )
    files = {}
    for p in sorted(Path(root).iterdir()):
        if p.is_file() and (p.name not in before or p.name in ("openmw.cfg", "out.toml")):
            files[p.name] = unrooted(p.read_bytes().decode("utf-8").replace("\r\n", "\n"), root)
    return {
        "printed": unrooted(out.getvalue().split("\n")[:-1], root),
        "wrote_cfg": result["wrote_cfg"],
        "wrote_toml": result["wrote_toml"],
        "files": files,
    }


def cases() -> list[dict[str, Any]]:
    """The cases, answered by ``compute_plan`` / ``write_plan`` (ordercore, through native)."""
    sys.path.insert(0, str(ROOT))
    return [run_case(*c) for c in _cases_in()]


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    doc = {
        "_about": (
            "A whole sort: setups made on disk and the command line, answered by "
            "wraithguard_toolkit.compute_plan (the plan, what it prints and logs, or why it stops); "
            "run by tests/test_plan_cases.py and viewer-shell/ordercore/tests/plan_cases.rs. "
            "Regenerate: python tools/gen_plan_cases.py"
        ),
        "cases": cases(),
    }
    OUT.write_bytes((json.dumps(doc, indent=1, ensure_ascii=False) + "\n").encode("utf-8"))
    print(f"{len(doc['cases'])} cases -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
