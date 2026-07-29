"""Tests for deciding whether two versions of a texture actually differ.

The question this answers is the one a user has when the conflict scan says two
mods ship the same texture: *does the winner look different?* Every case here
is one where a naive answer -- compare the bytes, or average the difference --
gives the wrong one.
"""

from __future__ import annotations

import pytest

from mlox_subset.images import (
    Image,
    TextureRole,
    Verdict,
    compare_bytes,
    compare_images,
    difference_image,
)
from mlox_subset.images.viewer import build_compare_page


def solid(width: int, height: int, colour: tuple[int, int, int, int]) -> Image:
    """An image of one colour.

    Args:
        width: Width in pixels.
        height: Height in pixels.
        colour: Red, green, blue and alpha.

    Returns:
        The image.
    """
    return Image(width, height, bytes(colour) * (width * height))


class TestTheCheapAnswersComeFirst:
    """Most pairs in a mod collection need no pixels at all."""

    def test_identical_bytes_are_settled_without_decoding(self) -> None:
        """The common case: the same file shipped by two mods.

        Deciding this by decoding would spend the cost of the whole feature on
        the pairs that least need it.
        """
        outcome = compare_bytes(b"not even an image", b"not even an image")
        assert outcome.verdict is Verdict.IDENTICAL
        assert not outcome.differs

    def test_the_same_image_saved_twice_is_not_a_difference(self) -> None:
        """Different bytes, identical pixels -- a re-save or a re-compression.

        Reporting this as a conflict would bury the real ones, and it is
        extremely common: every tool that touches a DDS rewrites its header.
        """
        image = solid(4, 4, (10, 20, 30, 255))
        outcome = compare_images(image, Image(4, 4, image.pixels))
        assert outcome.verdict is Verdict.SAME_PIXELS
        assert not outcome.differs


class TestDifferentSizesAreAnAnswer:
    """A retexture that doubles the resolution is the commonest conflict here."""

    def test_a_size_change_is_reported_rather_than_rescaled(self) -> None:
        """Resampling would invent pixels and then report differences in them.

        The dimensions *are* the finding, so they are what gets reported.
        """
        outcome = compare_images(solid(4, 4, (1, 2, 3, 255)), solid(8, 8, (1, 2, 3, 255)))
        assert outcome.verdict is Verdict.DIFFERENT_SIZE
        assert outcome.differs
        assert outcome.left_size == (4, 4)
        assert outcome.right_size == (8, 8)

    def test_no_difference_image_is_offered_for_mismatched_sizes(self) -> None:
        """And asking for one is refused rather than silently rescaled."""
        with pytest.raises(Exception, match="cannot difference"):
            difference_image(solid(4, 4, (0, 0, 0, 255)), solid(8, 8, (0, 0, 0, 255)))


class TestRolesAreCheckedBeforePixels:
    """Comparing a normal map to a diffuse map gives a confident wrong number."""

    def test_a_normal_map_against_a_diffuse_map_is_not_a_conflict(self) -> None:
        """They are complementary channels of one material, not rivals."""
        outcome = compare_images(
            solid(4, 4, (128, 128, 255, 255)),
            solid(4, 4, (90, 60, 30, 255)),
            left_role=TextureRole.NORMAL,
            right_role=TextureRole.DIFFUSE,
        )
        assert outcome.verdict is Verdict.NOT_COMPARABLE
        assert not outcome.differs

    def test_two_normal_maps_are_compared_normally(self) -> None:
        """The case the whole role model exists to permit."""
        outcome = compare_images(
            solid(4, 4, (128, 128, 255, 255)),
            solid(4, 4, (140, 120, 250, 255)),
            left_role=TextureRole.NORMAL,
            right_role=TextureRole.NORMAL,
        )
        assert outcome.verdict is Verdict.DIFFERENT


