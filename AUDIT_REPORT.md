# Wraithguard Toolkit — Full Audit

**Date:** 2026-08-04 · **Version audited:** 3.1.2 · **Environment:** fresh checkout,
Python 3.10.12, Linux, no Tk, no network.

**Tools:** ruff 0.16.1, black 26.5.1, mypy 1.14.1 and 1.8.0, pytest 9.1.1 +
pytest-cov, plus the project's own `tools/` gates.

> **Update — all findings resolved; re-verified at 3.1.3.** Every issue below
> has since been fixed, and the gates were re-run on a clean checkout after the
> 3.1.3 work (the Merged Lands world-map fix, the `[NearEnd]` ordering fix, the
> backups-scan fix, and the documentation consolidation). Current gate state:
> **ruff clean, black clean (182 files), mypy clean (0 errors, 110 files, on
> both 1.8.0 and 1.14.1), 3,289 passed / 3 skipped, `.pot` current (640
> messages), check_undefined / check_placeholders ok.** See the "Resolution"
> section at the end for what the original 3.1.2 audit changed; the narrative
> below is that audit, kept as the record.

## Bottom line

The code is in genuinely good shape: **black is clean, the full test suite is
green (3,202 passed / 3 skipped), branch coverage is 79%, and no runtime bugs
were found.** Every ruff and mypy finding below is runtime-safe.

The real issue is **drift between the tree and its own documentation.** Several
gates the docs describe as passing "with zero findings" no longer do on a clean
checkout — not because the code regressed, but because the toolchain moved and
nothing pins it, and because the running documentation was updated by hand and
has fallen behind. Nothing here blocks use of the tool; it's cleanup and
honesty-of-record work.

## Gate results

| Gate | Doc claims | Actual | Verdict |
|------|-----------|--------|---------|
| `black --check .` | clean | clean, 179 files | ✅ matches |
| `pytest` | 1,273 / 800 tests | 3,202 passed, 3 skipped | ✅ green, ❌ count stale |
| coverage floor 52 | measured 54% | 79.3% | ✅ passes, baseline stale |
| `check_undefined.py` | ok | ok | ✅ |
| `check_placeholders.py` | ok | ok (108 files) | ✅ |
| `ruff check .` | zero findings | **69 findings** | ⚠️ see below |
| `mypy` | green, gates all files | **30 errors** | ⚠️ see below |
| `make_pot.py --check` | ".pot must be current" | **out of date** | ❌ regenerate |

## Finding 1 — ruff: 69 findings, both benign, both toolchain drift

All 69 fall in exactly two rules, and both rules sit *inside categories the
project already selects* — they were stabilized in ruff after the config was
last validated:

- **ISC004 ×50** (`implicit-string-concatenation-in-collection`) — long
  user-facing / HTML strings deliberately wrapped across lines, already
  correctly parenthesized as tuple elements. This is the false-positive-prone
  sibling of ISC001, which the config *already* ignores as "formatter-owned."
  Concentrated in `tools/gen_merged_lands_table.py` (35) and the GUI.
- **PLR0917 ×19** (`too-many-positional-arguments`) — the same complexity
  family as PLR0913 (`too-many-arguments`), which the config already ignores
  with a documented rationale. PLR0917 is a newer split-out of that rule.

Every other selected rule — pycodestyle (E/W), pyflakes (F), bugbear (B),
naming (N), imports (I), pydocstyle/PEP 257 (D), annotations/PEP 484 (ANN),
bandit security (S), and the rest — passes clean. The PEP-conformance core the
project cares about is intact.

**Recommendation:** add `"ISC004"` and `"PLR0917"` to `[tool.ruff.lint].ignore`
(consistent with the existing ISC001 / PLR0913 exemptions and their comments),
and **pin ruff** in a dev-requirements list so the rule set can't shift
silently again.

## Finding 2 — mypy: 30 errors, all runtime-safe false positives

Reproduced identically under **both mypy 1.8.0 (Dec 2023) and 1.14.1 (Dec
2024)** — so this is *not* recent drift; it's persistent and present on any
modern mypy. The documented "mypy-clean / gated" state is **not reproducible on
a fresh checkout.** Breakdown by cause:

- **`type-var` ×22** — `redirect_stdout(QueueWriter(...))` /
  `redirect_stderr(...)`. `QueueWriter` correctly subclasses `io.TextIOBase`,
  but typeshed does not model `io.TextIOBase` as `typing.IO[str]`, so it fails
  the `redirect_*` TypeVar bound. A long-standing typeshed gap, not a code
  defect — the object supplies `write()` and works at runtime. (`gui/conflicts.py`,
  `gui/t3.py`, `wraithguard_toolkit_gui.py`.)
