#!/usr/bin/env bash
set -Eeuo pipefail

# Build WraithguardToolkit as a Linux one-file PyInstaller binary in Docker.
#
# Usage:
#   ./build_linux.sh
#
# Optional:
#   ./build_linux.sh /path/to/project
#
# The project directory must contain:
#   wraithguard_toolkit_gui.py
#   wraithguard_toolkit.py
#   README.md
#   QUICKSTART.md
#   MLOX_RULES.md
#   wraithguard/
#   wraithguard_toolkit_icon.ico
#
# Output:
#   dist/wraithguard_toolkit_gui
#
# This intentionally builds against Linux/GTK/WebKit dependencies rather than
# trying to use the Windows PyInstaller command.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="${1:-$SCRIPT_DIR}"
PROJECT_DIR="$(cd -- "$PROJECT_DIR" && pwd)"

IMAGE_NAME="wraithguard-toolkit-builder"
CONTAINER_NAME="wraithguard-toolkit-build-$$"

if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: Docker is not installed or is not in PATH." >&2
    exit 1
fi

if ! docker info >/dev/null 2>&1; then
    echo "ERROR: Docker daemon is not running or the current user cannot access it." >&2
    exit 1
fi

required_files=(
    "wraithguard_toolkit_gui.py"
    "wraithguard_toolkit.py"
    "README.md"
    "QUICKSTART.md"
    "MLOX_RULES.md"
    "wraithguard_toolkit_icon.ico"
)

for file in "${required_files[@]}"; do
    if [[ ! -e "$PROJECT_DIR/$file" ]]; then
        echo "ERROR: Missing required project file: $PROJECT_DIR/$file" >&2
        exit 1
    fi
done

if [[ ! -d "$PROJECT_DIR/wraithguard" ]]; then
    echo "ERROR: Missing required directory: $PROJECT_DIR/wraithguard" >&2
    exit 1
fi

echo "============================================================"
echo " WraithguardToolkit Linux Builder"
echo "============================================================"
echo "Project : $PROJECT_DIR"
echo "Output  : $PROJECT_DIR/dist/wraithguard_toolkit_gui"
echo

# Build a dedicated Linux environment. The source tree is mounted read/write
# only for the duration of the build, so Docker does not modify the project
# except for dist/ and build/ produced by PyInstaller.
docker build \
    --pull \
    -t "$IMAGE_NAME" \
    -f "$SCRIPT_DIR/Dockerfile.wraithguard" \
    "$SCRIPT_DIR"

mkdir -p "$PROJECT_DIR/dist" "$PROJECT_DIR/build"

cleanup() {
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker run \
    --name "$CONTAINER_NAME" \
    --rm \
    -v "$PROJECT_DIR:/src" \
    "$IMAGE_NAME"

echo
echo "============================================================"
echo " Build complete"
echo "============================================================"
echo "Binary:"
echo "  $PROJECT_DIR/dist/wraithguard_toolkit_gui"

if [[ -f "$PROJECT_DIR/dist/wraithguard_toolkit_gui" ]]; then
    file "$PROJECT_DIR/dist/wraithguard_toolkit_gui" 2>/dev/null || true
    ls -lh "$PROJECT_DIR/dist/wraithguard_toolkit_gui"
fi
