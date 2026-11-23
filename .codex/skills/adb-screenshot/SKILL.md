---
name: adb-screenshot
description: Capture a PNG of the current screen on an Android device or emulator already connected to ADB. Use for live Android screenshots, including requests to capture, save, or inspect the current device screen.
---

# ADB Screenshot

1. Run `adb devices -l`. If `adb` is missing from PATH, check for `platform-tools/adb` under the configured Android SDK. Use the requested serial if it is in the `device` state. Otherwise, when no target was specified, use the sole ready device. If multiple devices are ready and the target is ambiguous, ask which serial to use. If the target is offline, unauthorized, or absent, report the observed state.
2. Use the selected serial with `adb -s` so the capture stays bound to that device. Set `serial` to the selected value before running the shell example. If you found ADB outside PATH, invoke its resolved path.

   ```sh
   shot_dir=$(mktemp -d "${TMPDIR:-/tmp}/adb-screenshot.XXXXXX") || exit 1
   shot="$shot_dir/screenshot.png"
   if adb -s "$serial" exec-out screencap -p > "$shot" && [ -s "$shot" ]; then
     printf '%s\n' "$shot"
   else
     rm -f "$shot"
     rmdir "$shot_dir"
     printf '%s\n' 'ADB screenshot failed or returned no data.' >&2
     exit 1
   fi
   ```

3. Verify that the file opens as a PNG with the image viewer or a local PNG decoder. If the capture fails or cannot be decoded, report the error instead of presenting the file as a screenshot. If the user specified a destination, save the verified PNG there. In Codex desktop, show the screenshot using Markdown image syntax with its absolute path, or give the path if the user only asked to save it.
