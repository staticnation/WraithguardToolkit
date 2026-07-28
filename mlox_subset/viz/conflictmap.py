"""The world conflict map: where your mods actually collide.

**An alternative map, not a change to the existing one.** ``cell_map.html``
answers "which mods touch which cells" -- coverage -- and it stays exactly as
it is, SVG and all. This page answers a different question over the same world
grid: "which cells have records that *conflict*, and who wins there". Two mods
can touch the same cell happily; the interesting cells are the ones where the
same record is defined twice.

Keeping them as two maps rather than one map with extra marks is deliberate.
Coverage is much the larger set, and painting collisions on top of it would
invite reading a busy cell as a broken one.

Drawn as a sparse SVG -- one ``<rect>`` per cell that has conflicts, placed
absolutely -- for the same reason the cell map is: a dense grid over Morrowind
plus Tamriel Rebuilt is millions of cells, almost all of them empty, and
emitting them all would produce a file no browser will open.

**Focusing on one plugin.** The cell map's own "Focus on mod" dropdown mutes
every cell a plugin does not touch and leaves the rest alone; that muting
convention is reused here so the two maps interact the same way. It goes one
step further because the question here is different -- not just "is this
plugin present" but "how much is it colliding, and over what" -- so a focused
cell is also recoloured by *that plugin's own* count there, using the same
bands as the all-plugins view so the two remain comparable, and a summary line
reports its landscape/path-grid/cell breakdown. The
per-plugin counts this needs are decoded once, server-side, into
``CellConflicts.by_plugin`` (see :mod:`~mlox_subset.viz.geometry`) and
embedded as JSON; the dropdown itself is then a pure client-side redraw, the
same split every other switcher in this package uses.

**Hovering a cell.** Native SVG ``<title>`` tooltips carry a browser-controlled
delay of about a second and can't be styled, which is exactly wrong for a grid
meant to be swept over with the cursor. Each cell instead carries its tooltip
text in a ``data-t`` attribute, shown instantly in a mouse-following div --
the same ``data-t``/``#tt`` pattern the cell map uses, so hovering either map
feels the same. The text names which specific plugins are conflicting there,
not just the count.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from mlox_subset import _, ngettext
from mlox_subset.viz import html as h
from mlox_subset.viz.geometry import Cell, CellConflicts, bounds, group_by_cell, parse_grid
from mlox_subset.viz.palette import (
    MINE,
    NEUTRAL,
    severity_band_table,
    severity_banded,
    severity_legend_rows,
)

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

#: Pixel size of one cell in the rendered map.
_CELL_PX = 9

#: Instant, mouse-following tooltip -- native SVG <title> tooltips have a
#: browser-controlled ~1s hover delay and can't be styled, which is exactly
#: wrong for a dense grid the person is sweeping the cursor across. Mirrors
#: the cell map's own #tt/data-t pattern so hovering either map feels the
#: same. Unconditional (not tied to the focus feature): it's useful even with
#: nothing to focus on.
_TOOLTIP_CSS = """
<style>
/* The map scrolls in its own pane, like the cell map's: a Tamriel-sized grid
   is wider than any window, and without this the page itself scrolls and the
   focus bar disappears off the top. Drag the bottom edge to resize. */
