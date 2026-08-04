r"""Report which mods fight over terrain, and how much a merge would recover.

**This writes nothing.** It converts plugins to JSON, builds the reference
landmass from the masters, diffs every mod against it, and prints what it
found. Run it on a real load order before deciding whether merging is worth
doing -- the answer is a number, not an opinion, and it costs nothing to get.

**What the two numbers mean.** For each cell that more than one plugin edits,
vertices split two ways:

* *contested* -- more than one plugin moved this vertex. A merge has to choose,
  and a conflict strategy decides how.
* *mergeable* -- exactly one plugin moved it. The load order throws this away
  and keeps only the last plugin's whole record; a merge keeps every one.

The mergeable count is the case for merging. If it is small, the load order is
already doing an adequate job and a merged plugin adds risk for little gain.

**Land textures are resolved before anything is compared**, which is not
optional. Each plugin numbers its own textures from zero, so three mods in a
sample of two hundred all called index 0 something different. Comparing raw
indices would report imaginary conflicts and, worse, a merge built on them
would repaint terrain. See :mod:`wraithguard.land.textures`.

**Two sources, one of them much faster.** Point it at a ``Data Files``
directory and it runs tes3conv over every plugin, or point ``--json-dir`` at an
existing tes3conv dump and it reads that instead. The dump is the same data
without the conversion cost, so it is the better option when one is to hand.

**Load order is not alphabetical, and this matters.** Texture indices are
resolved against the table as it stands at each plugin's position, so surveying
in the wrong order mistranslates them. A ``Data Files`` scan has no load order
available either -- neither source does -- so pass ``--order`` with a plugin
list when the result needs to be exact. Without it the survey is a good
estimate of *where* the conflicts are and a poor one about textures.

Usage:
    python tools/survey_landscape.py "E:/OpenMW/Morrowind/Data Files"
    python tools/survey_landscape.py --json-dir tes3conv_json
    python tools/survey_landscape.py --json-dir tes3conv_json --order plugins.txt
    python tools/survey_landscape.py "E:/.../Data Files" --limit 100 --cells 40
"""

from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Final

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from tools.check_plugin_roundtrip import find_tes3conv
from wraithguard.land.landmass import PluginRecords, build_reference, survey

#: The vanilla masters, in the order the engine loads them. Anything else in
#: the folder is treated as a mod.
VANILLA_MASTERS: Final[tuple[str, ...]] = ("Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm")

#: Extensions holding TES3 records.
PLUGIN_SUFFIXES: Final[frozenset[str]] = frozenset({".esp", ".esm", ".omwaddon", ".omwgame"})

#: How many contested cells to detail unless told otherwise.
CELLS_SHOWN: Final = 20

#: Sidecar files the toolkit writes alongside a tes3conv dump. They hold
#: extracted cell and key summaries, not records, and reading them as plugins
#: would invent landscape edits that do not exist.
SIDECAR_SUFFIXES: Final[tuple[str, ...]] = (".cells.json", ".keys.json")


#: Read size for the landscape pre-scan.
_CHUNK: Final = 1 << 20

#: Longest overlap needed so the marker is not missed across a chunk boundary.
_OVERLAP: Final = len("Landscape") - 1


def mentions_landscape(path: Path) -> bool:
    """Cheaply decide whether a converted plugin is worth parsing.

    Most plugins in a load order contain no terrain at all -- of 1,036 in this
    repository's dump, the overwhelming majority hold only objects, dialogue
    and scripts. Decoding a hundred-megabyte JSON document to discover it has
    no ``Landscape`` record costs far more than scanning its bytes for the
    word, and the scan cannot produce a false negative: every landscape and
    land-texture record type contains it.

    A false *positive* is harmless -- a plugin merely mentioning the word in a
    script or an id gets parsed and found to have nothing, which is the
    behaviour without this check.

    Args:
        path: A tes3conv JSON file.

    Returns:
        ``True`` when the file might hold landscape records.
    """
    try:
        with path.open("rb") as handle:
            tail = b""
            while chunk := handle.read(_CHUNK):
                if b"Landscape" in tail + chunk:
                    return True
                tail = chunk[-_OVERLAP:]
    except OSError:
        return False
    return False


def read_json_records(path: Path) -> list[dict[str, object]]:
    """Read one already-converted plugin.

    Args:
        path: A tes3conv JSON file.

    Returns:
        The records, or an empty list when the file cannot be read.
    """
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError, MemoryError):
        return []
    return data if isinstance(data, list) else []


