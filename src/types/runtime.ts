export interface RuntimeStatus {
  requiredRuntimeVersion: string;
  installedRuntimeVersion: string | null;
  runtimeInstalled: boolean;
  installed: boolean;
  platform: string;
  runtimeDownloadUrl: string | null;
  runtimeExpectedSha256: string | null;
  runtimeInstallDir: string;
  executablePath: string;
}

export interface RuntimeInstallResult {
  installedKind: "runtime";
  installedVersion: string;
  platform: string;
  installDir: string;
  executablePath: string | null;
}

export interface RuntimeDownloadProgress {
  kind: "runtime";
  phase: "connecting" | "downloading" | "downloaded" | "installing" | "installed";
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
}

export interface RuntimeDownloadInstallResult {
  installed: RuntimeInstallResult[];
}
