# CV runtime

This directory owns the native Python runtime inputs: `pyproject.toml`,
`poetry.lock`, `runtime_launcher.py`, `models/`, and `build_runtime.py`.
Business code, its tests and training tools live in [cv-code](../cv-code/README.md).
The runtime launcher loads the code directory supplied by the app through
`MASH_CV_CODE_DIR`; OCR models keep their existing archive location at
`mash-cv-runtime/_internal/mash_cv/models/`.

## Development and tests

Requires Python 3.11–3.14 and Poetry 2.2.1 (CI uses Python 3.12).
From this directory:

```bash
poetry install --no-root
poetry run pytest
```

This runs both `../cv-code/tests/` and `tests/` with the same locked dependency
environment. To run only business tests from `../cv-code/`, use
`poetry -P ../cv-runtime run pytest`. No package installation or editable link
is needed. Development OCR resolves models from this directory; the app uses
models from its installed runtime.

## Native build

On the target platform, from this directory:

```bash
poetry run python build_runtime.py
```

Or use the platform wrappers from the repository root:

```bash
scripts/macos/build-cv.sh --runtime-only
```

```powershell
./scripts/windows/build-cv.ps1 -Mode runtime
```

Outputs `dist/mash-cv-runtime-<platform>-v<runtimeVersion>.zip`. The version
comes from `[mash_cv.runtime].<platform>` in `versions.toml` or the explicit
`MASH_CV_RUNTIME_VERSION` override. Wrappers also build the internal code
candidate by default; its ZIP goes into `../cv-code/dist/`.

App release validates the manifest's runtime version, URL and SHA-256, then
runs its bundled code against the exact pinned runtime on each platform.
Source file changes and directory moves do not by themselves require a new
runtime release. Publish a new runtime when native dependencies, OCR models
or launcher behavior actually need updating. Protocol/OCR smoke checks cover
the startup path and basic OCR, rather than every recognition feature.
Local release wrappers still use Git path checks to flag runtime source edits.
See [distribution](../../docs/distribution.md).

## PP-OCRv6 small test candidate

CN and JP share the bundled PP-OCRv6 small detector/recognizer and the legacy
text-direction classifier. RapidOCR is pinned to 3.10.0, ONNX Runtime to 1.24.4,
and each ORT session uses one inference thread. Model provenance, SHA-256 and
upstream license notices are under `models/`. No model download is needed at
runtime. Missing weights or an old native runtime produce an explicit error.
The worker's binary IPC format is unchanged.

This change needs a **new native runtime**, not just a code ZIP. Published
runtime URLs/hashes in the app manifest are deliberately left untouched until
platform artifacts are published. Do not overwrite existing published ZIPs.
The PR's Windows CV job builds both ZIPs as `mash-cv-windows-x64` artifacts.

For a local Windows test, check out this branch, install dependencies and run:

```powershell
$env:MASH_CV_RUNTIME_VERSION = '0.2.0'
./scripts/windows/build-cv.ps1
```

Before building/running the app from this branch, point its **local test**
manifest at the newly built ZIP's version and real SHA-256. For example:

```powershell
$zip = 'sidecar/cv-runtime/dist/mash-cv-runtime-windows-x86_64-v0.2.0.zip'
$path = 'src-tauri/resources/runtime-manifest.json'
$manifest = Get-Content -Raw $path | ConvertFrom-Json
$manifest.platforms.'windows-x86_64'.runtimeVersion = '0.2.0'
$manifest.platforms.'windows-x86_64'.runtimeSha256 = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path (Get-Location) $path), ($manifest | ConvertTo-Json -Depth 10), (New-Object Text.UTF8Encoding $false))
```

Run the app from this checkout and import this runtime ZIP through resource
settings. Use **local import**, not the production download button: the URL
still refers to the published runtime. The app uses this checkout's new CV
source in development and bundles it in a local release build. If using CI's
ZIPs, compute the hash from the inner runtime ZIP, not the Actions artifact
wrapper, and use its filename's runtime version. Do not commit these temporary
manifest changes; publish and pin actual platform artifacts before an App
release. Check the OCR log for `unified PP-OCRv6 small` and `recognizer ready`.
