import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync, execFileSync } from "node:child_process";
import test from "node:test";

const source = fs.readFileSync(new URL("../release.sh", import.meta.url), "utf8").replaceAll("\r\n", "\n");
const changeFunction = source.match(/^has_changes_since\(\) \{\n[\s\S]*?^\}/m)[0];
const runtimeDetection = source.slice(source.indexOf("CV_RUNTIME_CHANGED=false\n"), source.indexOf('\nif [[ "$CV_CODE_CHANGED"'));
const runtimeGate = source.match(/^if \[\[ "\$CV_RUNTIME_CHANGED"[\s\S]*?^fi/m)[0];
const bash = process.platform === "win32" ? path.join(process.env.ProgramFiles, "Git/bin/bash.exe") : "bash";

for (const [changed, expected] of [
  ["sidecar/mash_cv/build_sidecar.py", true],
  ["sidecar/mash_cv/build_sidecar.sh", true],
  ["sidecar/mash_cv/mash_cv/cv.py", false],
]) {
  test(`runtime release gate detects ${changed}: ${expected}`, t => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash-release-detection-"));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const git = args => execFileSync("git", args, { cwd: root, encoding: "utf8", windowsHide: true });
    git(["init", "--quiet"]);
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
      `RUNTIME_BASE_REF=${ref}\nPUBLISH=true\nCV_RUNTIME_VERSION=\n${runtimeDetection}\n${runtimeGate}\n` +
      'echo "$CV_RUNTIME_CHANGED"\n';
    const result = spawnSync(bash, ["-c", script], { cwd: root, encoding: "utf8", windowsHide: true });
    assert.ifError(result.error);
    assert.equal(result.status, expected ? 1 : 0, result.stderr);
    if (expected) assert.match(result.stderr, /CV runtime changes detected.*--cv-runtime/);
    else assert.equal(result.stdout.trim(), "false");
  });
}

test("release skill includes the Python builder in its runtime detection example", () => {
  const skill = fs.readFileSync(new URL("../../.codex/skills/mash-release-app/SKILL.md", import.meta.url), "utf8");
  const example = skill.split(/\r?\n/).find(line => line.startsWith("git diff --name-only <last-cv-runtime-tag>"));
  assert.ok(example?.split(" ").includes("sidecar/mash_cv/build_sidecar.py"));
});
