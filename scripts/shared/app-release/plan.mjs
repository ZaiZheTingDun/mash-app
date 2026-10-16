import { location, platforms } from "../cv-code-release.mjs";
import { compareVersions } from "../updater-manifest.mjs";
import {
  runtimeManifestPath,
  gitCommand,
  readGitFile,
  hashBytes,
  assertStableVersion,
  assertCommitSha,
  resolveTagCommit,
  listChangedFiles,
  objectKeyFromUrl,
} from "./common.mjs";

const runtimeInputPaths = [
  "sidecar/mash_cv/pyproject.toml",
  "sidecar/mash_cv/poetry.lock",
  "sidecar/mash_cv/runtime_launcher.py",
  "sidecar/mash_cv/build_sidecar.py",
  "sidecar/mash_cv/mash_cv/models",
  "src-tauri/resources/scrcpy",
];

export function planDigest(plan) {
  return hashBytes(JSON.stringify(plan));
}

function isPackagedCodeEntry(treeEntry) {
  const file = treeEntry.split("\t")[1];
  if (!file) {
    return false;
  }
  const isExcludedDirectory = file.split("/").some((part) => {
    return part === "models" || part === "__pycache__";
  });
  return !isExcludedDirectory && !/\.py[co]$/.test(file);
}

export function codeDigest(ref) {
  const tree = gitCommand("ls-tree", "-r", ref, "--", "sidecar/mash_cv/mash_cv");
  const entries = tree.split("\n").filter(isPackagedCodeEntry);
  return hashBytes(entries.join("\n"));
}

export function releaseNotes(changelog, version) {
  const lines = changelog.split(/\r?\n/);
  let start = lines.findIndex((line) => line.startsWith(`## v${version} - `));
  if (start < 0) {
    start = lines.findIndex((line) => line === "## 未发布");
  }
  if (start < 0) {
    return "";
  }
  let end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  if (end < 0) {
    end = lines.length;
  }
  return lines
    .slice(start + 1, end)
    .join("\n")
    .trim();
}

function readVersionValue(toml, section, key) {
  const sectionBody = toml.split(`[${section}]`)[1]?.split(/\r?\n\[/)[0];
  const escapedKey = key.replaceAll(".", "\\.");
  const assignment = new RegExp(`^${escapedKey}\\s*=\\s*"([^"]+)"`, "m");
  return sectionBody?.match(assignment)?.[1];
}

function readReleaseSource(sourceSha) {
  const cargoToml = readGitFile(sourceSha, "src-tauri/Cargo.toml");
  const cargoLock = readGitFile(sourceSha, "src-tauri/Cargo.lock");
  return {
    manifest: JSON.parse(readGitFile(sourceSha, runtimeManifestPath)),
    versions: readGitFile(sourceSha, "versions.toml"),
    appConfig: JSON.parse(readGitFile(sourceSha, "src-tauri/tauri.conf.json")),
    windowsConfig: JSON.parse(readGitFile(sourceSha, "src-tauri/tauri.windows.conf.json")),
    cargoVersion: cargoToml.match(/\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1],
    lockVersion: cargoLock.match(/\[\[package\]\]\r?\nname = "mash"\r?\nversion = "([^"]+)"/)?.[1],
    changelog: readGitFile(sourceSha, "CHANGELOG.md"),
  };
}

function checkSourceVersions(source, blockers) {
  const appVersion = readVersionValue(source.versions, "app", "version");
  const codeVersion = readVersionValue(source.versions, "mash_cv", "code");
  if (
    appVersion !== source.appConfig.version ||
    codeVersion !== source.manifest.mashCvCodeVersion
  ) {
    blockers.push("Source versions and manifests are inconsistent");
  }
  if (
    source.cargoVersion !== source.appConfig.version ||
    source.lockVersion !== source.appConfig.version
  ) {
    blockers.push("Cargo versions do not match the app config");
  }
}

function checkAppVersion(appVersion, sourceVersion, executing, blockers) {
  if (compareVersions(appVersion, sourceVersion) <= 0) {
    blockers.push("App version must increase relative to the selected source");
  }
  if (resolveTagCommit(`v${appVersion}`) && !executing) {
    blockers.push(`App tag v${appVersion} already exists`);
  }
  const appTags = gitCommand("tag", "--list", "v*").split("\n");
  for (const tag of appTags.filter((tag) => /^v\d+\.\d+\.\d+$/.test(tag))) {
    if (compareVersions(tag.slice(1), appVersion) > 0) {
      blockers.push(`Newer app tag already exists: ${tag}`);
    }
  }
}

