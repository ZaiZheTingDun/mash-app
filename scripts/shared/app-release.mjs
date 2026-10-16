import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { runReleaseCommand } from "./app-release/cli.mjs";

export { metadataFiles } from "./app-release/common.mjs";

export {
  planDigest,
  codeDigest,
  releaseNotes,
  createPlan,
  validatePlan,
  validatePreflightRun,
  summary,
} from "./app-release/plan.mjs";

export {
  prepareSource,
  recordPlatform,
  verifySignature,
  assemble,
  syncMetadata,
} from "./app-release/candidate.mjs";

export { uploadRelease, readObject, promoteRelease } from "./app-release/publish.mjs";

export { publishTags, publishGithub } from "./app-release/github.mjs";

const entryPath = process.argv[1];
const currentPath = fileURLToPath(import.meta.url);

if (entryPath && fs.realpathSync.native(currentPath) === fs.realpathSync.native(entryPath)) {
  await runReleaseCommand(process.argv.slice(2));
}
