"""The lint's per-record facts (cells, evil GMSTs, interior path grids), plus
flatten_dict and _rec_deleted -- small pure helpers behind lint_plugins and the
tes3conv JSON path.

The per-record reading is the Rust ``PluginLinter`` (native/src/lint.rs); the
helpers below wrap one record in a plugin and read back what it reported, so the
cases are the same ones the Python ``_lint_*`` helpers were pinned by.

_lint_cell's fog_bug detection has two independent sources for the same
fact (AMBI's fog float when present, DATA's when not) and one flag that
disables the check entirely ("behaves like exterior") -- getting the
precedence and the disable flag right matters more than either source
alone, since a false [FOG] finding on every interior using the disable flag
would make the lint useless for anyone relying on it.
"""

from __future__ import annotations

import struct
from typing import NamedTuple

import pytest
from conftest import rec, sub, zstr

pytest.importorskip("wraithguard_native")

import wraithguard_toolkit as core


class _Cell(NamedTuple):
    name: str
    cell_id: str
    fog_bug: bool


def _scan(tag: str, body: bytes) -> dict:
    return core._plugin_linter().scan(rec(tag, body), False)


def _lint_cell(body: bytes) -> _Cell | None:
    events = _scan("CELL", body)["events"]
    return _Cell(*events[0][1:]) if events else None


def _lint_evil_gmst(body: bytes) -> str | None:
    names = _scan("GMST", body)["evil_gmsts"]
    return names[0] if names else None


def _lint_interior_pathgrid(body: bytes) -> str | None:
    grids = _scan("PGRD", body)["pathgrids"]
    return grids[0] if grids else None


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
        assert _lint_cell(_cell_body("", flags=0)) is None

    def test_data_too_short_to_read_returns_none(self) -> None:
        body = sub("NAME", zstr("Some Interior")) + sub("DATA", struct.pack("<I", 1))
        assert _lint_cell(body) is None

    def test_a_zero_fog_in_data_is_flagged_when_there_is_no_ambi(self) -> None:
        facts = _lint_cell(_cell_body("Some Interior", fog=0.0))
        assert facts is not None
        assert facts.fog_bug is True

    def test_a_nonzero_fog_in_data_is_not_flagged(self) -> None:
        facts = _lint_cell(_cell_body("Some Interior", fog=0.75))
        assert facts is not None
        assert facts.fog_bug is False

    def test_ambi_fog_takes_precedence_over_datas_fog_when_present(self) -> None:
        # DATA says nonzero, AMBI (the value actually used in-game) says zero.
        facts = _lint_cell(_cell_body("Some Interior", fog=0.9, ambi_fog=0.0))
        assert facts is not None
        assert facts.fog_bug is True

    def test_ambi_present_and_nonzero_overrules_a_zero_in_data(self) -> None:
        facts = _lint_cell(_cell_body("Some Interior", fog=0.0, ambi_fog=0.6))
        assert facts is not None
        assert facts.fog_bug is False

    def test_the_behaves_like_exterior_flag_disables_the_fog_check_entirely(self) -> None:
        # bit 128 set, alongside bit 1 (interior) -- fog=0.0 would otherwise flag.
        facts = _lint_cell(_cell_body("Some Interior", flags=1 | 128, fog=0.0))
        assert facts is not None
        assert facts.fog_bug is False

    def test_a_skip_listed_cell_id_is_blanked_but_the_name_is_kept(self) -> None:
        facts = _lint_cell(_cell_body("Ashlands Region (0, 0)", fog=1.0))
        assert facts is not None
        assert facts.name == "Ashlands Region (0, 0)"
        assert facts.cell_id == ""

    def test_an_ordinary_cell_id_is_the_lower_cased_name(self) -> None:
        facts = _lint_cell(_cell_body("Balmora, Guild of Mages", fog=1.0))
        assert facts is not None
        assert facts.cell_id == "balmora, guild of mages"

    def test_an_unrecognized_subrecord_is_skipped(self) -> None:
        """A CELL can carry subrecords this lint doesn't care about (e.g. REGN); ignore them."""
        body = (
            sub("NAME", zstr("Some Interior"))
            + sub("REGN", zstr("SomeRegion"))
            + sub("DATA", struct.pack("<Iif", 1, 0, 0.5))
        )
        facts = _lint_cell(body)
        assert facts is not None
        assert facts.fog_bug is False


class TestLintEvilGmst:
    def test_a_matching_name_and_value_is_flagged(self) -> None:
        name, (tag, raw_value) = next(iter(core._EVIL_GMSTS.items()))
        body = sub("NAME", zstr(name)) + sub(tag, raw_value)

        assert _lint_evil_gmst(body) == name

    def test_an_unrecognized_subrecord_is_skipped(self) -> None:
        """A GMST can carry subrecords this lint doesn't key on (e.g. FNAM); ignore them."""
        name, (tag, raw_value) = next(iter(core._EVIL_GMSTS.items()))
        body = sub("NAME", zstr(name)) + sub("FNAM", zstr("Some Label")) + sub(tag, raw_value)

        assert _lint_evil_gmst(body) == name


class TestLintInteriorPathgrid:
    def test_grid_zero_zero_with_a_name_is_an_interior(self) -> None:
        assert _lint_interior_pathgrid(_pathgrid_body("Balmora, Guild", 0, 0)) == ("balmora, guild")

    def test_a_nonzero_grid_is_an_exterior_and_returns_none(self) -> None:
        assert _lint_interior_pathgrid(_pathgrid_body("", 3, -2)) is None

    def test_grid_zero_zero_with_no_name_returns_none(self) -> None:
        assert _lint_interior_pathgrid(_pathgrid_body("", 0, 0)) is None

    def test_a_missing_data_subrecord_returns_none(self) -> None:
        body = sub("NAME", zstr("Some Interior"))
        assert _lint_interior_pathgrid(body) is None

    def test_a_data_subrecord_too_short_to_hold_grid_coords_is_ignored(self) -> None:
        """DATA shorter than the 8-byte grid pair is not usable -- treated as absent."""
        body = sub("NAME", zstr("Some Interior")) + sub("DATA", struct.pack("<i", 0))
        assert _lint_interior_pathgrid(body) is None


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


class TestInteriorCellNames:
    def test_a_named_interior_cell_by_int_flags_is_collected(self) -> None:
        records = [{"type": "Cell", "id": "Balmora, Guild", "data": {"flags": 0x01}}]
        assert core._interior_cell_names(records) == {"balmora, guild"}

    def test_a_named_interior_cell_by_string_flags_is_collected(self) -> None:
        records = [{"type": "Cell", "name": "Balmora, Guild", "data": {"flags": "INTERIOR"}}]
        assert core._interior_cell_names(records) == {"balmora, guild"}

    def test_an_exterior_cell_is_not_collected(self) -> None:
        records = [{"type": "Cell", "id": "Seyda Neen", "data": {"flags": 0}}]
        assert core._interior_cell_names(records) == set()

    def test_an_interior_cell_with_no_id_or_name_is_skipped(self) -> None:
        """The interior flag alone isn't enough; there has to be something to key on."""
        records = [{"type": "Cell", "data": {"flags": 0x01}}]
        assert core._interior_cell_names(records) == set()

    def test_a_non_cell_record_is_skipped(self) -> None:
        records = [{"type": "Static", "id": "torch_01"}]
        assert core._interior_cell_names(records) == set()

    def test_a_non_dict_entry_is_skipped(self) -> None:
        assert core._interior_cell_names(["not a dict", 42]) == set()
