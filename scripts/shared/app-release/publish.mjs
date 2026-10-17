import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { digest, publishImmutable } from "../release-artifacts.mjs";
import { compareVersions, mergeUpdaterManifest } from "../updater-manifest.mjs";
import { readJson, writeJson, objectKeyFromUrl, createR2Client } from "./common.mjs";
import { verifySignature, validateFile } from "./candidate.mjs";

function artifactKey(plan, platform, name) {
  return `${plan.prefix}/releases/versions/v${plan.appVersion}/${platform}/${name}`;
}

async function uploadBuildArtifacts(receipt, directory, aws) {
  for (const build of receipt.builds) {
    for (const file of build.files) {
      const localFile = path.join(directory, build.platform, file.name);
      validateFile(localFile, file);
      await publishImmutable(localFile, artifactKey(receipt.plan, build.platform, file.name), {
        aws,
      });
    }
  }
}

function verifyR2Download(aws, key, sha256, directory) {
  const download = path.join(directory, "download");
  aws(["s3api", "get-object", "--key", key, download]);
  const actualSha256 = digest(download);
  if (actualSha256 !== sha256) {
    throw new Error(`R2 object SHA-256 mismatch: ${key}; expected ${sha256}, got ${actualSha256}`);
  }
}

export async function uploadRelease(receipt, directory, aws = createR2Client()) {
  const workDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "mash-upload-"));
  try {
    for (const runtime of Object.values(receipt.plan.runtimes)) {
      const key = objectKeyFromUrl(runtime.runtimeUrl, receipt.plan.baseUrl);
      verifyR2Download(aws, key, runtime.runtimeSha256, workDirectory);
    }
    await uploadBuildArtifacts(receipt, directory, aws);
    const releaseRecord = path.join(directory, "app-release.json");
    const releaseKey = `${receipt.plan.prefix}/releases/versions/v${receipt.plan.appVersion}/release.json`;
    await publishImmutable(releaseRecord, releaseKey, { aws });
  } finally {
    fs.rmSync(workDirectory, { recursive: true, force: true });
  }
}

function isMissingObject(error) {
  return /NoSuchKey|Not Found|\(404\)/.test(`${error.message}\n${error.stderr ?? ""}`);
}

export function readObject(aws, key, directory) {
  const file = path.join(directory, "previous");
  try {
    const metadata = JSON.parse(aws(["s3api", "get-object", "--key", key, file]));
    return { document: readJson(file), etag: metadata.ETag };
  } catch (error) {
    if (isMissingObject(error)) {
      return null;
    }
    throw error;
  }
}

function readObjectHead(aws, key) {
  try {
    return JSON.parse(aws(["s3api", "head-object", "--key", key]));
  } catch (error) {
    if (isMissingObject(error)) {
      return null;
    }
    throw error;
  }
}

function buildUpdaterManifest(receipt, directory, signatureVerifier) {
  const { plan } = receipt;
  const manifest = {
    version: plan.appVersion,
    notes: `Mash ${plan.appVersion}`,
    pub_date: plan.createdAt,
    platforms: {},
  };
  for (const build of receipt.builds) {
    const updater = build.files.find((file) => file.roles.includes("updater"));
    if (!updater) {
      throw new Error("Missing updater artifact");
    }
    const updaterFile = path.join(directory, build.platform, updater.name);
    const signature = build.files.find((file) => file.name === `${updater.name}.sig`);
    validateFile(updaterFile, updater);
    validateFile(`${updaterFile}.sig`, signature);
    signatureVerifier(updaterFile, `${updaterFile}.sig`, plan.publicKeys[build.platform]);
    manifest.platforms[build.platform] = {
      url: `${plan.baseUrl}/${artifactKey(plan, build.platform, updater.name)}`,
      signature: fs.readFileSync(`${updaterFile}.sig`, "utf8").trim(),
    };
  }
  return manifest;
}

