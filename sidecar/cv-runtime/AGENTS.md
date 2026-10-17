# CV native runtime

- Keep native dependency declarations and lock file, OCR models, the frozen launcher and PyInstaller build logic here. Business source, fixtures and training tools belong in `../cv-code/`.
- Keep the runtime ZIP layout stable: `mash-cv-runtime/` contains the launcher and `_internal/mash_cv/models/` contains OCR models. Source directory moves must not change installed runtime paths.
- `build_runtime.py` must not import or bundle business code. `runtime_launcher.py` loads it through `MASH_CV_CODE_DIR` at process startup.
- App release checks the pinned manifest and runs bundled code against each published native runtime. Do not block CI App releases merely because runtime source files moved or changed; do not infer binary compatibility from Git history. Local release wrappers retain Git path checks for native build inputs.
- From this directory run `poetry install --no-root` and `poetry run pytest`; this includes the business tests in `../cv-code/tests/`. Frozen Windows smoke tests require a locally built runtime and internal code candidate; ordinary tests require no device or frozen bundle.
