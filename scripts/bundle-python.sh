#!/bin/bash
# Bundle Python environment into the app

set -e

SRC_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="$1"

if [ -z "$BUILD_DIR" ]; then
    echo "Usage: bundle-python.sh <build_dir>"
    exit 1
fi

# macOS app
MACOS_APP="$BUILD_DIR/macos/Macaron Singer.app/Contents/Resources"
if [ -d "$MACOS_APP" ]; then
    echo "Bundling Python for macOS..."
    cp -R "$SRC_DIR/python" "$MACOS_APP/python"
    echo "Python bundled successfully at $MACOS_APP/python"
fi

# Windows app (if building)
WIN_DIR="$BUILD_DIR/windows"
if [ -d "$WIN_DIR" ]; then
    echo "Bundling Python for Windows..."
    # Windows logic here
fi

echo "Done!"
