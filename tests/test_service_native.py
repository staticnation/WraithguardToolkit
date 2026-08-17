"""The native-read fallback in the Merged Lands service.

When tes3conv refuses a plugin, the service tries reading its landscape records
directly. If that also fails, both reasons are reported together rather than a
bare empty list -- this pins that failure path.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from wraithguard.land.service import _records_natively

if TYPE_CHECKING:
    from pathlib import Path


def test_a_native_read_failure_reports_both_reasons(tmp_path: Path) -> None:
    """A non-plugin file fails the direct read too; the message names both."""
    bad = tmp_path / "garbage.esm"
    bad.write_bytes(b"not a plugin at all")
    records, reason = _records_natively(bad, "tes3conv refused it")
    assert records == []
    assert "tes3conv refused it" in reason
    assert "reading it directly also failed" in reason
