# wraithguard_native

Wraithguard Toolkit's Rust backend: a Python extension module on
[greatness7's tes3 crates](https://github.com/Greatness7/tes3) (MIT OR Apache-2.0).
It replaced the toolkit's pure-Python ports of those crates one piece at a time, and
since 4.2.0 also carries the per-vertex and per-pixel loops that were too slow in
Python (Merged Lands, textures, the lint scan). Each piece keeps the Python API its
callers already use, and each moved loop has a parity test holding it to the Python
it replaced (`tests/test_*_native_parity.py`, with that Python kept beside them as
the reference).

| Piece | Rust | Replaces |
|---|---|---|
| `.bsa` archives | `src/bsa.rs` on `tes3::bsa` | the reader in `wraithguard/nif/bsa.py` (now a thin wrapper) |
| plugin records to Python | `src/esp.rs` on `tes3::esp` (`plugin_records`, via `pythonize`) | the JSON spool, its sidecars and the `ijson` streaming in the conflict session: `NativeEspSession` gets each plugin's records as dicts in tes3conv's layout, with no JSON in between. The packed arrays (landscape grids, script variables and bytecode, path-grid connections) come as raw `bytes` rather than zstd'd base64. `light=True` leaves out what no record key needs; `keep` parses only the tags asked for |
| meshes | `src/nif.rs` on `tes3::nif` (`nif_summary`, `nif_blocks`, `nif_edit`) | `wraithguard/nif`'s reader, block layouts, geometry and byte-level editor - `report.summarise`, `inspect.inspect_mesh` and `edit.apply_edits` now call the crate |
| plugin reading and writing | `src/esp.rs` (`plugin_records(raw=True)` / `plugin_write`) | the byte layer of `wraithguard/esp` (Reader/Writer, every record's `load`/`save`) - `wraithguard/esp/plugin.py` calls the crate with no JSON in between (so a NaN float round-trips as itself); the record dataclasses stay as the objects the toolkit works with, built by `wraithguard/esp/json.py`. The patch writer and Merged Lands write through it |
| conflict keys | `src/esp.rs` (`plugin_keys`) | `_keys_and_cells` over every record in Python: the conflict scan's `(type, id, deleted)` keys and the cells, computed in Rust from the same fields by the same rules |
| Merged Lands' numeric core | `src/land.rs` (`RelativeGrid`, `merge_grids`, `limit_slopes`, `resolve_cell_normals`, `vertex_normals_from_heights`, `decode_heights`) | the per-vertex Python of `wraithguard/land/` (diff, merge, slope, heights, normals); settings, strategies and reports stay in Python |
| texture decoding | `src/img.rs` (`dds_decode`, `dds_uncompressed`, `tga_decode`, `image_measure`, `image_difference`) | the per-pixel Python of `wraithguard/images` - the DXT1-5, BC4, BC5 and BC7 block decoders, uncompressed DDS, Targa, and the texture-comparison loops (about 100x faster: a 2048px BC7 went from ~12 s to ~70 ms). Header parsing, mip selection and the error types stay in Python |
| load-order lint scan | `src/lint.rs` (`PluginLinter`) | the `_lint_*` raw-record helpers in `wraithguard_toolkit.py` - one pass per plugin returning header gaps, interior cells and fog, interior path grids, evil GMSTs, masters and expansion-only script calls. The warning text, accumulators and skip lists stay in Python |
| tes3conv-format JSON | `src/esp.rs` (`plugin_json`, `plugin_json_file`, `plugin_records_json`, `plugin_records_bytes`) | kept for the "Dump records as JSON" export, the round-trip tools and tests |

## Build

Needs Rust 1.88+ (the crates are edition 2024):

    pip install ./native

On Windows, `.\tools\setup_dev_env.ps1 -NativeOnly` rebuilds it into every dev venv
(see `PREFLIGHT.md`). The CI and release workflows run `pip install ./native` before
the tests and before PyInstaller.
The module declares free-threaded support, so the 3.14t builds keep their threads.

## Test

    cargo test --manifest-path native/Cargo.toml

To build against a local checkout of the crates instead of GitHub, add:

    --config 'patch."https://github.com/Greatness7/tes3".tes3.path="../../tes3-main"'
    --config 'patch."https://github.com/Greatness7/tes3".bytes_io.path="../../tes3-main/libs/bytes_io"'

Do not commit a `Cargo.lock` produced that way: a patched build drops the
`source = "git+https://github.com/Greatness7/tes3..."` lines, and CI's `--locked`
builds then fail. Check with `cargo metadata --locked --manifest-path native/Cargo.toml`.
