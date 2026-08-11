"""Assemble the reference landmass and every plugin's difference from it.

This is the step that turns a load order into the thing a merge operates on.

**The reference landmass** is the terrain as the game ships it: the masters,
applied in order, before any mod. It is the common ancestor every mod's edit is
measured against, and having one is the entire reason disjoint edits can be
combined -- without it, two mods' cells can only be compared to each other, and
"both changed this cell" is all anyone can say.

**Each plugin's difference** is then a sparse set of moved vertices, computed
by :func:`~wraithguard.land.diff.diff_against_reference`. Two mods conflict only
where their sets intersect. Measured on this repository's sample, cell (0, 5)
is edited by four plugins: two of them moved 1,148 and 6 height vertices with
only **5 in common**, so 1,144 edits that a load order would discard survive a
merge intact.

**Reading order matters and is the caller's to supply.** Records must arrive in
load order, masters first. Land textures are resolved against the table as it
stands at each plugin's position (see :mod:`~wraithguard.land.textures`), and
getting that order wrong silently repaints terrain rather than raising.

**Nothing here writes.** This module reads records and reports. It is safe to
run against a real load order before any merging code exists, which is the
point of building it first.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Final

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

    from wraithguard.land.meta import PluginMeta

from wraithguard.land.diff import (
    ALL_LAYERS,
    LandData,
    LandscapeDiff,
    LandscapeLayers,
    diff_against_reference,
    is_deleted,
)
from wraithguard.land.textures import KnownTextures, translate_indices

_log: Final = logging.getLogger(__name__)

#: The record type carrying terrain.
LANDSCAPE_TYPE: Final = "Landscape"

#: Exterior cell coordinates.
Coords = tuple[int, int]


@dataclass(slots=True)
class PluginRecords:
    """One plugin's decoded records, as tes3conv writes them.

    Attributes:
        name: The plugin's file name, used in every report.
        records: Its records. Only ``Landscape`` and ``LandscapeTexture``
            entries are read; everything else is ignored, so a caller may pass
            a whole plugin.
    """

    name: str
    records: list[dict[str, object]]


@dataclass(slots=True)
class Landmass:
    """A set of landscape cells, keyed by exterior coordinates.

    Attributes:
        name: What this landmass represents, for reporting.
        cells: The terrain, one entry per cell.
        sources: Which plugin supplied each cell's current version.
    """

    name: str
    cells: dict[Coords, LandscapeLayers] = field(default_factory=dict)
    sources: dict[Coords, str] = field(default_factory=dict)

    def __len__(self) -> int:
        """How many cells the landmass covers."""
        return len(self.cells)

    def get(self, coords: Coords) -> LandscapeLayers | None:
        """The terrain at one cell.

        Args:
            coords: Exterior grid coordinates.

        Returns:
            The layers, or ``None`` when this landmass has no such cell.
        """
        return self.cells.get(coords)


def _landscape_records(records: Iterable[dict[str, object]]) -> list[dict[str, object]]:
    """Filter a record list down to landscapes.

    Args:
        records: Decoded records.

    Returns:
        Only the ``Landscape`` entries.
    """
    kept: list[dict[str, object]] = []
    for record in records:
        if record.get("type") != LANDSCAPE_TYPE:
            continue
        if is_deleted(record):
            # The plugin removed this cell's landscape. Its grids are stale, so
            # treating them as an edit would merge terrain the mod deleted.
            _log.debug("skipping a deleted landscape record")
            continue
        kept.append(record)
    return kept


def _decode_cell(
    plugin: str, record: dict[str, object], mapping: dict[int, int]
) -> LandscapeLayers | None:
    """Decode one landscape record and translate its texture indices.

    Args:
        plugin: The plugin the record came from.
        record: The record.
        mapping: The plugin's texture translation.

    Returns:
        The decoded layers, or ``None`` when the record could not be read.
    """
    try:
        layers = LandscapeLayers.from_record(record)
    except (ValueError, KeyError, TypeError) as exc:
        # One malformed cell must not abandon a two-hundred-plugin load order.
        # It is logged with its plugin so it can be chased, and skipped.
        _log.warning("%s: skipping an unreadable landscape record: %s", plugin, exc)
        return None

    if layers.textures is not None:
        result = translate_indices(layers.textures, mapping)
        if not result.is_complete:
            _log.warning(
                "%s: cell %s paints with %d texture index/indices no LTEX record "
                "defines (%s) -- left untranslated. A master is probably missing.",
                plugin,
                layers.coords,
                len(result.unknown),
                ", ".join(str(value) for value in sorted(result.unknown)[:8]),
            )
        layers.textures = result.values
    return layers


def merge_master_layers(existing: LandscapeLayers, incoming: LandscapeLayers) -> None:
    """Fold one master's version of a cell into an earlier master's.

    **Masters combine per layer, not per record.** A later master that
    redefines a cell with heights alone does not erase the earlier master's
    vertex colours and textures -- it replaces the layers it declares and
    leaves the rest standing. The declared flags are the *union* of both.

    Mirrors ``merge_tes3_landscape``. Replacing wholesale instead would drop
    real terrain data whenever two masters describe the same cell, which the
    vanilla three do not but ``Tamriel_Data.esm``, ``OAAB_Data.esm`` and other
    master-flagged expansions certainly do.

    A layer is only taken when the incoming record both *declares* it and
    *carries* it: an undeclared grid is zeros, and a declared-but-absent one is
    nothing to copy.

    Args:
        existing: The merged reference so far, modified in place.
        incoming: The later master's version.
    """
    for name, flag in (
        ("heights", LandData.VERTEX_HEIGHTS),
        ("normals", LandData.VERTEX_NORMALS),
        ("colors", LandData.VERTEX_COLORS),
        ("textures", LandData.TEXTURES),
        ("world_map", LandData.WORLD_MAP),
    ):
        if not incoming.declared & flag:
            continue
        value = getattr(incoming, name)
        if value is not None:
            setattr(existing, name, value)
            existing.declared |= flag


def _mask_layers(layers: LandscapeLayers, allowed: LandData) -> None:
    """Drop the layers a plugin's ``.mergedlands.toml`` excludes from a cell.

    A layer set ``included = false`` must not contribute -- for a master, that
    means it does not enter the reference. The excluded grids are cleared and
    the declared flags are narrowed to match, so the rest of the merge treats
    them exactly as it would a master that never carried them.

    Args:
        layers: The decoded cell, modified in place.
        allowed: The layers the plugin's settings permit.
    """
    for name, flag in (
        ("heights", LandData.VERTEX_HEIGHTS),
        ("normals", LandData.VERTEX_NORMALS),
        ("colors", LandData.VERTEX_COLORS),
        ("textures", LandData.TEXTURES),
        ("world_map", LandData.WORLD_MAP),
    ):
        if not (allowed & flag):
            setattr(layers, name, None)
            layers.declared = LandData(layers.declared & ~flag)


def build_reference(
    masters: Sequence[PluginRecords],
    textures: KnownTextures | None = None,
    metas: Mapping[str, PluginMeta] | None = None,
) -> tuple[Landmass, KnownTextures]:
    """Build the reference landmass from the master files.

    Masters are applied in order, and where two describe the same cell they
    combine **per layer** rather than wholesale -- see
    :func:`merge_master_layers`. The result is the terrain a player would see
    with every mod disabled.

    Args:
        masters: The masters, in load order.
        textures: An existing texture table to extend, or ``None`` to start
            one. Supplying one lets a caller share the table with
            :func:`plugin_differences`, which it must.
        metas: Per-plugin ``.mergedlands.toml`` settings. A master's settings
            apply here, to the reference, because a master is not diffed: a layer
            it excludes is dropped from its contribution, and a master marked as
            a previous merge is skipped entirely. ``None`` treats every master as
            fully included, which is the default with no sidecars.

    Returns:
        The reference landmass and the texture table built alongside it.
    """
    known = textures if textures is not None else KnownTextures()
    landmass = Landmass(name="reference")

    for master in masters:
        meta = metas.get(master.name) if metas else None
        if meta is not None and meta.is_previous_merge:
            _log.info("reference: skipping %s -- it is marked a previous merge", master.name)
            continue
        allowed = meta.allowed_layers() if meta is not None else ALL_LAYERS
        mapping = known.observe(master.name, master.records)
        for record in _landscape_records(master.records):
            layers = _decode_cell(master.name, record, mapping)
            if layers is None:
                continue
            if allowed != ALL_LAYERS:
                _mask_layers(layers, allowed)
            existing = landmass.cells.get(layers.coords)
            if existing is None:
                landmass.cells[layers.coords] = layers
            else:
                merge_master_layers(existing, layers)
            landmass.sources[layers.coords] = master.name

    _log.info(
        "reference landmass: %d cell(s) from %d master(s), %d land texture(s)",
        len(landmass),
        len(masters),
        len(known),
    )
    return landmass, known


def plugin_differences(
    reference: Landmass,
    plugin: PluginRecords,
    known: KnownTextures,
    allowed: LandData = ALL_LAYERS,
) -> list[LandscapeDiff]:
    """Compute what one plugin changed, cell by cell, against the reference.

    Args:
        reference: The reference landmass.
        plugin: The plugin's records.
        known: The shared texture table, already carrying the masters. It is
            updated with this plugin's textures.
        allowed: Which layers to consider.

    Returns:
        One entry per cell this plugin actually changed. Cells it rewrote
        without altering are omitted, which is the common case for mods that
        touch a cell only to add an object.
    """
    mapping = known.observe(plugin.name, plugin.records)
    changes: list[LandscapeDiff] = []

    for record in _landscape_records(plugin.records):
        layers = _decode_cell(plugin.name, record, mapping)
        if layers is None:
            continue
        difference = diff_against_reference(
            plugin.name, layers, reference.get(layers.coords), allowed
        )
        if difference.is_modified or difference.missing:
            changes.append(difference)

    return changes


@dataclass(slots=True)
class CellContention:
    """Which plugins changed one cell, and how much they disagree.

    Attributes:
        coords: The cell.
        changes: Every plugin's difference for it, in load order.
    """

    coords: Coords
    changes: list[LandscapeDiff] = field(default_factory=list)

    @property
    def plugins(self) -> list[str]:
        """The plugins that changed this cell, in load order."""
        return [change.plugin for change in self.changes]

    @property
    def is_contested(self) -> bool:
        """Whether more than one plugin changed it."""
        return len(self.changes) > 1

    @property
    def is_new_land(self) -> bool:
        """Whether the masters had no terrain here.

        Worth separating in any report. On a large collection most changed
        cells are new land -- 1,033 plugins in this repository's dump produce
        13,819 changed cells against a 1,540-cell vanilla reference, so roughly
        nine in ten are terrain the masters never had. Counting those as
        "recovered by merging" would make the case for merging look enormous
        for the wrong reason.
        """
        return any(change.new_land for change in self.changes)

    def height_overlap(self) -> tuple[int, int]:
        """How much the plugins' height edits collide.

        Returns:
            A pair of ``(contested, mergeable)`` vertex counts. ``contested``
            is the number of vertices more than one plugin moved; ``mergeable``
            is the number exactly one plugin moved, which a merge keeps in full
            and a load order throws away.
        """
        seen: set[tuple[int, int]] = set()
        twice: set[tuple[int, int]] = set()
        for change in self.changes:
            if change.heights is None:
                continue
            for vertex in change.heights.changed_vertices():
                if vertex in seen:
                    twice.add(vertex)
                else:
                    seen.add(vertex)
        return len(twice), len(seen) - len(twice)


def survey(
    reference: Landmass,
    plugins: Sequence[PluginRecords],
    known: KnownTextures,
    allowed: LandData = ALL_LAYERS,
) -> dict[Coords, CellContention]:
    """Compute every plugin's differences and group them by cell.

    This is the read-only half of merging: it establishes exactly which cells
    are contested and by how much, without producing a file. Running it against
    a real load order is the cheapest way to find out whether a merge is worth
    doing before any merging code exists.

    Args:
        reference: The reference landmass.
        plugins: The mods, in load order, after the masters.
        known: The shared texture table from :func:`build_reference`.
        allowed: Which layers to consider.

    Returns:
        One entry per changed cell, keyed by coordinates.
    """
    contention: dict[Coords, CellContention] = {}

    for plugin in plugins:
        for change in plugin_differences(reference, plugin, known, allowed):
            cell = contention.get(change.coords)
            if cell is None:
                cell = CellContention(coords=change.coords)
                contention[change.coords] = cell
            cell.changes.append(change)

    contested = sum(1 for cell in contention.values() if cell.is_contested)
    _log.info(
        "surveyed %d plugin(s): %d changed cell(s), %d contested by more than one",
        len(plugins),
        len(contention),
        contested,
    )
    return contention
