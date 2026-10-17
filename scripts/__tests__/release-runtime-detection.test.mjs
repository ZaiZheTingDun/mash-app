import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync, execFileSync } from "node:child_process";
import test from "node:test";

const source = fs.readFileSync(new URL("../macos/release.sh", import.meta.url), "utf8").replaceAll("\r\n", "\n");
const changeFunction = source.match(/^has_changes_since\(\) \{\n[\s\S]*?^\}/m)[0];
const runtimeDetection = source.match(/^CV_RUNTIME_CHANGED=.*$/m)[0];
const runtimeGate = source.match(/^if \[\[ "\$CV_RUNTIME_CHANGED"[\s\S]*?^fi/m)[0];
const bash = process.platform === "win32" ? path.join(process.env.ProgramFiles, "Git/bin/bash.exe") : "bash";

for (const [changed, expected] of [
  ["sidecar/cv-code/build_code.py", false],
  ["sidecar/cv-runtime/build_runtime.py", true],
  ["scripts/macos/build-cv.sh", true],
  ["scripts/windows/build-cv.ps1", false],
  ["src-tauri/resources/scrcpy/scrcpy-server.jar", false],
  ["sidecar/cv-runtime/runtime_launcher.py", true],
  ["sidecar/cv-runtime/models/model.onnx", true],
  ["sidecar/cv-code/mash_cv/cv.py", false],
  ["sidecar/cv-code/mash_cv/assets/classifier.onnx", false],
  ["sidecar/cv-code/tests/test_cv.py", false],
  ["sidecar/cv-runtime/tests/test_build_runtime.py", false],
  ["sidecar/cv-runtime/README.md", false],
  ["sidecar/cv-runtime/new-native-input.dat", true],
]) {
  test(`runtime release gate detects ${changed}: ${expected}`, t => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash-release-detection-"));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const git = args => execFileSync("git", args, { cwd: root, encoding: "utf8", windowsHide: true });
    git(["init", "--quiet"]);
    fs.mkdirSync(path.join(root, "scripts/shared"), { recursive: true });
    fs.copyFileSync(new URL("../shared/runtime-inputs.mjs", import.meta.url), path.join(root, "scripts/shared/runtime-inputs.mjs"));
    const file = path.join(root, changed);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "baseline");
    git(["add", "."]);
    const commit = message => git(["-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid",
      "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", message]);
    commit("baseline");
    const ref = git(["rev-parse", "HEAD"]).trim();
    fs.writeFileSync(file, "changed");
    git(["add", "."]);
    commit("change");
    // Execute the actual detection and publication gate with a real Git diff.
    const script = `set -euo pipefail\nfail() { echo "$*" >&2; exit 1; }\n${changeFunction}\n` +
      `RUNTIME_BASE_REF=${ref}\nPLATFORM=darwin-aarch64\nPUBLISH=true\nCV_RUNTIME_VERSION=\n${runtimeDetection}\n${runtimeGate}\n` +
      'echo "$CV_RUNTIME_CHANGED"\n';
    const result = spawnSync(bash, ["-c", script], { cwd: root, encoding: "utf8", windowsHide: true });
    assert.ifError(result.error);
    assert.equal(result.status, expected ? 1 : 0, result.stderr);
    if (expected) assert.match(result.stderr, /CV runtime changes detected.*--cv-runtime/);
    else assert.equal(result.stdout.trim(), "false");
  });
}
