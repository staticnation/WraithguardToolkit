"""PyInstaller runtime hook: pin pywebview to its Qt backend in the -qt build.

Runs inside the frozen ``-qt.exe`` before the app imports ``webview``. On
Windows pywebview defaults to the EdgeChromium backend, which it selects
whenever ``clr``/pythonnet imports -- and it does here, because the edgechromium
stack is bundled alongside Qt. On a machine where the user removed the WebView2
runtime that backend renders a blank white view rather than failing over, so a
build whose entire purpose is to work *without* WebView2 must pin pywebview to Qt
explicitly. ``PYWEBVIEW_GUI`` is the documented override; ``setdefault`` leaves
any value the user set themselves untouched.

``QTWEBENGINE_DISABLE_SANDBOX`` addresses the second, independent cause of a
blank Qt view: QtWebEngine's helper process (``QtWebEngineProcess.exe``) can fail
to launch from PyInstaller's ``--onefile`` temp extraction under the Chromium
sandbox. Disabling it there is the standard remedy for a frozen single-file
QtWebEngine app.
"""

import os

os.environ.setdefault("PYWEBVIEW_GUI", "qt")
os.environ.setdefault("QTWEBENGINE_DISABLE_SANDBOX", "1")
