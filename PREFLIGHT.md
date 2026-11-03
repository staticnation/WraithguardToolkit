# Preflight checklist

What to run before a push, how to push a normal change, a test build and a release,
and how to redo a build that failed. The commands are for PowerShell on Windows,
from the repo folder (`WraithguardToolkit`).

## What runs where

| Workflow | Runs on | What it does |
|---|---|---|
| `ci.yml` | every push, any branch; pull requests | every crate's tests, clippy with `-D warnings`, the command layer compiled without Tauri, and the page tests against `wg-view-serve` (the headless engine) |
| `build-app.yml` | `build/**` branches, `v*` tags, manual | the release packages: Windows (`.zip`, and `-webview2.zip` when a WebView2 runtime is named), Linux (`.tar.gz` and the `.AppImage`), macOS (Apple Silicon and Intel); each with the program, the command line and the documents. On a tag it attaches them to the GitHub Release |

## 0. One-time setup

- **Rust**, current stable, through [rustup](https://rustup.rs). The crates say 1.89 at
  least; CI's clippy is the current stable's, so a newer local Rust can flag lints CI has
  not met yet (and the other way round: update with `rustup update`).
- **Windows**: the Visual Studio Build Tools with the "Desktop development with C++"
  workload ("linker `link.exe` not found" means they are missing), and the WebView2
  runtime (already there on Windows 10 and 11 with Edge).
- **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev`.
- **Node 22**, for the page tests.
- **The Tauri CLI**, only to build the AppImage locally: `cargo install tauri-cli --locked`.

If PowerShell refuses to run the scripts in `tools\` ("running scripts is disabled"),
allow local scripts once with `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`.

## 1. Before every push

### The checks, in CI's order

One script builds the program and runs every Rust check CI runs, then prints a
pass/fail table. A step that fails does not stop the others; the exit code is 1 if any
failed.

```powershell
.\tools\build_and_check_rust.ps1                   # the program and every Rust check
.\tools\build_and_check_rust.ps1 -Pages            # ...plus the page tests
.\tools\build_and_check_rust.ps1 -SkipViewer       # the checks without the slow release build
.\tools\build_and_check_rust.ps1 -Only cli,clippy  # just these steps
.\tools\build_and_check_rust.ps1 -ClippyReport     # list clippy's warnings without failing
```

By hand, one crate: `cargo test --manifest-path viewer-shell\<crate>\Cargo.toml`
(`luacore` with `--features teal`), and its clippy:
`cargo clippy --all-targets --manifest-path viewer-shell\<crate>\Cargo.toml -- -D warnings`.

`viewer-shell\physcore` (the Physics drop) is not in CI's list yet; after changing it,
run `cargo test --manifest-path viewer-shell\physcore\Cargo.toml` too.

### The page tests

Needed after changing `viewer-shell\ui\` or the viewer's commands (`-Pages` above runs
them):

```powershell
cargo build --release --manifest-path viewer-shell\check-commands\Cargo.toml --bin wg-view-serve
cd viewer-shell\ui\tests
npm ci --no-audit --no-fund
npm run preview                          # the preview's loading races
node boot.js                             # the cell viewer boots
$env:WG_MESH_VIEW=1; node boot.js; Remove-Item Env:WG_MESH_VIEW   # the mesh viewer
$env:WG_SETUP=1; node boot.js; Remove-Item Env:WG_SETUP           # a first start opens Setup
node editor\run.js                       # the page's own tests (editor\*.test.js)
cd ..\..\..
```

Each `boot.js` run must end with `page errors: 0`.

After adding a page file to `viewer-shell\ui\ORDER`, list it in `viewer-shell\MANIFEST.md`
too.

### Look at what you are about to commit

```powershell
git status
```

- **New files** (`??`) you meant to add? New source files are easy to miss. `git add -A`
  takes everything.
- **Nothing that should not go in**: build output (`target\`, `ui-dist\`), logs, your
  settings and caches (`wraithguard_toolkit_settings.json`, `wraithguard_cellviewer.*`,
  `wraithguard_record_cache.sqlite3*`, `plugin_cache\`), any `*.key`. `.gitignore`
  covers these; if something like that shows up, add it to `.gitignore` rather than
  committing it.
- **Lockfiles**: after changing a `Cargo.toml`, commit the `Cargo.lock` next to it. Each
  crate in `viewer-shell\` that is not a workspace member has its own.
- **Never commit a lockfile built against a local copy of the tes3 or merge_to_master
  crates.** In a lockfile built that way, their entries (`tes3`, `esp`, `nif`, `bsa`,
  `bytes_io`, `merge_to_master`) have no `source = "git+https://github.com/..."` line.
  `git diff -- '*Cargo.lock'` shows it.

## 2. The push sequences

### A normal change (main)

```powershell
git switch main
git pull
# ... work, then the checks in section 1 ...
git add -A
git status
git commit -m "What changed and why"
git push
```

CI runs on the push. Watch it under **Actions** on GitHub.

### A test build (build branch)

A `build/**` branch builds every package, without making a release.

```powershell
git switch main
git pull
git switch -C build/test          # -C: create it, or reset it to here if it exists
git push -u origin build/test --force-with-lease
```

- Or by hand: **Actions → Build the app (Tauri) → Run workflow**, on any branch; it can
  take a WebView2 runtime link for that run. With the GitHub CLI:
  `gh workflow run build-app.yml --ref build/test`.
- The packages are under that run's **Artifacts**.
- **Done with it:**
  ```powershell
  git switch main
  git push origin --delete build/test
  git branch -D build/test
  ```

### A release (tag)

1. **Set the version** in `viewer-shell\Cargo.toml`, `viewer-shell\tauri.conf.json` and
   `viewer-shell\cli\Cargo.toml` (then build once, so the `Cargo.lock`s carry it).
2. **Check the top heading of `CHANGELOG.md`** is that version: the entries since the last
   release gather under it.
3. **Check the repository settings** (**Settings → Secrets and variables → Actions**):
   `WEBVIEW2_FIXED_URL` (variable), the WebView2 Fixed Version `.cab` link for the
   `-webview2` Windows package. Refresh it now and then; Microsoft replaces it with each
   runtime release. Without it, that package is skipped.
4. **Commit, push, and tag:**

   ```powershell
   git switch main
   git pull
   git add -A
   git commit -m "Release 5.1.0"
   git push
   # wait for CI to pass on main, then:
   git tag v5.1.0
   git push origin v5.1.0
   ```

The tag builds every package and attaches them to a GitHub Release named after the tag.

## 3. When a build fails

### It was GitHub, not the code

A runner hiccup, a download timeout, or the cache: re-run it.

- On GitHub: open the run → **Re-run failed jobs**.
- With the CLI: `gh run list --limit 5`, then `gh run rerun <run-id> --failed`.

The run uses the same commit, and the same tag for a release build.

### It needs a code fix: a normal or build branch

Fix, commit, push. The push starts CI (and the builds, on a `build/**` branch):

```powershell
git add -A
git commit -m "Fix the build"
git push
```

**Just re-run without a new change:**

```powershell
git commit --allow-empty -m "Rebuild"
git push
```

**Keep history clean on a build branch** by folding the fix into the last commit
and overwriting the branch:

```powershell
git add -A
git commit --amend --no-edit
git push --force-with-lease
```

- **`--force-with-lease`, not `--force`:** it refuses to overwrite if the branch
  on GitHub has commits you do not have.
- **Only rewrite build branches.** Never rewrite `main`: add a new commit instead.

**Point a build branch at the current main:**

```powershell
git switch build/test
git reset --hard main
git push --force-with-lease
```

### It needs a code fix: a release tag

A tag names one commit, so the fix needs the tag moved to the fixed commit.

1. **Fix on main** as a normal change and push it. Wait for CI to pass.
2. **Delete the failed release** on GitHub: **Releases → the release → Delete**.
   Otherwise the new assets are added next to the old ones.
3. **Move the tag** to the fixed commit and push it again:

```powershell
git switch main
git pull
git tag -d v5.1.0                         # delete it here
git push origin :refs/tags/v5.1.0         # delete it on GitHub
git tag v5.1.0                            # re-create it on the fixed commit
git push origin v5.1.0                    # starts the release builds again
```

If people may already have downloaded the release, bump to the next version
(`v5.1.1`) instead of moving the tag.

### If a push is rejected

- **`rejected (fetch first)` / `non-fast-forward`:** GitHub has commits you do not
  have. On main, run `git pull`, then `git push`. On a build branch you mean to
  overwrite, `git push --force-with-lease`.
- **`stale info` from `--force-with-lease`:** someone pushed to the branch since you
  last fetched. Run `git fetch`, look at what they pushed, then decide.
