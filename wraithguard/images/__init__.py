"""Reading Morrowind's textures, and writing something a viewer can show.

Every format this game and its mods use, decoded without a third-party
dependency, for the same reason the NIF reader is written rather than imported:
the alternatives are large, compiled, or licensed incompatibly. ``pydds`` is
the closest technical fit for the hardest format here and is GPLv3, which would
relicense the whole project.

* :mod:`~wraithguard.images.dds` -- BC1 to BC5 and BC7, plus uncompressed
  surfaces, in both the original and the Direct3D 10 header form.
* :mod:`~wraithguard.images.bc7` -- the eight-mode BC7 block decoder, which is
  large enough and strange enough to live on its own.
* :mod:`~wraithguard.images.targa` -- Targa, plain and run-length encoded.
* :mod:`~wraithguard.images.bitmap` -- Windows bitmaps, palette and all.
* :mod:`~wraithguard.images.png` -- RGBA out to a PNG, using only :mod:`zlib`.
* :mod:`~wraithguard.images.reader` -- picks the decoder by inspecting the
  bytes, because in this game the file extension is genuinely unreliable.
* :mod:`~wraithguard.images.roles` -- what a texture is *for*, so that a normal
  map is not compared against a diffuse map or shown as though it were color.
* :mod:`~wraithguard.images.compare` -- whether two versions of a texture
  actually differ, and a difference image showing where.

The point of all of it is the same question a mesh conflict raises: when two
mods ship the same texture, does the winner actually look different? Answering
that needs pixels rather than a hash.
"""

from __future__ import annotations

from wraithguard.images.bitmap import BitmapError, read_bmp
from wraithguard.images.compare import (
    Comparison,
    Verdict,
    compare_bytes,
    compare_images,
    difference_image,
)
from wraithguard.images.dds import DdsError, read_dds
from wraithguard.images.image import Image, ImageError
from wraithguard.images.png import encode_png
from wraithguard.images.reader import ImageFormat, browser_image, detect, read_image
from wraithguard.images.roles import TextureRole, classify, comparable
from wraithguard.images.targa import TargaError, read_tga

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
