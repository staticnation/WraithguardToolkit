"""Resolve land-texture indices into one shared space across a load order.

**Why nothing about textures can be compared without this.** A ``LAND`` record's
``VTEX`` grid holds sixteen-bit *indices*, not texture names, and each index is
resolved through the load order's table of ``LTEX`` records. Every plugin
numbers its own textures from zero. Measured on real mods in this repository's
sample: ``Arvesa - An Armigers Tale`` calls index 0 ``RM_rock_01``,
``AscadianFarmhouses`` calls it ``Rock_Coastal``, and ``BCOM - Taller
Lighthouse`` calls it ``Tx_BC_dirt.tga``. Three mods, one number, three
unrelated textures.

So comparing two plugins' ``VTEX`` grids directly compares numbers that do not
mean the same thing. A merge built on that would produce terrain painted with
whatever texture happened to occupy the slot -- grass where there should be
lava -- and it would look like a plausible merge, not like a crash.

**What this module does.** It walks the load order, keeps the ``LTEX`` table
the way the engine does (later plugins overwrite an index), assigns each
distinct texture *identity* a stable index of its own, and hands back a
translation from each plugin's numbering into that shared space. After
translation, two plugins that painted the same texture hold the same number and
a diff means what it appears to mean.

**Two numberings, one off-by-one.** An ``LTEX`` record carries a zero-based
``index``. A ``VTEX`` cell stores that index *plus one*, reserving zero for "no
texture". Conflating them shifts an entire cell's painting by one texture,
which is why the conversion is named in both directions here rather than
written inline at each use.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Final

from wraithguard.land.diff import is_deleted

_log: Final = logging.getLogger(__name__)

#: The ``VTEX`` value meaning "nothing painted here". Never remapped.
NO_TEXTURE: Final = 0


def vtex_of(ltex_index: int) -> int:
    """Convert an ``LTEX`` record index to the value stored in ``VTEX``.

    Args:
        ltex_index: The zero-based index on the ``LTEX`` record.

    Returns:
        The ``VTEX`` cell value.
    """
    return ltex_index + 1


def ltex_of(vtex_value: int) -> int | None:
    """Convert a ``VTEX`` cell value back to an ``LTEX`` record index.

    Args:
        vtex_value: The value stored in the grid.

    Returns:
        The zero-based ``LTEX`` index, or ``None`` for "no texture", which has
        no corresponding record.
    """
    if vtex_value == NO_TEXTURE:
        return None
    return vtex_value - 1


@dataclass(slots=True)
class KnownTexture:
    """One distinct land texture, and where its current definition came from.

    Attributes:
        identifier: The ``LTEX`` record's ``id``, which is what makes two
            plugins' textures *the same* texture.
        index: The index assigned in the shared space.
        file_name: The texture file, from the last plugin to set one.
        source: The plugin that supplied that file name.
    """

    identifier: str
    index: int
    file_name: str | None = None
    source: str = ""


class KnownTextures:
    """The shared land-texture table, built by walking a load order.

    Textures are identified by their ``LTEX`` ``id`` rather than by index,
    because the id is the only part that survives being loaded next to another
    mod. Indices are assigned here in first-seen order and never reused.
    """

    __slots__ = ("_by_id", "_table")

    def __init__(self) -> None:
        """Start with an empty table."""
        self._by_id: dict[str, KnownTexture] = {}
        # The engine's own view: which id each index currently resolves to.
        # A later plugin defining an index that a master already used replaces
        # it, exactly as the game does, and that is precisely the collision
        # this whole module exists to survive.
        self._table: dict[int, str] = {}

    def __len__(self) -> int:
        """How many distinct textures the load order defines."""
        return len(self._by_id)

    def sorted(self) -> list[KnownTexture]:
        """Every known texture, in shared-index order.

        Returns:
            The textures, ready to emit as ``LTEX`` records.
        """
        return sorted(self._by_id.values(), key=lambda texture: texture.index)

    def get(self, identifier: str) -> KnownTexture | None:
        """Look up a texture by its identifier.

        Args:
            identifier: The ``LTEX`` ``id``.

        Returns:
            The texture, or ``None`` when the load order never defined it.
        """
        return self._by_id.get(identifier)

    def observe(self, plugin: str, records: list[dict[str, object]]) -> dict[int, int]:
        """Register one plugin's ``LTEX`` records and build its translation.

        Call once per plugin, **in load order**. The returned mapping converts
        that plugin's ``VTEX`` values into shared ones and is only valid for
        landscape records from the same plugin.

        Args:
            plugin: The plugin's name, recorded as the source of any file name
                it supplies.
            records: The plugin's decoded records. Entries that are not
                ``LandscapeTexture`` are ignored, so a caller may pass a whole
                plugin without filtering it first.

        Returns:
            A mapping from this plugin's ``VTEX`` values to shared ``VTEX``
            values. :data:`NO_TEXTURE` always maps to itself.
        """
        for record in records:
            if record.get("type") != "LandscapeTexture":
                continue
            identifier = record.get("id")
            index = record.get("index")
            if not isinstance(identifier, str) or not isinstance(index, int):
                # A texture with no id cannot be matched against another
                # plugin's, and one with no index cannot be referenced by a
                # grid. Either way there is nothing to merge, so skip it
                # rather than invent a placeholder that would collide.
                continue

            known = self._by_id.get(identifier)
            if known is None:
                if is_deleted(record):
                    # A texture this plugin deleted must not enter the shared
                    # table: emitting an LTEX for it would reinstate something
                    # the mod removed, and it would occupy an index. Merged
                    # Lands asserts here; see diff.is_deleted for why we do not.
                    _log.debug("%s: skipping deleted land texture %s", plugin, identifier)
                    continue
                known = KnownTexture(identifier=identifier, index=len(self._by_id))
                self._by_id[identifier] = known

            file_name = record.get("file_name")
            if isinstance(file_name, str) and file_name != known.file_name:
                known.file_name = file_name
                known.source = plugin

            self._table[index] = identifier

        return self.translation()

    def translation(self) -> dict[int, int]:
        """The current ``VTEX`` translation, given everything observed so far.

        Returns:
            A mapping from local ``VTEX`` values to shared ones, including the
            identity entry for :data:`NO_TEXTURE`.
        """
        mapping = {NO_TEXTURE: NO_TEXTURE}
        for index, identifier in self._table.items():
            known = self._by_id.get(identifier)
            if known is not None:
                mapping[vtex_of(index)] = vtex_of(known.index)
        return mapping


@dataclass(slots=True)
class TranslationResult:
    """A translated grid, plus what could not be translated.

    Attributes:
        values: The grid in shared indices.
        unknown: Local values with no known texture behind them, and how often
            each appeared.
    """

    values: list[int]
    unknown: dict[int, int] = field(default_factory=dict)

    @property
    def is_complete(self) -> bool:
        """Whether every value resolved."""
        return not self.unknown


def fallback_texture_index(mapping: dict[int, int]) -> int:
    """The smallest real texture index in a compaction mapping.

    Ports Merged Lands' ``RemappedTextures::fallback_texture_index``. When a
    merged cell paints with an index no ``LTEX`` record defines, the fork
    substitutes this -- the lowest valid painted texture -- so the plugin never
    carries a dangling index. :data:`NO_TEXTURE` is excluded because it is not a
    texture; if nothing else is mapped, :data:`NO_TEXTURE` is the only answer.

    Args:
        mapping: A compaction mapping from :func:`compact_textures`.

    Returns:
        The smallest mapped value other than :data:`NO_TEXTURE`, or
        :data:`NO_TEXTURE` when there is none.
    """
    real = [value for value in mapping.values() if value != NO_TEXTURE]
    return min(real) if real else NO_TEXTURE


def compact_textures(
    known: KnownTextures, used: set[int], *, substitute_unknown: bool = True
) -> tuple[dict[int, int], list[KnownTexture], list[int]]:
    """Renumber the shared table down to the textures a merge actually paints.

    Merged Lands does this in ``KnownTextures::remove_unused``, and skipping it
    is not merely wasteful. The shared table accumulates every land texture
    every surveyed plugin declares -- 141 across this repository's vanilla
    masters alone, far more across a full load order -- while a merged plugin
    typically paints with a fraction of them. Emitting the whole table ships
    ``LTEX`` records for textures nothing references, and every one of those
    occupies an index the game must resolve.

    So after merging, the ``VTEX`` values the merged cells actually contain are
    collected and the table is compacted: only those textures survive, renumbered
    contiguously from zero.

    :data:`NO_TEXTURE` is always retained. It is not a texture, but it is a
    value merged grids contain, and dropping it from the mapping would leave
    unpainted terrain untranslatable.

    Args:
        known: The shared table, as built by walking the load order.
        used: Every ``VTEX`` value appearing in the merged cells.
        substitute_unknown: What to do with a value no ``LTEX`` record defines
            (a missing master, already reported cell-by-cell during the diff).
            The default (``True``) maps it to :func:`fallback_texture_index`
            (the smallest valid painted texture), so the written plugin always
            loads rather than carrying a dangling index -- the safe default for
            a GUI-driven run, reported at emit as a fallback. ``False`` leaves
            it out of the mapping, so the emit passes it through unchanged: the
            index dangles, but nothing is silently repainted. Either way the
            caller is told; the choice is which risk to take, and a CLI run that
            wants the second can pass it.

    Returns:
        A triple ``(mapping, kept, unresolved)``: the shared-to-compacted
        ``VTEX`` mapping, the textures to emit in their new index order, and the
        sorted values no ``LTEX`` defined. With ``substitute_unknown`` the
        mapping also carries every ``unresolved`` value, pointing at the
        fallback; without it they are absent and pass through. ``unresolved`` is
        returned either way so the emit can report what it did.
    """
    by_index = {vtex_of(texture.index): texture for texture in known.sorted()}
    mapping = {NO_TEXTURE: NO_TEXTURE}
    kept: list[KnownTexture] = []
    unresolved: list[int] = []

    for value in sorted(used):
        if value == NO_TEXTURE:
            continue
        texture = by_index.get(value)
        if texture is None:
            # A merged grid references a texture the table never learned. That
            # is a missing master, already reported by translate_indices.
            unresolved.append(value)
            continue
        kept.append(
            KnownTexture(
                identifier=texture.identifier,
                index=len(kept),
                file_name=texture.file_name,
                source=texture.source,
            )
        )
        mapping[value] = vtex_of(len(kept) - 1)

    if substitute_unknown and unresolved:
        # Point every unresolved value at the smallest valid painted texture,
        # so the emit substitutes it rather than writing a dangling index.
        fallback = fallback_texture_index(mapping)
        for value in unresolved:
            mapping[value] = fallback

    return mapping, kept, unresolved


def translate_indices(values: list[int], mapping: dict[int, int]) -> TranslationResult:
    """Rewrite a ``VTEX`` grid from one plugin's numbering into the shared one.

    A value with no entry in the mapping is left as it is and reported. That
    happens when a plugin paints with an index no ``LTEX`` record defines --
    usually a mod whose master is missing from the load order. Substituting
    zero would silently repaint that terrain, and dropping the cell would lose
    the mod's other edits, so it is passed through and named.

    Args:
        values: The grid, flat.
        mapping: A translation from :meth:`KnownTextures.observe`.

    Returns:
        The translated grid and any unresolved values.
    """
    translated: list[int] = []
    unknown: dict[int, int] = {}
    for value in values:
        shared = mapping.get(value)
        if shared is None:
            unknown[value] = unknown.get(value, 0) + 1
            translated.append(value)
        else:
            translated.append(shared)
    return TranslationResult(values=translated, unknown=unknown)
