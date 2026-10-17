import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { compareVersions } from "./updater-manifest.mjs";
import { runtimeChangedFiles } from "./runtime-inputs.mjs";

const manifestPath = "src-tauri/resources/runtime-manifest.json";
const read = file => fs.readFileSync(file, "utf8");
const writeJson = (file, value) => fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
const git = (...args) => execFileSync("git", args, { encoding: "utf8", windowsHide: true }).trim();
export function validateVersion(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`Version must look like x.y.z: ${version}`);
}
const escapeRegex = text => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const sectionPattern = section => new RegExp(`(\\[${escapeRegex(section)}\\][ \\t]*\\r?\\n)([\\s\\S]*?)(?=\\r?\\n\\[|$)`);
function replaceValue(text, section, key, value, allowNew = false) {
  const pattern = sectionPattern(section);
  const entry = new RegExp(`(^${escapeRegex(key)}\\s*=\\s*")[^"]+(")`, "m");
  const match = text.match(pattern);
  if (!match || (!entry.test(match[2]) && !allowNew)) throw new Error(`Missing ${section}.${key}`);
  return text.replace(pattern, (_, header, body) => header + (entry.test(body)
    ? body.replace(entry, (_, prefix, suffix) => `${prefix}${value}${suffix}`)
    : `${body.trimEnd()}\n${key} = "${value}"\n`));
}
function validatePlatform(platform) {
  if (!/^(windows|darwin)-(x86_64|aarch64)$/.test(platform)) throw new Error("Invalid platform");
}
export function readVersions(platform) {
  if (platform) validatePlatform(platform);
  const source = read("versions.toml");
  const value = (section, key) => {
    const body = source.match(sectionPattern(section))?.[2];
    const match = body?.match(new RegExp(`^${escapeRegex(key)}\\s*=\\s*"([^"]+)"`, "m"));
    if (!match) throw new Error(`Missing ${section}.${key}`);
    return match[1];
  };
  const runtimeBody = source.match(sectionPattern("mash_cv.runtime"))?.[2];
  if (runtimeBody === undefined) throw new Error("Missing mash_cv.runtime");
  const runtimes = Object.fromEntries([...runtimeBody.matchAll(/^([\w-]+)\s*=\s*"([^"]+)"/gm)].map(match => [match[1], match[2]]));
  return { app: value("app", "version"), runtimes,
    ...(platform ? { runtime: runtimes[platform] ?? "" } : {}) };
}
export function updateVersion(kind, version, platform) {
  validateVersion(version);
  if (!["app", "runtime"].includes(kind)) throw new Error(`Unknown version kind: ${kind}`);
  if (kind === "runtime") validatePlatform(platform);
  const section = kind === "app" ? "app" : kind === "runtime" ? "mash_cv.runtime" : "mash_cv";
  const key = kind === "app" ? "version" : kind === "runtime" ? platform : kind;
  const updates = new Map([["versions.toml", replaceValue(read("versions.toml"), section, key, version, kind === "runtime")]]);
  if (kind === "app") {
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    config.version = version;
    updates.set("src-tauri/tauri.conf.json", `${JSON.stringify(config, null, 2)}\n`);
    updates.set("src-tauri/Cargo.toml", replaceValue(read("src-tauri/Cargo.toml"), "package", "version", version));
    const lock = read("src-tauri/Cargo.lock");
    const pattern = /(\[\[package\]\]\r?\nname = "mash"\r?\nversion = ")[^"]+(")/;
    if (!pattern.test(lock)) throw new Error("Missing mash package in Cargo.lock");
    updates.set("src-tauri/Cargo.lock", lock.replace(pattern, (_, prefix, suffix) => `${prefix}${version}${suffix}`));
  }
  for (const [file, text] of updates) fs.writeFileSync(file, text);
}
export function applyCvArtifact(kind, version, platform, url, sha256) {
  validateVersion(version);
  if (kind !== "runtime") throw new Error("Only external runtime artifacts are supported");
  validatePlatform(platform);
  if (!/^[a-f0-9]{64}$/.test(sha256)) throw new Error("Invalid SHA-256");
  if (new URL(url).protocol !== "https:") throw new Error("Artifact URL must use HTTPS");
  const manifest = JSON.parse(read(manifestPath));
  manifest.platforms ??= {};
  manifest.platforms[platform] ??= {};
  manifest.platforms[platform].runtimeVersion = version;
  manifest.platforms[platform].runtimeUrl = url;
  manifest.platforms[platform].runtimeSha256 = sha256;
  writeJson(manifestPath, manifest);
}
export function finalizeChangelog(version, date) {
  validateVersion(version);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error("Invalid release date");
  const original = read("CHANGELOG.md");
  if (!/^## 未发布\r?$/m.test(original)) throw new Error("Missing unreleased changelog heading");
  fs.writeFileSync("CHANGELOG.md", original.replace(/^## 未发布\r?$/m, `## v${version} - ${date}`));
}
export function hasChangesSince(ref, paths) {
  return Boolean(ref && git("diff", "--name-only", `${ref}..HEAD`, "--", ...paths));
}
export function releasePlan(app, runtime, platform, publish = false) {
  validateVersion(app);
  if (runtime) validateVersion(runtime);
  if (!/^windows-(x86_64|aarch64)$/.test(platform)) throw new Error("Unsupported Windows platform");
  const current = readVersions(platform);
  const latestTag = pattern => git("for-each-ref", "--count=1", "--sort=-version:refname", "--format=%(refname:strip=2)", `refs/tags/${pattern}`);
  const appTag = `v${app}`;
  if (compareVersions(app, current.app) < 0) throw new Error("App version cannot go backwards");
  const sameVersion = app === current.app;
  if (sameVersion && !git("tag", "--points-at", "HEAD", "--list", appTag)) throw new Error(`Current app version requires HEAD at ${appTag}`);
  if (sameVersion && runtime) throw new Error("Changing CV packages requires a new app version");
  const tags = [!sameVersion && appTag, runtime && `cv-runtime/${platform}/${runtime}`].filter(Boolean);
  for (const tag of tags) {
    if (git("tag", "--list", tag)) throw new Error(`Tag already exists: ${tag}`);
  }
  if (runtime && /^\d+\.\d+\.\d+$/.test(current.runtime) && compareVersions(runtime, current.runtime) <= 0) {
    throw new Error("CV runtime version must increase");
  }
  const changelog = read("CHANGELOG.md");
  const ready = new RegExp(`^## v${app.replaceAll(".", "\\.")} - \\d{4}-\\d{2}-\\d{2}\\r?$`, "m").test(changelog);
  const unreleased = /^## 未发布\r?$/m.test(changelog);
  if (!ready && !unreleased) throw new Error("Missing target release or unreleased changelog heading");
  const runtimeTag = `cv-runtime/${platform}/${current.runtime}`;
  const base = git("tag", "--list", runtimeTag) || git("log", "-1", "--format=%H", "-S", `${platform} = "${current.runtime}"`, "--", "versions.toml") || latestTag("v*");
  const runtimeChanged = Boolean(base && runtimeChangedFiles(base, "HEAD", platform).length);
  // A second platform publishes the exact same tagged source without new CV releases.
  if (publish && !sameVersion) {
    if (runtimeChanged && !runtime) throw new Error("CV runtime changes detected; pass -CvRuntime <version>");
  }
  return { current, app, runtime, platform, tags, sameVersion, changelog: ready ? "ready" : "unreleased", runtimeChanged };
}

if (process.argv[1] && fs.realpathSync.native(fileURLToPath(import.meta.url)) === fs.realpathSync.native(process.argv[1])) {
  const [command, ...args] = process.argv.slice(2);
  switch (command) {
    case "versions": console.log(JSON.stringify(readVersions(args[0]))); break;
    case "version": updateVersion(...args); break;
    case "artifact": applyCvArtifact(...args); break;
    case "changelog": finalizeChangelog(...args); break;
    case "plan": console.log(JSON.stringify(releasePlan(args[0], args[1] === "-" ? "" : args[1], args[2], args[3] === "publish"))); break;
    default: throw new Error(`Unknown release command: ${command}`);
  }
}
