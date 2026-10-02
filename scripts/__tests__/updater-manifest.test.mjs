import assert from "node:assert/strict";
import test from "node:test";
import { compareVersions, mergeUpdaterManifest } from "../updater-manifest.mjs";

const release = { version: "0.14.0", pubDate: "2026-10-02T12:00:00Z",
  target: "windows-x86_64", url: "https://example.com/windows.exe", signature: "win-signature" };
const mac = { version: "0.14.0", platforms: {
  "darwin-aarch64": { url: "https://example.com/mac.tar.gz", signature: "mac-signature" },
} };

test("adding Windows preserves the same version's macOS artifact", () => {
  const doc = mergeUpdaterManifest(release, mac);
  assert.deepEqual(doc.platforms["darwin-aarch64"], mac.platforms["darwin-aarch64"]);
  assert.equal(doc.platforms["windows-x86_64"].signature, "win-signature");
});
test("same-version retry is idempotent", () => {
  const doc = mergeUpdaterManifest(release, mac);
  assert.deepEqual(mergeUpdaterManifest(release, doc), doc);
});
test("older artifacts are not relabelled as a new version", () => {
  assert.deepEqual(Object.keys(mergeUpdaterManifest(release, { ...mac, version: "0.13.0" }).platforms), ["windows-x86_64"]);
});
test("newer channel and immutable artifacts cannot be overwritten", () => {
  assert.throws(() => mergeUpdaterManifest(release, null, { ...mac, version: "0.15.0" }), /newer release/);
  const doc = mergeUpdaterManifest(release, mac);
  assert.throws(() => mergeUpdaterManifest({ ...release, signature: "changed" }, doc), /Immutable artifact/);
});
test("incomplete platform entries cannot break every updater client", () => {
  assert.throws(() => mergeUpdaterManifest(release, { ...mac, platforms: { "darwin-aarch64": {} } }), /Incomplete/);
});
test("version ordering handles prereleases and build metadata", () => {
  assert.equal(compareVersions("0.14.0", "0.14.0-beta.2"), 1);
  assert.equal(compareVersions("0.14.0-beta.10", "0.14.0-beta.2"), 1);
  assert.equal(compareVersions("v0.14.0+build", "0.14.0"), 0);
});
