---
name: np-readiness-detection
overview: "Add a sidecar primitive that reports, for each of the 3 fixed Noble Phantasm card slots, whether an NP card is currently drawn (ready). Mirrors the command-card primitive: Python detector + Rust IPC + Tauri command + debug overlay."
todos:
  - id: py_detector
    content: Add DEFAULT_NP_CARD_SLOTS, _find_noble_phantasms, and REPL dispatch in cv.py
    status: completed
  - id: py_init
    content: Export new constants/helpers from mash_cv/__init__.py
    status: completed
  - id: py_tests
    content: Copy noble_debug.png to test_data and add TestFindNoblePhantasms cases
    status: completed
  - id: py_smoke
    content: Run pytest and verify all tests pass
    status: completed
  - id: rust_client
    content: Add NoblePhantasmMatch struct and SidecarClient::find_noble_phantasms in screen.rs
    status: completed
  - id: rust_debug
    content: Add debug_find_noble_phantasms command in debug.rs and register it in lib.rs
    status: completed
  - id: frontend
    content: Add NP detect button + overlays + side panel in DebugPage.tsx (+ App.css classes)
    status: completed
  - id: rebuild_sidecar
    content: Rebuild the mash-cv sidecar bundle so Tauri picks up the new REPL command
    status: completed
isProject: false
---

## Detection signal

For each of the 3 fixed slots, compute Canny edge density inside the slot. An NP card has a dense geometric X-frame + face circle + text glyphs, so it lights up at ~11–12% edge fraction. An empty slot shows the battle background, which is much smoother (~5%). Threshold at `0.08` (well between the two clusters).

Also include `stdBgr` in the response as a corroborating signal so you can re-tune from the debug UI without code changes if a future scene is borderline.

Validated on [sidecar/mash_cv/noble_debug.png](sidecar/mash_cv/noble_debug.png):
- NP1 (Altria, ready): edge=12.0%, std=91 -> ready
- NP2 (Oberon, ready): edge=11.5%, std=96 -> ready
- NP3 (Altria Lv.100 at 63%, empty): edge=5.2%, std=43 -> not ready

## Files to change

### Python sidecar

[sidecar/mash_cv/mash_cv/cv.py](sidecar/mash_cv/mash_cv/cv.py)
- Add fixed slot constants and threshold near the existing command-card section:
  ```python
  DEFAULT_NP_CARD_SLOTS: tuple[dict, ...] = (
      {"x": 0.241, "y": 0.097, "w": 0.187, "h": 0.396},
      {"x": 0.410, "y": 0.097, "w": 0.187, "h": 0.396},
      {"x": 0.603, "y": 0.097, "w": 0.187, "h": 0.396},
  )
  NP_READY_EDGE_THRESHOLD = 0.08
  NP_CANNY_LOW = 80
  NP_CANNY_HIGH = 160
  ```
- Add `_find_noble_phantasms(img, np_regions, edge_threshold)` modeled on `_find_command_cards` (around line 584). For each slot:
  1. `_slot_to_pixels(region, w, h)` -> `(sx, sy, sw, sh)`
  2. Convert ROI to gray, run `cv2.Canny(gray, NP_CANNY_LOW, NP_CANNY_HIGH)`, compute `edge_frac = (edges > 0).mean()`
  3. Compute `std_bgr = float(roi.std())` for debug
  4. `ready = edge_frac >= edge_threshold`
  5. Emit a record with `slot`, `cardRegion`, `ready`, `edgeFrac`, `stdBgr`
- Wire REPL dispatch alongside the existing `find_command_cards` branch (around line 922):
  ```python
  elif action == "find_noble_phantasms":
      img, err = _load_frame(cmd)
      if img is None:
          _reply(req_id, {"slots": [], "error": err})
      else:
          regions = cmd.get("npRegions") or list(DEFAULT_NP_CARD_SLOTS)
          _reply(req_id, _find_noble_phantasms(
              img, regions,
              float(cmd.get("edgeThreshold", NP_READY_EDGE_THRESHOLD)),
          ))
  ```

