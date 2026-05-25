# minitouch

Native touch-injection server pushed to the device by `Runner` (see
`src-tauri/src/minitouch.rs`) to drive smooth swipes that don't trigger
Android's fling momentum. Used today only by the support-list scroll;
all other taps and swipes still go through `adb shell input …`.

## Why a separate binary?

`adb shell input motionevent` spawns a fresh JVM per event (~30–80 ms
each on real devices), so a smooth 60-frame swipe would take 2–5 s and
look choppy. minitouch writes raw evdev events to `/dev/input/event*`
in well under a millisecond per command, so a full-page swipe with
~30 MOVE events can land in ~1 s and look indistinguishable from a
real finger drag.

## Layout

```
src-tauri/resources/minitouch/
  README.md           ← this file
  arm64-v8a/
    minitouch         ← native binary for arm64 (most modern emulators & phones)
  # add more ABIs as needed:
  # armeabi-v7a/minitouch
  # x86/minitouch
  # x86_64/minitouch
```

`Minitouch::start` reads the device's `ro.product.cpu.abi` via
`adb shell getprop`, then pushes
`<resources>/minitouch/<abi>/minitouch` to
`/data/local/tmp/minitouch`. If the per-ABI binary is missing, the
runner emits a warning and falls back to `Adb::swipe_with_settle`, so
missing binaries degrade gracefully rather than breaking automation.

## Where to get the binary

Upstream:
[DeviceFarmer/minitouch](https://github.com/DeviceFarmer/minitouch). The
official repo no longer ships prebuilt binaries; you can either:

1. **Build from source** — clone the repo, follow its Android NDK
   instructions, copy the resulting binary out of `libs/<abi>/`.
2. **Take an existing build** — Appium, OpenSTF, Maestro, and similar
   projects ship vendored copies. Confirm the build is recent enough
   to support `^` banner pressure reporting (anything from 2017+ does).

Drop the binary at the path above. The repo intentionally does **not**
include any precompiled binary — pick one you trust and verify the
SHA-256 before committing it.

Then add the bundle glob to `src-tauri/tauri.conf.json` so production
builds ship it:

```jsonc
// inside "bundle.resources"
"resources/minitouch/*/minitouch",
```

It's left out by default because Tauri's build script errors when a
declared resource glob doesn't match any files, and we don't want
`pnpm tauri build` to fail for contributors who haven't installed a
minitouch binary locally.

After dropping a new binary, run:

```bash
shasum -a 256 src-tauri/resources/minitouch/arm64-v8a/minitouch
```

and record the digest below for future reproducibility.

## Pinned SHA-256

| ABI         | SHA-256                                                            |
| ----------- | ------------------------------------------------------------------ |
| arm64-v8a   | _(fill in after first commit)_                                      |

## Protocol overview

minitouch listens on the abstract Unix socket `@minitouch` and accepts
a simple text protocol. The runner only uses the subset below:

```
d <contact> <x> <y> <pressure>   press
m <contact> <x> <y> <pressure>   move
u <contact>                      lift
w <ms>                           server-side wait
c                                commit pending events
```

Coordinates are device pixels in the range advertised by the startup
banner (`^ <max_contacts> <max_x> <max_y> <max_pressure>`); the runner
scales normalized `[0.0, 1.0]` coordinates into that range.

The settle-swipe payload looks like:

```
d 0 x0 y0 50
c
w 16
m 0 x1 y1 50
c
...
w <settle_ms>
m 0 xN yN 50
c
u 0
c
```

The trailing `w` + redundant MOVE at the destination give Android's
velocity tracker a stretch of "no motion" right before UP, suppressing
the fling response that the plain `input swipe` path used to trigger.
