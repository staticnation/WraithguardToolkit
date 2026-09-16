"""A parsed-plugin cache for the cell preview -- across previews and launches.

A cell preview re-resolves the same load order every time it runs, and without a
cache it re-reads and re-parses every enabled plugin from disk each time. This
keeps the parsed result and hands it back untouched when the file has not
changed, so a repeat preview jumps straight to resolution.

Two layers, both keyed the way the rest of the load order is -- ``(path, size,
mtime)``:

* an in-memory map, so repeated previews in one session never re-parse;
* an optional on-disk *sidecar* per plugin, so a warmed cache survives a restart
  (the pre-warm that runs after a Sort writes these; the next launch reads them).

The parse is *filtered*: a preview only needs the cell, its terrain and land
textures, and the objects a cell can place, so :data:`PREVIEW_RECORD_TAGS` names
exactly those and everything else is skipped unread -- far less work than parsing
a plugin whole. The tag set is derived from the record registry (every record
that carries a mesh, plus cells, land, land textures and actors), so a
newly-ported record type that places a mesh is picked up without editing a list.
"""

from __future__ import annotations

import hashlib
import pickle
from pathlib import Path
from typing import TYPE_CHECKING, Final

from wraithguard.esp.plugin import read_header, read_plugin_filtered
from wraithguard.esp.record import REGISTRY
from wraithguard.esp.records.cell import Cell
from wraithguard.esp.records.landscape import Landscape
from wraithguard.esp.records.landscapetexture import LandscapeTexture
from wraithguard.logging_setup import get_logger
from wraithguard.scene.cellview import LoadedPlugin
from wraithguard.scene.resolve import ACTOR_TAGS

if TYPE_CHECKING:
    import os

LOG = get_logger(__name__)

#: Bump to invalidate every on-disk sidecar at once -- when the filter set, the
#: record layout, or anything a sidecar's pickled records depend on changes.
_SIDECAR_VERSION: Final[int] = 1


def _preview_record_tags() -> frozenset[bytes]:
    """The record tags a cell preview actually reads.

    Cells, terrain (``LAND``) and its textures (``LTEX``), every registered
    record that carries a mesh (the objects a cell places), and the actor tags
    (so their references are recognised and skipped). Derived from the registry
    so a mesh-bearing record ported later is included without touching this.
    """
    tags: set[bytes] = {Cell.TAG, Landscape.TAG, LandscapeTexture.TAG}
    tags |= set(ACTOR_TAGS)
    for tag, cls in REGISTRY.items():
        if "mesh" in getattr(cls, "__dataclass_fields__", {}):
            tags.add(tag)
    return frozenset(tags)


#: The tags :class:`PluginParseCache` keeps when it parses a plugin for a preview.
PREVIEW_RECORD_TAGS: Final[frozenset[bytes]] = _preview_record_tags()


class PluginParseCache:
    """Parsed plugins reused across previews, and optionally across launches.

    Not thread-safe by a lock, and it does not need to be: the worst a race
    between the background pre-warm and a preview can do is parse one plugin
    twice, which wastes a little work and corrupts nothing (both writes store an
    equal result). Everything is keyed on ``(path, size, mtime)``, so a file
    edited between previews is re-read and everything unchanged is reused.
    """

    def __init__(
        self,
        keep: frozenset[bytes] = PREVIEW_RECORD_TAGS,
        cache_dir: str | os.PathLike[str] | None = None,
    ) -> None:
        """Set up the cache.

        Args:
            keep: The record tags to parse (see :data:`PREVIEW_RECORD_TAGS`).
            cache_dir: Where to read and write sidecars; ``None`` keeps the cache
                in memory only (no persistence across launches).
        """
        self._keep = keep
        self._cache_dir = Path(cache_dir) if cache_dir is not None else None
        self._by_key: dict[tuple[str, int, int], LoadedPlugin] = {}

    def load(self, name: str, path: str | os.PathLike[str]) -> LoadedPlugin:
        """The parsed plugin at ``path``, from cache when the file is unchanged.

        Args:
            name: The plugin's load-order name (stored on the result).
            path: The file to read.

        Returns:
            The :class:`~wraithguard.scene.cellview.LoadedPlugin`.

        Raises:
            OSError: If the file cannot be stat-ed or read.
            EspError: If the plugin cannot be parsed.
        """
        file = Path(path)
        stat = file.stat()
        key = (str(file), stat.st_size, stat.st_mtime_ns)
        hit = self._by_key.get(key)
        if hit is not None:
            return hit
        loaded = self._from_sidecar(name, file, stat.st_size, stat.st_mtime_ns) or self._parse(
            name, file, stat.st_size, stat.st_mtime_ns
        )
        self._by_key[key] = loaded
        return loaded

    def _sidecar_path(self, file: Path) -> Path | None:
        """Where ``file``'s sidecar lives, or ``None`` when persistence is off."""
        if self._cache_dir is None:
            return None
        digest = hashlib.blake2b(str(file).encode("utf-8"), digest_size=16).hexdigest()
        return self._cache_dir / f"{digest}.plc"

    def _from_sidecar(self, name: str, file: Path, size: int, mtime: int) -> LoadedPlugin | None:
        """The plugin from its sidecar, or ``None`` to parse it fresh.

        Any failure -- no sidecar, a corrupt or stale one, an older format -- is a
        cache miss, never an error: the caller then parses the file as usual.
        """
        sidecar = self._sidecar_path(file)
        if sidecar is None:
            return None
        try:
            record = pickle.loads(sidecar.read_bytes())  # noqa: S301 -- our own cache dir
        except (OSError, pickle.UnpicklingError, EOFError, AttributeError, ValueError) as exc:
            LOG.debug("plugin sidecar miss for %s: %s", file, exc)
            return None
        if (
            not isinstance(record, dict)
            or record.get("v") != _SIDECAR_VERSION
            or record.get("size") != size
            or record.get("mtime") != mtime
        ):
            return None
        return LoadedPlugin(name=name, masters=record["masters"], records=record["records"])

    def _parse(self, name: str, file: Path, size: int, mtime: int) -> LoadedPlugin:
        """Parse the plugin (filtered) and, when persistence is on, write a sidecar."""
        data = file.read_bytes()
        records = read_plugin_filtered(data, self._keep)
        masters = [master for master, _size in read_header(data).masters]
        loaded = LoadedPlugin(name=name, masters=masters, records=records)
        sidecar = self._sidecar_path(file)
        if sidecar is not None:
            try:
                self._cache_dir.mkdir(parents=True, exist_ok=True)  # type: ignore[union-attr]
                sidecar.write_bytes(
                    pickle.dumps(
                        {
                            "v": _SIDECAR_VERSION,
                            "size": size,
                            "mtime": mtime,
                            "masters": masters,
                            "records": records,
                        },
                        protocol=pickle.HIGHEST_PROTOCOL,
                    )
                )
            except OSError as exc:  # a cache we cannot write is just one we do not have
                LOG.debug("could not write plugin sidecar for %s: %s", file, exc)
        return loaded
