import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { digest, platforms } from "../release-artifacts.mjs";
import {
  finalizeChangelog,
  readVersions,
  updateVersion,
} from "../release-state.mjs";
import { compareVersions } from "../updater-manifest.mjs";
import { selectUpdaterArtifact } from "../updater-artifact.mjs";
import {
  runtimeManifestPath,
  metadataFiles,
  readJson,
  writeJson,
  runCommand,
  gitCommand,
  resolveTagCommit,
} from "./common.mjs";
import { planDigest, codeDigest } from "./plan.mjs";

export function prepareCodeArchive(plan, directory) {
  fs.mkdirSync(directory, { recursive: true });
  if (gitCommand("rev-parse", "HEAD") !== plan.sourceSha) {
    throw new Error("Code checkout differs from plan");
  }
  runCommand("python", ["build_code.py"], {
    cwd: "sidecar/cv-code",
    env: { ...process.env, MASH_APP_VERSION: plan.appVersion },
  });
  const archive = path.join("sidecar/cv-code/dist", plan.code.artifact);
  fs.copyFileSync(archive, path.join(directory, plan.code.artifact));
}

function applyReleaseMetadata(plan) {
  updateVersion("app", plan.appVersion);
  const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
  const hasVersionHeading = changelog
    .split(/\r?\n/)
    .some((line) => line.startsWith(`## v${plan.appVersion} - `));
  if (!hasVersionHeading) {
    finalizeChangelog(plan.appVersion, plan.createdAt.slice(0, 10));
  }
}

function createReleaseCommit(plan) {
  gitCommand("add", "--", ...metadataFiles);
  const tree = gitCommand("write-tree");
  // Stable identity and dates let a retry reproduce the same release commit.
  const botName = "github-actions[bot]";
  const botEmail = "41898282+github-actions[bot]@users.noreply.github.com";
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: botName,
    GIT_AUTHOR_EMAIL: botEmail,
    GIT_COMMITTER_NAME: botName,
    GIT_COMMITTER_EMAIL: botEmail,
    GIT_AUTHOR_DATE: plan.createdAt,
    GIT_COMMITTER_DATE: plan.createdAt,
  };
  const releaseSha = runCommand(
    "git",
    [
      "-c",
      "commit.gpgsign=false",
      "commit-tree",
      tree,
      "-p",
      plan.sourceSha,
      "-m",
      `chore(release): prepare app ${plan.appVersion}`,
    ],
    { env },
  );
  const existingTag = resolveTagCommit(`v${plan.appVersion}`);
  if (existingTag && existingTag !== releaseSha) {
    throw new Error("App tag belongs to another release snapshot");
  }
  return releaseSha;
}

function writeSourceBundle(plan, releaseSha, directory) {
  gitCommand("update-ref", "refs/heads/release-candidate", releaseSha);
  gitCommand(
    "bundle",
    "create",
    path.resolve(directory, "source.bundle"),
    "refs/heads/release-candidate",
    `^${plan.sourceSha}`,
  );
  gitCommand("checkout", "--detach", releaseSha);
}

export function prepareSource(plan, directory) {
  const isPlannedCheckout = gitCommand("rev-parse", "HEAD") === plan.sourceSha;
  if (!isPlannedCheckout || gitCommand("status", "--porcelain")) {
    throw new Error("Prepare requires a clean checkout at the planned SHA");
  }
  const archive = path.join(directory, plan.code.artifact);
  const codeReceipt = {
    schemaVersion: 1,
    version: plan.appVersion,
    sourceSha: plan.sourceSha,
    artifact: plan.code.artifact,
    sha256: digest(archive),
    size: fs.statSync(archive).size,
    runtimes: plan.runtimes,
  };
  writeJson(path.join(directory, "release.json"), codeReceipt);
  applyReleaseMetadata(plan);
  const releaseSha = createReleaseCommit(plan);
  writeSourceBundle(plan, releaseSha, directory);
  const prepared = {
    plan,
    planSha256: planDigest(plan),
    releaseSha,
    code: codeReceipt,
    manifestSha256: digest(runtimeManifestPath),
    manifest: readJson(runtimeManifestPath),
  };
  writeJson(path.join(directory, "prepared.json"), prepared);
  return prepared;
}

