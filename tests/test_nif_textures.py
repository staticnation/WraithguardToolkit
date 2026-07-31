"""Tests for resolving a mesh's texture reference to a file on disk.

Every case here is one that actually occurs in Morrowind mod collections. A
naive path join fails all of them, and each failure looks the same from the
outside -- an untextured mesh -- so they are worth separating.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from wraithguard.nif.textures import TextureResolver

if TYPE_CHECKING:
    from pathlib import Path


def make_texture(folder: Path, name: str, body: bytes = b"DDS fake") -> Path:
    """Put a file in a folder's texture directory.

    Args:
        folder: The data folder.
        name: The file name, which may include subdirectories.
        body: Its contents.

    Returns:
        The file written.
    """
    path = folder / "textures" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body)
    return path


class TestReferencesAreNotPaths:
    """The four ways a reference differs from where the file lives."""

    def test_a_bare_name_is_rooted_at_textures(self, tmp_path: Path) -> None:
        """Meshes live under meshes/ and never say the texture does not."""
        make_texture(tmp_path / "Mod", "tx_rock.dds")
        found = TextureResolver([tmp_path / "Mod"]).resolve("tx_rock.dds")
        assert found.found
        assert found.path is not None
        assert found.path.name == "tx_rock.dds"

    def test_case_is_ignored(self, tmp_path: Path) -> None:
        """These paths were written on Windows and are read where case matters."""
        make_texture(tmp_path / "Mod", "TX_Rock.DDS")
        assert TextureResolver([tmp_path / "Mod"]).resolve("tx_rock.dds").found

    def test_a_redundant_textures_prefix_is_handled(self, tmp_path: Path) -> None:
        """Some exporters write it and some do not."""
        make_texture(tmp_path / "Mod", "tx_rock.dds")
        for reference in ("textures/tx_rock.dds", r"textures\tx_rock.dds", "tx_rock.dds"):
            assert TextureResolver([tmp_path / "Mod"]).resolve(reference).found, reference

    def test_a_tga_reference_finds_the_dds_that_shipped(self, tmp_path: Path) -> None:
        """The single most common case in practice.

        Meshes exported in 2003 reference ``.tga``; the mod ships only
        ``.dds`` and relies on the game's fallback. Refusing to substitute
        would leave a large share of real meshes untextured.
        """
        make_texture(tmp_path / "Mod", "tx_rock.dds")
        found = TextureResolver([tmp_path / "Mod"]).resolve("tx_rock.tga")
        assert found.found
        assert found.substituted

    def test_substituted_means_the_extension_changed(self, tmp_path: Path) -> None:
        """And nothing else.

        Stripping a redundant prefix is not a substitution. Reporting it as one
        would make the flag mean "we tried more than one candidate", which is
        not what the name says -- and it did, until this test.
        """
        make_texture(tmp_path / "Mod", "tx_rock.dds")
        found = TextureResolver([tmp_path / "Mod"]).resolve("textures/tx_rock.dds")
        assert found.found
        assert not found.substituted


class TestTheVirtualFileSystem:
    """A mesh in one mod routinely draws with a texture from another."""

    def test_the_last_folder_wins(self, tmp_path: Path) -> None:
        """The same rule the conflict scan applies, so this is what would load."""
        first, second = tmp_path / "ModA", tmp_path / "ModB"
        make_texture(first, "tx.dds", b"first")
        make_texture(second, "tx.dds", b"second")
        found = TextureResolver([first, second]).resolve("tx.dds")
        assert found.path is not None
        assert found.path.read_bytes() == b"second"

    def test_a_contested_texture_is_reported_as_such(self, tmp_path: Path) -> None:
        """A mesh conflict is often really a texture conflict."""
        first, second = tmp_path / "ModA", tmp_path / "ModB"
        make_texture(first, "tx.dds")
        make_texture(second, "tx.dds")
        found = TextureResolver([first, second]).resolve("tx.dds")
        assert found.contested
        assert len(found.providers) == 2

    def test_a_single_provider_is_not_contested(self, tmp_path: Path) -> None:
        """A negative control, so "contested" carries information."""
        make_texture(tmp_path / "Mod", "tx.dds")
        assert not TextureResolver([tmp_path / "Mod"]).resolve("tx.dds").contested

    def test_a_texture_from_another_mod_is_found(self, tmp_path: Path) -> None:
        """Resolving only within the mesh's own folder would miss most of them."""
        mesh_mod, texture_mod = tmp_path / "Meshes", tmp_path / "Textures"
        (mesh_mod / "meshes").mkdir(parents=True)
        make_texture(texture_mod, "shared.dds")
        assert TextureResolver([mesh_mod, texture_mod]).resolve("shared.dds").found


