#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/bump-app-version.sh x.y.z

Updates the app version in versions.toml, src-tauri/tauri.conf.json,
src-tauri/Cargo.toml, and src-tauri/Cargo.lock, then commits and tags
the current HEAD as vx.y.z.

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
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)*$ ]] || fail "version must look like x.y.z; got $VERSION"
TAG="v$VERSION"

require_cmd cargo
require_cmd git
require_cmd node

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  fail "worktree must be clean before bumping the app version"
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  fail "tag already exists: $TAG"
fi

node - "$VERSION" <<'NODE'
const fs = require("fs");
const version = process.argv[2];

function replaceOne(path, pattern, replacement, label) {
  const original = fs.readFileSync(path, "utf8");
  const next = original.replace(pattern, replacement);
  if (next === original) {
    console.error(`error: did not update ${label} in ${path}`);
    process.exit(1);
  }
  fs.writeFileSync(path, next);
}

replaceOne(
  "versions.toml",
  /(\[app\]\s*\nversion\s*=\s*")[^"]+(")/,
  `$1${version}$2`,
  "[app].version",
);

const tauriConfigPath = "src-tauri/tauri.conf.json";
const tauriConfig = JSON.parse(fs.readFileSync(tauriConfigPath, "utf8"));
tauriConfig.version = version;
fs.writeFileSync(tauriConfigPath, `${JSON.stringify(tauriConfig, null, 2)}\n`);

replaceOne(
  "src-tauri/Cargo.toml",
  /(\[package\][\s\S]*?\nversion\s*=\s*")[^"]+(")/,
  `$1${version}$2`,
  "package.version",
);
NODE

cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 >/dev/null

APP_VERSION="$(
  node -e 'const fs = require("fs"); const c = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")); process.stdout.write(c.version || "");'
)"
[[ "$APP_VERSION" == "$VERSION" ]] || fail "tauri.conf.json version mismatch: $APP_VERSION"

CARGO_VERSION="$(
  node -e 'const fs = require("fs"); const text = fs.readFileSync("src-tauri/Cargo.toml", "utf8"); const m = text.match(/\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/); process.stdout.write(m?.[1] || "");'
)"
[[ "$CARGO_VERSION" == "$VERSION" ]] || fail "Cargo.toml version mismatch: $CARGO_VERSION"

LOCK_VERSION="$(
  node -e 'const fs = require("fs"); const text = fs.readFileSync("src-tauri/Cargo.lock", "utf8"); const m = text.match(/\[\[package\]\]\nname = "mash"\nversion = "([^"]+)"/); process.stdout.write(m?.[1] || "");'
)"
[[ "$LOCK_VERSION" == "$VERSION" ]] || fail "Cargo.lock version mismatch: $LOCK_VERSION"

git add versions.toml src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "Bump app to $VERSION"
git tag "$TAG"

echo "Created commit and tag $TAG"
