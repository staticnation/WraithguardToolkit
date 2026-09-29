"""The plugin reader's error type.

The byte layer that lived here (a little-endian Reader and Writer) is gone: reading
and writing plugins is greatness7's ``tes3::esp`` now (see
:mod:`wraithguard.esp.plugin`).
"""

from __future__ import annotations


class EspError(Exception):
    """A plugin's bytes did not hold what the format requires."""
