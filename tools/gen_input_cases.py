r"""Regenerate ``tests/fixtures/input_cases.json`` from the Python that gathers a sort's inputs.

The inputs are written here (and many more made from a fixed seed); the answers are what
``wraithguard_toolkit`` and ``wraithguard.momw_datapaths`` give. ``ordercore::inputs``
(``viewer-shell/ordercore``) runs the same file, so the two cannot drift.

Families:

* ``lines``: subset-file lines (``extract_subset_from_lines``, with the entries it warns about
  and skips) and the grass they declare (``extract_groundcover_declarations``);
* ``subset_tomls``: the subset file's TOML form (``extract_subset_from_subset_file`` and the
  ``groundcover`` key ``declared_groundcover`` reads);
* ``customizations``: ``extract_subset_from_toml`` over a ``customizations.toml``;
* ``holdbacks``: ``hold_back_groundcover``;
* ``scan_dirs``: ``all_scan_dirs`` and ``pending_custom_dirs``;
* ``data_paths``: a MOMW list's cached data paths read (``parse_data_paths_cache``) and set
  against a cfg's (``managed_cfg_data_path_norms``, ``reconcile_data_paths``);
* ``orphans``: the ``--subset-from-cfg`` pull (``_pull_cfg_orphans``);
* ``trees``: a small mods folder made on disk, then the mods-folder scan
  (``scan_mod_directories``), the inferred ``data=`` anchors (``infer_data_path_anchors``) and
  the header-master check (``check_missing_masters``). Paths are written with ``<root>`` for
  the folder and ``/`` between names; the scan's folders are compared in sorted order, since a
  folder lists its entries in whatever order the system keeps them.

A case marked ``declined`` is one ``ordercore`` does not read (a TOML value of a type the
Python stringifies or iterates, a byte-order mark); its answer is still the Python's, which is
what the bridge falls back to. A case with ``error`` is a file tomllib refuses.

Run: ``python tools/gen_input_cases.py``.
"""

from __future__ import annotations

import contextlib
import io
import json
import struct
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "input_cases.json"


class Rng:
    """A small generator of its own, so the file does not move with Python's ``random``."""

    def __init__(self, seed: int) -> None:
        """Start from ``seed``."""
        self.state = (seed * 2654435761 + 12345) % (1 << 32)

    def below(self, n: int) -> int:
        """A number from 0 up to, not including, ``n``."""
        self.state = (self.state * 1664525 + 1013904223) % (1 << 32)
        return (self.state >> 8) % n

    def chance(self, num: int, den: int) -> bool:
        """Whether a ``num`` in ``den`` chance came up."""
        return self.below(den) < num

    def pick(self, items: list[Any]) -> Any:
        """One of the items."""
        return items[self.below(len(items))]

    def shuffled(self, items: list[Any]) -> list[Any]:
        """The items in another order."""
        out = list(items)
        for i in range(len(out) - 1, 0, -1):
            j = self.below(i + 1)
            out[i], out[j] = out[j], out[i]
        return out


def _toolkit() -> Any:
    sys.path.insert(0, str(ROOT))
    import wraithguard_toolkit

    return wraithguard_toolkit


@contextlib.contextmanager
def _warned() -> Any:
    """Collect the ``entry`` of each warning the toolkit logs."""
    tk = _toolkit()
    seen: list[str] = []

    def warning(_msg: str, values: dict[str, Any] | None = None, *_a: Any) -> None:
        if isinstance(values, dict) and "entry" in values:
            seen.append(values["entry"])

    with mock.patch.object(tk._LOG, "warning", warning):
        yield seen


# ---------------------------------------------------------------- lines

