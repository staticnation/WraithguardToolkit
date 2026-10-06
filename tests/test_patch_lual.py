"""OpenMW's Lua script lists (LUAL) carried into a patch: their object references follow
the patch's master list, as a cell's do."""

from __future__ import annotations

import pytest

from wraithguard.patch.records import PatchError, needs_remapping, remap_references

PLAIN = {
    "type": "ScriptConfigList",
    "flags": "",
    "scripts": [
        {
            "path": "scripts/a.lua",
            "init_data": "",
            "flags": "",
            "types": ["NPC_"],
            "records": [],
            "instances": [],
        }
    ],
}


def test_a_list_naming_no_objects_is_carried_as_it_is():
    """No LUAI entries and no Lua data: nothing to renumber."""
    assert not needs_remapping(PLAIN)
    assert remap_references(PLAIN, {0: 3}) == PLAIN


def _with_instance(mast_idx: int) -> dict:
    rec = {**PLAIN, "scripts": [dict(PLAIN["scripts"][0])]}
    rec["scripts"][0]["instances"] = [
        {"attach": True, "mast_idx": mast_idx, "ref_idx": 7, "data": ""}
    ]
    return rec


def test_per_reference_entries_need_remapping():
    """A LUAI entry names a placed object by content file."""
    assert needs_remapping(_with_instance(1))


def test_remapped_by_the_backend_or_refused():
    """With the Rust backend, renumbered; without it, refused rather than written wrong."""
    try:
        import wraithguard_native
    except ImportError:
        wraithguard_native = None
    rec = _with_instance(1)
    if wraithguard_native is None or not hasattr(wraithguard_native, "lual_remap"):
        with pytest.raises(PatchError, match="Rust backend"):
            remap_references(rec, {0: 2, 1: 1})
        return
    out = remap_references(rec, {0: 2, 1: 3})
    assert out["scripts"][0]["instances"][0]["mast_idx"] == 3
    with pytest.raises(PatchError):
        remap_references(_with_instance(5), {0: 2, 1: 3})