.mapwrap{overflow:auto;max-height:74vh;border:1px solid var(--line);background:#06111c;
display:block;max-width:100%;resize:vertical}
.mapwrap svg{display:block;max-width:none}
.listwrap{overflow:auto;max-height:60vh;border:1px solid var(--line);resize:vertical}
.listwrap th{position:sticky;top:0;background:var(--panel)}
#cm-tt{position:fixed;pointer-events:none;display:none;z-index:99;max-width:440px;
background:var(--panel);color:var(--ink);border:1px solid var(--line);border-radius:4px;
padding:4px 8px;font-size:12.5px;box-shadow:0 2px 8px rgba(0,0,0,.4)}
.grid rect:hover{stroke:#fff;stroke-width:1.4}
</style>
"""

_TOOLTIP_SCRIPT = """
(function(){
var tt=document.getElementById('cm-tt');
document.addEventListener('mouseover',function(e){
  var r=e.target;
  if(r&&r.getAttribute&&r.hasAttribute('data-t')){
    tt.textContent=r.getAttribute('data-t');tt.style.display='block';
  }
});
document.addEventListener('mousemove',function(e){
  if(tt.style.display==='block'){
    tt.style.left=(e.clientX+12)+'px';tt.style.top=(e.clientY+12)+'px';
  }
});
document.addEventListener('mouseout',function(e){
  var r=e.target;
  if(r&&r.getAttribute&&r.hasAttribute('data-t')){tt.style.display='none';}
});
})();
"""

#: Styling for the focus bar and the muted state, kept separate from
#: html.py's shared ``_CSS`` because it's specific to this one page's
#: interactivity rather than something every visualisation needs.
_FOCUS_CSS = """
<style>
.focusbar{margin:2px 0 12px;display:flex;align-items:center;gap:8px;flex-wrap:wrap}
.focusbar select{background:var(--panel);color:var(--ink);border:1px solid var(--line);
border-radius:4px;padding:5px 8px;max-width:360px;font:inherit}
.focusbar button{background:#2c313a;color:#d7dae0;border:1px solid #333945;border-radius:4px;
padding:5px 10px;cursor:pointer;font:inherit}
#cm-focus-info{color:var(--dim);font-size:12.5px;min-height:1.4em}
.grid rect.dim{opacity:.15}
</style>
"""

_SCRIPT = """
(function(){
const D=window.__conflictmap;
// A lookup, not a ramp. The page embeds the same bands the server drew the
// unfocused map with, so focusing a plugin cannot colour a count differently
// from the way that count is coloured everywhere else -- which is exactly what
// re-implementing the curve here used to risk.
function severityColour(count){
  if(count<=0)return D.neutral;
  const bands=D.bands;
  for(let i=0;i<bands.length;i++){
    const low=bands[i][0],high=bands[i][1];
    if(count>=low&&(high===null||count<=high))return bands[i][2];
  }
  // Above every band: a stale payload, or a focused count larger than any
  // whole-cell total. The top band is the honest answer either way.
  return bands.length?bands[bands.length-1][2]:D.neutral;
}

const select=document.getElementById('cm-focus');
const clearBtn=document.getElementById('cm-clear');
const info=document.getElementById('cm-focus-info');
const rects=document.querySelectorAll('.grid rect');
const rows=document.querySelectorAll('#cm-worst tbody tr');

function matches(dataM,name){return !name||(dataM||'').indexOf('|'+name+'|')>-1;}

function setFocus(name){
  name=(name||'').toLowerCase();
  let cellsTouched=0,total=0;
  const byType={};
  rects.forEach(function(r){
    const hit=matches(r.getAttribute('data-m'),name);
    r.classList.toggle('dim',!!name&&!hit);
    if(name&&hit){
      const cellInfo=D.cells[r.getAttribute('data-cell')];
      const counts=(cellInfo&&cellInfo.by_plugin[name])||{};
      let sum=0;
      for(const k in counts){sum+=counts[k];byType[k]=(byType[k]||0)+counts[k];}
      r.setAttribute('fill',severityColour(sum));
      if(sum>0)cellsTouched++;
      total+=sum;
    }else{
      r.setAttribute('fill',r.getAttribute('data-orig'));
    }
  });
  rows.forEach(function(row){
    row.style.display=matches(row.getAttribute('data-m'),name)?'':'none';
  });
  if(!name){info.textContent='';return;}
  const parts=[];
  for(const k in byType)parts.push(k+': '+byType[k]);
  info.textContent=D.labels.focus_summary
    .replace('%(cells)s',cellsTouched).replace('%(total)s',total)
    .replace('%(breakdown)s',parts.length?parts.join(', '):D.labels.no_breakdown);
}

select.addEventListener('change',function(){setFocus(select.value);});
clearBtn.addEventListener('click',function(){select.value='';setFocus('');});
})();
"""


def _modattr(plugins: Sequence[str]) -> str:
    """Render a cell's plugin list as an exact-match filter token.

    Mirrors the cell map's own ``modattr`` helper: ``|a.esp|b.esp|`` so a
    substring search for ``|name|`` can never partially match a longer
    plugin's filename, and the two maps' focus filters behave identically.

    Args:
        plugins: Plugin filenames touching this cell, any case.

    Returns:
        An HTML-attribute-safe token string.
    """
    return h.escape("|" + "|".join(p.lower() for p in plugins) + "|")


def _focus_options(
    cells: Mapping[Cell, CellConflicts], subset_lower: set[str]
) -> tuple[str, dict[str, str]]:
    """Build the "Focus on plugin" dropdown, customs first and starred.

    Args:
        cells: Aggregated conflicts per cell.
        subset_lower: Lower-cased filenames of the user's own mods.

    Returns:
        ``(option_markup, display_names)`` where ``display_names`` maps a
        lower-cased plugin name to how it should be shown -- also doubling as
        "is there anything to put in the dropdown at all".
    """
    display: dict[str, str] = {}
    for info in cells.values():
        for plugin in info.plugins:
            display.setdefault(plugin.lower(), plugin)
    # The star marker is built outside the f-string: a backslash escape inside
    # an f-string *expression* is a syntax error before Python 3.12, and this
    # project targets 3.10.
    star = " \u2605"
    options = "".join(
        f'<option value="{h.escape(low)}">{h.escape(display[low])}'
        f"{star if low in subset_lower else ''}</option>"
        for low in sorted(display, key=lambda x: (x not in subset_lower, x))
    )
    return options, display


def _svg_grid(cells: Mapping[Cell, CellConflicts], worst: int) -> str:
    """Draw the conflict grid as absolute-positioned SVG rectangles.

    Args:
        cells: Aggregated conflicts per cell.
        worst: The highest conflict count, saturating the colour ramp.

    Returns:
        The ``<svg>`` markup, or an empty-state note when there is nothing to
        draw.
    """
    box = bounds(cells)
    if box is None:
        return f'<div class="empty">{h.escape(_("No exterior cells have conflicts."))}</div>'
    min_x, min_y, max_x, max_y = box
    width = (max_x - min_x + 1) * _CELL_PX
    height = (max_y - min_y + 1) * _CELL_PX
    parts = [
        # Wrapped in a scrolling pane like the cell map's: a Tamriel-sized grid
        # is wider than any window, and letting the page scroll instead pushes
        # the focus bar off the top.
        '<div class="mapwrap">',
        f'<svg class="grid" viewBox="0 0 {width} {height}" '
        f'width="{width}" height="{height}" role="img">',
    ]
    for cell, info in sorted(cells.items()):
        # SVG y grows downward, the world's grid y grows north: flip, or the
        # map comes out upside down against every other Morrowind map.
        px = (cell.x - min_x) * _CELL_PX
        py = (max_y - cell.y) * _CELL_PX
        colour = severity_banded(info.total, worst)
        klass = ' class="mine"' if info.mine else ""
        tip = ngettext(
            "(%(x)d, %(y)d) \u2014 %(count)d conflict, %(n)d plugin(s): %(plugins)s",
            "(%(x)d, %(y)d) \u2014 %(count)d conflicts, %(n)d plugin(s): %(plugins)s",
            info.total,
        ) % {
            "x": cell.x,
            "y": cell.y,
            "count": info.total,
            "n": len(info.plugins),
            "plugins": ", ".join(info.plugins),
        }
        # data-cell/data-orig/data-m are inert without the focus script (no
        # other plugins to filter by, or the page was generated before this
        # feature) -- they just ride along unused, same as any other
        # attribute an SVG viewer ignores.
        parts.append(
            f'<rect x="{px}" y="{py}" width="{_CELL_PX}" height="{_CELL_PX}" '
            f'fill="{colour}"{klass} data-cell="{cell.x},{cell.y}" data-orig="{colour}" '
            f'data-m="{_modattr(info.plugins)}" data-t="{h.escape(tip)}"></rect>'
        )
    parts.append("</svg></div>")
    return "".join(parts)


def _worst_table(cells: Mapping[Cell, CellConflicts]) -> str:
    """Tabulate every conflicting cell, worst first.

    Args:
        cells: Aggregated conflicts per cell.

    Returns:
        The table markup.
    """
    ranked = sorted(cells.values(), key=lambda c: (-c.total, c.cell))
    rows = []
    row_attrs = []
    for info in ranked:
        top_winner = max(info.winners.items(), key=lambda kv: kv[1])[0] if info.winners else ""
        kinds = ", ".join(f"{k} x{v}" for k, v in sorted(info.types.items(), key=lambda kv: -kv[1]))
        rows.append(
            [
                f"({info.cell.x}, {info.cell.y})",
                info.total,
                info.mine,
                kinds,
                top_winner,
            ]
        )
        row_attrs.append({"data-m": _modattr(info.plugins)})
    return h.table(
        [
            _("Cell"),
            _("Conflicts"),
            _("Yours"),
            _("Record types"),
            _("Usually wins"),
        ],
        rows,
        numeric={1, 2},
        row_attrs=row_attrs,
    )


def _type_meaning() -> dict[str, str]:
    """What each spatially-keyed record type governs.

    A function rather than a module constant so the strings are marked *at the
    call*: ``_(variable)`` extracts nothing, so a lookup table of bare strings
    translated later would silently never appear in the ``.pot`` and could
    never be translated.

    Returns:
        Record type name to a one-line description of what it controls.
    """
    return {
        "Landscape": _("terrain shape, textures and vertex colours"),
        "PathGrid": _("NPC navigation -- broken edges strand NPCs, and nothing else reports it"),
        "Cell": _("the cell's own record: name, water level, region, ambient light"),
    }


def _type_table(cells: Mapping[Cell, CellConflicts]) -> str:
    """Break the conflicts down by what kind of record is being edited.

    A count of "conflicts in this cell" does not say whether two mods reshaped
    the same hillside or merely both placed a barrel. Landscape and path-grid
    conflicts are the ones with consequences you cannot see in a list, so the
    breakdown leads with them.

    This table stays global regardless of the focus filter -- it has no
    per-cell row to hide, and it is the reference the focused plugin's own
    breakdown (in the info line above the map) is meant to be read against.

    Args:
        cells: Aggregated conflicts per cell.

    Returns:
        The table markup.
    """
    totals: dict[str, int] = {}
    places: dict[str, int] = {}
    for info in cells.values():
        for rectype, count in info.types.items():
            totals[rectype] = totals.get(rectype, 0) + count
            places[rectype] = places.get(rectype, 0) + 1
    meaning = _type_meaning()
    rows = [
        [rectype, count, places[rectype], meaning.get(rectype, "")]
        for rectype, count in sorted(totals.items(), key=lambda kv: -kv[1])
    ]
    return h.table(
        [_("Record type"), _("Conflicts"), _("Cells"), _("What it governs")],
        rows,
        numeric={1, 2},
    )


def build_conflict_map(
    conflicts: Sequence[Mapping[str, Any]],
    *,
    title: str = "",
    subset_lower: Iterable[str] = (),
) -> str:
    """Render the world conflict map as a self-contained HTML page.

    Args:
        conflicts: Conflict dicts as ``detect_conflicts`` returns them.
        title: Optional page title; a sensible default is used when empty.
        subset_lower: Lower-cased filenames of the user's own mods, purely
            cosmetic -- it only decides which entries get a star and sort
            first in the focus dropdown, matching the cell map's own
            convention. An empty iterable (the default) just means an
            unstarred, alphabetical dropdown.

    Returns:
        A complete HTML document. Never raises on odd input -- records with
        unusable ids are simply not spatial and are counted as such.
    """
    cells = group_by_cell(conflicts)
    # The true maximum, not a percentile. Percentile clamping existed to stop
    # one forty-conflict cell flattening every ordinary cell to green -- a real
    # problem for a continuous ramp, and one the banding solves outright: an
    # outlier now lands in the open-ended top band and costs the lower bands
    # nothing. Using the maximum also makes the legend describe the real range,
    # and the worst cell is findable in the table below rather than clamped
    # away into a colour it shares with cells a tenth its size.
    worst = max((c.total for c in cells.values()), default=0)
    spatial = sum(c.total for c in cells.values())
    mine = sum(c.mine for c in cells.values())
    non_spatial = len(conflicts) - spatial
    subset = {s.lower() for s in subset_lower}

    # The band's own label ("1", "6-10", "76+") is the legend text. Building a
    # sentence per swatch was how this read before; with one swatch per band
    # that is a wall of repeated words, and the unit belongs in the note once.
    stops = [(colour, label) for label, colour, _dark in severity_legend_rows(worst)]

    focus_options, focus_names = _focus_options(cells, subset)
    focus_bar = (
        f'<div class="focusbar">{h.escape(_("Focus on plugin:"))} '
        f'<select id="cm-focus"><option value="">{h.escape(_("— all plugins —"))}</option>'
        f"{focus_options}</select> "
        f'<button id="cm-clear" type="button">{h.escape(_("Clear"))}</button></div>'
        f'<div id="cm-focus-info"></div>'
    )

    body = [
        '<div id="cm-tt"></div>',
        h.summary(
            {
                _("Cells with conflicts"): len(cells),
                _("Spatial conflicts"): spatial,
                _("Involving your mods"): mine,
                _("Non-spatial (objects, dialogue, interiors)"): max(0, non_spatial),
            }
        ),
        h.card(
            _("Conflict density by cell"),
            (focus_bar if focus_names else "")
            + _svg_grid(cells, worst)
            + h.legend(
                [*stops, (MINE, _("outlined = involves your mods"))],
                _(
                    "Conflicting records per cell. North is up. Hover a cell for its "
                    "count and which plugins are conflicting there. Each of the first "
                    "few counts has its own colour and larger counts are grouped, "
                    "because one, two and three conflicts are different situations "
                    "while thirty and thirty-five are not. Focusing a plugin recolours "
                    "its cells by its own count there on this same scale, and mutes "
                    "the rest."
                ),
            ),
        ),
        h.card(_("What is being edited"), _type_table(cells)),
        h.card(
            _("All cells (worst first)"),
            f'<div class="listwrap" id="cm-worst">{_worst_table(cells)}</div>',
        ),
        _TOOLTIP_CSS,
        f"<script>{_TOOLTIP_SCRIPT}</script>",
    ]

    if focus_names:
        payload = {
            "neutral": NEUTRAL,
            # The bands themselves, so the client looks a count up instead of
            # recomputing a curve it could get subtly wrong.
            "bands": severity_band_table(worst),
            "cells": {
                f"{cell.x},{cell.y}": {
                    "by_plugin": {p.lower(): counts for p, counts in info.by_plugin.items()},
                }
                for cell, info in cells.items()
            },
            "labels": {
                "focus_summary": _(
                    "Touches %(cells)s cell(s), %(total)s conflict(s) here. %(breakdown)s"
                ),
                "no_breakdown": _("(no type breakdown available)"),
            },
        }
        body.append(_FOCUS_CSS)
        body.append(
            f"<script>window.__conflictmap={h.script_json(payload)};</script><script>{_SCRIPT}</script>"
        )

    return h.page(
        title or _("Conflict map"),
        _(
            "Cells where the same record is defined by more than one plugin. "
            "Coverage (which mods touch which cells) is a different question -- "
            "see the cell map."
        ),
        "".join(body),
    )


def cells_with_conflicts(conflicts: Iterable[Mapping[str, Any]]) -> set[tuple[int, int]]:
    """List the exterior cells that have any conflicting record.

    Used to cross-link the cell map: a coverage cell that also appears here can
    be marked so the two pages point at each other.

    Args:
        conflicts: Conflict dicts as ``detect_conflicts`` returns them.

    Returns:
        ``(x, y)`` pairs, plain tuples so callers need not import
        :class:`~mlox_subset.viz.geometry.Cell`.
    """
    out: set[tuple[int, int]] = set()
    for conflict in conflicts:
        cell = parse_grid(conflict.get("id"))
        if cell is not None:
            out.add((cell.x, cell.y))
    return out
