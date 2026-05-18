#!/usr/bin/env bash
set -euo pipefail

AWS_PROFILE="${AWS_PROFILE:-mash}"
R2_PREFIX="${R2_PREFIX:-mash}"
LONG_CACHE_CONTROL="${LONG_CACHE_CONTROL:-public, max-age=31536000, immutable}"

usage() {
  cat <<'EOF'
Usage:
  R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-mash-cv-code.sh x.y.z

Builds the mash-cv code zip, uploads it to R2, validates the R2 object and
public URL, and prints the URL + SHA-256.

Optional environment:
  AWS_PROFILE       AWS CLI profile to use (default: mash; set to empty to use AWS env credentials)
  R2_PREFIX         Object key prefix (default: mash)

When GITHUB_OUTPUT is set, the script writes code_version, artifact, sha256,
object_key, and url outputs for GitHub Actions.
EOF
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

aws_args() {
  if [[ -n "${AWS_PROFILE:-}" ]]; then
    printf '%s\n' --profile "$AWS_PROFILE" --endpoint-url "$R2_ENDPOINT"
  else
    printf '%s\n' --endpoint-url "$R2_ENDPOINT"
  fi
}

aws_s3_cp() {
  local args=()
  while IFS= read -r arg; do
    args+=("$arg")
  done < <(aws_args)
  aws s3 cp "$@" "${args[@]}"
}

aws_s3api_head_object() {
  local args=()
  while IFS= read -r arg; do
    args+=("$arg")
  done < <(aws_args)
  aws s3api head-object "$@" "${args[@]}" >/dev/null
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
[[ $# -eq 1 ]] || fail "expected exactly one CV code version"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version must look like x.y.z; got $VERSION"

[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required"

require_cmd aws
require_cmd curl
require_cmd shasum

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

R2_PREFIX="${R2_PREFIX#/}"
R2_PREFIX="${R2_PREFIX%/}"
RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
ARTIFACT="mash-cv-code-v${VERSION}.zip"
DIST_PATH="$REPO_ROOT/sidecar/mash_cv/dist/$ARTIFACT"
OBJECT_KEY="$R2_PREFIX/runtime/mash-cv/code/$ARTIFACT"
URL="$RELEASE_BASE_URL/$OBJECT_KEY"

(
  cd sidecar/mash_cv
  MASH_CV_CODE_VERSION="$VERSION" ./build_sidecar.sh --code-only
)

[[ -f "$DIST_PATH" ]] || fail "CV code artifact not found: $DIST_PATH"
SHA256="$(shasum -a 256 "$DIST_PATH" | awk '{print $1}')"

echo "Uploading mash-cv code artifact"
echo "  file:   $ARTIFACT"
echo "  target: s3://$R2_BUCKET/$OBJECT_KEY"
echo "  url:    $URL"
aws_s3_cp "$DIST_PATH" "s3://$R2_BUCKET/$OBJECT_KEY" \
  --cache-control "$LONG_CACHE_CONTROL"

echo "Validating R2 object"
aws_s3api_head_object --bucket "$R2_BUCKET" --key "$OBJECT_KEY"

echo "Validating public artifact URL"
curl --fail --location --silent --show-error \
  --range 0-0 \
  --user-agent "mash-release/1.0" \
  "$URL" >/dev/null

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "code_version=$VERSION"
    echo "artifact=$ARTIFACT"
    echo "sha256=$SHA256"
    echo "object_key=$OBJECT_KEY"
    echo "url=$URL"
  } >> "$GITHUB_OUTPUT"
fi

echo "Released mash-cv code $VERSION"
echo "  artifact: $URL"
echo "  sha256:   $SHA256"
