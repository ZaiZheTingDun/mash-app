import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { DatasetStore, createLabelServer, safeChildPath } from "../server.mjs";

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "mash-digit-labeler-"));
  const dataRoot = join(root, "data");
  const rawPath = join(dataRoot, "raw", "cn", "frame.png");
  const cropPath = join(dataRoot, "crops", "cn", "frame", "digit.png");
  await mkdir(join(dataRoot, "raw", "cn"), { recursive: true });
  await mkdir(join(dataRoot, "crops", "cn", "frame"), { recursive: true });
  const rawBytes = Buffer.from("raw-image");
  await writeFile(rawPath, rawBytes);
  await writeFile(cropPath, Buffer.from("crop-image"));
  const parentId = `sha256:${createHash("sha256").update(rawBytes).digest("hex")}`;
  const sample = {
    schemaVersion: 1,
    id: "sample-1",
    image: "crops/cn/frame/digit.png",
    label: null,
    source: "np_gauge",
    style: "battle_hud",
    server: "cn",
    parentId,
    resolution: [1920, 1080],
    sourceBboxPx: [100, 200, 14, 20],
    split: "test",
    notes: null,
  };
  await writeFile(join(dataRoot, "manifest.jsonl"), `${JSON.stringify(sample)}\n`);
  const store = new DatasetStore(dataRoot);
  await store.initialize();
  return { dataRoot, root, sample, store };
}

test("catalog exposes crop and source screenshot URLs", async () => {
  const { store } = await fixture();
  const catalog = store.catalog();
  assert.deepEqual(catalog.sources, ["np_gauge"]);
  assert.deepEqual(catalog.servers, ["cn"]);
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

test("safeChildPath rejects traversal", async () => {
  const { dataRoot } = await fixture();
  assert.throws(() => safeChildPath(dataRoot, "../../outside.png"), /escapes root/u);
});

test("HTTP server returns the page and catalog", async () => {
  const { root, store } = await fixture();
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
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});