LINE_SETS: list[tuple[str, list[str]]] = [
    ("empty", []),
    (
        "plugins and folders",
        ["Foo.esp", "Bar.ESM", "mods/Thing", "C:\\Mods\\Other", "Scripts.omwscripts"],
    ),
    (
        "comments",
        [
            "# all of it",
            "   # indented",
            "A.esp # trailing",
            "B.esp\t# tab",
            "FMI_#NotAllDunmer.ESP",
        ],
    ),
    (
        "quoted and pathed",
        ['"Quoted.esp"', "'single.esm'", "C:\\Mods\\Deep\\Inner.omwaddon", "a/b/c.ESP"],
    ),
    ("words", ["readme", "Foo", "   ", "", "esp", ".esp"]),
    (
        "grass",
        [
            "groundcover=Vurt_Grass.esp",
            "GROUNDCOVER = C:\\Mods\\Grass\\Rem_Grass.ESP",
            "groundcover=notaplugin",
            "groundcover = vurt_grass.ESP",
            "  groundcover=Aesthesia.esp  # with a note",
            "Plain.esp",
        ],
    ),
    (
        "odd spaces",
        ["\u00a0Spaced.esp\u00a0", "\x1cSep.esp", "\u2003mods/x\u2003", "a.esp\u00a0# nb"],
    ),
    ("non-ascii", ["Caf\u00e9.esp", "M\u00f6ds/\u00c9cole", "\u0130.esp", "\ufeffBom.esp"]),
]

LINE_POOL: list[str] = [
    "Foo.esp",
    "bar.ESM",
    "x.omwaddon",
    "y.omwgame",
    "z.omwscripts",
    "mods/A",
    "C:\\M\\B",
    "word",
    " ",
    "#",
    " # c",
    "groundcover=",
    "groundcover = ",
    "G.esp",
    "\t",
    '"',
    "'",
    "/",
    "\\",
    "\u00a0",
    ".esp",
    "#x",
]


def lines_case(name: str, lines: list[str]) -> dict[str, Any]:
    """One set of lines through both readers."""
    tk = _toolkit()
    with _warned() as skipped:
        plugins, data = tk.extract_subset_from_lines(lines)
    with _warned() as not_grass:
        grass = tk.extract_groundcover_declarations(lines)
    return {
        "name": name,
        "lines": lines,
        "plugins": plugins,
        "data": [d["value"] for d in data],
        "skipped": skipped,
        "groundcover": grass,
        "not_groundcover": not_grass,
    }


def _fuzz_lines(r: Rng) -> list[str]:
    return ["".join(r.pick(LINE_POOL) for _ in range(r.below(4))) for _ in range(r.below(8))]


# ---------------------------------------------------------------- the subset file's TOML

SUBSET_TOMLS: list[tuple[str, str, bool]] = [
    ("empty", "", False),
    (
        "all three",
        'subset = ["A.esp", "mods/B", "word"]\ndata = ["C:/Mods/D"]\ngroundcover = ["G.esp"]\n',
        False,
    ),
    ("grass in the subset", "subset = ['groundcover=G.esp', 'H.esp']\n", False),
    ("literal strings", "subset = ['C:\\Mods\\A.esp']\ndata = ['C:\\Mods\\B']\n", False),
    ("a number in the subset", "subset = [1, 'A.esp']\n", True),
    ("a string for the subset", "subset = 'A.esp'\n", True),
    ("a table for data", "[data]\nx = 'y'\n", True),
    ("groundcover a string", "groundcover = 'G.esp'\n", True),
    ("a byte-order mark", "\ufeffsubset = ['A.esp']\n", True),
    ("other keys are ignored", "subset = ['A.esp']\nnote = 3\n[extra]\nk = true\n", False),
    ("not TOML", "subset = [\n", False),
    ("a duplicate key", "subset = ['A.esp']\nsubset = ['B.esp']\n", False),
]


def subset_toml_case(name: str, text: str, declined: bool) -> dict[str, Any]:
    """The subset file's TOML form, read as a file and for its grass."""
    tk = _toolkit()
    case: dict[str, Any] = {"name": name, "text": text}
    if declined:
        case["declined"] = True
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "subset.toml"
        path.write_bytes(text.encode("utf-8"))
        try:
            with _warned() as skipped:
                plugins, data = tk.extract_subset_from_subset_file(path)
        except ValueError:
            case["error"] = True
            return case
        args = SimpleNamespace(groundcover=None, subset_file=path, subset_lines=None)
        case.update(
            plugins=plugins,
            data=[d["value"] for d in data],
            skipped=skipped,
            groundcover=tk.declared_groundcover(args),
        )
    return case


# ---------------------------------------------------------------- customizations.toml