class TestMissingAndMalformed:
    """A mod collection is full of both."""

    def test_a_missing_texture_is_reported_not_raised(self, tmp_path: Path) -> None:
        """Broken references are common and are not our failure."""
        make_texture(tmp_path / "Mod", "tx.dds")
        found = TextureResolver([tmp_path / "Mod"]).resolve("does_not_exist.dds")
        assert not found.found
        assert found.path is None

    def test_an_empty_reference_resolves_to_nothing(self, tmp_path: Path) -> None:
        """Untextured shapes carry an empty string, not a missing field."""
        assert not TextureResolver([tmp_path]).resolve("").found
        assert not TextureResolver([tmp_path]).resolve("   ").found

    def test_a_folder_with_no_textures_directory_is_fine(self, tmp_path: Path) -> None:
        """Plenty of mods ship meshes only."""
        (tmp_path / "Mod" / "meshes").mkdir(parents=True)
        assert not TextureResolver([tmp_path / "Mod"]).resolve("tx.dds").found

    def test_a_nonexistent_folder_does_not_raise(self, tmp_path: Path) -> None:
        """A data path can be stale in the configuration."""
        assert not TextureResolver([tmp_path / "gone"]).resolve("tx.dds").found

    def test_subdirectories_are_indexed(self, tmp_path: Path) -> None:
        """Textures are routinely nested under a mod's own folder."""
        make_texture(tmp_path / "Mod", "bm/tx_snow.dds")
        assert TextureResolver([tmp_path / "Mod"]).resolve("bm/tx_snow.dds").found
        assert TextureResolver([tmp_path / "Mod"]).resolve(r"bm\tx_snow.dds").found


class TestArchivesAreSearchedAfterLooseFiles:
    """The engine prefers a loose file; so does this.

    A mod dropping a texture into ``Textures/`` overrides the archived one, and
    reproducing that order is what makes the result "what would actually load"
    rather than "what exists somewhere".
    """

    @staticmethod
    def _bsa(files: dict[str, bytes]) -> bytes:
        """Build a Morrowind archive to the documented layout.

        Written here rather than imported so the test data is visible: this
        *is* the format, and a reader validated only against a writer that
        shares its assumptions proves nothing about the format itself. The real
        check is ``tools/check_bsa.py`` against a shipped archive.

        Args:
            files: Stored path to contents.

        Returns:
            The archive bytes.
        """
        import struct

        names = list(files)
        count = len(names)
        blob, offsets = b"", []
        for name in names:
            offsets.append(len(blob))
            blob += name.encode("cp1252") + b"\x00"
        out = struct.pack("<III", 0x100, count * 12 + len(blob), count)
        data, records = b"", b""
        for name in names:
            records += struct.pack("<II", len(files[name]), len(data))
            data += files[name]
        out += records + b"".join(struct.pack("<I", o) for o in offsets) + blob
        return out + b"\x00" * (count * 8) + data

    def test_an_archived_texture_is_found(self, tmp_path: Path) -> None:
        """Most of the base game lives in Morrowind.bsa and nowhere else."""
        folder = tmp_path / "Data Files"
        folder.mkdir()
        (folder / "Morrowind.bsa").write_bytes(self._bsa({"textures/tx.dds": b"ARCHIVED"}))
        resolver = TextureResolver([folder])
        found = resolver.resolve("tx.dds")
        assert found.found
        assert found.from_archive
        assert resolver.read(found) == b"ARCHIVED"

    def test_a_loose_file_beats_the_archive(self, tmp_path: Path) -> None:
        """Which is how a retexture mod works at all."""
        folder = tmp_path / "Data Files"
        make_texture(folder, "tx.dds", b"LOOSE")
        (folder / "Morrowind.bsa").write_bytes(self._bsa({"textures/tx.dds": b"ARCHIVED"}))
        resolver = TextureResolver([folder])
        found = resolver.resolve("tx.dds")
        assert not found.from_archive
        assert resolver.read(found) == b"LOOSE"

    def test_extension_substitution_works_through_an_archive(self, tmp_path: Path) -> None:
        """Base-game meshes name .bmp and .tga for files archived as .dds."""
        folder = tmp_path / "Data Files"
        folder.mkdir()
        (folder / "Morrowind.bsa").write_bytes(self._bsa({"textures/tx.dds": b"ARCHIVED"}))
        found = TextureResolver([folder]).resolve("tx.bmp")
        assert found.found
        assert found.substituted

    def test_a_later_format_archive_is_skipped_not_fatal(self, tmp_path: Path) -> None:
        """A Skyrim BSA in a folder must not stop the rest from opening."""
        folder = tmp_path / "Data Files"
        make_texture(folder, "tx.dds", b"LOOSE")
        (folder / "Skyrim.bsa").write_bytes(b"BSA\x00" + b"\x00" * 64)
        assert TextureResolver([folder]).resolve("tx.dds").found

    def test_a_texture_in_no_archive_and_no_folder_is_not_found(self, tmp_path: Path) -> None:
        """A negative control: the archive path must be able to say no."""
        folder = tmp_path / "Data Files"
        folder.mkdir()
        (folder / "Morrowind.bsa").write_bytes(self._bsa({"textures/tx.dds": b"ARCHIVED"}))
        assert not TextureResolver([folder]).resolve("other.dds").found

    def test_membership_does_not_read_the_file(self, tmp_path: Path) -> None:
        """Resolving tries ten names per texture against every archive.

        Answering each with a read pulls the bytes and discards them -- around
        sixty wasted reads per texture against the three vanilla archives, on
        the path the 3D viewer walks for every shape in a mesh.
        """
        from wraithguard.nif.bsa import BsaArchive

        path = tmp_path / "Morrowind.bsa"
        path.write_bytes(self._bsa({"textures/tx.dds": b"ARCHIVED"}))
        archive = BsaArchive(path)
        assert "textures/tx.dds" in archive
        assert r"TEXTURES\TX.DDS" in archive
        assert "textures/absent.dds" not in archive


