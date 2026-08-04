"""Deciding whether two versions of a texture actually differ, and how.

The conflict scan can already say that two mods ship the same texture path. It
cannot say whether that matters. Two files with different bytes may be the same
image saved twice; two files of the same size may be entirely different art.
Answering the question a user actually has -- *does the winner look different?*
-- needs pixels.

**Identical bytes are not the interesting case, and are handled first.** A hash
comparison settles most pairs for free and is what stops this module decoding
thousands of textures to conclude nothing.

**Different dimensions are a real answer, not an obstacle.** A retexture that
doubles the resolution is the single most common texture conflict in this game,
and it is a *finding*, not a failure to compare. Nothing is rescaled here:
resampling would invent pixels and then report differences in the pixels it
invented. The dimensions are reported and the pixel comparison is skipped.

**Roles are checked before pixels.** Comparing a normal map against a diffuse
map produces a large, confident, meaningless number. See
:mod:`wraithguard.images.roles`.

**What "different" means is a choice, so it is made explicitly.** A JPEG-style
requantisation moves almost every pixel by one or two levels while looking
identical; a genuine retexture moves a smaller number of pixels a great deal.
Reporting only a mean would call the first one a bigger change than the second.
So this reports both the *share of pixels that changed at all* and *how far the
worst of them moved*, and leaves the judgement to the reader.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import Enum
from typing import Final

from wraithguard.images.image import Image, ImageError
from wraithguard.images.reader import read_image
from wraithguard.images.roles import TextureRole, classify, comparable
from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)

#: Channel difference below which two pixels are treated as the same color.
#: Not zero: re-encoding a texture through a different DXT compressor moves
#: nearly every pixel by a level or two without changing the image, and a
#: threshold of zero would report those as wholly different.
_SAME: Final[int] = 2

#: A ceiling on pixels compared. Difference images are for looking at, and a
#: pair of 4096-square textures is 16 million pixels per side in pure Python.
_MAX_COMPARE: Final[int] = 16 << 20


class Verdict(Enum):
    """What the comparison concluded.

    Distinct outcomes rather than a single score, because the actions they
    suggest are different: identical files can be deduplicated, rescaled ones
    are a deliberate upgrade, and undecodable ones are a problem with the mod.
    """

    IDENTICAL = "identical"
    """Byte for byte the same file. Nothing to look at."""

    SAME_PIXELS = "same-pixels"
    """Different files, identical images -- re-saved or re-compressed."""

    DIFFERENT = "different"
    """The images differ. How much is in the metrics."""

    DIFFERENT_SIZE = "different-size"
    """Different dimensions, so pixels were not compared. Usually an upscale."""

    NOT_COMPARABLE = "not-comparable"
    """Different roles: a normal map against a diffuse map, say."""

    UNDECODABLE = "undecodable"
    """At least one side could not be decoded. A finding about the mod."""


@dataclass(frozen=True, slots=True)
class Comparison:
    """The outcome of comparing two textures.

    Attributes:
        verdict: What was concluded.
        detail: A sentence naming the reason, for a report line.
        left_size: Width and height of the first image, when it decoded.
        right_size: The same for the second.
        changed_share: Fraction of pixels that differ by more than a
            requantisation step, 0.0 to 1.0.
        worst_channel: The largest single-channel difference found, 0-255.
        mean_channel: The mean absolute channel difference across the image.
        left_role: What the first texture is for.
        right_role: What the second is for.
    """

    verdict: Verdict
    detail: str = ""
    left_size: tuple[int, int] | None = None
    right_size: tuple[int, int] | None = None
    changed_share: float = 0.0
    worst_channel: int = 0
    mean_channel: float = 0.0
    left_role: TextureRole = TextureRole.UNKNOWN
    right_role: TextureRole = TextureRole.UNKNOWN

    @property
    def differs(self) -> bool:
        """Whether a user would see any difference at all."""
        return self.verdict in (Verdict.DIFFERENT, Verdict.DIFFERENT_SIZE)

    @property
    def worth_showing(self) -> bool:
        """Whether a side-by-side view would tell the user anything.

        Identical pixels are not worth two panes, and an undecodable file has
        nothing to put in them.
        """
        return self.differs


def compare_bytes(left: bytes, right: bytes, *, reference: str = "", slot: str = "") -> Comparison:
    """Compare two texture files.

    Args:
        left: The whole first file.
        right: The whole second file.
        reference: The texture path both provide, used to infer their role.
        slot: The vanilla texture slot, when the reference came from a mesh.

    Returns:
        What was concluded, and the numbers behind it.
    """
    role = classify(reference, slot) if reference else TextureRole.UNKNOWN
    if left == right:
        # The common case in a mod collection: the same file shipped twice.
        return Comparison(
            Verdict.IDENTICAL,
            "byte-for-byte identical",
            left_role=role,
            right_role=role,
        )
    if not comparable(role, role):  # pragma: no cover -- a role is comparable with itself
        raise AssertionError("a role must be comparable with itself")

    try:
        first = read_image(left)
    except ImageError as exc:
        return Comparison(
            Verdict.UNDECODABLE,
            f"the first could not be decoded: {exc}",
            left_role=role,
            right_role=role,
        )
    try:
        second = read_image(right)
    except ImageError as exc:
        return Comparison(
            Verdict.UNDECODABLE,
            f"the second could not be decoded: {exc}",
            left_size=(first.width, first.height),
            left_role=role,
            right_role=role,
        )

    return compare_images(first, second, left_role=role, right_role=role)


def compare_images(
    left: Image,
    right: Image,
    *,
    left_role: TextureRole = TextureRole.UNKNOWN,
    right_role: TextureRole = TextureRole.UNKNOWN,
) -> Comparison:
    """Compare two decoded surfaces.

    Args:
        left: The first image.
        right: The second.
        left_role: What the first is for.
        right_role: What the second is for.

    Returns:
        What was concluded, and the numbers behind it.
    """
    sizes = ((left.width, left.height), (right.width, right.height))
    if not comparable(left_role, right_role):
        return Comparison(
            Verdict.NOT_COMPARABLE,
            f"a {left_role.value} map and a {right_role.value} map are different "
            f"channels of one material, not rival versions of one texture",
            *sizes,
            left_role=left_role,
            right_role=right_role,
        )
    if sizes[0] != sizes[1]:
        # Not rescaled deliberately: resampling would invent pixels and then
        # report differences in the pixels it invented.
        return Comparison(
            Verdict.DIFFERENT_SIZE,
            f"{left.width}x{left.height} against {right.width}x{right.height}, "
            f"so the pixels were not compared",
            *sizes,
            left_role=left_role,
            right_role=right_role,
        )
    if left.pixels == right.pixels:
        return Comparison(
            Verdict.SAME_PIXELS,
            "different files, identical images -- re-saved or re-compressed",
            *sizes,
            left_role=left_role,
            right_role=right_role,
        )
    if left.width * left.height > _MAX_COMPARE:
        return Comparison(
            Verdict.DIFFERENT,
            f"{left.width}x{left.height} is too large to compare pixel by pixel",
            *sizes,
            left_role=left_role,
            right_role=right_role,
        )

    changed, worst, total = _measure(left.pixels, right.pixels)
    pixels = left.width * left.height
    share = changed / pixels if pixels else 0.0
    mean = total / (pixels * 4) if pixels else 0.0
    return Comparison(
        Verdict.DIFFERENT,
        f"{share:.1%} of pixels differ, worst channel by {worst}",
        *sizes,
        changed_share=share,
        worst_channel=worst,
        mean_channel=mean,
        left_role=left_role,
        right_role=right_role,
    )


def _measure(left: bytes, right: bytes) -> tuple[int, int, int]:
    """Count how many pixels changed and by how much.

    Two numbers rather than one, because they answer different questions. A
    re-compression nudges nearly every pixel slightly: high share, low worst. A
    retexture of one region moves few pixels a long way: low share, high worst.
    A single mean would rank the first above the second.

    Args:
        left: RGBA bytes.
        right: RGBA bytes of the same length.

    Returns:
        Pixels that changed beyond the requantisation threshold, the largest
        single-channel difference, and the summed absolute difference.
    """
    changed = 0
    worst = 0
    total = 0
    for start in range(0, len(left), 4):
        biggest = 0
        for offset in range(4):
            gap = left[start + offset] - right[start + offset]
            if gap < 0:
                gap = -gap
            total += gap
            if gap > biggest:  # noqa: PLR1730 -- a call per channel, per pixel
                biggest = gap
        if biggest > _SAME:
            changed += 1
        if biggest > worst:  # noqa: PLR1730 -- hot loop; max() costs a call
            worst = biggest
    return changed, worst, total


def difference_image(left: Image, right: Image, *, amplify: int = 4) -> Image:
    """Build an image showing where two textures differ.

    A plain absolute difference is almost always *black*: real changes between
    two versions of a texture are a few levels on most pixels, which is
    invisible. So the difference is amplified, and the result is a map of
    *where* the change is rather than a measurement of how much -- the numbers
    in :class:`Comparison` are the measurement.

    Unchanged pixels come out black and fully opaque rather than transparent,
    so the image can be looked at against any background without the viewer's
    own checkerboard reading as content.

    Args:
        left: The first image.
        right: The second, which must be the same size.
        amplify: How much to multiply each difference by.

    Returns:
        An opaque image of the amplified per-channel difference.

    Raises:
        ImageError: If the two are not the same size, which the caller should
            have established with :func:`compare_images` first.
    """
    if (left.width, left.height) != (right.width, right.height):
        raise ImageError(
            f"cannot difference {left.width}x{left.height} against " f"{right.width}x{right.height}"
        )
    out = bytearray(len(left.pixels))
    first, second = left.pixels, right.pixels
    for index in range(0, len(first), 4):
        for offset in range(3):
            gap = first[index + offset] - second[index + offset]
            if gap < 0:
                gap = -gap
            scaled = gap * amplify
            out[index + offset] = 255 if scaled > 255 else scaled
        # Alpha differences matter -- a changed cutout mask is a real change --
        # but showing them *as* alpha would make the difference invisible. They
        # are folded into the visible channels instead.
        alpha_gap = first[index + 3] - second[index + 3]
        if alpha_gap < 0:
            alpha_gap = -alpha_gap
        if alpha_gap:
            scaled = alpha_gap * amplify
            capped = 255 if scaled > 255 else scaled
            for offset in range(3):
                if capped > out[index + offset]:  # noqa: PLR1730 -- hot loop
                    out[index + offset] = capped
        out[index + 3] = 255
    return Image(left.width, left.height, bytes(out))


def digest(data: bytes) -> str:
    """A short stable identity for a texture file.

    Args:
        data: The whole file.

    Returns:
        A hex digest, truncated to something a report line can carry.
    """
    return hashlib.sha256(data).hexdigest()[:16]
