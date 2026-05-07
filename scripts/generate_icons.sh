#!/usr/bin/env bash
set -euo pipefail

# Usage:
# ./generate-icons.sh [project_root] [source_png]
#
# Example:
# ./generate-icons.sh . ./icon.png

PROJECT_ROOT="${1:-.}"
SRC="${2:-icon.png}"

ICON_DIR="${PROJECT_ROOT}/src-tauri/icons"

if [ ! -f "$SRC" ]; then
  echo "Source image not found: $SRC"
  exit 1
fi

mkdir -p "$ICON_DIR"

echo "Cleaning existing icons..."

rm -f \
  "$ICON_DIR/32x32.png" \
  "$ICON_DIR/128x128.png" \
  "$ICON_DIR/128x128@2x.png" \
  "$ICON_DIR/icon.png" \
  "$ICON_DIR/icon.ico" \
  "$ICON_DIR/icon.icns" \
  "$ICON_DIR/Square30x30Logo.png" \
  "$ICON_DIR/Square44x44Logo.png" \
  "$ICON_DIR/Square71x71Logo.png" \
  "$ICON_DIR/Square89x89Logo.png" \
  "$ICON_DIR/Square107x107Logo.png" \
  "$ICON_DIR/Square142x142Logo.png" \
  "$ICON_DIR/Square150x150Logo.png" \
  "$ICON_DIR/Square284x284Logo.png" \
  "$ICON_DIR/Square310x310Logo.png" \
  "$ICON_DIR/StoreLogo.png"

gen() {
  local size="$1"
  local output="$2"

  sips -z "$size" "$size" "$SRC" \
    --out "$ICON_DIR/$output" >/dev/null

  echo "Generated $output"
}

echo "Generating PNG assets..."

# Base
cp "$SRC" "$ICON_DIR/icon.png"

# Tauri / app icons
gen 32  "32x32.png"
gen 128 "128x128.png"
gen 256 "128x128@2x.png"

# Windows assets
gen 30  "Square30x30Logo.png"
gen 44  "Square44x44Logo.png"
gen 71  "Square71x71Logo.png"
gen 89  "Square89x89Logo.png"
gen 107 "Square107x107Logo.png"
gen 142 "Square142x142Logo.png"
gen 150 "Square150x150Logo.png"
gen 284 "Square284x284Logo.png"
gen 310 "Square310x310Logo.png"

# Store
gen 50 "StoreLogo.png"

echo "Generating ICO..."

iconutil_tmp="$(mktemp -d)"

mkdir -p "$iconutil_tmp/icon.iconset"

sips -z 16 16   "$SRC" --out "$iconutil_tmp/icon.iconset/icon_16x16.png" >/dev/null
sips -z 32 32   "$SRC" --out "$iconutil_tmp/icon.iconset/icon_16x16@2x.png" >/dev/null
sips -z 32 32   "$SRC" --out "$iconutil_tmp/icon.iconset/icon_32x32.png" >/dev/null
sips -z 64 64   "$SRC" --out "$iconutil_tmp/icon.iconset/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "$SRC" --out "$iconutil_tmp/icon.iconset/icon_128x128.png" >/dev/null
sips -z 256 256 "$SRC" --out "$iconutil_tmp/icon.iconset/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$SRC" --out "$iconutil_tmp/icon.iconset/icon_256x256.png" >/dev/null
sips -z 512 512 "$SRC" --out "$iconutil_tmp/icon.iconset/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$SRC" --out "$iconutil_tmp/icon.iconset/icon_512x512.png" >/dev/null
cp "$SRC" "$iconutil_tmp/icon.iconset/icon_512x512@2x.png"

iconutil -c icns "$iconutil_tmp/icon.iconset" \
  -o "$ICON_DIR/icon.icns"

echo "Generating Windows ICO..."

if command -v magick >/dev/null 2>&1; then
  magick "$SRC" \
    -define icon:auto-resize=256,128,64,48,32,16 \
    "$ICON_DIR/icon.ico"

  echo "Generated icon.ico"
else
  echo "ImageMagick not installed, skipping icon.ico"
  echo "Install with: brew install imagemagick"
fi

rm -rf "$iconutil_tmp"

echo
echo "Done:"
echo "$ICON_DIR"