class TestTheMetricsAnswerDifferentQuestions:
    """A single mean would rank a re-compression above a real retexture."""

    def test_a_requantisation_step_is_not_counted_as_a_change(self) -> None:
        """A different DXT compressor moves nearly every pixel by a level.

        With a threshold of zero, every recompressed texture in a collection
        would report as 100% changed, which is true and useless.
        """
        left = solid(8, 8, (100, 100, 100, 255))
        right = solid(8, 8, (101, 99, 100, 255))
        outcome = compare_images(left, right)
        assert outcome.changed_share == 0.0
        assert outcome.worst_channel == 1

    def test_a_small_region_changing_a_lot_is_visible_in_the_worst_channel(self) -> None:
        """Few pixels, large move: the case a mean average hides."""
        pixels = bytearray(solid(10, 10, (0, 0, 0, 255)).pixels)
        pixels[0:4] = bytes((255, 255, 255, 255))
        outcome = compare_images(solid(10, 10, (0, 0, 0, 255)), Image(10, 10, bytes(pixels)))
        assert outcome.changed_share == pytest.approx(0.01)
        assert outcome.worst_channel == 255
        # The mean is tiny, which is exactly why it cannot be the only number.
        assert outcome.mean_channel < 3

    def test_an_alpha_only_change_is_still_a_change(self) -> None:
        """A changed cutout mask alters what the mesh looks like."""
        outcome = compare_images(solid(4, 4, (9, 9, 9, 255)), solid(4, 4, (9, 9, 9, 0)))
        assert outcome.verdict is Verdict.DIFFERENT
        assert outcome.changed_share == 1.0


class TestTheDifferenceImage:
    """A map of where the change is, not a measurement of how much."""

    def test_unchanged_pixels_are_opaque_black(self) -> None:
        """Transparent would let the viewer's own background read as content."""
        image = solid(4, 4, (40, 50, 60, 255))
        result = difference_image(image, Image(4, 4, image.pixels))
        assert result.pixel(0, 0) == (0, 0, 0, 255)

    def test_a_change_is_amplified_so_it_can_be_seen(self) -> None:
        """A real difference is a few levels, which renders as black."""
        left = solid(2, 2, (100, 100, 100, 255))
        right = solid(2, 2, (105, 100, 100, 255))
        result = difference_image(left, right, amplify=8)
        assert result.pixel(0, 0)[0] == 40

    def test_amplification_saturates_rather_than_wrapping(self) -> None:
        """Wrapping would draw the largest differences as though they were nil."""
        result = difference_image(
            solid(2, 2, (0, 0, 0, 255)), solid(2, 2, (255, 0, 0, 255)), amplify=8
        )
        assert result.pixel(0, 0)[0] == 255

    def test_an_alpha_change_shows_in_the_visible_channels(self) -> None:
        """Rendering it as alpha would make the difference invisible."""
        result = difference_image(solid(2, 2, (0, 0, 0, 255)), solid(2, 2, (0, 0, 0, 0)))
        red, green, blue, alpha = result.pixel(0, 0)
        assert alpha == 255
        assert max(red, green, blue) == 255


