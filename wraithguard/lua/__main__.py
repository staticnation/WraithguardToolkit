"""``python -m wraithguard.lua openmw.cfg``: the Lua report for a load order.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from wraithguard.lua.openmw_api import check_cfg
from wraithguard.lua.report import all_findings, render


def main(argv: list[str] | None = None) -> int:
    """Scan an openmw.cfg's Lua scripts and print the report.

    Args:
        argv: The arguments (default: the command line's).

    Returns:
        1 when there are errors, else 0.
    """
    ap = argparse.ArgumentParser(prog="python -m wraithguard.lua", description=__doc__)
    ap.add_argument("cfg", type=Path, help="the openmw.cfg to read")
    ap.add_argument("--no-info", action="store_true", help="leave out the notes")
    ap.add_argument(
        "--openmw",
        type=Path,
        help="the OpenMW install (or its resources folder) whose documented Lua API to check "
        "against; found on its own when not given (also WG_OPENMW_RESOURCES)",
    )
    ap.add_argument(
        "--teal-declarations",
        type=Path,
        help="a folder of Teal declarations (OpenMW's teal_declarations) to check against instead",
    )
    ap.add_argument(
        "--write-teal-declarations",
        type=Path,
        metavar="DIR",
        help="keep the declarations written from the install in DIR, for a Teal project's include_dir",
    )
    ap.add_argument("--no-teal", action="store_true", help="skip the Teal checks")
    args = ap.parse_args(argv)
    result = check_cfg(
        args.cfg,
        resources=args.openmw,
        declarations=args.teal_declarations,
        teal=not args.no_teal,
        write_declarations=args.write_teal_declarations,
    )
    sys.stdout.write(render(result, info=not args.no_info))
    return 1 if any(f.severity == "error" for _, f in all_findings(result)) else 0


if __name__ == "__main__":
    raise SystemExit(main())
