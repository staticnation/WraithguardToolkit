"""Decoding Targa images, which Morrowind meshes name far more often than ship.

A ``NiSourceTexture`` written in 2003 usually says ``.tga``, because that is
what the exporter of the day wrote. The file beside it is usually ``.dds``,
because that is what the packager converted it to, and the engine falls back
from one to the other -- which is why
:mod:`wraithguard.nif.textures` substitutes extensions. But not always: plenty
of mods ship the Targa, and some ship *only* the Targa. Without this, those
resolve, load, and then fail to decode.

**The format is two ideas.** A header giving dimensions and depth, then pixels
either laid out plainly or run-length encoded a packet at a time. Both are
here. Color-mapped images are handled too, since 8-bit paletted Targas turn up
in older mods where disk space mattered.

**Two traps, both silent.** Channels are stored **blue first**, so a decoder
that assumes RGB produces an image that looks right until you notice the sky is
orange. And the origin is the **bottom-left** unless a descriptor bit says
otherwise, so a decoder that ignores it returns the image upside down -- which
on a tiling texture is not obvious at all, and on a comparison would report two
identical files as different.
"""

from __future__ import annotations

import wraithguard_native as _native

from wraithguard.images.image import Image, ImageError


class TargaError(ImageError):
    """Raised when a Targa image cannot be decoded."""


def read_tga(data: bytes) -> Image:
    """Decode a Targa image.

    Args:
        data: The whole file.

    Returns:
        The image, in top-down reading order whatever the file's own origin.

    Raises:
        TargaError: If the file is malformed, truncated, or uses a variant this
            decoder does not handle.
    """
    # Header, colour map, run-length packets, pixel depths and orientation
    # (bottom origin unless the descriptor says top) are all handled in
    # native/src/img.rs -- a port of the Python that was here, with the same
    # checks and messages; tests/test_images_native_parity.py holds them equal.
    try:
        width, height, pixels = _native.tga_decode(data)
    except ValueError as exc:
        raise TargaError(str(exc)) from exc
    return Image(width, height, pixels)
