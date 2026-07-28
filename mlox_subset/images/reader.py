"""One entry point for every texture format, choosing by content not by name.

**The extension cannot be trusted here.** That is not a general principle, it
is a specific fact about this game: Morrowind's engine falls back between
extensions, so mod authors ship a ``.dds`` and reference it as ``.tga`` and
nothing complains. :mod:`mlox_subset.nif.textures` already resolves a reference
to whichever file actually exists, which means the bytes it hands back
routinely disagree with the name that found them. Dispatching on the name would
send a DDS to the Targa decoder for a large share of the base game.

So the format is read out of the first few bytes. Every format here announces
itself -- except Targa, which predates the convention and has no magic number
at all, and is therefore identified last and by elimination. See
:func:`detect`.

**Two ways to want an image.** :func:`read_image` decodes to RGBA, which is
what a comparison or a difference view needs. :func:`browser_image` returns
bytes some browser can display, which is what the 3D viewer needs -- and for a
PNG those are the same bytes it was given, so it hands them straight over.
"""

from __future__ import annotations

from enum import Enum
from typing import Final

from mlox_subset.images.bitmap import MAGIC as BMP_MAGIC
from mlox_subset.images.bitmap import read_bmp
from mlox_subset.images.dds import MAGIC as DDS_MAGIC
from mlox_subset.images.dds import read_dds
from mlox_subset.images.image import Image, ImageError
from mlox_subset.images.png import encode_png
from mlox_subset.images.targa import read_tga
from mlox_subset.logging_setup import get_logger

LOG = get_logger(__name__)

#: What a PNG starts with. The trailing bytes are a deliberate corruption
#: check -- they catch a transfer that mangled line endings, which is exactly
#: the sort of thing that happens to files passed around modding sites.
PNG_MAGIC: Final[bytes] = b"\x89PNG\r\n\x1a\n"

#: What a TIFF starts with, little- and big-endian respectively.
_TIFF_MAGICS: Final[tuple[bytes, ...]] = (b"II\x2a\x00", b"MM\x00\x2a")

#: The footer a Targa written to the 2.0 specification ends with. Older ones
#: have nothing at all, which is why it cannot be the only test.
_TGA_FOOTER: Final[bytes] = b"TRUEVISION-XFILE."

#: Targa image type codes that exist. A first byte pattern that does not land
#: on one of these is not a Targa.
_TGA_TYPES: Final[frozenset[int]] = frozenset({0, 1, 2, 3, 9, 10, 11})

#: Targa pixel depths that exist.
_TGA_DEPTHS: Final[frozenset[int]] = frozenset({8, 15, 16, 24, 32})


class ImageFormat(Enum):
    """A texture container this tool can recognise.

    Recognising a format it cannot decode is useful in itself: "this is a
    TIFF, which is not supported" is a far better report than "this is not an
    image", and tells the user whether to expect it to work later.
    """

    DDS = "dds"
    PNG = "png"
    BMP = "bmp"
    TGA = "tga"
    TIFF = "tiff"
    UNKNOWN = "unknown"

    @property
    def decodable(self) -> bool:
        """Whether :func:`read_image` can turn this into pixels."""
        return self in (ImageFormat.DDS, ImageFormat.BMP, ImageFormat.TGA)

    @property
    def browser_native(self) -> bool:
        """Whether a browser displays these bytes without help from us.

        PNG and BMP both qualify, which means the viewer can hand them over
        untouched instead of decoding and re-encoding them.
        """
        return self in (ImageFormat.PNG, ImageFormat.BMP)


#: The MIME type for each format a browser can take directly.
_MIME: Final[dict[ImageFormat, str]] = {
    ImageFormat.PNG: "image/png",
    ImageFormat.BMP: "image/bmp",
}


def _looks_like_tga(data: bytes) -> bool:
    """Judge whether these bytes are a Targa, which carries no magic number.

    Targa predates the idea of a signature, so this reads the header and asks
    whether every field is one the format defines. That is weaker than a magic
    number and is why it is tried only after every other format has declined,
    but the combination of a valid type, a valid depth, a sane colour-map flag
    and non-zero dimensions is specific enough in practice.

    Args:
        data: The whole file.

    Returns:
        Whether to hand it to the Targa decoder.
    """
    if len(data) >= 26 and data[-18:-1] == _TGA_FOOTER:
        return True
    if len(data) < 18:
        return False
    has_map, image_type, depth = data[1], data[2], data[16]
    width = int.from_bytes(data[12:14], "little")
    height = int.from_bytes(data[14:16], "little")
    return (
        has_map in (0, 1)
        and image_type in _TGA_TYPES
        and depth in _TGA_DEPTHS
        and width > 0
        and height > 0
    )


def detect(data: bytes) -> ImageFormat:
    """Say what kind of image these bytes are.

    Args:
        data: The whole file, or at least its first 18 bytes.

    Returns:
        The format, or :attr:`ImageFormat.UNKNOWN` when nothing matches.
    """
    if data.startswith(DDS_MAGIC):
        return ImageFormat.DDS
    if data.startswith(PNG_MAGIC):
        return ImageFormat.PNG
    if data.startswith(BMP_MAGIC):
        return ImageFormat.BMP
    if any(data.startswith(magic) for magic in _TIFF_MAGICS):
        return ImageFormat.TIFF
    if _looks_like_tga(data):
        return ImageFormat.TGA
    return ImageFormat.UNKNOWN


def read_image(data: bytes) -> Image:
    """Decode any supported texture to RGBA.

    Args:
        data: The whole file.

    Returns:
        The decoded surface.

    Raises:
        ImageError: If the format is unrecognised, unsupported, or the file is
            malformed. Callers get one exception type for every format, which
            is the point of this module.
    """
    kind = detect(data)
    if kind is ImageFormat.DDS:
        return read_dds(data)
    if kind is ImageFormat.BMP:
        return read_bmp(data)
    if kind is ImageFormat.TGA:
        return read_tga(data)
    if kind is ImageFormat.PNG:
        # Decoding PNG would mean inflating, unfiltering, and handling five
        # filter types, six colour types, palettes and interlacing -- a real
        # decoder, written to display images the browser already displays.
        # Nothing needs it yet; browser_image passes PNGs through untouched.
        raise ImageError("PNG decoding is not implemented; use browser_image to display one")
    if kind is ImageFormat.TIFF:
        raise ImageError("TIFF is not supported")
    raise ImageError("unrecognised image format")


def browser_image(data: bytes) -> tuple[bytes, str]:
    """Return bytes a browser can display, and their MIME type.

    A PNG is handed back exactly as it arrived. Decoding one only to re-encode
    it would cost time and could only lose fidelity, and a browser's own PNG
    decoder is better than anything written here would be.

    A BMP is *not* passed through by default, even though browsers read it.
    Decoding it first is what makes a corrupt file reportable as a finding
    about the mod rather than a broken image in a pane. Passthrough is kept as
    the fallback for the variants this decoder declines -- a run-length encoded
    bitmap is something a browser handles perfectly well, and showing the user
    their texture beats showing them an error.

    Args:
        data: The whole file.

    Returns:
        The payload and its MIME type.

    Raises:
        ImageError: If the image can be neither decoded nor passed through.
    """
    kind = detect(data)
    if kind is ImageFormat.PNG:
        LOG.debug("passing a PNG through untouched (%d bytes)", len(data))
        return data, _MIME[kind]
    try:
        return encode_png(read_image(data)), "image/png"
    except ImageError:
        if kind.browser_native:
            LOG.info("%s did not decode; handing it to the browser instead", kind.value)
            return data, _MIME[kind]
        raise
