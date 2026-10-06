# Preflight checklist

What to run before a push, how to push a normal change, a test build and a release,
and how to redo a build that failed. The commands are for PowerShell on Windows,
from the repo folder (`WraithguardToolkit`).

## What runs where

| Workflow | Runs on | What it does |
|---|---|---|
| `ci.yml` | every push, any branch; pull requests | the checks below, on Linux, under Python 3.14 and 3.14t; the viewer's engine tests; the page boot tests; the GUI smoke tests |
| `build-linux.yml` | `build/**` branches, `v*` tags, manual | the plain Linux `.tar.gz` (needs the system's WebKitGTK 4.1) |
| `build-flatpak.yml` | `build/**` branches, `v*` tags, manual | the `.flatpak` (Steam Deck; set `PBS_TAG` to pin its Python) |
| `build-macos.yml` | `build/**` branches, `v*` tags, manual | the `.app`, Intel and Apple Silicon |
| `build-windows.yml` | `v*` tags, manual only | the `.exe`, and the zip with WebView2 bundled (needs the `WEBVIEW2_FIXED_URL` variable) |

A `build/**` branch does **not** build Windows. Start that one by hand (see
[A test build](#a-test-build-build-branch)).

## 0. One-time setup

The regular and the free-threaded Python share one `site-packages` on Windows, so
each gets its own environment. CI tests both, so both are worth having.

**The short way** -- one script does everything in this section (creates both
venvs, installs the dev tools, builds the Rust backend into each, and checks it
loads):

```powershell
.\tools\setup_dev_env.ps1                    # first time, or a full refresh
.\tools\setup_dev_env.ps1 -NativeOnly        # after changing native/
.\tools\setup_dev_env.ps1 -FreeThreadedOnly  # only .venv-t
```

If PowerShell refuses to run it ("running scripts is disabled"), allow local
scripts once with `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`.

**By hand**, the same steps:

```powershell
# Free-threaded 3.14t -- the one the app ships. Your main environment.
py -3.14t -m venv .venv-t
.venv-t\Scripts\python -m pip install -e .[dev]
.venv-t\Scripts\python -m pip install ./native

# Regular 3.14 -- CI's other interpreter. Optional, for when a test fails only there.
py -3.14 -m venv .venv
.venv\Scripts\python -m pip install -e .[dev]
.venv\Scripts\python -m pip install ./native
```

Also needed: Rust 1.88+ (`rustup`), and Node 22 for the viewer page tests.

In the commands below, `py` means `.venv-t\Scripts\python`. Activate the venv once
per window with `.venv-t\Scripts\Activate.ps1` and you can type `python` instead.

### The Rust backend (`wraithguard_native`)

The images, the lint, Merged Lands and the plugin reader/writer all need it; the
tests that use it skip or fail without it. Each venv needs its own build: a module
built for 3.14t does not load in 3.14, and the other way round.

```powershell
# Install or rebuild, in each venv you use:
.venv-t\Scripts\python -m pip install --force-reinstall --no-deps ./native
.venv\Scripts\python -m pip install --force-reinstall --no-deps ./native

# Check it loaded, and from the right place (the GIL line should say False in .venv-t):
.venv-t\Scripts\python -c "import sys, wraithguard_native as n; print(n.__version__, n.__file__); print('GIL', sys._is_gil_enabled())"
```

- Plain `pip install ./native` also rebuilds and replaces it (pip always rebuilds
  from a folder); `--force-reinstall --no-deps` just makes that explicit and skips
  re-checking other packages.
- **"The module '.venv' could not be loaded"**: that venv does not exist yet --
  PowerShell's way of saying "no such file". Create it first (the setup commands
  above, or `.\tools\setup_dev_env.ps1`).
- **`ImportError` / "DLL load failed"**: the module was built by the other
  interpreter. Rebuild it with that venv's own `python`, as above.
- **Build fails with "linker `link.exe` not found"**: install the Visual Studio
  Build Tools with the "Desktop development with C++" workload, then retry.
- CI, the Docker image and the three build workflows build it themselves
  (`pip install ./native`); nothing to do for those.

## 1. Before every push

### Rebuild what changed

| If you changed | Run |
|---|---|
| `native/` (the Rust backend) | `.\tools\setup_dev_env.ps1 -NativeOnly` (or by hand: see [the Rust backend](#the-rust-backend-wraithguard_native)) |
| `viewer-shell/` | `.\tools\build_and_check_rust.ps1 -Only viewer` (or `cd viewer-shell; cargo build --release; cd ..`) |

### The CI checks, in CI's order

```powershell
python -m ruff check .
python -m black --check .
python -m mypy --platform linux
python tools/check_undefined.py wraithguard_toolkit_gui.py
python tools/check_placeholders.py
python tools/make_pot.py --check
python -m pytest
```

- **`mypy --platform linux`**: CI runs mypy on Linux. On Windows it type-checks the
  Windows branches of platform-specific code and flags things that are fine on
  Linux. `--platform linux` checks what CI checks.
- **black fails?** `python -m black .` fixes it. Then run `--check` again.
- **ruff fails?** `python -m ruff check . --fix` fixes the safe ones.
- **make_pot fails?** You added or changed a translated string. Run
  `python tools/make_pot.py` and commit `locale/wraithguard_toolkit.pot`.

### The Rust checks

One script builds the viewer and runs all of these, then prints a pass/fail table
(add `-Pages` for the page boot tests below, `-SkipViewer` to skip the slow release
build):

```powershell
.\tools\build_and_check_rust.ps1
.\tools\build_and_check_rust.ps1 -Pages
```

By hand:

```powershell
cargo test --manifest-path native/Cargo.toml
cargo test --manifest-path viewer-shell/viewcore/Cargo.toml
cargo build --manifest-path viewer-shell/check-commands/Cargo.toml
cargo build --manifest-path viewer-shell/check/Cargo.toml
```

### The viewer page boot tests

Only needed after changing `viewer-shell/ui/` or the viewer's commands:

```powershell
cargo build --release --manifest-path viewer-shell/check-commands/Cargo.toml --bin wg-view-serve
cd viewer-shell/ui/tests
npm install --no-audit --no-fund
node boot.js
$env:WG_MESH_VIEW=1; node boot.js; Remove-Item Env:WG_MESH_VIEW
cd ../../..
```

Each run must end with `page errors: 0`.

### Look at what you are about to commit

```powershell
git status
```

- **New files** (`??`) you meant to add? New source files are easy to miss. `git add -A` takes everything.
- **Nothing that should not go in**: build output, logs, `cell_map_*.html`, your
  settings, any `*.key`. `.gitignore` covers these; if something like that
  shows up, add it to `.gitignore` rather than committing it.
- **Lockfiles**: after changing a `Cargo.toml`, commit the `Cargo.lock` next to it.
  `native/`, `viewer-shell/`, `viewer-shell/viewcore/` and
  `viewer-shell/check-commands/` each have one.
- **Never commit a lockfile built against a local copy of the tes3 crates.** In
  a lockfile built that way, the `tes3`, `esp`, `nif`, `bsa` and `bytes_io`
  entries have no `source = "git+https://github.com/Greatness7/tes3#..."` line.
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

A `build/**` branch builds the Linux `.tar.gz` and `.flatpak` and the macOS app, without making a release.

```powershell
git switch main
git pull
git switch -C build/test          # -C: create it, or reset it to here if it exists
git push -u origin build/test --force-with-lease
```

- **Windows too:** on GitHub, **Actions → Build Windows app → Run workflow**,
  choose the branch `build/test`. With the GitHub CLI:
  `gh workflow run build-windows.yml --ref build/test`.
- The builds are under that run's **Artifacts** (kept 14 days).
- **Done with it:**
  ```powershell
  git switch main
  git push origin --delete build/test
  git branch -D build/test
  ```

### A release (tag)

1. **Set the version** in `pyproject.toml`, `wraithguard/__init__.py` (`__version__`),
   the "current" line in `README.md`, and a new `<release>` at the top of
   `packaging/flatpak/io.github.staticnation.WraithguardToolkit.metainfo.xml`; then run
   `python tools/make_pot.py` so the `.pot` header carries it too.
2. **Rename `## Unreleased`** at the top of `CHANGELOG.md` to that version.
3. **Check the repository settings** (**Settings → Secrets and variables →
   Actions**):
   - `WEBVIEW2_FIXED_URL` (variable): the WebView2 Fixed Version `.cab` link.
     Refresh it now and then; Microsoft replaces it with each runtime release.
   - `MINISIGN_SECRET_KEY` / `MINISIGN_PASSWORD`: no longer used (signing is
     retired) and can be deleted.
4. **Commit, push, and tag:**

   ```powershell
   git switch main
   git pull
   git add -A
   git commit -m "Release 4.3.0"
   git push
   # wait for CI to pass on main, then:
   git tag v4.3.0
   git push origin v4.3.0
   ```

The tag builds Windows (both variants), Linux (the `.tar.gz` and the `.flatpak`) and
macOS, and attaches them to a GitHub Release named after the tag. The Linux build writes the release
notes.

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
git tag -d v4.3.0                         # delete it here
git push origin :refs/tags/v4.3.0         # delete it on GitHub
git tag v4.3.0                            # re-create it on the fixed commit
git push origin v4.3.0                    # starts the release builds again
```

If people may already have downloaded the release, bump to the next version
(`v4.2.2`) instead of moving the tag.

### If a push is rejected

- **`rejected (fetch first)` / `non-fast-forward`:** GitHub has commits you do not
  have. On main, run `git pull`, then `git push`. On a build branch you mean to
  overwrite, `git push --force-with-lease`.
- **`stale info` from `--force-with-lease`:** someone pushed to the branch since you
  last fetched. Run `git fetch`, look at what they pushed, then decide.
