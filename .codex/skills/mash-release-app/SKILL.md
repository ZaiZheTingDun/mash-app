---
name: mash-release-app
description: Delegate to mash-releaser subagent. Use this skill when publishing the full Mash desktop app release, with app-bundled Python CV code, independently versioned native runtimes, updater publication, and release validation.
---

# Mash Full App Release

Use this workflow to publish Mash end to end. Python CV code ships in the app; only native runtimes are downloaded separately. See `docs/distribution.md` for the maintained CI workflow and environment settings.

## Release Invariants

- Start from repo root and a clean worktree unless you are intentionally preparing a release commit.
- Do not publish mutable app updater metadata until immutable versioned artifacts are uploaded and validated.
- Keep `versions.toml`, `src-tauri/resources/runtime-manifest.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `src-tauri/Cargo.lock` aligned.
- CV code has no independent version, tag, remote ZIP, or download/install step. `build.rs` stages it into app resources, excluding OCR models and Python caches. Internal code ZIPs are compatibility-test candidates only; never publish them separately.
- Preserve existing user changes. If the worktree is dirty, inspect it and stop unless the dirty files are the intended release edits.
- User-facing release notes should describe only visible changes. Use `$mash-release-notes` when drafting them.

## Required Environment

Confirm these before publishing anything.

The release-required environment variables are already defined in the repo's
`.mise.toml`. Do not export them manually for release runs; execute the scripts
inside `mise` so those values are injected consistently.

For local preflight checks, verify the environment is available through `mise`
and then run the checks in that same environment:

```bash
mise exec -- git status --short
mise exec -- pnpm install
mise exec -- test -n "${R2_ENDPOINT:-}"
mise exec -- test -n "${R2_BUCKET:-}"
mise exec -- test -n "${RELEASE_BASE_URL:-}"
mise exec -- test -n "${TAURI_SIGNING_PRIVATE_KEY:-}"
mise exec -- test -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
mise exec -- /bin/zsh -lc 'prefix="${R2_PREFIX:-mash}"; prefix="${prefix#/}"; prefix="${prefix%/}"; curl --fail --location --silent --show-error --head "${RELEASE_BASE_URL%/}/${prefix}/releases/${RELEASE_CHANNEL:-stable}/latest.json"'
```

The CDN root may legitimately return HTTP 404 because no object is published at
that key. Probe the current updater channel instead. A 404 at a brand-new channel
still proves connectivity; the artifact release scripts perform the authoritative
post-upload public URL checks.

Do not use `aws sts get-caller-identity` as a release preflight for this repo.
The configured R2-backed AWS profile can reject STS even when artifact uploads are
working. Prefer validating required environment variables up front and rely on the
release scripts' real checks:

- `scripts/macos/release-cv-runtime.sh` uploads the runtime zip and validates the
  public runtime URL after the upload completes.
- `scripts/macos/release-tauri-updater.sh` uploads immutable artifacts, publishes
  channel `latest.json`, and verifies the remote JSON plus artifact URL.

For app releases, default scripts assume `TAURI_TARGET=darwin-aarch64` and `RELEASE_CHANNEL=stable`. Override explicitly when publishing another target or channel.

## Detect What Changed

Compare against the latest relevant tags:

```bash
git tag --list 'cv-runtime/*/*' --sort=-v:refname | head -10
git tag --list 'v*' --sort=-v:refname | head -5
node scripts/shared/runtime-inputs.mjs <current-platform-runtime-tag> HEAD <platform>
git diff --name-only <last-app-tag>..HEAD
```

Use judgment:

- CV runtime bump: the app actually needs updated native dependencies, OCR models or launcher behavior. Source moves and build-script refactors alone do not require publishing another runtime. Python code, app templates and the scrcpy JAR follow the app version.
- App bump: any user-visible app behavior, frontend, Rust backend, updater config, runtime manifest, resources consumed by the app, or published CV version changed.

Reuse native runtimes when they meet the app requirements. App release CI validates the pinned manifest and tests the bundled code against that exact runtime on each release platform; it does not use source-file changes or historical tags as a compatibility gate. These protocol/OCR smoke checks cover startup and basic OCR, not every recognition path. Local release wrappers retain their Git path checks for runtime source edits.

## Version Selection

Read current versions:

```bash
cat versions.toml
node -e 'const fs=require("fs"); const m=JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json","utf8")); console.log(m)'
```

Choose versions before running scripts and apply Semantic Versioning (SemVer)
consistently:

- Every newly selected app and CV runtime version must be a stable
  `MAJOR.MINOR.PATCH` value (`x.y.z`). Do not introduce date-based versions,
  arbitrary suffixes, or a new pre-release/build identifier for a stable
  channel release.
- Select the increment from the user-visible compatibility impact: increment
  `PATCH` for backward-compatible fixes or release-only corrections, `MINOR`
  for backward-compatible user-facing functionality, and `MAJOR` for breaking
  persisted-data, IPC, configuration, or runtime compatibility changes. Reset
  lower-order components after a higher-order increment.
- Compare against the latest matching release tag and existing manifest before
  choosing a version. The selected version must be strictly greater than the
  current one, must not reuse an existing tag, and must be used consistently in
  `versions.toml`, manifests, package metadata, and Git tags.
  For runtime, compare only with the target platform's configured version and
  use `cv-runtime/<platform>/<current-runtime-version>` as the change baseline;
  another platform's release must not reset that baseline.
- Validate each selected value before running a release script (for example,
  with `node` and a strict `^\\d+\\.\\d+\\.\\d+$` check). If a script still
  expects a historical date-style version, update the script/data contract or
  migrate the value to SemVer before publishing; do not create another
  date-style release.

- App version: semver `x.y.z`, matching `src-tauri/tauri.conf.json`, Cargo package, and tag `vX.Y.Z`.
- CV runtime version: semver `x.y.z`, tag `cv-runtime/<platform>/x.y.z`. Existing
  date-style runtime values are legacy inputs only and must not be used for a
  new stable release.

## Validation Before Publishing

Run the suites for touched layers before uploading artifacts:

```bash
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
(cd sidecar/cv-runtime && poetry run pytest)
```

If the release touches multiple layers, run all three. For final app confidence, run:

```bash
pnpm build
```

Document any skipped test and the reason.

## Publish CV Runtime

Only when runtime changed:

```bash
mise exec -- scripts/macos/release-cv-runtime.sh --platform darwin-aarch64 <runtime-version>
```

This script updates only `[mash_cv.runtime].<platform>` and that platform's `runtimeVersion`/URL/SHA in `runtime-manifest.json`, builds the runtime zip, uploads it, validates the URL, commits, and tags `cv-runtime/<platform>/<runtime-version>`. Runtime versions are independent per platform; Python code is bundled with the app.

After it finishes, verify:

```bash
git show --stat --oneline HEAD
git tag --points-at HEAD
cat versions.toml
node -e 'const fs=require("fs"); const m=JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json","utf8")); console.log(m.platforms["darwin-aarch64"].runtimeVersion, m.platforms["darwin-aarch64"].runtimeUrl, m.platforms["darwin-aarch64"].runtimeSha256)'
```

## Apply Latest CV To App

Before bumping the app, verify the app commit includes the intended per-platform runtime manifest and bundled Python source:

```bash
mise exec -- cat versions.toml
mise exec -- node -e 'const fs=require("fs"); const m=JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json","utf8")); console.log(JSON.stringify(m,null,2))'
mise exec -- git diff --stat $(git describe --tags --abbrev=0 --match 'v*')..HEAD
```

The app release must include the newest intended `runtime-manifest.json`, because installed apps use this file to discover external native runtime artifacts.

## Bump And Publish App

After CV artifacts and manifest commits are in place:

```bash
mise exec -- scripts/macos/bump-app-version.sh <app-version>
mise exec -- scripts/macos/release-tauri-updater.sh
```

`bump-app-version.sh` updates versions, commits, and tags `v<app-version>`. `release-tauri-updater.sh` must run from that exact clean tag; it builds the app, uploads immutable bundle/signature assets, publishes channel `latest.json`, and validates CDN URLs.

## Post-Publish Checks

Run these before calling the release done:

```bash
git status --short
git tag --points-at HEAD
curl --fail --location "$RELEASE_BASE_URL/${R2_PREFIX:-mash}/releases/${RELEASE_CHANNEL:-stable}/latest.json"
```

Also verify:

- The channel `latest.json` version matches the app tag.
- The artifact URL in `latest.json` is reachable.
- The app bundle signature was generated from the intended signing key.
- Published runtime URLs in `runtime-manifest.json` are reachable.
- Commits and tags that must leave the machine are pushed deliberately:

```bash
git push origin HEAD
git push origin cv-runtime/darwin-aarch64/<runtime-version> v<app-version>
```

Push only tags that were actually created in this release.

## Common Omissions To Catch

- Release notes/changelog for visible changes.
- Publishing the runtime for every supported platform, not just the local default.
- Signing key and updater password availability before the app build.
- CDN cache behavior: immutable artifacts use long cache; channel `latest.json` must be no-cache.
- Smoke test update/install on a clean profile or throwaway app data dir.
- Rollback note: the previous stable `latest.json` URL/version and the command or object needed to restore it.
- GitHub/Git remote push of release commits and all created tags.
- State-machine docs when automation states, routing, probes, or `cv.json` semantics changed.

## Windows entry points

Use native PowerShell scripts in scripts/windows/ on Windows. The complete entry
point is release.ps1 <app-version> (read-only by default); add -Publish to execute,
-CvRuntime <version>, -Installer and -Push as needed. Code ships with the app.
Individual entry points are bump-app-version.ps1, release-cv-runtime.ps1,
release-tauri-updater.ps1 and
release-installer.ps1. Shared Node helpers live in scripts/shared/.
See docs/windows.md for Windows environment setup and multi-platform ordering.
