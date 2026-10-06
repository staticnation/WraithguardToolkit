"""wraithguard.patch.armor: an armor piece's weight class from the game settings."""

from __future__ import annotations

import pytest

from wraithguard.patch.armor import limits, settings_from, weight_class
from wraithguard.patch.columns import table


@pytest.mark.parametrize(
    ("slot", "weight", "want"),
    [
        ("Helmet", 3.0, "Light"),  # 5 x 0.6, exactly at the limit
        ("Helmet", 3.01, "Medium"),
        ("Helmet", 4.5, "Medium"),  # 5 x 0.9
        ("Helmet", 4.6, "Heavy"),
        ("Cuirass", 18, "Light"),
        ("Cuirass", 27, "Medium"),
        ("Cuirass", 30, "Heavy"),
        ("LeftBracer", 3, "Light"),  # bracers take the gauntlets' base weight
        ("Shield", 13.5, "Medium"),
        ("Nonsense", 1, ""),
    ],
)
def test_vanilla_settings(slot, weight, want):
    assert weight_class(slot, weight) == want


def test_float32_weights_at_a_limit_stay_in_the_class():
    import struct

    f32 = struct.unpack("<f", struct.pack("<f", 4.5))[0]
    assert weight_class("Helmet", f32) == "Medium"


def test_the_load_orders_settings_win_last_first():
    def gmst(v: object, t: str = "Float") -> dict:
        """A GMST record as the reader gives one."""
        return {"type": "GameSetting", "value": {"type": t, "data": v}}

    s = settings_from(
        [
            {"flightmaxmod": gmst(0.5), "ihelmweight": gmst(6, "Integer")},
            {"flightmaxmod": gmst(0.7), "sname": gmst("x", "String")},
        ]
    )
    assert s["fLightMaxMod"] == 0.7
    assert s["iHelmWeight"] == 6
    assert limits("Helmet", s) == pytest.approx((4.2, 5.4))
    assert weight_class("Helmet", 4.2, s) == "Light"


def test_the_object_window_has_a_class_column():
    rec = {"type": "Armor", "id": "x", "data": {"armor_type": "Boots", "weight": 20, "health": 1}}
    t = table("ARMO", [{"x": rec}], {"fLightMaxMod": 0.6, "fMedMaxMod": 0.9})
    labels = [c["label"] for c in t["columns"]]
    assert labels[:2] == ["Type", "Class"]
    assert t["rows"]["x"][1] == "Heavy"
