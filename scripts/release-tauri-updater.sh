#!/usr/bin/env bash
set -euo pipefail

AWS_PROFILE="${AWS_PROFILE:-mash}"
R2_PREFIX="${R2_PREFIX:-mash}"
RELEASE_CHANNEL="${RELEASE_CHANNEL:-stable}"
TAURI_TARGET="${TAURI_TARGET:-darwin-aarch64}"

LONG_CACHE_CONTROL="${LONG_CACHE_CONTROL:-public, max-age=31536000, immutable}"
LATEST_CACHE_CONTROL="${LATEST_CACHE_CONTROL:-no-cache}"

usage() {
  cat <<'EOF'
Usage:
  R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-tauri-updater.sh

Required environment:
  R2_ENDPOINT       Cloudflare R2 S3 endpoint, e.g. https://<accountid>.r2.cloudflarestorage.com
  R2_BUCKET         R2 bucket name
  RELEASE_BASE_URL  Public CDN base URL, e.g. https://cdn.example.com

Optional environment:
  AWS_PROFILE       AWS CLI profile to use (default: mash; set to empty to use AWS env credentials)
  R2_PREFIX         Object key prefix (default: mash)
  RELEASE_CHANNEL   Mutable channel directory (default: stable)
  TAURI_TARGET      Tauri updater platform key (default: darwin-aarch64)

The script must run from a clean worktree at an exact vX.Y.Z git tag.
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

[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required"

require_cmd aws
require_cmd curl
require_cmd find
require_cmd git
require_cmd node
require_cmd pnpm

aws_s3_cp() {
  if [[ -n "${AWS_PROFILE:-}" ]]; then
    aws s3 cp "$@" --profile "$AWS_PROFILE" --endpoint-url "$R2_ENDPOINT"
  else
    aws s3 cp "$@" --endpoint-url "$R2_ENDPOINT"
  fi
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

TAG="$(git describe --tags --exact-match 2>/dev/null || true)"
[[ -n "$TAG" ]] || fail "current commit is not an exact git tag"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)*$ ]] || fail "tag must look like vX.Y.Z; got $TAG"

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  fail "worktree must be clean before publishing"
fi

VERSION="${TAG#v}"
CONFIG_VERSION="$(
  node -e 'const fs = require("fs"); const c = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")); process.stdout.write(c.version || "");'
)"
[[ "$CONFIG_VERSION" == "$VERSION" ]] || fail "tag version $VERSION does not match src-tauri/tauri.conf.json version $CONFIG_VERSION"

R2_PREFIX="${R2_PREFIX#/}"
R2_PREFIX="${R2_PREFIX%/}"
RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
VERSION_PREFIX="$R2_PREFIX/releases/versions/$TAG/$TAURI_TARGET"
VERSION_LATEST_KEY="$R2_PREFIX/releases/versions/$TAG/latest.json"
CHANNEL_LATEST_KEY="$R2_PREFIX/releases/$RELEASE_CHANNEL/latest.json"
WORK_DIR="$REPO_ROOT/src-tauri/target/release-updater/$TAG"
LATEST_JSON="$WORK_DIR/latest.json"
REMOTE_LATEST_JSON="$WORK_DIR/remote-latest.json"

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

echo "Building Tauri release for $TAG ($TAURI_TARGET)"
pnpm tauri build

BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle"
[[ -d "$BUNDLE_DIR" ]] || fail "bundle directory not found: $BUNDLE_DIR"

SIG_FILES=()
while IFS= read -r sig_file; do
  SIG_FILES+=("$sig_file")
done < <(find "$BUNDLE_DIR" -type f -name '*.sig' | sort)
if [[ "${#SIG_FILES[@]}" -eq 0 ]]; then
  fail "no updater .sig files found under $BUNDLE_DIR; ensure TAURI_SIGNING_PRIVATE_KEY is configured"
