import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

async function config(name) {
  return JSON.parse(await readFile(new URL(`../../src-tauri/${name}`, import.meta.url), "utf8"));
}

function resourceMap(value) {
  assert.ok(value && typeof value === "object" && !Array.isArray(value),
    "Resource maps must remain objects so Tauri merges common and platform resources");
  return value;
}

test("dev watcher excludes the generated CV bundle to prevent rebuild loops", async () => {
  const ignored = (await readFile(new URL("../../src-tauri/.taurignore", import.meta.url), "utf8"))
    .split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith("#"));
  assert.deepEqual(ignored, ["/resources/mash-cv-code/"]);
  const resources = resourceMap((await config("tauri.conf.json")).bundle.resources);
  assert.ok("resources/mash-cv-code/" in resources, "Watcher exclusion must preserve app bundling");
});

test("common resources preserve their installed paths and exclude ADB", async () => {
  const common = resourceMap((await config("tauri.conf.json")).bundle.resources);
  for (const [source, destination] of Object.entries(common)) {
    assert.ok(!source.startsWith("resources/adb"), source);
    assert.equal(destination, source.endsWith("/*") ? source.slice(0, -1) : source);
  }
  for (const source of ["resources/servers/shared/cv.json", "resources/servers/jp/cv.json",
    "resources/servers/cn/cv.json", "resources/images/*", "resources/scrcpy/scrcpy-server.jar",
    "src/resources/servants.json", "resources/assets-manifest.json", "resources/runtime-manifest.json",
    "resources/mash-cv-code/"]) {
    assert.ok(source in common, `Missing shared resource: ${source}`);
  }
});

for (const [platform, expected] of [
  ["macos", ["adb"]],
  ["windows", ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll", "NOTICE.txt"]],
]) {
  test(`${platform} bundles only its own ADB files alongside shared resources`, async () => {
    const common = resourceMap((await config("tauri.conf.json")).bundle.resources);
    const native = resourceMap((await config(`tauri.${platform}.conf.json`)).bundle.resources);
    // Tauri uses JSON Merge Patch: object keys merge, whereas arrays replace.
    const merged = { ...common, ...native };
    assert.deepEqual(Object.keys(native).sort(), expected.map(name => `resources/adb/${name}`).sort());
    assert.deepEqual(Object.keys(merged).filter(name => name.startsWith("resources/adb/")).sort(),
      Object.keys(native).sort());
    for (const [source, destination] of Object.entries(common)) {
      assert.equal(merged[source], destination, `Platform override changed shared resource: ${source}`);
    }
    for (const [source, destination] of Object.entries(native)) {
      assert.equal(destination, source);
      assert.ok((await readFile(new URL(`../../src-tauri/${source}`, import.meta.url))).length > 0);
    }
  });
}
