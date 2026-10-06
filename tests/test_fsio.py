"""wraithguard.fsio: walking data folders and comparing files, Rust and Python alike."""

from __future__ import annotations

from pathlib import Path

import pytest

from wraithguard import fsio

try:
    import wraithguard_native

    NATIVE = hasattr(wraithguard_native, "walk_files")
except ImportError:
    NATIVE = False


@pytest.fixture(params=["python", "native"])
def mode(request, monkeypatch):
    if request.param == "native" and not NATIVE:
        pytest.skip("the Rust backend is not built")
    if request.param == "python":
        monkeypatch.setattr(fsio, "_native", lambda _name: None)
    return request.param


def _tree(root: Path) -> Path:
    (root / "Meshes" / "X").mkdir(parents=True)
    (root / "a.ESP").write_bytes(b"p")
    (root / "Meshes" / "X" / "Rock.NIF").write_bytes(b"rock")
    (root / "Meshes" / "b.nif").write_bytes(b"b")
    return root


def test_walk_relative_lowered_skipping(tmp_path, mode):
    root = _tree(tmp_path / "mod")
    got = fsio.walk_files([root, tmp_path / "missing"], skip_exts={"esp"})
    assert sorted(got[0] or []) == ["meshes/b.nif", "meshes/x/rock.nif"]
    assert got[1] is None
    kept = fsio.walk_files([root], lower=False)[0] or []
    assert sorted(kept) == ["Meshes/X/Rock.NIF", "Meshes/b.nif", "a.ESP"]


def test_identical(tmp_path, mode):
    big = bytes(i % 251 for i in range((1 << 20) * 2 + 17))
    late = big[:-1] + bytes([big[-1] ^ 1])
    for name, data in {"a": big, "b": big, "c": late, "e1": b"", "e2": b""}.items():
        (tmp_path / name).write_bytes(data)
    p = tmp_path
    assert fsio.files_identical([p / "a", p / "b"])
    assert not fsio.files_identical([p / "a", p / "c"])
    assert not fsio.files_identical([p / "a", p / "missing"])
    assert fsio.files_identical([p / "e1", p / "e2"])
    assert fsio.files_identical_many([[p / "a", p / "b"], [p / "b", p / "c"]]) == [True, False]


def test_digest_is_hashlibs(tmp_path, mode):
    import hashlib

    data = bytes(range(256)) * 5
    (tmp_path / "m.nif").write_bytes(data)
    assert fsio.file_digest(tmp_path / "m.nif") == hashlib.blake2b(data, digest_size=16).hexdigest()
    assert fsio.file_digest(tmp_path / "missing") == ""


def test_mod_folders_stop_at_a_match(tmp_path, mode):
    (tmp_path / "A" / "Meshes" / "deep" / "Textures").mkdir(parents=True)
    (tmp_path / "B").mkdir()
    (tmp_path / "C" / "x").mkdir(parents=True)
    (tmp_path / "B" / "b.ESP").write_bytes(b"")
    (tmp_path / "B" / "readme.txt").write_bytes(b"")
    got = fsio.scan_mod_folders(tmp_path, {"meshes", "textures"}, {".esp", ".esm"})
    names = sorted((Path(p).name, pl) for p, pl in got)
    assert names == [("A", []), ("B", ["b.ESP"])]
