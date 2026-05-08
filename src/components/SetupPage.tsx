import { useCallback, useEffect, useState } from "react";
import { Box, Flex, Spinner, Text } from "@radix-ui/themes";
import { AssetBundleButton } from "./AssetBundleButton";
import { RuntimeBundleButton } from "./RuntimeBundleButton";
import { invoke } from "../tauri";
import type { AssetBundleStatus } from "../types/assets";
import type { RuntimeStatus } from "../types/runtime";

interface SetupPageProps {
  onReady: () => void;
}

export function SetupPage({ onReady }: SetupPageProps) {
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null);
  const [assetStatus, setAssetStatus] = useState<AssetBundleStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
      if (runtime.installed && assets.installed) {
        onReady();
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setChecking(false);
    }
  }, [onReady]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runtimeReady = runtimeStatus?.installed ?? false;
  const assetsReady = assetStatus?.installed ?? false;

  return (
    <Flex direction="column" className="setup-page">
      <Box className="setup-panel">
        <Flex direction="column" gap="4">
          <Box>
            <Text size="6" weight="bold">
              初始化 mash
            </Text>
            <Text size="2" color="gray" className="setup-subtitle">
              安装 CV 运行时并导入素材包后即可进入主页。
            </Text>
          </Box>

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
            <RuntimeBundleButton onInstalled={refresh} />
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
                : "需要导入包含 assets/servants 和 assets/ces 的素材包"}
            </Text>
            <AssetBundleButton onImported={refresh} />
          </Box>
        </Flex>
      </Box>
    </Flex>
  );
}
