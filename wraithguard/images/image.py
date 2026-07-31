"""The one thing every texture decoder in this package produces.

Morrowind ships textures in four containers and OpenMW adds a fifth, but the
viewer, the difference view and the conflict report all want the same thing: a
width, a height, and straight RGBA. Keeping that type here rather than inside
any one decoder means ``bitmap`` and ``targa`` do not import from ``dds`` to
borrow a dataclass, which is what they were doing before this module existed.

Alpha is **non-premultiplied** throughout. DXT2 and DXT4 are the premultiplied
variants of DXT3 and DXT5 and are vanishingly rare; they are decoded as their
straight-alpha counterparts, which is what every tool in this space does, and
the difference only shows on pixels that are already nearly transparent.
"""

from __future__ import annotations

from dataclasses import dataclass


class ImageError(Exception):
    """Raised when a texture cannot be decoded.

    Every decoder in this package raises this rather than letting a
    :class:`struct.error`, :class:`IndexError` or :class:`ValueError` escape.
    These files come out of third-party mod archives and are routinely
    truncated or mislabelled, so a bad one has to be reportable as a finding
    about the mod rather than a crash in the tool.
    """


@dataclass(frozen=True, slots=True)
class Image:
    """A decoded surface.

    Attributes:
        width: Width in pixels.
        height: Height in pixels.
        pixels: ``width * height * 4`` bytes of non-premultiplied RGBA, in
            reading order: left to right, top to bottom.
    """

    width: int
    height: int
    pixels: bytes

    def __post_init__(self) -> None:
        """Check the buffer matches the dimensions.

        Raises:
            ImageError: If it does not. A decoder that miscounts rows produces
                an image that renders as diagonal garbage rather than failing,
                and that is far harder to diagnose from a screenshot than an
                exception is.
        """
        expected = self.width * self.height * 4
        if len(self.pixels) != expected:
            raise ImageError(
                f"{self.width}x{self.height} needs {expected} byte(s) of RGBA, "
                f"got {len(self.pixels)}"
            )

    def pixel(self, x: int, y: int) -> tuple[int, int, int, int]:
        """Read one pixel.

        Args:
            x: Column.
            y: Row.

        Returns:
            Red, green, blue and alpha, each 0-255.

        Raises:
            IndexError: If the coordinates are outside the image.
        """
        if not (0 <= x < self.width and 0 <= y < self.height):
            raise IndexError(f"({x}, {y}) is outside {self.width}x{self.height}")
        start = (y * self.width + x) * 4
        r, g, b, a = self.pixels[start : start + 4]
        return (r, g, b, a)
