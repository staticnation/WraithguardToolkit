#!/bin/sh
# Wraithguard's Flatpak entry point: the toolkit on its own free-threaded Python, with
# the viewer shell named for wraithguard/viewer_launch.py (the app does not sit beside
# it here, as it does in the frozen builds).
export WRAITHGUARD_VIEWER=/app/bin/wraithguard-viewer
exec /app/python/bin/python3 /app/share/wraithguard/wraithguard_toolkit_gui.py "$@"
