import { useCallback, useEffect, useState } from "react";
import { Box, Flex, Select, Switch, Text, Tooltip } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { NoblePhantasmDetectionMode, RecognitionSettings } from "../../types/recognition";
import { normalizeRecognitionSettings } from "./recognitionSettingsModel";

export function SettingsBasicPage({ active }: { active: boolean }) {
  const [mode, setMode] = useState<NoblePhantasmDetectionMode>("card");
  const [stopOnBondLevelUp, setStopOnBondLevelUp] = useState(false);
  const [stopOnBondMaxLevel, setStopOnBondMaxLevel] = useState(false);
  const [verifySkillActivation, setVerifySkillActivation] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  const applySettings = useCallback((settings: RecognitionSettings) => {
    setMode(settings.noblePhantasmDetectionMode);
    setStopOnBondLevelUp(settings.stopOnBondLevelUp);
    setStopOnBondMaxLevel(settings.stopOnBondMaxLevel);
    setVerifySkillActivation(settings.verifySkillActivation);
  }, []);

  const loadSettings = useCallback(async () => {
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("get_recognition_settings")
      );
      applySettings(settings);
    } catch (err) {
      setError(String(err));
    }
  }, [applySettings]);

  useEffect(() => {
    if (active) {
      void loadSettings();
    }
  }, [active, loadSettings]);

  const saveMode = useCallback(async (value: NoblePhantasmDetectionMode) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("set_noble_phantasm_detection_mode", { value })
      );
      applySettings(settings);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [applySettings]);

  const saveBondStopSetting = useCallback(
    async (command: string, value: boolean) => {
      setSaving(true);
      setError(null);
      setSavedMessage(null);
      try {
        const settings = normalizeRecognitionSettings(
          await invoke<RecognitionSettings>(command, { value })
        );
        applySettings(settings);
        setSavedMessage("已保存");
      } catch (err) {
        setError(String(err));
      } finally {
        setSaving(false);
      }
    },
    [applySettings]
  );

  const saveVerifySkillActivation = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("set_verify_skill_activation", { value })
      );
      applySettings(settings);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [applySettings]);

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

        <Flex
          align="start"
          justify="between"
          gap="4"
          wrap="wrap"
          className="basic-setting-row"
        >
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              牵绊升级自动停止
            </Text>
            <Text size="1" color="gray">
              出现牵绊等级提升页面时自动停止
            </Text>
          </Flex>

          {stopOnBondMaxLevel ? (
            <Tooltip content="牵绊满级自动停止已开启；关闭满级开关后可修改此项">
              <Box tabIndex={0}>
                <Switch
                  checked={stopOnBondLevelUp}
                  onCheckedChange={(value) =>
                    void saveBondStopSetting("set_stop_on_bond_level_up", value)
                  }
                  disabled
                  aria-label="牵绊升级自动停止"
                />
              </Box>
            </Tooltip>
          ) : (
            <Box>
              <Switch
                checked={stopOnBondLevelUp}
                onCheckedChange={(value) =>
                  void saveBondStopSetting("set_stop_on_bond_level_up", value)
                }
                disabled={saving}
                aria-label="牵绊升级自动停止"
              />
            </Box>
          )}
        </Flex>

        <Flex
          align="start"
          justify="between"
          gap="4"
          wrap="wrap"
          className="basic-setting-row"
        >
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              牵绊满级自动停止
            </Text>
            <Text size="1" color="gray">
              出现牵绊等级提升页面且等级达到 10 或以上时自动停止；开启后会关闭牵绊升级自动停止
            </Text>
          </Flex>

          <Switch
            checked={stopOnBondMaxLevel}
            onCheckedChange={(value) =>
              void saveBondStopSetting("set_stop_on_bond_max_level", value)
            }
            disabled={saving}
            aria-label="牵绊满级自动停止"
          />
        </Flex>

        <Flex
          align="start"
          justify="between"
          gap="4"
          wrap="wrap"
          className="basic-setting-row"
        >
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              技能使用确认
            </Text>
            <Text size="1" color="gray">
              开启后会确认技能使用成功，失败会进行重试，一般无需开启
            </Text>
          </Flex>

          <Switch
            checked={verifySkillActivation}
            onCheckedChange={(value) => void saveVerifySkillActivation(value)}
            disabled={saving}
            aria-label="技能使用确认"
          />
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