function expectedPlatformFiles(productName, version, platform) {
  const isWindows = platform === "windows-x86_64";
  const updaterName = isWindows
    ? `${productName}_${version}_x64-setup.exe`
    : `${productName}.app.tar.gz`;
  const updaterRoles = isWindows ? ["updater", "installer"] : ["updater"];
  const files = [
    { name: updaterName, roles: updaterRoles },
    { name: `${updaterName}.sig`, roles: ["signature"] },
  ];
  if (!isWindows) {
    files.push({ name: `${productName}_${version}_aarch64.dmg`, roles: ["installer"] });
  }
  return files;
}

function fileRecord(file, roles) {
  if (!fs.statSync(file).isFile()) {
    throw new Error(`Missing build file: ${file}`);
  }
  return { name: path.basename(file), sha256: digest(file), size: fs.statSync(file).size, roles };
}

function validateCompatibilityReport(prepared, platform, report, message) {
  const matchesCandidate =
    report.status === "passed" &&
    report.platform === platform &&
    report.sourceSha === prepared.code.sourceSha &&
    report.codeSha256 === prepared.code.sha256;
  const matchesRuntime = report.runtimeSha256 === prepared.plan.runtimes[platform].runtimeSha256;
  if (!matchesCandidate || !matchesRuntime) {
    throw new Error(message);
  }
}

export function recordPlatform(prepared, platform, directory, reportFile) {
  if (!platforms.includes(platform)) {
    throw new Error(`Unsupported build platform: ${platform}`);
  }
  const sourceSha = gitCommand("rev-parse", "HEAD");
  if (sourceSha !== prepared.releaseSha) {
    throw new Error(`Build source differs from the release snapshot: expected ${prepared.releaseSha}, got ${sourceSha}`);
  }
  const manifestSha256 = digest(runtimeManifestPath);
  if (manifestSha256 !== prepared.manifestSha256) {
    throw new Error(`Embedded CV manifest differs from the release snapshot: expected ${prepared.manifestSha256}, got ${manifestSha256}`);
  }
  const report = readJson(reportFile);
  validateCompatibilityReport(
    prepared,
    platform,
    report,
    "Compatibility report does not match the candidate",
  );
  const config = readJson("src-tauri/tauri.conf.json");
  const updater = selectUpdaterArtifact("src-tauri/target/release/bundle", config, platform);
  const expectedFiles = expectedPlatformFiles(config.productName, config.version, platform);
  fs.mkdirSync(directory, { recursive: true });
  const files = expectedFiles.map((expected) => {
    let file = path.join(path.dirname(updater), expected.name);
    if (expected.name.endsWith(".dmg")) {
      file = path.resolve("src-tauri/target/release/bundle/dmg", expected.name);
    }
    const record = fileRecord(file, expected.roles);
    fs.copyFileSync(file, path.join(directory, record.name));
    return record;
  });
  const receipt = {
    platform,
    releaseSha: prepared.releaseSha,
    planSha256: prepared.planSha256,
    manifestSha256: prepared.manifestSha256,
    compatibility: report,
    files,
  };
  writeJson(path.join(directory, "platform.json"), receipt);
  return receipt;
}