CUSTOMIZATIONS: list[tuple[str, str, bool]] = [
    ("empty", "", False),
    (
        "a realistic file",
        """\
[[Customizations]]
listName = "expanded-vanilla"

[[Customizations.insert]]
insert = "C:\\\\Mods\\\\Patches\\\\My Patch"
after = "Better Bodies"

[[Customizations.insert]]
insertBlock = '''
My Patch.esp
Other Thing.ESP
C:/Mods/Folder
'''
before = "Tamriel_Data.esm"

[[Customizations.replace]]
source = "Old.esp"
dest = "New.esp"

[[Customizations.append]]
appendBlock = '''
content=Appended.esp
  content = Spaced.omwaddon
fallback=x
'''
""",
        False,
    ),
    (
        "two blocks and a plugin twice",
        """\
[[Customizations]]
listName = "one"
[[Customizations.insert]]
insert = "A.esp"
[[Customizations]]
listName = "two"
[[Customizations.insert]]
insert = "a.ESP"
[[Customizations.insert]]
insert = "B.esp"
""",
        False,
    ),
    (
        "no list name, an empty one",
        """\
[[Customizations]]
[[Customizations.insert]]
insert = "A.esp"
[[Customizations]]
listName = ""
[[Customizations.insert]]
insert = "B.esp"
""",
        False,
    ),
    (
        "a word, an empty value, a number",
        """\
[[Customizations]]
[[Customizations.insert]]
insert = "word"
[[Customizations.insert]]
insert = ""
[[Customizations.insert]]
insert = 7
""",
        False,
    ),
    (
        "insert wins over insertBlock",
        """\
[[Customizations]]
[[Customizations.insert]]
insert = "A.esp"
insertBlock = "B.esp"
""",
        False,
    ),
    (
        "carriage returns in a block",
        '[[Customizations]]\n[[Customizations.insert]]\ninsertBlock = "A.esp\\r\\nmods/B\\r\\n\\r\\nC.esp"\nafter = ""\n',
        False,
    ),
    (
        "append over appendBlock, and content across a line",
        """\
[[Customizations]]
listName = "l"
[[Customizations.append]]
append = "content=First.esp\\ncontent=\\nNext.esp"
appendBlock = "content=Ignored.esp"
[[Customizations.append]]
append = ""
appendBlock = "content=Second.esp"
""",
        False,
    ),
    (
        "replace with no source",
        '[[Customizations]]\n[[Customizations.replace]]\ndest = "Dest.esp"\n'
        '[[Customizations.replace]]\ndest = "mods/folder"\nsource = "S"\n',
        False,
    ),
    ("a number for listName", "[[Customizations]]\nlistName = 3\n", True),
    (
        "insertBlock a list",
        "[[Customizations]]\n[[Customizations.insert]]\ninsertBlock = ['A.esp']\n",
        True,
    ),
    (
        "after a number",
        "[[Customizations]]\n[[Customizations.insert]]\ninsert = 'A.esp'\nafter = 1\n",
        True,
    ),
    ("Customizations a table", "[Customizations]\nlistName = 'x'\n", True),
    (
        "an append that is a list",
        "[[Customizations]]\n[[Customizations.append]]\nappend = ['x']\n",
        True,
    ),
    ("not TOML", "[[Customizations]\n", False),
]

_NAMES = [
    "A.esp",
    "b.ESM",
    "Mod One.esp",
    "Caf\u00e9.esp",
    "x.omwaddon",
    "s.omwscripts",
    "Clean_A.esp",
]
_FOLDERS = ["C:\\Mods\\A", "mods/B", "D:/M/C D", "rel/x/", "\\\\server\\share"]


