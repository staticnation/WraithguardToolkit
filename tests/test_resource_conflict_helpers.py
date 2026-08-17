"""describe_mesh_finding, _mesh_note, _providers_are_identical, and
_html_escape -- the resource-conflict formatting/comparison helpers that
sit beneath detect_resource_conflicts and analyse_mesh_conflicts.

Neither of those two larger functions has coverage yet either (a real gap,
noted as future work in test_lint_and_resource_stages.py's own docstring),
but these four are the pure, self-contained pieces worth pinning on their
own first: describe_mesh_finding's job is specifically to say nothing when
a finding can't be trusted, which is easy to get backwards under
refactoring since "no difference" and "don't know" look identical unless
you check .reliable first.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import wraithguard_toolkit as core
from wraithguard.nif.analysis import MeshFinding
from wraithguard.nif.report import Difference

if TYPE_CHECKING:
    from pathlib import Path


def _difference(
    *,
    triangle_ratio: float | None = 1.0,
    lost_collision: bool = False,
    lost_animation: bool = False,
    added_textures: list[str] | None = None,
    dropped_textures: list[str] | None = None,
    unreliable: bool = False,
) -> Difference:
    return Difference(
        triangle_ratio=triangle_ratio,
        lost_collision=lost_collision,
        lost_animation=lost_animation,
        added_textures=added_textures or [],
        dropped_textures=dropped_textures or [],
        unreliable=unreliable,
    )


class TestDescribeMeshFinding:
    def test_an_unreadable_finding_reports_the_reason(self) -> None:
        finding = MeshFinding("mesh/x.nif", unreadable="not a Morrowind NIF version")
        assert (
            core.describe_mesh_finding(finding)
            == "could not read the mesh: not a Morrowind NIF version"
        )

    def test_a_partial_read_says_nothing_rather_than_guess(self) -> None:
        # difference exists, but loser_partial=True makes .reliable False --
        # an absence here can't be proven, so this must report nothing at all.
        finding = MeshFinding(
            "mesh/x.nif", difference=_difference(lost_collision=True), loser_partial=True
        )
        assert core.describe_mesh_finding(finding) == ""

    def test_no_difference_at_all_says_nothing(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=None)
        assert core.describe_mesh_finding(finding) == ""

    def test_lost_collision_is_named(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=_difference(lost_collision=True))
        assert core.describe_mesh_finding(finding) == "loses collision"

    def test_lost_animation_is_named(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=_difference(lost_animation=True))
        assert core.describe_mesh_finding(finding) == "loses animation"

    def test_a_single_added_texture_uses_the_singular_phrasing(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=_difference(added_textures=["tex/new.dds"]))
        result = core.describe_mesh_finding(finding)
        assert "1 texture the other does not ship" in result
        assert "textures" not in result

    def test_several_added_textures_use_the_plural_phrasing(self) -> None:
        finding = MeshFinding(
            "mesh/x.nif",
            difference=_difference(added_textures=["tex/a.dds", "tex/b.dds"]),
        )
        assert "2 textures the other does not ship" in core.describe_mesh_finding(finding)

    def test_a_much_simpler_winning_mesh_reports_the_triangle_percentage(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=_difference(triangle_ratio=0.1))
        assert "10% of the triangles" in core.describe_mesh_finding(finding)

    def test_a_ratio_at_or_above_the_threshold_is_not_mentioned(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=_difference(triangle_ratio=0.9))
        assert core.describe_mesh_finding(finding) == ""

    def test_a_none_ratio_is_not_mentioned(self) -> None:
        finding = MeshFinding("mesh/x.nif", difference=_difference(triangle_ratio=None))
        assert core.describe_mesh_finding(finding) == ""

    def test_several_findings_are_joined_with_commas(self) -> None:
        finding = MeshFinding(
            "mesh/x.nif",
            difference=_difference(lost_collision=True, lost_animation=True),
        )
        assert core.describe_mesh_finding(finding) == "loses collision, loses animation"

    def test_winner_partial_also_makes_it_unreliable(self) -> None:
        finding = MeshFinding(
            "mesh/x.nif", difference=_difference(lost_collision=True), winner_partial=True
        )
        assert core.describe_mesh_finding(finding) == ""


class TestMeshNote:
    def test_a_real_mesh_finding_is_delegated_to_describe_mesh_finding(self) -> None:
        conflict = {"mesh": MeshFinding("mesh/x.nif", difference=_difference(lost_collision=True))}
        assert core._mesh_note(conflict) == "loses collision"

    def test_no_mesh_key_returns_an_empty_string(self) -> None:
        assert core._mesh_note({}) == ""

    def test_a_non_meshfinding_value_returns_an_empty_string(self) -> None:
        # e.g. a resource conflict entry for a non-mesh asset never got one.
        assert core._mesh_note({"mesh": "not actually a MeshFinding"}) == ""


class TestProvidersAreIdentical:
    def test_two_byte_identical_files_are_identical(self, tmp_path: Path) -> None:
        (tmp_path / "a").mkdir()
        (tmp_path / "b").mkdir()
        (tmp_path / "a" / "tex.dds").write_bytes(b"same content")
        (tmp_path / "b" / "tex.dds").write_bytes(b"same content")

        assert core._providers_are_identical("tex.dds", [str(tmp_path / "a"), str(tmp_path / "b")])

    def test_different_sizes_are_never_identical(self, tmp_path: Path) -> None:
        (tmp_path / "a").mkdir()
        (tmp_path / "b").mkdir()
        (tmp_path / "a" / "tex.dds").write_bytes(b"short")
        (tmp_path / "b" / "tex.dds").write_bytes(b"a good deal longer")

        assert not core._providers_are_identical(
            "tex.dds", [str(tmp_path / "a"), str(tmp_path / "b")]
        )

    def test_same_size_but_different_bytes_is_not_identical(self, tmp_path: Path) -> None:
        (tmp_path / "a").mkdir()
        (tmp_path / "b").mkdir()
        (tmp_path / "a" / "tex.dds").write_bytes(b"AAAAAAAAAA")
        (tmp_path / "b" / "tex.dds").write_bytes(b"BBBBBBBBBB")

        assert not core._providers_are_identical(
            "tex.dds", [str(tmp_path / "a"), str(tmp_path / "b")]
        )

    def test_a_provider_missing_the_file_entirely_is_not_identical(self, tmp_path: Path) -> None:
        (tmp_path / "a").mkdir()
        (tmp_path / "b").mkdir()
        (tmp_path / "a" / "tex.dds").write_bytes(b"content")
        # b never gets the file at all.

        assert not core._providers_are_identical(
            "tex.dds", [str(tmp_path / "a"), str(tmp_path / "b")]
        )

    def test_three_providers_all_identical_is_identical(self, tmp_path: Path) -> None:
        for name in ("a", "b", "c"):
            (tmp_path / name).mkdir()
            (tmp_path / name / "tex.dds").write_bytes(b"same everywhere")

        assert core._providers_are_identical(
            "tex.dds", [str(tmp_path / n) for n in ("a", "b", "c")]
        )

    def test_a_single_provider_is_trivially_identical(self, tmp_path: Path) -> None:
        (tmp_path / "a").mkdir()
        (tmp_path / "a" / "tex.dds").write_bytes(b"whatever")

        assert core._providers_are_identical("tex.dds", [str(tmp_path / "a")])


class TestHtmlEscape:
    def test_the_four_special_characters_are_all_escaped(self) -> None:
        assert core._html_escape('<a href="x">&</a>') == (
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        )

    def test_a_single_quote_is_left_unescaped(self) -> None:
        # Only &, <, >, and " are replaced -- not the fifth HTML entity.
        assert core._html_escape("O'Brien") == "O'Brien"

    def test_plain_text_with_nothing_special_is_unchanged(self) -> None:
        assert core._html_escape("Balmora, Guild of Mages") == "Balmora, Guild of Mages"

    def test_a_non_string_value_is_stringified_first(self) -> None:
        assert core._html_escape(42) == "42"

    def test_an_empty_string_stays_empty(self) -> None:
        assert core._html_escape("") == ""
