#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/bump-cv-code-version.sh x.y.z

Updates [mash_cv].code in versions.toml, then commits and tags the
current HEAD as cv-code/x.y.z.

The worktree must be clean before running this script.
EOF
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

VERSION="${1:-}"
[[ -n "$VERSION" ]] || {
  usage >&2
  exit 1
}
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version must look like x.y.z; got $VERSION"
TAG="cv-code/$VERSION"

require_cmd git
require_cmd node

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  fail "worktree must be clean before bumping the CV code version"
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  fail "tag already exists: $TAG"
fi

node - "$VERSION" <<'NODE'
const fs = require("fs");
const version = process.argv[2];
const path = "versions.toml";
const original = fs.readFileSync(path, "utf8");
const next = original.replace(
  /(\[mash_cv\][\s\S]*?\ncode\s*=\s*")[^"]+(")/,
  `$1${version}$2`,
);
if (next === original) {
  console.error("error: did not update [mash_cv].code in versions.toml");
  process.exit(1);
}
fs.writeFileSync(path, next);
NODE

CODE_VERSION="$(
  node -e 'const fs = require("fs"); const text = fs.readFileSync("versions.toml", "utf8"); const m = text.match(/\[mash_cv\][\s\S]*?\ncode\s*=\s*"([^"]+)"/); process.stdout.write(m?.[1] || "");'
)"
[[ "$CODE_VERSION" == "$VERSION" ]] || fail "versions.toml CV code version mismatch: $CODE_VERSION"

git add versions.toml
git commit -m "Bump mash-cv code to $VERSION"
git tag "$TAG"

echo "Created commit and tag $TAG"
