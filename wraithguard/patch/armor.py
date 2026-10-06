"""An armor piece's weight class (Light, Medium, Heavy), as the game works it out.

The class is not stored in the record. The game takes the slot's base weight (a game
setting per slot: ``iHelmWeight``, ``iCuirassWeight`` ...) and two multipliers,
``fLightMaxMod`` and ``fMedMaxMod``: a piece no heavier than base x light is Light, no
heavier than base x medium is Medium, anything heavier Heavy. Bracers share the
gauntlets' base weight. The settings are the load order's (the last plugin with a GMST
wins), so a mod that changes them moves pieces between classes.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

#: The game setting holding each slot's base weight.
SLOT_SETTING: Final[dict[str, str]] = {
    "Helmet": "iHelmWeight",
    "Cuirass": "iCuirassWeight",
    "LeftPauldron": "iPauldronWeight",
    "RightPauldron": "iPauldronWeight",
    "Greaves": "iGreavesWeight",
    "Boots": "iBootsWeight",
    "LeftGauntlet": "iGauntletWeight",
    "RightGauntlet": "iGauntletWeight",
    "LeftBracer": "iGauntletWeight",
    "RightBracer": "iGauntletWeight",
    "Shield": "iShieldWeight",
}

#: Morrowind.esm's values, used where the load order has no GMST of that name.
DEFAULTS: Final[dict[str, float]] = {
    "iHelmWeight": 5,
    "iPauldronWeight": 10,
    "iCuirassWeight": 30,
    "iGauntletWeight": 5,
    "iGreavesWeight": 15,
    "iBootsWeight": 20,
    "iShieldWeight": 15,
    "fLightMaxMod": 0.6,
    "fMedMaxMod": 0.9,
}

#: Weights are stored as 32-bit floats: a piece exactly at a limit (3.0 against 5 x 0.6)
#: must not fall over it by the rounding.
_SLACK: Final = 5e-4


def setting_value(record: Mapping[str, Any]) -> float | None:
    """A GMST record's number (``value`` is ``{"type", "data"}``), or None."""
    value = record.get("value")
    if isinstance(value, dict):
        value = value.get("data")
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    return float(value)


def settings_from(versions: Iterable[Mapping[str, Mapping[str, Any]]]) -> dict[str, float]:
    """The settings the weight class needs, from GMST records (load order; last wins).

    Args:
        versions: Per plugin in load order, its GMST records by lower-case id.

    Returns:
        Every name in :data:`DEFAULTS`, the load order's value where it has one.
    """
    want = {k.lower(): k for k in DEFAULTS}
    out = dict(DEFAULTS)
    for records in versions:
        for key, rec in records.items():
            name = want.get(str(key).lower())
            if name is None:
                continue
            v = setting_value(rec)
            if v is not None:
                out[name] = v
    return out


def limits(
    armor_type: str, settings: Mapping[str, float] | None = None
) -> tuple[float, float] | None:
    """The heaviest a piece of this slot can be and still be Light, and Medium."""
    s = {**DEFAULTS, **(settings or {})}
    name = SLOT_SETTING.get(str(armor_type))
    if name is None:
        return None
    base = float(s[name])
    return base * float(s["fLightMaxMod"]), base * float(s["fMedMaxMod"])


def weight_class(
    armor_type: str, weight: float, settings: Mapping[str, float] | None = None
) -> str:
    """``"Light"``, ``"Medium"`` or ``"Heavy"``; empty for a slot it cannot tell."""
    lim = limits(armor_type, settings)
    if lim is None:
        return ""
    w = float(weight)
    if w <= lim[0] + _SLACK:
        return "Light"
    if w <= lim[1] + _SLACK:
        return "Medium"
    return "Heavy"


def browser_table(settings: Mapping[str, float]) -> dict[str, Any]:
    """What the page needs to work the class out itself as a weight is typed.

    Args:
        settings: The load order's settings (:func:`settings_from`).

    Returns:
        ``{slots: {armor type: base weight}, light, medium}``.
    """
    s = {**DEFAULTS, **settings}
    return {
        "slots": {t: float(s[n]) for t, n in SLOT_SETTING.items()},
        "light": float(s["fLightMaxMod"]),
        "medium": float(s["fMedMaxMod"]),
    }


__all__ = ["DEFAULTS", "SLOT_SETTING", "browser_table", "limits", "settings_from", "weight_class"]
