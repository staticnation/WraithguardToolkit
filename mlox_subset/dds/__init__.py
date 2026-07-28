"""Reading Morrowind's textures, and writing something a viewer can show.

Two halves, both dependency-free for the same reason the NIF reader is:
:mod:`mlox_subset.dds.decode` turns a DDS into RGBA, and
:mod:`mlox_subset.dds.png` turns RGBA into a PNG using only :mod:`zlib`.

The point is the same question a mesh conflict raises -- when two mods ship the
same texture, does the winner actually look different? -- and answering it
needs pixels rather than a hash.
"""

from __future__ import annotations

from mlox_subset.dds.decode import DdsError, Image, read_dds
from mlox_subset.dds.png import encode_png

__all__ = [
    "DdsError",
    "Image",
    "encode_png",
    "read_dds",
]
