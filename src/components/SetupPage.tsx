import { useCallback, useEffect, useState } from "react";
import { Box, Button, Flex, Spinner, Text } from "@radix-ui/themes";
import { AssetBundleButton } from "./AssetBundleButton";
import { RuntimeBundleButton } from "./RuntimeBundleButton";
import { invoke } from "../tauri";
import type { AssetBundleStatus } from "../types/assets";
import type { RuntimeStatus } from "../types/runtime";

interface SetupPageProps {
  mode?: "setup" | "manage";
  onBack?: () => void;
  onReady?: () => void;
}

interface ResourceManagementPanelProps {
  mode?: "setup" | "manage";
  onBack?: () => void;
  onReady?: () => void;
  embedded?: boolean;
}

export function ResourceManagementPanel({
  mode = "setup",
  onBack,
  onReady,
  embedded = false,
}: ResourceManagementPanelProps) {
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null);
  const [assetStatus, setAssetStatus] = useState<AssetBundleStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const [assetsBusy, setAssetsBusy] = useState(false);

  const refresh = useCallback(async () => {
    setChecking(true);
    setError(null);
    try {
      const [runtime, assets] = await Promise.all([
        invoke<RuntimeStatus>("get_runtime_status"),
        invoke<AssetBundleStatus>("get_asset_bundle_status"),
      ]);
      setRuntimeStatus(runtime);
      setAssetStatus(assets);
      if (mode === "setup" && runtime.installed && assets.installed) {
        onReady?.();
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setChecking(false);
    }
  }, [mode, onReady]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleCancel = useCallback(async () => {
    await invoke("cancel_resource_downloads").catch((err) => {
      console.error("cancel_resource_downloads failed", err);
    });
    onBack?.();
  }, [onBack]);

  const handleDone = useCallback(() => {
    onBack?.();
  }, [onBack]);

  const runtimeReady = runtimeStatus?.installed ?? false;
  const assetsReady = assetStatus?.installed ?? false;
  const resourceBusy = runtimeBusy || assetsBusy;

  return (
    <Flex direction="column" gap="4" className={embedded ? "resource-panel-embedded" : undefined}>
      {!embedded && (
        <Flex align="start" justify="between" gap="3">
          <Box>
            <Text size="6" weight="bold">
              资源管理
            </Text>
          </Box>
          {mode === "manage" && onBack && (
            <Flex align="center" gap="2">
              <Button type="button" variant="soft" color="gray" onClick={handleCancel}>
                取消
              </Button>
              <Button type="button" onClick={handleDone} disabled={resourceBusy}>
                完成
              </Button>
            </Flex>
          )}
        </Flex>
      )}
      {checking && (
        <Flex align="center" gap="2">
          <Spinner size="1" />
          <Text size="2" color="gray">
            正在检查本地安装状态…
          </Text>
        </Flex>
      )}

      {error && (
        <Text size="2" color="red">
          检查失败：{error}
        </Text>
      )}

      <Box className={`setup-requirement ${runtimeReady ? "ready" : "missing"}`}>
        <Text size="3" weight="bold">
          CV 运行时
        </Text>
        <Text size="2" color="gray">
          {runtimeReady ? "已安装运行时 base 和 code 包" : "需要安装 runtime base 和 code 包"}
        </Text>
        <RuntimeBundleButton
          status={runtimeStatus}
          onInstalled={refresh}
          onBusyChange={setRuntimeBusy}
        />
      </Box>

      <Box className={`setup-requirement ${assetsReady ? "ready" : "missing"}`}>
        <Text size="3" weight="bold">
          素材包
        </Text>
        <Text size="2" color="gray">
          {assetsReady
            ? `已导入从者 ${assetStatus?.servantFiles ?? 0} 个文件，礼装 ${
                assetStatus?.craftEssenceFiles ?? 0
              } 个文件`
            : assetStatus?.updateAvailable && assetStatus.targetVersion != null
              ? `需要更新素材包 v${assetStatus.currentVersion} → v${assetStatus.targetVersion}`
              : "需要下载包含 assets/servants 和 assets/ces 的素材包"}
        </Text>
        <AssetBundleButton
          status={assetStatus}
          onImported={refresh}
          onBusyChange={setAssetsBusy}
        />
      </Box>
    </Flex>
  );
}

export function SetupPage({ mode = "setup", onBack, onReady }: SetupPageProps) {
  return (
    <Flex direction="column" className="setup-page">
      <Box className="setup-panel">
        <ResourceManagementPanel mode={mode} onBack={onBack} onReady={onReady} />
      </Box>
    </Flex>
  );
}
