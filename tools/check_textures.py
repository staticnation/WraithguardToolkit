r"""Trace a texture from a mesh's reference to the pixels the viewer shows.

When a mesh renders untextured there are six places it can have gone wrong, and
the symptom is identical for all of them:

1. the data folders were never handed to the resolver;
2. no archive was opened, so the base game's textures are invisible;
3. the mesh names a texture the resolver cannot find;
4. it is found but the bytes cannot be read;
5. the bytes decode to nothing we can show;
6. everything works and the failure is in the page, not the pipeline.

Guessing between those wastes time. This walks the whole path and says which
step it stopped at, for a real mesh against a real install.

Usage:
    python tools/check_textures.py "E:/OpenMW/Morrowind/Data Files"
    python tools/check_textures.py "E:/.../Data Files" --mesh meshes/x/ex_common_balcony_01.nif
    python tools/check_textures.py "E:/.../Data Files" --texture tx_wood_dark.dds

Give the data folders in load order -- later ones override earlier -- exactly as
the conflict scan does, because that is what decides which copy would load.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from wraithguard.images import ImageError, browser_image, detect, read_image
from wraithguard.images.roles import classify
from wraithguard.nif.bsa import BsaArchive, BsaError
from wraithguard.nif.geometry import world_meshes
from wraithguard.nif.reader import NifParseError, read_nif_bytes
from wraithguard.nif.textures import TextureResolver

#: How many textures to report in full before summarising.
DETAIL = 12


def describe_folders(dirs: list[Path]) -> int:
    """Report what each data folder actually contains.

    Args:
        dirs: The data folders, in load order.

    Returns:
        How many problems were found.
    """
    print("data folders, in load order:")
    problems = 0
    for folder in dirs:
        if not folder.is_dir():
            print(f"  MISSING  {folder}")
            problems += 1
            continue
        children = {child.name.lower(): child for child in folder.iterdir()}
        textures = children.get("textures")
        archives = sorted(p for p in folder.iterdir() if p.suffix.lower() == ".bsa")
        loose = sum(1 for _ in textures.rglob("*")) if textures else 0
        print(f"  {folder}")
        print(f"      loose textures: {loose}")
        print(f"      archives: {', '.join(p.name for p in archives) or 'none'}")
        for path in archives:
            try:
                archive = BsaArchive(path)
            except BsaError as exc:
                print(f"        {path.name}: WILL NOT OPEN -- {exc}")
                problems += 1
                continue
            inside = sum(1 for n in archive.names if n.startswith("textures/"))
            print(f"        {path.name}: {len(archive)} file(s), {inside} under textures/")
            if not inside:
                print("          (an archive with no textures/ prefix is worth a look)")
    return problems


def trace(reference: str, resolver: TextureResolver, *, verbose: bool = True) -> bool:
    """Follow one texture reference all the way to pixels.

    Args:
        reference: The path as a mesh wrote it.
        resolver: The resolver to ask.
        verbose: Whether to print each step.

    Returns:
        Whether the texture came out the far end as something displayable.
    """
    found = resolver.resolve(reference)
    role = classify(reference)
    label = f"{reference} [{role.value}]"
    if not found.found:
        if verbose:
            print(f"  NOT FOUND   {label}")
        return False
    where = (
        f"archive {found.archive.name}:{found.archived_name}"
        if found.from_archive
        else str(found.path)
    )
    raw = resolver.read(found)
    if raw is None:
        if verbose:
            print(f"  UNREADABLE  {label}\n              at {where}")
        return False
    kind = detect(raw)
    try:
        image = read_image(raw)
        shown, mime = browser_image(raw)
    except ImageError as exc:
        if verbose:
            print(f"  NO DECODE   {label}\n              {kind.value}, {len(raw)} bytes: {exc}")
        return False
    if verbose:
        note = " (extension substituted)" if found.substituted else ""
        print(
            f"  OK          {label}{note}\n"
            f"              {kind.value} {image.width}x{image.height} "
            f"-> {len(shown)} bytes of {mime}"
        )
    return True


def main(argv: list[str] | None = None) -> int:
    """Trace textures for a folder set, and optionally for one mesh.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code; non-zero when something did not resolve.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("dirs", nargs="+", type=Path, help="data folders, in load order")
    parser.add_argument("--mesh", help="a .nif to trace every texture of")
    parser.add_argument(
        "--texture",
        action="append",
        default=[],
        help="a texture reference to trace; may be repeated",
    )
    args = parser.parse_args(argv)

    problems = describe_folders(args.dirs)
    print()

    resolver = TextureResolver(args.dirs)

    references: list[str] = list(args.texture)
    if args.mesh:
        path = Path(args.mesh)
        data: bytes | None = None
        if path.is_file():
            data = path.read_bytes()
        else:
            # A mesh named the way a NIF names one, so look through the VFS.
            for folder in args.dirs:
                candidate = folder / args.mesh
                if candidate.is_file():
                    data = candidate.read_bytes()
                    break
        if data is None:
            print(f"could not find mesh {args.mesh}")
            return 2
        try:
            # Geometry is needed: without it the shapes carry no texture.
            parsed = read_nif_bytes(data, geometry=True)
        except NifParseError as exc:
            print(f"could not parse {args.mesh}: {exc}")
            return 2
        meshes = world_meshes(parsed)
        print(f"{args.mesh}: {len(meshes)} shape(s)")
        references.extend(sorted({m.texture for m in meshes if m.texture}))

    if not references:
        # Nothing named, so sample the archives -- which is the case that
        # matters, since the base game's textures live nowhere else.
        for folder in args.dirs:
            for archive_path in sorted(p for p in folder.iterdir() if p.suffix.lower() == ".bsa"):
                try:
                    archive = BsaArchive(archive_path)
                except BsaError:
                    continue
                inside = [n for n in archive.names if n.startswith("textures/")]
                step = max(1, len(inside) // DETAIL)
                references.extend(inside[::step][:DETAIL])
                break
            if references:
                break
        if references:
            print(f"no mesh or texture named, so sampling {len(references)} from the archives")

    print(f"\ntracing {len(references)} texture(s):")
    good = sum(trace(ref, resolver, verbose=index < DETAIL) for index, ref in enumerate(references))
    if len(references) > DETAIL:
        print(f"  ... {len(references) - DETAIL} more not shown")

    print(f"\n{good} of {len(references)} texture(s) reached the viewer.")
    if good != len(references) or problems:
        print("\nThe first failing line above says which step it stopped at.")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
