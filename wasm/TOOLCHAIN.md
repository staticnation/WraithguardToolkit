# Setting up Rust and WebAssembly on Windows

Everything here is Apache-2.0 OR MIT. Nothing in this file has been run on a
Windows machine by the person who wrote it — it is assembled from the official
documentation, and the verification steps after each stage are there because of
that, not as ceremony.

## 1. Build tools

Rust on Windows needs a linker, and the MSVC one is what `rustup` defaults to.
Install **Visual Studio Build Tools** and select *Desktop development with C++*:

<https://visualstudio.microsoft.com/visual-cpp-build-tools/>

You do not need the full Visual Studio IDE. If you already have VS 2019 or 2022
with C++ support, you already have this.

*Alternative:* `rustup` can use the GNU toolchain instead and skip this, but the
MSVC target is the better-supported default on Windows and the one every guide
assumes. Take MSVC unless you have a reason not to.

## 2. Rust

<https://rustup.rs> — download and run `rustup-init.exe`, accept the defaults.

Close and reopen PowerShell afterwards so `PATH` picks it up. Then:

```powershell
rustc --version
cargo --version
```

Both should print a version. If `rustc` is not found, the terminal predates the
install — reopen it rather than debugging anything.

## 3. The WebAssembly target

```powershell
rustup target add wasm32-unknown-unknown
rustup target list --installed
```

The second command should list `wasm32-unknown-unknown` alongside your host
target. This is a *compilation* target, not a program; there is nothing to run.

## 4. wasm-pack

```powershell
cargo install wasm-pack
wasm-pack --version
```

This compiles from source and takes a few minutes the first time.

**On getting the right one.** `wasm-pack` has changed organisation three
times — `ashleygwilliams` → `rustwasm` → `drager` → `wasm-bindgen`. Those are
real former homes rather than typosquats, which makes them *more* likely to be
mistaken for current. `cargo install wasm-pack` resolves through crates.io and
is the safe route; if you want the source, the authoritative pointer is the
`repository` field on:

<https://crates.io/api/v1/crates/wasm-pack>

which is what the maintainer publishes from. Today that is
<https://github.com/wasm-bindgen/wasm-pack>.

## 5. Build this crate

`Cargo.toml` here expects `tes3` as a sibling checkout:

```
subset sort/
├── tes3-main/                 <- Greatness7/tes3
└── MLOXSubsetSort/wasm/       <- this crate
```

If yours is elsewhere, edit the `nif = { path = ... }` line.

```powershell
cd "C:\Users\l33t3\Desktop\subset sort\MLOXSubsetSort\wasm"
cargo build
```

**Do this before `wasm-pack`.** A plain host build compiles the same code
without the WebAssembly and JS-binding layers, so any error it reports is
about the Rust rather than about the toolchain. Given that this crate was
written against `tes3`'s API without ever being compiled, expect this step to
find things — that is what it is for, and the compiler is a better source of
truth about that API than the person who wrote this was.

Then:

```powershell
wasm-pack build --target no-modules --release
```

Output lands in `pkg/`.

## Why `no-modules` and not `web`

Every tutorial says `--target web`. It is wrong here.

`--target web` emits an **ES module**. ES modules do not load from `file://` —
the origin is `null` and CORS refuses them. The viewer's *export a standalone
page* feature produces exactly such a file, so `web` would work over loopback
and silently break the export. This project has already lost a debugging
session to that same constraint with three.js (`CODE_REVIEW.md` §39).

`--target no-modules` emits a classic script that defines a global, which is
how three.js is already loaded here. Use it unless the export feature is being
dropped.

## Verifying the build without a browser

```powershell
cargo test
```

The crate is built as both `cdylib` and `rlib` precisely so this works: `rlib`
lets the parsing logic be tested on the host, where failures are readable, and
`cdylib` is what WebAssembly needs. Testing in a browser first means debugging
Rust through a JS console, which is a worse instrument.

## What to do once it builds

**Not a feature.** Run the same mesh through both readers and compare object
counts and per-shape triangle counts against `mlox_subset.nif`. Every value
this crate exposes was chosen to be one the Python reader also produces.

If they agree across the corpus, the bridge is trustworthy and geometry
extraction is worth writing. If they disagree, that disagreement is worth
considerably more than the feature would have been — it means one of two
independent implementations is wrong about a real file, and finding out which
is exactly the kind of thing that has produced every genuine fix in this
project.

## If you decide not to bother

That is a reasonable outcome and worth stating. The WebAssembly route only
helps the 3D viewer; the conflict scan, the CSV report and the mesh digests are
headless Python and always will be. The Python reader currently fails on 9 of
80,197 mod meshes with zero divergence against an independent scan. The
argument for WebAssembly is speed and deleting the geometry-shipping plumbing,
not correctness — and it costs a Rust toolchain in the build for everyone who
ever rebuilds the app.
