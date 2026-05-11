#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
VERSIONS_FILE="${REPO_ROOT}/versions.toml"
cd "$SCRIPT_DIR"

BUILD_RUNTIME=1
BUILD_CODE=1

usage() {
  cat <<'EOF'
Usage: ./build_sidecar.sh [--all|--runtime-only|--code-only]

Build mash-cv distribution artifacts.

  --all           Build runtime and code artifacts (default)
  --runtime-only  Build only the PyInstaller runtime artifact
  --code-only     Build only the lightweight Python code artifact
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)
      BUILD_RUNTIME=1
      BUILD_CODE=1
      ;;
    --runtime-only)
      BUILD_RUNTIME=1
      BUILD_CODE=0
      ;;
    --code-only)
      BUILD_RUNTIME=0
      BUILD_CODE=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

read_toml_value() {
  local section="$1"
  local key="$2"
  awk -v section="$section" -v key="$key" '
    $0 ~ "^[[:space:]]*\\[" section "\\][[:space:]]*$" { in_section = 1; next }
    $0 ~ "^[[:space:]]*\\[[^]]+\\][[:space:]]*$" { in_section = 0 }
    in_section && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      value = $0
      sub(/^[^=]*=[[:space:]]*/, "", value)
      sub(/[[:space:]]*(#.*)?$/, "", value)
      gsub(/^"|"$/, "", value)
      print value
      exit
    }
  ' "$VERSIONS_FILE"
}

if [[ ! -f "$VERSIONS_FILE" ]]; then
  echo "versions file not found: $VERSIONS_FILE" >&2
  exit 1
fi

DEFAULT_RUNTIME_VERSION="$(read_toml_value "mash_cv" "runtime")"
DEFAULT_CODE_VERSION="$(read_toml_value "mash_cv" "code")"
if [[ -z "$DEFAULT_RUNTIME_VERSION" || -z "$DEFAULT_CODE_VERSION" ]]; then
  echo "missing [mash_cv] runtime/code in $VERSIONS_FILE" >&2
  exit 1
fi

RUNTIME_VERSION="${MASH_CV_RUNTIME_VERSION:-$DEFAULT_RUNTIME_VERSION}"
CODE_VERSION="${MASH_CV_CODE_VERSION:-$DEFAULT_CODE_VERSION}"
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

# PyInstaller refuses to write into a non-empty distpath. For code-only builds,
# leave existing runtime artifacts untouched so their SHA-256 values remain
# stable.
mkdir -p dist

if [[ "$BUILD_RUNTIME" -eq 1 ]]; then
  rm -rf build dist/mash-cv dist/mash-cv-runtime
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
  (
    cd dist
    rm -f "$RUNTIME_ARTIFACT"
    zip -qry "$RUNTIME_ARTIFACT" mash-cv-runtime
  )
fi

if [[ "$BUILD_CODE" -eq 1 ]]; then
  rm -rf dist/mash-cv-code
  mkdir -p dist/mash-cv-code
  cp -R mash_cv dist/mash-cv-code/mash_cv
  rm -rf dist/mash-cv-code/mash_cv/models
  find dist/mash-cv-code -name "__pycache__" -type d -prune -exec rm -rf {} +
  (
    cd dist
    rm -f "$CODE_ARTIFACT"
    zip -qry "$CODE_ARTIFACT" mash-cv-code
  )
fi

echo "Platform: ${PLATFORM}"
if [[ "$BUILD_RUNTIME" -eq 1 ]]; then
  RUNTIME_SHA256="$(shasum -a 256 "dist/${RUNTIME_ARTIFACT}" | awk '{print $1}')"
  echo "Runtime artifact: ${SCRIPT_DIR}/dist/${RUNTIME_ARTIFACT}"
  echo "Runtime version: ${RUNTIME_VERSION}"
  echo "Runtime sha256: ${RUNTIME_SHA256}"
else
  echo "Runtime artifact: skipped"
fi
if [[ "$BUILD_CODE" -eq 1 ]]; then
  CODE_SHA256="$(shasum -a 256 "dist/${CODE_ARTIFACT}" | awk '{print $1}')"
  echo "Code artifact: ${SCRIPT_DIR}/dist/${CODE_ARTIFACT}"
  echo "Code version: ${CODE_VERSION}"
  echo "Code sha256: ${CODE_SHA256}"
else
  echo "Code artifact: skipped"
fi
echo "Update src-tauri/resources/runtime-manifest.json with the final URL and sha256."
