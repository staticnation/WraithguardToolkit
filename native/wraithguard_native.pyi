"""Type stubs for wraithguard_native (native/src)."""

from collections.abc import Callable, Mapping, Sequence
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

def conflict_scan(
    order: Sequence[str], paths: Sequence[str | None], subset: Sequence[str], mode: str
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """The record scan with the native session: ``mode`` ``"conflicts"``, ``"mine"`` or ``"other"``."""

def conflict_report(
    conflicts: Sequence[Mapping[str, Any]], stats: Mapping[str, Any], subset_only: bool, limit: int
) -> str:
    """``format_conflict_report``'s text (English)."""

def conflict_csv(conflicts: Sequence[Mapping[str, Any]]) -> str:
    """The text ``write_conflict_csv`` writes."""

def resource_scan(
    data_dirs: Sequence[str], subset_dirs: Sequence[str], exclude_exts: Sequence[str], compare: bool
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    """``detect_resource_conflicts``'s loose-file conflicts and counts."""

def resource_report(
    conflicts: Sequence[Mapping[str, Any]],
    stats: Mapping[str, Any],
    notes: Sequence[str],
    subset_only: bool,
    limit: int,
) -> str:
    """``format_resource_report``'s text (English); ``notes`` the mesh line of each conflict."""

def resource_csv(
    conflicts: Sequence[Mapping[str, Any]], columns: Sequence[tuple[str, str, str, str]]
) -> str:
    """The text ``write_resource_csv`` writes; ``columns`` the mesh cells of each conflict."""

def mesh_analyse(
    conflicts: Sequence[Mapping[str, Any]], limit: int
) -> tuple[list[dict[str, Any] | None], dict[str, int]]:
    """``analyse_mesh_conflicts``'s finding for each conflict (None: not looked at), and counts."""

def lint_order(
    order: Sequence[str],
    paths: Sequence[str | None],
    subset: Sequence[str],
    origins: Mapping[str, str],
) -> tuple[list[str], dict[str, int]]:
    """``lint_plugins``, whole: the warnings and the counts."""

def backup_scan(roots: Sequence[str], max_depth: int) -> list[tuple[str, str, str]]:
    """``scan_backups``: ``(backup, original, kind)`` for each, by path without case."""

def master_remove(data: bytes, master: str) -> tuple[bytes, dict[str, Any]]:
    """``remove_master_from_bytes``: the new bytes and the report's fields."""

def master_rename(data: bytes, old: str, new: str, new_size: int) -> tuple[bytes, dict[str, Any]]:
    """``rename_master_in_bytes``; ValueError when ``new`` is already another master."""

def save_content_files(data: bytes) -> list[str]:
    """The content files a save names; ValueError (why) when they cannot be read."""

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

# The load order, read by ordercore (native/src/order.rs).
def cell_key(spec: str) -> dict[str, Any] | None:
    """A cell named the one way: ``fold``, ``spec``, ``pool`` and ``grid``; None for no name."""

def ref_winner(deleted: Sequence[bool]) -> tuple[int, bool, list[int]] | None:
    """Which of a reference's versions stands: its position, deleted, the ignored ones."""

# The load order's rules, sort, checks, export, rule maker and updates, ordercore's
# (native/src/loadorder.rs, port step 4.2 slice 2j). Each raises OrderUnsupportedError where
# ordercore declines; wraithguard/ordercore_bridge.py then keeps the Python answer.
class OrderUnsupportedError(Exception):
    """ordercore does not read this input; the caller keeps the Python answer."""

def order_parse_rules(text: str) -> tuple[list[tuple[str, list[str]]], int]:
    """The ordering blocks of a rule file's text and how many lines were bridged over."""

def order_build_and_sort(
    base: Sequence[str],
    subset: Sequence[str],
    blocks: Sequence[tuple[Sequence[str], int]],
    masters: Mapping[str, Sequence[str]],
    nearstart: Sequence[str],
    nearend: Sequence[str],
) -> tuple[list[str], dict[str, tuple[str, str | None]], list[tuple[str, str]]]:
    """The sort: the order, each custom's anchor, the refused rule edges."""

def order_check_predicates(
    rules_text: str, final_order: Sequence[str], subset_origins: Mapping[str, str], plugins: object
) -> list[str]:
    """The predicate rules' warnings; ``plugins`` answers usable/exists/size/description."""

def order_momw_parse(data: bytes) -> list[Any]:
    """plugin-order.yml's entries; ValueError, saying where, for a file that is not MOMW's."""

def order_momw_curated_for_list(
    entries: Sequence[Any], list_name: str
) -> tuple[set[str], list[str]]:
    """A curated list's plugins: the lower-cased set and the order."""

def order_momw_needs_cleaning(entries: Sequence[Any]) -> set[str]:
    """The lower-cased plugins flagged for cleaning."""

def order_momw_post_sort_findings(
    final_order: Sequence[str] | None,
    base: Sequence[str],
    entries: Sequence[Any],
    curated_set: set[str],
    curated_order: Sequence[str],
    declared_lower: set[str],
    list_name: str | None,
) -> list[str]:
    """The notes for a finished sort."""

def order_render_rule(rule: Mapping[str, Any]) -> str:
    """A rule (its plain form) as rule-file text."""

def order_validate_rule(rule: Mapping[str, Any]) -> list[tuple[str, str, str]]:
    """A rule's problems as (severity, message, remedy)."""

def order_format_ref(ref: str) -> str:
    """A citation as a rule's message carries it."""

def order_append_authored_rule(path: str, rule: Mapping[str, Any]) -> str:
    """Write a rule to the user's rule file; ValueError listing its errors."""

def order_append_user_rule(
    path: str, keyword: str, names: Sequence[str], comment: str | None = None
) -> str:
    """Write an ordering rule to the user's rule file."""

def order_rule_frozen_conflicts(
    names: Sequence[str], final_order: Sequence[str], curated_lower: Sequence[str]
) -> list[tuple[str, str]]:
    """The pairs a proposed [Order] would fight the cfg over."""

def order_derive(
    which: str,
    masters_by_plugin: Sequence[tuple[str, Sequence[str]]],
    active: Sequence[str],
    conflicts: Sequence[Mapping[str, Any]],
    minimum_records: int,
) -> list[dict[str, Any]]:
    """Rule proposals from masters and conflicts (rules in their plain form)."""

def order_insert_data_paths(
    data_lines: Sequence[str], data_inserts: Sequence[Mapping[str, Any]]
) -> tuple[list[tuple[str, bool, str | None]], list[str]]:
    """The data= lines with the inserts placed, and the notes."""

def order_customizations_toml(args: Mapping[str, Any]) -> tuple[str, list[str]]:
    """The customizations TOML and its warnings (arguments by name)."""

def order_plugins_needing_removal(
    disabled: Sequence[str], curated_set: Sequence[str], base_order_names: Sequence[str]
) -> list[str]:
    """The opted-out plugins that need a removeContent."""

def order_data_paths_needing_removal(
    disabled_data: Sequence[str], our_values: set[str], base_data_norms: set[str]
) -> list[str]:
    """The opted-out data paths that need a removeData."""

def order_read_groundcover_names(lines: Sequence[str]) -> list[str]:
    """The plugins on a cfg's groundcover= lines."""

def order_splice_cfg_lines(
    lines: Sequence[str], segments: Sequence[tuple[Sequence[int], Sequence[str]]]
) -> list[str]:
    """The cfg lines with the content=/data= segments spliced in."""

def order_fetch_url_bytes(
    get: Callable[[str, int, int], bytes], url: str, timeout: int, max_bytes: int
) -> bytes:
    """A checked download through ``get``; ValueError refused, OSError failed."""

def order_update_plugin_order_yml(
    path: str,
    urls: Sequence[str] | None,
    timeout: int,
    count: Callable[[bytes], int],
    env: Mapping[str, Any],
    get: Callable[[str, int, int], bytes],
) -> list[str]:
    """Update plugin-order.yml; the report lines."""

def order_fetch_list_data_paths(
    path: str,
    list_name: str,
    host: str | None,
    timeout: int,
    env: Mapping[str, Any],
    get: Callable[[str, int, int], bytes],
) -> list[str]:
    """Cache a curated list's data paths; the report lines."""

def order_update_rule_files(
    rule_paths: Sequence[str],
    url_template: str | None,
    timeout: int,
    env: Mapping[str, Any],
    get: Callable[[str, int, int], bytes],
) -> list[str]:
    """Update the managed rule files; the report lines."""

def order_rule_file_ages(rule_paths: Sequence[str], now: float) -> list[tuple[str, int | None]]:
    """Each rule file's age in days, or None."""

# The sort's inputs (port step 4.2 slice 2k), ordercore::inputs.
def order_subset_from_lines(lines: Sequence[str]) -> tuple[list[str], list[str], list[str]]:
    """Subset lines as (plugins, data folders, skipped entries)."""

def order_groundcover_declarations(lines: Sequence[str]) -> tuple[list[str], list[str]]:
    """The plugins subset lines declare as grass, and the declared entries that are not plugins."""

def order_subset_from_toml(text: str) -> tuple[list[str], list[str], list[str], list[str]]:
    """The subset file's TOML form: (plugins, data folders, skipped, groundcover)."""

def order_customizations(
    text: str,
) -> tuple[list[str], list[tuple[str, str | None, str | None]], list[str], dict[str, str]]:
    """A customizations.toml: (subset, [(value, after, before)], replace dests, listNames)."""

def order_hold_back_groundcover(
    subset: Sequence[str], groundcover: Sequence[str]
) -> tuple[list[str], list[str]]:
    """The subset without grass, and the grass held back."""

def order_all_scan_dirs(
    data_order: Sequence[str], raw_values: Sequence[str], insert_values: Sequence[str]
) -> list[str]:
    """The cfg's data folders and the pending ones not among them."""

def order_pending_custom_dirs(raw_values: Sequence[str], insert_values: Sequence[str]) -> list[str]:
    """The pending data folders, once each."""

def order_scan_mod_directories(start: str) -> tuple[list[str], int, int, str]:
    """The mods-folder scan: (lines, folders, plugins, start made absolute)."""

def order_parse_data_paths_cache(text: str) -> list[str]:
    """A list's cached data-path tails."""

def order_managed_data_path_norms(tails: Sequence[str], cfg_values: Sequence[str]) -> set[str]:
    """The normalized cfg data paths the list's tails account for."""

def order_reconcile_data_paths(
    tails: Sequence[str], cfg_values: Sequence[str], report_missing: bool
) -> list[str]:
    """The list's data paths against the cfg's: the findings."""

def order_cfg_orphans(
    subset: Sequence[str],
    base_order: Sequence[str],
    data_order: Sequence[str],
    curated: set[str],
    needs_cleaning: set[str],
    declared: set[str],
    groundcover: set[str],
    declared_data: Sequence[str],
    tails: Sequence[str] | None,
) -> tuple[list[str], list[str] | None]:
    """The --subset-from-cfg pull: (new plugins, orphan data values or None)."""

def order_infer_data_path_anchors(
    inserts: Sequence[Mapping[str, object]],
    data_order: Sequence[str],
    final_order: Sequence[str],
    base_dir: str | None,
) -> tuple[list[tuple[str | None, str | None]], list[tuple[str, str, str, str]]]:
    """Each insert's (after, before), and (value, mode, anchor, via) per anchor chosen."""

def order_check_missing_masters(
    active_order: Sequence[str], dirs: Sequence[str], origins: Mapping[str, str]
) -> tuple[list[str], list[str], list[str], int, list[str]]:
    """The header-master check: (missing, order problems, size notes, checked, problem plugins)."""

# The Editor's record cache, editcore's (native/src/cache.rs).
def cache_dumps(value: object) -> bytes:
    """A value as the tagged JSON the record cache stores; TypeError for one it does not keep."""

def cache_loads(data: bytes) -> object:
    """The value ``cache_dumps`` stored."""

class RecordCacheDb:
    """The record cache's SQLite file (editcore::cache); plugin lists answered by position."""

    def __init__(
        self, path: str | PathLike[str], parser: str = "", max_bytes: int = 1 << 30
    ) -> None: ...
    def get(self, plugin: str | PathLike[str], tag: str) -> bytes | None: ...
    def put(
        self,
        plugin: str | PathLike[str],
        tag: str,
        data: bytes,
        keys: Sequence[str] | None = None,
    ) -> None: ...
    def put_keys(self, plugin: str | PathLike[str], tag: str, keys: Sequence[str]) -> None: ...
    def stale_of(self, plugins: Sequence[str | PathLike[str]], tag: str) -> list[int] | None: ...
    def keys_current(self, plugin: str | PathLike[str], tag: str) -> bool: ...
    def missing_keys(self, plugins: Sequence[str | PathLike[str]], tag: str) -> list[int]: ...
    def holders(
        self, plugins: Sequence[str | PathLike[str]], tag: str, key: str
    ) -> list[int] | None: ...
    def all_keys(self, plugins: Sequence[str | PathLike[str]], tag: str) -> set[str] | None: ...
    def put_uses(
        self, plugin: str | PathLike[str], rows: Sequence[tuple[str, str, str, str, int]]
    ) -> None: ...
    def missing_uses(self, plugins: Sequence[str | PathLike[str]]) -> list[int]: ...
    def uses_for(
        self, plugins: Sequence[str | PathLike[str]], value: str
    ) -> list[tuple[int, str, str, list[str], int]] | None: ...
    def definers(
        self, plugins: Sequence[str | PathLike[str]], records: Sequence[tuple[str, str]]
    ) -> dict[tuple[str, str], list[int]] | None: ...
    @staticmethod
    def load_order_signature(plugins: Sequence[str | PathLike[str]]) -> str | None: ...
    def get_report(self, sig: str, kind: str, key: str) -> bytes | None: ...
    def put_report(self, sig: str, kind: str, key: str, data: bytes) -> None: ...
    def clear(self) -> None: ...
    def stats(self) -> dict[str, int]: ...
    def compact(self) -> bool: ...
    def size_bytes(self) -> int: ...
    def close(self) -> None: ...

def order_compute_plan(
    args: Mapping[str, object],
) -> tuple[str | None, list[tuple[str, str, str | None, list[str]]], dict[str, Any] | None]:
    """A whole sort: (refused message, report items, plan)."""

def order_write_plan(
    args: Mapping[str, object],
    plan: Mapping[str, object],
    counts: tuple[int, int],
    final_order: Sequence[str] | None,
    data_order: Sequence[str] | None,
    disabled_plugins: Sequence[str],
    disabled_data: Sequence[str],
) -> tuple[list[tuple[str, str, str | None, list[str]]], bool, bool, str | None, list[str], list[str]]:
    """write_plan: (report items, wrote_cfg, wrote_toml, toml text, cfg lines, user data values)."""

def order_preview_configurator(
    plan_lines: Sequence[str],
    toml_text: str,
    expected: Sequence[str],
    subset: Sequence[str],
    user_data_norms: Sequence[str],
    list_name: str | None,
) -> tuple[bool, list[str]]:
    """The configurator preview: (ok, report lines)."""

def order_render_cfg_lines(
    plan: Mapping[str, object], final_order: Sequence[str] | None, data_order: Sequence[str] | None
) -> list[str]:
    """The cfg a plan would write."""
