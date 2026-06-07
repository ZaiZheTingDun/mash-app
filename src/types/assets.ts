export interface AssetBundleStatus {
  installed: boolean;
  importedServants: boolean;
  importedCraftEssences: boolean;
  servantFiles: number;
  craftEssenceFiles: number;
  installDir: string;
  currentVersion: number | null;
  appAssetsVersion: number;
  remoteLatestVersion: number | null;
  remoteLatestBaseVersion: number | null;
  targetVersion: number | null;
  updateAvailable: boolean;
  updateDownloadSize: number;
  updatePlan: "none" | "base" | "patch" | string;
  latestUrl: string;
  remoteManifestUrl: string | null;
  updateCheckError: string | null;
}

export interface AssetBundleImportResult {
  importedServants: boolean;
  importedCraftEssences: boolean;
  servantFiles: number;
  craftEssenceFiles: number;
  installDir: string;
}

export interface AssetDownloadProgress {
  kind: string;
  phase: "connecting" | "downloading" | "downloaded" | "installing" | "installed";
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number | null;
  etaSeconds: number | null;
}

export interface AssetDownloadInstallResult {
  installed: boolean;
  installedVersion: number | null;
  plan: "none" | "base" | "patch" | string;
  servantFiles: number;
  craftEssenceFiles: number;
  installDir: string;
}