def _basic(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def _fuzz_customizations(r: Rng) -> str:
    out: list[str] = []
    for _ in range(r.below(4)):
        out.append("[[Customizations]]")
        if r.chance(2, 3):
            out.append(f"listName = {_basic(r.pick(['l1', 'l2', 'Expanded Vanilla']))}")
        for _ in range(r.below(4)):
            kind = r.below(4)
            if kind == 0:
                out.append("[[Customizations.insert]]")
                out.append(f"insert = {_basic(r.pick(_NAMES + _FOLDERS + ['word', '']))}")
            elif kind == 1:
                out.append("[[Customizations.insert]]")
                vals = [r.pick(_NAMES + _FOLDERS + ["", "word"]) for _ in range(r.below(4))]
                out.append(f"insertBlock = {_basic(chr(10).join(vals))}")
            elif kind == 2:
                out.append("[[Customizations.replace]]")
                out.append(f"dest = {_basic(r.pick(_NAMES + _FOLDERS))}")
                if r.chance(1, 2):
                    out.append(f"source = {_basic(r.pick(_NAMES))}")
            else:
                out.append("[[Customizations.append]]")
                body = "\n".join(f"content={r.pick(_NAMES)}" for _ in range(r.below(3)))
                out.append(f"{r.pick(['append', 'appendBlock'])} = {_basic(body)}")
            if kind < 2 and r.chance(1, 2):
                out.append(f"{r.pick(['after', 'before'])} = {_basic(r.pick([*_FOLDERS, '']))}")
    return "\n".join(out) + "\n"


def customizations_case(name: str, text: str, declined: bool) -> dict[str, Any]:
    """A customizations.toml read."""
    tk = _toolkit()
    case: dict[str, Any] = {"name": name, "text": text}
    if declined:
        case["declined"] = True
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "customizations.toml"
        path.write_bytes(text.encode("utf-8"))
        try:
            subset, data, replace_dest, list_names = tk.extract_subset_from_toml(path)
        except (ValueError, AttributeError, TypeError):
            case["error"] = True
            return case
    case.update(
        subset=subset,
        data=data,
        replace_dest=sorted(replace_dest),
        list_names=list_names,
    )
    return case


# ---------------------------------------------------------------- small lists

HOLDBACKS: list[tuple[list[str], list[str]]] = [
    ([], []),
    (["A.esp", "Grass.esp", "B.esp"], []),
    (["A.esp", "Grass.esp", "B.esp", "GRASS.ESP"], ["grass.esp"]),
    (["Caf\u00c9.esp", "x.esp"], ["caf\u00e9.ESP", "Y.esp"]),
]

SCAN_DIRS: list[tuple[list[str], list[str], list[str]]] = [
    ([], [], []),
    (["data=C:\\A", 'data="C:\\B"', "data=c:\\a"], ["C:\\a", "D"], ["d", "E", ""]),
    (["content=x.esp", "data="], ["X"], []),
    (["data=\u00c9"], ["\u00e9"], ["\u00c9"]),
]


def holdback_case(subset: list[str], grass: list[str]) -> dict[str, Any]:
    """``hold_back_groundcover``."""
    kept, held = _toolkit().hold_back_groundcover(subset, grass)
    return {"subset": subset, "groundcover": grass, "kept": kept, "held": held}


def scan_dirs_case(data_order: list[str], raw: list[str], inserts: list[str]) -> dict[str, Any]:
    """``all_scan_dirs`` and ``pending_custom_dirs``."""
    tk = _toolkit()
    raw_d = [{"value": v, "after": None, "before": None} for v in raw]
    ins_d = [{"value": v, "after": None, "before": None} for v in inserts]
    return {
        "data_order": data_order,
        "raw": raw,
        "inserts": inserts,
        "all": tk.all_scan_dirs(data_order, raw_d, ins_d),
        "pending": tk.pending_custom_dirs(raw_d, ins_d),
    }


# ---------------------------------------------------------------- a list's data paths

DATA_PATHS: list[tuple[str, str, list[str]]] = [
    ("no cache", "", ["C:\\Mods\\A"]),
    (
        "a list and a cfg",
        "\ufeff# MOMW data paths\nCat/ModA\nCat/ModB/Data Files\n\n  Other\\ModC  \nCat/ModD\n",
        [
            "C:\\games\\OpenMWMods\\Cat\\ModA",
            "D:/MyMods/list/cat/moda",
            "E:\\X\\Other\\ModC",
            "C:/games/OpenMWMods/Cat/ModB/Data Files",
            "C:\\Unrelated\\ModZ",
        ],
    ),
    (
        "compact keys",
        "Stra\u00dfe/Mod_01\nCat/A-B\n",
        ["C:/x/strasse/mod01", "C:/x/cat/a b", "C:/x/cat/ab"],
    ),
    ("all present in order", "A/1\nA/2\n", ["x/A/1", "x/A/2"]),
    ("a tail with no name", "/\n//\nA\n", ["x/A", "/"]),
]


def data_paths_case(name: str, text: str, cfg_values: list[str]) -> dict[str, Any]:
    """A cache read, then set against the cfg's ``data=`` values."""
    from wraithguard import momw_datapaths as md

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "cache.txt"
        path.write_bytes(text.encode("utf-8"))
        tails = md.parse_data_paths_cache(path)
    return {
        "name": name,
        "text": text,
        "cfg": cfg_values,
        "tails": tails,
        "managed": sorted(md.managed_cfg_data_path_norms(tails, cfg_values)),
        "reconcile": md.reconcile_data_paths(tails, cfg_values),
        "reconcile_listed": md.reconcile_data_paths(tails, cfg_values, report_missing=True),
    }


def _fuzz_data_paths(r: Rng) -> tuple[str, list[str]]:
    mods = [f"Cat{r.below(3)}/Mod{c}" for c in "ABCDEFG"]
    tails = [m + r.pick(["", "/Data Files", "/00 Core"]) for m in mods if r.chance(2, 3)]
    cfg = []
    for t in r.shuffled(tails):
        if r.chance(3, 4):
            base = r.pick(["C:\\games\\OpenMWMods\\", "D:/x/", "/home/u/mods/list/"])
            v = base + t
            v = v.replace("/", "\\") if r.chance(1, 2) else v
            cfg.append(v.upper() if r.chance(1, 4) else v)
    cfg.extend(f"Z:/Other/{r.below(9)}" for _ in range(r.below(3)))
    return "\n".join(tails) + "\n", r.shuffled(cfg) if r.chance(1, 3) else cfg


# ---------------------------------------------------------------- the cfg's orphans


def orphan_case(name: str, q: dict[str, Any]) -> dict[str, Any]:
    """The ``--subset-from-cfg`` pull over ``q`` (``tails`` None: no cache)."""
    tk = _toolkit()
    with tempfile.TemporaryDirectory() as tmp:
        cache = None
        if q["tails"] is not None:
            cache = Path(tmp) / "cache.txt"
            cache.write_text("\n".join(q["tails"]) + "\n", encoding="utf-8")
        args = SimpleNamespace(subset_from_cfg=True, data_paths_cache=cache)
        subset = list(q["subset"])
        with contextlib.redirect_stdout(io.StringIO()):
            got, orphan_data = tk._pull_cfg_orphans(
                args,
                subset,
                [{"value": v, "after": None, "before": None} for v in q["declared_data"]],
                [],
                {},
                {},
                q["base_order"],
                q["data_order"],
                set(q["curated"]),
                set(q["needs_cleaning"]),
                set(q["declared"]),
                set(q["groundcover"]),
            )
    return {"name": name, **q, "new_plugins": got[len(q["subset"]) :], "orphan_data": orphan_data}


ORPHANS: list[tuple[str, dict[str, Any]]] = [
    (
        "no cache",
        {
            "subset": ["Mine.esp"],
            "base_order": [
                "Morrowind.esm",
                "Curated.esp",
                "Mine.esp",
                "Orphan.esp",
                "Grass.esp",
                "clean_Dirty.esp",
            ],
            "data_order": ["data=C:\\Mods\\A", 'data="D:\\Morrowind\\Data Files"'],
            "curated": ["curated.esp", "dirty.esp"],
            "needs_cleaning": ["dirty.esp"],
            "declared": ["mine.esp"],
            "groundcover": ["grass.esp"],
            "declared_data": [],
            "tails": None,
        },
    ),
    (
        "with a cache",
        {
            "subset": [],
            "base_order": ["Orphan.esp", "Declared.esp"],
            "data_order": [
                "data=C:\\games\\OpenMWMods\\Cat\\ModA",
                "data=C:\\Hand\\Added",
                "data=C:\\Mine\\Declared",
                "data=D:\\Morrowind\\Data Files",
            ],
            "curated": [],
            "needs_cleaning": [],
            "declared": ["declared.esp"],
            "groundcover": [],
            "declared_data": ["c:/mine/declared/"],
            "tails": ["Cat/ModA"],
        },
    ),
    (
        "an empty cache",
        {
            "subset": [],
            "base_order": [],
            "data_order": ["data=C:\\A", "data=C:\\B"],
            "curated": [],
            "needs_cleaning": [],
            "declared": [],
            "groundcover": [],
            "declared_data": [],
            "tails": [],
        },
    ),
]


# ---------------------------------------------------------------- trees on disk


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
    """Write ``files`` (``{relative path: spec}``) under ``root``.

    A spec is ``{"masters": [[name, size]], "data": bool, "pad": n}`` for a plugin, a string for
    a text file, or ``None`` for an empty folder.
    """
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
                plugin_bytes(
                    spec.get("masters", []), data=spec.get("data", True), pad=spec.get("pad", 0)
                )
            )


