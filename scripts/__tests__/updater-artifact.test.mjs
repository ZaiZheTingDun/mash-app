import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { selectUpdaterArtifact } from "../updater-artifact.mjs";

const config = { productName: "Mash", version: "0.14.0" };
const helper = fileURLToPath(new URL("../updater-artifact.mjs", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../", import.meta.url));
const appConfig = JSON.parse(fs.readFileSync(path.join(repoRoot, "src-tauri/tauri.conf.json"), "utf8"));
const currentInstaller = `${appConfig.productName}_${appConfig.version}_x64-setup.exe`;

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash updater artifacts "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

function write(root, name, signed = true) {
  const file = path.join(root, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, "artifact");
  if (signed) fs.writeFileSync(`${file}.sig`, "signature");
  return file;
}

test("Windows selects the current version and architecture with old signatures retained", t => {
  const root = fixture(t);
  write(root, "nsis/Mash_0.13.0_x64-setup.exe");
  const arm = write(root, "nsis/Mash_0.14.0_arm64-setup.exe");
  const x64 = write(root, "nsis/Mash_0.14.0_x64-setup.exe");
  write(root, "msi/Mash_0.14.0_x64.msi");
  write(root, "macos/Mash.app.tar.gz");
  assert.equal(selectUpdaterArtifact(root, config, "windows-x86_64"), x64);
  assert.equal(selectUpdaterArtifact(root, config, "windows-aarch64"), arm);
});

test("macOS selects its native app archive despite Windows and previous product bundles", t => {
  const root = fixture(t);
  write(root, "nsis/Mash_0.13.0_x64-setup.exe");
  write(root, "macos/OldMash.app.tar.gz");
  const app = write(root, "macos/Mash.app.tar.gz");
  for (const target of ["darwin-aarch64", "darwin-x86_64"]) {
    assert.equal(selectUpdaterArtifact(root, config, target), app);
  }
});

test("missing current artifact or signature never falls back to an older release", t => {
  const root = fixture(t);
  write(root, "nsis/Mash_0.13.0_x64-setup.exe");
  assert.throws(() => selectUpdaterArtifact(root, config, "windows-x86_64"), /0\.14\.0_x64-setup\.exe/);
  write(root, "nsis/Mash_0.14.0_x64-setup.exe", false);
  assert.throws(() => selectUpdaterArtifact(root, config, "windows-x86_64"), /setup\.exe\.sig/);
  assert.throws(() => selectUpdaterArtifact(root, config, "linux-x86_64"), /Unsupported/);
});

test("the publisher CLI preserves paths with spaces and emits shell-compatible separators", t => {
  const root = fixture(t);
  const expected = write(root, "nsis/Mash_0.14.0_x64-setup.exe");
  const configPath = path.join(root, "tauri.conf.json");
  fs.writeFileSync(configPath, JSON.stringify(config));
  const output = execFileSync(process.execPath, [helper, root, configPath, "windows-x86_64"], { encoding: "utf8" });
  assert.equal(output, expected.split(path.sep).join("/"));
});

test("Bash publisher uses the current artifact and reads its signature with spaces in the path", t => {
  const root = fixture(t);
  write(root, "nsis/Mash_0.13.0_x64-setup.exe");
  write(root, `nsis/${currentInstaller}`);
  const source = fs.readFileSync(new URL("../release-tauri-updater.sh", import.meta.url), "utf8").replaceAll("\r\n", "\n");
  const selection = source.slice(source.indexOf('ARTIFACT_FILE="$(node'), source.indexOf('\nPUB_DATE='));
  const bash = process.platform === "win32" ? path.join(process.env.ProgramFiles, "Git/bin/bash.exe") : "bash";
  const script = 'set -euo pipefail\nfail() { exit 1; }\nBUNDLE_DIR="$1"\n' +
    'TAURI_TARGET=windows-x86_64\nVERSION_PREFIX=test\nRELEASE_BASE_URL=https://example.invalid\n' +
    `${selection}\nprintf '%s\\n' "$ARTIFACT_NAME" "$SIGNATURE"\n`;
  const output = execFileSync(bash, ["-c", script, "test", root.split(path.sep).join("/")],
    { cwd: repoRoot, encoding: "utf8", windowsHide: true });
  assert.equal(output.trim(), `${currentInstaller}\nsignature`);
});

test("PowerShell publisher selects the current artifact despite retained old signatures", {
  skip: process.platform !== "win32",
}, t => {
  const root = fixture(t);
  write(root, "nsis/Mash_0.13.0_x64-setup.exe");
  write(root, `nsis/${currentInstaller}`);
  const source = fs.readFileSync(new URL("../release-tauri-updater.ps1", import.meta.url), "utf8");
  const selection = source.slice(source.indexOf("    $artifact = node"), source.indexOf("    $prefix ="));
  const script = `$ErrorActionPreference = 'Stop'; $bundle = '${root.replaceAll("'", "''")}'; ` +
    `$target = 'windows-x86_64';\n${selection}\nSplit-Path -Leaf $artifact\n$signature`;
  const output = execFileSync("powershell.exe", ["-NoProfile", "-Command", script],
    { cwd: repoRoot, encoding: "utf8", windowsHide: true });
  assert.equal(output.replaceAll("\r\n", "\n").trim(), `${currentInstaller}\nsignature`);
});
