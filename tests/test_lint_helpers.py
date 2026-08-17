"""_lint_cell, _lint_interior_pathgrid, flatten_dict, and _rec_deleted --
small pure helpers behind lint_plugins and the tes3conv JSON path, all
previously untested.

_lint_cell's fog_bug detection has two independent sources for the same
fact (AMBI's fog float when present, DATA's when not) and one flag that
disables the check entirely ("behaves like exterior") -- getting the
precedence and the disable flag right matters more than either source
alone, since a false [FOG] finding on every interior using the disable flag
would make the lint useless for anyone relying on it.
"""

from __future__ import annotations

import struct

from conftest import sub, zstr

import wraithguard_toolkit as core


def _cell_body(
    name: str, *, flags: int = 1, fog: float = 0.0, ambi_fog: float | None = None
) -> bytes:
    body = sub("NAME", zstr(name)) + sub("DATA", struct.pack("<Iif", flags, 0, fog))
    if ambi_fog is not None:
        body += sub("AMBI", struct.pack("<IIIf", 0, 0, 0, ambi_fog))
    return body


def _pathgrid_body(name: str, x: int = 0, y: int = 0) -> bytes:
    return sub("NAME", zstr(name)) + sub("DATA", struct.pack("<iihBB", x, y, 0, 0, 0))


class TestLintCell:
    def test_an_exterior_cell_returns_none(self) -> None:
        # flags with bit 0 clear means exterior -- nothing for this lint.
        assert core._lint_cell(_cell_body("", flags=0)) is None

    def test_data_too_short_to_read_returns_none(self) -> None:
        body = sub("NAME", zstr("Some Interior")) + sub("DATA", struct.pack("<I", 1))
        assert core._lint_cell(body) is None

    def test_a_zero_fog_in_data_is_flagged_when_there_is_no_ambi(self) -> None:
        facts = core._lint_cell(_cell_body("Some Interior", fog=0.0))
        assert facts is not None
        assert facts.fog_bug is True

    def test_a_nonzero_fog_in_data_is_not_flagged(self) -> None:
        facts = core._lint_cell(_cell_body("Some Interior", fog=0.75))
        assert facts is not None
        assert facts.fog_bug is False

    def test_ambi_fog_takes_precedence_over_datas_fog_when_present(self) -> None:
        # DATA says nonzero, AMBI (the value actually used in-game) says zero.
        facts = core._lint_cell(_cell_body("Some Interior", fog=0.9, ambi_fog=0.0))
        assert facts is not None
        assert facts.fog_bug is True

    def test_ambi_present_and_nonzero_overrules_a_zero_in_data(self) -> None:
        facts = core._lint_cell(_cell_body("Some Interior", fog=0.0, ambi_fog=0.6))
        assert facts is not None
        assert facts.fog_bug is False

    def test_the_behaves_like_exterior_flag_disables_the_fog_check_entirely(self) -> None:
        # bit 128 set, alongside bit 1 (interior) -- fog=0.0 would otherwise flag.
        facts = core._lint_cell(_cell_body("Some Interior", flags=1 | 128, fog=0.0))
        assert facts is not None
        assert facts.fog_bug is False

    def test_a_skip_listed_cell_id_is_blanked_but_the_name_is_kept(self) -> None:
        facts = core._lint_cell(_cell_body("Ashlands Region (0, 0)", fog=1.0))
        assert facts is not None
        assert facts.name == "Ashlands Region (0, 0)"
        assert facts.cell_id == ""

    def test_an_ordinary_cell_id_is_the_lower_cased_name(self) -> None:
        facts = core._lint_cell(_cell_body("Balmora, Guild of Mages", fog=1.0))
        assert facts is not None
        assert facts.cell_id == "balmora, guild of mages"


class TestLintInteriorPathgrid:
    def test_grid_zero_zero_with_a_name_is_an_interior(self) -> None:
        assert core._lint_interior_pathgrid(_pathgrid_body("Balmora, Guild", 0, 0)) == (
            "balmora, guild"
        )

    def test_a_nonzero_grid_is_an_exterior_and_returns_none(self) -> None:
        assert core._lint_interior_pathgrid(_pathgrid_body("", 3, -2)) is None

    def test_grid_zero_zero_with_no_name_returns_none(self) -> None:
        assert core._lint_interior_pathgrid(_pathgrid_body("", 0, 0)) is None

    def test_a_missing_data_subrecord_returns_none(self) -> None:
        body = sub("NAME", zstr("Some Interior"))
        assert core._lint_interior_pathgrid(body) is None


class TestFlattenDict:
    def test_nested_dicts_become_dotted_keys(self) -> None:
        assert core.flatten_dict({"a": {"b": 1}}) == {"a.b": 1}

    def test_a_deeply_nested_dict_flattens_all_the_way_down(self) -> None:
        assert core.flatten_dict({"a": {"b": {"c": 2}}}) == {"a.b.c": 2}

    def test_a_list_value_is_kept_whole_not_flattened(self) -> None:
        assert core.flatten_dict({"a": [1, 2, 3]}) == {"a": [1, 2, 3]}

    def test_a_flat_dict_is_returned_unchanged_in_shape(self) -> None:
        assert core.flatten_dict({"a": 1, "b": 2}) == {"a": 1, "b": 2}

    def test_an_empty_dict_returns_an_empty_dict(self) -> None:
        assert core.flatten_dict({}) == {}

    def test_a_custom_separator_is_used_instead_of_a_dot(self) -> None:
        assert core.flatten_dict({"a": {"b": 1}}, sep="/") == {"a/b": 1}


class TestRecDeleted:
    def test_a_non_dict_input_is_not_deleted(self) -> None:
        assert core._rec_deleted("not a dict") is False  # type: ignore[arg-type]

    def test_flags_as_a_list_containing_deleted_is_deleted(self) -> None:
        assert core._rec_deleted({"flags": ["Deleted", "Blocked"]}) is True

    def test_flags_as_a_list_without_deleted_is_not(self) -> None:
        assert core._rec_deleted({"flags": ["Blocked"]}) is False

    def test_flags_as_a_string_containing_deleted_is_deleted(self) -> None:
        assert core._rec_deleted({"flags": "Deleted"}) is True

    def test_flags_as_a_string_is_case_insensitive(self) -> None:
        assert core._rec_deleted({"flags": "DELETED"}) is True

    def test_flags_as_an_int_with_the_deleted_bit_set_is_deleted(self) -> None:
        assert core._rec_deleted({"flags": 0x20}) is True

    def test_flags_as_an_int_without_the_bit_is_not_deleted(self) -> None:
        assert core._rec_deleted({"flags": 0x01}) is False

    def test_no_flags_key_falls_back_to_a_bare_deleted_key(self) -> None:
        assert core._rec_deleted({"deleted": True}) is True

    def test_neither_flags_nor_deleted_present_is_not_deleted(self) -> None:
        assert core._rec_deleted({}) is False