def _rooted(value: Any, root: str) -> Any:
    if isinstance(value, str):
        return value.replace("<root>", root)
    if isinstance(value, list):
        return [_rooted(v, root) for v in value]
    if isinstance(value, dict):
        return {k: _rooted(v, root) for k, v in value.items()}
    return value


def _unrooted(value: Any, root: str) -> Any:
    if isinstance(value, str):
        return value.replace(root, "<root>").replace("\\", "/")
    if isinstance(value, (list, tuple)):
        return [_unrooted(v, root) for v in value]
    if isinstance(value, dict):
        return {k: _unrooted(v, root) for k, v in value.items()}
    return value


def scan_groups(lines: list[str]) -> list[list[str]]:
    """The scan's lines as one list a folder (its path, then its plugins), sorted."""
    groups: list[list[str]] = []
    current: list[str] = []
    for line in lines:
        if line == "":
            groups.append(current)
            current = []
        else:
            current.append(line)
    return sorted(groups)


def tree_case(name: str, files: dict[str, Any], q: dict[str, Any]) -> dict[str, Any]:
    """Build the tree in a temp folder and ask it the scan, the anchors and the master check."""
    tk = _toolkit()
    from wraithguard.configurator.datapaths import infer_data_path_anchors
    from wraithguard.plugins import PluginFileIndex

    case: dict[str, Any] = {"name": name, "files": files, **q}
    with tempfile.TemporaryDirectory() as tmp:
        root = str(Path(tmp).resolve())
        build_tree(Path(root), files)
        with contextlib.redirect_stdout(io.StringIO()):
            lines, n_folders, n_plugins = tk.scan_mod_directories(_rooted(q["scan"], root))
        case["scan_groups"] = _unrooted(scan_groups(lines), root)
        case["scan_counts"] = [n_folders, n_plugins]

        inserts = [dict(i) for i in _rooted(q["inserts"], root)]
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            infer_data_path_anchors(
                inserts, _rooted(q["data_order"], root), q["final_order"], Path(root) / "openmw.cfg"
            )
        case["anchored"] = _unrooted(inserts, root)
        case["anchor_notes"] = _unrooted(
            [ln.strip() for ln in buffer.getvalue().splitlines()], root
        )

        index = PluginFileIndex(_rooted(q["dirs"], root))
        missing, order, sizes, checked, problems = tk.check_missing_masters(
            q["active"], index, q["origins"]
        )
        case["masters"] = {
            "missing": missing,
            "order_problems": order,
            "size_notes": sizes,
            "checked": checked,
            "problem_plugins": sorted(problems, key=str.lower),
        }
    return case


