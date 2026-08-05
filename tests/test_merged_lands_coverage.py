"""The port's coverage claim has to be checkable, not asserted.

``MERGED_LANDS_FUNCTIONS.md`` says every function in Merged Lands is
accounted for. The first version of that document said so while grouping
related functions onto one table row and reporting the *row* count as the
function count -- ``land/`` was labelled 37 functions when it has 64, and forty
functions were covered only by a heading that never named them.

So the claim is checked here rather than believed. When a copy of the Rust
source is present the whole map is verified against it; when it is not, the
document and the coverage map are still checked against each other, because
those two can rot without the source being anywhere nearby.
"""

from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Final

import pytest

if TYPE_CHECKING:
    from types import ModuleType

#: The repository root.
ROOT: Final = Path(__file__).resolve().parent.parent

#: The generator, which owns the coverage map.
GENERATOR: Final = ROOT / "tools" / "gen_merged_lands_table.py"

#: The document it produces.
DOC: Final = ROOT / "MERGED_LANDS_FUNCTIONS.md"

#: Where a copy of the Rust source may be, relative to the repository root.
#: Absent on a clean checkout, which is why those tests skip rather than fail.
SOURCES: Final[tuple[Path, ...]] = (
    ROOT.parent / "merged_lands-main" / "src",
    ROOT / "merged_lands-main" / "src",
)

#: A generated table row: any first cell, then the source file it came from.
#: Matching on the file column rather than on pipe count keeps a legitimately
#: escaped pipe inside a cell from being read as a row boundary.
_ROW: Final = re.compile(r"^\| .+? \| `[A-Za-z0-9_/.]+\.rs` \| ")

#: Pipes that actually separate cells -- that is, not preceded by a backslash.
_UNESCAPED_PIPE: Final = re.compile(r"(?<!\\)\|")

#: How many functions Merged Lands has. Pinned so that a scan quietly finding
#: fewer -- a broken regex, a half-copied tree -- fails instead of passing.
EXPECTED_FUNCTIONS: Final = 191


def _generator() -> ModuleType:
    """Import the generator as a module.

    Returns:
        The imported module.

    Raises:
        AssertionError: If it cannot be imported.
    """
    spec = importlib.util.spec_from_file_location("gen_merged_lands_table", GENERATOR)
    assert spec is not None and spec.loader is not None, f"cannot load {GENERATOR}"
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _source() -> Path | None:
    """Find a copy of the Rust source, if one is present.

    Returns:
        The ``src`` directory, or ``None``.
    """
    return next((path for path in SOURCES if path.is_dir()), None)


class TestTheCoverageMapIsComplete:
    """Every function in the source has an entry, and every entry a function."""

    def test_the_generator_exists(self) -> None:
        """The document is generated; the generator is part of the claim."""
        assert GENERATOR.is_file()

    def test_no_function_is_uncovered(self) -> None:
        """A function with no entry is the failure this whole file exists for."""
        source = _source()
        if source is None:
            pytest.skip("no copy of merged_lands-main/src alongside the repository")
        module = _generator()
        problems = module.check(module.scan(source))
        assert not problems, "\n".join(problems)

    def test_the_function_count_is_what_we_claim(self) -> None:
        """191, pinned -- a scan that finds fewer has broken, not improved."""
        source = _source()
        if source is None:
            pytest.skip("no copy of merged_lands-main/src alongside the repository")
        module = _generator()
        assert len(module.scan(source)) == EXPECTED_FUNCTIONS

    def test_the_map_itself_has_the_right_size(self) -> None:
        """Checkable without the source: the map must have one entry per fn."""
        module = _generator()
        assert len(module.COVERAGE) == EXPECTED_FUNCTIONS

    def test_every_entry_has_a_known_status(self) -> None:
        """A typo in a status would silently weaken the claim it makes."""
        module = _generator()
        unknown = {
            key: status
            for key, (status, _where) in module.COVERAGE.items()
            if status not in module.STATUS
        }
        assert not unknown, unknown

    def test_every_entry_says_where(self) -> None:
        """ "Ported" with no destination is not an account of anything."""
        module = _generator()
        empty = [key for key, (_status, where) in module.COVERAGE.items() if not where.strip()]
        assert not empty, empty


