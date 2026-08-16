# -*- mode: python ; coding: utf-8 -*-
from PyInstaller.utils.hooks import collect_submodules
from PyInstaller.utils.hooks import collect_all

datas = [('/src/wraithguard_toolkit.py', '.'), ('/src/README.md', '.'), ('/src/QUICKSTART.md', '.'), ('/src/MLOX_RULES.md', '.'), ('/src/wraithguard/viz/assets', 'assets'), ('/src/wraithguard_toolkit_icon.ico', '.')]
binaries = []
hiddenimports = ['clr', 'webview.platforms.qt', 'tkinterweb']
hiddenimports += collect_submodules('wraithguard')
tmp_ret = collect_all('webview')
datas += tmp_ret[0]; binaries += tmp_ret[1]; hiddenimports += tmp_ret[2]
tmp_ret = collect_all('clr_loader')
datas += tmp_ret[0]; binaries += tmp_ret[1]; hiddenimports += tmp_ret[2]
tmp_ret = collect_all('pythonnet')
datas += tmp_ret[0]; binaries += tmp_ret[1]; hiddenimports += tmp_ret[2]


a = Analysis(
    ['/src/wraithguard_toolkit_gui.py'],
    pathex=[],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=['PySide6', 'PySide2', 'shiboken6', 'PyQt6.QtQml', 'PyQt6.QtQuick', 'PyQt6.QtQuickWidgets', 'PyQt6.QtQuick3D', 'PyQt6.QtPositioning', 'PyQt6.QtDBus', 'PyQt6.QtMultimedia', 'PyQt6.QtSpatialAudio', 'PyQt6.QtTextToSpeech'],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name='wraithguard_toolkit_gui',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['/src/wraithguard_toolkit_icon.ico'],
)