TREES: list[tuple[str, dict[str, Any], dict[str, Any]]] = [
    (
        "a mods folder",
        {
            "mods/Base/Morrowind.esm": {"masters": []},
            "mods/Base/Tribunal.esm": {"masters": [["Morrowind.esm", 79]]},
            "mods/A/a.ESP": {"masters": [["Morrowind.esm", 0], ["Tribunal.esm", 10]]},
            "mods/A/b.esp": {"masters": [["Missing.esm", 1]]},
            "mods/A/Meshes/x.nif": "nif",
            "mods/B/Textures/t.dds": "dds",
            "mods/C/deep/Cee.omwaddon": {"masters": [["Later.esp", 5]]},
            "mods/C/deep/Later.esp": {"masters": []},
            "mods/C/deep/s.omwscripts": "PLAYER: x.lua",
            "mods/D/readme.txt": "nothing here",
            "mods/E/sub": None,
            "mods/F/NoData.esp": {"masters": [["Morrowind.esm", 0]], "data": False},
            "mods/F/Installed.esp": {"masters": []},
            "mods/G/Uses.esp": {"masters": [["Installed.esp", 0]]},
        },
        {
            "scan": "<root>/mods",
            "data_order": [
                "data=<root>/mods/Base",
                "data=<root>/mods/C/deep",
                'data="<root>/mods/A"',
            ],
            "final_order": [
                "Morrowind.esm",
                "Tribunal.esm",
                "a.ESP",
                "Uses.esp",
                "Later.esp",
                "Cee.omwaddon",
            ],
            "inserts": [
                {"value": "<root>/mods/G", "after": None, "before": None},
                {"value": "<root>/mods/F", "after": "", "before": None},
                {"value": "<root>/mods/A", "after": "x", "before": None},
                {"value": "<root>/mods/B", "after": None, "before": None},
                {"value": "mods/G", "after": None, "before": None},
            ],
            "dirs": [
                "<root>/mods/Base",
                "<root>/mods/A",
                "<root>/mods/C/deep",
                "<root>/mods/F",
                "<root>/mods/G",
                "<root>/nowhere",
            ],
            "active": [
                "Morrowind.esm",
                "a.ESP",
                "Tribunal.esm",
                "b.esp",
                "Cee.omwaddon",
                "Later.esp",
                "NoData.esp",
                "Uses.esp",
                "s.omwscripts",
                "Unfound.esp",
            ],
            "origins": {
                "a.esp": "customizations.toml -> 'list'",
                "uses.esp": "subset file (s.txt)",
            },
        },
    ),
    (
        "a later folder wins a name",
        {
            "one/Shared.esp": {"masters": [["Gone.esm", 0]]},
            "two/Shared.esp": {"masters": []},
            "two/Meshes": None,
        },
        {
            "scan": "<root>",
            "data_order": ["data=<root>/one"],
            "final_order": ["Shared.esp"],
            "inserts": [{"value": "<root>/two", "after": None, "before": None}],
            "dirs": ["<root>/one", "<root>/two"],
            "active": ["Shared.esp"],
            "origins": {},
        },
    ),
    (
        "nothing reachable",
        {"empty": None},
        {
            "scan": "<root>/empty",
            "data_order": [],
            "final_order": [],
            "inserts": [{"value": "<root>/empty", "after": None, "before": None}],
            "dirs": ["<root>/missing"],
            "active": ["A.esp"],
            "origins": {},
        },
    ),
]


