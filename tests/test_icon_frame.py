"""Getting a PNG frame out of a .ico without a display to test it on.

Split out of the GUI script for exactly this reason: ``wraithguard_toolkit_gui.py``
imports tkinter at module level, and this suite doesn't -- a window icon that
never showed up on Steam Deck (``Tk.iconbitmap()`` only understands XBM on
Linux, and the app was passing it a Windows .ico) turned out to need a real
fix, and a real fix needs to be checked by something other than a person
launching the app and looking. See :mod:`wraithguard.images.ico` for the full
story; this only exercises the byte-parsing it does, never Tk itself -- that
half is tests/test_gui_icon.py, which does need a display and skips without one.
"""

from __future__ import annotations

import struct
import zlib
from typing import Final

from wraithguard.images.ico import largest_png_frame

#: PNG's own signature -- unrelated to ICO, this is just how PNG starts.
PNG_MAGIC: Final = b"\x89PNG\r\n\x1a\n"


def make_png(width: int, height: int, rgba: tuple[int, int, int, int]) -> bytes:
    """Build a real, valid, single-color RGBA PNG -- small enough to write by
    hand, real enough that a genuine PNG decoder (Tk's) accepts it.

    Args:
        width: Image width in pixels.
        height: Image height in pixels.
        rgba: The one color every pixel gets.

    Returns:
        A complete PNG file's bytes.
    """

    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

    ihdr = chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    raw = b"".join(b"\x00" + bytes(rgba) * width for _row in range(height))
    idat = chunk(b"IDAT", zlib.compress(raw))
    iend = chunk(b"IEND", b"")
    return PNG_MAGIC + ihdr + idat + iend


def build_ico(entries: list[tuple[int, int, bytes]]) -> bytes:
    """Assemble a raw ``.ico`` file from ``(width, height, frame_bytes)`` entries.

    ``frame_bytes`` is written verbatim -- a real PNG for the cases meant to
    be found, arbitrary non-PNG bytes for the classic-BMP/DIB cases this
    reader is meant to skip. Nothing here decodes a DIB, so there is no need
    to build a valid one to prove it gets ignored.

    Args:
        entries: One tuple per icon size, largest last or first, order
            doesn't matter -- the reader picks by declared area, not position.

    Returns:
        The file bytes.
    """
    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    table = b""
    data = b""
    for width, height, frame in entries:
        table += struct.pack("<BBBBHHII", width, height, 0, 0, 1, 32, len(frame), offset)
        data += frame
        offset += len(frame)
    return header + table + data


class TestTheLargestPngFrameIsFound:
    """The common case: a modern icon, one or more PNG-encoded sizes."""

    def test_a_single_png_frame_is_returned_byte_exact(self) -> None:
        png = make_png(16, 16, (255, 0, 0, 255))
        ico = build_ico([(16, 16, png)])
        assert largest_png_frame(ico) == png

    def test_the_largest_of_several_png_frames_wins(self) -> None:
        small = make_png(16, 16, (255, 0, 0, 255))
        big = make_png(32, 32, (0, 255, 0, 255))
        ico = build_ico([(16, 16, small), (32, 32, big)])
        assert largest_png_frame(ico) == big

    def test_frame_order_in_the_file_does_not_matter(self) -> None:
        """Only declared area decides -- not which entry comes first."""
        small = make_png(16, 16, (255, 0, 0, 255))
        big = make_png(32, 32, (0, 255, 0, 255))
        ico = build_ico([(32, 32, big), (16, 16, small)])
        assert largest_png_frame(ico) == big

    def test_a_zero_byte_pair_means_256_not_zero(self) -> None:
        """ICONDIRENTRY's one quirk: 0 in the size byte encodes 256."""
        small = make_png(16, 16, (255, 0, 0, 255))
        huge = make_png(256, 256, (0, 0, 255, 255))
        ico = build_ico([(16, 16, small), (0, 0, huge)])  # 0, 0 == 256x256
        assert largest_png_frame(ico) == huge

    def test_a_png_frame_among_bmp_frames_is_still_found(self) -> None:
        """A mixed icon -- small classic sizes as BMP, larger ones as PNG,
        which is how most real-world icon tools export -- must still surface
        the PNG one."""
        bmp_like = b"\x28\x00\x00\x00" + b"\x00" * 36  # a DIB header shape, not a PNG
        png = make_png(48, 48, (10, 20, 30, 255))
        ico = build_ico([(16, 16, bmp_like), (48, 48, png)])
        assert largest_png_frame(ico) == png


class TestNoUsablePngFrameMeansNone:
    """An icon this old, or this malformed, is a graceful miss, not a crash."""

    def test_an_icon_with_only_bmp_frames_returns_none(self) -> None:
        bmp_like = b"\x28\x00\x00\x00" + b"\x00" * 36
        ico = build_ico([(16, 16, bmp_like), (32, 32, bmp_like * 2)])
        assert largest_png_frame(ico) is None

    def test_not_an_icondir_at_all_returns_none(self) -> None:
        assert largest_png_frame(b"just some random file bytes, not an icon") is None

    def test_empty_bytes_return_none(self) -> None:
        assert largest_png_frame(b"") is None

    def test_a_truncated_header_returns_none(self) -> None:
        assert largest_png_frame(b"\x00\x00\x01") is None

    def test_the_wrong_reserved_word_returns_none(self) -> None:
        """A well-formed-looking header that simply isn't an ICONDIR."""
        wrong = struct.pack("<HHH", 1, 1, 1) + b"\x00" * 16
        assert largest_png_frame(wrong) is None

    def test_a_cursor_file_type_is_not_treated_as_an_icon(self) -> None:
        """Type 2 is a .cur (cursor) file -- same container, different
        semantics (a hotspot instead of nothing); not what this reads."""
        png = make_png(16, 16, (1, 2, 3, 255))
        ico = build_ico([(16, 16, png)])
        as_cursor = ico[:2] + struct.pack("<H", 2) + ico[4:]
        assert largest_png_frame(as_cursor) is None

    def test_zero_declared_entries_returns_none(self) -> None:
        assert largest_png_frame(struct.pack("<HHH", 0, 1, 0)) is None

    def test_an_entry_pointing_past_the_end_of_the_file_is_skipped(self) -> None:
        """A corrupt or truncated download must not read past the buffer."""
        header = struct.pack("<HHH", 0, 1, 1)
        # Declares a 1000-byte frame at offset 22, but the file ends there.
        entry = struct.pack("<BBBBHHII", 16, 16, 0, 0, 1, 32, 1000, 22)
        assert largest_png_frame(header + entry) is None

    def test_a_directory_table_longer_than_the_file_stops_cleanly(self) -> None:
        """The header claims more entries than the file has room for; this
        must stop at the last complete one rather than reading garbage."""
        header = struct.pack("<HHH", 0, 1, 5)  # claims 5, file has room for 0
        assert largest_png_frame(header) is None

    def test_a_frame_too_short_to_hold_the_png_signature_is_skipped(self) -> None:
        header = struct.pack("<HHH", 0, 1, 1)
        entry = struct.pack("<BBBBHHII", 16, 16, 0, 0, 1, 32, 3, 22)
        assert largest_png_frame(header + entry + b"\x89PN") is None
