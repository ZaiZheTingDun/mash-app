export type RuntimePlatform = "windows-x86_64" | "darwin-aarch64";

export interface RuntimeManifestEntry {
  runtimeVersion: string;
  runtimeUrl: string;
  runtimeSha256: string;
}

export interface RuntimeManifest {
  platforms: Record<string, RuntimeManifestEntry>;
}

export interface RuntimePlanOptions {
  ref: string;
  version: string;
  selection: string;
  branch: string;
  toolingSha: string;
  baseUrl: string;
  prefix?: string;
}

export interface RuntimePlan {
  schemaVersion: 1;
  sourceSha: string;
  toolingSha: string;
  branch: string;
  version: string;
  platforms: RuntimePlatform[];
  baseUrl: string;
  prefix: string;
  previous: Record<string, { version: string; manifest: RuntimeManifestEntry }>;
}

export interface RuntimeArtifact {
  name: string;
  key: string;
  url: string;
}

export interface RuntimeBuild extends RuntimeArtifact {
  platform: RuntimePlatform;
  sha256: string;
}

export interface RuntimeReceipt {
  plan: RuntimePlan;
  builds: RuntimeBuild[];
}

export interface RuntimeReport {
  status: string;
  platform: string;
  sourceSha: string;
  version: string;
  planSha256: string;
  runtimeSha256: string;
}

export type R2Client = (args: string[]) => unknown;
