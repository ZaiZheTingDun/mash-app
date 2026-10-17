import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { digest } from "../release-artifacts.mjs";
import { runCommand, gitCommand, resolveTagCommit, hashBytes } from "./common.mjs";
import { releaseNotes } from "./plan.mjs";

export function publishTags(receipt) {
  const tags = [{ name: `v${receipt.plan.appVersion}`, commit: receipt.releaseSha }];
  for (const tag of tags) {
    const existingCommit = resolveTagCommit(tag.name);
    if (existingCommit && existingCommit !== tag.commit) {
      throw new Error(`Refusing to move tag ${tag.name}`);
    }
    if (!existingCommit) {
      gitCommand("tag", tag.name, tag.commit);
    }
    gitCommand("push", "origin", `refs/tags/${tag.name}`);
  }
}

function githubReleases(receipt, directory) {
  const appFiles = [path.join(directory, "app-release.json")];
  for (const build of receipt.builds) {
    for (const file of build.files) {
      appFiles.push(path.join(directory, build.platform, file.name));
    }
  }
  const releases = [
    {
      tag: `v${receipt.plan.appVersion}`,
      title: `Mash v${receipt.plan.appVersion}`,
      files: appFiles,
      notes: releaseNotes(fs.readFileSync("CHANGELOG.md", "utf8"), receipt.plan.appVersion),
    },
  ];
  return releases;
}

function findGithubRelease(tag, gh) {
  try {
    return JSON.parse(gh(["release", "view", tag, "--json", "assets,isDraft"]));
  } catch (error) {
    const details = `${error.message}\n${error.stderr ?? ""}`;
    if (/release not found|HTTP 404|Not Found/i.test(details)) {
      return null;
    }
    throw error;
  }
}

function ensureGithubRelease(release, workDirectory, gh) {
  const existing = findGithubRelease(release.tag, gh);
  if (existing) {
    return existing;
  }
  if (!release.notes) {
    throw new Error("Missing release notes");
  }
  const notesFile = path.join(workDirectory, "notes.md");
  fs.writeFileSync(notesFile, release.notes);
  gh([
    "release",
    "create",
    release.tag,
    "--verify-tag",
    "--draft",
    "--title",
    release.title,
    "--notes-file",
    notesFile,
  ]);
  return { assets: [], isDraft: true };
}

function verifyGithubAsset(release, existing, file, workDirectory, gh) {
  const name = path.basename(file);
  const assetExists = existing.assets.some((asset) => asset.name === name);
  if (!assetExists) {
    if (!existing.isDraft) {
      throw new Error(`Published GitHub release is missing ${name}`);
    }
    gh(["release", "upload", release.tag, file]);
  }
  const downloadDirectory = path.join(workDirectory, hashBytes(`${release.tag}/${name}`));
  fs.mkdirSync(downloadDirectory);
  gh(["release", "download", release.tag, "--pattern", name, "--dir", downloadDirectory]);
  if (digest(path.join(downloadDirectory, name)) !== digest(file)) {
    throw new Error(`GitHub asset differs: ${release.tag}/${name}`);
  }
}

export function publishGithub(receipt, directory, gh = (args) => runCommand("gh", args)) {
  const releases = githubReleases(receipt, directory);
  const workDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "mash-github-release-"));
  try {
    for (const release of releases) {
      const existing = ensureGithubRelease(release, workDirectory, gh);
      for (const file of release.files) {
        verifyGithubAsset(release, existing, file, workDirectory, gh);
      }
    }
    // Verify every attachment before publishing.
    for (const release of releases) {
      const latestFlag = "--latest";
      gh(["release", "edit", release.tag, "--draft=false", latestFlag]);
    }
  } finally {
    fs.rmSync(workDirectory, { recursive: true, force: true });
  }
}