#: Extensions a load-order entry may carry. Used to tell a plugin line from
#: the settings that surround it in a real config file.
_ORDER_SUFFIXES: Final[tuple[str, ...]] = (".esm", ".esp", ".omwaddon", ".omwgame")


def read_order(path: Path) -> list[str]:
    """Read a load order from a config file or a plain plugin list.

    Three formats are understood, because all three are what people actually
    have to hand:

    * ``openmw.cfg`` -- ``content=Name.esp`` lines, in order.
    * ``Morrowind.ini`` -- ``GameFile0=Name.esp`` under ``[Game Files]``.
    * a plain list, one plugin per line, as mlox prints.

    **Why this is strict about what counts as a plugin.** An earlier version
    took every non-comment line as a name. Handed a real ``openmw.cfg`` it
    produced 2,836 "plugins" -- ``encoding=win1252``, ``fallback=...`` -- none
    of which matched anything, so the load order silently had no effect *and*
    the "no order given" warning did not fire, because an order had nominally
    been supplied. The run merged alphabetically while reporting that it had
    not. Requiring a plugin extension makes that failure impossible: a file
    with no recognisable plugin lines is refused outright.

    Args:
        path: The config or list file.

    Returns:
        Plugin names, lowercased for matching, in load order.

    Raises:
        SystemExit: If the file cannot be read, or holds no plugin entries.
    """
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError as exc:
        raise SystemExit(f"could not read the load order: {exc}") from exc

    names: list[str] = []
    for line in lines:
        cleaned = line.strip()
        if not cleaned or cleaned.startswith(("#", ";", "[")):
            continue
        # openmw.cfg is content=Name.esp; Morrowind.ini is GameFileN=Name.esp.
        # A plain list has no '=' at all. Splitting on the first '=' handles
        # every case, and the suffix check below rejects the settings lines
        # that share the file with them.
        value = cleaned.split("=", 1)[1].strip() if "=" in cleaned else cleaned
        name = Path(value).name
        if name.lower().endswith(_ORDER_SUFFIXES):
            names.append(name.lower())

    if not names:
        raise SystemExit(
            f"{path} holds no plugin entries. Expected openmw.cfg 'content=' "
            "lines, Morrowind.ini 'GameFile' lines, or one plugin name per "
            "line. Refusing to continue: proceeding would merge in "
            "alphabetical order while appearing to honour a load order."
        )
    return names


def apply_order(names: list[str], order: list[str]) -> list[str]:
    """Sort plugin stems by a load order, keeping unlisted ones at the end.

    Args:
        names: Plugin file stems, as found.
        order: Plugin names from :func:`read_order`.

    Returns:
        The names, load-ordered.
    """
    position = {Path(name).stem.lower(): index for index, name in enumerate(order)}
    # Unlisted plugins sort after everything named, in their original order,
    # rather than being dropped: a load order file that is merely incomplete
    # should narrow the guesswork, not discard data.
    return sorted(names, key=lambda name: (position.get(name.lower(), len(order)), name.lower()))


