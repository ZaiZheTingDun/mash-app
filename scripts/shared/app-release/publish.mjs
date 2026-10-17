import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { digest, publishImmutable } from "../release-artifacts.mjs";
import { compareVersions, mergeUpdaterManifest } from "../updater-manifest.mjs";
import { readJson, writeJson, hashBytes, objectKeyFromUrl, createR2Client } from "./common.mjs";
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

function publicDownloads(receipt) {
  const { plan } = receipt;
  const downloads = [];
  for (const runtime of Object.values(plan.runtimes)) {
    downloads.push({
      key: objectKeyFromUrl(runtime.runtimeUrl, plan.baseUrl),
      sha256: runtime.runtimeSha256,
    });
  }
  for (const build of receipt.builds) {
    for (const file of build.files) {
      downloads.push({ key: artifactKey(plan, build.platform, file.name), sha256: file.sha256 });
    }
  }
  return downloads;
}

async function verifyPublicDownload({
  url,
  sha256,
  errorMessage,
  fetcher = fetch,
  noCache = false,
}) {
  const options = { signal: AbortSignal.timeout(300000) };
  if (noCache) {
    options.cache = "no-store";
  }
  const response = await fetcher(url, options);
  if (!response.ok || hashBytes(Buffer.from(await response.arrayBuffer())) !== sha256) {
    throw new Error(errorMessage);
  }
}

export async function uploadRelease(receipt, directory, aws = createR2Client()) {
  await uploadBuildArtifacts(receipt, directory, aws);
  const releaseRecord = path.join(directory, "app-release.json");
  const releaseKey = `${receipt.plan.prefix}/releases/versions/v${receipt.plan.appVersion}/release.json`;
  await publishImmutable(releaseRecord, releaseKey, { aws });
  // Clients use the public URLs. Successful R2 API reads alone are insufficient.
  for (const { key, sha256 } of publicDownloads(receipt)) {
    await verifyPublicDownload({
      url: `${receipt.plan.baseUrl}/${key}`,
      sha256,
      errorMessage: `Public CDN verification failed: ${key}`,
    });
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

async function updateChannel({
  plan,
  manifest,
  manifestFile,
  channel,
  aws,
  workDirectory,
  fetcher,
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
  await verifyPublicDownload({
    url: `${plan.baseUrl}/${key}`,
    sha256: digest(manifestFile),
    errorMessage: `Public channel verification failed: ${key}`,
    fetcher,
    noCache: true,
  });
}

async function updateInstaller(plan, download, aws, fetcher) {
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
  await verifyPublicDownload({
    url: `${plan.baseUrl}/${key}`,
    sha256: installer.sha256,
    errorMessage: `Public installer verification failed: ${key}`,
    fetcher,
    noCache: true,
  });
}

export async function promoteRelease(
  receipt,
  directory,
  aws = createR2Client(),
  signatureVerifier = verifySignature,
  fetcher = fetch,
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
      await updateChannel({ plan, manifest, manifestFile, channel, aws, workDirectory, fetcher });
    }
    for (const download of downloads) {
      await updateInstaller(plan, download, aws, fetcher);
    }
  } finally {
    fs.rmSync(workDirectory, { recursive: true, force: true });
  }
}