fi
if [[ "${#SIG_FILES[@]}" -gt 1 ]]; then
  printf 'found updater signatures:\n' >&2
  printf '  %s\n' "${SIG_FILES[@]}" >&2
  fail "multiple updater artifacts found; set up the script for the desired target before publishing"
fi

SIG_FILE="${SIG_FILES[0]}"
ARTIFACT_FILE="${SIG_FILE%.sig}"
[[ -f "$ARTIFACT_FILE" ]] || fail "artifact for signature not found: $ARTIFACT_FILE"

ARTIFACT_NAME="$(basename "$ARTIFACT_FILE")"
SIG_NAME="$(basename "$SIG_FILE")"
ARTIFACT_KEY="$VERSION_PREFIX/$ARTIFACT_NAME"
SIG_KEY="$VERSION_PREFIX/$SIG_NAME"
ARTIFACT_URL="$RELEASE_BASE_URL/$ARTIFACT_KEY"
SIGNATURE="$(tr -d '\r\n' < "$SIG_FILE")"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

node - "$LATEST_JSON" "$VERSION" "$PUB_DATE" "$TAURI_TARGET" "$ARTIFACT_URL" "$SIGNATURE" <<'NODE'
const fs = require("fs");
const [out, version, pubDate, target, url, signature] = process.argv.slice(2);
const doc = {
  version,
  notes: `mash ${version}`,
  pub_date: pubDate,
  platforms: {
    [target]: {
      signature,
      url,
    },
  },
};
fs.writeFileSync(out, `${JSON.stringify(doc, null, 2)}\n`);
JSON.parse(fs.readFileSync(out, "utf8"));
NODE

echo "Uploading immutable artifacts"
aws_s3_cp "$ARTIFACT_FILE" "s3://$R2_BUCKET/$ARTIFACT_KEY" \
  --cache-control "$LONG_CACHE_CONTROL"
aws_s3_cp "$SIG_FILE" "s3://$R2_BUCKET/$SIG_KEY" \
  --cache-control "$LONG_CACHE_CONTROL"
aws_s3_cp "$LATEST_JSON" "s3://$R2_BUCKET/$VERSION_LATEST_KEY" \
  --cache-control "$LONG_CACHE_CONTROL" \
  --content-type "application/json"

echo "Publishing channel latest.json"
aws_s3_cp "$LATEST_JSON" "s3://$R2_BUCKET/$CHANNEL_LATEST_KEY" \
  --cache-control "$LATEST_CACHE_CONTROL" \
  --content-type "application/json"

LATEST_URL="$RELEASE_BASE_URL/$CHANNEL_LATEST_KEY"

echo "Validating public CDN URLs"
curl --fail --location --silent --show-error "$LATEST_URL" --output "$REMOTE_LATEST_JSON"
node - "$LATEST_JSON" "$REMOTE_LATEST_JSON" "$VERSION" "$TAURI_TARGET" "$ARTIFACT_URL" "$SIGNATURE" <<'NODE'
const fs = require("fs");
const [localPath, remotePath, version, target, url, signature] = process.argv.slice(2);
const local = JSON.parse(fs.readFileSync(localPath, "utf8"));
const remote = JSON.parse(fs.readFileSync(remotePath, "utf8"));

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

if (JSON.stringify(local) !== JSON.stringify(remote)) {
  fail("remote latest.json does not match the generated latest.json");
}
if (remote.version !== version) {
  fail(`remote latest.json version ${remote.version} does not match ${version}`);
}
const platform = remote.platforms && remote.platforms[target];
if (!platform) {
  fail(`remote latest.json is missing platforms.${target}`);
}
if (platform.url !== url) {
  fail("remote latest.json artifact URL does not match the uploaded artifact URL");
}
if (platform.signature !== signature) {
  fail("remote latest.json signature does not match the local .sig content");
}
NODE
curl --fail --location --silent --show-error --head "$ARTIFACT_URL" >/dev/null

echo "Published $TAG"
echo "  latest:   $LATEST_URL"
echo "  artifact: $ARTIFACT_URL"
