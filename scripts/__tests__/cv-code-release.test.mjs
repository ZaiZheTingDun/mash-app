import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import test from "node:test";
import {
  applyRelease,
  digest,
  location,
  planRelease,
  platforms,
  publishImmutable,
  recordRelease,
  tagRelease,
  validateReceipt,
  validateReports,
} from "../shared/cv-code-release.mjs";

function createReleaseFixture(t) {
  const previous = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "cv code 中文 "));
  process.chdir(root);
  t.after(() => {
    process.chdir(previous);
    fs.rmSync(root, { recursive: true, force: true });
  });
  const write = (file, text) => {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
  };
  const git = (...args) =>
    execFileSync("git", args, {
      encoding: "utf8",
      stdio: "pipe",
      windowsHide: true,
    }).trim();
  const manifest = {
    mashCvCodeVersion: "1.0.0",
    platforms: Object.fromEntries(
      platforms.map((platform) => [
        platform,
        {
          runtimeVersion: "0.1.0",
          runtimeUrl: `https://example.invalid/${platform}.zip`,
          runtimeSha256: "a".repeat(64),
          codeUrl: "https://example.invalid/old.zip",
          codeSha256: "b".repeat(64),
        },
      ]),
    ),
  };
  write("src-tauri/resources/runtime-manifest.json", JSON.stringify(manifest));
  write(
    "versions.toml",
    '[app]\nversion = "1.0.0"\n[mash_cv]\ncode = "1.0.0"\n[mash_cv.runtime]\nwindows-x86_64 = "0.1.0"\ndarwin-aarch64 = "0.1.0"\n',
  );
  write("sidecar/mash_cv/mash_cv/__init__.py", "# code\n");
  git("init", "--quiet");
  git("config", "user.name", "Release Test");
  git("config", "user.email", "test@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("config", "tag.gpgsign", "false");
  const commit = () => {
    git("add", ".");
    git("commit", "--quiet", "-m", "fixture");
  };
  commit();
  git("update-ref", "refs/remotes/origin/main", "HEAD");
  const plan = () => planRelease("1.1.0", "HEAD", "main", "https://example.invalid", "mash");
  const candidate = () => {
    write("dist/code.zip", "candidate bytes");
    return recordRelease(plan(), "dist/code.zip", "candidate");
  };
  return { root, write, git, commit, plan, candidate, manifest };
}

test("plan pins reviewed source and runtime hashes without changing the worktree", (t) => {
  const release = createReleaseFixture(t);
  const before = release.git("status", "--porcelain");
  const plan = release.plan();
  assert.equal(plan.sourceSha, release.git("rev-parse", "HEAD"));
  assert.deepEqual(Object.keys(plan.runtimes), platforms);
  assert.equal(
    plan.url,
    "https://example.invalid/mash/runtime/mash-cv/code/mash-cv-code-v1.1.0.zip",
  );
  assert.equal(release.git("status", "--porcelain"), before);
  assert.throws(
    () => planRelease("1.0.0", "HEAD", "main", "https://example.invalid"),
    /must increase/,
  );
  release.write("unreviewed.txt", "change");
  release.commit();
  assert.throws(() => release.plan(), /merge-base/);
});

test("plan refuses conflicting source tags and unsafe destinations", (t) => {
  const release = createReleaseFixture(t);
  release.git("tag", "cv-code/1.1.0");
  assert.equal(release.plan().version, "1.1.0");
  release.write("reviewed.txt", "new source");
  release.commit();
  release.git("update-ref", "refs/remotes/origin/main", "HEAD");
  assert.throws(() => release.plan(), /different source/);
  for (const prefix of ["../mash", "/mash", "mash//code", "mash;echo"]) {
    assert.throws(() => location("1.1.0", "https://example.invalid", prefix), /prefix/);
  }
  assert.throws(() => location("1.1.0", "http://example.invalid"), /HTTPS/);
  assert.throws(() => location("1.1.0", "https://secret@example.invalid"), /HTTPS/);
});

test("candidate records the actual bytes and refuses a mismatched build checkout", (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.candidate();
  assert.equal(receipt.sha256, digest("candidate/mash-cv-code-v1.1.0.zip"));
  assert.equal(receipt.size, Buffer.byteLength("candidate bytes"));
  assert.deepEqual(JSON.parse(fs.readFileSync("candidate/release.json")), receipt);
  assert.throws(
    () => recordRelease({ ...receipt, sourceSha: "0".repeat(40) }, "dist/code.zip", "bad"),
    /checkout/,
  );
  assert.throws(() => validateReceipt({ ...receipt, artifact: "../bad.zip" }), /record/);
});

test("both compatibility reports must match this code and its pinned runtimes", (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.candidate();
  assert.throws(() => validateReports(receipt, "reports"), /ENOENT/);
  for (const platform of platforms) {
    release.write(
      `reports/${platform}.json`,
      JSON.stringify({
        platform,
        status: "passed",
        sourceSha: receipt.sourceSha,
        codeSha256: receipt.sha256,
        runtimeSha256: receipt.runtimes[platform].runtimeSha256,
      }),
    );
  }
  validateReports(receipt, "reports");
  const report = JSON.parse(fs.readFileSync("reports/windows-x86_64.json"));
  release.write(
    "reports/windows-x86_64.json",
    JSON.stringify({ ...report, codeSha256: "0".repeat(64) }),
  );
  assert.throws(() => validateReports(receipt, "reports"), /mismatched/);
});

