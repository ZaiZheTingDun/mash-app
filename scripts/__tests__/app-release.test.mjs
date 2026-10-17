import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import test from "node:test";
import {
  assemble,
  codeDigest,
  createPlan,
  planDigest,
  prepareSource,
  promoteRelease,
  publishGithub,
  publishTags,
  recordPlatform,
  releaseNotes,
  summary,
  syncMetadata,
  uploadRelease,
  validatePlan,
} from "../shared/app-release.mjs";
import { digest, platforms } from "../shared/release-artifacts.mjs";


function createReleaseFixture(t) {
  const previous = process.cwd();
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "mash app release 中文 "));
  const root = path.join(directory, "repo");
  fs.mkdirSync(root);
  process.chdir(root);
  t.after(() => {
    process.chdir(previous);
    fs.rmSync(directory, { recursive: true, force: true });
  });
  const write = (file, value) => {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, value);
  };
  const git = (...args) =>
    execFileSync("git", args, {
      encoding: "utf8",
      stdio: "pipe",
      windowsHide: true,
    }).trim();
  git("init", "--quiet");
  git("config", "user.name", "Release Test");
  git("config", "user.email", "test@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("config", "tag.gpgsign", "false");
  const candidate = path.join(directory, "candidate");
  fs.mkdirSync(candidate);
  const manifest = {
    platforms: Object.fromEntries(
      platforms.map((platform) => [
        platform,
        {
          runtimeVersion: "1.0.0",
          runtimeUrl: `https://example.invalid/mash/runtime/${platform}.zip`,
          runtimeSha256: createHash("sha256").update(`runtime ${platform}`).digest("hex"),
        },
      ]),
    ),
  };
  write(".gitattributes", fs.readFileSync(new URL("../../.gitattributes", import.meta.url), "utf8"));
  write(".gitignore", "src-tauri/target/\nsidecar/cv-code/dist/\nsidecar/cv-runtime/dist/\n");
  write(
    "versions.toml",
    '[app]\nversion = "1.0.0"\n[mash_cv.runtime]\nwindows-x86_64 = "1.0.0"\ndarwin-aarch64 = "1.0.0"\n',
  );
  write("src-tauri/resources/runtime-manifest.json", JSON.stringify(manifest));
  write(
    "src-tauri/tauri.conf.json",
    JSON.stringify({
      version: "1.0.0",
      productName: "Mash",
      plugins: { updater: { pubkey: "mac-public-key" } },
    }),
  );
  write(
    "src-tauri/tauri.windows.conf.json",
    JSON.stringify({ plugins: { updater: { pubkey: "windows-public-key" } } }),
  );
  write("src-tauri/Cargo.toml", '[package]\nname = "mash"\nversion = "1.0.0"\n');
  write("src-tauri/Cargo.lock", '[[package]]\nname = "mash"\nversion = "1.0.0"\n');
  write("sidecar/cv-code/mash_cv/cv.py", "# original\n");
  write("sidecar/cv-runtime/runtime_launcher.py", "# launcher\n");
  write("sidecar/cv-code/tests/test_cv.py", "# tests\n");
  write("CHANGELOG.md", "# Changelog\n\n## 未发布\n\n- Feature\n");
  const commit = () => {
    git("add", ".");
    git("commit", "--quiet", "-m", "fixture");
    git("update-ref", "refs/remotes/origin/main", "HEAD");
  };
  commit();
  git("tag", "v1.0.0");
  for (const platform of platforms) {
    git("tag", `cv-runtime/${platform}/1.0.0`);
  }
  write("app.txt", "feature\n");
  commit();
  const source = git("rev-parse", "HEAD");
  const options = {
    ref: "HEAD",
    appVersion: "1.1.0",
    branch: "main",
    baseUrl: "https://example.invalid",
    prefix: "mash",
    toolingSha: source,
    createdAt: "2026-10-16T10:00:00.000Z",
  };
  const env = { RELEASE_BASE_URL: options.baseUrl, R2_PREFIX: options.prefix };
  const plan = (extra) => createPlan({ ...options, ...extra });
  const prepare = (selected) => {
    const selectedPlan = selected || plan();
    if (!fs.existsSync(path.join(candidate, selectedPlan.code.artifact))) {
      write(path.join(candidate, selectedPlan.code.artifact), "bundled code ZIP");
    }
    fs.writeFileSync(path.join(candidate, "release-plan.json"), JSON.stringify(selectedPlan));
    return prepareSource(selectedPlan, candidate);
  };
  const build = (prepared) => {
    const builds = path.join(directory, "builds");
    for (const platform of platforms) {
      const bundle = "src-tauri/target/release/bundle";
      const updater =
        platform === "windows-x86_64"
          ? `${bundle}/nsis/Mash_1.1.0_x64-setup.exe`
          : `${bundle}/macos/Mash.app.tar.gz`;
      write(updater, `tested updater ${platform}`);
      write(`${updater}.sig`, `signature ${platform}`);
      if (platform === "darwin-aarch64") {
        write(`${bundle}/dmg/Mash_1.1.0_aarch64.dmg`, "tested DMG");
      }
      const report = {
        platform,
        status: "passed",
        sourceSha: prepared.plan.sourceSha,
        codeSha256: prepared.code.sha256,
        runtimeSha256: prepared.plan.runtimes[platform].runtimeSha256,
      };
      const reportFile = path.join(directory, `${platform}.json`);
      write(reportFile, JSON.stringify(report));
      recordPlatform(prepared, platform, path.join(builds, platform), reportFile);
    }
    return builds;
  };
  const assembled = () => {
    const prepared = prepare();
    const builds = build(prepared);
    return assemble(prepared, builds, candidate, () => {});
  };
  return {
    directory,
    root,
    candidate,
    git,
    write,
    commit,
    source,
    manifest,
    options,
    env,
    plan,
    prepare,
    build,
    assembled,
  };
}

