#!/usr/bin/env bash
set -euo pipefail

AWS_PROFILE="${AWS_PROFILE:-mash}"
R2_PREFIX="${R2_PREFIX:-mash}"
LONG_CACHE_CONTROL="${LONG_CACHE_CONTROL:-public, max-age=31536000, immutable}"
LATEST_CACHE_CONTROL="${LATEST_CACHE_CONTROL:-no-cache}"
RETAIN_VERSIONED_DMG_COUNT="${RETAIN_VERSIONED_DMG_COUNT:-3}"

usage() {
  cat <<'EOF'
Usage:
  R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-dmg.sh

Builds the macOS DMG installer and publishes it to R2:
  mash/downloads/versions/Mash_<version>_<arch>.dmg
  mash/downloads/latest/Mash_<arch>.dmg

Required environment:
  R2_ENDPOINT       Cloudflare R2 S3 endpoint, e.g. https://<accountid>.r2.cloudflarestorage.com
  R2_BUCKET         R2 bucket name
  RELEASE_BASE_URL  Public CDN base URL, e.g. https://mash.xiaotongx.com

Optional environment:
  AWS_PROFILE       AWS CLI profile to use (default: mash; set to empty to use AWS env credentials)
  R2_PREFIX         Object key prefix (default: mash)
  RETAIN_VERSIONED_DMG_COUNT
                    Number of versioned DMG files to keep per arch (default: 3)

The app version is read from src-tauri/tauri.conf.json.
EOF
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

aws_s3_cp() {
  if [[ -n "${AWS_PROFILE:-}" ]]; then
    aws s3 cp "$@" --profile "$AWS_PROFILE" --endpoint-url "$R2_ENDPOINT"
  else
    aws s3 cp "$@" --endpoint-url "$R2_ENDPOINT"
  fi
}

aws_s3_ls() {
  if [[ -n "${AWS_PROFILE:-}" ]]; then
    aws s3 ls "$@" --profile "$AWS_PROFILE" --endpoint-url "$R2_ENDPOINT"
  else
    aws s3 ls "$@" --endpoint-url "$R2_ENDPOINT"
  fi
}

aws_s3_rm() {
  if [[ -n "${AWS_PROFILE:-}" ]]; then
    aws s3 rm "$@" --profile "$AWS_PROFILE" --endpoint-url "$R2_ENDPOINT"
  else
    aws s3 rm "$@" --endpoint-url "$R2_ENDPOINT"
  fi
}

file_size_bytes() {
  if stat -f%z "$1" >/dev/null 2>&1; then
    stat -f%z "$1"
  else
    stat -c%s "$1"
  fi
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
[[ $# -eq 0 ]] || fail "release-dmg.sh does not accept positional arguments; app version is read from src-tauri/tauri.conf.json"
[[ "$RETAIN_VERSIONED_DMG_COUNT" =~ ^[0-9]+$ ]] || fail "RETAIN_VERSIONED_DMG_COUNT must be a non-negative integer"

[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required"

require_cmd aws
require_cmd awk
require_cmd curl
require_cmd find
require_cmd mkdir
require_cmd node
require_cmd pnpm
require_cmd shasum
require_cmd sort
require_cmd stat
require_cmd touch

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

VERSION="$(
  node -e 'const fs = require("fs"); const c = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")); process.stdout.write(c.version || "");'
)"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)*$ ]] || fail "src-tauri/tauri.conf.json version must look like x.y.z; got $VERSION"

PRODUCT_NAME="$(
  node -e 'const fs = require("fs"); const c = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")); process.stdout.write(c.productName || "");'
)"
[[ -n "$PRODUCT_NAME" ]] || fail "src-tauri/tauri.conf.json productName is empty"

WORK_DIR="$REPO_ROOT/src-tauri/target/release-dmg"
BUILD_MARKER="$WORK_DIR/build-start.marker"
mkdir -p "$WORK_DIR"
touch "$BUILD_MARKER"

echo "Building DMG for $PRODUCT_NAME $VERSION"
pnpm tauri build --bundles dmg

BUNDLE_DIRS=(
  "$REPO_ROOT/src-tauri/target/release/bundle/dmg"
  "$REPO_ROOT/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg"
  "$REPO_ROOT/src-tauri/target/x86_64-apple-darwin/release/bundle/dmg"
)

DMG_FILES=()
for bundle_dir in "${BUNDLE_DIRS[@]}"; do
  [[ -d "$bundle_dir" ]] || continue
  while IFS= read -r dmg_file; do
    DMG_FILES+=("$dmg_file")
  done < <(
    find "$bundle_dir" -type f -name "${PRODUCT_NAME}_${VERSION}_*.dmg" \
      -newer "$BUILD_MARKER" \
      | sort
  )
done

if [[ "${#DMG_FILES[@]}" -eq 0 ]]; then
  fail "no DMG found for ${PRODUCT_NAME}_${VERSION}_*.dmg under src-tauri/target/**/bundle/dmg"
fi
if [[ "${#DMG_FILES[@]}" -gt 1 ]]; then
  printf 'found multiple DMG artifacts:\n' >&2
  printf '  %s\n' "${DMG_FILES[@]}" >&2
  fail "expected exactly one DMG artifact"
fi

DMG_FILE="${DMG_FILES[0]}"
DMG_NAME="${DMG_FILE##*/}"
DMG_VERSION_PREFIX="${PRODUCT_NAME}_${VERSION}_"
DMG_ARCH="${DMG_NAME#"$DMG_VERSION_PREFIX"}"
DMG_ARCH="${DMG_ARCH%.dmg}"
[[ -n "$DMG_ARCH" && "$DMG_ARCH" != "$DMG_NAME" ]] || fail "could not derive DMG arch from $DMG_NAME"
LATEST_NAME="${DMG_NAME/_${VERSION}_/_}"
if [[ "$LATEST_NAME" == "$DMG_NAME" ]]; then
  fail "could not derive latest DMG name from $DMG_NAME"
fi

R2_PREFIX="${R2_PREFIX#/}"
R2_PREFIX="${R2_PREFIX%/}"
RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
VERSIONS_PREFIX="$R2_PREFIX/downloads/versions"
VERSION_KEY="$VERSIONS_PREFIX/$DMG_NAME"
LATEST_KEY="$R2_PREFIX/downloads/latest/$LATEST_NAME"
VERSION_URL="$RELEASE_BASE_URL/$VERSION_KEY"
LATEST_URL="$RELEASE_BASE_URL/$LATEST_KEY"
SHA256="$(shasum -a 256 "$DMG_FILE" | awk '{print $1}')"
SIZE_BYTES="$(file_size_bytes "$DMG_FILE")"

echo "Uploading versioned DMG"
echo "  file:   $DMG_NAME"
echo "  size:   $SIZE_BYTES bytes"
echo "  sha256: $SHA256"
echo "  target: s3://$R2_BUCKET/$VERSION_KEY"
echo "  url:    $VERSION_URL"
aws_s3_cp "$DMG_FILE" "s3://$R2_BUCKET/$VERSION_KEY" \
  --cache-control "$LONG_CACHE_CONTROL" \
  --content-type "application/x-apple-diskimage"

echo "Publishing latest DMG"
echo "  target: s3://$R2_BUCKET/$LATEST_KEY"
echo "  url:    $LATEST_URL"
aws_s3_cp "$DMG_FILE" "s3://$R2_BUCKET/$LATEST_KEY" \
  --cache-control "$LATEST_CACHE_CONTROL" \
  --content-type "application/x-apple-diskimage"

echo "Validating public DMG URLs"
curl --fail --location --silent --show-error --head "$VERSION_URL" >/dev/null
curl --fail --location --silent --show-error --head "$LATEST_URL" >/dev/null

echo "Pruning versioned DMG files for $DMG_ARCH"
PRUNED_COUNT=0
SEEN_COUNT=0
while IFS=$'\t' read -r _timestamp object_name; do
  [[ -n "${object_name:-}" ]] || continue
  SEEN_COUNT="$((SEEN_COUNT + 1))"
  if [[ "$SEEN_COUNT" -le "$RETAIN_VERSIONED_DMG_COUNT" ]]; then
    continue
  fi
  old_key="$VERSIONS_PREFIX/$object_name"
  echo "  deleting s3://$R2_BUCKET/$old_key"
  aws_s3_rm "s3://$R2_BUCKET/$old_key"
  PRUNED_COUNT="$((PRUNED_COUNT + 1))"
done < <(
  aws_s3_ls "s3://$R2_BUCKET/$VERSIONS_PREFIX/" \
    | awk -v product="$PRODUCT_NAME" -v arch="$DMG_ARCH" '
        $1 ~ /^[0-9][0-9][0-9][0-9]-/ && $2 ~ /^[0-9][0-9]:/ {
          name = $4
          prefix = product "_"
          suffix = "_" arch ".dmg"
          if (index(name, prefix) == 1 && substr(name, length(name) - length(suffix) + 1) == suffix) {
            print $1 "T" $2 "\t" name
          }
        }
      ' \
    | sort -r
)

echo "Published DMG installer"
echo "  versioned: $VERSION_URL"
echo "  latest:    $LATEST_URL"
echo "  pruned:    $PRUNED_COUNT old versioned DMG file(s)"
