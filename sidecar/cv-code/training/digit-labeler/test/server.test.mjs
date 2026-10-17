import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { DatasetStore, createLabelServer, safeChildPath } from "../server.mjs";

async function fixture({ withSequence = false, withRegion = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "mash-digit-labeler-"));
  const dataRoot = join(root, "data");
  const rawPath = join(dataRoot, "raw", "battle", "cn", "frame.png");
  const cropPath = join(dataRoot, "crops", "battle", "cn", "frame", "digit.png");
  await mkdir(join(dataRoot, "raw", "battle", "cn"), { recursive: true });
  await mkdir(join(dataRoot, "crops", "battle", "cn", "frame"), { recursive: true });
  const rawBytes = Buffer.from("raw-image");
  await writeFile(rawPath, rawBytes);
  await writeFile(cropPath, Buffer.from("crop-image"));
  if (withRegion) {
    const regionDir = join(dataRoot, "regions", "battle", "cn", "frame");
    await mkdir(regionDir, { recursive: true });
    await writeFile(join(regionDir, "np_gauge-0.png"), Buffer.from("whole-region"));
  }
  const parentId = `sha256:${createHash("sha256").update(rawBytes).digest("hex")}`;
  const sample = {
    schemaVersion: 1,
    id: "sample-1",
    image: "crops/battle/cn/frame/digit.png",
    label: null,
    source: "np_gauge",
    style: "battle_hud",
    server: "cn",
    parentId,
    screenshotType: "battle",
    resolution: [1920, 1080],
    sourceBboxPx: [100, 200, 14, 20],
    split: "test",
    notes: null,
  };
  await writeFile(join(dataRoot, "manifest.jsonl"), `${JSON.stringify(sample)}\n`);
  let sequence = null;
  if (withSequence) {
    const digest = createHash("sha256")
      .update(`${parentId}:battle:np_gauge:0:sequence-v1`).digest("hex").slice(0, 16);
    sequence = {
      schemaVersion: 1, id: `battle-np_gauge-sequence-${digest}`,
      image: "regions/battle/cn/frame/np_gauge-0.png", label: null,
      source: "np_gauge", server: "cn", parentId, screenshotType: "battle",
      slot: 0, resolution: [1920, 1080], sourceBboxPx: [90, 200, 70, 20],
    };
    await writeFile(join(dataRoot, "sequence-manifest.jsonl"), `${JSON.stringify(sequence)}\n`);
  }
  const store = new DatasetStore(dataRoot);
  await store.initialize();
  return { dataRoot, root, sample, sequence, store };
}

test("catalog exposes crop and source screenshot URLs", async () => {
  const { store } = await fixture();
  const catalog = store.catalog();
  assert.deepEqual(catalog.sources, ["np_gauge"]);
  assert.deepEqual(catalog.servers, ["cn"]);
  assert.deepEqual(catalog.screenshotTypes, ["battle"]);
  assert.match(catalog.samples[0].cropUrl, /^\/asset\?path=/u);
  assert.match(catalog.samples[0].rawUrl, /^\/raw\?parentId=/u);
});

test("setLabel persists through an atomic manifest replacement", async () => {
  const { dataRoot, sample, store } = await fixture();
  const updated = await store.setLabel(sample.id, "7");
  assert.equal(updated.label, "7");
  const manifest = await readFile(join(dataRoot, "manifest.jsonl"), "utf8");
  const persisted = JSON.parse(manifest.trim());
  assert.equal(persisted.label, "7");
  assert.equal(persisted.split, "test");
});

test("whole-number labels are independent of single-digit labels", async () => {
  const { dataRoot, sample, sequence, store } = await fixture({ withSequence: true });
  const updated = await store.setSequenceLabel(sequence.id, "105");
  assert.equal(updated.label, "105");
  assert.equal(store.catalog().samples[0].label, null);
  assert.equal(JSON.parse((await readFile(join(dataRoot, "sequence-manifest.jsonl"), "utf8")).trim()).label, "105");
  assert.equal(JSON.parse((await readFile(join(dataRoot, "manifest.jsonl"), "utf8")).trim()).id, sample.id);
  await assert.rejects(store.setSequenceLabel(sequence.id, "10x"), /1-12 digits/u);
  await store.setSequenceLabel(sequence.id, "invalid");
  assert.equal(store.catalog().sequences[0].label, "invalid");
});

test("existing whole-region crops get a separate unconfirmed manifest", async () => {
  const { dataRoot, store } = await fixture({ withRegion: true });
  const catalog = store.catalog();
  assert.equal(catalog.sequences.length, 1);
  assert.equal(catalog.sequences[0].label, null);
  const saved = JSON.parse((await readFile(join(dataRoot, "sequence-manifest.jsonl"), "utf8")).trim());
  assert.equal(saved.image, "regions/battle/cn/frame/np_gauge-0.png");
});

test("individual NP labels appear only as an unconfirmed whole-number candidate", async () => {
  const { dataRoot, sample } = await fixture({ withSequence: true });
  const digits = ["invalid", "9", "0"].map((label, index) => ({
    ...sample, id: `fixed-${index}`, label,
    image: `crops/battle/cn/frame/np_gauge-0-${["hundreds", "tens", "ones"][index]}-12345678.png`,
  }));
  await writeFile(join(dataRoot, "manifest.jsonl"), `${digits.map((digit) => JSON.stringify(digit)).join("\n")}\n`);
  const store = new DatasetStore(dataRoot);
  await store.initialize();
  const candidate = store.catalog().sequences[0];
  assert.equal(candidate.suggestedLabel, "90");
  assert.equal(candidate.label, null);
});

test("safeChildPath rejects traversal", async () => {
  const { dataRoot } = await fixture();
  assert.throws(() => safeChildPath(dataRoot, "../../outside.png"), /escapes root/u);
});

test("HTTP server returns the page and catalog", async () => {
  const { root, store, sequence } = await fixture({ withSequence: true });
  const publicRoot = join(root, "public");
  await mkdir(publicRoot);
  await writeFile(join(publicRoot, "index.html"), "<h1>labeler</h1>");
  const server = createLabelServer(store, { publicRoot });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const address = server.address();
    const page = await fetch(`http://127.0.0.1:${address.port}/`);
    const catalog = await fetch(`http://127.0.0.1:${address.port}/api/catalog`);
    assert.equal(await page.text(), "<h1>labeler</h1>");
    assert.equal((await catalog.json()).samples.length, 1);
    const response = await fetch(`http://127.0.0.1:${address.port}/api/sequence-label`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: sequence.id, label: "120" }),
    });
    assert.equal(response.status, 200);
    assert.equal((await response.json()).label, "120");
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});
