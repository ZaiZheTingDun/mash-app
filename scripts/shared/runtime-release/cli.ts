import fs from "node:fs";
import path from "node:path";
import { readJson, writeJson } from "../app-release/common.mjs";
import { tagRuntime, uploadRuntime, syncRuntime } from "./artifacts.ts";
import { createRuntimePlan, runtimeRunners } from "./plan.ts";
import type { RuntimePlan, RuntimeReceipt } from "./types.ts";

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function writeGithubOutputs(outputs: Record<string, string>): void {
  const destination = process.env.GITHUB_OUTPUT;
  if (!destination) {
    return;
  }

  const content = Object.entries(outputs)
    .map(([name, value]) => `${name}=${value}\n`)
    .join("");
  fs.appendFileSync(destination, content);
}

function appendSummary(content: string): void {
  if (process.env.GITHUB_STEP_SUMMARY) {
    fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, content);
  }
}

function writeReleasePlan(file: string): void {
  const plan = createRuntimePlan({
    ref: requiredEnv("SOURCE_REF"),
    version: requiredEnv("RUNTIME_VERSION"),
    selection: requiredEnv("PLATFORMS"),
    branch: requiredEnv("DEFAULT_BRANCH"),
    toolingSha: requiredEnv("GITHUB_SHA"),
    baseUrl: requiredEnv("RELEASE_BASE_URL"),
    prefix: process.env.R2_PREFIX || "mash",
  });

  fs.mkdirSync(path.dirname(file), { recursive: true });
  writeJson(file, plan);

  const matrix = {
    include: plan.platforms.map(platform => ({ platform, os: runtimeRunners[platform] })),
  };
  writeGithubOutputs({
    source_sha: plan.sourceSha,
    tooling_sha: plan.toolingSha,
    runtime_version: plan.version,
    matrix: JSON.stringify(matrix),
  });

  appendSummary([
    `## CV runtime ${plan.version}`,
    "",
    `Source: \`${plan.sourceSha}\``,
    "",
    `Platforms: ${plan.platforms.join(", ")}`,
    "",
    "Source tests, frozen OCR and Rust ZIP installation must pass before publication.",
    "",
  ].join("\n"));
}

async function publishRelease(file: string, directory: string): Promise<void> {
  const plan = readJson(file) as RuntimePlan;
  const receipt = await uploadRuntime(plan, directory);
  writeJson(path.join(directory, "runtime-release.json"), receipt);
  tagRuntime(plan);

  const links = receipt.builds.map(build =>
    `- [${build.platform}](${build.url}) — SHA-256 \`${build.sha256}\`\n`,
  );
  appendSummary(links.join(""));
}

export async function runRuntimeReleaseCommand(args: string[]): Promise<void> {
  const [command, file, directory] = args;
  switch (command) {
    case "plan":
      writeReleasePlan(file);
      break;
    case "publish":
      await publishRelease(file, directory);
      break;
    case "sync":
      syncRuntime(readJson(file) as RuntimeReceipt);
      break;
    default:
      throw new Error(`Unknown runtime release command: ${command}`);
  }
}
