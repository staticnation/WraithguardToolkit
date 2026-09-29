"""Records holding a NaN or infinite float (wraithguard.esp.plugin).

JSON has no number for those (the crate writes ``null`` and cannot read it back, and
tes3conv shares the limitation). The records now reach Python and go back to the crate
with no JSON in between, so a NaN is just a float: read, edited or not, it is written
back as the same bits.
"""

from __future__ import annotations

import struct
from pathlib import Path

import pytest

from wraithguard.esp.plugin import read_plugin, write_plugin

ALL_TYPES = Path(__file__).resolve().parents[2] / "tes3-main/libs/esp/tests/assets/all_types.esp"
NAN = struct.pack("<I", 0x7FC00001)


@pytest.fixture
def with_nan() -> tuple[bytes, int]:
    if not ALL_TYPES.is_file():
        pytest.skip("the tes3 checkout's all_types.esp is not beside the repo")
    data = bytearray(ALL_TYPES.read_bytes())
    at = data.find(b"FLTV") + 8
    data[at : at + 4] = NAN
    return bytes(data), at


def _setting(records: list, name: str):
    return next(r for r in records if getattr(r, "id", "") == name)


def test_an_unchanged_record_with_a_nan_goes_back_exactly(with_nan) -> None:
    data, at = with_nan
    out = write_plugin(read_plugin(data))
    assert NAN in out
    assert out[at - 64 : at + 64] == data[at - 64 : at + 64]


def test_setting_a_real_number_writes_it(with_nan) -> None:
    data, _ = with_nan
    records = read_plugin(data)
    _setting(records, "fAIFleeFleeMult").value = 2.0
    assert struct.pack("<f", 2.0) in write_plugin(records)


def test_a_changed_record_keeps_its_nan(with_nan) -> None:
    data, _ = with_nan
    records = read_plugin(data)
    _setting(records, "fAIFleeFleeMult").id = "renamed"
    out = write_plugin(records)
    assert NAN in out
    assert b"renamed" in out
