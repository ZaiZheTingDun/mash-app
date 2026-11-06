import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { runRuntimeReleaseCommand } from "./runtime-release/cli.ts";

export {
  createRuntimePlan,
  planDigest,
  validateRuntimePlan,
} from "./runtime-release/plan.ts";
export {
  assembleRuntime,
  runtimeArtifact,
  syncRuntime,
  tagRuntime,
  uploadRuntime,
} from "./runtime-release/artifacts.ts";
export type {
  R2Client,
  RuntimePlan,
  RuntimePlanOptions,
  RuntimePlatform,
  RuntimeReceipt,
} from "./runtime-release/types.ts";

const scriptPath = process.argv[1];
const modulePath = fileURLToPath(import.meta.url);
if (scriptPath && fs.realpathSync.native(modulePath) === fs.realpathSync.native(scriptPath)) {
  await runRuntimeReleaseCommand(process.argv.slice(2));
}
