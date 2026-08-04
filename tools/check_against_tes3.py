r"""Compare this reader's type coverage against Greatness7's `tes3` library.

`tes3` is MIT (repository root `LICENSE`), and the NIF library inside
`io_scene_mw` was relicensed MIT for `lib/es3/` in commit `cbe18b5` on
28 July 2026. Both may therefore be read. See ``NIF_PROVENANCE.md`` for the
exact boundary - in `io_scene_mw` it is `lib/es3/` and nothing else.

**Why this exists.** The corpus measurement - 100% of 7,343 vanilla meshes,
~99% of 80,197 mod meshes - is evidence about files that have been *seen*. It
says nothing about a block type no file in the corpus happens to contain. That
kind of gap produces no failure, no warning and no test result; it is invisible
by construction, and the only instrument that detects it is a second
implementation of the same format.

It found one within the first half hour of reading `tes3`: `NiUnionBV`, bound
type 4, which is a recursive list rather than a fixed width. Nothing in either
corpus carries one, so nothing here could have known.

**What this tool does and does not claim.** It compares the *set of types* each
implementation knows. It does not compare field layouts - those live in a Rust
struct on one side and a layout tuple on the other, and reconciling them is a
reading job, not a diffing job. What this gives you is the list to read.

A type we lack is not automatically a bug: `tes3` targets every TES3-era NIF
including ones Morrowind never shipped, and this reader only needs what appears
in real load orders. The output separates those questions rather than merging
them.

Usage:
    python tools/check_against_tes3.py ../tes3-main
    python tools/check_against_tes3.py ../tes3-main --corpus-types NifCorpus/AllNames.txt
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from wraithguard.nif.blocks import BLOCK_LAYOUTS, FIXED_WIDTHS

#: Rust struct declarations name the type exactly as the NIF does.
_STRUCT = re.compile(r"^pub struct ([A-Za-z0-9_]+)", re.MULTILINE)

#: Types that are structural to `tes3` rather than blocks a file contains: key
#: types, bound helpers, enums and the like. They have no counterpart here
#: because this reader never names them separately.
_NOT_BLOCKS = frozenset(
    {
        "NiBound", "NiBoxBV", "NiSphereBV", "NiUnionBV", "NiBoundingVolume",
        "NiColorKey", "NiFloatKey", "NiPosKey", "NiRotKey", "NiTextKey", "NiVisKey",
        "NiSkinPartition", "NiSkinWeight", "NiTexturingPropertyMap",
    }
)  # fmt: skip


def tes3_types(root: Path) -> set[str]:
    """Read every NIF type `tes3` declares.

    Args:
        root: The checkout root, containing ``libs/nif``.

    Returns:
        Type names.

    Raises:
        SystemExit: If the path does not look like a `tes3` checkout.
    """
    types_dir = root / "libs" / "nif" / "src" / "types"
    if not types_dir.is_dir():
        raise SystemExit(f"not a tes3 checkout: {types_dir} does not exist")
    found: set[str] = set()
    for path in sorted(types_dir.glob("*.rs")):
        found.update(_STRUCT.findall(path.read_text(encoding="utf-8", errors="replace")))
    return found


def ours() -> set[str]:
    """Every block type this reader can walk.

    Returns:
        Type names, from both the layout table and the fixed-width table.
    """
    return set(BLOCK_LAYOUTS) | set(FIXED_WIDTHS)


def corpus_types(path: Path) -> set[str]:
    """Read the block types a real corpus actually contains.

    This is what turns "they have a type we lack" into "they have a type we
    lack *and real files use it*", which is a different and much sharper claim.

    Args:
        path: A file listing block type names, one per line.

    Returns:
        Type names, or an empty set when the file is absent.
    """
    if not path.is_file():
        return set()
    names: set[str] = set()
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        cleaned = line.strip()
        if cleaned and not cleaned.startswith("#"):
            names.add(cleaned.split()[0])
    return names


def main(argv: list[str] | None = None) -> int:
    """Report the coverage difference.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code. Non-zero only when a type is missing that a real
        corpus is known to contain, because that is the only case that is
        unambiguously a gap rather than a scope difference.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("tes3", type=Path, help="a tes3 checkout")
    parser.add_argument(
        "--corpus-types", type=Path, default=None, help="a list of block types real files contain"
    )
    args = parser.parse_args(argv)

    theirs = tes3_types(args.tes3)
    mine = ours()
    blocks = {name for name in theirs if name not in _NOT_BLOCKS}
    corpus = corpus_types(args.corpus_types) if args.corpus_types else set()

    print(f"tes3 declares {len(theirs)} type(s), {len(blocks)} of them file blocks")
    print(f"this reader walks {len(mine)}")

    missing = sorted(blocks - mine)
    extra = sorted(mine - theirs)

    if corpus:
        confirmed = [name for name in missing if name in corpus]
        unseen = [name for name in missing if name not in corpus]
        print(f"\nmissing AND present in the corpus ({len(confirmed)}) -- real gaps:")
        for name in confirmed:
            print(f"  {name}")
        if not confirmed:
            print("  none")
        print(f"\nmissing but absent from the corpus ({len(unseen)}) -- scope, not gaps:")
        print("  " + (", ".join(unseen) if unseen else "none"))
        print("\n  These are the interesting ones anyway: the corpus cannot")
        print("  prove a type is unused, only that it has not been seen.")
    else:
        print(f"\ntes3 knows {len(missing)} type(s) this reader does not:")
        for name in missing:
            print(f"  {name}")
        print("\n  Pass --corpus-types to separate real gaps from scope differences.")

    if extra:
        print(f"\nthis reader names {len(extra)} type(s) tes3 does not:")
        print("  " + ", ".join(extra))
        print("  (worth checking: a name only we use may be a misreading)")

    if corpus and any(name in corpus for name in missing):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
