import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { planRelease, recordRelease, validateCandidate } from "../shared/windows-release.mjs";

function fixture(t) {
  const previous = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "windows release "));
  process.chdir(root);
  t.after(() => { process.chdir(previous); fs.rmSync(root, { recursive: true, force: true }); });
  const write = (file, value) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, value); };
  const git = (...args) => execFileSync("git", args, { encoding: "utf8", stdio: "pipe" }).trim();
  write("src-tauri/tauri.conf.json", JSON.stringify({ productName: "Mash", version: "1.2.3" }));
  write("src-tauri/Cargo.toml", '[package]\nname = "mash"\nversion = "1.2.3"\n');
  write("versions.toml", '[app]\nversion = "1.2.3"\n[mash_cv]\ncode = "0.4.0"\n[mash_cv.runtime]\nwindows-x86_64 = "0.1.0"\n');
  write("src-tauri/resources/runtime-manifest.json", JSON.stringify({ mashCvCodeVersion: "0.4.0", platforms: {
    "windows-x86_64": { codeUrl: "https://example.invalid/code.zip", codeSha256: "a".repeat(64), runtimeUrl: "https://example.invalid/runtime.zip", runtimeSha256: "b".repeat(64), runtimeVersion: "0.1.0" },
  } }));
  write("src-tauri/target/release/bundle/nsis/Mash_1.2.3_x64-setup.exe", "MZ candidate");
  write("src-tauri/target/release/bundle/nsis/Mash_1.2.3_x64-setup.exe.sig", "signature");
  git("init", "--quiet"); git("config", "user.name", "Release Test"); git("config", "user.email", "test@example.invalid"); git("config", "commit.gpgsign", "false"); git("config", "tag.gpgsign", "false"); git("add", "."); git("commit", "--quiet", "-m", "fixture"); git("tag", "v1.2.3");
  return { root, git };
}

test("Windows plan pins a tagged app source and its published CV manifest", t => {
  const f = fixture(t);
  const plan = planRelease("1.2.3", "HEAD", "windows-stable");
  assert.equal(plan.sourceSha, f.git("rev-parse", "HEAD"));
  assert.equal(plan.platform, "windows-x86_64");
  assert.throws(() => planRelease("1.2.4", "HEAD", "windows-stable"), /must point|unknown revision/);
});

test("Windows candidate recording locks installer and signature bytes", t => {
  const f = fixture(t);
  const plan = planRelease("1.2.3", "HEAD", "windows-stable");
  const receipt = recordRelease(plan, "src-tauri/target/release/bundle", "candidate");
  assert.equal(receipt.artifact, "Mash_1.2.3_x64-setup.exe");
  assert.doesNotThrow(() => validateCandidate("candidate/release.json", "candidate"));
  fs.appendFileSync("candidate/Mash_1.2.3_x64-setup.exe", "tampered");
  assert.throws(() => validateCandidate("candidate/release.json", "candidate"), /changed/);
});