class TestTheComparisonPage:
    """One file, no network, and an explanation when a pane would be empty."""

    def _page(self, outcome_pair: tuple[Image, Image], **kwargs: object) -> str:
        """Build a page for two images.

        Args:
            outcome_pair: The two images.
            kwargs: Passed to the builder.

        Returns:
            The page.
        """
        left, right = outcome_pair
        outcome = compare_images(left, right)
        return build_compare_page(
            ("ModA", b"\x89PNG\r\n\x1a\n", "image/png"),
            ("ModB", b"\x89PNG\r\n\x1a\n", "image/png"),
            outcome,
            **kwargs,  # type: ignore[arg-type]
        )

    def test_it_inlines_everything_by_default(self) -> None:
        """So the exported file works from disk with no server."""
        page = self._page((solid(2, 2, (1, 1, 1, 255)), solid(2, 2, (9, 9, 9, 255))))
        assert "data:image/png;base64," in page
        assert "http://" not in page

    def test_a_sink_replaces_inlining_for_a_served_page(self) -> None:
        """The same builder produces both, as the mesh viewer does."""
        page = self._page(
            (solid(2, 2, (1, 1, 1, 255)), solid(2, 2, (9, 9, 9, 255))),
            sink=lambda _b, _t="": {"url": "http://127.0.0.1:1/a.png"},
        )
        assert "http://127.0.0.1:1/a.png" in page
        assert "base64," not in page

    def test_a_missing_difference_pane_says_why(self) -> None:
        """An empty pane reads as a broken feature rather than as an answer."""
        page = self._page((solid(4, 4, (1, 1, 1, 255)), solid(8, 8, (1, 1, 1, 255))))
        assert "different sizes" in page
        assert "invent pixels" in page

    def test_the_verdict_line_carries_the_numbers(self) -> None:
        """The summary is the thing a user reads first, so it has to say something."""
        page = self._page((solid(4, 4, (0, 0, 0, 255)), solid(4, 4, (255, 255, 255, 255))))
        assert "different" in page
        assert "100.00%" in page

    def test_the_title_is_escaped(self) -> None:
        """Texture paths come from mod files and are not trusted input."""
        page = self._page(
            (solid(2, 2, (1, 1, 1, 255)), solid(2, 2, (9, 9, 9, 255))),
            title="<script>alert(1)</script>",
        )
        assert "<script>alert(1)</script>" not in page
        assert "&lt;script&gt;" in page


class TestTheLitMaterialView:
    """The only view in which two normal maps can be compared at all.

    A flat side-by-side view of a normal map is a field of pale blue. What a
    normal map encodes is how a surface catches light, so seeing one requires
    a light -- and comparing two requires the *same* light on both.
    """

    def _page(self, **kwargs: object) -> str:
        """Build a comparison page with maps and a library.

        Args:
            kwargs: Passed to the builder.

        Returns:
            The page.
        """
        left, right = solid(4, 4, (200, 120, 80, 255)), solid(4, 4, (190, 130, 90, 255))
        png = (b"\x89PNG\r\n\x1a\n", "image/png")
        return build_compare_page(
            ("ModA", *png),
            ("ModB", *png),
            compare_images(left, right),
            **kwargs,  # type: ignore[arg-type]
        )

    def test_the_lit_mode_is_absent_without_a_library(self) -> None:
        """There is no renderer, so offering the button would offer a dead one."""
        page = self._page()
        assert '"canLight": false' in page
        assert "Lit material" in page  # the string exists...
        assert "if (data.canLight) modes.push" in page  # ...but is gated

    def test_a_library_url_is_wrapped_in_the_commonjs_shim(self) -> None:
        """The vendored three.js is a CommonJS build.

        Without the shim the page fails with "THREE is not defined", which
        reads as a broken feature rather than a missing global. That bug has
        already cost this project one debugging session.
        """
        page = self._page(library_url="http://127.0.0.1:1/three.js")
        assert "var exports = {}" in page
        assert "module.exports || exports" in page
        assert '<script src="http://127.0.0.1:1/three.js"></script>' in page

    def test_a_map_toggle_appears_only_where_a_map_exists(self) -> None:
        """A permanently dead control implies the feature is broken."""
        png = (b"\x89PNG\r\n\x1a\n", "image/png")
        with_normal = self._page(
            library_url="http://x/three.js", left_maps={"_n": png}
        )
        without = self._page(library_url="http://x/three.js")
        assert '"_n"' in with_normal
        assert '"leftMaps": {}' in without

    def test_normal_maps_are_not_loaded_as_srgb(self) -> None:
        """A normal map is a field of vectors, not colour.

        Reading one as sRGB bends every vector in it before it is used, which
        tilts the lighting everywhere and looks like a subtly wrong material
        rather than a bug.
        """
        page = self._page(library_url="http://x/three.js")
        assert "texture(maps[\"_n\"].url, false)" in page
        assert "texture(base, true)" in page

    def test_turning_the_diffuse_map_off_leaves_a_surface(self) -> None:
        """Not a black hole, which would read as a broken toggle."""
        page = self._page(library_url="http://x/three.js")
        assert "show.diffuse ? 0xffffff : 0x9aa0aa" in page
