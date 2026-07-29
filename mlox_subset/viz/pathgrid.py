"""Path-grid navigation graphs, and what a mod did to them.

A ``PGRD`` record is a navigation mesh: points with world coordinates, and
edges between them that NPCs follow. It is also the record type most likely to
be broken silently -- a mod that edits a cell and rebuilds its path grid can
drop connections, and nothing complains until an NPC walks into a wall. The
resource folder's ``missing_pathgrids.pl`` exists precisely because this is a
known and under-diagnosed failure.

The text view already renders the adjacency list, which is readable but not
*comparable*: spotting that node 37 lost two edges means reading two columns of
numbers side by side. Drawn as a graph with added and removed edges coloured,
the same change is immediate.

Projection is a plain top-down ``(x, y)`` drop. Path grids are near-planar
within a cell and Z varies little, so an isometric view would add distortion
for no information -- Z is reported in the tooltip instead.

**A chain of edits, not a star of comparisons against the winner.**
Morrowind's landscape and path-grid records don't merge -- whichever plugin
loads last for a given record completely replaces what was there before. So
the question that matters at each step isn't "how does this differ from the
winner" but "what did *this* plugin change, relative to whatever the plugin
before it left behind". The Base tab is the earliest contributor -- usually
the game's own master file -- shown alone, since there's nothing before it to
diff against; each later tab diffs that plugin against the one immediately
before it in load order. The page opens on the winner's own step, since
that's the change that's actually live in the game. Every contributing
plugin's edges are decoded once, server-side (only Python can), and embedded
as JSON; a tab switch is then a pure client-side set-difference and redraw
against the winner's fixed node layout, the same "decode once, redraw in
place" split :mod:`~mlox_subset.viz.terrain3d` uses for its plugin switcher.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any

from mlox_subset import _
from mlox_subset.tes3fields.pathgrid import PathGridDecodeError, _point_fields, decode_connections
from mlox_subset.viz import html as h

#: Drawing area in pixels, before margins.
_SPAN = 620

#: Margin so nodes on the boundary are not clipped by the viewBox.
_MARGIN = 24

_ADDED = "#5cc45c"
_REMOVED = "#e05561"
_KEPT = "#5a6473"
_NODE = "#d7dae0"

#: Shared tab-strip styling, matching terrain3d's plugin switcher so the two
#: pages feel like the same tool.
_TABS_CSS = (
    "<style>.tabs{margin-bottom:10px}.tabs button{background:#2c313a;color:#d7dae0;"
    "border:1px solid #333945;border-radius:4px;padding:5px 10px;margin-right:6px;"
    "cursor:pointer;font:inherit}.tabs button.on{background:#3d4450;border-color:#7cc5ff;"
    "color:#fff}</style>"
)

_SCRIPT = """
(function(){
const D=window.__pathgrid;
const edgesGroup=document.getElementById('edges');
const NS='http://www.w3.org/2000/svg';
const btns=document.querySelectorAll('[data-step]');

function edgeSet(pairs){
  const s=new Set();
  for(const p of pairs)s.add(p[0]+','+p[1]);
  return s;
}

function drawSet(set,colour,width){
  for(const key of set){
    const parts=key.split(',');
    const a=+parts[0],b=+parts[1];
    if(a>=D.positions.length||b>=D.positions.length)continue;
    const p1=D.positions[a],p2=D.positions[b];
    const line=document.createElementNS(NS,'line');
    line.setAttribute('x1',p1[0].toFixed(1));line.setAttribute('y1',p1[1].toFixed(1));
    line.setAttribute('x2',p2[0].toFixed(1));line.setAttribute('y2',p2[1].toFixed(1));
    line.setAttribute('stroke',colour);line.setAttribute('stroke-width',width);
    edgesGroup.appendChild(line);
  }
}

function draw(step){
  edgesGroup.textContent='';
  const name=D.chain[step];
  const current=edgeSet(D.edges[name]||[]);

  if(step<=0){
    // Base: nothing loaded before it, so there's nothing to diff against.
    drawSet(current,D.colours.kept,1.0);
    document.getElementById('val-step').textContent=D.labels.base;
    document.getElementById('val-added').textContent='\u2014';
    document.getElementById('val-removed').textContent='\u2014';
    document.getElementById('verdict').style.display='none';
    return;
  }

  const prevName=D.chain[step-1];
  const before=edgeSet(D.edges[prevName]||[]);
  const added=new Set([...current].filter(function(x){return !before.has(x);}));
  const removed=new Set([...before].filter(function(x){return !current.has(x);}));
  const kept=new Set([...current].filter(function(x){return before.has(x);}));

  drawSet(kept,D.colours.kept,1.0);
  drawSet(removed,D.colours.removed,2.0);
  drawSet(added,D.colours.added,2.0);

  document.getElementById('val-step').textContent=prevName+' \u2192 '+name;
  document.getElementById('val-added').textContent=added.size;
  document.getElementById('val-removed').textContent=removed.size;

  const verdict=document.getElementById('verdict');
  if(added.size===0&&removed.size===0){
    verdict.textContent=D.labels.unchanged;verdict.style.display='';
  }else if(removed.size>0&&added.size===0){
    verdict.textContent=D.labels.only_removed;verdict.style.display='';
  }else{
    verdict.style.display='none';
  }
}

btns.forEach(function(b){
  b.addEventListener('click',function(){
    btns.forEach(function(o){o.className='';});
    b.className='on';
    draw(+b.dataset.step);
  });
});
draw(D.default_step);
})();
"""


def _points_and_edges(
    value: str | bytes,
    points: Any,  # noqa: ANN401 - tes3conv's `points` JSON; shape has varied by version
) -> tuple[list[tuple[int, int, int]], set[tuple[int, int]]]:
    """Decode one plugin's path grid into positioned nodes and an edge set.

    Args:
        value: The record's ``connections`` field.
        points: The record's ``points`` list.

    Returns:
        ``(coordinates, edges)``. Edges are ``(source, target)`` index pairs,
        normalised so an undirected connection has one representation.

    Raises:
        PathGridDecodeError: If the connections field cannot be decoded.
    """
    coords: list[tuple[int, int, int]] = []
    counts: list[int] = []
    if isinstance(points, Sequence) and not isinstance(points, str):
        for point in points:
            where, count = _point_fields(point)
            coords.append(where or (0, 0, 0))
            counts.append(count or 0)
    expected = sum(counts) if counts else None
    edges_flat = decode_connections(value, expected)

    edges: set[tuple[int, int]] = set()
    cursor = 0
    for index, count in enumerate(counts):
        for target in edges_flat[cursor : cursor + count]:
            # Normalise: the grid stores each connection from both ends, so
            # without this every edge appears twice and "removed" counts double.
            edges.add((index, target) if index <= target else (target, index))
        cursor += count
    return coords, edges


def _project(
    coords: Sequence[tuple[int, int, int]],
) -> tuple[list[tuple[float, float]], float, float]:
    """Scale world coordinates into the drawing area.

    Args:
        coords: World ``(x, y, z)`` per node.

    Returns:
        ``(screen_positions, scale, unused)`` where scale is world units per
        pixel, for reporting the graph's real extent.
    """
    if not coords:
        return [], 1.0, 1.0
    xs = [c[0] for c in coords]
    ys = [c[1] for c in coords]
    width = max(1, max(xs) - min(xs))
    height = max(1, max(ys) - min(ys))
    scale = _SPAN / max(width, height)
    return (
        [
            (
                _MARGIN + (c[0] - min(xs)) * scale,
                # World Y grows north; screen Y grows down.
                _MARGIN + (max(ys) - c[1]) * scale,
            )
            for c in coords
        ],
        scale,
        max(width, height),
    )


def build_pathgrid_graph(
    surfaces: Mapping[str, tuple[str | bytes, Any]],
    *,
    winner_name: str,
    cell_label: str = "",
) -> str:
    """Render a path grid as the chain of edits that produced it.

    The winner's node layout is fixed and drawn once; stepping through the
    tab strip only changes which pair of plugins' edges are diffed, matching
    :func:`~mlox_subset.viz.terrain3d.build_terrain_3d`'s pattern of decoding
    every surface up front and letting the browser redraw.

    Args:
        surfaces: Every contributing plugin's filename to its
            ``(connections, points)`` fields, in load order. Must contain
            ``winner_name``, which should be last.
        winner_name: The plugin whose graph is drawn; every step in the chain
            is drawn against its node positions.
        cell_label: Optional cell description.

    Returns:
        A complete HTML document. With no other plugin decodable, it draws
        the winner's grid alone with no chain controls.

    Raises:
        PathGridDecodeError: If the winner's connections cannot be decoded.
        KeyError: If ``winner_name`` is not a key of ``surfaces``.
    """
    winner_value, winner_points = surfaces[winner_name]
    coords, winner_edges = _points_and_edges(winner_value, winner_points)
    positions, _scale, extent = _project(coords)

    chain: list[str] = []
    edges_by_name: dict[str, set[tuple[int, int]]] = {}
    for name, (value, points) in surfaces.items():
        if name == winner_name:
            chain.append(name)
            edges_by_name[name] = winner_edges
            continue
        try:
            # NB: not `_, edges = ...` -- `_` is the gettext marker in this
            # module and rebinding it here shadows it for the rest of the
            # function. That has now cost two debugging rounds in this
            # codebase (see the sort engine's `_rank`), so
            # tests/test_standards.py enforces it.
            _coords, edges = _points_and_edges(value, points)
        except PathGridDecodeError:
            # One unreadable step just closes the gap in the chain: the next
            # good plugin ends up diffed against the last good one before it,
            # as if the broken one had never contributed at all.
            continue
        chain.append(name)
        edges_by_name[name] = edges

    parts = [
        f'<svg id="graph" class="grid" viewBox="0 0 {_SPAN + 2 * _MARGIN} {_SPAN + 2 * _MARGIN}" '
        f'width="{_SPAN + 2 * _MARGIN}" height="{_SPAN + 2 * _MARGIN}" role="img">'
        '<g id="edges"></g><g id="nodes">'
    ]
    for index, (x, y) in enumerate(positions):
        z = coords[index][2] if index < len(coords) else 0
        label = _("point %(index)d at (%(x)d, %(y)d, %(z)d)") % {
            "index": index,
            "x": coords[index][0],
            "y": coords[index][1],
            "z": z,
        }
        parts.append(
            f'<circle cx="{x:.1f}" cy="{y:.1f}" r="2.6" fill="{_NODE}">'
            f"<title>{h.escape(label)}</title></circle>"
        )
    parts.append("</g></svg>")

    base_facts = (
        f'<span>{h.escape(_("Cell"))}: <b>{h.escape(cell_label or _("(unknown)"))}</b></span>'
        f'<span>{h.escape(_("Winner"))}: <b>{h.escape(winner_name)}</b></span>'
        f'<span>{h.escape(_("Points"))}: <b>{len(coords)}</b></span>'
        f'<span>{h.escape(_("Connections"))}: <b>{len(winner_edges)}</b></span>'
        f'<span>{h.escape(_("Extent"))}: <b>'
        f'{h.escape(_("%(units)d world units") % {"units": int(extent)})}</b></span>'
    )

    entries = [
        (_NODE, _("path point")),
        (_ADDED, _("edge added at this step")),
        (_REMOVED, _("edge removed at this step")),
        (_KEPT, _("unchanged")),
    ]

    if len(chain) < 2:
        # Only the winner decoded: nothing to chain against, so this draws
        # exactly like the graph did before comparison existed at all.
        graph_body = "".join(parts) + h.legend(
            [entries[0], (_KEPT, _("connection"))],
            _("Top-down view, north up. Hover a point for its coordinates."),
        )
        return h.page(
            _("Path grid"),
            _("The navigation mesh NPCs follow, and what this plugin changed about it."),
            f'<div class="legend">{base_facts}</div>' + h.card(_("Navigation graph"), graph_body),
        )

    tabs = "".join(
        f'<button data-step="{index}" class="{"on" if index == len(chain) - 1 else ""}">'
        + (
            h.escape(_("Base: %(plugin)s") % {"plugin": name})
            if index == 0
            else (
                h.escape(_("Winner: %(plugin)s") % {"plugin": name})
                if index == len(chain) - 1
                else h.escape(name)
            )
        )
        + "</button>"
        for index, name in enumerate(chain)
    )

    facts = base_facts + (
        f'<span>{h.escape(_("Step"))}: <b id="val-step"></b></span>'
        f'<span>{h.escape(_("Edges added"))}: <b id="val-added"></b></span>'
        f'<span>{h.escape(_("Edges removed"))}: <b id="val-removed"></b></span>'
    )

    graph_body = (
        f'<div class="tabs">{tabs}</div>'
        f'<div class="empty" id="verdict" style="display:none"></div>'
        + "".join(parts)
        + h.legend(entries, _("Top-down view, north up. Hover a point for its coordinates."))
    )

    payload = {
        "positions": positions,
        "chain": chain,
        "edges": {name: sorted(edges_by_name[name]) for name in chain},
        "default_step": len(chain) - 1,
        "colours": {"added": _ADDED, "removed": _REMOVED, "kept": _KEPT},
        "labels": {
            "base": _("Base version \u2014 nothing loaded before it to compare against."),
            "unchanged": _(
                "The navigation graph is unchanged at this step. These records differ "
                "in point positions or flags, not in connectivity."
            ),
            "only_removed": _(
                "This step only removes connections. That is the shape of an "
                "accidentally rebuilt path grid, and it is worth checking in game."
            ),
        },
    }

    body = [
        f'<div class="legend">{facts}</div>',
        h.card(_("Navigation graph"), graph_body),
        _TABS_CSS,
        f"<script>window.__pathgrid={h.script_json(payload)};</script><script>{_SCRIPT}</script>",
    ]

    return h.page(
        _("Path grid"),
        _("The navigation mesh NPCs follow, and what this plugin changed about it."),
        "".join(body),
    )