test("release notes select the target section before unreleased entries and reject an empty section", () => {
  assert.equal(releaseNotes("## 未发布\n\n## v1.0.0 - 2026-10-15\n\n- Old", "1.1.0"), "");
  assert.equal(
    releaseNotes(
      "## 未发布\n\n- Future\n\n## v1.1.0 - 2026-10-16\n\n- Release\n\n## v1.0.0 - 2026-10-15\n\n- Old",
      "1.1.0",
    ),
    "- Release",
  );
});

test("planning is read-only, pins the chosen ref, and ignores a newer unrelated CV release", (t) => {
  const release = createReleaseFixture(t);
  const before = release.git("status", "--porcelain");
  release.git("tag", "cv-code/9.0.0");
  const plan = release.plan();
  assert.equal(plan.ready, true, plan.blockers.join("; "));
  assert.equal(plan.sourceSha, release.source);
  assert.equal(release.git("status", "--porcelain"), before);
  assert.match(summary(plan), /READY/);
  release.write("app.txt", "newer main\n");
  release.commit();
  assert.deepEqual(release.plan({ ref: release.source }), plan);
  assert.doesNotThrow(() => validatePlan(plan, release.env));
});

test("code changes follow the app without requiring a CV release or code tag", t => {
  const release = createReleaseFixture(t);
  const previous = codeDigest("HEAD");
  release.write("sidecar/cv-code/mash_cv/cv.py", "# changed code\n");
  release.commit();
  const plan = release.plan();
  assert.equal(plan.ready, true, plan.blockers.join("; "));
  assert.notEqual(plan.code.sourceDigest, previous);
  assert.equal(plan.code.artifact, "mash-cv-code-v1.1.0.zip");
});

test("runtime source and build edits reuse the pinned artifacts pending compatibility checks", (t) => {
  const release = createReleaseFixture(t);
  for (const file of [
    "sidecar/cv-runtime/runtime_launcher.py",
    "sidecar/cv-runtime/models/model.onnx",
    "sidecar/cv-runtime/build_runtime.py",
    "sidecar/cv-runtime/poetry.lock",
    "sidecar/cv-runtime/pyproject.toml",
    "scripts/windows/build-cv.ps1",
    "scripts/macos/build-cv.sh",
  ]) {
    release.write(file, "changed runtime source input");
  }
  release.commit();
  const plan = release.plan();
  assert.equal(plan.ready, true, plan.blockers.join("; "));
  assert.deepEqual(plan.runtimes, release.manifest.platforms);
  assert.doesNotThrow(() => validatePlan(plan, release.env));
});