export function verifySignature(file, signatureFile, encodedPublicKey) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "mash-signature-"));
  try {
    const publicKeyPath = path.join(directory, "public.key");
    const signaturePath = path.join(directory, "artifact.sig");
    const encodedSignature = fs.readFileSync(signatureFile, "utf8").trim();
    fs.writeFileSync(publicKeyPath, Buffer.from(encodedPublicKey, "base64"));
    fs.writeFileSync(signaturePath, Buffer.from(encodedSignature, "base64"));
    runCommand("minisign", ["-Vm", file, "-p", publicKeyPath, "-x", signaturePath]);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

export function validateFile(file, entry) {
  const safeName =
    entry.name && entry.name === path.basename(entry.name) && !/[\\/]/.test(entry.name);
  const validHash = /^[a-f0-9]{64}$/.test(entry.sha256);
  const validSize = Number.isSafeInteger(entry.size) && entry.size > 0;
  if (!safeName || !validHash || !validSize) {
    throw new Error(`Candidate file changed: ${entry.name}`);
  }
  if (digest(file) !== entry.sha256 || fs.statSync(file).size !== entry.size) {
    throw new Error(`Candidate file changed: ${entry.name}`);
  }
}

function validatePreparedSource(prepared) {
  const matchesPlan = planDigest(prepared.plan) === prepared.planSha256;
  const matchesCommit = gitCommand("rev-parse", "HEAD") === prepared.releaseSha;
  const matchesManifest =
    digest(runtimeManifestPath) === prepared.manifestSha256 &&
    JSON.stringify(readJson(runtimeManifestPath)) === JSON.stringify(prepared.manifest);
  if (!matchesPlan || !matchesCommit || !matchesManifest) {
    throw new Error("Release snapshot mismatch");
  }
}

function validateCodeDependency(prepared, directory) {
  const { code, plan, manifest } = prepared;
  const matchesSource =
    code.sourceSha === plan.sourceSha && codeDigest("HEAD") === plan.code.sourceDigest;
  const matchesRuntime = JSON.stringify(code.runtimes) === JSON.stringify(plan.runtimes);
  const matchesArtifact = code.artifact === plan.code.artifact && code.version === plan.appVersion;
  if (!matchesArtifact || !matchesSource || !matchesRuntime) {
    throw new Error("Bundled CV code differs from plan");
  }
  for (const platform of platforms) {
    const embedded = manifest.platforms[platform];
    if (!Object.entries(plan.runtimes[platform]).every(([key, value]) => embedded[key] === value)) {
      throw new Error("Embedded runtime manifest differs from tested dependencies");
    }
  }
  validateFile(path.join(directory, code.artifact), {
    name: code.artifact,
    sha256: code.sha256,
    size: code.size,
  });
}

function validatePlatformFiles(plan, platform, receipt, directory) {
  const expected = expectedPlatformFiles(plan.productName, plan.appVersion, platform);
  const actualNames = receipt.files.map((file) => file.name).sort();
  const expectedNames = expected.map((file) => file.name).sort();
  if (JSON.stringify(actualNames) !== JSON.stringify(expectedNames)) {
    throw new Error("Missing or unexpected platform files");
  }
  for (const file of receipt.files) {
    const expectedRoles = expected.find((expectedFile) => expectedFile.name === file.name).roles;
    if (JSON.stringify(file.roles) !== JSON.stringify(expectedRoles)) {
      throw new Error("Unexpected artifact role");
    }
    validateFile(path.join(directory, file.name), file);
  }
  return expected[0].name;
}

function copyPlatformFiles(receipt, sourceDirectory, outputDirectory) {
  fs.mkdirSync(outputDirectory, { recursive: true });
  const fileNames = [...receipt.files.map((file) => file.name), "platform.json"];
  for (const name of fileNames) {
    const source = path.join(sourceDirectory, name);
    const destination = path.join(outputDirectory, name);
    if (path.resolve(source) !== path.resolve(destination)) {
      fs.copyFileSync(source, destination);
    }
  }
}

function assemblePlatform(prepared, platform, buildsDirectory, outputDirectory, signatureVerifier) {
  const directory = path.join(buildsDirectory, platform);
  const receipt = readJson(path.join(directory, "platform.json"));
  const matchesSnapshot =
    receipt.platform === platform &&
    receipt.planSha256 === prepared.planSha256 &&
    receipt.releaseSha === prepared.releaseSha &&
    receipt.manifestSha256 === prepared.manifestSha256;
  if (!matchesSnapshot) {
    throw new Error(`Mismatched build receipt: ${platform}`);
  }
  validateCompatibilityReport(
    prepared,
    platform,
    receipt.compatibility,
    `Mismatched compatibility report: ${platform}`,
  );
  const updaterName = validatePlatformFiles(prepared.plan, platform, receipt, directory);
  signatureVerifier(
    path.join(directory, updaterName),
    path.join(directory, `${updaterName}.sig`),
    prepared.plan.publicKeys[platform],
  );
  copyPlatformFiles(receipt, directory, path.join(outputDirectory, platform));
  return receipt;
}

export function assemble(prepared, builds, directory, signatureVerifier = verifySignature) {
  validatePreparedSource(prepared);
  validateCodeDependency(prepared, directory);
  const receipts = platforms.map((platform) =>
    assemblePlatform(prepared, platform, builds, directory, signatureVerifier),
  );
  const candidate = { ...prepared, builds: receipts };
  writeJson(path.join(directory, "app-release.json"), candidate);
  return candidate;
}

export function syncMetadata(receipt) {
  const versions = readVersions();
  if (compareVersions(versions.app, receipt.plan.appVersion) > 0) {
    throw new Error("Default branch has a newer app version; sync manually");
  }
  const alreadySynchronized =
    versions.app === receipt.plan.appVersion &&
    digest(runtimeManifestPath) === receipt.manifestSha256;
  if (alreadySynchronized) {
    return false;
  }
  // Git leaves conflicting changes for review instead of overwriting main.
  gitCommand("cherry-pick", receipt.releaseSha);
  return true;
}
