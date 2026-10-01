#!/bin/sh
# Wraithguard's Flatpak entry point: the toolkit on its own free-threaded Python, with
# the viewer shell named for wraithguard/viewer_launch.py (the app does not sit beside
# it here, as it does in the frozen builds).
export WRAITHGUARD_VIEWER=/app/bin/wraithguard-viewer
# The viewer's dropdown lists are GTK menus, which take the GTK theme rather than the
# page's colours - the runtime's default Adwaita is white. Dark unless the user set one.
: "${GTK_THEME:=Adwaita:dark}"
export GTK_THEME
exec /app/python/bin/python3 /app/share/wraithguard/wraithguard_toolkit_gui.py "$@"
