# CV Resources

Bundled CV configuration, templates and device-streaming resources. Installation and release details are covered in [distribution](../../docs/distribution.md).

## Layout

```text
resources/
  runtime-manifest.json       Required native runtime versions, URLs and SHA-256
  mash-cv-code/mash_cv/        Generated bundled Python code (no OCR models)
  assets-manifest.json        Target image-asset version
  servers/
    shared/{cv.json,templates/}
    jp/{cv.json,templates/}
    cn/{cv.json,templates/}
  images/                     Bundled UI images
  scrcpy/scrcpy-server.jar     Pinned streaming server
  adb/                        Platform-specific ADB binaries
```

Tauri resource maps preserve installed paths. Common resources belong in `tauri.conf.json`; ADB entries belong in `tauri.macos.conf.json` or `tauri.windows.conf.json` to avoid bundling another platform's binaries.

## Server configuration and templates

The app loads `servers/shared/` first, then the selected JP or CN bundle. Shared `cv.json` is deep-merged with the server config. Server selection is persisted in `server_settings.json`.

PNG keys preserve their relative path without the extension. Shared templates receive a `shared/` prefix; for example:

- `shared/templates/screen_team_party.png` → `shared/screen_team_party`
- `jp/templates/battle/button_attack.png` → `battle/button_attack`

Templates may be referenced from `cv.json` or directly by recognition code. Missing referenced templates produce an error.

## cv.json schema

Coordinates are normalized to the screenshot in `[0, 1]`. Screens describe base pages, variants describe alternate views of that page, and elements describe interaction targets or status probes. Each server screen defines at least `variants.main`.

Example of a merged screen configuration:

```json
{
  "screens": {
    "TeamConfirm": {
      "detect": {
        "requiredTemplates": [
          {
            "template": "shared/screen_team_party",
            "region": { "x": 0.169, "y": 0.15, "w": 0.023, "h": 0.037 },
            "threshold": 0.75
          },
          {
            "template": "screen_team_confirm/button_mission_start",
            "region": { "x": 0.818, "y": 0.851, "w": 0.187, "h": 0.161 },
            "threshold": 0.8
          }
        ]
      },
      "variants": {
        "main": {
          "elements": {
            "mission_start": {
              "template": "screen_team_confirm/button_mission_start",
              "region": { "x": 0.818, "y": 0.851, "w": 0.187, "h": 0.161 },
              "threshold": 0.8
            }
          }
        }
      }
    }
  }
}
```

Detection supports a single `template`, alternative `templates` (OR), or `requiredTemplates` (AND, scored by the weakest match). Ordered `templateOptions` accept the first passing option. Matching screens are ranked by `priority`, then score; no match returns `Unknown`.

Use elements for states such as dialog visibility or menu readiness. Do not add a variant merely to represent a status flag.

## Adding a template

1. Crop a PNG from a screenshot of the target server into `servers/<jp|cn>/templates/`. Use `shared/` for server-neutral templates.
2. Reference the relative key in `cv.json` or the detector.
3. Account for frame resolution when scaling templates; source PNG pixels are not runtime coordinates.
4. Add an offline fixture and test, including a 1920×1080 downscaled case. Restart the development app to reload resources.

Compare template coverage, including nested directories:

```bash
pnpm templates:diff
pnpm templates:diff -- --check
```

State and probe changes also require updating the matching [automation document](../../docs/README.md#自动化与识别). Battle-counter recognition is documented in [battle-scene detection](../../docs/battle-scene-detection.md).

## runtime-manifest.json

The app bundles the manifest; it describes only external native runtimes:

| Field | Meaning |
| --- | --- |
| `platforms.<platform>.runtimeVersion` | Native runtime version for that platform |
| `runtimeUrl` / `runtimeSha256` | Runtime download and checksum |

The external ZIP root is `mash-cv-runtime/`, containing `mash-cv` on macOS or
`mash-cv.exe` on Windows, native dependencies and OCR models. The app verifies
SHA-256, extracts through staging, then writes `runtime-version.json`. Its path is
`app_data_dir()/runtime/mash-cv/runtime/<runtimeVersion>/mash-cv-runtime/`.

Python code and its lightweight assets are staged by `build.rs` into
`resources/mash-cv-code/mash_cv/` and bundled with the app. There is no separate
code version, download or installation; old downloaded code directories are not
loaded. Development still reads `sidecar/cv-code/` directly. The launcher receives
`MASH_CV_CODE_DIR` for this code and `MASH_CV_MODELS_DIR` for the external models.

Change runtime versions only for native dependencies, OCR models, the launcher or
native packaging inputs. The app's bundled scrcpy JAR and Python code follow the
app version. Validate bundled code against the exact pinned runtime on each platform.

## scrcpy-server.jar

The sidecar pushes this JAR to `/data/local/tmp/scrcpy-server.jar` and launches it through `app_process`. The stream protocol targets scrcpy **2.7**; update the JAR and sidecar together.

The [upstream v2.7 server](https://github.com/Genymobile/scrcpy/releases/download/v2.7/scrcpy-server-v2.7) must have SHA-256:

```text
a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba
```

Verify the checksum before replacing `scrcpy/scrcpy-server.jar`.