function inspectCvCode(sourceSha, currentVersion, requestedVersion, executing, blockers) {
  const baselineSha = resolveTagCommit(`cv-code/${currentVersion}`);
  if (!baselineSha) {
    blockers.push(`Missing source tag for referenced CV code ${currentVersion}`);
  }
  const sourceDigest = codeDigest(sourceSha);
  const changed = !baselineSha || codeDigest(baselineSha) !== sourceDigest;
  const testChanges = baselineSha
    ? listChangedFiles(baselineSha, sourceSha, ["sidecar/mash_cv/tests"])
    : [];
  if (changed && !requestedVersion) {
    blockers.push("CV code changed; provide a new cv_code_version");
  }
  if (requestedVersion && !changed) {
    blockers.push("CV code is unchanged; omit cv_code_version");
  }
  if (requestedVersion && compareVersions(requestedVersion, currentVersion) <= 0) {
    blockers.push("CV code version must increase");
  }
  const existingTag = requestedVersion ? resolveTagCommit(`cv-code/${requestedVersion}`) : null;
  const isSameReleaseRetry = executing && existingTag === sourceSha;
  if (existingTag && !isSameReleaseRetry) {
    blockers.push("Target CV code tag already exists");
  }
  return {
    changed,
    version: requestedVersion || currentVersion,
    baselineVersion: currentVersion,
    baselineSha,
    sourceDigest,
    testChanges,
  };
}

function isPublishedRuntime(entry, configuredVersion) {
  if (!entry) {
    return false;
  }
  const validVersion = /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(entry.runtimeVersion);
  const validHash = /^[a-f0-9]{64}$/.test(entry.runtimeSha256) && !/^0+$/.test(entry.runtimeSha256);
  return validVersion && validHash && entry.runtimeVersion === configuredVersion;
}

function runtimeBaseline(platform, version, sourceSha) {
  const taggedCommit = resolveTagCommit(`cv-runtime/${platform}/${version}`);
  if (taggedCommit) {
    return taggedCommit;
  }
  // Historical runtimes predate the tags. Use the commit that introduced this
  // platform's configured version, rather than another platform's latest tag.
  return gitCommand(
    "log",
    sourceSha,
    "-1",
    "--format=%H",
    "-S",
    `${platform} = "${version}"`,
    "--",
    "versions.toml",
  );
}

function inspectRuntimes(source, sourceSha, baseUrl, blockers) {
  const runtimes = {};
  const runtimeChanges = {};
  for (const platform of platforms) {
    const entry = source.manifest.platforms?.[platform];
    const configuredVersion = readVersionValue(source.versions, "mash_cv.runtime", platform);
    if (!isPublishedRuntime(entry, configuredVersion)) {
      blockers.push(`Missing or inconsistent published runtime: ${platform}`);
      continue;
    }
    try {
      objectKeyFromUrl(entry.runtimeUrl, baseUrl);
      objectKeyFromUrl(entry.codeUrl, baseUrl);
    } catch (error) {
      blockers.push(`${platform}: ${error.message}`);
    }
    if (!/^[a-f0-9]{64}$/.test(entry.codeSha256) || /^0+$/.test(entry.codeSha256)) {
      blockers.push(`Invalid CV code hash: ${platform}`);
    }
    runtimes[platform] = {
      runtimeVersion: entry.runtimeVersion,
      runtimeUrl: entry.runtimeUrl,
      runtimeSha256: entry.runtimeSha256,
    };
    const baselineSha = runtimeBaseline(platform, entry.runtimeVersion, sourceSha);
    const nativeBuildScript =
      platform === "windows-x86_64" ? "scripts/windows/build-cv.ps1" : "scripts/macos/build-cv.sh";
    const files = baselineSha
      ? listChangedFiles(baselineSha, sourceSha, [...runtimeInputPaths, nativeBuildScript])
      : [];
    runtimeChanges[platform] = { sourceSha: baselineSha || null, files };
    if (!baselineSha) {
      blockers.push(`Cannot determine runtime baseline: ${platform}`);
    } else if (files.length) {
      blockers.push(`Runtime inputs changed for ${platform}; publish and apply its runtime first`);
    }
  }
  return { runtimes, runtimeChanges };
}

function inspectSharedCodeArtifact(manifest, baseUrl, blockers) {
  const entries = platforms.map((platform) => manifest.platforms?.[platform]);
  const sharedEntry = entries[0];
  const artifactsDiffer = entries.some((entry) => {
    return entry?.codeUrl !== sharedEntry?.codeUrl || entry?.codeSha256 !== sharedEntry?.codeSha256;
  });
  if (artifactsDiffer) {
    blockers.push("Platforms reference different shared CV code artifacts");
  }
  let objectKey = null;
  try {
    objectKey = objectKeyFromUrl(sharedEntry?.codeUrl, baseUrl);
  } catch {
    // inspectRuntimes already reports invalid artifact URLs for each platform.
  }
  return {
    artifact: `mash-cv-code-v${manifest.mashCvCodeVersion}.zip`,
    url: sharedEntry?.codeUrl,
    objectKey,
    sha256: sharedEntry?.codeSha256,
  };
}

