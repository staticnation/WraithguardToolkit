r"""Per-plugin merge settings read from ``.mergedlands.toml`` sidecars.

A plugin may ship a file beside it controlling how its landscape edits are
merged::

    Data Files/
        Cantons_on_the_Global_Map_v1.1.esp
        Cantons_on_the_Global_Map_v1.1.mergedlands.toml

Two knobs per layer. ``included = false`` drops that plugin's edits to the
layer entirely; ``conflict_strategy`` decides what happens where its edits
collide with another mod's.

**Why this exists rather than a global switch.** Conflicts are between specific
pairs of mods, and the right answer differs per pair. The documented example:
*Beautiful Cities of Morrowind Suran Expansion* loads after *BCoM* and edits the
same land, and its edits should win -- so it sets every layer to ``Overwrite``.
A global "later wins" would apply that everywhere and throw away merges that
were working.

Example, dropping everything but the world map and forcing that to win::

    version = "0"
    meta_type = "Patch"

    [height_map]
    included = false

    [vertex_colors]
    included = false

    [texture_indices]
    included = false

    [world_map_data]
    conflict_strategy = "Overwrite"

**Defaults are permissive on purpose.** Every layer is ``included = true`` with
``conflict_strategy = "Auto"``, so a plugin with no sidecar behaves exactly as
if this module did not exist. Merged Lands' own advice is not to write one until
it is known to be necessary, and nothing here encourages otherwise.

**One layer name covers two.** ``height_map`` governs vertex normals as well,
because the record stores them under a single flag and a merge that moved a
height without its normal would light the old terrain.

Ported from ``io/meta_schema.rs`` in Merged Lands (MIT, David Von Derau).
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Final

from wraithguard.land.diff import LandData
from wraithguard.land.merge import ConflictStrategy

if TYPE_CHECKING:
    from pathlib import Path

_log: Final = logging.getLogger(__name__)

#: The suffix a sidecar carries, after the plugin's own name.
META_SUFFIX: Final = ".mergedlands.toml"

#: The only schema version this understands. Merged Lands versions the file so
#: a later format can be recognised rather than misread; an unknown version is
#: refused for the same reason.
SUPPORTED_VERSION: Final = "0"

#: ``meta_type`` values. ``MergedLands`` marks a previously generated
#: ``Merged Lands.esp`` so a re-run ignores its own last output instead of
#: merging a merge.
META_AUTO: Final = "Auto"
META_PATCH: Final = "Patch"
META_MERGED_LANDS: Final = "MergedLands"

#: The layer names the file uses, mapped to the layers they govern.
#:
#: ``height_map`` covers normals too: the record stores heights and normals
#: under one flag, and excluding one while merging the other produces terrain
#: lit as though it had not moved.
LAYER_NAMES: Final[dict[str, LandData]] = {
    "height_map": LandData.VERTEX_HEIGHTS | LandData.VERTEX_NORMALS,
    "vertex_colors": LandData.VERTEX_COLORS,
    "texture_indices": LandData.TEXTURES,
    "world_map_data": LandData.WORLD_MAP,
}

#: ``conflict_strategy`` values, as the file spells them.
_STRATEGIES: Final[dict[str, ConflictStrategy]] = {
    "Auto": ConflictStrategy.AUTO,
    "Resolve": ConflictStrategy.RESOLVE,
    "Overwrite": ConflictStrategy.OVERWRITE,
    "Ignore": ConflictStrategy.IGNORE,
    # Not a Merged Lands value. Ours, so a sidecar can ask for the
    # structure-weighted resolve described in wraithguard.land.curvature.
    "Curvature": ConflictStrategy.CURVATURE,
}


class MetaError(Exception):
    """Raised when a sidecar exists but cannot be trusted."""


@dataclass(frozen=True, slots=True)
class MergeSettings:
    """How one layer of one plugin should be treated.

    Attributes:
        included: When false, this plugin's edits to the layer are dropped.
        conflict_strategy: What to do where its edits collide with another's.
    """

    included: bool = True
    conflict_strategy: ConflictStrategy = ConflictStrategy.AUTO


@dataclass(slots=True)
class PluginMeta:
    """Everything a sidecar says about one plugin.

    Attributes:
        meta_type: ``Auto`` when no file existed, ``Patch`` for a real one,
            ``MergedLands`` to mark a previously generated merge.
        layers: Settings per layer name.
    """

    meta_type: str = META_AUTO
    layers: dict[str, MergeSettings] = field(default_factory=dict)

    def settings_for(self, name: str) -> MergeSettings:
        """The settings for one layer, defaulting to permissive.

        Args:
            name: A key from :data:`LAYER_NAMES`.

        Returns:
            The settings, or the defaults when the file did not mention it.
        """
        return self.layers.get(name, MergeSettings())

    @property
    def is_previous_merge(self) -> bool:
        """Whether this marks a plugin the tool generated itself.

        A merged plugin left in the Data Files would otherwise be read as an
        ordinary mod on the next run and merged into its own successor,
        compounding every earlier compromise.
        """
        return self.meta_type == META_MERGED_LANDS

    def allowed_layers(self) -> LandData:
        """Which layers this plugin may contribute at all.

        Returns:
            The layers not excluded by ``included = false``.
        """
        allowed = LandData.NONE
        for name, flag in LAYER_NAMES.items():
            if self.settings_for(name).included:
                allowed |= flag
        return allowed

    def strategy_for(self, layer: LandData) -> ConflictStrategy:
        """The conflict strategy governing one layer.

        Args:
            layer: The layer being merged.

        Returns:
            The configured strategy, or ``AUTO``.
        """
        for name, flag in LAYER_NAMES.items():
            if layer & flag:
                return self.settings_for(name).conflict_strategy
        return ConflictStrategy.AUTO


def _load_toml(path: Path) -> dict[str, Any]:
    """Read a TOML file with whatever parser is available.

    ``tomllib`` is standard from Python 3.11 and this project supports 3.10,
    so the third-party ``tomli`` is accepted as a fallback. Neither being
    present is reported rather than silently ignored: a sidecar that exists and
    is not read means the merge quietly disobeys the user.

    Args:
        path: The sidecar.

    Returns:
        The parsed document.

    Raises:
        MetaError: If no parser is available or the file will not parse.
    """
    try:
        import tomllib
    except ImportError:
        try:
            # no-redef fires only when tomllib resolved above (3.11+), and
            # unused-ignore only when it did not (3.10). The project supports
            # both, so both codes are silenced -- suppressing one alone makes
            # the file fail to check on the other version.
            import tomli as tomllib  # type: ignore[no-redef, unused-ignore]
        except ImportError as exc:
            raise MetaError(
                f"{path.name} exists but cannot be read: TOML parsing needs "
                "Python 3.11+ or the 'tomli' package. Refusing to continue "
                "rather than merge in a way the file was written to prevent."
            ) from exc

    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except OSError as exc:
        raise MetaError(f"could not read {path.name}: {exc}") from exc
    except tomllib.TOMLDecodeError as exc:
        raise MetaError(f"{path.name} is not valid TOML: {exc}") from exc


def parse_meta(document: dict[str, Any], name: str = "<meta>") -> PluginMeta:
    """Turn a parsed sidecar into settings.

    Args:
        document: The parsed TOML.
        name: The file's name, for error messages.

    Returns:
        The settings.

    Raises:
        MetaError: If the version is unknown, or a value is not one this
            understands. Both are refused rather than defaulted: a typo in a
            strategy name would otherwise silently give ``Auto``, which is
            usually the opposite of why the file was written.
    """
    version = document.get("version")
    if version is not None and str(version) != SUPPORTED_VERSION:
        raise MetaError(
            f"{name} declares version {version!r}, and only "
            f"{SUPPORTED_VERSION!r} is understood. A newer file may mean "
            "something different by the same keys."
        )

    meta_type = document.get("meta_type", META_AUTO)
    if meta_type not in (META_AUTO, META_PATCH, META_MERGED_LANDS):
        raise MetaError(f"{name} has an unknown meta_type {meta_type!r}")

    layers: dict[str, MergeSettings] = {}
    for key, value in document.items():
        if key in ("version", "meta_type"):
            continue
        if key not in LAYER_NAMES:
            raise MetaError(
                f"{name} configures {key!r}, which is not a landscape layer. "
                f"Expected one of: {', '.join(sorted(LAYER_NAMES))}."
            )
        if not isinstance(value, dict):
            raise MetaError(f"{name}: [{key}] must be a table")

        included = value.get("included", True)
        if not isinstance(included, bool):
            raise MetaError(f"{name}: [{key}] included must be true or false")

        raw = value.get("conflict_strategy", "Auto")
        strategy = _STRATEGIES.get(str(raw))
        if strategy is None:
            raise MetaError(
                f"{name}: [{key}] conflict_strategy {raw!r} is not recognised. "
                f"Expected one of: {', '.join(_STRATEGIES)}."
            )
        layers[key] = MergeSettings(included=included, conflict_strategy=strategy)

    return PluginMeta(meta_type=str(meta_type), layers=layers)


def meta_path_for(plugin: Path) -> Path:
    """Where a plugin's sidecar would live.

    Args:
        plugin: The plugin file.

    Returns:
        The sidecar path, whether or not it exists.
    """
    return plugin.with_suffix("").with_name(plugin.stem + META_SUFFIX)


def load_meta(plugin: Path) -> PluginMeta:
    """Read a plugin's sidecar, or return permissive defaults.

    Args:
        plugin: The plugin file.

    Returns:
        The settings. A plugin with no sidecar merges exactly as it would
        without this module.

    Raises:
        MetaError: If a sidecar exists and cannot be trusted.
    """
    path = meta_path_for(plugin)
    if not path.is_file():
        return PluginMeta()
    meta = parse_meta(_load_toml(path), path.name)
    _log.info("%s: read merge settings from %s", plugin.name, path.name)
    return meta


def load_all(folder: Path, plugins: list[str]) -> dict[str, PluginMeta]:
    """Read every sidecar in a folder.

    Args:
        folder: The Data Files directory.
        plugins: Plugin file names.

    Returns:
        Settings per plugin name. Plugins with no sidecar are still present,
        carrying the defaults, so a caller never has to test for absence.

    Raises:
        MetaError: If any sidecar exists and cannot be trusted.
    """
    return {name: load_meta(folder / name) for name in plugins}


def write_merged_marker(plugin: Path) -> Path:
    """Mark a plugin we generated so a later run ignores it.

    **Without this, merging twice compounds.** A ``Merged Lands.esp`` sitting in
    the load order is an ``.esp`` that edits every merged cell, and it loads
    last -- so a second run would read it as a mod, treat its terrain as one
    more opinion to reconcile, and produce a merge of a merge. Nothing about
    that fails loudly; the terrain simply drifts a little further from what any
    mod author wrote on every run.

    Merged Lands writes this file in ``save_plugin`` for the same reason, and
    :meth:`PluginMeta.is_previous_merge` is what reads it back.

    The file is written next to the plugin and named after it, so deleting the
    plugin and leaving the sidecar behind is harmless -- the sidecar only ever
    describes a plugin of that name.

    Args:
        plugin: The plugin that was just written.

    Returns:
        The sidecar path.

    Raises:
        MetaError: If the sidecar cannot be written. This is raised rather than
            logged: a merged plugin with no marker beside it is a trap for the
            next run, and the user needs to know now.
    """
    path = meta_path_for(plugin)
    body = (
        "# Written by Wraithguard Toolkit. Do not delete.\n"
        "#\n"
        "# It marks the plugin beside it as generated, so that merging again\n"
        "# ignores it instead of merging a merge. The format is Merged Lands'\n"
        "# .mergedlands.toml.\n"
        f'version = "{SUPPORTED_VERSION}"\n'
        f'meta_type = "{META_MERGED_LANDS}"\n'
    )
    try:
        path.write_text(body, encoding="utf-8")
    except OSError as exc:
        raise MetaError(
            f"could not write {path.name}, which marks {plugin.name} as generated. "
            "Without it a later merge would treat this plugin as a mod and merge "
            f"its terrain back into itself: {exc}"
        ) from exc
    return path
