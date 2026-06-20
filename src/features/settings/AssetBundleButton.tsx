import { useCallback, useEffect, useState } from "react";
import { Button, Spinner, Text } from "@radix-ui/themes";
import { invoke, listen } from "../../tauri";
import type {
  AssetBundleStatus,
  AssetDownloadInstallResult,
  AssetDownloadProgress,
} from "../../types/assets";

interface AssetBundleButtonProps {
  status?: AssetBundleStatus | null;
  onImported?: () => void;
  onBusyChange?: (busy: boolean) => void;
}

function formatBytes(value: number): string {
  if (value >= 1024 * 1024) {
    return `${(value / 1024 / 1024).toFixed(1)} MB`;
  }
  if (value >= 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${value} B`;
}

function formatSeconds(value: number): string {
  if (value >= 60) {
    const minutes = Math.floor(value / 60);
    const seconds = value % 60;
    return `${minutes}分${seconds.toString().padStart(2, "0")}秒`;
  }
  return `${value}秒`;
}

function versionLabel(value: number | null | undefined): string {
  return value == null ? "未安装" : `v${value}`;
}

function statusText(status: AssetBundleStatus | null): string {
  if (!status) {
    return "正在检查素材包…";
  }
  if (status.updateAvailable && status.targetVersion != null) {
    const size =
      status.updateDownloadSize > 0 ? `（${formatBytes(status.updateDownloadSize)}）` : "";
    return `素材包需要更新：${versionLabel(status.currentVersion)} → v${status.targetVersion}${size}`;
  }
  if (!status.installed) {
    return "素材包未安装：需要下载包含 servants / ces 的资源包";
  }
  if (status.updateCheckError) {
    return `素材包已安装：${versionLabel(status.currentVersion)}（检查更新失败）`;
  }
  return `素材包已安装：${versionLabel(status.currentVersion)}`;
}

function progressLabel(progress: AssetDownloadProgress | null): string {
  if (!progress) {
    return "";
  }
  if (progress.phase === "connecting") {
    return `正在连接 ${progress.kind} 下载源…`;
  }
  if (progress.phase === "installing") {
    return `正在安装 ${progress.kind}…`;
  }
  if (progress.phase === "installed") {
    return `${progress.kind} 安装完成`;
  }
  if (progress.phase === "downloaded") {
    return `${progress.kind} 下载完成`;
  }

  const total = progress.totalBytes ? ` / ${formatBytes(progress.totalBytes)}` : "";
  const speed = progress.bytesPerSecond
    ? ` · ${formatBytes(progress.bytesPerSecond)}/s`
    : "";
  const eta = progress.etaSeconds != null
    ? ` · 预计剩余 ${formatSeconds(progress.etaSeconds)}`
    : "";
  return `正在下载 ${progress.kind}：${formatBytes(progress.downloadedBytes)}${total}${speed}${eta}`;
}

function progressPercent(progress: AssetDownloadProgress | null): number | null {
  if (!progress) {
    return null;
  }
  if (
    progress.phase === "downloaded" ||
    progress.phase === "installing" ||
    progress.phase === "installed"
  ) {
    return 100;
  }
  if (!progress.totalBytes) {
    return null;
  }
  return Math.min(100, (progress.downloadedBytes / progress.totalBytes) * 100);
}

export function AssetBundleButton({
  status: controlledStatus,
  onImported,
  onBusyChange,
}: AssetBundleButtonProps) {
  const [localBundleStatus, setLocalBundleStatus] = useState<AssetBundleStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<AssetDownloadProgress | null>(null);
  const bundleStatus = controlledStatus !== undefined ? controlledStatus : localBundleStatus;
  const usesControlledStatus = controlledStatus !== undefined;

  const refreshStatus = useCallback(async () => {
    if (usesControlledStatus) return;
    const next = await invoke<AssetBundleStatus>("get_asset_bundle_status");
    setLocalBundleStatus(next);
  }, [usesControlledStatus]);

  useEffect(() => {
    refreshStatus().catch((err) => setStatus(`检查失败：${String(err)}`));
  }, [refreshStatus]);

  useEffect(() => {
    onBusyChange?.(busy);
  }, [busy, onBusyChange]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<AssetDownloadProgress>("asset-download-progress", (event) => {
      setDownloadProgress(event.payload);
    })
      .then((next) => {
        if (disposed) {
          next();
        } else {
          unlisten = next;
        }
      })
      .catch((err) => setStatus(`监听下载进度失败：${String(err)}`));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const handleDownload = useCallback(async () => {
    const forceBase = Boolean(bundleStatus?.installed && !bundleStatus.updateAvailable);
    if (forceBase) {
      const confirmed = window.confirm(
        "确认重新下载素材包？\n\n这会重新下载完整素材包，并覆盖本地从者和礼装素材。"
      );
      if (!confirmed) {
        return;
      }
    }

    setBusy(true);
    setStatus(null);
    setDownloadProgress(null);
    try {
      const result = forceBase
        ? await invoke<AssetDownloadInstallResult>("download_asset_bundles", {
            forceBase: true,
          })
        : await invoke<AssetDownloadInstallResult>("download_asset_bundles");
      if (!result.installed) {
        setStatus("素材包已是最新。");
      } else {
        setStatus(
          forceBase
            ? `重新下载完成：素材包 v${result.installedVersion}`
            : `安装完成：素材包 v${result.installedVersion}`
        );
      }
      await refreshStatus();
      onImported?.();
    } catch (err) {
      setStatus(`下载失败：${String(err)}`);
    } finally {
      setBusy(false);
    }
  }, [bundleStatus, onImported, refreshStatus]);

  const canDownload = Boolean(bundleStatus?.updateAvailable || !bundleStatus?.installed);
  const progressValue = progressPercent(downloadProgress);

  return (
    <div className="asset-import-block runtime-import-block">
      <div className="runtime-actions">
        <Button
          type="button"
          variant="soft"
          color={canDownload ? "blue" : "gray"}
          disabled={busy || !bundleStatus}
          onClick={handleDownload}
        >
          {busy ? <Spinner size="1" /> : null}
          <Text size="2" weight="medium">
            {busy ? "下载中…" : canDownload ? "在线更新" : "重新下载"}
          </Text>
        </Button>
      </div>
      <Text size="1" color={bundleStatus?.updateAvailable || !bundleStatus?.installed ? "amber" : "gray"}>
        {statusText(bundleStatus)}
      </Text>
      {downloadProgress && (
        <div className="runtime-download-progress">
          <progress
            className="runtime-progress-bar"
            max={100}
            value={progressValue ?? undefined}
          />
          <Text size="1" color="gray">
            {progressLabel(downloadProgress)}
          </Text>
        </div>
      )}
      {bundleStatus?.updateCheckError && (
        <Text size="1" color="gray" className="runtime-download-url">
          {bundleStatus.updateCheckError}
        </Text>
      )}
      {status && (
        <Text size="1" color={status.startsWith("导入失败") ? "red" : "gray"}>
          {status}
        </Text>
      )}
    </div>
  );
}
