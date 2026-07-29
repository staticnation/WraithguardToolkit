"""Reading Morrowind's textures, and writing something a viewer can show.

Every format this game and its mods use, decoded without a third-party
dependency, for the same reason the NIF reader is written rather than imported:
the alternatives are large, compiled, or licensed incompatibly. ``pydds`` is
the closest technical fit for the hardest format here and is GPLv3, which would
relicense the whole project.

* :mod:`~mlox_subset.images.dds` -- BC1 to BC5 and BC7, plus uncompressed
  surfaces, in both the original and the Direct3D 10 header form.
* :mod:`~mlox_subset.images.bc7` -- the eight-mode BC7 block decoder, which is
  large enough and strange enough to live on its own.
* :mod:`~mlox_subset.images.targa` -- Targa, plain and run-length encoded.
* :mod:`~mlox_subset.images.bitmap` -- Windows bitmaps, palette and all.
* :mod:`~mlox_subset.images.png` -- RGBA out to a PNG, using only :mod:`zlib`.
* :mod:`~mlox_subset.images.reader` -- picks the decoder by inspecting the
  bytes, because in this game the file extension is genuinely unreliable.
* :mod:`~mlox_subset.images.roles` -- what a texture is *for*, so that a normal
  map is not compared against a diffuse map or shown as though it were colour.
* :mod:`~mlox_subset.images.compare` -- whether two versions of a texture
  actually differ, and a difference image showing where.

The point of all of it is the same question a mesh conflict raises: when two
mods ship the same texture, does the winner actually look different? Answering
that needs pixels rather than a hash.
"""

from __future__ import annotations

from mlox_subset.images.bitmap import BitmapError, read_bmp
from mlox_subset.images.compare import (
    Comparison,
    Verdict,
    compare_bytes,
    compare_images,
    difference_image,
)
from mlox_subset.images.dds import DdsError, read_dds
from mlox_subset.images.image import Image, ImageError
from mlox_subset.images.png import encode_png
from mlox_subset.images.reader import ImageFormat, browser_image, detect, read_image
from mlox_subset.images.roles import TextureRole, classify, comparable
from mlox_subset.images.targa import TargaError, read_tga

__all__ = [
    "BitmapError",
    "Comparison",
    "DdsError",
    "Image",
    "ImageError",
    "ImageFormat",
    "TargaError",
    "TextureRole",
    "Verdict",
    "browser_image",
    "classify",
    "comparable",
    "compare_bytes",
    "compare_images",
    "detect",
    "difference_image",
    "encode_png",
    "read_bmp",
    "read_dds",
    "read_image",
    "read_tga",
]
