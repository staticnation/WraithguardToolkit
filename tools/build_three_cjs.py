r"""Rebuild the vendored three.js bundle at ``wraithguard/viz/assets/three.cjs``.

Every viewer page runs three.js as a *classic script* from ``file://`` or over
loopback, behind a three-line ``exports`` shim (see
:mod:`wraithguard.viz.library`). Through r185 that was trivial: three.js shipped
``build/three.cjs``, a single self-contained CommonJS file, and we vendored it
unmodified.

**r186 removed that build.** ``build/three.cjs`` is now a deprecation stub that
``require()``\\s the ESM module -- useless as a classic script -- and the real
code ships ESM-only (``three.module.js`` + ``three.core.js``). ES module scripts
do not load from ``file://`` (the origin is ``null`` and the CORS check fails),
so we can no longer vendor an upstream file as-is. Instead we *bundle* the
upstream ESM into one self-contained CommonJS module with esbuild -- no minifier,
no source transform, just module concatenation into a single ``module.exports``
-- which the existing shim then loads exactly as it loaded the old file.

So the vendored file is no longer "upstream, unmodified"; it is "built by this
script from upstream's unmodified ESM sources." This tool is that build, kept in
the tree so the provenance is a command anyone can rerun rather than a binary to
trust. It needs Node (for ``npx esbuild``) and network access to the npm
registry; both are a developer-machine concern, never the app's.

Usage:
    python tools/build_three_cjs.py            # build the pinned version
    python tools/build_three_cjs.py --version 0.186.0
    python tools/build_three_cjs.py --check     # verify the vendored file only
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

#: The npm version to build. three.js revision N is npm ``0.N.0``; r186 is
#: ``0.186.0``. Bump this (and rerun) to move to a new revision.
THREE_NPM_VERSION = "0.186.0"

#: The three.js revision that version carries, asserted against the built and
#: vendored file so a wrong or half-applied update cannot pass silently.
THREE_REVISION = "186"

#: The esbuild version pinned for the build, so the bundle is reproducible.
ESBUILD_VERSION = "0.23.1"

#: The vendored bundle, relative to the repository root.
ASSET_PATH = Path("wraithguard/viz/assets/three.cjs")


def _repo_root() -> Path:
    """The repository root (the parent of this ``tools`` directory)."""
    return Path(__file__).resolve().parent.parent


def _revision_of(source: str) -> str | None:
    """The ``REVISION`` string a three.js source declares, or ``None``."""
    match = re.search(r"""REVISION\s*=\s*["']([0-9]+)["']""", source)
    return match.group(1) if match else None


def check_vendored() -> int:
    """Verify the vendored bundle exists and carries the expected revision.

    Returns:
        ``0`` when the file is present and its revision matches, ``1`` otherwise.
    """
    path = _repo_root() / ASSET_PATH
    if not path.is_file():
        print(f"MISSING: {ASSET_PATH} does not exist", file=sys.stderr)
        return 1
    revision = _revision_of(path.read_text(encoding="utf-8", errors="replace"))
    if revision != THREE_REVISION:
        print(
            f"REVISION MISMATCH: {ASSET_PATH} is r{revision}, expected r{THREE_REVISION}",
            file=sys.stderr,
        )
        return 1
    print(f"OK: {ASSET_PATH} is three.js r{revision} ({path.stat().st_size} bytes)")
    return 0


def build(version: str) -> int:
    """Download three.js ``version``, bundle its ESM to CJS, and vendor it.

    Args:
        version: The npm version to build (e.g. ``0.186.0``).

    Returns:
        A process exit code: ``0`` on success.
    """
    tarball_url = f"https://registry.npmjs.org/three/-/three-{version}.tgz"
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        archive = work / "three.tgz"
        print(f"downloading {tarball_url}")
        urllib.request.urlretrieve(tarball_url, archive)
        with tarfile.open(archive) as tar:
            tar.extractall(work, filter="data")
        module = work / "package" / "build" / "three.module.js"
        if not module.is_file():
            print(f"ERROR: {module} not found in the package", file=sys.stderr)
            return 1
        # Re-export the whole ESM module through one entry point, then let
        # esbuild concatenate the graph into a single CommonJS file. No minify,
        # no transform: the point is packaging, not rewriting.
        entry = work / "entry.js"
        entry.write_text(f"export * from {str(module)!r};\n", encoding="utf-8")
        out = work / "three.cjs"
        cmd = [
            "npx",
            "--yes",
            f"esbuild@{ESBUILD_VERSION}",
            str(entry),
            "--bundle",
            "--format=cjs",
            "--legal-comments=none",
            f"--outfile={out}",
        ]
        print(" ".join(cmd))
        result = subprocess.run(cmd, cwd=work, check=False)  # noqa: S603 - fixed argv
        if result.returncode != 0:
            print("ERROR: esbuild failed", file=sys.stderr)
            return result.returncode
        built = out.read_text(encoding="utf-8")
        revision = _revision_of(built)
        if revision != THREE_REVISION:
            print(
                f"ERROR: built bundle is r{revision}, expected r{THREE_REVISION}",
                file=sys.stderr,
            )
            return 1
        target = _repo_root() / ASSET_PATH
        target.write_text(built, encoding="utf-8")
        print(f"wrote {ASSET_PATH}: three.js r{revision} ({len(built)} bytes)")
    return 0


def main(argv: list[str] | None = None) -> int:
    """Command-line entry point.

    Args:
        argv: Arguments, or ``None`` to read ``sys.argv``.

    Returns:
        A process exit code.
    """
    parser = argparse.ArgumentParser(description="Rebuild the vendored three.js CJS bundle.")
    parser.add_argument("--version", default=THREE_NPM_VERSION, help="npm version to build")
    parser.add_argument(
        "--check", action="store_true", help="only verify the vendored file's revision"
    )
    args = parser.parse_args(argv)
    if args.check:
        return check_vendored()
    return build(args.version)


if __name__ == "__main__":
    sys.exit(main())