test("runtime directory moves and missing historical tags do not block an app release", (t) => {
  const release = createReleaseFixture(t);
  fs.renameSync("sidecar/cv-runtime", "sidecar/old-runtime-location");
  release.commit();
  for (const platform of platforms) {
    release.git("tag", "-d", `cv-runtime/${platform}/1.0.0`);
  }
  const plan = release.plan();
  assert.equal(plan.ready, true, plan.blockers.join("; "));
  assert.deepEqual(plan.runtimes, release.manifest.platforms);
});

test("bad runtime metadata, conflicting tags, versions and outside refs fail closed", (t) => {
  const release = createReleaseFixture(t);
  assert.equal(release.plan({ appVersion: "1.0.0" }).ready, false);
  const manifest = structuredClone(release.manifest);
  manifest.platforms["windows-x86_64"].runtimeSha256 = "0".repeat(64);
  release.write("src-tauri/resources/runtime-manifest.json", JSON.stringify(manifest));
  release.commit();
  assert.equal(release.plan().ready, false);
  release.git("tag", "v1.1.0");
  assert.match(release.plan().blockers.join(";"), /already exists/);
  release.write("unreviewed.txt", "unreviewed");
  release.git("add", ".");
  release.git("commit", "--quiet", "-m", "unreviewed");
  assert.throws(() => release.plan(), /merge-base/);
});

test("execution rejects edited plans, destination changes and newer releases but permits main advancing", (t) => {
  const release = createReleaseFixture(t);
  const plan = release.plan();
  assert.throws(() => validatePlan({ ...plan, ready: false }, release.env), /blocked/);
  assert.throws(() => validatePlan({ ...plan, sourceSha: "0".repeat(40) }, release.env));
  assert.throws(() => validatePlan({ ...plan, publicKeys: {} }, release.env), /plan changed/);
  assert.throws(() => validatePlan(plan, { ...release.env, R2_PREFIX: "other" }), /destination/);
  release.git("tag", "v2.0.0");
  assert.throws(() => validatePlan(plan, release.env), /Newer app tag/);
});

test("release planning emits pinned job outputs and fails immediately for a blocked plan", (t) => {
  const release = createReleaseFixture(t);
  const planFile = path.join(release.directory, "plan.json");
  const outputs = path.join(release.directory, "outputs.txt");
  const summaryFile = path.join(release.directory, "summary.md");
  const env = {
    ...process.env,
    ...release.env,
    SOURCE_REF: release.source,
    APP_VERSION: "1.1.0",
    DEFAULT_BRANCH: "main",
    GITHUB_SHA: release.source,
    GITHUB_OUTPUT: outputs,
    GITHUB_STEP_SUMMARY: summaryFile,
  };
  const command = [fileURLToPath(new URL("../shared/app-release.mjs", import.meta.url)), "plan", planFile];
  execFileSync(process.execPath, command, { env, stdio: "pipe", windowsHide: true });
  const plan = JSON.parse(fs.readFileSync(planFile, "utf8"));
  assert.equal(plan.ready, true);
  assert.equal(plan.sourceSha, release.source);
  assert.equal(fs.readFileSync(outputs, "utf8"),
    `ready=true\nsource_sha=${release.source}\ntooling_sha=${release.source}\napp_version=1.1.0\n`);
  assert.match(fs.readFileSync(summaryFile, "utf8"), /validate bundled code against each pinned native runtime/);
  assert.equal(release.git("status", "--porcelain"), "");

  release.write("CHANGELOG.md", "# Changelog\n\n## v1.0.0 - 2026-10-15\n\n- Old release\n");
  release.commit();
  const blocked = spawnSync(process.execPath, command, {
    env: { ...env, SOURCE_REF: release.git("rev-parse", "HEAD") },
    encoding: "utf8",
    windowsHide: true,
  });
  assert.ifError(blocked.error);
  assert.equal(blocked.status, 1);
  assert.match(blocked.stderr, /Release plan is blocked:.*CHANGELOG.md/);
  assert.equal(JSON.parse(fs.readFileSync(planFile, "utf8")).ready, false);
  assert.match(fs.readFileSync(summaryFile, "utf8"), /BLOCKED/);
});

