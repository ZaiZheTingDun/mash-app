import { useCallback, useEffect, useState } from "react";
import { Box, Flex, Switch, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { DebugSettings } from "../../types/debug";

const DEFAULT_DEBUG_SETTINGS: DebugSettings = {
  autoCaptureBattleResultLoot: false,
};

function normalizeDebugSettings(settings: Partial<DebugSettings>): DebugSettings {
  return {
    autoCaptureBattleResultLoot: settings.autoCaptureBattleResultLoot === true,
  };
}

export function SettingsDebugPage({ active }: { active: boolean }) {
  const [settings, setSettings] = useState<DebugSettings>(DEFAULT_DEBUG_SETTINGS);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  const loadSettings = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(await invoke<DebugSettings>("get_debug_settings"));
      setSettings(next);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (active) {
      void loadSettings();
    }
  }, [active, loadSettings]);

  const saveAutoCaptureBattleResultLoot = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(
        await invoke<DebugSettings>("set_auto_capture_battle_result_loot", { value })
      );
      setSettings(next);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, []);

  return (
    <Box className="settings-section-panel">
      <Flex direction="column" gap="4" className="recognition-setting-block">
        <Flex align="start" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              自动截图战利品页面
            </Text>
            <Text size="1" color="gray">
              开启后自动化运行中每个战利品页面都会保存一张截图，用于排查掉落识别问题。
            </Text>
          </Flex>

          <Switch
            checked={settings.autoCaptureBattleResultLoot}
            onCheckedChange={(value) => void saveAutoCaptureBattleResultLoot(value)}
            disabled={loading || saving}
            aria-label="自动截图战利品页面"
          />
        </Flex>

        <Flex align="center" gap="2">
          {(loading || saving) && (
            <Text size="1" color="gray">
              {saving ? "保存中…" : "加载中…"}
            </Text>
          )}
          {savedMessage && !loading && !saving && (
            <Text size="1" color="green">
              {savedMessage}
            </Text>
          )}
          {error && !loading && !saving && (
            <Text size="1" color="red">
              {error}
            </Text>
          )}
        </Flex>
      </Flex>
    </Box>
  );
}
