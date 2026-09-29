"""The Rust core of Merged Lands against the Python it replaced (native/src/land.rs).

The per-vertex work of ``wraithguard/land`` -- building and merging relative grids,
the slope limiter, vertex normals, height decoding -- moved to Rust. The Python it
replaced is kept here, verbatim in substance, as the reference: every strategy, on
random terrain, must give the same result to the unit. (A full Merged Lands run on
a 4,000-cell landmass also wrote the byte-identical plugin; see the changelog.)
"""

from __future__ import annotations

import math
import random
import struct
from array import array

import pytest

wraithguard_native = pytest.importorskip("wraithguard_native")

from wraithguard.land.diff import LandData, RelativeGrid  # noqa: E402
from wraithguard.land.heights import vertex_normals_from_heights  # noqa: E402
from wraithguard.land.merge import ConflictParams, ConflictStrategy, merge_layer  # noqa: E402
from wraithguard.land.slope import MAX_PASSES, MAX_STEP, limit_slopes  # noqa: E402

SIDE = 65
HEIGHT_SCALE = 8

# ---------------------------------------------------------------------------
# The Python reference implementations


def _ref_average(first: int, second: int, p: ConflictParams) -> tuple[int, bool]:
    s1, s2 = abs(first), abs(second)
    total = s1 + s2
    if total == 0:
        return 0, False
    weight = s1 / total
    biased = weight**1.5
    other = (1.0 - weight) ** 1.5
    weight = biased / (biased + other)
    blended = weight * first + (1.0 - weight) * second
    smaller = min(first, second)
    threshold = min(
        max(p.minor_threshold_pct * smaller, p.minor_threshold_min), p.minor_threshold_max
    )
    return int(blended), abs(smaller - blended) >= threshold


def _ref_weighted(
    first: int, second: int, w1: float, w2: float, p: ConflictParams
) -> tuple[int, bool]:
    total = w1 + w2
    if total <= 0.0:
        return _ref_average(first, second, p)
    share = w1 / total
    blended = share * first + (1.0 - share) * second
    smaller = min(first, second)
    threshold = min(
        max(p.minor_threshold_pct * smaller, p.minor_threshold_min), p.minor_threshold_max
    )
    return int(blended), abs(smaller - blended) >= threshold


def _ref_normal_at(rows: list[list[float]], x: int, y: int) -> tuple[float, float, float]:
    limit = len(rows) - 1
    fx = x - 1 if x == limit else x
    fy = y - 1 if y == limit else y
    step = 128.0 / HEIGHT_SCALE
    here = rows[fy][fx] / HEIGHT_SCALE
    east = rows[fy][fx + 1] / HEIGHT_SCALE
    north = rows[fy + 1][fx] / HEIGHT_SCALE
    nx, ny, nz = -(east - here) * step, -(north - here) * step, step * step
    length = math.sqrt(nx * nx + ny * ny + nz * nz)
    return (nx / length, ny / length, nz / length)


def _ref_curvature(rows: list[list[float]], x: int, y: int) -> float:
    here = _ref_normal_at(rows, x, y)
    total, counted = 0.0, 0
    for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1)):
        nx, ny = x + dx, y + dy
        if not (0 <= nx < len(rows) and 0 <= ny < len(rows)):
            continue
        o = _ref_normal_at(rows, nx, ny)
        total += math.acos(max(-1.0, min(1.0, here[0] * o[0] + here[1] * o[1] + here[2] * o[2])))
        counted += 1
    return total / counted if counted else 0.0


def _ref_merge(
    first: tuple[list[int], list[int], list[bool]],
    second: tuple[list[int], list[int], list[bool]],
    components: int,
    strategy: str,
) -> tuple[list[int], list[bool], tuple[int, int, int, int, int]]:
    reference, d1, c1 = first
    _, d2, c2 = second
    p = ConflictParams()
    delta = [0] * len(reference)
    changed = [False] * (SIDE * SIDE)
    one = two = contested = minor = major = 0
    rows = None
    if strategy == "curvature":

        def as_rows(flat: list[int]) -> list[list[float]]:
            return [[float(v) for v in flat[y * SIDE : (y + 1) * SIDE]] for y in range(SIDE)]

        rows = (
            as_rows(reference),
            as_rows([a + b for a, b in zip(reference, d1)]),
            as_rows([a + b for a, b in zip(reference, d2)]),
        )

    def put(v: int, deltas: list[int]) -> None:
        delta[v * components : (v + 1) * components] = deltas
        changed[v] = any(d != 0 for d in deltas)

    for y in range(SIDE):
        for x in range(SIDE):
            v = y * SIDE + x
            a = d1[v * components : (v + 1) * components]
            b = d2[v * components : (v + 1) * components]
            if not c1[v] and not c2[v]:
                continue
            if c1[v] and not c2[v]:
                one += 1
                put(v, a)
                continue
            if c2[v] and not c1[v]:
                two += 1
                put(v, b)
                continue
            contested += 1
            if strategy == "overwrite":
                put(v, b)
                continue
            if strategy == "ignore":
                put(v, a)
                continue
            worst = False
            if rows is not None:
                before = _ref_curvature(rows[0], x, y)
                add1 = max(0.0, _ref_curvature(rows[1], x, y) - before)
                add2 = max(0.0, _ref_curvature(rows[2], x, y) - before)
                w1 = abs(a[0]) * (1.0 + add1 * 8.0)
                w2 = abs(b[0]) * (1.0 + add2 * 8.0)
                value, worst = _ref_weighted(a[0], b[0], w1, w2, p)
                blended = [value]
            else:
                blended = []
                for i, j in zip(a, b):
                    value, sev = _ref_average(i, j, p)
                    blended.append(value)
                    worst = worst or sev
            put(v, blended)
            if worst:
                major += 1
            else:
                minor += 1
    return delta, changed, (one, two, contested, minor, major)


