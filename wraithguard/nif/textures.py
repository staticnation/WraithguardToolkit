"""Finding the file a mesh's texture reference actually points at.

A ``NiSourceTexture`` holds a string like ``tx_rock_01.tga``. Turning that into
bytes on disk is not a path join, for four reasons that all bite in practice:

* **The reference is relative to ``textures/``**, not to the mesh. Meshes live
  under ``meshes/`` and never say so.
* **The extension is frequently wrong, and ``.dds`` wins.** A mesh exported in
  2003 names a ``.tga``, but Bethesda converted the shipped textures to ``.dds``
  (keeping the ``.tga`` references), and DDS replacers drop a ``.dds`` beside the
  vanilla file. So the engine resolves ``.dds`` *first* whatever the reference
  says, and only falls back to the named extension -- the order OpenMW's
  ``correctTexturePath`` uses. Trying the named extension first would pick a
  stale ``.tga`` over the ``.dds`` that actually loads.
* **Case is not preserved.** These paths were written on Windows and are read
  on machines where case matters.
* **The texture may not be in the same mod as the mesh.** It is resolved
  through the same folder order the conflict scan uses, so a mesh in one mod
  can reference a texture another mod overrides -- which is itself worth
  knowing, and is why :meth:`TextureResolver.providers` exists.

The index is built once per folder set and reused. Walking a mod collection is
expensive enough that doing it per texture would be the slowest thing in the
viewer by a wide margin.

**Loose files first, then archives.** The base game ships most of its textures
inside ``Morrowind.bsa``, and the engine prefers a loose file over an archived
one -- so a mod dropping a texture into ``Textures/`` overrides the archive.
That order is reproduced here: every data folder's loose files are searched
before any archive, which is what makes "what would actually load" the answer
this returns.

An unresolved texture still means *"nothing provides it"* rather than *"the
file is corrupt"*, and a texture found only in an archive is not a conflict
with anything.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from typing import Final

from wraithguard.images.dds import CompressedTexture, dds_passthrough, dds_passthrough_info
from wraithguard.logging_setup import get_logger
from wraithguard.nif.bsa import BsaArchive, BsaError
from wraithguard.nif.vfs import archives_in

LOG = get_logger(__name__)

#: Where texture references are rooted. A reference names a file *inside* this
#: folder unless it already says otherwise.
_TEXTURE_ROOT: Final[str] = "textures"

#: Extensions to try, in order, when the referenced one is missing. ``.dds``
#: comes first because it is what almost everything actually ships.
_FALLBACK_SUFFIXES: Final[tuple[str, ...]] = (".dds", ".tga", ".bmp", ".png")

#: A guard on how many files are indexed, so a folder that is not a mod setup
#: cannot turn opening a viewer into a filesystem walk of the whole disk.
_MAX_INDEXED: Final[int] = 500_000

#: The name suffixes OpenMW looks for beside a diffuse texture, in the order a
#: viewer would want to offer them. These are the stock patterns from the
#: ``[Shaders]`` section of ``settings.cfg``; an install can configure others,
#: which is why they are named here rather than buried in the lookup.
_AUXILIARY_SUFFIXES: Final[tuple[str, ...]] = ("_n", "_nh", "_spec", "_diffusespec")


@dataclass(frozen=True, slots=True)
class Resolved:
    """Where a texture reference led.

    Attributes:
        reference: The path as the mesh wrote it.
        path: The loose file found, or ``None`` when it is archived or absent.
        providers: Every data folder that ships this texture, in load order.
            More than one means the texture is itself contested.
        substituted: Whether the extension had to be changed to find it.
        archived_name: The name inside an archive, when that is where it lives.
        archive: Which archive holds it.
    """

    reference: str
    path: Path | None = None
    providers: tuple[Path, ...] = ()
    substituted: bool = False
    archived_name: str = ""
    archive: Path | None = None

    @property
    def found(self) -> bool:
        """Whether anything provides it, loose or archived."""
        return self.path is not None or bool(self.archived_name)

    @property
    def from_archive(self) -> bool:
        """Whether it came out of a BSA rather than off the disk.

        Worth distinguishing: an archived texture is the base game's own and
        cannot be in conflict with anything, while a loose one was put there
        by a mod.
        """
        return self.path is None and bool(self.archived_name)

    @property
    def contested(self) -> bool:
        """Whether more than one data folder provides this texture."""
        return len(self.providers) > 1


class TextureResolver:
    """Resolves texture references against an ordered set of data folders.

    Later folders win, matching the game's virtual file system and the
    conflict scan's rule, so what this returns is what would actually load.
    """

    def __init__(self, data_dirs: list[Path], archives: list[Path] | None = None) -> None:
        """Index the textures in each folder, and open any archives.

        Args:
            data_dirs: Data folders in load order; later ones override earlier.
            archives: ``.bsa`` files to fall back on. Omitted, they are looked
                for in the data folders themselves, which is where they live.
        """
        self._dirs = list(data_dirs)
        self._archives: list[BsaArchive] = []
        # key -> providers in load order. The key is lower-cased and
        # slash-normalised, which is what makes lookups case-insensitive
        # without a second pass per query.
        self._index: dict[str, list[Path]] = {}
        # Decoded (reference -> PNG bytes + MIME, or None) memo. DDS decoding is
        # pure-Python and slow -- seconds per texture at cell scale -- and a
        # collection reuses the same wall and crate textures across cell after
        # cell, so decoding each one once per session (this resolver is reused)
        # is the single biggest win when previewing many cells in a row.
        self._decode_cache: dict[tuple[object, int], tuple[bytes, str] | None] = {}
        # Candidate resolution is repeated for auxiliary maps and for meshes
        # sharing a texture. Cache the VFS answer, not only the decoded pixels.
        self._resolve_cache: dict[str, Resolved] = {}
        self._siblings_cache: dict[str, dict[str, Resolved]] = {}
        # Header-only DDS probes let the cell viewer decide on GPU passthrough
        # without reading or decoding the whole texture during page creation.
        self._dds_info_cache: dict[object, CompressedTexture | None] = {}
        self._compressed_cache: dict[object, CompressedTexture | None] = {}
        self._build()
        self._open_archives(archives)

    def _build(self) -> None:
        """Walk each folder's texture directory once."""
        indexed = 0
        for folder in self._dirs:
            root = _texture_root(folder)
            if root is None:
                continue
            try:
                # os.walk avoids constructing a Path object for every directory
                # entry just to test it; on large mod collections this index is a
                # measurable part of first-view startup.
                for dirpath, _dirs, files in os.walk(root):
                    base = Path(dirpath)
                    for name in files:
                        path = base / name
                        indexed += 1
                        if indexed > _MAX_INDEXED:
                            LOG.warning("stopped indexing textures at %d files", _MAX_INDEXED)
                            return
                        key = path.relative_to(root).as_posix().lower()
                        self._index.setdefault(key, []).append(path)
            except OSError as exc:
                LOG.warning("cannot index textures under %s: %s", root, exc)
        LOG.debug("indexed %d texture(s) across %d folder(s)", indexed, len(self._dirs))

    def _open_archives(self, archives: list[Path] | None) -> None:
        """Open the archives to fall back on.

        Args:
            archives: Explicit paths, or ``None`` to look in the data folders.
        """
        if archives is None:
            # MeshVfs already opened and indexed these same BSA tables. Reuse
            # those objects so the first viewer does not parse every archive
            # twice, and later viewers do not reopen them at all.
            self._archives = [archive for folder in self._dirs for archive in archives_in(folder)]
            return
        for path in archives:
            try:
                self._archives.append(BsaArchive(path))
            except BsaError as exc:  # noqa: PERF203 -- one bad archive must not stop the rest
                # A post-Morrowind archive, or a corrupt one. Neither is our
                # failure and neither should stop the rest from opening.
                LOG.info("skipping %s: %s", path.name, exc)
        if self._archives:
            LOG.debug(
                "opened %d archive(s) holding %d file(s)",
                len(self._archives),
                sum(len(a) for a in self._archives),
            )

    def read(self, resolved: Resolved) -> bytes | None:
        """Read a resolved texture's bytes, from disk or from an archive.

        Args:
            resolved: The outcome of :meth:`resolve`.

        Returns:
            The bytes, or ``None`` when nothing provides it.
        """
        if resolved.path is not None:
            try:
                return resolved.path.read_bytes()
            except OSError as exc:
                LOG.warning("cannot read %s: %s", resolved.path, exc)
                return None
        if resolved.archived_name:
            archives = (
                [archive for archive in self._archives if archive.path == resolved.archive]
                if resolved.archive is not None
                else list(reversed(self._archives))
            )
            for archive in archives:
                try:
                    data = archive.read(resolved.archived_name)
                except BsaError as exc:
                    LOG.warning("cannot read from %s: %s", archive.path.name, exc)
                    continue
                if data is not None:
                    return data
        return None

    @staticmethod
    def _source_key(resolved: Resolved) -> object:
        """Stable identity for the actual file behind a resolved reference."""
        if resolved.path is not None:
            return ("path", str(resolved.path))
        if resolved.archived_name:
            return ("archive", str(resolved.archive or ""), resolved.archived_name)
        return ("missing", resolved.reference.lower())

    def dds_info(self, resolved: Resolved) -> CompressedTexture | None:
        """Probe a resolved DDS header without reading its mip payload."""
        if not resolved.found:
            return None
        key = self._source_key(resolved)
        if key in self._dds_info_cache:
            return self._dds_info_cache[key]
        info = dds_passthrough_info(self.read_prefix(resolved, 148) or b"")
        # The generic viewer only has S3TC in its compressed path today. BC7
        # remains on the decoder until an appropriate WebGL extension is wired.
        if info is not None and info.format not in {"dxt1", "dxt3", "dxt5"}:
            info = None
        self._dds_info_cache[key] = info
        return info

    def read_compressed(self, resolved: Resolved) -> CompressedTexture | None:
        """Read a GPU-uploadable DDS without decoding its blocks."""
        if not resolved.found:
            return None
        key = self._source_key(resolved)
        if key in self._compressed_cache:
            return self._compressed_cache[key]
        raw = self.read(resolved)
        info = dds_passthrough(raw) if raw is not None else None
        if info is not None and info.format not in {"dxt1", "dxt3", "dxt5"}:
            info = None
        self._compressed_cache[key] = info
        return info

    def read_prefix(self, resolved: Resolved, length: int) -> bytes | None:
        """Read only a small prefix from the source behind ``resolved``."""
        if not resolved.found:
            return None
        if resolved.path is not None:
            try:
                with resolved.path.open("rb") as handle:
                    return handle.read(length)
            except OSError as exc:
                LOG.warning("cannot read %s: %s", resolved.path, exc)
                return None
        if resolved.archived_name:
            for archive in reversed(self._archives):
                if resolved.archive is not None and archive.path != resolved.archive:
                    continue
                try:
                    data = archive.read_prefix(resolved.archived_name, length)
                except BsaError as exc:
                    LOG.warning(
                        "cannot read %s from %s: %s",
                        resolved.archived_name,
                        archive.path.name,
                        exc,
                    )
                    continue
                if data is not None:
                    return data
        return None

    def siblings(self, reference: str) -> dict[str, Resolved]:
        """Find the OpenMW auxiliary maps that sit beside a diffuse texture.

        A vanilla mesh names one texture per slot and knows nothing about
        normal or specular maps -- the Morrowind NIF has no dependable slot for
        them. OpenMW fills that gap by *looking for them by name*: given
        ``tx_rock.dds`` it will use ``tx_rock_n.dds`` as a normal map if the
        file is there and the feature is switched on in ``settings.cfg``.

        So these maps exist in a mod collection while being mentioned nowhere
        in any mesh. A viewer that only followed mesh references would never
        show them, and a conflict report that only compared referenced textures
        would miss two mods overwriting each other's normal maps entirely.

        Args:
            reference: A diffuse texture reference.

        Returns:
            The suffix (``"_n"``, ``"_nh"``, ``"_spec"``, ``"_diffusespec"``)
            mapped to what it resolved to, for those that exist. Empty when
            the collection ships none, which is the common case.
        """
        cleaned = reference.strip().replace("\\", "/")
        if not cleaned:
            return {}
        cache_key = cleaned.lower()
        cached = self._siblings_cache.get(cache_key)
        if cached is not None:
            return cached
        stem, _, suffix = cleaned.rpartition(".")
        if not stem:
            stem, suffix = cleaned, "dds"
        found: dict[str, Resolved] = {}
        for extra in _AUXILIARY_SUFFIXES:
            resolved = self.resolve(f"{stem}{extra}.{suffix}")
            if resolved.found:
                found[extra] = resolved
        if found:
            LOG.debug("%s has auxiliary map(s): %s", reference, ", ".join(found))
        self._siblings_cache[cache_key] = found
        return found

    def resolve(self, reference: str) -> Resolved:
        """Find the file a reference points at.

        Args:
            reference: The string from the mesh.

        Returns:
            What was found, including whether the extension was substituted and
            whether more than one folder provides it.
        """
        cleaned = reference.strip().replace("\\", "/").lstrip("/").lower()
        # Doubled separators occur in hand-edited and re-exported meshes, and
        # a key with "//" in it matches nothing in an index built from real
        # paths. Collapsing them costs one pass and avoids a class of silent
        # "untextured" results that look identical to a missing file.
        while "//" in cleaned:
            cleaned = cleaned.replace("//", "/")
        if not cleaned:
            return Resolved(reference)
        cached = self._resolve_cache.get(cleaned)
        if cached is not None:
            return cached
        # A reference may or may not already include the textures/ prefix.
        bases = [cleaned]
        if cleaned.startswith(f"{_TEXTURE_ROOT}/"):
            bases.append(cleaned[len(_TEXTURE_ROOT) + 1 :])
        # ``.dds`` is tried before the referenced extension, whatever it says.
        # Bethesda converted the BSA textures from ``.tga`` to ``.dds`` but left
        # the references naming ``.tga``, and DDS replacers ship a ``.dds`` beside
        # (or instead of) a ``.tga``; the engine resolves ``.dds`` first for both
        # reasons (OpenMW's ``correctTexturePath``, which passes ``ext=".dds"``).
        # Only then the named extension, then the remaining formats.
        candidates: list[str] = []
        for key in bases:
            stem = key.rsplit(".", 1)[0] if "." in key else key
            candidates.append(f"{stem}.dds")
            candidates.append(key)
            candidates.extend(f"{stem}{suffix}" for suffix in _FALLBACK_SUFFIXES)
        seen: set[str] = set()
        wanted_suffix = cleaned.rsplit(".", 1)[-1] if "." in cleaned else ""
        for key in candidates:
            if key in seen:
                continue
            seen.add(key)
            providers = self._index.get(key)
            if providers:
                found_suffix = key.rsplit(".", 1)[-1] if "." in key else ""
                resolved = Resolved(
                    reference=reference,
                    # Last provider wins, which is the VFS rule the rest of the
                    # tool already applies.
                    path=providers[-1],
                    providers=tuple(providers),
                    # Only an *extension* change counts. Stripping a redundant
                    # "textures/" prefix is not a substitution, and reporting
                    # it as one would make the flag mean "we tried more than
                    # one candidate" -- which is not what a reader would take
                    # from the name.
                    substituted=found_suffix != wanted_suffix,
                )
                self._resolve_cache[cleaned] = resolved
                return resolved
        # Nothing loose provides it, so try the archives -- the order the
        # engine itself uses.
        for key in candidates:
            for archive in reversed(self._archives):
                for stored in (key, f"{_TEXTURE_ROOT}/{key}"):
                    # Membership only -- reading here would fetch the bytes of
                    # every candidate that misses as well as the one that hits.
                    if stored in archive:
                        found_suffix = key.rsplit(".", 1)[-1] if "." in key else ""
                        resolved = Resolved(
                            reference=reference,
                            archived_name=stored,
                            archive=archive.path,
                            substituted=found_suffix != wanted_suffix,
                        )
                        self._resolve_cache[cleaned] = resolved
                        return resolved
        resolved = Resolved(reference)
        self._resolve_cache[cleaned] = resolved
        return resolved


def _texture_root(folder: Path) -> Path | None:
    """Find a folder's ``textures`` directory, whatever its case.

    Args:
        folder: A data folder.

    Returns:
        The directory, or ``None`` when it has none.
    """
    try:
        if not folder.is_dir():
            return None
        for child in folder.iterdir():
            if child.is_dir() and child.name.lower() == _TEXTURE_ROOT:
                return child
    except OSError as exc:
        LOG.debug("cannot list %s: %s", folder, exc)
    return None
