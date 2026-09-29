#!/usr/bin/env bash
# Build Wraithguard's Linux AppImage: the toolkit, its viewer shell, and WebKitGTK with
# its whole dependency tree, so it runs on a machine that has no WebKitGTK at all (the
# Steam Deck's SteamOS ships none).
#
# Expects, already built on this machine (build-linux.yml does both):
#   viewer-shell/target/release/wraithguard-viewer    cargo build --release
#   dist/wraithguard_toolkit_gui/                     PyInstaller --onedir
# and libwebkit2gtk-4.1 installed (the -dev package the viewer was built against).
# Build on the oldest distro you support: the AppImage runs where that glibc runs
# (Ubuntu 22.04 -> glibc 2.35, older than any current SteamOS).
#
# How the WebKit part works - the same way Tauri's own AppImage bundler does it:
#  * linuxdeploy + its GTK plugin copy the viewer's library closure into usr/lib, set
#    RPATHs, and add hooks for GTK's modules, GIO, GDK-pixbuf loaders and schemas.
#  * libwebkit2gtk starts helper processes (WebKitWebProcess, WebKitNetworkProcess) and
#    loads an injected bundle from an absolute directory compiled into it
#    (/usr/lib/<triplet>/webkit2gtk-4.1). They are copied into the AppDir at the same
#    relative place, and the library's "/usr" is rewritten to "././" (same length), so
#    it looks for them relative to the working directory - which AppRun and
#    wraithguard/viewer_launch.py set to AppDir/usr for the viewer.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VIEWER="${VIEWER:-$ROOT/viewer-shell/target/release/wraithguard-viewer}"
APP="${APP:-$ROOT/dist/wraithguard_toolkit_gui}"
OUT_DIR="${OUT_DIR:-$ROOT/dist}"
WORK="${WORK:-$ROOT/build/appimage}"
APPDIR="$WORK/Wraithguard.AppDir"
LINUXDEPLOY_URL="${LINUXDEPLOY_URL:-https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage}"
GTK_PLUGIN_URL="${GTK_PLUGIN_URL:-https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh}"

[ -x "$VIEWER" ] || { echo "no viewer at $VIEWER (cargo build --release in viewer-shell)"; exit 1; }
[ -x "$APP/wraithguard_toolkit_gui" ] || { echo "no PyInstaller --onedir build at $APP"; exit 1; }

rm -rf "$WORK"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/opt" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$WORK/tools"

# The toolkit and its viewer.
cp "$VIEWER" "$APPDIR/usr/bin/wraithguard-viewer"
cp -a "$APP" "$APPDIR/opt/wraithguard"
ln -sf ../../opt/wraithguard/wraithguard_toolkit_gui "$APPDIR/usr/bin/wraithguard_toolkit_gui"

# WebKit's helper processes and injected bundle, at the same relative path.
WEBKIT_EXEC="$(find -L /usr/lib /usr/lib64 /usr/libexec -name WebKitWebProcess -path '*webkit2gtk-4.1*' 2>/dev/null | head -n1 || true)"
[ -n "$WEBKIT_EXEC" ] || { echo "WebKitWebProcess (webkit2gtk-4.1) not found - install libwebkit2gtk-4.1-dev"; exit 1; }
WEBKIT_DIR="$(dirname "$WEBKIT_EXEC")"
case "$WEBKIT_DIR" in
  /usr/*) ;;
  *) echo "unexpected WebKit helper location $WEBKIT_DIR (the /usr rewrite needs it under /usr)"; exit 1 ;;
esac
mkdir -p "$APPDIR$WEBKIT_DIR"
cp -a "$WEBKIT_DIR/." "$APPDIR$WEBKIT_DIR/"

# The icon (PNG from the .ico's largest frame) and the desktop entry.
python3 - "$ROOT/wraithguard_toolkit_icon.ico" "$APPDIR/usr/share/icons/hicolor/256x256/apps/wraithguard.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1])
im.size = max(im.ico.sizes())
im.load()
im.resize((256, 256)).save(sys.argv[2])
PY
cp "$ROOT/packaging/linux/wraithguard.desktop" "$APPDIR/usr/share/applications/wraithguard.desktop"

# linuxdeploy and the GTK plugin (self-extracting: no FUSE needed in CI containers).
cd "$WORK/tools"
wget -q -O linuxdeploy-x86_64.AppImage "$LINUXDEPLOY_URL"
wget -q -O linuxdeploy-plugin-gtk.sh "$GTK_PLUGIN_URL"
chmod +x linuxdeploy-x86_64.AppImage linuxdeploy-plugin-gtk.sh
export PATH="$WORK/tools:$PATH"
export APPIMAGE_EXTRACT_AND_RUN=1
export DEPLOY_GTK_VERSION=3
LD="$WORK/tools/linuxdeploy-x86_64.AppImage"

# 1. Deploy: the viewer's closure, the helpers' closure, GTK's runtime bits.
"$LD" --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/wraithguard-viewer" \
  --deploy-deps-only "$APPDIR$WEBKIT_DIR" \
  --desktop-file "$APPDIR/usr/share/applications/wraithguard.desktop" \
  --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/wraithguard.png" \
  --plugin gtk

# 2. The helpers resolve relative to the working directory (see the header).
found=0
while IFS= read -r lib; do
  sed -i -e 's|/usr|././|g' "$lib"; found=1
done < <(find "$APPDIR/usr/lib" -name 'libwebkit2gtk-4.1.so*' -type f)
[ "$found" = 1 ] || { echo "libwebkit2gtk-4.1 was not deployed"; exit 1; }

# 3. Our entry point (it runs the GTK hooks itself), then the image.
rm -f "$APPDIR/AppRun" "$APPDIR/AppRun.wrapped"
cp "$ROOT/packaging/linux/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"
mkdir -p "$OUT_DIR"
cd "$OUT_DIR"
OUTPUT="Wraithguard-x86_64.AppImage" "$LD" --appdir "$APPDIR" --output appimage
ls -lh "$OUT_DIR/Wraithguard-x86_64.AppImage"