# ---------------------------------------------------------------------------


def _terrain(rng: random.Random, base: list[int], spread: int, share: float) -> list[int]:
    out = list(base)
    for i in range(len(out)):
        if rng.random() < share:
            out[i] += rng.randint(-spread, spread)
    return out


def _smooth(rng: random.Random) -> list[int]:
    return [
        int(400 * math.sin(x / 9.0) * math.cos(y / 7.0)) + rng.randint(-20, 20)
        for y in range(SIDE)
        for x in range(SIDE)
    ]


@pytest.mark.parametrize("strategy", ["overwrite", "ignore", "resolve", "curvature"])
@pytest.mark.parametrize("seed", [1, 2, 3])
def test_merge_matches_the_python(strategy: str, seed: int) -> None:
    rng = random.Random(seed)  # noqa: S311 - test terrain, not security
    reference = _smooth(rng)
    first = RelativeGrid.from_difference(reference, _terrain(rng, reference, 300, 0.4), SIDE)
    second = RelativeGrid.from_difference(reference, _terrain(rng, reference, 900, 0.4), SIDE)

    merged, report = merge_layer(LandData.VERTEX_HEIGHTS, first, second, ConflictStrategy(strategy))

    def parts(grid: RelativeGrid) -> tuple[list[int], list[int], list[bool]]:
        flat = grid.to_flat()
        ref = grid.to_flat_reference()
        delta = [a - b for a, b in zip(flat, ref)]
        return ref, delta, [grid.has_difference(i % SIDE, i // SIDE) for i in range(SIDE * SIDE)]

    delta, changed, counts = _ref_merge(parts(first), parts(second), 1, strategy)
    assert [a - b for a, b in zip(merged.to_flat(), merged.to_flat_reference())] == delta
    assert [merged.has_difference(i % SIDE, i // SIDE) for i in range(SIDE * SIDE)] == changed
    got = (
        report.taken_from_one,
        report.taken_from_two,
        report.contested,
        report.minor,
        report.major,
    )
    assert got == counts


def test_three_component_resolve_matches_the_python() -> None:
    rng = random.Random(7)  # noqa: S311 - test terrain, not security
    reference = [rng.randint(0, 255) for _ in range(SIDE * SIDE * 3)]
    first = RelativeGrid.from_difference(reference, _terrain(rng, reference, 60, 0.5), SIDE, 3)
    second = RelativeGrid.from_difference(reference, _terrain(rng, reference, 60, 0.5), SIDE, 3)
    merged, report = merge_layer(LandData.VERTEX_COLORS, first, second, ConflictStrategy.RESOLVE)
    ref = first.to_flat_reference()
    d1 = [a - b for a, b in zip(first.to_flat(), ref)]
    d2 = [a - b for a, b in zip(second.to_flat(), ref)]
    c1 = [first.has_difference(i % SIDE, i // SIDE) for i in range(SIDE * SIDE)]
    c2 = [second.has_difference(i % SIDE, i // SIDE) for i in range(SIDE * SIDE)]
    delta, _, counts = _ref_merge((ref, d1, c1), (ref, d2, c2), 3, "resolve")
    assert [a - b for a, b in zip(merged.to_flat(), ref)] == delta
    assert (report.contested, report.minor, report.major) == counts[2:]


def _ref_normals(rows: list[list[float]]) -> list[list[tuple[int, int, int]]]:
    step = 128.0 / HEIGHT_SCALE

    def byte(v: float) -> int:
        return 127 if v >= 127 else -128 if v <= -128 else int(v)

    out = []
    for y in range(SIDE):
        fy = y - 1 if y == SIDE - 1 else y
        row = []
        for x in range(SIDE):
            fx = x - 1 if x == SIDE - 1 else x
            here = rows[fy][fx] / HEIGHT_SCALE
            east = rows[fy][fx + 1] / HEIGHT_SCALE
            north = rows[fy + 1][fx] / HEIGHT_SCALE
            nx, ny, nz = -(east - here) * step, -(north - here) * step, step * step
            unit = (nx * nx + ny * ny + nz * nz) ** 0.5 / 127.0
            row.append((byte(nx / unit), byte(ny / unit), byte(nz / unit)))
        out.append(row)
    return out


def test_normals_match_the_python() -> None:
    rng = random.Random(11)  # noqa: S311 - test terrain, not security
    heights = _terrain(rng, _smooth(rng), 3000, 0.3)
    rows = [[float(v) for v in heights[y * SIDE : (y + 1) * SIDE]] for y in range(SIDE)]
    assert vertex_normals_from_heights(rows) == _ref_normals(rows)


def test_height_decoding_matches_the_python() -> None:
    rng = random.Random(5)  # noqa: S311 - test terrain, not security
    deltas = [rng.randint(-128, 127) for _ in range(SIDE * SIDE)]
    raw = struct.pack(f"<{SIDE * SIDE}b", *deltas)
    offset = 123.25
    expected = []
    row_height = offset
    for y in range(SIDE):
        row_height += deltas[y * SIDE]
        height = row_height
        expected.append(int(height * HEIGHT_SCALE))
        for x in range(1, SIDE):
            height += deltas[y * SIDE + x]
            expected.append(int(height * HEIGHT_SCALE))
    assert wraithguard_native.decode_heights(raw, offset) == expected


def _ref_limit(cells: dict[tuple[int, int], list[int]], limit: int, auth: frozenset) -> tuple:
    last = SIDE - 1

    def twins(c, x, y):
        cx, cy = c
        xs, ys = [(cx, x)], [(cy, y)]
        if x == 0:
            xs.append((cx - 1, last))
        elif x == last:
            xs.append((cx + 1, 0))
        if y == 0:
            ys.append((cy - 1, last))
        elif y == last:
            ys.append((cy + 1, 0))
        return [((gx, gy), vx, vy) for gx, vx in xs for gy, vy in ys if (gx, gy) != c]

    def movable(c, x, y, a):
        return c not in a and all(t in cells and t not in a for t, _, _ in twins(c, x, y))

    tally = {"adjusted": 0, "pinned": 0, "worst": 0}

    def shift(c, x, y, d):
        if d == 0:
            return
        if not movable(c, x, y, frozenset()):
            tally["pinned"] += 1
            return
        cells[c][y * SIDE + x] += d
        tally["adjusted"] += 1
        for t, tx, ty in twins(c, x, y):
            cells[t][ty * SIDE + tx] += d

    pairs = [((x - 1, y), (x, y)) for y in range(SIDE) for x in range(1, SIDE)]
    pairs += [((0, y - 1), (0, y)) for y in range(1, SIDE)]
    passes = 0
    for attempt in range(1, MAX_PASSES + 1):
        passes = attempt
        excessive = 0
        for c in sorted(cells):
            grid = cells[c]
            for (ax, ay), (bx, by) in pairs:
                step = grid[bx + by * SIDE] - grid[ax + ay * SIDE]
                if -limit <= step <= limit:
                    continue
                excessive += 1
                excess = abs(step) - limit
                tally["worst"] = max(tally["worst"], excess)
                ma, mb = movable(c, ax, ay, auth), movable(c, bx, by, auth)
                if ma and mb:
                    share_a = max(0, min(excess, round(excess * 0.5)))
                    share_b = excess - share_a
                elif ma:
                    share_a, share_b = excess, 0
                elif mb:
                    share_a, share_b = 0, excess
                else:
                    tally["pinned"] += 1
                    continue
                direction = 1 if step > 0 else -1
                shift(c, ax, ay, direction * share_a)
                shift(c, bx, by, -direction * share_b)
        if excessive == 0:
            break
    return cells, tally, passes


def test_slope_limiter_matches_the_python() -> None:
    rng = random.Random(3)  # noqa: S311 - test terrain, not security
    coords = [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0)]
    cells = {c: _terrain(rng, _smooth(rng), 4000, 0.02) for c in coords}
    # Make shared borders agree first, as seam repair leaves them.
    for (cx, cy), grid in cells.items():
        right = cells.get((cx + 1, cy))
        if right is not None:
            for y in range(SIDE):
                right[y * SIDE] = grid[y * SIDE + SIDE - 1]
        up = cells.get((cx, cy + 1))
        if up is not None:
            for x in range(SIDE):
                up[x] = grid[(SIDE - 1) * SIDE + x]
    authoritative = frozenset({(2, 0)})
    expected, tally, passes = _ref_limit(
        {c: list(g) for c, g in cells.items()}, MAX_STEP - 8, authoritative
    )

    arrays = {c: array("i", g) for c, g in cells.items()}
    report = limit_slopes(arrays, authoritative=authoritative)
    assert {c: list(g) for c, g in arrays.items()} == expected
    assert (report.adjusted, report.pinned, report.worst_excess, report.passes) == (
        tally["adjusted"],
        tally["pinned"],
        tally["worst"],
        passes,
    )
