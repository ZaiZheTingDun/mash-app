import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { pwshArgs, psLiteral as ps } from "./helpers/pwsh.mjs";

const repo = fileURLToPath(new URL("../../", import.meta.url));
const helper = path.join(repo, "scripts/shared/release-state.mjs");
const windows = process.platform === "win32";
const invoke = (command, args, cwd) => execFileSync(command, args, {
  cwd, encoding: "utf8", windowsHide: true, stdio: "pipe",
});
test("configured runtime versions match each platform in the bundled manifest", () => {
  const versions = JSON.parse(invoke(process.execPath, [helper, "versions"], repo));
  const manifest = JSON.parse(fs.readFileSync(path.join(repo, "src-tauri/resources/runtime-manifest.json"), "utf8"));
  assert.deepEqual(Object.keys(manifest.platforms).sort(), Object.keys(versions.runtimes).sort());
  for (const [platform, version] of Object.entries(versions.runtimes)) {
    assert.equal(manifest.platforms[platform].runtimeVersion, version, platform);
  }
});
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash release 中文 "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (name, text) => {
    const file = path.join(root, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
  };
  write("versions.toml", '[app]\nversion = "1.0.0"\n[mash_cv.runtime]\ndarwin-aarch64 = "2.0.0"\nwindows-x86_64 = "1.0.0"\n');
  write("src-tauri/tauri.conf.json", JSON.stringify({ version: "1.0.0", productName: "Mash" }));
  write("src-tauri/Cargo.toml", '[package]\nname = "mash"\nversion = "1.0.0"\n');
  write("src-tauri/Cargo.lock", '[[package]]\nname = "other"\nversion = "1.0.0"\n[[package]]\nname = "mash"\nversion = "1.0.0"\n');
  write("src-tauri/resources/runtime-manifest.json", JSON.stringify({ platforms: {
    "darwin-aarch64": { runtimeVersion: "2.0.0", runtimeUrl: "https://example.invalid/runtime.zip", runtimeSha256: "a".repeat(64) },
  } }));
  write("CHANGELOG.md", "# Changes\n\n## 未发布\n\nRelease notes\n");
  write("sidecar/cv-runtime/build_runtime.py", "# fixture builder\n");
  write(".gitignore", "sidecar/cv-code/dist/\nsidecar/cv-runtime/dist/\nsrc-tauri/target/\n*.log\n");
  // Node 22's native recursive copier converts Unicode Windows paths through
  // std::filesystem. A filter keeps directory traversal in the JS implementation.
  const copyOptions = { recursive: true, filter: () => true };
  fs.cpSync(path.join(repo, "scripts/windows"), path.join(root, "scripts/windows"), copyOptions);
  fs.cpSync(path.join(repo, "scripts/shared"), path.join(root, "scripts/shared"), copyOptions);
  const git = (...args) => invoke("git", args, root).trim();
  git("init", "--quiet");
  git("config", "core.autocrlf", "false");
  git("config", "user.name", "Release Test");
  git("config", "user.email", "test@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("config", "tag.gpgsign", "false");
  const commit = () => { git("add", "."); git("commit", "--quiet", "-m", "fixture"); };
  commit();
  git("tag", "v1.0.0");
  const run = (...args) => invoke(process.execPath, [helper, ...args], root);
  return { root, write, git, commit, run, read: name => fs.readFileSync(path.join(root, name), "utf8") };
}

test("release fixture copies scripts into the Unicode directory", t => {
  const f = fixture(t);
  for (const name of ["scripts/windows/release.ps1", "scripts/shared/release-state.mjs"]) {
    assert.equal(f.read(name), fs.readFileSync(path.join(repo, name), "utf8"));
  }
});

