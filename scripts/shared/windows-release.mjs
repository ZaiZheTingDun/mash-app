import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { selectUpdaterArtifact } from "./updater-artifact.mjs";
import { validateVersion } from "./release-state.mjs";

const platform = "windows-x86_64";
const manifestPath = "src-tauri/resources/runtime-manifest.json";
const readJson = file => JSON.parse(fs.readFileSync(file, "utf8"));
const writeJson = (file, value) => fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
const run = (command, args) => execFileSync(command, args, { encoding: "utf8", windowsHide: true, stdio: "pipe" }).trim();
const git = (...args) => run("git", args);
const digest = file => createHash("sha256").update(fs.readFileSync(file)).digest("hex");

function publicUrl(value) {
  const url = new URL(value);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) {
    throw new Error(`Expected a public HTTPS URL: ${value}`);
  }
  return value;
}

function tomlValue(text, section, key) {
  const sectionMatch = text.match(new RegExp(`\\[${section.replaceAll(".", "\\.")}\\][ \\t]*\\r?\\n([\\s\\S]*?)(?=\\r?\\n\\[|$)`));
  const value = sectionMatch?.[1].match(new RegExp(`^${key}\\s*=\\s*"([^"]+)"`, "m"))?.[1];
  if (!value) throw new Error(`Missing ${section}.${key}`);
  return value;
}

function sourceFile(sourceSha, file) {
  return git("show", `${sourceSha}:${file}`);
}

export function planRelease(version, sourceRef, channel) {
  validateVersion(version);
  if (!/^[A-Za-z0-9._-]+$/.test(channel)) throw new Error("Invalid release channel");
  const sourceSha = git("rev-parse", "--verify", "--end-of-options", `${sourceRef}^{commit}`);
  const tag = `v${version}`;
  if (git("rev-parse", `${tag}^{commit}`) !== sourceSha) throw new Error(`${tag} must point to the selected source`);
  const config = JSON.parse(sourceFile(sourceSha, "src-tauri/tauri.conf.json"));
  const versions = sourceFile(sourceSha, "versions.toml");
  const cargo = sourceFile(sourceSha, "src-tauri/Cargo.toml");
  if (config.version !== version || tomlValue(versions, "app", "version") !== version || tomlValue(cargo, "package", "version") !== version) {
    throw new Error("App version is inconsistent across Tauri, versions.toml and Cargo.toml");
  }
  const manifest = JSON.parse(sourceFile(sourceSha, manifestPath));
  const artifact = manifest.platforms?.[platform];
  if (!artifact || !/^[a-f0-9]{64}$/.test(artifact.codeSha256) || !/^[a-f0-9]{64}$/.test(artifact.runtimeSha256)) {
    throw new Error(`Published CV manifest is incomplete for ${platform}`);
  }
  if (tomlValue(versions, "mash_cv", "code") !== manifest.mashCvCodeVersion ||
      tomlValue(versions, "mash_cv.runtime", platform) !== artifact.runtimeVersion) {
    throw new Error("CV versions.toml and runtime manifest are inconsistent");
  }
  publicUrl(artifact.codeUrl);
  publicUrl(artifact.runtimeUrl);
  return { schemaVersion: 1, sourceSha, sourceRef, version, tag, channel, platform,
    cvCodeVersion: manifest.mashCvCodeVersion, cvRuntimeVersion: artifact.runtimeVersion,
    cvCodeUrl: artifact.codeUrl, cvCodeSha256: artifact.codeSha256,
    cvRuntimeUrl: artifact.runtimeUrl, cvRuntimeSha256: artifact.runtimeSha256 };
}

export function validateReceipt(receipt) {
  validateVersion(receipt.version);
  if (receipt.schemaVersion !== 1 || receipt.platform !== platform || !/^[a-f0-9]{40}$/.test(receipt.sourceSha) ||
      receipt.tag !== `v${receipt.version}` || !/^[A-Za-z0-9._-]+$/.test(receipt.channel) ||
      !/^Mash_\d+\.\d+\.\d+_(x64|arm64)-setup\.exe$/.test(receipt.artifact) ||
      !/^[a-f0-9]{64}$/.test(receipt.sha256) || !Number.isSafeInteger(receipt.size) || receipt.size <= 0) {
    throw new Error("Invalid Windows release record");
  }
  if (!/^[a-f0-9]{64}$/.test(receipt.signatureSha256) || !/^[a-f0-9]{64}$/.test(receipt.cvCodeSha256) ||
      !/^[a-f0-9]{64}$/.test(receipt.cvRuntimeSha256) || !/^[\w.-]+$/.test(receipt.cvCodeVersion) ||
      !/^[\w.-]+$/.test(receipt.cvRuntimeVersion)) throw new Error("Invalid release dependency record");
  publicUrl(receipt.cvCodeUrl);
  publicUrl(receipt.cvRuntimeUrl);
  return receipt;
}

export function recordRelease(plan, bundleDir, directory) {
  if (git("rev-parse", "HEAD") !== plan.sourceSha) throw new Error("Build checkout differs from release source");
  const config = readJson("src-tauri/tauri.conf.json");
  const artifact = selectUpdaterArtifact(bundleDir, config, plan.platform, true);
  const signature = `${artifact}.sig`;
  const receipt = validateReceipt({ ...plan, artifact: path.basename(artifact), sha256: digest(artifact), size: fs.statSync(artifact).size,
    signatureSha256: digest(signature) });
  fs.mkdirSync(directory, { recursive: true });
  fs.copyFileSync(artifact, path.join(directory, receipt.artifact));
  fs.copyFileSync(signature, path.join(directory, `${receipt.artifact}.sig`));
  writeJson(path.join(directory, "release.json"), receipt);
  return receipt;
}

export function validateCandidate(receiptFile, directory) {
  const receipt = validateReceipt(readJson(receiptFile));
  const artifact = path.join(directory, receipt.artifact);
  const signature = `${artifact}.sig`;
  if (digest(artifact) !== receipt.sha256 || fs.statSync(artifact).size !== receipt.size || digest(signature) !== receipt.signatureSha256) {
    throw new Error("Windows release candidate changed after build");
  }
  return receipt;
}

if (process.argv[1] && fs.realpathSync.native(fileURLToPath(import.meta.url)) === fs.realpathSync.native(process.argv[1])) {
  const [command, ...args] = process.argv.slice(2);
  switch (command) {
    case "plan": writeJson(args[3], planRelease(args[0], args[1], args[2])); break;
    case "record": recordRelease(readJson(args[0]), args[1], args[2]); break;
    case "validate": validateCandidate(args[0], args[1]); break;
    default: throw new Error(`Unknown command: ${command}`);
  }
}
