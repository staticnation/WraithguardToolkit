# To do

Open work, in no particular order. Done items move to `CHANGELOG.md`.

## Forks: LUAL, Clippy

The build takes tes3 and merge_to_master from `staticnation/tes3` and
`staticnation/merge_to_master` (the `[patch]` sections in native/, viewer-shell/,
viewcore/ and check-commands' Cargo.toml). Both are pushed and tested: tes3 carries
Greatness7/tes3#7 (LUAL) and #9 (CI with Clippy, passing); merge_to_master carries the
LUAL merge (`types/lua.rs`) and the LUAD walker (`types/luad.rs`), with its
TB_BM dialogue test passing against a baseline recorded from upstream 5ea27f1
(`tests/make_baselines.rs`). Still to do:

- Offer merge_to_master's LUAL work upstream (a PR to Greatness7/merge_to_master, once
  tes3#7 is merged there), and ask greatness7 for the `ignore/` pairs behind the
  `MW`, `MW_TB`, `MW_BM` and `MW_TB_BM` dialogue tests.
- When both are upstream: delete the `[patch]` sections and point merge_to_master back
  at Greatness7.
- Clippy for our own crates (native, viewcore, viewer-shell) in
  `tools/build_and_check_rust.ps1` and CI, warnings first, `-D warnings` once clean.

## OpenMW Lua tools

The load order's Lua scripts, read from each mod's `.omwscripts` lists (plain text,
`CONTEXT: path` lines): a script browser with syntax highlighting; a parsed tree (AST)
for each script; conflict detection (two mods registering the same script path, the
same interface name, the same event or engine handler, or overriding each other's
files in the VFS); and performance diagnostics (per-frame handlers such as `onUpdate`
and `onFrame`, loops over `nearby.actors` / `world.activeActors` / `types.*.records`,
queries run every frame). The OpenMW Lua API changes between releases, so API checks
should follow the version the setup's OpenMW reports rather than one fixed list.
Reference: https://openmw.readthedocs.io/en/openmw-0.51.0/reference/lua-scripting/api.html
(one page per OpenMW release; pick the one matching the setup).
Teal (a typed dialect of Lua that compiles to plain Lua) is worth supporting beside
plain Lua, for mods written in it: https://teal-language.org/book/latest/index.html
The Teal compiler's source (tl 0.24.8, MIT) is beside the repository in
`subset sort\tl-0.24.8`, and its build tool Cyan (0.4.1) in `subset sort\cyan-0.4.1`.
htl 0.12.0 (`subset sort\htl-0.12.0`, https://github.com/ynishi/htl, MIT or Apache-2.0) runs Teal from Rust through
mlua - type checking, linting and formatting without luarocks - which would let the
Rust side (native or viewcore) check scripts without a Lua install.

## Release notes for the next version

When the Unreleased section is released: rename it (PREFLIGHT.md, "Release"), and
write the GitHub release text from it - the water (MGE XE's own waves, dynamic
ripples, rain ripples, Wonders of Water), weather, bloom, shadows, the native merge,
and that downloads are no longer minisign-signed (check the SHA-256 on the Release
page instead).
