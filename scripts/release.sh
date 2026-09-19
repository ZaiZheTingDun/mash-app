#!/usr/bin/env bash
set -euo pipefail

PLATFORM="darwin-aarch64"
REMOTE="origin"
CV_CODE_VERSION=""
CV_RUNTIME_VERSION=""
PUBLISH=false
PUBLISH_DMG=false
PUSH=false
SKIP_TESTS=false
APP_VERSION=""

usage() {
  cat <<'EOF'
Usage:
  mise exec -- scripts/release.sh <app-version> [options]

Plans or publishes a complete Mash release. Without --publish, the script is
read-only and prints the release plan.

Options:
  --cv-code <version>     Publish and apply a mash-cv code package.
  --cv-runtime <version>  Publish and apply a mash-cv runtime package.
  --platform <platform>   Runtime/updater platform (default: darwin-aarch64).
  --dmg                   Also publish the versioned/latest DMG installer.
  --push                  Push HEAD and release tags after publication.
  --remote <name>         Git remote used by --push (default: origin).
  --skip-tests            Skip pnpm build, Rust tests, and CV tests.
  --publish               Execute the plan. Required for any mutation/upload.
  -h, --help              Show this help.

Examples:
  mise exec -- scripts/release.sh 0.12.3
  mise exec -- scripts/release.sh 0.13.0 --cv-code 0.4.25 --publish
  mise exec -- scripts/release.sh 0.13.0 --cv-code 0.4.25 --dmg --push --publish

Before an actual release, commit the current worktree and add user-facing notes
under "## 未发布" in CHANGELOG.md. The script converts that heading to the
version and release date, commits it, runs the selected CV releases, publishes
the app updater, and optionally publishes the DMG and pushes Git refs.
EOF
}

fail() {
  echo "error: $*" >&2
  exit 1
}

note() {
  echo "==> $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

require_value() {
  local option="$1"
  local value="${2:-}"
  [[ -n "$value" && "$value" != --* ]] || fail "$option requires a value"
}

validate_version() {
  local label="$1"
  local version="$2"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "$label must look like x.y.z; got $version"
}

version_relation() {
  node - "$1" "$2" <<'NODE'
const [current, target] = process.argv.slice(2);
const parse = (value) => value.split(".").map(Number);
const a = parse(current);
const b = parse(target);
for (let i = 0; i < 3; i += 1) {
  if (b[i] > a[i]) process.exit(0);
  if (b[i] < a[i]) process.exit(2);
}
process.exit(1);
NODE
}

read_toml_version() {
  local section="$1"
  local key="$2"
  node - "$section" "$key" <<'NODE'
const fs = require("fs");
const [section, key] = process.argv.slice(2);
const text = fs.readFileSync("versions.toml", "utf8");
const escapedSection = section.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const match = text.match(new RegExp(`\\[${escapedSection}\\][\\s\\S]*?\\n${escapedKey}\\s*=\\s*"([^"]+)"`));
if (!match) process.exit(1);
process.stdout.write(match[1]);
NODE
}

latest_tag() {
  git for-each-ref \
    --count=1 \
    --sort=-version:refname \
    --format='%(refname:strip=2)' \
    "refs/tags/$1"
}

has_changes_since() {
  local ref="$1"
  shift
  [[ -n "$ref" ]] || return 1
  ! git diff --quiet "$ref"..HEAD -- "$@"
}

print_command() {
  printf '  +'
  printf ' %q' "$@"
  printf '\n'
}

run_command() {
  print_command "$@"
  "$@"
}

run_cv_tests() {
  print_command poetry run pytest
  (
    cd sidecar/mash_cv
    poetry run pytest
  )
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cv-code)
      require_value "$1" "${2:-}"
      CV_CODE_VERSION="$2"
      shift 2
      ;;
    --cv-runtime)
      require_value "$1" "${2:-}"
      CV_RUNTIME_VERSION="$2"
      shift 2
      ;;
    --platform)
      require_value "$1" "${2:-}"
      PLATFORM="$2"
      shift 2
      ;;
    --remote)
      require_value "$1" "${2:-}"
      REMOTE="$2"
      shift 2
      ;;
    --dmg)
      PUBLISH_DMG=true
      shift
      ;;
    --push)
      PUSH=true
      shift
      ;;
    --skip-tests)
      SKIP_TESTS=true
      shift
      ;;
    --publish)
      PUBLISH=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --*)
      fail "unknown option: $1"
      ;;
    *)
      [[ -z "$APP_VERSION" ]] || fail "unexpected argument: $1"
      APP_VERSION="$1"
      shift
      ;;
  esac