test("shared release CLIs execute through a directory alias", t => {
  const f = fixture(t);
  const alias = path.join(f.root, "shared alias");
  fs.symlinkSync(path.join(f.root, "scripts/shared"), alias, windows ? "junction" : "dir");
  const run = (name, ...args) => invoke(process.execPath, [path.join(alias, name), ...args], f.root);
  assert.equal(JSON.parse(run("release-state.mjs", "versions")).app, "1.0.0");
  run("release-state.mjs", "version", "app", "1.1.0");
  assert.equal(JSON.parse(f.read("src-tauri/tauri.conf.json")).version, "1.1.0");

  const installer = "src-tauri/target/release/bundle/nsis/Mash_1.1.0_x64-setup.exe";
  f.write(installer, "fixture installer");
  f.write(`${installer}.sig`, "fixture signature");
  const artifact = run("updater-artifact.mjs", "src-tauri/target/release/bundle", "src-tauri/tauri.conf.json", "windows-x86_64");
  assert.equal(fs.realpathSync.native(artifact), fs.realpathSync.native(path.join(f.root, installer)));

  run("updater-manifest.mjs", "latest.json", "1.1.0", "2026-10-14T00:00:00Z", "windows-x86_64",
    "https://example.invalid/installer.exe", "fixture signature");
  assert.equal(JSON.parse(f.read("latest.json")).platforms["windows-x86_64"].signature, "fixture signature");

  // Preserve the alias in import.meta.url to exercise differing entry/module paths
  // on every host, including the native path normalization needed on Windows.
  const runtime = (...refs) => invoke(process.execPath, ["--preserve-symlinks-main",
    path.join(alias, "runtime-inputs.mjs"), ...refs, "windows-x86_64"], f.root).trim();
  assert.equal(runtime("HEAD", "HEAD"), "false");
  f.write("sidecar/cv-runtime/build_runtime.py", "changed runtime builder");
  f.commit();
  assert.equal(runtime("HEAD^", "HEAD"), "true");
});

