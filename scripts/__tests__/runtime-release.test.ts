import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test, { type TestContext } from "node:test";
import { digest, platforms as releasePlatforms } from "../shared/release-artifacts.mjs";
import {
  assembleRuntime, createRuntimePlan, planDigest, runtimeArtifact, syncRuntime,
  tagRuntime, uploadRuntime, validateRuntimePlan,
  type RuntimePlatform, type RuntimePlanOptions, type R2Client,
} from "../shared/runtime-release.ts";

const platforms = releasePlatforms as RuntimePlatform[];

function fixture(t: TestContext) {
  const previous = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash runtime release "));
  process.chdir(root);
  t.after(() => { process.chdir(previous); fs.rmSync(root, { recursive: true, force: true }); });
  const git = (...args: string[]) => execFileSync("git", args, { encoding: "utf8", stdio: "pipe", windowsHide: true }).trim();
  git("init", "--quiet");
  git("config", "user.name", "Runtime Test");
  git("config", "user.email", "test@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("config", "tag.gpgsign", "false");
  fs.mkdirSync("src-tauri/resources", { recursive: true });
  const manifest = { platforms: Object.fromEntries([...platforms, "darwin-x86_64"].map(platform => [platform, {
    runtimeVersion: "1.0.0", runtimeUrl: `https://example.invalid/old/${platform}.zip`, runtimeSha256: "a".repeat(64),
  }])) };
  fs.writeFileSync("src-tauri/resources/runtime-manifest.json", JSON.stringify(manifest));
  fs.writeFileSync("versions.toml", '[app]\nversion = "1.2.3"\n[mash_cv.runtime]\nwindows-x86_64 = "1.0.0"\ndarwin-aarch64 = "1.0.0"\ndarwin-x86_64 = "1.0.0"\n');
  git("add", ".");
  git("commit", "--quiet", "-m", "fixture");
  git("update-ref", "refs/remotes/origin/main", "HEAD");
  const source = git("rev-parse", "HEAD");
  const options: RuntimePlanOptions = { ref: "HEAD", version: "1.1.0", selection: "all", branch: "main", toolingSha: source, baseUrl: "https://example.invalid" };
  const plan = createRuntimePlan(options);
  const directory = path.join(root, "candidate");
  const build = (selected = plan) => {
    for (const platform of selected.platforms) {
      fs.mkdirSync(path.join(directory, platform), { recursive: true });
      const artifact = path.join(directory, platform, runtimeArtifact(selected, platform).name);
      fs.writeFileSync(artifact, `runtime ${platform}`);
      fs.writeFileSync(path.join(directory, platform, "report.json"), JSON.stringify({
        status: "passed", platform, version: selected.version, sourceSha: source,
        planSha256: planDigest(selected), runtimeSha256: digest(artifact),
      }));
    }
    return assembleRuntime(selected, directory);
  };
  return { root, git, source, options, plan, directory, build, manifest };
}

test("runtime plan selects native platforms and permits a single-platform release", t => {
  const f = fixture(t);
  assert.deepEqual(f.plan.platforms, platforms);
  assert.equal(validateRuntimePlan(f.plan), f.plan);
  assert.deepEqual(createRuntimePlan({ ...f.options, selection: "windows-x86_64" }).platforms, ["windows-x86_64"]);
  for (const selection of ["linux-x86_64", "", "../all"]) assert.throws(() => createRuntimePlan({ ...f.options, selection }));
  for (const version of ["1.0.0", "0.9.0", "1.1.0\nmalicious", "../../bad"]) assert.throws(() => createRuntimePlan({ ...f.options, version }));
  assert.throws(() => createRuntimePlan({ ...f.options, baseUrl: "file:///tmp" }));
  assert.throws(() => createRuntimePlan({ ...f.options, prefix: "mash/../bad" }));
});

test("the TypeScript CLI runs directly in Node and emits the pinned release plan", t => {
  const f = fixture(t);
  const output = path.join(f.root, "plan.json");
  const githubOutput = path.join(f.root, "github-output");
  const cli = fileURLToPath(new URL("../shared/runtime-release.ts", import.meta.url));
  const result = spawnSync(process.execPath, ["--experimental-strip-types", cli, "plan", output], {
    encoding: "utf8",
    env: { ...process.env, SOURCE_REF: "HEAD", RUNTIME_VERSION: f.options.version,
      PLATFORMS: "all", DEFAULT_BRANCH: "main", GITHUB_SHA: f.source,
      RELEASE_BASE_URL: f.options.baseUrl, R2_PREFIX: "mash", GITHUB_OUTPUT: githubOutput,
      GITHUB_STEP_SUMMARY: path.join(f.root, "summary.md") },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(fs.readFileSync(output, "utf8")), f.plan);
  assert.match(fs.readFileSync(githubOutput, "utf8"), new RegExp(`source_sha=${f.source}`));
});

test("planning rejects unmerged source and previously published versions", t => {
  const f = fixture(t);
  fs.writeFileSync("feature", "unmerged");
  f.git("add", "feature");
  f.git("commit", "--quiet", "-m", "unmerged");
  assert.throws(() => createRuntimePlan(f.options));
  f.git("checkout", "--detach", f.source);
  f.git("tag", "cv-runtime/windows-x86_64/1.2.0");
  assert.throws(() => createRuntimePlan(f.options), /Newer runtime tag/);
});

test("legacy date-based versions can migrate to a new semver release", t => {
  const f = fixture(t);
  fs.writeFileSync("versions.toml", fs.readFileSync("versions.toml", "utf8").replaceAll('"1.0.0"', '"2026.05.08-runtime1"'));
  for (const entry of Object.values(f.manifest.platforms)) entry.runtimeVersion = "2026.05.08-runtime1";
  fs.writeFileSync("src-tauri/resources/runtime-manifest.json", JSON.stringify(f.manifest));
  f.git("add", "versions.toml", "src-tauri/resources/runtime-manifest.json");
  f.git("commit", "--quiet", "-m", "legacy version");
  f.git("update-ref", "refs/remotes/origin/main", "HEAD");
  const plan = createRuntimePlan({ ...f.options, version: "0.2.0" });
  assert.equal(validateRuntimePlan(plan).version, "0.2.0");
});

test("validation allows a retry at the same source but never moves a runtime tag", t => {
  const f = fixture(t);
  f.git("tag", "cv-runtime/windows-x86_64/1.1.0");
  assert.equal(validateRuntimePlan(f.plan), f.plan);
  assert.throws(() => createRuntimePlan(f.options), /Tag already exists/);
  f.git("commit", "--allow-empty", "--quiet", "-m", "other source");
  f.git("tag", "--force", "cv-runtime/windows-x86_64/1.1.0");
  assert.throws(() => validateRuntimePlan(f.plan), /Refusing to move tag/);
});

test("validation rejects mutated source versions and platform lists", t => {
  const { plan } = fixture(t);
  const mutated = structuredClone(plan);
  mutated.previous[platforms[0]].version = "0.9.0";
  assert.throws(() => validateRuntimePlan(mutated), /source version changed/);
  assert.throws(() => validateRuntimePlan({ ...plan, platforms: [] }), /platforms/);
  assert.throws(() => validateRuntimePlan({ ...plan, platforms: [platforms[0], platforms[0]] }), /platforms/);
});

test("all reports and ZIP hashes are validated before any upload", async t => {
  const f = fixture(t);
  f.build();
  fs.appendFileSync(path.join(f.directory, platforms[1], runtimeArtifact(f.plan, platforms[1]).name), "tampered");
  let calls = 0;
  await assert.rejects(uploadRuntime(f.plan, f.directory, () => { calls += 1; }), /report mismatch/);
  assert.equal(calls, 0);
  fs.rmSync(path.join(f.directory, platforms[1], "report.json"));
  assert.throws(() => assembleRuntime(f.plan, f.directory));
});

test("verification reports must belong to this exact release and platform", t => {
  const f = fixture(t);
  f.build();
  const file = path.join(f.directory, platforms[0], "report.json");
  const report = JSON.parse(fs.readFileSync(file, "utf8"));
  for (const mutation of [{ status: "failed" }, { platform: platforms[1] }, { sourceSha: "b".repeat(40) }, { planSha256: "b".repeat(64) }, { version: "1.2.0" }]) {
    fs.writeFileSync(file, JSON.stringify({ ...report, ...mutation }));
    assert.throws(() => assembleRuntime(f.plan, f.directory), /report mismatch/);
  }
});

test("immutable uploads verify R2 bytes and reject a conflicting existing object", async t => {
  const f = fixture(t);
  f.build();
  const objects = new Map<string, Buffer>();
  const aws: R2Client = args => {
    const key = args[args.indexOf("--key") + 1];
    if (args[1] === "put-object") {
      assert.equal(args[args.indexOf("--if-none-match") + 1], "*");
      if (objects.has(key)) throw new Error("PreconditionFailed (412)");
      objects.set(key, fs.readFileSync(args[args.indexOf("--body") + 1]));
    } else if (args[1] === "get-object") {
      const destination = args.at(-1);
      const object = objects.get(key);
      assert.ok(destination && object);
      fs.writeFileSync(destination, object);
    } else assert.fail(JSON.stringify(args));
  };
  const receipt = await uploadRuntime(f.plan, f.directory, aws);
  assert.equal(objects.size, 2);
  assert.deepEqual(await uploadRuntime(f.plan, f.directory, aws), receipt);
  objects.set(receipt.builds[0].key, Buffer.from("different published bytes"));
  await assert.rejects(uploadRuntime(f.plan, f.directory, aws), /Immutable object differs/);
});

test("runtime tags point at the fixed source and push atomically", t => {
  const f = fixture(t);
  const remote = path.join(f.root, "remote.git");
  f.git("init", "--bare", "--quiet", remote);
  f.git("remote", "add", "origin", remote);
  tagRuntime(f.plan);
  tagRuntime(f.plan);
  for (const platform of platforms) {
    assert.equal(f.git("rev-parse", `cv-runtime/${platform}/1.1.0`), f.source);
    assert.match(f.git("ls-remote", "--tags", "origin", `cv-runtime/${platform}/1.1.0`), new RegExp(`^${f.source}\\s`));
  }
});

test("sync updates only selected runtime metadata, is idempotent and preserves app changes", t => {
  const f = fixture(t);
  const selected = createRuntimePlan({ ...f.options, selection: "windows-x86_64" });
  const receipt = f.build(selected);
  fs.appendFileSync("versions.toml", '\n[unrelated]\nvalue = "preserved"\n');
  syncRuntime(receipt);
  const result = JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json", "utf8"));
  assert.equal(result.platforms["windows-x86_64"].runtimeVersion, "1.1.0");
  assert.deepEqual(result.platforms["darwin-aarch64"], f.manifest.platforms["darwin-aarch64"]);
  assert.deepEqual(result.platforms["darwin-x86_64"], f.manifest.platforms["darwin-x86_64"]);
  assert.match(fs.readFileSync("versions.toml", "utf8"), /version = "1.2.3"/);
  assert.match(fs.readFileSync("versions.toml", "utf8"), /preserved/);
  syncRuntime(receipt);
});

test("sync rejects metadata races and unexpected platform receipts without partial writes", t => {
  const f = fixture(t);
  const receipt = f.build();
  const manifestPath = "src-tauri/resources/runtime-manifest.json";
  const raced = structuredClone(f.manifest);
  raced.platforms[platforms[1]].runtimeVersion = "1.2.0";
  fs.writeFileSync(manifestPath, JSON.stringify(raced));
  const before = fs.readFileSync("versions.toml", "utf8");
  assert.throws(() => syncRuntime(receipt), /metadata changed/);
  assert.equal(fs.readFileSync("versions.toml", "utf8"), before);
  assert.deepEqual(JSON.parse(fs.readFileSync(manifestPath, "utf8")), raced);
  assert.throws(() => syncRuntime({ ...receipt, builds: [...receipt.builds, receipt.builds[0]] }), /receipt platforms/);
});
