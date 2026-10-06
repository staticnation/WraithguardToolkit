"""The Patch Builder's write, in the steps the viewer's Editor shares with it.

``prepare_patch`` (the Tk thread: the queue and the load order), ``run_patch`` (any
thread: reading and writing) and ``finish_patch`` (the queue spent), without a display.
The service is stood in for: what is checked is what reaches it, and what comes back.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, Any

import pytest

if TYPE_CHECKING:
    from pathlib import Path

patchwin = pytest.importorskip("wraithguard.gui.patchwin")

from wraithguard.patch.queue import PatchQueue  # noqa: E402
from wraithguard.patch.records import Selection  # noqa: E402
from wraithguard.patch.service import PatchResult, PatchServiceError  # noqa: E402


class _Host(patchwin.PatchBuilderMixin):
    def __init__(self, tmp_path: Path) -> None:
        self._patch_queue = PatchQueue()
        self.refreshed = 0
        self.reader = True
        plugins = {"Morrowind.esm": tmp_path / "Morrowind.esm", "Mod.esp": tmp_path / "Mod.esp"}
        for p in plugins.values():
            p.write_bytes(b"x" * 10)
        self._conf_paths = {k: str(v) for k, v in plugins.items()}
        self._conf_session = SimpleNamespace(
            records=lambda path: (
                [{"type": "Npc", "id": "old"}] if str(path).endswith("Old.esp") else []
            ),
            exe="",
            engine_name="native",
        )
        self.order_panel = SimpleNamespace(get_enabled=lambda: ["Morrowind.esm", "Mod.esp"])
        self._shown_conflicts: list[dict] = []

    def _ensure_conflict_session(self, conv: str | None = None) -> bool:
        return self.reader

    def _apply_exclusions(self, order: list[str]) -> list[str]:
        return list(order)

    def refresh_patch_views(self) -> None:
        self.refreshed += 1


def test_the_steps(tmp_path, monkeypatch):
    host = _Host(tmp_path)
    target = tmp_path / "Patch.esp"
    with pytest.raises(PatchServiceError, match="Nothing is queued"):
        host.prepare_patch(target, append=False)
    host.patch_queue().add_whole(Selection("Mod.esp", "Npc", "fargoth"))
    assert host.patch_summary()["records"] == 1 and host.patch_summary()["whole"] == 1
    host.reader = False
    with pytest.raises(PatchServiceError, match="built-in reader"):
        host.prepare_patch(target, append=False)
    host.reader = True
    job = host.prepare_patch(target, append=False)
    assert job["names"] == ["Mod.esp"] and job["sizes"] == {"Morrowind.esm": 10, "Mod.esp": 10}

    seen: dict[str, Any] = {}

    def fake_build(selections, records, order, sizes, exe, out, **kw):
        seen.update(selections=selections, records=records, order=order, carried=kw["carried"])
        kw["report"]("written")
        return PatchResult(
            output=out, records=len(selections), masters=["Morrowind.esm", "Mod.esp"]
        )

    monkeypatch.setattr(patchwin, "build_record_patch", fake_build)
    lines: list[str] = []
    result = host.run_patch(job, report=lines.append)
    assert lines == ["written"] and seen["records"] == {"Mod.esp": []}
    assert seen["order"] == ["Morrowind.esm", "Mod.esp"] and seen["carried"] == []
    host.finish_patch(result)
    assert not len(host.patch_queue()) and host.refreshed == 1
    assert host.patch_last_path() == target
    assert "1 record(s) written" in host.patch_written_note(result)

    # Appending reads the file there back in; one that does not read is refused.
    host.patch_queue().add_whole(Selection("Mod.esp", "Npc", "fargoth"))
    old = tmp_path / "Old.esp"
    old.write_bytes(b"TES3")
    assert host.prepare_patch(old, append=True)["carried"] == [{"type": "Npc", "id": "old"}]
    target.write_bytes(b"TES3")
    with pytest.raises(PatchServiceError, match="could not be decoded"):
        host.prepare_patch(target, append=True)
    assert host.prepare_patch(target, append=False)["carried"] == []