# ---------------------------------------------------------------- the file


def cases() -> dict[str, Any]:
    """The cases, answered by the Python bodies (not ordercore's, which they check)."""
    sys.path.insert(0, str(ROOT))
    from wraithguard import ordercore_bridge

    with ordercore_bridge.python_only():
        return _cases()


def _cases() -> dict[str, Any]:
    lines = [lines_case(n, ls) for n, ls in LINE_SETS]
    lines += [lines_case(f"made up, seed {s}", _fuzz_lines(Rng(s))) for s in range(1, 201)]
    customs = [customizations_case(n, t, d) for n, t, d in CUSTOMIZATIONS]
    customs += [
        customizations_case(f"made up, seed {s}", _fuzz_customizations(Rng(s)), False)
        for s in range(1, 151)
    ]
    data_paths = [data_paths_case(n, t, c) for n, t, c in DATA_PATHS]
    for s in range(1, 101):
        text, cfg = _fuzz_data_paths(Rng(s))
        data_paths.append(data_paths_case(f"made up, seed {s}", text, cfg))
    return {
        "lines": lines,
        "subset_tomls": [subset_toml_case(n, t, d) for n, t, d in SUBSET_TOMLS],
        "customizations": customs,
        "holdbacks": [holdback_case(s, g) for s, g in HOLDBACKS],
        "scan_dirs": [scan_dirs_case(d, r, i) for d, r, i in SCAN_DIRS],
        "data_paths": data_paths,
        "orphans": [orphan_case(n, q) for n, q in ORPHANS],
        "trees": [tree_case(n, f, q) for n, f, q in TREES],
    }


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "A sort's inputs: the subset lines and TOML, the customizations TOML, grass, the "
            "folders, a list's data paths, the cfg's orphans, and a mods folder on disk (the scan, "
            "data= anchors, the master check), answered by wraithguard_toolkit and "
            "wraithguard.momw_datapaths; run by tests/test_input_cases.py and "
            "viewer-shell/ordercore/tests/input_cases.rs. Regenerate: python tools/gen_input_cases.py"
        ),
        **out,
    }
    OUT.write_bytes((json.dumps(doc, indent=1, ensure_ascii=False) + "\n").encode("utf-8"))
    print(", ".join(f"{len(v)} {k}" for k, v in out.items()) + f" -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