// Read Git objects without changing the worktree, even if ref is not HEAD.
export function createPlan({
  ref,
  appVersion,
  codeVersion = "",
  branch,
  baseUrl,
  prefix = "mash",
  toolingSha,
  createdAt = new Date().toISOString(),
  executing = false,
}) {
  assertStableVersion(appVersion);
  if (codeVersion) {
    assertStableVersion(codeVersion);
  }
  location("0.0.0", baseUrl, prefix); // Validate the shared publication destination.
  assertCommitSha(toolingSha);
  if (new Date(createdAt).toISOString() !== createdAt) {
    throw new Error("Invalid plan date");
  }
  const sourceSha = assertCommitSha(
    gitCommand("rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`),
  );
  gitCommand("merge-base", "--is-ancestor", sourceSha, `refs/remotes/origin/${branch}`);
  const source = readReleaseSource(sourceSha);
  const blockers = [];
  checkSourceVersions(source, blockers);
  checkAppVersion(appVersion, source.appConfig.version, executing, blockers);
  const code = inspectCvCode(
    sourceSha,
    source.manifest.mashCvCodeVersion,
    codeVersion,
    executing,
    blockers,
  );
  const { runtimes, runtimeChanges } = inspectRuntimes(source, sourceSha, baseUrl, blockers);
  const sharedArtifact = inspectSharedCodeArtifact(source.manifest, baseUrl, blockers);
  const codeArtifact =
    code.changed && codeVersion ? location(codeVersion, baseUrl, prefix) : sharedArtifact;
  const publicKeys = {
    "windows-x86_64": source.windowsConfig.plugins?.updater?.pubkey,
    "darwin-aarch64": source.appConfig.plugins?.updater?.pubkey,
  };
  if (!/^[A-Za-z][A-Za-z0-9 ._-]*$/.test(source.appConfig.productName)) {
    blockers.push("Unsupported product name");
  }
  if (!publicKeys["windows-x86_64"] || !publicKeys["darwin-aarch64"]) {
    blockers.push("Both platform updater public keys are required");
  }
  if (!releaseNotes(source.changelog, appVersion)) {
    blockers.push("Prepare a nonempty unreleased or target-version changelog section first");
  }
  return {
    schemaVersion: 1,
    kind: "mash-app-release",
    createdAt,
    toolingSha,
    sourceSha,
    branch,
    baseUrl: baseUrl.replace(/\/+$/, ""),
    prefix,
    appVersion,
    productName: source.appConfig.productName,
    platforms,
    publicKeys,
    code: { ...code, ...codeArtifact },
    runtimes,
    runtimeChanges,
    ready: blockers.length === 0,
    blockers,
  };
}

export function validatePlan(plan, env = process.env) {
  const isSupported = plan.schemaVersion === 1 && plan.kind === "mash-app-release";
  if (!isSupported || !plan.ready || plan.blockers?.length) {
    throw new Error("Release plan is blocked or unsupported");
  }
  const baseUrl = env.RELEASE_BASE_URL?.replace(/\/+$/, "");
  const prefix = env.R2_PREFIX || "mash";
  if (plan.baseUrl !== baseUrl || plan.prefix !== prefix) {
    throw new Error("Release destination changed since preflight");
  }
  const regenerated = createPlan({
    ref: plan.sourceSha,
    appVersion: plan.appVersion,
    codeVersion: plan.code.changed ? plan.code.version : "",
    branch: plan.branch,
    baseUrl: plan.baseUrl,
    prefix: plan.prefix,
    toolingSha: plan.toolingSha,
    createdAt: plan.createdAt,
    executing: true,
  });
  if (JSON.stringify(regenerated) !== JSON.stringify(plan)) {
    throw new Error(
      `Release plan changed; run preflight again: ${regenerated.blockers.join("; ")}`,
    );
  }
  return plan;
}

export function validatePreflightRun(plan, metadata, repository, branch) {
  const isSuccessfulPreflight =
    metadata.path === ".github/workflows/release-preflight.yml" &&
    metadata.event === "workflow_dispatch" &&
    metadata.conclusion === "success" &&
    metadata.status === "completed";
  const isSameRepository =
    metadata.repository?.full_name === repository &&
    metadata.head_repository?.full_name === repository;
  const isDefaultBranch = metadata.head_branch === branch && plan.branch === branch;
  const hasExpectedTooling = metadata.head_sha === plan.toolingSha;
  if (!isSuccessfulPreflight || !isSameRepository || !isDefaultBranch || !hasExpectedTooling) {
    throw new Error("Plan must come from a successful default-branch preflight in this repository");
  }
}

export function summary(plan) {
  const status = plan.ready ? "READY" : "BLOCKED";
  const codeRelease = plan.code.changed
    ? `joint release ${plan.code.version}`
    : `reuse ${plan.code.version}`;
  const details = [
    `Status: **${status}**`,
    `Source: \`${plan.sourceSha}\``,
    `App: ${plan.appVersion}`,
    `CV code: ${codeRelease}`,
    `Platforms: ${plan.platforms.join(", ")}`,
    `CV baseline: \`${plan.code.baselineSha ?? "unknown"}\` (${plan.code.baselineVersion})`,
    `CV test changes: ${plan.code.testChanges.length}`,
  ].join("\n\n");
  let nextSteps =
    "Pass this preflight run ID to App release. No source, tags or published objects were changed.";
  if (plan.blockers.length) {
    const issues = plan.blockers.map((message) => `- ${message}`).join("\n");
    nextSteps = `Blocking issues:\n\n${issues}`;
  }
  return `# Mash release preflight\n\n${details}\n\n${nextSteps}\n`;
}
