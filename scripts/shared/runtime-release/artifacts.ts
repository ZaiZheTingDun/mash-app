import path from "node:path";
import { digest, publishImmutable } from "../release-artifacts.mjs";
import { applyCvArtifact, readVersions, updateVersion } from "../release-state.mjs";
import {
  assertStableVersion,
  createR2Client,
  gitCommand as git,
  objectKeyFromUrl,
  readJson,
  resolveTagCommit,
  runtimeManifestPath,
} from "../app-release/common.mjs";
import { planDigest, sameJson, validateRuntimePlan } from "./plan.ts";
import type {
  R2Client,
  RuntimeArtifact,
  RuntimeBuild,
  RuntimeManifest,
  RuntimeManifestEntry,
  RuntimePlan,
  RuntimePlatform,
  RuntimeReceipt,
  RuntimeReport,
} from "./types.ts";

export function runtimeArtifact(plan: RuntimePlan, platform: RuntimePlatform): RuntimeArtifact {
  if (!plan.platforms.includes(platform)) {
    throw new Error("Unexpected runtime platform");
  }

  const name = `mash-cv-runtime-${platform}-v${plan.version}.zip`;
  const key = `${plan.prefix}/runtime/mash-cv/runtime/${platform}/${name}`;
  const url = `${plan.baseUrl}/${key}`;
  objectKeyFromUrl(url, plan.baseUrl);

  return { name, key, url };
}

function readVerifiedBuild(
  plan: RuntimePlan,
  platform: RuntimePlatform,
  directory: string,
): RuntimeBuild {
  const artifact = runtimeArtifact(plan, platform);
  const reportFile = path.join(directory, platform, "report.json");
  const report = readJson(reportFile) as RuntimeReport;
  const sha256 = digest(path.join(directory, platform, artifact.name));

  if (
    report.status !== "passed" ||
    report.platform !== platform ||
    report.sourceSha !== plan.sourceSha ||
    report.version !== plan.version ||
    report.planSha256 !== planDigest(plan) ||
    report.runtimeSha256 !== sha256
  ) {
    throw new Error(`Runtime verification report mismatch: ${platform}`);
  }

  return { platform, ...artifact, sha256 };
}

export function assembleRuntime(plan: RuntimePlan, directory: string): RuntimeReceipt {
  const builds = plan.platforms.map(platform => readVerifiedBuild(plan, platform, directory));
  return { plan, builds };
}

export async function uploadRuntime(
  plan: RuntimePlan,
  directory: string,
  aws: R2Client = createR2Client(),
): Promise<RuntimeReceipt> {
  const receipt = assembleRuntime(validateRuntimePlan(plan), directory);

  // Validate every selected platform before the first external write.
  for (const build of receipt.builds) {
    const archive = path.join(directory, build.platform, build.name);
    await publishImmutable(archive, build.key, { aws });
  }

  return receipt;
}

export function tagRuntime(plan: RuntimePlan): void {
  validateRuntimePlan(plan);
  const tags = plan.platforms.map(platform => `cv-runtime/${platform}/${plan.version}`);
  for (const tag of tags) {
    if (!resolveTagCommit(tag)) {
      git("tag", tag, plan.sourceSha);
    }
  }

  const references = tags.map(tag => `refs/tags/${tag}`);
  git("push", "--atomic", "origin", ...references);
}

function validateReceiptPlatforms({ plan, builds }: RuntimeReceipt): void {
  const uniquePlatforms = new Set(builds.map(build => build.platform));
  if (builds.length !== plan.platforms.length || uniquePlatforms.size !== plan.platforms.length) {
    throw new Error("Invalid runtime receipt platforms");
  }
}

function validateSyncPlatform(
  plan: RuntimePlan,
  platform: RuntimePlatform,
  build: RuntimeBuild | undefined,
  currentVersion: string,
  currentManifest: RuntimeManifestEntry,
): void {
  const artifact = runtimeArtifact(plan, platform);
  if (
    !build ||
    build.name !== artifact.name ||
    build.url !== artifact.url ||
    !/^[a-f0-9]{64}$/.test(build.sha256)
  ) {
    throw new Error("Invalid runtime receipt");
  }

  const alreadyApplied =
    currentVersion === plan.version &&
    currentManifest.runtimeVersion === plan.version &&
    currentManifest.runtimeUrl === build.url &&
    currentManifest.runtimeSha256 === build.sha256;
  if (alreadyApplied) {
    return;
  }

  const previous = plan.previous[platform];
  if (currentVersion !== previous.version || !sameJson(currentManifest, previous.manifest)) {
    throw new Error(
      `Runtime metadata changed on the default branch: ${platform}; synchronize manually`,
    );
  }
}

export function syncRuntime(receipt: RuntimeReceipt): void {
  const { plan, builds } = receipt;
  assertStableVersion(plan.version);
  validateReceiptPlatforms(receipt);

  const versions = readVersions(undefined);
  const manifest = readJson(runtimeManifestPath) as RuntimeManifest;

  // Validate all updates before writing either metadata file. Preserve unrelated
  // platform and app changes that landed on the default branch during the build.
  for (const platform of plan.platforms) {
    const build = builds.find(value => value.platform === platform);
    validateSyncPlatform(
      plan,
      platform,
      build,
      versions.runtimes[platform],
      manifest.platforms[platform],
    );
  }

  for (const build of builds) {
    updateVersion("runtime", plan.version, build.platform);
    applyCvArtifact("runtime", plan.version, build.platform, build.url, build.sha256);
  }
}
