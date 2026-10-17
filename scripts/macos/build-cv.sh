#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MODE="${1:---all}"
if [[ $# -gt 1 ]]; then
  echo "Usage: $0 [--all|--runtime-only|--code-only]" >&2
  exit 2
fi
case "$MODE" in
  --all|--runtime-only|--code-only) ;;
  *) echo "Unknown build mode: $MODE" >&2; exit 2 ;;
esac
cd "$SCRIPT_DIR/../../sidecar/cv-runtime"
if [[ "$MODE" != --code-only ]]; then
  poetry install --no-root
  poetry run python build_runtime.py
fi
if [[ "$MODE" != --runtime-only ]]; then
  python3 ../cv-code/build_code.py
fi
