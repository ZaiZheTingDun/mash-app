export interface RuntimeStatus {
  requiredRuntimeVersion: string;
  installedRuntimeVersion: string | null;
  runtimeInstalled: boolean;
  requiredCodeVersion: string;
  installedCodeVersion: string | null;
  codeInstalled: boolean;
  installed: boolean;
  platform: string;
  runtimeDownloadUrl: string | null;
  runtimeExpectedSha256: string | null;
  runtimeInstallDir: string;
  executablePath: string;
  codeDownloadUrl: string | null;
  codeExpectedSha256: string | null;
  codeInstallDir: string;
  codePath: string;
}

export interface RuntimeInstallResult {
  installedKind: "runtime" | "code";
  installedVersion: string;
  platform: string;
  installDir: string;
  executablePath: string | null;
  codePath: string | null;
}

export interface RuntimeDownloadProgress {
  kind: "runtime" | "code";
  phase: "connecting" | "downloading" | "downloaded" | "installing" | "installed";
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
}

export interface RuntimeDownloadInstallResult {
  installed: RuntimeInstallResult[];
}
