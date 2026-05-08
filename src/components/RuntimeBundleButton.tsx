import { useCallback, useEffect, useState } from "react";
import { Button, Spinner, Text } from "@radix-ui/themes";
import { invoke } from "../tauri";
import type { RuntimeInstallResult, RuntimeStatus } from "../types/runtime";

interface RuntimeBundleButtonProps {
  onInstalled?: () => void;
}

function basename(path: string): string {
  const normalized = path.split("\\").join("/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || path;
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

export function RuntimeBundleButton({ onInstalled }: RuntimeBundleButtonProps) {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    const next = await invoke<RuntimeStatus>("get_runtime_status");
    setStatus(next);
  }, []);

  useEffect(() => {
    refreshStatus().catch((err) => setMessage(`检查失败：${String(err)}`));
  }, [refreshStatus]);

  const handleImport = useCallback(async () => {
    setBusy(true);
    setMessage(null);
    try {
      const zipPath = await invoke<string | null>("pick_runtime_bundle");
      if (!zipPath) {
        return;
      }

      const confirmed = window.confirm(
        `确认安装 CV 运行时？\n\n${basename(zipPath)}\n\n现有同版本 runtime 将被替换。`
      );
      if (!confirmed) {
        return;
      }

      const result = await invoke<RuntimeInstallResult>("import_runtime_bundle", {
        zipPath,
      });
      setMessage(
        `安装完成：${result.installedKind === "runtime" ? "base" : "code"} ${result.installedVersion}`
      );
      await refreshStatus();
      onInstalled?.();
    } catch (err) {
      setMessage(`安装失败：${String(err)}`);
    } finally {
      setBusy(false);
    }
  }, [onInstalled, refreshStatus]);

  const needsInstall = !status?.installed;

  return (
    <div className="asset-import-block runtime-import-block">
      <Button
        type="button"
        variant="soft"
        color={needsInstall ? "blue" : "gray"}
        disabled={busy}
        onClick={handleImport}
      >
        {busy ? <Spinner size="1" /> : null}
        <Text size="2" weight="medium">
          {busy ? "安装中…" : needsInstall ? "安装 CV 包" : "替换 CV 包"}
        </Text>
      </Button>
      <Text size="1" color={needsInstall ? "amber" : "gray"}>
        {statusText(status)}
      </Text>
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
        <Text size="1" color={message.startsWith("安装失败") ? "red" : "gray"}>
          {message}
        </Text>
      )}
    </div>
  );
}