[sidecar/mash_cv/mash_cv/__init__.py](sidecar/mash_cv/mash_cv/__init__.py)
- Export `DEFAULT_NP_CARD_SLOTS`, `NP_READY_EDGE_THRESHOLD`, `_find_noble_phantasms`.

[sidecar/mash_cv/tests/test_cv.py](sidecar/mash_cv/tests/test_cv.py)
- Copy `sidecar/mash_cv/noble_debug.png` -> `sidecar/mash_cv/tests/test_data/screenshots/noble_debug.png` (treat as fixture).
- Add `TestFindNoblePhantasms`:
  - `test_returns_three_slots_with_correct_ready_flags`: load fixture, call `_find_noble_phantasms` with defaults, assert 3 records, assert `ready == [True, True, False]`, assert `edgeFrac` for ready slots > 0.10 and empty slot < 0.07.
  - `test_threshold_override_marks_all_empty`: pass `edge_threshold=0.99`, assert all `ready == False`.
  - `test_custom_regions_passthrough`: pass a single tiny slot region, assert exactly 1 record returned and `cardRegion` matches.

### Rust IPC

[src-tauri/src/screen.rs](src-tauri/src/screen.rs)
- Add struct (mirroring `CommandCardMatch` at line 61):
  ```rust
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct NoblePhantasmMatch {
      pub slot: u32,
      pub card_region: NormRect,
      pub ready: bool,
      pub edge_frac: f64,
      pub std_bgr: f64,
  }
  ```
- Add `SidecarClient::find_noble_phantasms(image_path: Option<&Path>, np_regions: Option<&[NormRect]>) -> Result<Vec<NoblePhantasmMatch>, String>` next to `find_command_cards` (line 472). It serializes `cmd: "find_noble_phantasms"` and `npRegions` (when provided), parses `slots` from the response.

[src-tauri/src/debug.rs](src-tauri/src/debug.rs)
- Add `debug_find_noble_phantasms(app, image_path: Option<String>) -> Result<Vec<NoblePhantasmMatch>, String>` next to `debug_find_command_cards` (line 341). Takes the image path the same way; passes `None` for regions so the sidecar uses defaults.

[src-tauri/src/lib.rs](src-tauri/src/lib.rs)
- Register `debug::debug_find_noble_phantasms` in the invoke handler list (line 466 area).

### Frontend debug UI

[src/components/DebugPage.tsx](src/components/DebugPage.tsx)
- Add `NoblePhantasmMatchDto` interface mirroring the Rust struct.
- Add a "查找宝具卡" button alongside the existing command-card debug button. On click:
  - `invoke<NoblePhantasmMatchDto[]>("debug_find_noble_phantasms", { imagePath })`
  - Stash result in component state.
- Render overlays: for each slot draw `cardRegion` with a green border if `ready`, dimmed/red border if not. Show label `NP{slot} ready=Y/N edge=12.0% std=91.4`.
- Add a small side-panel section listing each slot.

[src/App.css](src/App.css)
- Add two small classes:
  ```css
  .debug-overlay-np-ready { border: 2px solid var(--green-9); background: rgba(0,200,80,0.10); }
  .debug-overlay-np-empty { border: 2px dashed var(--gray-9); background: rgba(120,120,120,0.10); }
  ```

## After implementation

- `cd sidecar/mash_cv && poetry run pytest` to confirm all tests pass.
- `cd sidecar/mash_cv && bash build_sidecar.sh` so `pnpm tauri dev` picks up the new REPL command (the bundled `mash-cv` is what Tauri actually runs, not the source).
- Open the debug page, point at `noble_debug.png`, click "查找宝具卡", expect green overlays on slots 1+2 and a dim overlay on slot 3.