- **`assignment` ×3 + `arg-type` ×1** — `allowed: LandData = ~LandData.NONE`.
  Verified at runtime: `~LandData.NONE` returns a `LandData` (`LandData.B|A`).
  typeshed types `IntFlag.__invert__ -> int`, so mypy sees an `int` default
  against a `LandData` parameter. False positive. (`land/diff.py`,
  `land/landmass.py`, `land/pipeline.py`.)
- **`misc` ×2** — `_conflict_win` "incompatible across base classes." The two
  explicit annotations (`patchwin.py`, `pluginview.py`) are *identical*
  (`tk.Toplevel | None`); this is mypy's strictness about multiple-inheritance
  mixins, not a real conflict.
- **`unused-ignore` ×1** — `wraithguard_toolkit_gui.py:1991` carries a
  `# type: ignore[arg-type]` the current typeshed tk stubs no longer need
  (surfaced by `warn_unused_ignores = true`).
- **`import-untyped` ×1** — `yaml` has no stubs; environmental (appears only
  when PyYAML is installed; the project's `ignore_missing_imports` hides it
  when it isn't). Add `types-PyYAML` to the dev set if you want it gone.

24 of 30 are in the GUI, which the coverage config already excludes as Tk-bound.

**Recommendation:** none of these are bugs, but the tree should be made to pass
`mypy` again so the gate means something. Options: cast/annotate `QueueWriter`
as `IO[str]` at the `redirect_*` call sites (or a `TextIO` protocol), add
targeted `# type: ignore[assignment]` on the `~LandData.NONE` defaults with a
comment, drop the now-stale ignore at line 1991 — and **pin mypy** so a stub
bump can't silently re-break it.

## Finding 3 — `.pot` translation catalog is stale

`python tools/make_pot.py --check` reports
`locale/wraithguard_toolkit.pot is out of date`. This is a real gate failure
the docs list as a required pre-commit check. **Fix:** run
`python tools/make_pot.py` and commit the regenerated catalog.

## Finding 4 — documentation drift (the largest cluster)

The code is ahead of its own prose. Concrete mismatches:

- **Test count is stale in three places.** Actual: **3,205 collected** (3,202
  pass + 3 skip). `pyproject.toml` says `# 800 tests`; `README.md:731` and
  `PROJECT_LAYOUT.md` say "1,273 tests"; `REMAINING_WORK.md` says "1,273 tests:
  1,271 passed, 2 skipped." All three understate reality by ~2.5×.
- **mypy is described as green and complete when it isn't.** `README.md:735`
  ("gates all 54 shipped files"), the latest `CODE_REVIEW.md` status line
  ("mypy (56 files)"), the retired typing brief ("gates all 35 files"), and
  `REMAINING_WORK.md` ("Every gate passes with zero findings") all assert a
  clean mypy that a fresh checkout does not produce (30 errors; mypy actually
  checks 109 source files).
- **ruff is described as "zero findings"** (`REMAINING_WORK.md`) — actually 69.
- **Coverage baseline is stale (harmlessly).** Docs cite "measured at 54%";
  it's now 79.3%. The `fail_under = 52` floor still passes — consider ratcheting
  it up as the config comment itself invites.
- **Skip count.** Docs say "1 deliberate skip" / "2 skipped"; a hermetic
  checkout shows **3** — one deliberate (`test_differential` baseline) and two
  environmental (`test_gui_smoke` needs Tk, `test_nif_serve` needs network).
- **`REMAINING_WORK.md`'s one "outstanding action" is already done.** The nine
  unreferenced files listed for deletion under `CODE_REVIEW.md` §28.1
  (`viz/explorer.py`, `explorer_js.py`, `cellpage.py`, `sidecar.py`,
  `assets.py`, `cache.py`, `draw_js.py`, `detail.py`, `tests/test_viz_client.py`)
  **have all been removed.** The doc should drop the item.

**Version consistency is good:** `wraithguard.__version__`, `pyproject`
`[project].version`, and the `CHANGELOG.md` top entry all agree at 3.1.2, and
`tests/test_standards.py` enforces that mechanically.

## Root cause and the one structural fix

`tests/test_standards.py` deliberately asserts the *configuration* of the ruff
and mypy gates, not the *result* of running them ("the check itself belongs in
CI"). So when ruff/mypy drift, the in-repo suite stays green and the drift is
invisible until someone runs the external tools — which is why the docs could
confidently claim "zero findings" while a clean checkout shows 69 + 30.

Combined with **no version pins for ruff/black/mypy anywhere** (`dependencies`
is empty by design, and there's no dev-requirements file), the toolchain floats
and the gates mean whatever the latest release decides they mean.

**Highest-leverage fix:** add a pinned dev-tool set (e.g. a
`[project.optional-dependencies].dev` or a `requirements-dev.txt`) fixing exact
ruff/black/mypy/pytest versions, and have CI run those. That single change
freezes findings 1 and 2 and keeps the documentation's claims true.

## Suggested priority order

1. Regenerate `.pot` (Finding 3) — real, one command.
2. Pin ruff/black/mypy/pytest versions (root-cause fix).
3. Add `ISC004` + `PLR0917` ignores (Finding 1) — restores a clean ruff.
4. Clear the 30 mypy errors via targeted casts/ignores (Finding 2) — restores a
   clean mypy.
5. Refresh the drifted numbers and remove the completed §28.1 item across
   `README.md`, `PROJECT_LAYOUT.md`, `REMAINING_WORK.md`, the retired typing brief,
   `pyproject.toml`, and `CODE_REVIEW.md` (Finding 4).

*No runtime defects were found. The audited version passes its own test suite
cleanly; the work above is toolchain-pinning, a stale catalog, and record-keeping.*

## Resolution — what was changed

All five findings were fixed and every gate re-verified clean.

**Finding 3 — `.pot`:** regenerated with `tools/make_pot.py` (613 messages);
`--check` passes.

**Root cause / Finding 1&2 — pinned toolchain:** added a `[project.optional-
dependencies].dev` extra to `pyproject.toml` pinning `ruff==0.16.1`,
`black==26.5.1`, `mypy==1.14.1`, `pytest==9.1.1`, `pytest-cov==7.1.0`, and
`types-PyYAML==6.0.12.20260724`. `pip install -e .[dev]` now reproduces the exact
tools the standards are measured against. README's Developing section points at
it.

**Finding 1 — ruff:** added `ISC004` and `PLR0917` to `[tool.ruff.lint].ignore`,
each beside its sibling rule (`ISC001`, `PLR0913`) with a rationale comment.
`ruff check .` → *All checks passed!*

**Finding 2 — mypy (30 → 0), fixed at the source rather than suppressed:**

- *22 `type-var`* — added `QueueWriter.as_stream()` in `wraithguard/gui/widgets.py`,
  a one-line `cast("TextIO", self)` with a docstring explaining the typeshed
  gap, and routed all 11 `redirect_stdout(...)/redirect_stderr(...)` call sites
  (in `gui/conflicts.py`, `gui/t3.py`, `wraithguard_toolkit_gui.py`) through it.
- *4 `assignment`/`arg-type`* — added `ALL_LAYERS: Final[LandData] =
  LandData(~LandData.NONE)` in `wraithguard/land/diff.py` (the `LandData()`
  wrapper restores the type `IntFlag.__invert__` drops; value unchanged) and
  used it as the default in `diff.py`, `landmass.py` (×2) and `pipeline.py`.
- *2 `misc`* — declared `_conflict_win: tk.Toplevel | None` in
  `ConflictWindowsMixin`'s host-contract block (`gui/conflicts.py`), matching the
  two sibling mixins so the inherited definitions agree.
- *1 `unused-ignore`* — removed the stale `# type: ignore[arg-type]` at
  `wraithguard_toolkit_gui.py:1991`.
- *1 `import-untyped`* — resolved by the pinned `types-PyYAML` stub.

`python -m mypy` → *Success: no issues found in 109 source files* on both mypy
1.8.0 and 1.14.1.

**Finding 4 — doc numbers:** corrected across `pyproject.toml`, `README.md`,
`PROJECT_LAYOUT.md`, the retired typing brief, and `REMAINING_WORK.md` — test count
(→ 3,202 / 3,205), mypy file count (→ 109), `.pot` message count (→ 613), skip
count (→ 3, with the split explained), the completed §28.1 item removed, and the
"every gate passes" status made accurate.

**Final gate state (clean checkout):** ruff clean · black clean (179 files) ·
mypy clean (0 errors, 109 files) · 3,202 passed / 3 skipped · coverage 79% ·
`.pot` current · check_undefined ok · check_placeholders ok.

*Not touched:* `CODE_REVIEW.md`'s historical running log (its dated entries are
a changelog and were left as the record); the `fail_under = 52` coverage floor
(passes at 79% — a candidate to ratchet up, left as a judgment call).
