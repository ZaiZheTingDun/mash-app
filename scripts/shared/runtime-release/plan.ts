import { platforms } from "../release-artifacts.mjs";
import { compareVersions } from "../updater-manifest.mjs";
import {
  assertCommitSha,
  assertStableVersion,
  gitCommand as git,
  hashBytes,
  readGitFile,
  resolveTagCommit,
  runtimeManifestPath,
  validatePublicUrl,
} from "../app-release/common.mjs";
import type {
  RuntimeManifest,
  RuntimePlan,
  RuntimePlanOptions,
  RuntimePlatform,
} from "./types.ts";

export const runtimeRunners: Record<RuntimePlatform, string> = {
  "windows-x86_64": "windows-2022",
  "darwin-aarch64": "macos-15",
};

interface RuntimeSource {
  manifest: RuntimeManifest;
  versionsSection: string | undefined;
}

export function sameJson(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function planDigest(plan: RuntimePlan): string {
  return hashBytes(JSON.stringify(plan));
}

function selectPlatforms(selection: string): RuntimePlatform[] {
  const selected = selection === "all" ? platforms : [selection];

  return selected.map(value => {
    if (!Object.hasOwn(runtimeRunners, value) || !platforms.includes(value)) {
      throw new Error("Unsupported runtime platform");
    }
    return value as RuntimePlatform;
  });
}

function validateDestination(baseUrl: string, prefix: string): void {
  validatePublicUrl(baseUrl);
  if (!/^[A-Za-z0-9_-]+(?:\/[A-Za-z0-9_-]+)*$/.test(prefix)) {
    throw new Error("Invalid R2 prefix");
  }
}

function readRuntimeSource(sourceSha: string): RuntimeSource {
  const manifest: RuntimeManifest = JSON.parse(readGitFile(sourceSha, runtimeManifestPath));
  const versions = readGitFile(sourceSha, "versions.toml");
  const versionsSection = versions.split("[mash_cv.runtime]")[1]?.split(/\r?\n\[/)[0];

  return { manifest, versionsSection };
}

function sourceRuntimeVersion(source: RuntimeSource, platform: RuntimePlatform): string | undefined {
  const assignment = new RegExp(`^${platform}\\s*=\\s*"([^"]+)"`, "m");
  return source.versionsSection?.match(assignment)?.[1];
}

function assertVersionIncreases(version: string, current: string, message: string): void {
  // Date-based runtime versions predate semver and may migrate to any new semver.
  if (/^\d+\.\d+\.\d+$/.test(current) && compareVersions(version, current) <= 0) {
    throw new Error(message);
  }
}

function validateRuntimeTags(
  platform: RuntimePlatform,
  version: string,
  retrySourceSha?: string,
): void {
  const tag = `cv-runtime/${platform}/${version}`;
  const existing = resolveTagCommit(tag);

  if (existing && !retrySourceSha) {
    throw new Error(`Tag already exists: ${tag}`);
  }
  if (existing && existing !== retrySourceSha) {
    throw new Error(`Refusing to move tag ${tag}`);
  }

  const publishedTags = git("tag", "--list", `cv-runtime/${platform}/*`).split("\n");
  for (const published of publishedTags) {
    const publishedVersion = published.split("/").at(-1) ?? "";
    if (!/^\d+\.\d+\.\d+$/.test(publishedVersion)) {
      continue;
    }

    const comparison = compareVersions(publishedVersion, version);
    const blocksRelease = comparison > 0 || (comparison === 0 && !retrySourceSha);
    if (blocksRelease) {
      throw new Error(`Newer runtime tag exists: ${published}`);
    }
  }
}

export function createRuntimePlan(options: RuntimePlanOptions): RuntimePlan {
  const { ref, version, selection, branch, toolingSha, baseUrl, prefix = "mash" } = options;
  assertStableVersion(version);
  assertCommitSha(toolingSha);
  validateDestination(baseUrl, prefix);

  const selected = selectPlatforms(selection);
  const sourceSha = assertCommitSha(
    git("rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`),
  );
  git("merge-base", "--is-ancestor", sourceSha, `refs/remotes/origin/${branch}`);

  const source = readRuntimeSource(sourceSha);
  const previous: RuntimePlan["previous"] = {};
  for (const platform of selected) {
    const current = sourceRuntimeVersion(source, platform);
    const manifest = source.manifest.platforms[platform];
    if (!current || current !== manifest?.runtimeVersion) {
      throw new Error(`Inconsistent runtime version: ${platform}`);
    }

    assertVersionIncreases(version, current, `Runtime version must increase: ${platform}`);
    validateRuntimeTags(platform, version);
    previous[platform] = { version: current, manifest };
  }

  return {
    schemaVersion: 1,
    sourceSha,
    toolingSha,
    branch,
    version,
    platforms: selected,
    baseUrl: baseUrl.replace(/\/+$/, ""),
    prefix,
    previous,
  };
}

function validatePlanPlatforms(selected: RuntimePlatform[]): void {
  if (
    !Array.isArray(selected) ||
    selected.length === 0 ||
    new Set(selected).size !== selected.length ||
    !selected.every(platform => platforms.includes(platform))
  ) {
    throw new Error("Invalid plan platforms");
  }
}

function validatePlanSource(plan: RuntimePlan): void {
  git("merge-base", "--is-ancestor", plan.sourceSha, `refs/remotes/origin/${plan.branch}`);
  const source = readRuntimeSource(plan.sourceSha);

  for (const platform of plan.platforms) {
    const expected = plan.previous[platform];
    const manifest = source.manifest.platforms[platform];
    if (!sameJson(manifest, expected.manifest)) {
      throw new Error("Runtime source manifest changed");
    }

    const current = sourceRuntimeVersion(source, platform);
    if (!current || current !== expected.version || current !== manifest.runtimeVersion) {
      throw new Error("Runtime source version changed");
    }

    assertVersionIncreases(plan.version, current, "Runtime version must increase");
    validateRuntimeTags(platform, plan.version, plan.sourceSha);
  }
}

export function validateRuntimePlan(plan: RuntimePlan): RuntimePlan {
  assertCommitSha(plan.sourceSha);
  assertCommitSha(plan.toolingSha);
  assertStableVersion(plan.version);
  validateDestination(plan.baseUrl, plan.prefix);
  validatePlanPlatforms(plan.platforms);
  validatePlanSource(plan);

  // Preserve the serialized shape and field order shared with Python's report digest.
  const expected: RuntimePlan = {
    schemaVersion: 1,
    sourceSha: plan.sourceSha,
    toolingSha: plan.toolingSha,
    branch: plan.branch,
    version: plan.version,
    platforms: plan.platforms,
    baseUrl: plan.baseUrl,
    prefix: plan.prefix,
    previous: plan.previous,
  };
  if (!sameJson(expected, plan)) {
    throw new Error("Runtime release plan changed");
  }

  return plan;
}