class TestTheDocumentMatchesTheMap:
    """The published table is the map, not a stale copy of it."""

    def test_the_document_exists(self) -> None:
        """It is the thing the map is for."""
        assert DOC.is_file()

    def test_every_section_count_is_a_function_count(self) -> None:
        """The original defect: headings that counted rows, not functions."""
        module = _generator()
        text = DOC.read_text(encoding="utf-8")
        totals: dict[str, int] = {}
        for key in module.COVERAGE:
            group = key.split("/")[0] if "/" in key.split("::")[0] else "main.rs"
            totals[group] = totals.get(group, 0) + 1
        titles = {
            "land": "`land/`",
            "merge": "`merge/`",
            "repair": "`repair/`",
            "io": "`io/`",
            "main.rs": "`main.rs`",
        }
        for group, count in totals.items():
            heading = f"## {titles[group]} — {count} functions"
            assert heading in text, f"missing or wrong heading: {heading}"

    def test_the_table_has_a_row_for_every_function(self) -> None:
        """One row each, so the table can be read as the account it claims to be.

        Rows are matched on their *source file* column rather than by counting
        pipes: a cell may legitimately contain an escaped ``\\|``, as the entry
        for ``classify_conflict`` does when it writes out the weighting formula.
        """
        text = DOC.read_text(encoding="utf-8")
        rows = [line for line in text.splitlines() if _ROW.match(line)]
        assert len(rows) == EXPECTED_FUNCTIONS, f"{len(rows)} rows, expected {EXPECTED_FUNCTIONS}"

    def test_no_row_has_an_unescaped_pipe(self) -> None:
        """An unescaped pipe ends its cell and silently shears the row apart."""
        broken = [
            line
            for line in DOC.read_text(encoding="utf-8").splitlines()
            if _ROW.match(line) and _UNESCAPED_PIPE.findall(line) != ["|"] * 5
        ]
        assert not broken, broken

    def test_the_total_is_stated(self) -> None:
        """A reader should not have to add up the sections to check."""
        assert f"**all {EXPECTED_FUNCTIONS} of them**" in DOC.read_text(encoding="utf-8")


class TestTheReverseDirection:
    """Every module of ours is placed relative to Merged Lands.

    The coverage table proves *their* 191 functions all landed somewhere here.
    It says nothing about the other direction -- how much of this merge is code
    no reference implementation stands behind. That is now the larger risk: of
    158 functions in ``wraithguard/land``, only 83 trace to a Rust counterpart.

    Most of the remainder is Python plumbing, but some of it moves vertices the
    original never moves. Those modules are marked ``ours`` so that "faithful
    port" is never read as covering them.
    """

    def test_every_module_is_classified(self) -> None:
        """A new module must declare where it stands before it can ship."""
        module = _generator()
        present = {path.name for path in (ROOT / "wraithguard" / "land").glob("*.py")}
        assert present == set(module.MODULES), (
            f"unclassified: {sorted(present - set(module.MODULES))}; "
            f"stale: {sorted(set(module.MODULES) - present)}"
        )

    def test_every_classification_is_known(self) -> None:
        """Only three answers are meaningful here."""
        module = _generator()
        allowed = {"port", "ours", "ours-aux"}
        wrong = {name: role for name, (role, _why) in module.MODULES.items() if role not in allowed}
        assert not wrong, wrong

    def test_the_terrain_changing_additions_are_named(self) -> None:
        """These three are where our output can differ from Merged Lands'.

        Pinned by name because the honest claim about this port depends on the
        list being short and known. If a fourth appears, it is a deliberate
        decision that belongs in the documentation, not a quiet addition.
        """
        module = _generator()
        ours = {name for name, (role, _why) in module.MODULES.items() if role == "ours"}
        assert ours == {"seams.py", "slope.py", "curvature.py"}

    def test_every_classification_says_why(self) -> None:
        """A label with no reason is not an account of anything."""
        module = _generator()
        empty = [name for name, (_role, why) in module.MODULES.items() if not why.strip()]
        assert not empty, empty