def to_records(tool: str, plugin: Path, scratch: Path) -> list[dict[str, object]]:
    """Convert one plugin and decode its records.

    Args:
        tool: The tes3conv executable.
        plugin: The plugin file.
        scratch: A directory for the intermediate JSON.

    Returns:
        The records, or an empty list when the plugin could not be read. A
        plugin that will not convert is reported by the caller and skipped:
        one bad file in a load order is not a reason to abandon the survey.
    """
    target = scratch / (plugin.stem + ".json")
    try:
        result = subprocess.run(  # noqa: S603 -- fixed argv, no shell
            [tool, str(plugin), str(target), "--overwrite"],
            capture_output=True,
            text=True,
            timeout=600,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return []
    if result.returncode != 0 or not target.is_file():
        return []
    try:
        data = json.loads(target.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return []
    finally:
        target.unlink(missing_ok=True)
    return data if isinstance(data, list) else []


#: What a source function returns: masters, mods, and the unreadable. ``None``
#: masters means the source could not be used at all and the caller should stop.
Loaded = tuple[list[PluginRecords] | None, list[PluginRecords], list[str]]


def _from_dump(folder: Path, order: list[str], limit: int) -> Loaded:
    """Read plugins from an existing tes3conv dump.

    Args:
        folder: The dump directory.
        order: A load order, or empty for alphabetical.
        limit: Stop after this many mods, or 0 for all.

    Returns:
        Masters, mods, and the names that could not be read.
    """
    if not folder.is_dir():
        print(f"not a directory: {folder}", file=sys.stderr)
        return (None, [], [])

    stems: dict[str, Path] = {}
    for path in folder.glob("*.json"):
        if path.name.endswith(SIDECAR_SUFFIXES):
            continue
        stems[path.stem] = path

    master_stems = [Path(name).stem for name in VANILLA_MASTERS]
    masters: list[PluginRecords] = []
    for stem in master_stems:
        path = stems.pop(stem, None)
        if path is None:
            continue
        records = read_json_records(path)
        if records:
            masters.append(PluginRecords(name=f"{stem}.esm", records=records))
    if not masters:
        print(
            f"none of {', '.join(VANILLA_MASTERS)} are in {folder}.\n"
            "Without the masters there is no reference terrain to diff against, "
            "and every mod would appear to have rewritten the world.",
            file=sys.stderr,
        )
        return (None, [], [])

    names = apply_order(sorted(stems), order) if order else sorted(stems)
    if limit > 0:
        names = names[:limit]

    print(f"masters: {', '.join(m.name for m in masters)}")
    print(f"mods:    {len(names)} (from {folder})\n")
    print("reading mods ...")

    mods: list[PluginRecords] = []
    unreadable: list[str] = []
    skipped = 0
    for index, stem in enumerate(names, start=1):
        path = stems[stem]
        if not mentions_landscape(path):
            skipped += 1
            continue
        records = read_json_records(path)
        if records:
            mods.append(PluginRecords(name=f"{stem}.esp", records=records))
        else:
            unreadable.append(stem)
        if index % 100 == 0:
            print(f"  {index}/{len(names)}")
    if skipped:
        print(f"  skipped {skipped} plugin(s) with no landscape data")
    return (masters, mods, unreadable)


def _from_plugins(folder: Path, order: list[str], limit: int, converter: str | None) -> Loaded:
    """Read plugins from a Data Files directory, converting each one.

    Args:
        folder: The directory.
        order: A load order, or empty for alphabetical.
        limit: Stop after this many mods, or 0 for all.
        converter: An explicit tes3conv path, if given.

    Returns:
        Masters, mods, and the names that could not be read.
    """
    tool = find_tes3conv(converter)
    if tool is None:
        print("tes3conv was not found. Pass --tes3conv with its path.", file=sys.stderr)
        return (None, [], [])
    if not folder.is_dir():
        print(f"not a directory: {folder}", file=sys.stderr)
        return (None, [], [])

    present = {p.name.lower(): p for p in folder.iterdir() if p.is_file()}
    master_paths = [present[name.lower()] for name in VANILLA_MASTERS if name.lower() in present]
    if not master_paths:
        print(
            f"none of {', '.join(VANILLA_MASTERS)} are in {folder}.\n"
            "Without the masters there is no reference terrain to diff against, "
            "and every mod would appear to have rewritten the world.",
            file=sys.stderr,
        )
        return (None, [], [])

    master_names = {p.name.lower() for p in master_paths}
    mod_paths = [
        p
        for p in folder.iterdir()
        if p.is_file()
        and p.suffix.lower() in PLUGIN_SUFFIXES
        and p.name.lower() not in master_names
    ]
    by_name = {p.name: p for p in mod_paths}
    chosen = apply_order(sorted(by_name), order) if order else sorted(by_name)
    if limit > 0:
        chosen = chosen[:limit]

    print(f"masters: {', '.join(p.name for p in master_paths)}")
    print(f"mods:    {len(chosen)}\n")

    with tempfile.TemporaryDirectory() as scratch_dir:
        scratch = Path(scratch_dir)
        print("reading masters ...")
        masters: list[PluginRecords] = []
        for master in master_paths:
            records = to_records(tool, master, scratch)
            if not records:
                print(f"  could not read {master.name}", file=sys.stderr)
                return (None, [], [])
            masters.append(PluginRecords(name=master.name, records=records))

        print("reading mods ...")
        mods: list[PluginRecords] = []
        unreadable: list[str] = []
        for index, name in enumerate(chosen, start=1):
            records = to_records(tool, by_name[name], scratch)
            if records:
                mods.append(PluginRecords(name=name, records=records))
            else:
                unreadable.append(name)
            if index % 25 == 0:
                print(f"  {index}/{len(chosen)}")
    return (masters, mods, unreadable)


def main(argv: list[str] | None = None) -> int:
    """Survey a load order's landscape conflicts.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("folder", type=Path, nargs="?", help="a Data Files directory")
    parser.add_argument("--json-dir", type=Path, default=None, help="an existing tes3conv dump")
    parser.add_argument("--order", type=Path, default=None, help="a load order, one plugin a line")
    parser.add_argument("--tes3conv", default=None, help="path to the converter")
    parser.add_argument("--limit", type=int, default=0, help="stop after this many mods")
    parser.add_argument("--cells", type=int, default=CELLS_SHOWN, help="contested cells to detail")
    parser.add_argument("--verbose", action="store_true", help="show per-plugin warnings")
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.INFO if args.verbose else logging.ERROR,
        format="%(message)s",
    )

    if (args.folder is None) == (args.json_dir is None):
        print("give either a Data Files directory or --json-dir, not both.", file=sys.stderr)
        return 2

    order = read_order(args.order) if args.order else []
    if not order:
        print(
            "note: no --order given, so plugins are surveyed alphabetically.\n"
            "      Cells and heights are unaffected. Texture indices are resolved\n"
            "      against load order, so those may be wrong. Pass --order for an\n"
            "      exact answer.\n"
        )

    if args.json_dir is not None:
        loaded, mod_records, unreadable = _from_dump(args.json_dir, order, args.limit)
    else:
        loaded, mod_records, unreadable = _from_plugins(
            args.folder, order, args.limit, args.tes3conv
        )
    if loaded is None:
        return 2

    reference, known = build_reference(loaded)
    print(f"  reference landmass: {len(reference)} cell(s), {len(known)} land texture(s)\n")

    contention = survey(reference, mod_records, known)
    contested = {c: cell for c, cell in contention.items() if cell.is_contested}

    print(f"\n{len(contention)} cell(s) changed, {len(contested)} contested by more than one mod")
    if unreadable:
        print(f"{len(unreadable)} plugin(s) could not be read: {', '.join(unreadable[:5])}")

    if not contested:
        print("\nNo cell is edited by two mods. A merged landscape would change nothing.")
        return 0

    # New land and vanilla land are counted apart, because conflating them
    # overstates the case for merging by an order of magnitude. A cell the
    # masters never had reports all 4,225 vertices as changed -- true, and
    # useless: that is terrain a landmass mod added, not an edit a load order
    # would have discarded. On a large collection these dominate. A 1,033
    # plugin dump produces 13,819 changed cells against a 1,540 cell vanilla
    # reference, so roughly nine in ten changed cells are new.
    vanilla = {c: cell for c, cell in contested.items() if not cell.is_new_land}
    added = {c: cell for c, cell in contested.items() if cell.is_new_land}
    print(f"  of those, {len(vanilla)} are vanilla cells and {len(added)} are added land")

    if vanilla:
        ranked = sorted(vanilla.items(), key=lambda item: -item[1].height_overlap()[1])
        shown = min(args.cells, len(ranked))
        print(f"\nvanilla cells, most to gain from merging (showing {shown}):\n")
        print(f"  {'cell':>12}  {'mods':>4}  {'contested':>9}  {'mergeable':>9}  plugins")
        for coords, cell in ranked[: args.cells]:
            conflicted, mergeable = cell.height_overlap()
            names = ", ".join(name[:22] for name in cell.plugins[:3])
            if len(cell.plugins) > 3:
                names += f", +{len(cell.plugins) - 3}"
            print(
                f"  {coords!s:>12}  {len(cell.changes):>4}  "
                f"{conflicted:>9}  {mergeable:>9}  {names}"
            )

    van_conflicted = sum(cell.height_overlap()[0] for cell in vanilla.values())
    van_mergeable = sum(cell.height_overlap()[1] for cell in vanilla.values())
    add_conflicted = sum(cell.height_overlap()[0] for cell in added.values())
    add_mergeable = sum(cell.height_overlap()[1] for cell in added.values())

    print(
        f"\nvanilla cells edited by more than one mod: {van_conflicted} height "
        f"vertex/vertices\ngenuinely contested, {van_mergeable} that exactly one "
        "mod moved. The load order\ndiscards that second number; a merge keeps it."
    )
    print(
        f"\nadded land edited by more than one mod: {add_conflicted} contested, "
        f"{add_mergeable} single-mod.\nSeparate because a cell the masters never "
        "had reports every vertex as changed --\nthat is new terrain, not an edit "
        "rescued from a load order."
    )
    print(
        "\nNothing was written. This is a measurement, not a merge: it says how "
        "much\na merged landscape would recover, so the decision to build one is "
        "an informed\none rather than a hopeful one."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