test("joint preparation embeds the tested CV bytes and makes a deterministic, fetchable release commit", (t) => {
  const release = createReleaseFixture(t);
  release.write("sidecar/cv-code/mash_cv/cv.py", "# changed CV\n");
  release.commit();
  const plan = release.plan();
  assert.equal(plan.ready, true);
  const prepared = release.prepare(plan);
  assert.equal(prepared.planSha256, planDigest(plan));
  assert.equal(prepared.manifest.mashCvCodeVersion, undefined);
  assert.equal(prepared.code.sha256, digest(path.join(release.candidate, plan.code.artifact)));
  for (const platform of platforms) {
    assert.equal(prepared.manifest.platforms[platform].codeSha256, undefined);
  }
  assert.equal(release.git("rev-parse", "HEAD"), prepared.releaseSha);
  assert.equal(release.git("rev-parse", "HEAD^"), plan.sourceSha);
  assert.match(fs.readFileSync("CHANGELOG.md", "utf8"), /v1.1.0 - 2026-10-16/);
  assert.equal(release.git("status", "--porcelain"), "");
  release.git(
    "fetch",
    path.join(release.candidate, "source.bundle"),
    "refs/heads/release-candidate",
  );
  assert.equal(release.git("rev-parse", "FETCH_HEAD"), prepared.releaseSha);
  release.git("branch", "planned-source", plan.sourceSha);
  const consumer = path.join(release.directory, "shallow consumer");
  release.git(
    "clone",
    "--quiet",
    "--no-local",
    "--depth",
    "1",
    "--branch",
    "planned-source",
    release.root,
    consumer,
  );
  release.git(
    "-C",
    consumer,
    "fetch",
    path.join(release.candidate, "source.bundle"),
    "refs/heads/release-candidate",
  );
  assert.equal(release.git("-C", consumer, "rev-parse", "FETCH_HEAD"), prepared.releaseSha);
  release.git("checkout", "--detach", plan.sourceSha);
  assert.equal(release.prepare(plan).releaseSha, prepared.releaseSha);
});

test("macOS packaging selects system xattr ahead of Poetry's Python command", {
  skip: process.platform !== "darwin" || process.arch !== "arm64",
}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash-xattr-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const tools = path.join(root, "tools");
  const bundle = path.join(root, "Test.app");
  fs.mkdirSync(tools);
  fs.mkdirSync(bundle);
  fs.writeFileSync(path.join(tools, "xattr"), "#!/bin/bash\necho 'Python xattr selected' >&2\nexit 64\n", { mode: 0o755 });
  fs.writeFileSync(path.join(tools, "pnpm"), '#!/bin/bash\nexec xattr -crs "$XATTR_TEST_BUNDLE"\n', { mode: 0o755 });
  const workflow = fs.readFileSync(new URL("../../.github/workflows/app-release.yml", import.meta.url), "utf8");
  const step = workflow.split("      - name: Build signed macOS updater and DMG\n")[1]
    .split("\n      - name: ")[0];
  const command = step.split("        run: |\n")[1].split("\n")
    .map(line => line.slice(10)).join("\n");
  const result = spawnSync("/bin/bash", ["-e", "-c", command], {
    env: { ...process.env, PATH: `${tools}:${process.env.PATH}`, XATTR_TEST_BUNDLE: bundle },
    encoding: "utf8",
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /\/usr\/bin\/xattr/);
});

test("Windows checkout preserves the exact prepared runtime manifest bytes", t => {
  const release = createReleaseFixture(t);
  const manifest = "src-tauri/resources/runtime-manifest.json";
  release.write(manifest, `${JSON.stringify(release.manifest, null, 2)}\n`);
  release.commit();
  const prepared = release.prepare();
  release.git("config", "core.autocrlf", "true");
  fs.unlinkSync(manifest);
  release.git("checkout-index", "--force", "--", manifest);
  assert.equal(digest(manifest), prepared.manifestSha256);
  assert.doesNotThrow(() => release.build(prepared));
});

test("assembly rejects tampered internal bundled-code candidates", t => {
  const release = createReleaseFixture(t);
  const prepared = release.prepare();
  const builds = release.build(prepared);
  fs.writeFileSync(path.join(release.candidate, prepared.code.artifact), "tampered");
  assert.throws(() => assemble(prepared, builds, release.candidate, () => {}), /Candidate file changed/);
});

