#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

poetry install

poetry run pyinstaller --onedir mash_cv/__main__.py \
  --name mash-cv \
  --distpath dist \
  --workpath build \
  --specpath build \
  --collect-submodules av \
  --collect-binaries av \
  --collect-data rapidocr_onnxruntime \
  --collect-submodules rapidocr_onnxruntime \
  --add-data "${SCRIPT_DIR}/mash_cv/models:mash_cv/models"

binaries_dir="../../src-tauri/binaries"
rm -rf "${binaries_dir}/mash-cv"
mkdir -p "$binaries_dir"
cp -R "dist/mash-cv" "${binaries_dir}/mash-cv"

echo "Built sidecar: ${binaries_dir}/mash-cv/mash-cv"
