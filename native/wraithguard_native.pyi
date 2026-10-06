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

# -- OpenMW Lua (native/src/lua) -------------------------------------------------------

TEAL_VERSION: str

def lua_api(resources: str | PathLike[str]) -> dict[str, Any] | None:
    """An install's Lua API documentation, read from its ``resources`` folder.

    ``{"version", "modules", "members"}``: each module's ``name``, ``require``,
    ``interface``, ``contexts``, ``file``, ``fields``, ``functions`` and ``types``, and
    each declared type's member names by its Teal name. None when the folder has no
    ``lua_api/openmw``.
    """

def lua_write_declarations(
    out: str | PathLike[str],
    resources: str | PathLike[str] | None = None,
    packages: Sequence[str] = (),
) -> dict[str, Any]:
    """Teal declarations written into ``out`` (emptied first).

    From the install's documentation, or stubs for ``packages`` without one. Returns
    ``{"source": "openmw" | "stubs", "version", "packages"}``.
    """

def lua_check(
    paths: Sequence[str | PathLike[str]],
    include: Sequence[str | PathLike[str]],
    globals: Sequence[str] = (),  # noqa: A002 - the backend's keyword
) -> list[list[dict[str, Any]] | str]:
    """Each file checked by the Teal compiler (tl 0.24.8, embedded).

    ``include``: module folders, lowest priority first. Per path, its diagnostics
    (``line``, ``col``, ``severity``, ``kind``, ``rule``, ``message``) or why it could
    not be checked.
    """

def lual_scripts(path: str | PathLike[str]) -> list[tuple[str, int, list[str], bool]]: ...
def lual_remap(record_json: str, mapping: dict[int, int]) -> str: ...
def lua_tokenize(
    src: str, comments: bool = False, teal: bool = False
) -> list[tuple[str, str, int, int, int, str]]: ...
def lua_parse(src: str, teal: bool = False) -> tuple[Any, ...]: ...
def lua_analyze(
    src: str,
    contexts: Sequence[str],
    handlers: dict[str, Sequence[str]],
    per_frame: Sequence[str],
    packages: dict[str, Sequence[str]],
) -> dict[str, Any]: ...
def lua_omwscripts(
    text: str, name: str
) -> tuple[str, list[tuple[str, list[str], str, int]], list[tuple[int, str]]]: ...
def lua_scan(
    data_dirs: Sequence[str | PathLike[str]],
    content: Sequence[str],
    handlers: dict[str, Sequence[str]],
    per_frame: Sequence[str],
    packages: dict[str, Sequence[str]],
    builtin_interfaces: Sequence[str],
    builtin_events: Sequence[str],
) -> dict[str, Any]:
    """A load order's scripts found, read and checked (viewer-shell/luacore/src/scan.rs)."""

def lua_findings_for(
    diags: Sequence[tuple[str, str, int, str]],
    teal: bool,
    bound: Sequence[str],
    members: dict[str, list[str]],
    stubs: bool,
) -> list[tuple[str, str, int, str]]: ...
def lua_check_findings(
    jobs: Sequence[tuple[str | PathLike[str], bool, str | PathLike[str]]],
    include: Sequence[str | PathLike[str]],
    globals: Sequence[str],  # noqa: A002 - the backend's keyword
    members: dict[str, list[str]],
    stubs: bool,
) -> list[list[tuple[str, str, int, str]] | str]: ...
def lua_tlconfig_dirs(data_dirs: Sequence[str | PathLike[str]]) -> list[str]: ...
def lua_tlconfig_globals(data_dirs: Sequence[str | PathLike[str]]) -> list[str]: ...
def lua_find_teal_declarations(near: Sequence[str | PathLike[str]]) -> str | None: ...
def lua_find_resources(
    cfg: str | PathLike[str] | None = None, explicit: str | PathLike[str] | None = None
) -> str | None: ...
def lua_flowchart(node: tuple[Any, ...]) -> str: ...
def lua_call_graph_chart(chunk: tuple[Any, ...]) -> str: ...
def lua_mermaid(
    nodes: Sequence[tuple[str, str, str]], edges: Sequence[tuple[str, str, str | None]]
) -> str: ...
def lua_report(
    header: tuple[str, int, int, int, int, str, int, int],
    found: Sequence[tuple[str, str, str, int, str]],
    scripts: Sequence[tuple[str, list[str], str | None, list[str], bool]],
    info: bool = True,
) -> str: ...
def walk_files(
    roots: Sequence[str | PathLike[str]], skip_exts: Sequence[str] | None = None, lower: bool = True
) -> list[list[str] | None]:
    """Every file under each root, relative with ``/``; None for a root that is no folder."""

def files_identical(paths: Sequence[str | PathLike[str]]) -> bool:
    """Whether every file holds the same bytes (False when one cannot be read)."""

def files_identical_many(groups: Sequence[Sequence[str | PathLike[str]]]) -> list[bool]:
    """:func:`files_identical` for each group, compared in parallel."""

def file_digest(path: str | PathLike[str]) -> str:
    """A file's BLAKE2b-128 as hex (``hashlib.blake2b(digest_size=16)``), ``""`` if unreadable."""

def file_digests(paths: Sequence[str | PathLike[str]]) -> list[str]:
    """:func:`file_digest` for each, in parallel."""

def scan_mod_folders(
    start: str | PathLike[str], asset_dirs: Sequence[str], plugin_exts: Sequence[str]
) -> list[tuple[str, list[str]]]:
    """Folders under ``start`` holding an asset folder or a plugin (not looked into further)."""

def bsa_extract(
    archive: str | PathLike[str],
    dest: str | PathLike[str],
    prefix: str = "",
    suffixes: Sequence[str] | None = None,
) -> int:
    """Write the archive's files under ``prefix`` (ending in a suffix) beneath ``dest``."""