test("manifest application updates shared code, preserves every runtime and is retryable", (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.candidate();
  applyRelease(receipt);
  const manifest = JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json"));
  assert.equal(manifest.mashCvCodeVersion, "1.1.0");
  for (const platform of platforms) {
    assert.deepEqual(manifest.platforms[platform], {
      ...release.manifest.platforms[platform],
      codeUrl: receipt.url,
      codeSha256: receipt.sha256,
    });
  }
  assert.match(fs.readFileSync("versions.toml", "utf8"), /code = "1.1.0"/);
  assert.doesNotThrow(() => applyRelease(receipt));
  assert.throws(() => applyRelease({ ...receipt, sha256: "c".repeat(64) }), /another artifact/);
});

test("manifest application rejects changed source, changed runtime and downgrades", (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.candidate();
  release.write("sidecar/mash_cv/mash_cv/__init__.py", "# newer code\n");
  release.commit();
  assert.throws(() => applyRelease(receipt), /source changed/);
  release.git("checkout", receipt.sourceSha);
  const changed = structuredClone(release.manifest);
  changed.platforms["windows-x86_64"].runtimeSha256 = "c".repeat(64);
  release.write("src-tauri/resources/runtime-manifest.json", JSON.stringify(changed));
  assert.throws(() => applyRelease(receipt), /Runtime changed/);
  release.write("src-tauri/resources/runtime-manifest.json", JSON.stringify(release.manifest));
  assert.throws(
    () =>
      applyRelease({
        ...receipt,
        version: "0.9.0",
        ...location("0.9.0", "https://example.invalid"),
      }),
    /stale/,
  );
});

function fakeStore() {
  const objects = new Map();
  const calls = [];
  const aws = (args) => {
    calls.push([...args]);
    const key = args[args.indexOf("--key") + 1];
    if (args[1] === "put-object") {
      assert.equal(args[args.indexOf("--if-none-match") + 1], "*");
      if (objects.has(key)) {
        throw new Error("An error occurred (412): PreconditionFailed");
      }
      objects.set(key, fs.readFileSync(args[args.indexOf("--body") + 1]));
    } else {
      fs.writeFileSync(args.at(-1), objects.get(key));
    }
  };
  return { objects, calls, options: { aws } };
}

test("conditional upload reuses identical objects and refuses conflicting bytes", async (t) => {
  const release = createReleaseFixture(t);
  release.write("code.zip", "tested ZIP");
  const store = fakeStore();
  t.mock.method(globalThis, "fetch", () => {
    throw new Error("Public downloads are forbidden");
  });
  await publishImmutable("code.zip", "code.zip", store.options);
  await publishImmutable("code.zip", "code.zip", store.options);
  assert.equal(store.calls.filter((args) => args[1] === "get-object").length, 2);
  store.objects.set("code.zip", Buffer.from("another release"));
  await assert.rejects(publishImmutable("code.zip", "code.zip", store.options), /differs/);
  assert.equal(store.objects.get("code.zip").toString(), "another release");
});

test("R2 read failure can retry the original artifact without overwriting it", async (t) => {
  const release = createReleaseFixture(t);
  release.write("code.zip", "tested ZIP");
  const store = fakeStore();
  let temporaryFile;
  await assert.rejects(
    publishImmutable("code.zip", "code.zip", {
      aws: (args) => {
        if (args[1] === "get-object") {
          temporaryFile = args.at(-1);
          fs.writeFileSync(temporaryFile, "partial download");
          throw new Error("Download interrupted");
        }
        return store.options.aws(args);
      },
    }),
    /Download interrupted/,
  );
  assert.equal(fs.existsSync(path.dirname(temporaryFile)), false);
  assert.equal(store.objects.get("code.zip").toString(), "tested ZIP");
  await publishImmutable("code.zip", "code.zip", store.options);
  await assert.rejects(
    publishImmutable("code.zip", "new.zip", {
      ...store.options,
      aws: () => {
        throw new Error("AccessDenied");
      },
    }),
    /AccessDenied/,
  );
});

test("R2 verification rejects incorrect bytes after a successful upload", async (t) => {
  const release = createReleaseFixture(t);
  release.write("code.zip", "tested ZIP");
  const store = fakeStore();
  await assert.rejects(
    publishImmutable("code.zip", "code.zip", {
      aws: (args) => {
        if (args[1] === "get-object") {
          fs.writeFileSync(args.at(-1), "incorrect bytes");
        } else {
          store.options.aws(args);
        }
      },
    }),
    /Immutable object differs/,
  );
});

test("tag creation is retryable and never moves an existing tag", (t) => {
  const release = createReleaseFixture(t);
  const receipt = release.candidate();
  const remote = path.join(release.root, "remote.git");
  release.git("init", "--bare", "--quiet", remote);
  release.git("remote", "add", "origin", remote);
  tagRelease(receipt);
  tagRelease(receipt);
  assert.equal(release.git("rev-parse", "cv-code/1.1.0^{commit}"), receipt.sourceSha);
  release.write("new.txt", "new source");
  release.commit();
  assert.throws(
    () => tagRelease({ ...receipt, sourceSha: release.git("rev-parse", "HEAD") }),
    /different source/,
  );
});
