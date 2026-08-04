"""Landscape merging: the reference landmass, per-plugin differences, merging.

This package is a Python port of **Merged Lands** by David Von Derau (MIT,
2022), whose source sits beside this repository in ``merged_lands-main``. The
licence permits the port; ``CREDITS.md`` records it.

**What it is for.** When two mods edit terrain in the same cell, the load order
picks one and discards the other -- ``LAND`` is a whole record, and last plugin
wins. If the two mods changed *different parts* of that cell, the loss is
avoidable: their edits can be combined into a single landscape that honours
both. That is what this package computes, and what ``Merged Lands.esp`` carries.

**The shape of the algorithm**, following the original:

1. Build a *reference* landmass from the master files -- the terrain as the game
   ships it, before any mod.
2. For each plugin, compute a *difference* against the reference: not "what
   this cell looks like" but "which vertices this mod actually moved".
3. Merge the differences into one landmass. Where mods touch disjoint vertices
   both edits survive intact; where they overlap, a conflict strategy decides.
4. Check the result for seams between cells and repair them.
5. Emit ``LAND``, ``LTEX`` and ``CELL`` records as a new plugin.

Steps 1 and 2 live here and in :mod:`~wraithguard.land.diff`; the rest arrive
with the merge and repair modules.

**Scope, deliberately narrow.** Only landscape data is touched: heights,
normals, vertex colours, texture indices, the world map grid, and the land
textures those indices name. References inside cells are never read, compared
or written. A merged plugin carries terrain and nothing else, so every object,
NPC and script in the load order continues to resolve exactly as it did.

**Why this package holds no dependencies.** The toolkit runs on the standard
library alone (``pyproject.toml`` declares ``dependencies = []``) and ships as a
frozen binary. A 65x65 grid per layer per cell across a large load order is a
lot of arithmetic for pure Python, so the grids here are flat sequences of
machine integers rather than nested objects -- see :mod:`~wraithguard.land.grid`
for what that costs and buys.
"""

from __future__ import annotations