done

[[ -n "$APP_VERSION" ]] || {
  usage >&2
  exit 1
}

validate_version "app version" "$APP_VERSION"
[[ -z "$CV_CODE_VERSION" ]] || validate_version "CV code version" "$CV_CODE_VERSION"
[[ -z "$CV_RUNTIME_VERSION" ]] || validate_version "CV runtime version" "$CV_RUNTIME_VERSION"
[[ "$PLATFORM" =~ ^[A-Za-z0-9._-]+-[A-Za-z0-9._-]+$ ]] || fail "invalid platform: $PLATFORM"
[[ "$REMOTE" =~ ^[A-Za-z0-9._/-]+$ ]] || fail "invalid git remote: $REMOTE"

require_cmd git
require_cmd node
require_cmd rg

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

CURRENT_APP_VERSION="$(read_toml_version app version)"
CURRENT_CV_CODE_VERSION="$(read_toml_version mash_cv code)"
CURRENT_CV_RUNTIME_VERSION="$(read_toml_version mash_cv runtime)"
LAST_APP_TAG="$(latest_tag 'v*')"
LAST_CV_CODE_TAG="$(latest_tag 'cv-code/*')"
RUNTIME_BASE_REF="$(git log -1 --format='%H' -S "runtime = \"$CURRENT_CV_RUNTIME_VERSION\"" -- versions.toml)"
[[ -n "$RUNTIME_BASE_REF" ]] || RUNTIME_BASE_REF="$LAST_APP_TAG"
APP_TAG="v$APP_VERSION"
CV_CODE_TAG=""
CV_RUNTIME_TAG=""
[[ -z "$CV_CODE_VERSION" ]] || CV_CODE_TAG="cv-code/$CV_CODE_VERSION"
[[ -z "$CV_RUNTIME_VERSION" ]] || CV_RUNTIME_TAG="cv-runtime/$PLATFORM/$CV_RUNTIME_VERSION"