test("assembly requires both matching native builds, embedded manifest, files and signatures", (t) => {
  const release = createReleaseFixture(t);
  const prepared = release.prepare();
  const builds = release.build(prepared);
  const keys = [];
  const receipt = assemble(prepared, builds, release.candidate, (_file, _sig, key) =>
    keys.push(key),
  );
  assert.deepEqual(keys, ["windows-public-key", "mac-public-key"]);
  assert.equal(receipt.builds.length, 2);
  // The assembled candidate is self-contained, so publish can verify it again without rebuilding.
  assert.deepEqual(
    assemble(prepared, release.candidate, release.candidate, () => {}),
    receipt,
  );
  assert.throws(
    () =>
      assemble(prepared, builds, release.candidate, () => {
        throw new Error("invalid signature");
      }),
    /invalid signature/,
  );
  fs.writeFileSync(path.join(builds, "windows-x86_64", "Mash_1.1.0_x64-setup.exe"), "tampered");
  assert.throws(
    () => assemble(prepared, builds, release.candidate, () => {}),
    /Candidate file changed/,
  );
  fs.rmSync(path.join(builds, "darwin-aarch64"), { recursive: true });
  assert.throws(() => assemble(prepared, builds, release.candidate, () => {}));
});

test("compatibility reports and build checkout cannot be substituted", (t) => {
  const release = createReleaseFixture(t);
  const prepared = release.prepare();
  const builds = release.build(prepared);
  const file = path.join(builds, "darwin-aarch64", "platform.json");
  const receipt = JSON.parse(fs.readFileSync(file));
  for (const mismatch of [
    { status: "failed" },
    { codeSha256: "0".repeat(64) },
    { runtimeSha256: "0".repeat(64) },
  ]) {
    fs.writeFileSync(file, JSON.stringify({
      ...receipt,
      compatibility: { ...receipt.compatibility, ...mismatch },
    }));
    assert.throws(() => assemble(prepared, builds, release.candidate, () => {}), /compatibility/);
  }
  release.git("checkout", "--detach", prepared.plan.sourceSha);
  assert.throws(() => release.build(prepared), /Build source/);
});

function fakeStore() {
  const objects = new Map();
  const calls = [];
  let serial = 0;
  const seed = (key, bytes, metadata = {}) =>
    objects.set(key, {
      bytes: Buffer.from(bytes),
      etag: `"${++serial}"`,
      metadata,
    });
  const aws = (args) => {
    const key = args[args.indexOf("--key") + 1];
    calls.push({ operation: args[1], key, args });
    const object = objects.get(key);
    if (args[1] === "head-object") {
      if (!object) {
        throw new Error("NoSuchKey (404)");
      }
      return JSON.stringify({ ETag: object.etag, Metadata: object.metadata });
    }
    if (args[1] === "get-object") {
      if (!object) {
        throw new Error("NoSuchKey (404)");
      }
      fs.writeFileSync(args.at(-1), object.bytes);
      return JSON.stringify({ ETag: object.etag });
    }
    if (args.includes("--if-none-match") && object) {
      throw new Error("PreconditionFailed (412)");
    }
    if (args.includes("--if-match") && args[args.indexOf("--if-match") + 1] !== object?.etag) {
      throw new Error("PreconditionFailed (412)");
    }
    seed(
      key,
      fs.readFileSync(args[args.indexOf("--body") + 1]),
      args.includes("--metadata") ? JSON.parse(args[args.indexOf("--metadata") + 1]) : {},
    );
    return "{}";
  };
  return { objects, calls, aws, seed };
}

