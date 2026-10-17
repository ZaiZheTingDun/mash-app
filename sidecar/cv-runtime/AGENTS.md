# CV native runtime

- Keep native dependency declarations and lock file, OCR models, the frozen launcher and PyInstaller build logic here. Business source, fixtures and training tools belong in `../cv-code/`.
- Keep the runtime ZIP layout stable: `mash-cv-runtime/` contains the launcher and `_internal/mash_cv/models/` contains OCR models. Source directory moves must not change installed runtime paths.
- `build_runtime.py` must not import or bundle business code. `runtime_launcher.py` loads it through `MASH_CV_CODE_DIR` at process startup.
- Runtime release detection compares this directory (excluding `tests/` and Markdown docs) plus the target platform build wrapper using Git paths. Do not add semantic source parsing.
- From this directory run `poetry install --no-root` and `poetry run pytest`; this includes the business tests in `../cv-code/tests/`. Frozen Windows smoke tests require a locally built runtime and internal code candidate; ordinary tests require no device or frozen bundle.
