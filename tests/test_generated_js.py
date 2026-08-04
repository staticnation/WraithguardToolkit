"""Every page this project generates must at least be parseable JavaScript.

**Why this file exists.** Two bugs shipped in generated JS that nothing here
executed, and both were invisible to a suite that checked the pages by
substring:

* ``wraithguard/nif/viewer.py`` held its page in a **non-raw** Python string, so
  the eleven ``\\n`` sequences in its shader assembly became real newlines,
  landing inside JavaScript string literals. The whole page was a syntax error
  and the viewer rendered nothing.
* ``wraithguard/images/viewer.py`` declared ``function texture(url, srgb)``
  while every call site passed three arguments and the body used the third.
  In JavaScript an undeclared identifier is a ReferenceError, not ``undefined``
  -- thrown from inside an image's load handler, so the lit view died as soon
  as a texture arrived.

Neither is exotic. Both are what happens when one language is assembled inside
another as text and nothing ever asks the target language whether the result is
valid. A substring assertion cannot notice either, because the substrings were
all present and correct.

**What this checks, and what it does not.** ``node --check`` parses; it does not
run. It would have caught the first bug outright and the second only through
the separate arity check in ``test_image_compare.py``. Parsing is a floor, not
a guarantee -- but it is a floor these pages twice fell through.

Skipped when Node is unavailable, because a missing developer tool should not
fail someone else's test run.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

from wraithguard.images import Image, compare_images, difference_image, encode_png
from wraithguard.images.viewer import build_compare_page
from wraithguard.nif.geometry import Mesh
from wraithguard.nif.viewer import build_viewer_page

#: Node is a developer convenience here, not a dependency of the project.
_NODE = shutil.which("node")

pytestmark = pytest.mark.skipif(_NODE is None, reason="node is not installed")

#: A script tag that fetches its source has no inline body to check, and the
#: JSON payload block is data rather than code.
_INLINE = re.compile(r"<script(?![^>]*\bsrc=)(?![^>]*application/json)[^>]*>(.*?)</script>", re.S)


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


def png(image: Image) -> tuple[bytes, str]:
    """Encode an image for a page payload.

    Args:
        image: The image.

    Returns:
        PNG bytes and their MIME type.
    """
    return encode_png(image), "image/png"


def assert_parses(page: str) -> None:
    """Check every inline script in a page parses as JavaScript.

    Args:
        page: The whole HTML document.

    Raises:
        AssertionError: With Node's own message, which names the line.
    """
    body = "\n;\n".join(part for part in _INLINE.findall(page) if part.strip())
    assert body, "the page carries no inline script at all"
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "page.js"
        path.write_text(body, encoding="utf-8")
        result = subprocess.run(  # noqa: S603 -- a fixed argv, no shell
            [str(_NODE), "--check", str(path)],
            capture_output=True,
            text=True,
            check=False,
        )
    assert result.returncode == 0, f"generated JavaScript does not parse:\n{result.stderr}"


TRIANGLE = Mesh(
    name="t",
    vertices=[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)],
    triangles=[(0, 1, 2)],
    uvs=[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
)


class TestTheMeshViewerParses:
    """Both shapes of the page, because they differ in how the library arrives."""

    def test_served(self) -> None:
        """The library is fetched, so only our own script is inline."""
        assert_parses(
            build_viewer_page(
                [("side", [TRIANGLE])],
                sink=lambda _b, _t="": {"url": "http://127.0.0.1:1/g.bin"},
                library_url="http://127.0.0.1:1/three.js",
            )
        )

    def test_standalone(self) -> None:
        """The library is inlined, so the whole document is one file.

        This is the shape the export produces, and the one nobody runs during
        development -- which is precisely why it is worth parsing.
        """
        assert_parses(build_viewer_page([("side", [TRIANGLE])]))


class TestTheTextureComparisonParses:
    """Including the lit view, whose code only exists when a library does."""

    def test_standalone_with_maps(self) -> None:
        """Auxiliary maps add the material code path."""
        left, right = solid(4, 4, (200, 120, 80, 255)), solid(4, 4, (190, 130, 90, 255))
        assert_parses(
            build_compare_page(
                ("ModA", *png(left)),
                ("ModB", *png(right)),
                compare_images(left, right),
                difference=png(difference_image(left, right)),
                left_maps={"_n": png(left), "_spec": png(right)},
                right_maps={"_n": png(right)},
            )
        )

    def test_served(self) -> None:
        """The loopback shape, where the library is a URL."""
        left, right = solid(4, 4, (1, 2, 3, 255)), solid(4, 4, (9, 8, 7, 255))
        assert_parses(
            build_compare_page(
                ("ModA", *png(left)),
                ("ModB", *png(right)),
                compare_images(left, right),
                library_url="http://127.0.0.1:1/three.js",
            )
        )

    def test_without_a_library(self) -> None:
        """The flat-only page, which must still be valid with the lit code gated out."""
        left, right = solid(4, 4, (1, 2, 3, 255)), solid(4, 4, (9, 8, 7, 255))
        assert_parses(
            build_compare_page(
                ("ModA", *png(left)),
                ("ModB", *png(right)),
                compare_images(left, right),
            )
        )
