"""Type stubs for wraithguard_native (native/src)."""

from collections.abc import Sequence
from os import PathLike
from typing import Any

__version__: str

class Archive:
    """A Morrowind ``.bsa``: its index, files read on demand (native/src/bsa.rs)."""

    def __init__(self, path: str | PathLike[str]) -> None: ...
    def __len__(self) -> int: ...
    def __contains__(self, name: str) -> bool: ...
    @property
    def names(self) -> list[str]: ...
    def read(self, name: str) -> bytes | None: ...

def bsa_normalise(name: str) -> str: ...
def plugin_json(data: bytes) -> str:
    """A plugin's records as tes3conv-schema JSON (native/src/esp.rs)."""

def plugin_records_json(data: bytes, keep: list[bytes] | None = None) -> str:
    """A plugin's records as a JSON array, in file order (native/src/esp.rs).

    Modelled records in the tes3conv schema; any other as
    ``{"type": "__Raw", "tag", "flags", "body"}`` (base64 body). ``keep`` limits it
    to those four-byte tags.
    """

def plugin_records_bytes(json: str) -> bytes:
    """Records as ``plugin_records_json`` gives them, back to plugin bytes."""

def plugin_records(
    data: bytes, keep: list[bytes] | None = None, light: bool = False, raw: bool = False
) -> list[dict[str, Any]]:
    """A plugin's records as dicts in the tes3conv schema, with no JSON in between.

    The records the crate models, in file order; ``keep`` limits it to those four-byte
    tags. ``light`` empties what no record key needs: a cell's references, a
    landscape's vertex data, a path grid's points and connections, a script's text
    and bytecode. ``raw`` keeps records the crate does not model, as
    ``{"type": "__Raw", "tag", "flags", "body"}`` with ``body`` as bytes.
    """

def plugin_keys(
    data: bytes,
) -> tuple[list[tuple[str, str, bool]], list[tuple[str, str | int, int | None]]]:
    """A plugin's conflict keys ``(type, id, deleted)`` and cells, computed in Rust.

    Cells are ``("int", name, None)`` or ``("ext", x, y)``. The same result as
    ``wraithguard_toolkit._keys_and_cells`` over its records.
    """

def plugin_write(items: list[dict[str, Any]]) -> bytes:
    """Records as dicts (``plugin_records``'s shape, or ``__Raw``) to plugin bytes, no JSON."""

def plugin_json_file(src: str | PathLike[str], out: str | PathLike[str]) -> int:
    """Convert the plugin at ``src`` to tes3conv-schema JSON at ``out``; the record count."""

def nif_summary(data: bytes) -> str:
    """A mesh's summary as JSON (native/src/nif.rs); ``stopped_reason`` set if unreadable."""

def nif_blocks(data: bytes) -> str:
    """The mesh viewer's block panel as JSON: ``{"tree", "blocks", "complete"}``."""

def nif_edit(data: bytes, edits_json: str) -> bytes:
    """The block panel's edits applied, and the mesh saved by the crate."""

# The numeric core of Merged Lands (native/src/land.rs).

class RelativeGrid:
    """A reference grid plus the deltas one plugin applied to it (land/diff.py)."""

    side: int
    components: int
    def __init__(self, reference: Sequence[int], side: int, components: int = 1) -> None: ...
    @classmethod
    def from_difference(
        cls, reference: Sequence[int], plugin: Sequence[int], side: int, components: int = 1
    ) -> RelativeGrid: ...
    def offset_of(self, x: int, y: int, component: int = 0) -> int: ...
    def value_at(self, x: int, y: int, component: int = 0) -> int: ...
    def delta_at(self, x: int, y: int, component: int = 0) -> int: ...
    def has_difference(self, x: int, y: int) -> bool: ...
    def set_value(self, x: int, y: int, values: Sequence[int]) -> None: ...
    def deltas_at(self, x: int, y: int) -> tuple[int, ...]: ...
    def set_deltas(self, x: int, y: int, deltas: Sequence[int]) -> None: ...
    def to_flat_reference(self) -> list[int]: ...
    def clear(self, x: int, y: int) -> None: ...
    @property
    def is_modified(self) -> bool: ...
    @property
    def num_differences(self) -> int: ...
    def changed_vertices(self) -> list[tuple[int, int]]: ...
    def to_flat(self) -> list[int]: ...
    def to_rows(self) -> list[list[int]]: ...

