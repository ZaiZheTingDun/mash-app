import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
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
  validatePreflightRun,
} from "../shared/app-release.mjs";
import { digest, platforms } from "../shared/cv-code-release.mjs";

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
  write(path.join(candidate, "mash-cv-code-v1.0.0.zip"), "original code ZIP");
  const manifest = {
    mashCvCodeVersion: "1.0.0",
    platforms: Object.fromEntries(
      platforms.map((platform) => [
        platform,
        {
          runtimeVersion: "1.0.0",
          runtimeUrl: `https://example.invalid/mash/runtime/${platform}.zip`,
          runtimeSha256: createHash("sha256").update(`runtime ${platform}`).digest("hex"),
          codeUrl: "https://example.invalid/mash/runtime/mash-cv/code/mash-cv-code-v1.0.0.zip",
          codeSha256: digest(path.join(candidate, "mash-cv-code-v1.0.0.zip")),
        },
      ]),
    ),
  };
  write(".gitignore", "src-tauri/target/\nsidecar/mash_cv/dist/\n");
  write(
    "versions.toml",
    '[app]\nversion = "1.0.0"\n[mash_cv]\ncode = "1.0.0"\n[mash_cv.runtime]\nwindows-x86_64 = "1.0.0"\ndarwin-aarch64 = "1.0.0"\n',
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
  write("sidecar/mash_cv/mash_cv/cv.py", "# original\n");
  write("sidecar/mash_cv/runtime_launcher.py", "# launcher\n");
  write("sidecar/mash_cv/tests/test_cv.py", "# tests\n");
  write("CHANGELOG.md", "# Changelog\n\n## 未发布\n\n- Feature\n");
  const commit = () => {
    git("add", ".");
    git("commit", "--quiet", "-m", "fixture");
    git("update-ref", "refs/remotes/origin/main", "HEAD");
  };
  commit();
  git("tag", "cv-code/1.0.0");
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
    if (selectedPlan.code.changed) {
      write(path.join(candidate, selectedPlan.code.artifact), "new code ZIP");
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

test("preflight is read-only, pins the chosen ref, and ignores a newer unrelated CV release", (t) => {
  const release = createReleaseFixture(t);
  const before = release.git("status", "--porcelain");
  release.git("tag", "cv-code/9.0.0");
  const plan = release.plan();
  assert.equal(plan.ready, true, plan.blockers.join("; "));
  assert.equal(plan.sourceSha, release.source);
  assert.equal(plan.code.baselineSha, release.git("rev-parse", "cv-code/1.0.0"));
  assert.equal(plan.code.changed, false);
  assert.equal(release.git("status", "--porcelain"), before);
  assert.match(summary(plan), /READY/);
  release.write("app.txt", "newer main\n");
  release.commit();
  assert.deepEqual(release.plan({ ref: release.source }), plan);
  assert.doesNotThrow(() => validatePlan(plan, release.env));
});

test("actual packaged source changes require a CV version; tests and app templates do not", (t) => {
  const release = createReleaseFixture(t);
  release.write("sidecar/mash_cv/tests/test_cv.py", "# more tests\n");
  release.write("src-tauri/resources/servers/jp/templates/new.png", "template");
  release.write("sidecar/mash_cv/mash_cv/__pycache__/cache.pyc", "ignored");
  release.commit();
  assert.equal(release.plan().ready, true);
  assert.equal(release.plan().code.testChanges.length, 1);
  assert.equal(codeDigest("HEAD"), codeDigest("cv-code/1.0.0"));
  release.write("sidecar/mash_cv/mash_cv/cv.py", "# changed\n");
  release.commit();
  assert.equal(release.plan().ready, false);
  assert.match(summary(release.plan()), /provide a new cv_code_version/);
  assert.equal(release.plan({ codeVersion: "1.0.1" }).ready, true);
  assert.equal(release.plan({ codeVersion: "1.0.0" }).ready, false);
});

for (const file of [
  "sidecar/mash_cv/runtime_launcher.py",
  "sidecar/mash_cv/mash_cv/models/model.onnx",
  "sidecar/mash_cv/build_sidecar.py",
  "sidecar/mash_cv/poetry.lock",
]) {
  test(`runtime changes block app-only and joint releases: ${file}`, (t) => {
    const release = createReleaseFixture(t);
    release.write(file, "changed runtime input");
    release.commit();
    const plan = release.plan();
    assert.equal(plan.ready, false);
    assert.equal(plan.code.changed, false);
    for (const platform of platforms) {
      assert.ok(plan.runtimeChanges[platform].files.includes(file));
    }
    assert.match(plan.blockers.join("; "), /publish and apply its runtime first/);
  });
}

test("native runtime build changes affect only their platform", (t) => {
  const release = createReleaseFixture(t);
  release.write("scripts/windows/build-cv.ps1", "# native build change\n");
  release.commit();
  const plan = release.plan();
  assert.equal(plan.ready, false);
  assert.deepEqual(plan.runtimeChanges["windows-x86_64"].files, ["scripts/windows/build-cv.ps1"]);
  assert.deepEqual(plan.runtimeChanges["darwin-aarch64"].files, []);
});

test("missing baseline, bad runtime, conflicting tags, versions and outside refs fail closed", (t) => {
  const release = createReleaseFixture(t);
  assert.equal(release.plan({ appVersion: "1.0.0" }).ready, false);
  assert.equal(release.plan({ codeVersion: "1.0.1" }).ready, false);
  release.git("tag", "-d", "cv-code/1.0.0");
  assert.match(release.plan().blockers.join(";"), /Missing source tag/);
  release.git("tag", "cv-code/1.0.0", release.source);
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

test("only a successful same-repository default-branch preflight is accepted", (t) => {
  const release = createReleaseFixture(t);
  const plan = release.plan();
  const metadata = {
    path: ".github/workflows/release-preflight.yml",
    event: "workflow_dispatch",
    conclusion: "success",
    status: "completed",
    head_branch: "main",
    head_sha: plan.toolingSha,
    repository: { full_name: "owner/mash" },
    head_repository: { full_name: "owner/mash" },
  };
  validatePreflightRun(plan, metadata, "owner/mash", "main");
  for (const patch of [
    { conclusion: "failure" },
    { event: "pull_request" },
    { head_branch: "feature" },
    { head_sha: "0".repeat(40) },
    { path: ".github/workflows/other.yml" },
    { head_repository: { full_name: "fork/mash" } },
  ]) {
    assert.throws(
      () => validatePreflightRun(plan, { ...metadata, ...patch }, "owner/mash", "main"),
      /successful default-branch/,
    );
  }
});

test("joint preparation embeds the tested CV bytes and makes a deterministic, fetchable release commit", (t) => {
  const release = createReleaseFixture(t);
  release.write("sidecar/mash_cv/mash_cv/cv.py", "# changed CV\n");
  release.commit();
  const plan = release.plan({ codeVersion: "1.0.1" });
  assert.equal(plan.ready, true);
  const prepared = release.prepare(plan);
  assert.equal(prepared.planSha256, planDigest(plan));
  assert.equal(prepared.manifest.mashCvCodeVersion, "1.0.1");
  assert.equal(prepared.code.sha256, digest(path.join(release.candidate, plan.code.artifact)));
  for (const platform of platforms) {
    assert.equal(prepared.manifest.platforms[platform].codeSha256, prepared.code.sha256);
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

test("app-only preparation rejects altered reused CV bytes", (t) => {
  const release = createReleaseFixture(t);
  fs.writeFileSync(
    path.join(release.candidate, "mash-cv-code-v1.0.0.zip"),
    "wrong published bytes",
  );
  assert.throws(() => release.prepare(), /Published CV code hash mismatch/);
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
  receipt.compatibility.codeSha256 = "0".repeat(64);
  fs.writeFileSync(file, JSON.stringify(receipt));
  assert.throws(() => assemble(prepared, builds, release.candidate, () => {}), /compatibility/);
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
  const fetcher = async (url) => ({
    ok: objects.has(new URL(url).pathname.slice(1)),
    arrayBuffer: async () => objects.get(new URL(url).pathname.slice(1)).bytes,
  });
  return { objects, calls, aws, seed, fetcher };
}

test("uploads verify immutable R2 and public CDN bytes before any channel promotion", async (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.assembled();
  const store = fakeStore();
  store.seed(
    receipt.code.objectKey,
    fs.readFileSync(path.join(release.candidate, receipt.code.artifact)),
  );
  for (const [platform, runtime] of Object.entries(receipt.plan.runtimes)) {
    store.seed(new URL(runtime.runtimeUrl).pathname.slice(1), `runtime ${platform}`);
  }
  t.mock.method(globalThis, "fetch", async (url) => ({
    ok: true,
    arrayBuffer: async () => store.objects.get(url.slice(receipt.plan.baseUrl.length + 1)).bytes,
  }));
  await uploadRelease(receipt, release.candidate, store.aws);
  await uploadRelease(receipt, release.candidate, store.aws);
  assert.equal(
    store.calls.some((call) => /releases\/(stable|windows-stable)\//.test(call.key)),
    false,
  );
  t.mock.method(globalThis, "fetch", async () => ({
    ok: true,
    arrayBuffer: async () => Buffer.from("wrong CDN bytes"),
  }));
  await assert.rejects(
    uploadRelease(receipt, release.candidate, store.aws),
    /Public CDN verification failed/,
  );
});

test("both channels are validated before writes, and successful promotion can resume", async (t) => {
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
    promoteRelease(receipt, release.candidate, store.aws, () => {}, store.fetcher),
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
      store.fetcher,
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
  await promoteRelease(receipt, release.candidate, store.aws, () => {}, store.fetcher);
  await promoteRelease(receipt, release.candidate, store.aws, () => {}, store.fetcher);
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
    promoteRelease(receipt, release.candidate, store.aws, () => {}, store.fetcher),
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
      store.fetcher,
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
      installers.fetcher,
    ),
    /412/,
  );
  assert.equal(installers.objects.get(latest).bytes.toString(), "racing installer");
});

test("GitHub draft verification reuses matching assets and never clobbers conflicts", (t) => {
  const release = createReleaseFixture(t);
  release.write("sidecar/mash_cv/mash_cv/cv.py", "# joint release\n");
  release.commit();
  const prepared = release.prepare(release.plan({ codeVersion: "1.0.1" }));
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
  assert.equal(releases.size, 2);
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
