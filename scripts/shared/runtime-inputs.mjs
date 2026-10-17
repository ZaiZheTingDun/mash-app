import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { fileURLToPath } from "node:url";

const nativeBuildScripts = new Map([
  ["windows-x86_64", "scripts/windows/build-cv.ps1"],
  ["windows-aarch64", "scripts/windows/build-cv.ps1"],
  ["darwin-x86_64", "scripts/macos/build-cv.sh"],
  ["darwin-aarch64", "scripts/macos/build-cv.sh"],
]);

export function runtimeChangedFiles(base, source, platform) {
  const nativeScript = nativeBuildScripts.get(platform);
  if (!nativeScript) {
    throw new Error(`Unsupported runtime platform: ${platform}`);
  }
  const paths = [
    "sidecar/cv-runtime",
    ":(exclude)sidecar/cv-runtime/tests",
    ":(exclude)sidecar/cv-runtime/*.md",
    nativeScript,
  ];
  const output = execFileSync("git", ["diff", "--name-only", base, source, "--", ...paths], {
    encoding: "utf8",
    windowsHide: true,
  });
  return output.trim().split("\n").filter(Boolean);
}

if (process.argv[1] && fs.realpathSync.native(process.argv[1]) === fs.realpathSync.native(fileURLToPath(import.meta.url))) {
  const [base, source, platform] = process.argv.slice(2);
  console.log(Boolean(base && runtimeChangedFiles(base, source, platform).length));
}
