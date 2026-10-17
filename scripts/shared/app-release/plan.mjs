import { platforms } from "../release-artifacts.mjs";
import { compareVersions } from "../updater-manifest.mjs";
import {
  runtimeManifestPath,
  gitCommand,
  readGitFile,
  hashBytes,
  assertStableVersion,
  assertCommitSha,
  resolveTagCommit,
  objectKeyFromUrl,
  validatePublicUrl,
} from "./common.mjs";

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
  const tree = gitCommand("ls-tree", "-r", ref, "--", "sidecar/cv-code/mash_cv");
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
  if (appVersion !== source.appConfig.version) {
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

function isPublishedRuntime(entry, configuredVersion) {
  if (!entry) {
    return false;
  }
  const validVersion = /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(entry.runtimeVersion);
  const validHash = /^[a-f0-9]{64}$/.test(entry.runtimeSha256) && !/^0+$/.test(entry.runtimeSha256);
  return validVersion && validHash && entry.runtimeVersion === configuredVersion;
}

function inspectRuntimes(source, baseUrl, blockers) {
  const runtimes = {};
  for (const platform of platforms) {
    const entry = source.manifest.platforms?.[platform];
    const configuredVersion = readVersionValue(source.versions, "mash_cv.runtime", platform);
    if (!isPublishedRuntime(entry, configuredVersion)) {
      blockers.push(`Missing or inconsistent published runtime: ${platform}`);
      continue;
    }
    try {
      objectKeyFromUrl(entry.runtimeUrl, baseUrl);
    } catch (error) {
      blockers.push(`${platform}: ${error.message}`);
    }
    runtimes[platform] = {
      runtimeVersion: entry.runtimeVersion,
      runtimeUrl: entry.runtimeUrl,
      runtimeSha256: entry.runtimeSha256,
    };
  }
  return runtimes;
}

// Read Git objects without changing the worktree, even if ref is not HEAD.
export function createPlan({
  ref,
  appVersion,
  branch,
  baseUrl,
  prefix = "mash",
  toolingSha,
  createdAt = new Date().toISOString(),
  executing = false,
}) {
  assertStableVersion(appVersion);
  validatePublicUrl(baseUrl);
  if (!/^[A-Za-z0-9_-]+(?:\/[A-Za-z0-9_-]+)*$/.test(prefix)) {
    throw new Error("Invalid R2 prefix");
  }
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
  const runtimes = inspectRuntimes(source, baseUrl, blockers);
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
    blockers.push(
      `Prepare nonempty release notes in CHANGELOG.md at ${sourceSha}: use "## 未发布" or "## v${appVersion} - YYYY-MM-DD"`,
    );
  }
  return {
    schemaVersion: 2,
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
    code: { artifact: `mash-cv-code-v${appVersion}.zip`, sourceDigest: codeDigest(sourceSha) },
    runtimes,
    ready: blockers.length === 0,
    blockers,
  };
}

export function validatePlan(plan, env = process.env) {
  const isSupported = plan.schemaVersion === 2 && plan.kind === "mash-app-release";
  if (!isSupported || !plan.ready || plan.blockers?.length) {
    throw new Error("Release plan is blocked or unsupported");
  }
  const baseUrl = env.RELEASE_BASE_URL?.replace(/\/+$/, "");
  const prefix = env.R2_PREFIX || "mash";
  if (plan.baseUrl !== baseUrl || plan.prefix !== prefix) {
    throw new Error("Release destination changed since planning");
  }
  const regenerated = createPlan({
    ref: plan.sourceSha,
    appVersion: plan.appVersion,
    branch: plan.branch,
    baseUrl: plan.baseUrl,
    prefix: plan.prefix,
    toolingSha: plan.toolingSha,
    createdAt: plan.createdAt,
    executing: true,
  });
  if (JSON.stringify(regenerated) !== JSON.stringify(plan)) {
    throw new Error(
      `Release plan changed; start a new release run: ${regenerated.blockers.join("; ")}`,
    );
  }
  return plan;
}

export function summary(plan) {
  const status = plan.ready ? "READY" : "BLOCKED";
  const details = [
    `Status: **${status}**`,
    `Source: \`${plan.sourceSha}\``,
    `App: ${plan.appVersion}`,
    "CV code: bundled with the app",
    `Platforms: ${plan.platforms.join(", ")}`,
  ].join("\n\n");
  let nextSteps =
    "The pipeline will validate bundled code against each pinned native runtime before publishing.";
  if (plan.blockers.length) {
    const issues = plan.blockers.map((message) => `- ${message}`).join("\n");
    nextSteps = `Blocking issues:\n\n${issues}`;
  }
  return `# Mash release plan\n\n${details}\n\n${nextSteps}\n`;
}
