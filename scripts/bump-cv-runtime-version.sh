#!/usr/bin/env bash
set -euo pipefail

PLATFORM=""

usage() {
  cat <<'EOF'
Usage:
  scripts/bump-cv-runtime-version.sh [--platform <platform>] <runtime-version>

Updates [mash_cv].runtime in versions.toml, commits the change, and tags the
current HEAD as cv-runtime/<platform>/<runtime-version>.

Examples:
  scripts/bump-cv-runtime-version.sh 2026.05.17-runtime2
  scripts/bump-cv-runtime-version.sh --platform darwin-aarch64 2026.05.17-runtime2

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

detect_platform() {
  local os arch platform_os platform_arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os" in
    darwin) platform_os="darwin" ;;
    *) platform_os="$os" ;;
  esac
  case "$arch" in
    arm64) platform_arch="aarch64" ;;
    x86_64) platform_arch="x86_64" ;;
    *) platform_arch="$arch" ;;
  esac
  echo "${platform_os}-${platform_arch}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform)
      [[ $# -ge 2 ]] || fail "--platform requires a value"
      PLATFORM="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --*)
      fail "unknown option: $1"
      ;;
    *)
      break
      ;;
  esac
done

VERSION="${1:-}"
[[ -n "$VERSION" ]] || {
  usage >&2
  exit 1
}
[[ $# -eq 1 ]] || fail "expected exactly one runtime version"
[[ "$VERSION" =~ ^[0-9]{4}\.[0-9]{2}\.[0-9]{2}-runtime[0-9]+$ ]] || fail "runtime version must look like 2026.05.17-runtime2; got $VERSION"

require_cmd git
require_cmd node
require_cmd uname

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

if [[ -z "$PLATFORM" ]]; then
  PLATFORM="$(detect_platform)"
fi
[[ "$PLATFORM" =~ ^[A-Za-z0-9._-]+-[A-Za-z0-9._-]+$ ]] || fail "invalid platform: $PLATFORM"

TAG="cv-runtime/$PLATFORM/$VERSION"

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  fail "worktree must be clean before bumping the CV runtime version"
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
  /(\[mash_cv\][\s\S]*?\nruntime\s*=\s*")[^"]+(")/,
  `$1${version}$2`,
);
if (next === original) {
  console.error("error: did not update [mash_cv].runtime in versions.toml");
  process.exit(1);
}
fs.writeFileSync(path, next);
NODE

UPDATED_VERSION="$(
  node -e 'const fs = require("fs"); const text = fs.readFileSync("versions.toml", "utf8"); const m = text.match(/\[mash_cv\][\s\S]*?\nruntime\s*=\s*"([^"]+)"/); process.stdout.write(m?.[1] || "");'
)"
[[ "$UPDATED_VERSION" == "$VERSION" ]] || fail "versions.toml runtime mismatch: $UPDATED_VERSION"

git add versions.toml
git commit -m "Bump mash-cv runtime to $VERSION"
git tag "$TAG"

echo "Created commit and tag $TAG"