class TestOpenMwAuxiliaryMaps:
    """Normal and specular maps that no mesh ever mentions.

    The Morrowind NIF has no dependable slot for a normal map, so OpenMW finds
    them by *name*: given ``tx_rock.dds`` it looks for ``tx_rock_n.dds``. A
    viewer that followed only mesh references would never show one, and a
    conflict report that compared only referenced textures would miss two mods
    overwriting each other's normal maps entirely.
    """

    def test_the_stock_suffixes_are_found_beside_a_diffuse_texture(
        self, tmp_path: Path
    ) -> None:
        """The four patterns OpenMW ships with."""
        folder = tmp_path / "Mod"
        for name in ("tx_rock.dds", "tx_rock_n.dds", "tx_rock_nh.dds", "tx_rock_spec.dds"):
            make_texture(folder, name)
        found = TextureResolver([folder]).siblings("tx_rock.dds")
        assert set(found) == {"_n", "_nh", "_spec"}

    def test_a_texture_with_no_siblings_reports_none(self, tmp_path: Path) -> None:
        """The common case, and it must not invent anything."""
        make_texture(tmp_path / "Mod", "tx_rock.dds")
        assert TextureResolver([tmp_path / "Mod"]).siblings("tx_rock.dds") == {}

    def test_siblings_resolve_through_the_whole_virtual_file_system(
        self, tmp_path: Path
    ) -> None:
        """A texture pack adds normal maps for another mod's textures.

        That is the arrangement these suffixes exist for, so resolving them
        only within the diffuse texture's own folder would miss the point.
        """
        base, pack = tmp_path / "Base", tmp_path / "Pack"
        make_texture(base, "tx_rock.dds")
        make_texture(pack, "tx_rock_n.dds")
        found = TextureResolver([base, pack]).siblings("tx_rock.dds")
        assert set(found) == {"_n"}

    def test_an_empty_reference_asks_for_nothing(self, tmp_path: Path) -> None:
        """Untextured shapes carry an empty string, and must not be searched."""
        make_texture(tmp_path / "Mod", "tx_rock.dds")
        assert TextureResolver([tmp_path / "Mod"]).siblings("") == {}
