The viewer shell uses `icon.png` directly.

Keeping a PNG here avoids platform-specific ICO decoding during the Rust/Tauri
build. The toolkit's root `wraithguard_toolkit_icon.ico` remains the Windows
application icon for the Python/PyInstaller binary; it is not a dependency of
the viewer shell.
