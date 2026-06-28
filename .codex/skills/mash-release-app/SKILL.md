---
name: mash-release-app
description: Use this skill when publishing the full Mash desktop app release, including detecting mash-cv code/runtime changes, bumping CV versions, uploading CV code/runtime artifacts, applying the latest runtime manifest to the app, bumping the Tauri app version, publishing the updater release, and validating tags, CDN assets, and release readiness.
---

# Mash Full App Release

Use this workflow to publish Mash end to end. It coordinates the Python CV sidecar artifacts and the Tauri desktop updater release.

## Release Invariants

- Start from repo root and a clean worktree unless you are intentionally preparing a release commit.
- Do not publish mutable app updater metadata until immutable versioned artifacts are uploaded and validated.
- Keep `versions.toml`, `src-tauri/resources/runtime-manifest.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `src-tauri/Cargo.lock` aligned.
- Treat `release-cv-code.sh` output as an input to a required manifest update; it uploads code but does not edit `runtime-manifest.json`.
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
mise exec -- aws sts get-caller-identity --profile "${AWS_PROFILE:-mash}"
mise exec -- test -n "${R2_ENDPOINT:-}"
mise exec -- test -n "${R2_BUCKET:-}"
mise exec -- test -n "${RELEASE_BASE_URL:-}"
mise exec -- test -n "${TAURI_SIGNING_PRIVATE_KEY:-}"
mise exec -- test -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
```

For app releases, default scripts assume `TAURI_TARGET=darwin-aarch64` and `RELEASE_CHANNEL=stable`. Override explicitly when publishing another target or channel.

## Detect What Changed

Compare against the latest relevant tags:

```bash
git tag --list 'cv-code/*' --sort=-v:refname | head -5
git tag --list 'cv-runtime/*/*' --sort=-v:refname | head -10
git tag --list 'v*' --sort=-v:refname | head -5
git diff --name-only <last-cv-code-tag>..HEAD -- sidecar/mash_cv/mash_cv sidecar/mash_cv/tests src-tauri/resources/cv.json src-tauri/resources/templates
git diff --name-only <last-cv-runtime-tag>..HEAD -- sidecar/mash_cv/pyproject.toml sidecar/mash_cv/poetry.lock sidecar/mash_cv/build_sidecar.sh sidecar/mash_cv/mash_cv src-tauri/resources/scrcpy
git diff --name-only <last-app-tag>..HEAD
```

Use judgment:

- CV code bump: Python package source, CV logic, templates, `cv.json`, tests, or sidecar code packaging changed.
- CV runtime bump: runtime dependencies, PyInstaller/build layout, Python/Poetry lockfiles, binary runtime assets, scrcpy runtime, or anything requiring a new heavy runtime bundle changed.
- App bump: any user-visible app behavior, frontend, Rust backend, updater config, runtime manifest, resources consumed by the app, or published CV version changed.

If no CV runtime changed, do not rebuild/publish the runtime. If no CV code changed and the manifest already points to the desired code artifact, do not republish code.

## Version Selection

Read current versions:

```bash
cat versions.toml
node -e 'const fs=require("fs"); const m=JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json","utf8")); console.log(m)'
```

Choose versions before running scripts:

- App version: semver `x.y.z`, matching `src-tauri/tauri.conf.json`, Cargo package, and tag `vX.Y.Z`.
- CV code version: semver `x.y.z`, tag `cv-code/x.y.z`.
- CV runtime version: prefer semver `x.y.z` unless the repository scripts have been updated to accept the current `versions.toml` date-style runtime string. `release-cv-runtime.sh` currently requires `x.y.z`.

## Validation Before Publishing

Run the suites for touched layers before uploading artifacts:

```bash
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
(cd sidecar/mash_cv && poetry run pytest)
```

If the release touches multiple layers, run all three. For final app confidence, run:

```bash
pnpm build
```

Document any skipped test and the reason.

## Publish CV Runtime

Only when runtime changed:

```bash
mise exec -- scripts/release-cv-runtime.sh --platform darwin-aarch64 <runtime-version>
```

This script updates `[mash_cv].runtime`, builds the runtime zip, uploads it, validates the URL, updates `runtime-manifest.json`, commits, and tags `cv-runtime/<platform>/<runtime-version>`.

After it finishes, verify:

```bash
git show --stat --oneline HEAD
git tag --points-at HEAD
cat versions.toml
node -e 'const fs=require("fs"); const m=JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json","utf8")); console.log(m.mashCvRuntimeVersion, m.platforms["darwin-aarch64"].runtimeUrl, m.platforms["darwin-aarch64"].runtimeSha256)'
```

## Publish CV Code

Only when CV code changed:

```bash
mise exec -- scripts/bump-cv-code-version.sh <code-version>
mise exec -- scripts/release-cv-code.sh <code-version>
```

Capture the printed `artifact` URL and `sha256`, then update `src-tauri/resources/runtime-manifest.json`:

- `mashCvCodeVersion`
- `platforms["darwin-aarch64"].codeUrl`
- `platforms["darwin-aarch64"].codeSha256`

Commit the manifest update after validating JSON:

```bash
node -e 'JSON.parse(require("fs").readFileSync("src-tauri/resources/runtime-manifest.json","utf8"))'
git add src-tauri/resources/runtime-manifest.json
git commit -m "Apply mash-cv code <code-version>"
```

If publishing multiple app targets that share the same code artifact, update each platform entry that the updater/runtime installer expects.

## Apply Latest CV To App

Before bumping the app, verify the app commit includes the intended CV runtime/code manifest:

```bash
mise exec -- cat versions.toml
mise exec -- node -e 'const fs=require("fs"); const m=JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json","utf8")); console.log(JSON.stringify(m,null,2))'
mise exec -- git diff --stat $(git describe --tags --abbrev=0 --match 'v*')..HEAD
```

The app release must include the newest intended `runtime-manifest.json`, because installed apps use this file to discover sidecar runtime/code artifacts.

## Bump And Publish App

After CV artifacts and manifest commits are in place:

```bash
mise exec -- scripts/bump-app-version.sh <app-version>
mise exec -- scripts/release-tauri-updater.sh
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
- Published CV code/runtime URLs in `runtime-manifest.json` are reachable.
- Commits and tags that must leave the machine are pushed deliberately:

```bash
git push origin HEAD
git push origin cv-code/<code-version> cv-runtime/darwin-aarch64/<runtime-version> v<app-version>
```

Push only tags that were actually created in this release.

## Common Omissions To Catch

- Release notes/changelog for visible changes.
- Updating `runtime-manifest.json` after publishing CV code.
- Publishing the runtime for every supported platform, not just the local default.
- Signing key and updater password availability before the app build.
- CDN cache behavior: immutable artifacts use long cache; channel `latest.json` must be no-cache.
- Smoke test update/install on a clean profile or throwaway app data dir.
- Rollback note: the previous stable `latest.json` URL/version and the command or object needed to restore it.
- GitHub/Git remote push of release commits and all created tags.
- State-machine docs when automation states, routing, probes, or `cv.json` semantics changed.
