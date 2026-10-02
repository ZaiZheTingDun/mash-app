import fs from "node:fs";
import { pathToFileURL } from "node:url";

function parts(version) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(version);
  if (!match) throw new Error(`Invalid release version: ${version}`);
  return { core: match.slice(1, 4).map(Number), pre: match[4]?.split(".") };
}

export function compareVersions(left, right) {
  const a = parts(left);
  const b = parts(right);
  for (let i = 0; i < 3; i++) {
    if (a.core[i] !== b.core[i]) return Math.sign(a.core[i] - b.core[i]);
  }
  if (!a.pre && !b.pre) return 0;
  if (!a.pre) return 1;
  if (!b.pre) return -1;
  for (let i = 0; i < Math.max(a.pre.length, b.pre.length); i++) {
    if (a.pre[i] === b.pre[i]) continue;
    if (a.pre[i] === undefined) return -1;
    if (b.pre[i] === undefined) return 1;
    const numericA = /^\d+$/.test(a.pre[i]);
    const numericB = /^\d+$/.test(b.pre[i]);
    if (numericA && numericB) return Math.sign(Number(a.pre[i]) - Number(b.pre[i]));
    if (numericA !== numericB) return numericA ? -1 : 1;
    return a.pre[i] < b.pre[i] ? -1 : 1;
  }
  return 0;
}

export function mergeUpdaterManifest(release, previous = null, channel = null) {
  const { version, pubDate, target, url, signature } = release;
  parts(version);
  if (!/^(darwin|windows|linux)-(x86_64|aarch64|i686|armv7)$/.test(target)) {
    throw new Error(`Invalid updater target: ${target}`);
  }
  if (!url.startsWith("https://") || !signature.trim()) {
    throw new Error("Updater URL and signature must be complete");
  }
  for (const doc of [previous, channel]) {
    if (doc && compareVersions(doc.version, version) > 0) {
      throw new Error(`Refusing to replace newer release ${doc.version} with ${version}`);
    }
  }
  const platforms = {};
  // Every artifact in one manifest must belong to the same app version.
  for (const doc of [channel, previous]) {
    if (doc && compareVersions(doc.version, version) === 0) Object.assign(platforms, doc.platforms);
  }
  const existing = platforms[target];
  if (existing && (existing.url !== url || existing.signature !== signature.trim())) {
    throw new Error(`Immutable artifact already registered for ${version}/${target}`);
  }
  platforms[target] = { signature: signature.trim(), url };
  for (const [key, artifact] of Object.entries(platforms)) {
    if (!artifact.url?.startsWith("https://") || !artifact.signature?.trim()) {
      throw new Error(`Incomplete updater artifact: ${key}`);
    }
  }
  const sameVersion = [previous, channel].find(doc => doc && compareVersions(doc.version, version) === 0);
  return { version, notes: sameVersion?.notes ?? `mash ${version}`,
    pub_date: sameVersion?.pub_date ?? pubDate, platforms };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [out, version, pubDate, target, url, signature, previousPath, channelPath] = process.argv.slice(2);
  const read = path => path && fs.existsSync(path) ? JSON.parse(fs.readFileSync(path, "utf8")) : null;
  const doc = mergeUpdaterManifest({ version, pubDate, target, url, signature }, read(previousPath), read(channelPath));
  fs.writeFileSync(out, `${JSON.stringify(doc, null, 2)}\n`);
}