function prepareChannelUpdates(plan, manifest, aws, workDirectory) {
  return ["stable", "windows-stable"].map((channel) => {
    const key = `${plan.prefix}/releases/${channel}/latest.json`;
    const previous = readObject(aws, key, workDirectory);
    for (const [target, artifact] of Object.entries(manifest.platforms)) {
      mergeUpdaterManifest(
        { version: manifest.version, pubDate: manifest.pub_date, target, ...artifact },
        null,
        previous?.document,
      );
    }
    if (previous && !previous.etag) {
      throw new Error("Missing channel ETag");
    }
    return { key, previous };
  });
}

function prepareInstallerUpdates(receipt, directory, aws) {
  const { plan } = receipt;
  return receipt.builds.map((build) => {
    const installer = build.files.find((file) => file.roles.includes("installer"));
    const localFile = path.join(directory, build.platform, installer.name);
    validateFile(localFile, installer);
    const latestName = installer.name.replace(`_${plan.appVersion}_`, "_");
    const key = `${plan.prefix}/downloads/latest/${latestName}`;
    const previous = readObjectHead(aws, key);
    if (previous && !previous.ETag) {
      throw new Error("Missing direct-download ETag");
    }
    const publishedVersion = previous?.Metadata?.["app-version"];
    if (publishedVersion && compareVersions(publishedVersion, plan.appVersion) > 0) {
      throw new Error("Refusing to replace a newer direct installer");
    }
    return { installer, localFile, key, previous };
  });
}

function conditionalWriteArgs(etag) {
  if (etag) {
    return ["--if-match", etag];
  }
  return ["--if-none-match", "*"];
}

function updateChannel({
  manifest,
  manifestFile,
  channel,
  aws,
  workDirectory,
}) {
  const { key, previous } = channel;
  aws([
    "s3api",
    "put-object",
    "--key",
    key,
    "--body",
    manifestFile,
    "--content-type",
    "application/json",
    "--cache-control",
    "no-cache",
    ...conditionalWriteArgs(previous?.etag),
  ]);
  const publishedManifest = readObject(aws, key, workDirectory)?.document;
  if (JSON.stringify(publishedManifest) !== JSON.stringify(manifest)) {
    throw new Error(`Published channel mismatch: ${key}`);
  }
}

function updateInstaller(plan, download, aws, workDirectory) {
  const { key, previous, localFile, installer } = download;
  aws([
    "s3api",
    "put-object",
    "--key",
    key,
    "--body",
    localFile,
    "--cache-control",
    "no-cache",
    "--metadata",
    JSON.stringify({ "app-version": plan.appVersion }),
    ...conditionalWriteArgs(previous?.ETag),
  ]);
  verifyR2Download(aws, key, installer.sha256, workDirectory);
}

export async function promoteRelease(
  receipt,
  directory,
  aws = createR2Client(),
  signatureVerifier = verifySignature,
) {
  const { plan } = receipt;
  const workDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "mash-promote-"));
  try {
    const manifest = buildUpdaterManifest(receipt, directory, signatureVerifier);
    const manifestFile = path.join(workDirectory, "version.json");
    writeJson(manifestFile, manifest);
    await publishImmutable(
      manifestFile,
      `${plan.prefix}/releases/versions/v${plan.appVersion}/latest.json`,
      { aws },
    );
    // Validate every existing pointer before changing the first mutable object.
    const channels = prepareChannelUpdates(plan, manifest, aws, workDirectory);
    const downloads = prepareInstallerUpdates(receipt, directory, aws);
    for (const { installer, localFile } of downloads) {
      await publishImmutable(localFile, `${plan.prefix}/downloads/versions/${installer.name}`, {
        aws,
      });
    }
    for (const channel of channels) {
      updateChannel({ manifest, manifestFile, channel, aws, workDirectory });
    }
    for (const download of downloads) {
      updateInstaller(plan, download, aws, workDirectory);
    }
  } finally {
    fs.rmSync(workDirectory, { recursive: true, force: true });
  }
}
