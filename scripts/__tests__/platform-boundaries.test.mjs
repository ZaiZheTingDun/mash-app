import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../src-tauri/src/", import.meta.url));
const osDependency = /#\s*\[\s*cfg(?:_attr)?\s*\([^\]]*\b(?:windows|unix|desktop|mobile|target_os|target_family|target_arch|target_vendor|target_env)\b[^\]]*\]|\bcfg!\s*\([^;{}]*\b(?:windows|unix|desktop|mobile|target_os|target_family|target_arch|target_vendor|target_env)\b|std::os::|std::env::consts::(?:OS|ARCH)/;

async function rustSources(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name === "platform" || entry.name === "tests.rs") continue;
    const name = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await rustSources(name));
    else if (entry.name.endsWith(".rs")) files.push(name);
  }
  return files;
}

function productionSource(source) {
  // Sibling tests.rs files and trailing unit-test modules may use host fixtures.
  return source.split(/#\[cfg\(test\)\]\s*mod\s+tests\s*[{;]/)[0]
    .replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
}

test("shared Rust modules keep OS dependencies inside platform/", async () => {
  const violations = [];
  for (const name of await rustSources(root)) {
    let source = productionSource(await readFile(name, "utf8"));
    if (path.basename(name) === "main.rs") {
      // This executable-level attribute cannot live in a library module.
      source = source.replace('#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]', "");
    }
    if (osDependency.test(source)) violations.push(path.relative(root, name));
  }
  assert.deepEqual(violations, [], "Move host-specific implementation into src/platform/");
});

test("boundary check catches nested cfg, host APIs and runtime OS checks", () => {
  for (const source of [
    '#[cfg(not(any(target_os = "macos", windows)))] fn f() {}',
    'if cfg!(windows) {}',
    'use std::os::windows::process::CommandExt;',
    'let host = std::env::consts::OS;',
  ]) {
    assert.ok(osDependency.test(source), source);
  }
  assert.ok(!osDependency.test(productionSource(
    'fn shared() {}\n#[cfg(test)] mod tests { #[cfg(unix)] fn host_fixture() {} }',
  )));
});
