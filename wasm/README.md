# The WASM bridge, and how to build it

**Status: written, never compiled.** The sandbox this was authored in has no
Rust toolchain and no root to install one, so nothing here has been built, run
or tested. Treat it as a starting point that will probably need small fixes on
first `cargo build`, not as working code. Everything else in this repository
has been verified against something; this has not, and saying so is cheaper
than letting someone discover it.

## What this is for

`Greatness7/tes3` reads NIF in Rust, is MIT, and compiles to WebAssembly. The
3D viewer is already a browser page. So the mesh could be parsed *in the page*
from the raw `.nif`, instead of Python parsing it and shipping packed geometry
blobs over loopback.

That would **remove** code rather than add it - `_mesh_payload`, `_packed`, and
the geometry-retention path in `wraithguard/nif/reader.py` all exist to get
geometry into the page. It also keeps the loopback server's model intact: it
still publishes explicit blobs by token, just different bytes.

**It does not replace the Python reader.** The conflict scan, the CSV report,
`analyse_mesh_conflicts` and the mesh digests are headless and always will be.

## The property worth having

If the viewer parses with `tes3` and the scan parses with `wraithguard.nif`,
the two cross-check each other **in ordinary use**. A divergence shows up as
"the 3D view disagrees with the report" - a standing differential test that
costs nothing to run and needs no corpus.

That is also why this first version exposes object counts and per-shape vertex
and triangle counts and nothing else: every one of those is a number the Python
reader already produces. **Build the differential check before the feature.**

## Toolchain

**See `TOOLCHAIN.md`** in this folder for the full Windows setup, including the
MSVC build tools Rust needs for a linker, and why the build must use
`--target no-modules` rather than the `--target web` every tutorial suggests.

The short version:

```powershell
# rustup from https://rustup.rs, then:
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
cargo build                                    # host build first -- readable errors
wasm-pack build --target no-modules --release
```

## What was deliberately left out

A fuller version of `lib.rs` was written and then deleted: world-space
transform composition, triangle flattening, texture path extraction. It guessed
at `tes3`'s API in three places and was **wrong in two** -
`shape.base.base.name` where `Deref` makes it `shape.name`, and
`property.base_map` where the field is `texture_maps: Vec<Option<TextureMap>>`.
A third guess, `NiLink::from_index`, appears not to exist.

All were caught by reading `tes3`'s source, which is exactly how much
confidence untestable code deserves. Guessing at an API that cannot be compiled
produces work whose errors are invisible until someone else finds them, so what
remains uses only calls verified against the source. **Write the geometry
extraction with a compiler available**; it will answer in seconds what this had
to guess at.

## Vendoring

Put the built artifact beside three.js in `wraithguard/nif/assets/`, with its
licence file, and add it to the PyInstaller `--add-data` list the same way -
`assets/` is data, not imports, so it is not collected automatically.

Record it in `CREDITS.md`. Note that the shipped artifact is *compiled output*
of an MIT library, not a vendored copy of the library, which is a different and
simpler attribution question than three.js.

## First thing to do after it builds

Not a feature. Run the same mesh through both readers and compare object
counts and per-shape triangle counts. If they agree across the corpus, the
bridge is trustworthy and the geometry work can start. If they do not, the
disagreement is worth more than the feature would have been.
