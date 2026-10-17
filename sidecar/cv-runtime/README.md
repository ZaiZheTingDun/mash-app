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

Release checks compare this directory (excluding tests and Markdown docs)
and the target platform wrapper against the pinned runtime tag. Business
changes in `cv-code/` do not require rebuilding or publishing the runtime.
App releases still test their bundled code against the exact pinned runtime.
See [distribution](../../docs/distribution.md).
