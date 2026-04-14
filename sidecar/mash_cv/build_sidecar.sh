#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

poetry install

poetry run pyinstaller --onefile mash_cv/__main__.py \
  --name mash-cv \
  --distpath dist \
  --workpath build \
  --specpath build

triple=$(rustc --print host-tuple)
binaries_dir="../../src-tauri/binaries"
mkdir -p "$binaries_dir"
cp "dist/mash-cv" "${binaries_dir}/mash-cv-${triple}"

echo "Built sidecar: ${binaries_dir}/mash-cv-${triple}"