test("app bump aligns four files and leaves unrelated lock packages untouched", t => {
  const f = fixture(t);
  f.run("version", "app", "1.1.0");
  assert.equal(JSON.parse(f.run("versions")).app, "1.1.0");
  assert.equal(JSON.parse(f.read("src-tauri/tauri.conf.json")).version, "1.1.0");
  assert.match(f.read("src-tauri/Cargo.toml"), /version = "1.1.0"/);
  assert.match(f.read("src-tauri/Cargo.lock"), /name = "other"\nversion = "1.0.0"/);
  assert.match(f.read("src-tauri/Cargo.lock"), /name = "mash"\nversion = "1.1.0"/);
});
test("invalid bump or missing lock entry fails before changing version files", t => {
  const f = fixture(t);
  const original = f.read("versions.toml");
  assert.throws(() => f.run("version", "app", "../bad"));
  f.write("src-tauri/Cargo.lock", "invalid");
  assert.throws(() => f.run("version", "app", "1.1.0"), /Missing mash package/);
  assert.equal(f.read("versions.toml"), original);
});
test("runtime artifacts update only their target platform", t => {
  const f = fixture(t);
  const mac = JSON.parse(f.read("src-tauri/resources/runtime-manifest.json")).platforms["darwin-aarch64"];
  f.run("artifact", "runtime", "1.1.0", "windows-x86_64", "https://example.invalid/win.zip", "c".repeat(64));
  const doc = JSON.parse(f.read("src-tauri/resources/runtime-manifest.json"));
  assert.deepEqual(doc.platforms["darwin-aarch64"], mac);
  assert.equal(doc.platforms["windows-x86_64"].runtimeVersion, "1.1.0");
  assert.throws(() => f.run("artifact", "code", "1.0.0", "windows-x86_64", "https://example.invalid/code.zip", "d".repeat(64)), /Only external runtime/);
});
test("runtime bumps require a platform and preserve other platform versions", t => {
  const f = fixture(t);
  const original = f.read("versions.toml");
  assert.throws(() => f.run("version", "runtime", "1.1.0"), /Invalid platform/);
  assert.equal(f.read("versions.toml"), original);
  f.run("version", "runtime", "1.1.0", "windows-x86_64");
  assert.equal(JSON.parse(f.run("versions", "windows-x86_64")).runtime, "1.1.0");
  assert.equal(JSON.parse(f.run("versions", "darwin-aarch64")).runtime, "2.0.0");
  assert.equal(JSON.parse(f.run("versions")).code, undefined);
  f.run("version", "runtime", "1.0.0", "windows-aarch64");
  assert.equal(JSON.parse(f.run("versions", "windows-aarch64")).runtime, "1.0.0");
});
test("runtime release planning compares against the target platform version", t => {
  const f = fixture(t);
  assert.equal(JSON.parse(f.run("plan", "1.1.0", "1.1.0", "windows-x86_64", "plan")).current.runtime, "1.0.0");
  assert.throws(() => f.run("plan", "1.1.0", "1.0.0", "windows-x86_64", "plan"), /must increase/);
});
test("publishing another platform does not reset the target runtime change baseline", t => {
  const f = fixture(t);
  f.git("tag", "cv-runtime/windows-x86_64/1.0.0");
  f.write("sidecar/cv-runtime/build_runtime.py", "changed shared runtime builder");
  f.commit();
  f.run("version", "runtime", "2.1.0", "darwin-aarch64");
  f.commit();
  f.git("tag", "cv-runtime/darwin-aarch64/2.1.0");
  const plan = JSON.parse(f.run("plan", "1.1.0", "-", "windows-x86_64", "plan"));
  assert.equal(plan.runtimeChanged, true);
  assert.throws(() => f.run("plan", "1.1.0", "-", "windows-x86_64", "publish"), /CV runtime changes detected/);
});
test("Windows runtime detection ignores build CLI changes", t => {
  const f = fixture(t);
  f.write("sidecar/cv-code/build_code.py", "changed build CLI");
  f.commit();
  const plan = JSON.parse(f.run("plan", "1.1.0", "-", "windows-x86_64", "publish"));
  assert.equal(plan.runtimeChanged, false);
});
test("release planning is read-only, detects changed builder and enforces the runtime gate", t => {
  const f = fixture(t);
  f.write("sidecar/cv-runtime/build_runtime.py", "changed builder");
  f.commit();
  const before = f.git("rev-parse", "HEAD");
  const plan = JSON.parse(f.run("plan", "1.1.0", "-", "windows-x86_64", "plan"));
  assert.equal(plan.runtimeChanged, true);
  assert.throws(() => f.run("plan", "1.1.0", "-", "windows-x86_64", "publish"), /CV runtime changes detected/);
  assert.equal(f.git("status", "--porcelain"), "");
  assert.equal(f.git("rev-parse", "HEAD"), before);
});
test("runtime change detection uses the configured version tag rather than a legacy date version", t => {
  const f = fixture(t);
  f.git("tag", "cv-runtime/windows-x86_64/2026.05.08-runtime1");
  f.write("sidecar/cv-runtime/build_runtime.py", "new runtime builder");
  f.run("version", "runtime", "1.1.0", "windows-x86_64");
  f.commit();
  f.git("tag", "cv-runtime/windows-x86_64/1.1.0");
  assert.equal(JSON.parse(f.run("plan", "1.1.0", "-", "windows-x86_64", "plan")).runtimeChanged, false);
});
test("adding a second platform reuses the exact app tag and refuses CV mutations", t => {
  const f = fixture(t);
  assert.equal(JSON.parse(f.run("plan", "1.0.0", "-", "windows-x86_64", "publish")).sameVersion, true);
  assert.throws(() => f.run("plan", "1.0.0", "1.1.0", "windows-x86_64", "publish"), /requires a new app version/);
  assert.throws(() => f.run("plan", "0.9.0", "-", "windows-x86_64", "plan"), /backwards/);
});
test("changelog finalization handles CRLF and preserves the release notes", t => {
  const f = fixture(t);
  f.write("CHANGELOG.md", "# Notes\r\n## 未发布\r\n\r\n中文内容\r\n");
  f.run("changelog", "1.1.0", "2026-10-12");
  assert.match(f.read("CHANGELOG.md"), /## v1.1.0 - 2026-10-12\n\r\n中文内容/);
});
test("PowerShell entry point prints its plan without creating files or tags", { skip: !windows }, t => {
  const f = fixture(t);
  const script = `& ${ps(path.join(f.root, "scripts/windows/release.ps1"))} 1.1.0 -Installer`;
  const output = invoke("pwsh", pwshArgs(script, f.root), repo);
  assert.match(output, /No files, tags, artifacts or remote refs changed/);
  assert.match(output, /scripts\/windows\/release-installer.ps1/);
  assert.equal(f.git("status", "--porcelain"), "");
  assert.equal(f.git("tag", "--list", "v1.1.0"), "");
});
test("PowerShell native failures propagate instead of continuing release steps", { skip: !windows }, () => {
  const script = `. ${ps(path.join(repo, "scripts/windows/common.ps1"))}; Invoke-Native pwsh @('-NoProfile', '-Command', 'exit 9'); 'UNEXPECTED'`;
  const result = spawnSync("pwsh", pwshArgs(script, repo), { cwd: repo, encoding: "utf8", windowsHide: true });
  assert.equal(result.status, 1);
  assert.doesNotMatch(result.stdout, /UNEXPECTED/);
  assert.match(result.stderr, /failed \(exit 9\)/);
});

test("Windows full release runs offline through CV, updater and installer publication", { skip: !windows }, t => {
  const f = fixture(t);
  // Replace only network ADB preparation; execute the actual release scripts.
  f.write("scripts/windows/prepare-windows-adb.ps1", "Write-Host 'Fixture ADB ready'\n");
  f.commit();
  const bootstrap = path.join(repo, "scripts/__tests__/fixtures/offline-release.ps1");
  const output = invoke("pwsh", pwshArgs(`& ${ps(bootstrap)}`, f.root), repo);
  assert.match(output, /Release complete: v1.1.0/);
  assert.equal(f.git("status", "--porcelain"), "");
  for (const tag of ["v1.1.0", "cv-runtime/windows-x86_64/1.1.0"]) {
    assert.equal(f.git("tag", "--list", tag), tag);
  }
  const manifest = JSON.parse(f.read("src-tauri/resources/runtime-manifest.json"));
  assert.equal(manifest.platforms["windows-x86_64"].runtimeVersion, "1.1.0");
  assert.equal(manifest.platforms["darwin-aarch64"].runtimeVersion, "2.0.0");
  assert.ok(manifest.platforms["darwin-aarch64"]);
  const uploads = f.read("uploads.log");
  const order = ["runtime/mash-cv/runtime/windows-x86_64/",
    "releases/versions/v1.1.0/windows-x86_64/", "releases/versions/v1.1.0/latest.json",
    "releases/windows-stable/latest.json", "downloads/versions/", "downloads/latest/"];
  let offset = -1;
  for (const key of order) {
    const next = uploads.indexOf(key, offset + 1);
    assert.ok(next > offset, "Missing or out-of-order upload: " + key);
    offset = next;
  }
  const work = "src-tauri/target/release-updater/v1.1.0/windows-x86_64/";
  const id = fs.readdirSync(path.join(f.root, work))[0];
  const updater = JSON.parse(f.read(work + id + "/latest.json"));
  assert.ok(updater.platforms["darwin-aarch64"]);
  assert.equal(updater.platforms["windows-x86_64"].signature, "fixture signature");
});

test("relocated template diff helper resolves repository resources from another directory", t => {
  const f = fixture(t);
  const report = JSON.parse(invoke(process.execPath, [path.join(repo, "scripts/shared/diff-server-templates.mjs"), "--json"], f.root));
  assert.equal(path.resolve(report.root), path.join(repo, "src-tauri/resources/servers"));
});
