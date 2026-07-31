"""Answering "does this mesh conflict matter?" without paying for it twice.

The reader and :mod:`wraithguard.nif.report` can already say what a mesh holds
and what the winner of a conflict loses. What they cannot do is survive contact
with a real mod folder, for two reasons this module exists to solve:

* **Cost.** Parsing is roughly 120 meshes a second. Running it over every file
  in a twenty-thousand-mesh setup would add minutes to a scan for an answer
  nobody asked for. Only paths that already conflict *and* already differ in
  bytes are worth opening, and even those should be opened once.
* **Trust.** These files come from mod archives written by strangers over
  twenty years. A mesh from a newer exporter, a truncated download or a
  deliberately malformed file must produce a finding that says "not readable",
  never an exception out of a scan and never a *wrong* claim.

The second point is the sharper one, and it shapes the whole interface.
:class:`Structure` already records whether a read was partial, and
:func:`~wraithguard.nif.report.compare` refuses to call an absence a loss when
either side is incomplete. This module keeps that guarantee end to end: a
finding either rests on two fully-read meshes or says why it does not.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import TYPE_CHECKING

from wraithguard.logging_setup import get_logger
from wraithguard.nif.reader import NifParseError, read_nif
from wraithguard.nif.report import compare, summarise

if TYPE_CHECKING:
    from pathlib import Path

    from wraithguard.nif.report import Difference, Structure

LOG = get_logger(__name__)

#: How much of a file to hash for cache identity. Meshes are small and read
#: whole anyway, so there is nothing to gain from partial hashing.
_HASH_CHUNK = 1 << 20


@dataclass(frozen=True, slots=True)
class MeshFinding:
    """The outcome of comparing two providers of one mesh path.

    Exactly one of :attr:`difference` and :attr:`unreadable` is meaningful. A
    caller that wants to report a loss must check :attr:`reliable` first --
    that is the whole reason this is a record rather than a bare
    :class:`~wraithguard.nif.report.Difference`.

    Attributes:
        path: The asset path, relative to the data folder.
        difference: What the winner changes, or ``None`` when either side could
            not be read.
        unreadable: Why a side could not be read, empty when both were fine.
            Kept as text because the reason is worth showing: "not a Morrowind
            NIF version" and "file ends mid-block" send a user to different
            places.
        loser_partial: Whether the overridden mesh was only partly parsed.
        winner_partial: Whether the winning mesh was only partly parsed.
    """

    path: str
    difference: Difference | None = None
    unreadable: str = ""
    loser_partial: bool = False
    winner_partial: bool = False

    @property
    def reliable(self) -> bool:
        """Whether an *absence* in this finding can be believed.

        A partial read can prove a mesh has collision; it can never prove one
        does not, because the node may sit in the part that was not reached.
        Reporting "lost collision" from a partial read would be a false alarm
        about the one thing a user would act on immediately.
        """
        return (
            self.difference is not None
            and not self.unreadable
            and not self.loser_partial
            and not self.winner_partial
        )

    @property
    def worth_reporting(self) -> bool:
        """Whether this finding says anything a person needs to see."""
        if not self.reliable or self.difference is None:
            return False
        return bool(
            self.difference.lost_collision
            or self.difference.lost_animation
            or self.difference.added_textures
            or self.difference.dropped_textures
        )


def file_digest(path: Path) -> str:
    """Hash a file's contents for cache identity.

    Args:
        path: The file to hash.

    Returns:
        A hex digest, or ``""`` when the file could not be read. An empty
        digest deliberately does not compare equal to anything cached, so an
        unreadable file is retried rather than remembered as a failure.
    """
    digest = hashlib.blake2b(digest_size=16)
    try:
        with path.open("rb") as handle:
            while chunk := handle.read(_HASH_CHUNK):
                digest.update(chunk)
    except OSError as exc:
        LOG.debug("cannot hash %s: %s", path, exc)
        return ""
    return digest.hexdigest()


class MeshAnalyser:
    """Parses meshes on demand and remembers what it found.

    The cache is keyed on **content**, not on path, which is the property that
    makes it worth having. Mods re-ship identical assets constantly, so one
    mesh body is routinely provided by several folders; keying on path would
    re-parse the same bytes once per provider and cache nothing across a
    rescan.

    Not thread-safe, and deliberately so -- it is a plain dict behind a method,
    and adding a lock would buy nothing until something actually parses in
    parallel.
    """

    def __init__(self) -> None:
        """Start with an empty cache."""
        self._structures: dict[str, Structure | str] = {}
        self._digests: dict[tuple[str, int, int], str] = {}
        self.parsed = 0
        self.cache_hits = 0
        self.hashed = 0

    def digest_of(self, path: Path, digest: str = "") -> str:
        """Identify a file's contents, as cheaply as the caller allows.

        Three tiers, because hashing turned out to dominate once parsing was
        cached: re-reading 104 MB of meshes cost five seconds even when every
        parse was a cache hit.

        1. A digest the caller already has. ``detect_resource_conflicts``
           computes blake2b digests to decide which providers differ, so on
           the path that matters most this is free.
        2. A remembered digest for the same path, size and modification time.
        3. Hashing the file.

        Args:
            path: The file to identify.
            digest: A digest the caller already computed, if any.

        Returns:
            A digest, or ``""`` when the file could not be read.
        """
        if digest:
            return digest
        try:
            stat = path.stat()
        except OSError as exc:
            # Logged, unlike before: its sibling handler below logs, and a
            # silent branch beside a noisy one is how a missing provider looks
            # identical to a working one at every log level.
            LOG.debug("cannot stat %s: %s", path, exc)
            return ""
        key = (str(path), stat.st_size, stat.st_mtime_ns)
        remembered = self._digests.get(key)
        if remembered is not None:
            return remembered
        computed = file_digest(path)
        self.hashed += 1
        if computed:
            self._digests[key] = computed
        return computed

    def structure(self, path: Path, digest: str = "") -> Structure | str:
        """Summarise one mesh, reusing an earlier result for identical bytes.

        Args:
            path: The mesh to read.
            digest: A content digest the caller already has, to skip hashing.

        Returns:
            Its :class:`~wraithguard.nif.report.Structure`, or a string
            explaining why it could not be read. Returning the reason rather
            than ``None`` is what lets a report distinguish "an old NIF
            version" from "a truncated download".
        """
        digest = self.digest_of(path, digest)
        if digest:
            cached = self._structures.get(digest)
            if cached is not None:
                self.cache_hits += 1
                return cached
        try:
            result = summarise(read_nif(path))
        except NifParseError as exc:
            # Expected, not exceptional: mod folders contain meshes for other
            # games and other engine versions, and a path can turn out to be a
            # folder. This is a finding about the file, so it is returned
            # rather than raised.
            #
            # There is deliberately no ``except OSError`` beside this one.
            # ``read_nif`` already converts an unreadable path into a
            # ``NifParseError``, so such a handler could never run -- it was
            # here, and an audit found it only because coverage could not reach
            # it. A handler for an impossible case is worse than none: it
            # advertises a failure mode that does not exist.
            LOG.debug("cannot read %s: %s", path, exc)
            outcome: Structure | str = str(exc)
        else:
            outcome = result
        self.parsed += 1
        if digest:
            self._structures[digest] = outcome
        return outcome

    def compare_providers(
        self,
        asset_path: str,
        loser: Path,
        winner: Path,
        loser_digest: str = "",
        winner_digest: str = "",
    ) -> MeshFinding:
        """Compare the overridden mesh with the one that wins the VFS.

        Args:
            asset_path: The asset path, for the finding.
            loser: The provider that loses.
            winner: The provider that wins.
            loser_digest: A digest the caller already has for ``loser``.
            winner_digest: A digest the caller already has for ``winner``.

        Returns:
            What changes, or why that could not be determined.
        """
        left = self.structure(loser, loser_digest)
        right = self.structure(winner, winner_digest)
        if isinstance(left, str) or isinstance(right, str):
            reason = left if isinstance(left, str) else right
            return MeshFinding(asset_path, unreadable=str(reason))
        return MeshFinding(
            asset_path,
            difference=compare(left, right),
            loser_partial=left.partial,
            winner_partial=right.partial,
        )
