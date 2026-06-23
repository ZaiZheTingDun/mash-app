import { useCallback, useEffect, useState } from "react";
import { Box, Flex, Select, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { NoblePhantasmDetectionMode, RecognitionSettings } from "../../types/recognition";
import { normalizeRecognitionSettings } from "./recognitionSettingsModel";

export function SettingsBasicPage({ active }: { active: boolean }) {
  const [mode, setMode] = useState<NoblePhantasmDetectionMode>("card");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  const loadMode = useCallback(async () => {
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("get_recognition_settings")
      );
      setMode(settings.noblePhantasmDetectionMode);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    if (active) {
      void loadMode();
    }
  }, [active, loadMode]);

  const saveMode = useCallback(async (value: NoblePhantasmDetectionMode) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("set_noble_phantasm_detection_mode", { value })
      );
      setMode(settings.noblePhantasmDetectionMode);
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
        <Flex
          align="start"
          justify="between"
          gap="4"
          wrap="wrap"
          className="basic-setting-row"
        >
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              宝具识别方式
            </Text>
            <Text size="1" color="gray">
              出现宝具识别问题可尝试切换，仍在实验中可能导致选卡速度变慢
            </Text>
          </Flex>

          <Select.Root
            value={mode}
            onValueChange={(value) => void saveMode(value as NoblePhantasmDetectionMode)}
            disabled={saving}
          >
            <Select.Trigger aria-label="宝具识别方式" className="recognition-mode-select" />
            <Select.Content>
              <Select.Item value="card">宝具指令卡识别</Select.Item>
              <Select.Item value="gauge">底部宝具条识别（实验性）</Select.Item>
            </Select.Content>
          </Select.Root>
        </Flex>

        <Flex align="center" gap="2">
          {saving && (
            <Text size="1" color="gray">
              保存中…
            </Text>
          )}
          {savedMessage && !saving && (
            <Text size="1" color="green">
              {savedMessage}
            </Text>
          )}
          {error && !saving && (
            <Text size="1" color="red">
              {error}
            </Text>
          )}
        </Flex>
      </Flex>
    </Box>
  );
}
