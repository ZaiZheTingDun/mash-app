import { useCallback, useEffect, useState } from "react";
import { Button, Spinner, Text } from "@radix-ui/themes";
import { invoke, listen } from "../tauri";
import type {
  AssetBundleImportResult,
  AssetBundleStatus,
  AssetDownloadInstallResult,
  AssetDownloadProgress,
} from "../types/assets";

interface AssetBundleButtonProps {
  onImported?: () => void;
}

function basename(path: string): string {
  const normalized = path.split("\\").join("/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || path;
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
    return "素材包未安装：需要导入或下载包含 servants / ces 的资源包";
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

export function AssetBundleButton({ onImported }: AssetBundleButtonProps) {
  const [bundleStatus, setBundleStatus] = useState<AssetBundleStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<AssetDownloadProgress | null>(null);

  const refreshStatus = useCallback(async () => {
    const next = await invoke<AssetBundleStatus>("get_asset_bundle_status");
    setBundleStatus(next);
  }, []);

  useEffect(() => {
    refreshStatus().catch((err) => setStatus(`检查失败：${String(err)}`));
  }, [refreshStatus]);

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
    setBusy(true);
    setStatus(null);
    setDownloadProgress(null);
    try {
      const result = await invoke<AssetDownloadInstallResult>("download_asset_bundles");
      if (!result.installed) {
        setStatus("素材包已是最新。");
      } else {
        setStatus(`安装完成：素材包 v${result.installedVersion}`);
      }
      await refreshStatus();
      onImported?.();
    } catch (err) {
      setStatus(`下载失败：${String(err)}`);
    } finally {
      setBusy(false);
    }
  }, [onImported, refreshStatus]);

  const handleImport = useCallback(async () => {
    setBusy(true);
    setStatus(null);
    setDownloadProgress(null);
    try {
      const zipPath = await invoke<string | null>("pick_asset_bundle");
      if (!zipPath) {
        return;
      }

      const confirmed = window.confirm(
        `确认导入素材包？\n\n${basename(zipPath)}\n\n现有同名素材将被替换。`
      );
      if (!confirmed) {
        return;
      }

      const result = await invoke<AssetBundleImportResult>("import_asset_bundle", {
        zipPath,
      });
      const parts: string[] = [];
      if (result.importedServants) {
        parts.push(`从者 ${result.servantFiles} 个文件`);
      }
      if (result.importedCraftEssences) {
        parts.push(`礼装 ${result.craftEssenceFiles} 个文件`);
      }
      setStatus(
        parts.length > 0
          ? `导入完成：${parts.join("，")}`
          : "导入完成，但压缩包中没有可用素材"
      );
      await refreshStatus();
      onImported?.();
    } catch (err) {
      setStatus(`导入失败：${String(err)}`);
    } finally {
      setBusy(false);
    }
  }, [onImported, refreshStatus]);

  const canDownload = Boolean(bundleStatus?.updateAvailable || !bundleStatus?.installed);
  const progressPercent = downloadProgress?.totalBytes
    ? Math.min(100, (downloadProgress.downloadedBytes / downloadProgress.totalBytes) * 100)
    : null;

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
        <button
          type="button"
          className="link-button runtime-manual-upload"
          disabled={busy}
          onClick={handleImport}
        >
          手动导入
        </button>
      </div>
      <Text size="1" color={bundleStatus?.updateAvailable || !bundleStatus?.installed ? "amber" : "gray"}>
        {statusText(bundleStatus)}
      </Text>
      {downloadProgress && (
        <div className="runtime-download-progress">
          <progress
            className="runtime-progress-bar"
            max={100}
            value={progressPercent ?? undefined}
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