test("uploads verify runtime and immutable artifact bytes through R2 without public downloads", async (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.assembled();
  const store = fakeStore();
  for (const [platform, runtime] of Object.entries(receipt.plan.runtimes)) {
    store.seed(new URL(runtime.runtimeUrl).pathname.slice(1), `runtime ${platform}`);
  }
  t.mock.method(globalThis, "fetch", async () => assert.fail("Publishing must use the R2 API"));
  await uploadRelease(receipt, release.candidate, store.aws);
  await uploadRelease(receipt, release.candidate, store.aws);
  assert.equal(
    store.calls.some((call) => /releases\/(stable|windows-stable)\//.test(call.key)),
    false,
  );
  for (const build of receipt.builds) {
    for (const file of build.files) {
      const key = `mash/releases/versions/v1.1.0/${build.platform}/${file.name}`;
      assert.ok(store.calls.some(call => call.operation === "get-object" && call.key === key));
    }
  }
  const runtime = receipt.plan.runtimes["windows-x86_64"];
  store.seed(new URL(runtime.runtimeUrl).pathname.slice(1), "wrong runtime bytes");
  await assert.rejects(
    uploadRelease(receipt, release.candidate, store.aws),
    /R2 object SHA-256 mismatch/,
  );
});

test("both channels are validated before writes, and successful promotion can resume", async (t) => {
  t.mock.method(globalThis, "fetch", async () => assert.fail("Promotion must use the R2 API"));
  const release = createReleaseFixture(t);
  const receipt = release.assembled();
  const store = fakeStore();
  store.seed(
    "mash/releases/stable/latest.json",
    JSON.stringify({ version: "1.0.0", platforms: {} }),
  );
  store.seed(
    "mash/releases/windows-stable/latest.json",
    JSON.stringify({ version: "2.0.0", platforms: {} }),
  );
  await assert.rejects(
    promoteRelease(receipt, release.candidate, store.aws, () => {}),
    /newer release/,
  );
  assert.equal(
    store.calls.some(
      (call) =>
        call.operation === "put-object" && /releases\/(stable|windows-stable)\//.test(call.key),
    ),
    false,
  );
  store.seed(
    "mash/releases/windows-stable/latest.json",
    JSON.stringify({ version: "1.0.0", platforms: {} }),
  );
  await assert.rejects(
    promoteRelease(
      receipt,
      release.candidate,
      (args) => {
        if (
          args[1] === "put-object" &&
          args[args.indexOf("--key") + 1] === "mash/releases/windows-stable/latest.json"
        ) {
          throw new Error("temporary interruption");
        }
        return store.aws(args);
      },
      () => {},
    ),
    /temporary interruption/,
  );
  assert.equal(
    JSON.parse(store.objects.get("mash/releases/stable/latest.json").bytes).version,
    "1.1.0",
  );
  assert.equal(
    JSON.parse(store.objects.get("mash/releases/windows-stable/latest.json").bytes).version,
    "1.0.0",
  );
  await promoteRelease(receipt, release.candidate, store.aws, () => {});
  await promoteRelease(receipt, release.candidate, store.aws, () => {});
  for (const channel of ["stable", "windows-stable"]) {
    const manifest = JSON.parse(store.objects.get(`mash/releases/${channel}/latest.json`).bytes);
    assert.equal(manifest.version, "1.1.0");
    assert.deepEqual(Object.keys(manifest.platforms), platforms);
  }
  assert.ok(store.calls.some((call) => call.args.includes("--if-match")));
  store.seed("mash/downloads/latest/Mash_x64-setup.exe", "future installer", {
    "app-version": "2.0.0",
  });
  const writes = store.calls.filter(
    (call) =>
      call.operation === "put-object" && /releases\/(stable|windows-stable)\//.test(call.key),
  ).length;
  await assert.rejects(
    promoteRelease(receipt, release.candidate, store.aws, () => {}),
    /newer direct installer/,
  );
  assert.equal(
    store.calls.filter(
      (call) =>
        call.operation === "put-object" && /releases\/(stable|windows-stable)\//.test(call.key),
    ).length,
    writes,
  );
});

test("promotion verifies the direct installer bytes through R2", async t => {
  const release = createReleaseFixture(t);
  const receipt = release.assembled();
  const store = fakeStore();
  await assert.rejects(promoteRelease(receipt, release.candidate, args => {
    const result = store.aws(args);
    const key = args[args.indexOf("--key") + 1];
    if (args[1] === "get-object" && key.startsWith("mash/downloads/latest/")) {
      fs.writeFileSync(args.at(-1), "corrupt installer bytes");
    }
    return result;
  }, () => {}), /R2 object SHA-256 mismatch: mash\/downloads\/latest\//);
});

test("a racing publisher fails conditional promotion without overwriting its channel", async (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.assembled();
  const store = fakeStore();
  const key = "mash/releases/stable/latest.json";
  store.seed(key, JSON.stringify({ version: "1.0.0", platforms: {} }));
  const raced = JSON.stringify({ version: "2.0.0", platforms: {} });
  await assert.rejects(
    promoteRelease(
      receipt,
      release.candidate,
      (args) => {
        if (args[1] === "put-object" && args[args.indexOf("--key") + 1] === key) {
          store.seed(key, raced);
        }
        return store.aws(args);
      },
      () => {},
    ),
    /412/,
  );
  assert.equal(store.objects.get(key).bytes.toString(), raced);
  const installers = fakeStore();
  const latest = "mash/downloads/latest/Mash_x64-setup.exe";
  installers.seed(latest, "previous installer", { "app-version": "1.0.0" });
  await assert.rejects(
    promoteRelease(
      receipt,
      release.candidate,
      (args) => {
        if (args[1] === "put-object" && args[args.indexOf("--key") + 1] === latest) {
          installers.seed(latest, "racing installer", {
            "app-version": "2.0.0",
          });
        }
        return installers.aws(args);
      },
      () => {},
    ),
    /412/,
  );
  assert.equal(installers.objects.get(latest).bytes.toString(), "racing installer");
});

test("GitHub draft verification reuses matching assets and never clobbers conflicts", (t) => {
  const release = createReleaseFixture(t);
  release.write("sidecar/cv-code/mash_cv/cv.py", "# joint release\n");
  release.commit();
  const prepared = release.prepare(release.plan());
  const receipt = assemble(prepared, release.build(prepared), release.candidate, () => {});
  const releases = new Map();
  const calls = [];
  const gh = (args) => {
    calls.push(args);
    const tag = args[2];
    const release = releases.get(tag);
    switch (args[1]) {
      case "view":
        if (!release) {
          throw new Error("release not found");
        }
        return JSON.stringify({
          isDraft: release.isDraft,
          assets: [...release.assets.keys()].map((name) => ({ name })),
        });
      case "create":
        releases.set(tag, { isDraft: true, assets: new Map() });
        break;
      case "upload":
        release.assets.set(path.basename(args[3]), fs.readFileSync(args[3]));
        break;
      case "download":
        fs.writeFileSync(
          path.join(args[args.indexOf("--dir") + 1], args[args.indexOf("--pattern") + 1]),
          release.assets.get(args[args.indexOf("--pattern") + 1]),
        );
        break;
      case "edit":
        release.isDraft = false;
        break;
      default:
        throw new Error("Unexpected gh call");
    }
    return "";
  };
  publishGithub(receipt, release.candidate, gh);
  assert.equal(releases.size, 1);
  assert.ok(
    calls.findIndex((args) => args[1] === "edit") >
      calls.findLastIndex((args) => args[1] === "download"),
  );
  const uploads = calls.filter((args) => args[1] === "upload").length;
  publishGithub(receipt, release.candidate, gh);
  assert.equal(calls.filter((args) => args[1] === "upload").length, uploads);
  releases.get("v1.1.0").assets.set("app-release.json", Buffer.from("conflict"));
  const edits = calls.filter((args) => args[1] === "edit").length;
  assert.throws(() => publishGithub(receipt, release.candidate, gh), /GitHub asset differs/);
  assert.equal(calls.filter((args) => args[1] === "edit").length, edits);
  assert.equal(
    calls.some((args) => args.includes("--clobber")),
    false,
  );
});

test("release tags never move; the generated metadata commit can be synchronized once", (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.assembled();
  const remote = path.join(release.directory, "remote.git");
  release.git("init", "--bare", "--quiet", remote);
  release.git("remote", "add", "origin", remote);
  publishTags(receipt);
  publishTags(receipt);
  assert.equal(release.git("rev-parse", "v1.1.0"), receipt.releaseSha);
  assert.throws(() => publishTags({ ...receipt, releaseSha: release.source }), /Refusing to move/);
  release.git("checkout", "--detach", release.source);
  assert.equal(syncMetadata(receipt), true);
  assert.equal(syncMetadata(receipt), false);
});