def merge_grids(
    first: RelativeGrid,
    second: RelativeGrid,
    strategy: str,
    minor_threshold_pct: float = 0.3,
    minor_threshold_min: float = 10.0,
    minor_threshold_max: float = 64.0,
) -> tuple[RelativeGrid, int, int, int, int, int, list[tuple[int, int]]]:
    """merge_layer's per-vertex loop: the merged grid and its counts."""

def limit_slopes(
    cells: dict[tuple[int, int], list[int]],
    limit: int,
    max_passes: int,
    authoritative: set[tuple[int, int]],
    use_curvature: bool = False,
) -> tuple[dict[tuple[int, int], list[int]], int, int, int, int, bool, set[tuple[int, int]]]:
    """The slope limiter, with its counts.

    Returns the adjusted cells, adjusted, pinned, worst excess, passes, whether a
    pass found nothing, and the cells touched.
    """

def count_unencodable(cells: list[list[int]], limit: int) -> int: ...
def vertex_normals_from_heights(
    rows: Sequence[Sequence[float]],
) -> list[list[tuple[int, int, int]]]: ...
def resolve_cell_normals(
    heights: Sequence[int],
    original: Sequence[int] | None = None,
    base: Sequence[int] | None = None,
) -> tuple[list[int], int]: ...
def decode_heights(raw: bytes, offset: float) -> list[int]: ...

# Texture decoding (src/img.rs). Errors are ValueError; wraithguard/images
# re-raises them as DdsError / TargaError / ImageError.

def dds_decode(surface: bytes, width: int, height: int, fourcc: bytes) -> bytes:
    """Decode a block-compressed DDS surface to RGBA.

    ``fourcc`` is DXT1-DXT5, ATI1/BC4U/BC4S, ATI2/BC5U/BC5S, or ``DX10`` for BC7.
    """

def dds_uncompressed(
    surface: bytes, width: int, height: int, bit_count: int, masks: tuple[int, int, int, int]
) -> bytes:
    """Decode an uncompressed DDS surface described by R, G, B, A channel masks."""

def tga_decode(data: bytes) -> tuple[int, int, bytes]:
    """Decode a whole Targa file to ``(width, height, rgba)``, top-down."""

def image_measure(left: bytes, right: bytes, same: int) -> tuple[int, int, int]:
    """Pixels changed beyond ``same``, the worst channel gap, and the summed gap."""

def image_difference(left: bytes, right: bytes, amplify: int) -> bytes:
    """The amplified per-channel difference, alpha folded into RGB, opaque."""

# The load-order lint's record scan (src/lint.rs).

class PluginLinter:
    """Reads one plugin's raw records into the facts the lint checks need."""

    def __init__(
        self,
        evil_gmsts: dict[str, tuple[str, bytes]],
        tribunal_funcs: list[str],
        bloodmoon_funcs: list[str],
        skip_cells: set[str],
    ) -> None: ...
    def scan(self, raw: bytes, is_custom: bool) -> dict[str, Any]:
        """One plugin's lint facts.

        Keys: ``events`` (record order: ``("header", [missing])`` or
        ``("cell", name, cell_id, fog_bug)``), ``evil_gmsts``, ``pathgrids``,
        ``masters``, ``tribunal``, ``bloodmoon``.
        """

# Whole-plugin merging: greatness7's merge_to_master, vendored (src/merge.rs).

def merge_plugins(
    plugin_path: str,
    master_path: str,
    *,
    remove_deleted: bool = False,
    apply_moved_references: bool = False,
    preserve_duplicate_references: bool = False,
) -> bytes:
    """The plugin merged into one of its masters, as plugin bytes."""

def merge_load_order(
    plugin_paths: list[str],
    *,
    remove_deleted: bool = False,
    apply_moved_references: bool = False,
    preserve_duplicate_references: bool = False,
) -> bytes:
    """A whole load order merged into one master, as plugin bytes."""
