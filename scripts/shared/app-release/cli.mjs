import fs from "node:fs";
import path from "node:path";
import { readJson, writeJson, assertStableVersion } from "./common.mjs";
import { createPlan, validatePlan, validatePreflightRun, summary } from "./plan.mjs";
import {
  prepareCodeArchive,
  prepareSource,
  recordPlatform,
  assemble,
  syncMetadata,
} from "./candidate.mjs";
import { uploadRelease, promoteRelease } from "./publish.mjs";
import { publishTags, publishGithub } from "./github.mjs";

function writeGithubOutputs(values) {
  if (!process.env.GITHUB_OUTPUT) {
    return;
  }
  const output = Object.entries(values)
    .map(([name, value]) => `${name}=${value}\n`)
    .join("");
  fs.appendFileSync(process.env.GITHUB_OUTPUT, output);
}

function writeReleasePlan(planFile) {
  const plan = createPlan({
    ref: process.env.SOURCE_REF,
    appVersion: process.env.APP_VERSION,
    codeVersion: process.env.CV_CODE_VERSION || "",
    branch: process.env.DEFAULT_BRANCH,
    toolingSha: process.env.GITHUB_SHA,
    baseUrl: process.env.RELEASE_BASE_URL,
    prefix: process.env.R2_PREFIX || "mash",
  });
  writeJson(planFile, plan);
  const summaryFile = process.env.GITHUB_STEP_SUMMARY || `${planFile}.md`;
  fs.appendFileSync(summaryFile, summary(plan));
  writeGithubOutputs({ ready: plan.ready });
}

function validatePlanOrigin(planFile, runFile) {
  const plan = readJson(planFile);
  validatePreflightRun(
    plan,
    readJson(runFile),
    process.env.GITHUB_REPOSITORY,
    process.env.DEFAULT_BRANCH,
  );
  if (process.env.GITHUB_OUTPUT) {
    writeGithubOutputs({
      source_sha: plan.sourceSha,
      tooling_sha: plan.toolingSha,
      app_version: assertStableVersion(plan.appVersion),
    });
  }
}

function readCandidate(directory) {
  return readJson(path.join(directory, "app-release.json"));
}

// Keep command names and positional arguments stable for the workflows. Each
// case only decodes its inputs and dispatches to the owning release module.
export async function runReleaseCommand(commandLineArgs) {
  const [command, ...args] = commandLineArgs;
  switch (command) {
    case "plan":
      writeReleasePlan(args[0]);
      break;
    case "validate-run":
      validatePlanOrigin(args[0], args[1]);
      break;
    case "validate":
      validatePlan(readJson(args[0]));
      break;
    case "code": {
      const [planFile, directory] = args;
      prepareCodeArchive(validatePlan(readJson(planFile)), directory);
      break;
    }
    case "prepare": {
      const [planFile, directory] = args;
      prepareSource(readJson(planFile), path.resolve(directory));
      break;
    }
    case "record": {
      const [preparedFile, platform, directory, reportFile] = args;
      recordPlatform(readJson(preparedFile), platform, directory, reportFile);
      break;
    }
    case "assemble": {
      const [directory, buildsDirectory] = args;
      const prepared = readJson(path.join(directory, "prepared.json"));
      assemble(prepared, buildsDirectory, directory);
      break;
    }
    case "upload":
      await uploadRelease(readCandidate(args[0]), args[0]);
      break;
    case "tags":
      publishTags(readCandidate(args[0]));
      break;
    case "github":
      publishGithub(readCandidate(args[0]), args[0]);
      break;
    case "promote":
      await promoteRelease(readCandidate(args[0]), args[0]);
      break;
    case "sync":
      syncMetadata(readJson(args[0]));
      break;
    default:
      throw new Error(`Unknown app release command: ${command}`);
  }
}
