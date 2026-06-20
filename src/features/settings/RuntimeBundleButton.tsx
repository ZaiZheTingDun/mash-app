import { useCallback, useEffect, useState } from "react";
import { Button, Spinner, Text } from "@radix-ui/themes";
import { invoke, listen } from "../../tauri";
import type {
  RuntimeDownloadInstallResult,
  RuntimeDownloadProgress,
  RuntimeStatus,
} from "../../types/runtime";

interface RuntimeBundleButtonProps {
  status?: RuntimeStatus | null;
  onInstalled?: () => void;
  onBusyChange?: (busy: boolean) => void;
}

function statusText(status: RuntimeStatus | null): string {
  if (!status) {
    return "正在检查 CV 运行时…";
  }
  if (status.installed) {
    return `CV 运行时已安装：base ${status.requiredRuntimeVersion} / code ${status.requiredCodeVersion}`;
  }
  const missing: string[] = [];
  if (!status.runtimeInstalled) {
    missing.push(
      status.installedRuntimeVersion
        ? `base ${status.installedRuntimeVersion} → ${status.requiredRuntimeVersion}`
        : `base ${status.requiredRuntimeVersion}`
    );
  }
  if (!status.codeInstalled) {
    missing.push(
      status.installedCodeVersion
        ? `code ${status.installedCodeVersion} → ${status.requiredCodeVersion}`
        : `code ${status.requiredCodeVersion}`
    );
  }
  return `CV 运行时未就绪：需要 ${missing.join("，")}`;
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

function progressLabel(progress: RuntimeDownloadProgress | null): string {
  if (!progress) {
    return "";
  }
  const target = progress.kind === "runtime" ? "base" : "code";
  if (progress.phase === "connecting") {
    return `正在连接 ${target} 下载源…`;
  }
  if (progress.phase === "installing") {
    return `正在安装 ${target}…`;
  }
  if (progress.phase === "installed") {
    return `${target} 安装完成`;
  }
  if (progress.phase === "downloaded") {
    return `${target} 下载完成`;
  }

  const total = progress.totalBytes ? ` / ${formatBytes(progress.totalBytes)}` : "";
  const speed = progress.bytesPerSecond
    ? ` · ${formatBytes(progress.bytesPerSecond)}/s`
    : "";
  const eta = progress.etaSeconds != null
    ? ` · 预计剩余 ${formatSeconds(progress.etaSeconds)}`
    : "";
  return `正在下载 ${target}：${formatBytes(progress.downloadedBytes)}${total}${speed}${eta}`;
}

function progressPercent(progress: RuntimeDownloadProgress | null): number | null {
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

export function RuntimeBundleButton({
  status: controlledStatus,
  onInstalled,
  onBusyChange,
}: RuntimeBundleButtonProps) {
  const [localStatus, setLocalStatus] = useState<RuntimeStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<RuntimeDownloadProgress | null>(null);
  const status = controlledStatus !== undefined ? controlledStatus : localStatus;
  const usesControlledStatus = controlledStatus !== undefined;

  const refreshStatus = useCallback(async () => {
    if (usesControlledStatus) return;
    const next = await invoke<RuntimeStatus>("get_runtime_status");
    setLocalStatus(next);
  }, [usesControlledStatus]);

  useEffect(() => {
    refreshStatus().catch((err) => setMessage(`检查失败：${String(err)}`));
  }, [refreshStatus]);

  useEffect(() => {
    onBusyChange?.(busy);
  }, [busy, onBusyChange]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<RuntimeDownloadProgress>("runtime-download-progress", (event) => {
      setDownloadProgress(event.payload);
    })
      .then((next) => {
        if (disposed) {
          next();
        } else {
          unlisten = next;
        }
      })
      .catch((err) => setMessage(`监听下载进度失败：${String(err)}`));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const handleDownload = useCallback(async () => {
    setBusy(true);
    setMessage(null);
    setDownloadProgress(null);
    try {
      const result = await invoke<RuntimeDownloadInstallResult>("download_runtime_bundles");
      if (result.installed.length === 0) {
        setMessage("CV 运行时已是最新。");
      } else {
        const installed = result.installed
          .map(
            (item) =>
              `${item.installedKind === "runtime" ? "base" : "code"} ${item.installedVersion}`
          )
          .join(" / ");
        setMessage(`安装完成：${installed}`);
      }
      await refreshStatus();
      onInstalled?.();
    } catch (err) {
      setMessage(`下载失败：${String(err)}`);
    } finally {
      setBusy(false);
    }
  }, [onInstalled, refreshStatus]);

  const needsInstall = !status?.installed;
  const progressValue = progressPercent(downloadProgress);

  return (
    <div className="asset-import-block runtime-import-block">
      <div className="runtime-actions">
        <Button
          type="button"
          variant="soft"
          color={needsInstall ? "blue" : "gray"}
          disabled={busy || !status}
          onClick={handleDownload}
        >
          {busy ? <Spinner size="1" /> : null}
          <Text size="2" weight="medium">
            {busy ? "下载中…" : needsInstall ? "开始下载" : "重新下载"}
          </Text>
        </Button>
      </div>
      <Text size="1" color={needsInstall ? "amber" : "gray"}>
        {statusText(status)}
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
      {status?.runtimeDownloadUrl && !status.runtimeInstalled && (
        <Text size="1" color="gray" className="runtime-download-url">
          base：{status.runtimeDownloadUrl}
        </Text>
      )}
      {status?.codeDownloadUrl && !status.codeInstalled && (
        <Text size="1" color="gray" className="runtime-download-url">
          code：{status.codeDownloadUrl}
        </Text>
      )}
      {message && (
        <Text
          size="1"
          color={
            message.startsWith("安装失败") || message.startsWith("下载失败") ? "red" : "gray"
          }
        >
          {message}
        </Text>
      )}
    </div>
  );
}
