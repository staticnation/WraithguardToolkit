"""OpenMW Lua: the load order's scripts, read, parsed and checked.

OpenMW mods register Lua scripts in ``.omwscripts`` files (plain text, one
``FLAGS: path`` line each) listed as ``content=`` in openmw.cfg. This package reads
them the way the engine does and reports on what they add up to:

- :mod:`.omwscripts` - the ``.omwscripts`` format.
- :mod:`.lexer` / :mod:`.parser` - a Lua 5.1 (LuaJIT) tokenizer and parser that
  builds a syntax tree, for highlighting, for the tree view and for the checks.
- :mod:`.analysis` - what one script declares (its interface, engine and event
  handlers) and what it does every frame.
- :mod:`.scan` - the whole load order: which file each script path resolves to in
  the data folders, and the conflicts between mods.
- :mod:`.report` - the findings as text.

Nothing here runs Lua, and nothing here writes a file.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations
