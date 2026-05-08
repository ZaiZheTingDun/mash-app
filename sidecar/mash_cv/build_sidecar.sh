#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

RUNTIME_VERSION="${MASH_CV_RUNTIME_VERSION:-2026.05.08-runtime1}"
CODE_VERSION="${MASH_CV_CODE_VERSION:-2026.05.08-code1}"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS" in
  darwin) PLATFORM_OS="darwin" ;;
  *) PLATFORM_OS="$OS" ;;
esac
case "$ARCH" in
  arm64) PLATFORM_ARCH="aarch64" ;;
  x86_64) PLATFORM_ARCH="x86_64" ;;
  *) PLATFORM_ARCH="$ARCH" ;;
esac
PLATFORM="${PLATFORM_OS}-${PLATFORM_ARCH}"
RUNTIME_ARTIFACT="mash-cv-runtime-${PLATFORM}-v${RUNTIME_VERSION}.zip"
CODE_ARTIFACT="mash-cv-code-v${CODE_VERSION}.zip"

poetry install --no-root

# PyInstaller refuses to write into a non-empty distpath; nuke the
# previous build artefacts so the script is safe to re-run.
rm -rf dist build

poetry run pyinstaller --onedir runtime_launcher.py \
  --name mash-cv \
  --distpath dist \
  --workpath build \
  --specpath build \
  --hidden-import cv2 \
  --hidden-import numpy \
  --hidden-import rapidocr_onnxruntime \
  --collect-submodules av \
  --collect-binaries av \
  --collect-data rapidocr_onnxruntime \
  --collect-submodules rapidocr_onnxruntime \
  --add-data "${SCRIPT_DIR}/mash_cv/models:mash_cv/models"

mv dist/mash-cv dist/mash-cv-runtime
mkdir -p dist/mash-cv-code
cp -R mash_cv dist/mash-cv-code/mash_cv
rm -rf dist/mash-cv-code/mash_cv/models
find dist/mash-cv-code -name "__pycache__" -type d -prune -exec rm -rf {} +

(
  cd dist
  rm -f "$RUNTIME_ARTIFACT" "$CODE_ARTIFACT"
  zip -qry "$RUNTIME_ARTIFACT" mash-cv-runtime
  zip -qry "$CODE_ARTIFACT" mash-cv-code
)

RUNTIME_SHA256="$(shasum -a 256 "dist/${RUNTIME_ARTIFACT}" | awk '{print $1}')"
CODE_SHA256="$(shasum -a 256 "dist/${CODE_ARTIFACT}" | awk '{print $1}')"

echo "Platform: ${PLATFORM}"
echo "Runtime artifact: ${SCRIPT_DIR}/dist/${RUNTIME_ARTIFACT}"
echo "Runtime version: ${RUNTIME_VERSION}"
echo "Runtime sha256: ${RUNTIME_SHA256}"
echo "Code artifact: ${SCRIPT_DIR}/dist/${CODE_ARTIFACT}"
echo "Code version: ${CODE_VERSION}"
echo "Code sha256: ${CODE_SHA256}"
echo "Update src-tauri/resources/runtime-manifest.json with the final URL and sha256."