if [[ "$CURRENT_APP_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  if version_relation "$CURRENT_APP_VERSION" "$APP_VERSION"; then
    :
  else
    relation_status="$?"
    if [[ "$relation_status" -eq 2 ]]; then
      fail "app version $APP_VERSION is older than current version $CURRENT_APP_VERSION"
    fi
    [[ -n "$(git tag --points-at HEAD --list "$APP_TAG")" ]] || fail "app version $APP_VERSION is already current but HEAD is not tagged $APP_TAG"
  fi
fi

if [[ -n "$CV_CODE_VERSION" && "$CURRENT_CV_CODE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  if ! version_relation "$CURRENT_CV_CODE_VERSION" "$CV_CODE_VERSION"; then
    fail "CV code version $CV_CODE_VERSION must be newer than current version $CURRENT_CV_CODE_VERSION"
  fi
fi
if [[ -n "$CV_RUNTIME_VERSION" && "$CURRENT_CV_RUNTIME_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  if ! version_relation "$CURRENT_CV_RUNTIME_VERSION" "$CV_RUNTIME_VERSION"; then
    fail "CV runtime version $CV_RUNTIME_VERSION must be newer than current version $CURRENT_CV_RUNTIME_VERSION"
  fi
fi

if [[ "$APP_VERSION" != "$CURRENT_APP_VERSION" ]] && git rev-parse -q --verify "refs/tags/$APP_TAG" >/dev/null; then
  fail "tag already exists: $APP_TAG"
fi
if [[ -n "$CV_CODE_TAG" ]] && git rev-parse -q --verify "refs/tags/$CV_CODE_TAG" >/dev/null; then
  fail "tag already exists: $CV_CODE_TAG"
fi
if [[ -n "$CV_RUNTIME_TAG" ]] && git rev-parse -q --verify "refs/tags/$CV_RUNTIME_TAG" >/dev/null; then
  fail "tag already exists: $CV_RUNTIME_TAG"
fi

CHANGELOG_STATE=""
if rg -q "^## v${APP_VERSION} - [0-9]{4}-[0-9]{2}-[0-9]{2}$" CHANGELOG.md; then
  CHANGELOG_STATE="ready"
elif rg -q '^## 未发布$' CHANGELOG.md; then
  CHANGELOG_STATE="unreleased"
else
  fail "CHANGELOG.md must contain either '## 未发布' or a v$APP_VERSION release heading"
fi

CV_CODE_CHANGED=false
if has_changes_since "$LAST_CV_CODE_TAG" \
  sidecar/mash_cv/mash_cv \
  sidecar/mash_cv/tests \
  src-tauri/resources/cv.json \
  src-tauri/resources/templates \
  src-tauri/resources/servers; then
  CV_CODE_CHANGED=true
fi

CV_RUNTIME_CHANGED=false
if has_changes_since "$RUNTIME_BASE_REF" \
  sidecar/mash_cv/pyproject.toml \
  sidecar/mash_cv/poetry.lock \
  sidecar/mash_cv/build_sidecar.sh \
  src-tauri/resources/scrcpy; then
  CV_RUNTIME_CHANGED=true
fi

if [[ "$CV_CODE_CHANGED" == true && -z "$CV_CODE_VERSION" ]]; then
  if [[ "$PUBLISH" == true ]]; then
    fail "CV code changes detected since ${LAST_CV_CODE_TAG:-the beginning}; pass --cv-code <version>"
  fi
  echo "warning: CV code changes detected; an actual release requires --cv-code <version>" >&2
fi
if [[ "$CV_RUNTIME_CHANGED" == true && -z "$CV_RUNTIME_VERSION" ]]; then
  if [[ "$PUBLISH" == true ]]; then
    fail "CV runtime changes detected since the current runtime was selected; pass --cv-runtime <version>"
  fi
  echo "warning: CV runtime changes detected; an actual release requires --cv-runtime <version>" >&2
fi

note "Release summary"
echo "  app:             $CURRENT_APP_VERSION -> $APP_VERSION"
echo "  changelog:       $CHANGELOG_STATE"
echo "  CV code:         ${CV_CODE_VERSION:-unchanged} (changes detected: $CV_CODE_CHANGED)"
echo "  CV runtime:      ${CV_RUNTIME_VERSION:-unchanged} (changes detected: $CV_RUNTIME_CHANGED)"
echo "  platform:        $PLATFORM"
echo "  publish DMG:     $PUBLISH_DMG"
echo "  push Git refs:   $PUSH"
echo "  tests:           $([[ "$SKIP_TESTS" == true ]] && echo skipped || echo enabled)"

if [[ "$PUBLISH" != true ]]; then
  note "Read-only plan"
  if [[ "$SKIP_TESTS" != true ]]; then
    print_command pnpm build
    print_command cargo test --manifest-path src-tauri/Cargo.toml
    if [[ -n "$CV_CODE_VERSION" || -n "$CV_RUNTIME_VERSION" ]]; then
      echo "  + (cd sidecar/mash_cv && poetry run pytest)"
    fi
  fi
  if [[ "$CHANGELOG_STATE" == "unreleased" ]]; then
    echo "  + finalize and commit CHANGELOG.md for v$APP_VERSION"
  fi
  if [[ -n "$CV_RUNTIME_VERSION" ]]; then
    print_command scripts/release-cv-runtime.sh --platform "$PLATFORM" "$CV_RUNTIME_VERSION"
  fi
  if [[ -n "$CV_CODE_VERSION" ]]; then
    print_command scripts/bump-cv-code-version.sh "$CV_CODE_VERSION"
    print_command scripts/release-cv-code.sh "$CV_CODE_VERSION"
    echo "  + apply CV code URL/SHA to runtime-manifest.json and commit"
  fi
  print_command scripts/bump-app-version.sh "$APP_VERSION"
  print_command scripts/release-tauri-updater.sh
  [[ "$PUBLISH_DMG" != true ]] || print_command scripts/release-dmg.sh
  if [[ "$PUSH" == true ]]; then
    print_command git push "$REMOTE" HEAD
    echo "  + push the release tags created by this run"
  else
    echo "  + leave commits and tags local"
  fi
  echo
  echo "No files, tags, artifacts, or remote refs were changed. Add --publish to execute this plan."
  exit 0
fi

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  fail "worktree must be clean before publishing"
fi

require_cmd cargo
require_cmd awk
require_cmd curl
require_cmd date
require_cmd pnpm
require_cmd shasum
[[ -z "$CV_CODE_VERSION" && -z "$CV_RUNTIME_VERSION" ]] || require_cmd poetry

[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required; run through 'mise exec --'"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required; run through 'mise exec --'"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required; run through 'mise exec --'"
[[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] || fail "TAURI_SIGNING_PRIVATE_KEY is required; run through 'mise exec --'"
[[ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]] || fail "TAURI_SIGNING_PRIVATE_KEY_PASSWORD is required; run through 'mise exec --'"

RELEASE_PREFIX="${R2_PREFIX:-mash}"
RELEASE_PREFIX="${RELEASE_PREFIX#/}"
RELEASE_PREFIX="${RELEASE_PREFIX%/}"

RELEASE_PROBE_URL="${RELEASE_BASE_URL%/}/$RELEASE_PREFIX/releases/${RELEASE_CHANNEL:-stable}/latest.json"
note "Checking updater channel endpoint"
print_command curl --location --silent --show-error --head "$RELEASE_PROBE_URL"
RELEASE_PROBE_STATUS="$(
  curl --location --silent --show-error \
    --head \
    --output /dev/null \
    --write-out '%{http_code}' \
    "$RELEASE_PROBE_URL"
)"
case "$RELEASE_PROBE_STATUS" in
  2??|3??)
    echo "  reachable: HTTP $RELEASE_PROBE_STATUS"
    ;;
  404)
    echo "  channel is not published yet (HTTP 404); connectivity is available"
    ;;
  *)
    fail "updater channel probe returned HTTP $RELEASE_PROBE_STATUS: $RELEASE_PROBE_URL"
    ;;
esac

if [[ "$SKIP_TESTS" != true ]]; then
  note "Running release validation"
  run_command pnpm build
  run_command cargo test --manifest-path src-tauri/Cargo.toml
  if [[ -n "$CV_CODE_VERSION" || -n "$CV_RUNTIME_VERSION" ]]; then
    run_cv_tests
  fi
else
  note "Skipping tests by request"
fi

CREATED_TAGS=()

if [[ "$CHANGELOG_STATE" == "unreleased" ]]; then
  RELEASE_DATE="$(date +%Y-%m-%d)"
  note "Finalizing release notes"
  node - "$APP_VERSION" "$RELEASE_DATE" <<'NODE'
const fs = require("fs");
const [version, date] = process.argv.slice(2);
const path = "CHANGELOG.md";
const original = fs.readFileSync(path, "utf8");
const marker = "## 未发布";
if (!original.includes(marker)) throw new Error(`missing ${marker}`);
const next = original.replace(marker, `## v${version} - ${date}`);
fs.writeFileSync(path, next);
NODE
  run_command git add CHANGELOG.md
  run_command git commit -m "docs: add v$APP_VERSION release notes"
fi

if [[ -n "$CV_RUNTIME_VERSION" ]]; then
  note "Publishing mash-cv runtime $CV_RUNTIME_VERSION"
  run_command scripts/release-cv-runtime.sh --platform "$PLATFORM" "$CV_RUNTIME_VERSION"
  CREATED_TAGS+=("$CV_RUNTIME_TAG")
fi

if [[ -n "$CV_CODE_VERSION" ]]; then
  note "Publishing mash-cv code $CV_CODE_VERSION"
  run_command scripts/bump-cv-code-version.sh "$CV_CODE_VERSION"
  CREATED_TAGS+=("$CV_CODE_TAG")
  run_command scripts/release-cv-code.sh "$CV_CODE_VERSION"

  CODE_ARTIFACT="mash-cv-code-v${CV_CODE_VERSION}.zip"
  CODE_PATH="$REPO_ROOT/sidecar/mash_cv/dist/$CODE_ARTIFACT"
  [[ -f "$CODE_PATH" ]] || fail "CV code artifact not found after publication: $CODE_PATH"
  CODE_SHA256="$(shasum -a 256 "$CODE_PATH" | awk '{print $1}')"
  CODE_URL="${RELEASE_BASE_URL%/}/$RELEASE_PREFIX/runtime/mash-cv/code/$CODE_ARTIFACT"

  note "Applying mash-cv code artifact to runtime manifest"
  node - "$CV_CODE_VERSION" "$PLATFORM" "$CODE_URL" "$CODE_SHA256" <<'NODE'
const fs = require("fs");
const [version, platform, codeUrl, codeSha256] = process.argv.slice(2);
const path = "src-tauri/resources/runtime-manifest.json";
const manifest = JSON.parse(fs.readFileSync(path, "utf8"));
manifest.mashCvCodeVersion = version;
manifest.platforms ??= {};
manifest.platforms[platform] ??= {};
manifest.platforms[platform].codeUrl = codeUrl;
manifest.platforms[platform].codeSha256 = codeSha256;
fs.writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);

const reread = JSON.parse(fs.readFileSync(path, "utf8"));
if (reread.mashCvCodeVersion !== version) throw new Error("CV code version did not update");
if (reread.platforms?.[platform]?.codeUrl !== codeUrl) throw new Error("CV code URL did not update");
if (reread.platforms?.[platform]?.codeSha256 !== codeSha256) throw new Error("CV code SHA did not update");
NODE
  run_command git add src-tauri/resources/runtime-manifest.json
  run_command git commit -m "Apply mash-cv code $CV_CODE_VERSION"
fi

if [[ "$APP_VERSION" != "$CURRENT_APP_VERSION" ]]; then
  note "Bumping app to $APP_VERSION"
  run_command scripts/bump-app-version.sh "$APP_VERSION"
  CREATED_TAGS+=("$APP_TAG")
fi

note "Publishing Tauri updater"
TAURI_TARGET="$PLATFORM" run_command scripts/release-tauri-updater.sh

if [[ "$PUBLISH_DMG" == true ]]; then
  note "Publishing DMG installer"
  run_command scripts/release-dmg.sh
fi

if [[ "$PUSH" == true ]]; then
  note "Pushing release commits and tags"
  run_command git push "$REMOTE" HEAD
  if [[ "${#CREATED_TAGS[@]}" -gt 0 ]]; then
    run_command git push "$REMOTE" "${CREATED_TAGS[@]}"
  fi
  for tag in "${CREATED_TAGS[@]}"; do
    run_command git ls-remote --exit-code --tags "$REMOTE" "refs/tags/$tag"
  done
else
  note "Git refs remain local"
  echo "  git push $REMOTE HEAD"
  if [[ "${#CREATED_TAGS[@]}" -gt 0 ]]; then
    printf '  git push %s' "$REMOTE"
    printf ' %s' "${CREATED_TAGS[@]}"
    printf '\n'
  fi
fi

[[ -z "$(git status --porcelain)" ]] || fail "release finished with a dirty worktree"

note "Release complete"
echo "  app tag: $APP_TAG"
echo "  updater: ${RELEASE_BASE_URL%/}/$RELEASE_PREFIX/releases/${RELEASE_CHANNEL:-stable}/latest.json"
echo "  pushed:  $PUSH"
