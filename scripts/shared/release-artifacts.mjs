import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";

export const platforms = ["windows-x86_64", "darwin-aarch64"];
export const digest = file => createHash("sha256").update(fs.readFileSync(file)).digest("hex");

// Conditional writes also protect against another publisher racing our release.
export async function publishImmutable(file, key, { aws }) {
  const sha256 = digest(file);
  try {
    aws([
      "s3api",
      "put-object",
      "--key",
      key,
      "--body",
      path.resolve(file),
      "--if-none-match",
      "*",
      "--cache-control",
      "public, max-age=31536000, immutable",
      "--content-type",
      key.endsWith(".json") ? "application/json" : "application/zip",
    ]);
  } catch (error) {
    const detail = `${error.message}\n${error.stderr ?? ""}`;
    if (!/PreconditionFailed|\(412\)/.test(detail)) {
      throw error;
    }
  }
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "mash-cv-verify-"));
  try {
    const downloaded = path.join(work, "object");
    aws(["s3api", "get-object", "--key", key, downloaded]);
    if (digest(downloaded) !== sha256) {
      throw new Error(`Immutable object differs: ${key}`);
    }
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

