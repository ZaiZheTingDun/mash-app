import fs from "node:fs";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

export const runtimeManifestPath = "src-tauri/resources/runtime-manifest.json";

export const metadataFiles = [
  "versions.toml",
  "src-tauri/tauri.conf.json",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  runtimeManifestPath,
  "CHANGELOG.md",
];

export function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

export function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

export function runCommand(command, args, options = {}) {
  return execFileSync(command, args, {
    encoding: "utf8",
    windowsHide: true,
    stdio: "pipe",
    ...options,
  }).trim();
}

export function gitCommand(...args) {
  return runCommand("git", args);
}

export function readGitFile(commitSha, file) {
  return gitCommand("show", `${commitSha}:${file}`);
}

export function resolveTagCommit(tag) {
  const matchingTag = gitCommand("tag", "--list", tag);
  if (!matchingTag) {
    return null;
  }
  return gitCommand("rev-parse", `${tag}^{commit}`);
}

export function listChangedFiles(baseSha, sourceSha, paths) {
  const output = gitCommand("diff", "--name-only", baseSha, sourceSha, "--", ...paths);
  return output.split("\n").filter(Boolean);
}

export function hashBytes(value) {
  return createHash("sha256").update(value).digest("hex");
}

export function assertStableVersion(value) {
  if (!/^\d+\.\d+\.\d+$/.test(value)) {
    throw new Error("Version must look like x.y.z");
  }
  return value;
}

export function assertCommitSha(value) {
  if (!/^[a-f0-9]{40}$/.test(value)) {
    throw new Error("Invalid source SHA");
  }
  return value;
}

export function validatePublicUrl(value) {
  const url = new URL(value);
  const hasPrivateOrMutableParts = url.username || url.password || url.search || url.hash;
  if (url.protocol !== "https:" || hasPrivateOrMutableParts) {
    throw new Error("Expected public HTTPS URL");
  }
  return value;
}

export function objectKeyFromUrl(url, baseUrl) {
  validatePublicUrl(url);
  const publicPrefix = `${baseUrl.replace(/\/+$/, "")}/`;
  if (!url.startsWith(publicPrefix)) {
    throw new Error("Artifact URL must belong to RELEASE_BASE_URL");
  }
  const key = url.slice(publicPrefix.length);
  const hasUnsafeSegment = key.split("/").some((part) => {
    return !part || part === "." || part === ".." || /%2[ef]/i.test(part);
  });
  if (key.includes("\\") || hasUnsafeSegment) {
    throw new Error("Invalid object key");
  }
  return key;
}

function contentTypeForKey(key) {
  if (key.endsWith(".json")) {
    return "application/json";
  }
  if (key.endsWith(".sig")) {
    return "text/plain";
  }
  return "application/octet-stream";
}

// Keep bucket/endpoint configuration out of the release steps. The returned
// function accepts AWS CLI arguments, which tests replace with an in-memory store.
export function createR2Client(env = process.env) {
  for (const name of ["R2_ENDPOINT", "R2_BUCKET"]) {
    if (!env[name]) {
      throw new Error(`${name} is required`);
    }
  }
  validatePublicUrl(env.R2_ENDPOINT);
  return (args) => {
    const commandArgs = [...args];
    const isDownload = commandArgs[1] === "get-object";
    const outputFile = isDownload ? [commandArgs.pop()] : [];
    const contentTypeIndex = commandArgs.indexOf("--content-type");
    if (contentTypeIndex >= 0) {
      const objectKey = commandArgs[commandArgs.indexOf("--key") + 1];
      commandArgs[contentTypeIndex + 1] = contentTypeForKey(objectKey);
    }
    return runCommand("aws", [
      ...commandArgs,
      "--bucket",
      env.R2_BUCKET,
      "--endpoint-url",
      env.R2_ENDPOINT,
      "--cli-connect-timeout",
      "30",
      "--cli-read-timeout",
      "300",
      ...outputFile,
    ]);
  };
}
