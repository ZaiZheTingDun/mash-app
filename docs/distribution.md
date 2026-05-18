# Distribution

mash ships three independently versioned artifacts: the app, the `mash-cv` code package, and the `mash-cv` runtime package. All three are published to Cloudflare R2 behind the public CDN, but they have different release cadences and update entry points.

## R2 Layout

The current defaults are `R2_BUCKET=mash` and `R2_PREFIX=mash`. Public URLs are built by joining `RELEASE_BASE_URL` with the object key.

```text
mash/
  releases/
    versions/
      v0.2.2/
        darwin-aarch64/
          mash.app.tar.gz
          mash.app.tar.gz.sig
        latest.json
    stable/
      latest.json

  runtime/
    mash-cv/
      code/
        mash-cv-code-v0.2.2.zip
      runtime/
        darwin-aarch64/
          mash-cv-runtime-darwin-aarch64-v0.2.2.zip
```

In `s3://mash/mash/...`, the first `mash` is the bucket name and the second `mash` is the object prefix.

## Version Sources

Versions are managed in `versions.toml`:

```toml
[app]
version = "0.2.2"

[mash_cv]
runtime = "0.2.2"
code = "0.2.2"
```

All release versions use the `x.x.x` format. Tag conventions:

- App: `v0.2.2`
- CV code: `cv-code/0.2.2`
- CV runtime: `cv-runtime/darwin-aarch64/0.2.2`

## App Release

The app is distributed through Tauri updater. Clients read only the channel manifest:

```text
https://mash.xiaotongx.com/mash/releases/stable/latest.json
```

`latest.json` points to an immutable artifact:

```text
mash/releases/versions/v0.2.2/darwin-aarch64/mash.app.tar.gz
```

Release flow:

```bash
scripts/bump-app-version.sh 0.2.2
git push --follow-tags
```

Pushing a `v*.*.*` tag triggers `.github/workflows/release-app.yml`, which builds the macOS app and uploads the updater artifacts to R2. A local release is also supported:

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-tauri-updater.sh
```

The local script requires the current commit to be exactly on a `vX.Y.Z` tag and the worktree to be clean. It builds only the `app` bundle, so it does not create a DMG.

## CV Code Release

The CV code package contains the lightweight Python source. It is the high-frequency package for recognition logic changes under `sidecar/mash_cv/mash_cv/`.

Release flow:

```bash
scripts/bump-cv-code-version.sh 0.2.2
git tag cv-code/0.2.2
git push origin cv-code/0.2.2
```

Pushing a `cv-code/*` tag triggers `.github/workflows/release-cv-code.yml`, which calls `scripts/release-cv-code.sh`. The same release script can also be run locally:

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-code.sh 0.2.2
```

The script builds:

```text
sidecar/mash_cv/dist/mash-cv-code-v0.2.2.zip
```

and uploads it to:

```text
mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip
```

The workflow summary prints the public URL and SHA-256. Use those values to update `src-tauri/resources/runtime-manifest.json` for the current platform, including `codeUrl`, `codeSha256`, and `mashCvCodeVersion`.

## CV Runtime Release

The CV runtime package contains the PyInstaller onedir build, native dependencies, models, and runtime layout. It is large and should be released infrequently. Bump the runtime version only for changes to:

- PyInstaller dependencies
- Native dependencies
- OCR models or other heavy runtime resources
- Runtime launcher or archive layout

Release flow:

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-runtime.sh 0.2.2
```

The script:

1. Updates `[mash_cv].runtime` in `versions.toml`
2. Builds `mash-cv-runtime-<platform>-v<version>.zip`
3. Uploads the artifact to R2
4. Computes the SHA-256
5. Updates `src-tauri/resources/runtime-manifest.json`
6. Creates a commit
7. Tags the commit as `cv-runtime/<platform>/<version>`

Before uploading, the script prints the artifact name, size, S3 target, and public URL. After uploading, it prints the elapsed time and average upload speed.

## Runtime Manifest

`src-tauri/resources/runtime-manifest.json` is bundled with the app. At startup, the app uses it to determine which CV code/runtime versions must be installed locally.

```json
{
  "mashCvRuntimeVersion": "0.2.2",
  "mashCvCodeVersion": "0.2.2",
  "platforms": {
    "darwin-aarch64": {
      "runtimeUrl": "https://mash.xiaotongx.com/mash/runtime/mash-cv/runtime/darwin-aarch64/mash-cv-runtime-darwin-aarch64-v0.2.2.zip",
      "runtimeSha256": "...",
      "codeUrl": "https://mash.xiaotongx.com/mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip",
      "codeSha256": "..."
    }
  }
}
```

Local install paths:

```text
app_data_dir()/runtime/mash-cv/runtime/<mashCvRuntimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<mashCvCodeVersion>/mash-cv-code/
```

If the local machine is missing the manifest-required versions, the app asks the user to install the CV packages. Updating the app does not imply that the CV packages are already installed; the manifest and local install state decide whether another CV package install is needed.

## Upload Notes

R2 uploads use the S3 API endpoint:

```text
https://<account-id>.r2.cloudflarestorage.com
```

This endpoint is different from the public CDN domain. Cloudflare zone-level WAF, Bot, and Block AI Bots settings usually do not affect the S3 API endpoint. If TCP connect timeout occurs, the problem is on the network path from the local machine to the R2 S3 API. Common fixes:

- Route the terminal through a proxy or VPN
- Switch networks
- Upload from GitHub Actions instead

Connectivity check:

```bash
curl -I https://<account-id>.r2.cloudflarestorage.com
```

If this command times out, the release scripts will time out too.

## Cache Policy

Immutable artifacts use long caching:

```text
Cache-Control: public, max-age=31536000, immutable
```

Mutable channel manifests use short caching or no caching:

```text
Cache-Control: no-cache
```

For app releases, upload the versioned artifact first, then the versioned `latest.json`, and finally overwrite `stable/latest.json`. Otherwise clients may see the new version before its artifact has finished uploading.
