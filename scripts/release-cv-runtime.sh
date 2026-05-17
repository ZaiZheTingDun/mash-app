#!/usr/bin/env bash
set -euo pipefail

AWS_PROFILE="${AWS_PROFILE:-mash}"
R2_PREFIX="${R2_PREFIX:-mash}"
LONG_CACHE_CONTROL="${LONG_CACHE_CONTROL:-public, max-age=31536000, immutable}"
PLATFORM=""

usage() {
  cat <<'EOF'
Usage:
  R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-runtime.sh [--platform <platform>] <runtime-version>

Updates [mash_cv].runtime in versions.toml, builds the mash-cv runtime zip,
uploads it to R2, updates src-tauri/resources/runtime-manifest.json, commits
the change, and tags HEAD as cv-runtime/<platform>/<runtime-version>.

Examples:
  scripts/release-cv-runtime.sh 0.2.2
  scripts/release-cv-runtime.sh --platform darwin-aarch64 0.2.2

Required environment:
  R2_ENDPOINT       Cloudflare R2 S3 endpoint, e.g. https://<accountid>.r2.cloudflarestorage.com
  R2_BUCKET         R2 bucket name
  RELEASE_BASE_URL  Public CDN base URL, e.g. https://cdn.example.com

Optional environment:
  AWS_PROFILE       AWS CLI profile to use (default: mash; set to empty to use AWS env credentials)
  R2_PREFIX         Object key prefix (default: mash)

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

aws_s3_cp() {
  if [[ -n "${AWS_PROFILE:-}" ]]; then
    aws s3 cp "$@" --profile "$AWS_PROFILE" --endpoint-url "$R2_ENDPOINT"
  else
    aws s3 cp "$@" --endpoint-url "$R2_ENDPOINT"
  fi
}

file_size_bytes() {
  if stat -f%z "$1" >/dev/null 2>&1; then
    stat -f%z "$1"
  else
    stat -c%s "$1"
  fi
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
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "runtime version must look like x.y.z; got $VERSION"

[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required"

require_cmd aws
require_cmd awk
require_cmd curl
require_cmd date
require_cmd git
require_cmd node
require_cmd shasum
require_cmd stat
require_cmd uname

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

if [[ -z "$PLATFORM" ]]; then
  PLATFORM="$(detect_platform)"
fi
[[ "$PLATFORM" =~ ^[A-Za-z0-9._-]+-[A-Za-z0-9._-]+$ ]] || fail "invalid platform: $PLATFORM"

TAG="cv-runtime/$PLATFORM/$VERSION"
ARTIFACT="mash-cv-runtime-${PLATFORM}-v${VERSION}.zip"
DIST_PATH="$REPO_ROOT/sidecar/mash_cv/dist/$ARTIFACT"
R2_PREFIX="${R2_PREFIX#/}"
R2_PREFIX="${R2_PREFIX%/}"
RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
OBJECT_KEY="$R2_PREFIX/runtime/mash-cv/runtime/$PLATFORM/$ARTIFACT"
RUNTIME_URL="$RELEASE_BASE_URL/$OBJECT_KEY"

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  fail "worktree must be clean before publishing the CV runtime"
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

(
  cd sidecar/mash_cv
  MASH_CV_RUNTIME_VERSION="$VERSION" ./build_sidecar.sh --runtime-only
)

[[ -f "$DIST_PATH" ]] || fail "runtime artifact not found: $DIST_PATH"
RUNTIME_SHA256="$(shasum -a 256 "$DIST_PATH" | awk '{print $1}')"
ARTIFACT_SIZE="$(du -h "$DIST_PATH" | awk '{print $1}')"
ARTIFACT_BYTES="$(file_size_bytes "$DIST_PATH")"

echo "Uploading runtime artifact"
echo "  file:   $ARTIFACT"
echo "  size:   $ARTIFACT_SIZE"
echo "  target: s3://$R2_BUCKET/$OBJECT_KEY"
echo "  url:    $RUNTIME_URL"
UPLOAD_STARTED_AT="$(date +%s)"
aws_s3_cp "$DIST_PATH" "s3://$R2_BUCKET/$OBJECT_KEY" \
  --cache-control "$LONG_CACHE_CONTROL"
UPLOAD_FINISHED_AT="$(date +%s)"
UPLOAD_SECONDS="$((UPLOAD_FINISHED_AT - UPLOAD_STARTED_AT))"
if [[ "$UPLOAD_SECONDS" -lt 1 ]]; then
  UPLOAD_SECONDS=1
fi
UPLOAD_SPEED="$(awk -v bytes="$ARTIFACT_BYTES" -v seconds="$UPLOAD_SECONDS" 'BEGIN { printf "%.2f MiB/s", bytes / seconds / 1024 / 1024 }')"
echo "Upload completed in ${UPLOAD_SECONDS}s (${UPLOAD_SPEED})"

echo "Validating public runtime URL"
curl --fail --location --silent --show-error --head "$RUNTIME_URL" >/dev/null

node - "$VERSION" "$PLATFORM" "$RUNTIME_URL" "$RUNTIME_SHA256" <<'NODE'
const fs = require("fs");
const [version, platform, runtimeUrl, runtimeSha256] = process.argv.slice(2);
const path = "src-tauri/resources/runtime-manifest.json";
const manifest = JSON.parse(fs.readFileSync(path, "utf8"));
manifest.mashCvRuntimeVersion = version;
manifest.platforms ??= {};
manifest.platforms[platform] ??= {};
manifest.platforms[platform].runtimeUrl = runtimeUrl;
manifest.platforms[platform].runtimeSha256 = runtimeSha256;
fs.writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);

const reread = JSON.parse(fs.readFileSync(path, "utf8"));
if (reread.mashCvRuntimeVersion !== version) {
  throw new Error("runtime manifest version did not update");
}
if (reread.platforms?.[platform]?.runtimeUrl !== runtimeUrl) {
  throw new Error("runtime manifest URL did not update");
}
if (reread.platforms?.[platform]?.runtimeSha256 !== runtimeSha256) {
  throw new Error("runtime manifest sha did not update");
}
NODE

git add versions.toml src-tauri/resources/runtime-manifest.json
git commit -m "Release mash-cv runtime $VERSION"
git tag "$TAG"

echo "Released mash-cv runtime $VERSION"
echo "  tag:     $TAG"
echo "  artifact: $RUNTIME_URL"
echo "  sha256:   $RUNTIME_SHA256"
