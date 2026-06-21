import { useCallback, useEffect, useMemo, useState } from "react";
import { Box, Button, Flex, Text, TextField } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { RecognitionSettings } from "../../types/recognition";
import {
  clampThreshold,
  DEFAULT_RECOGNITION_SETTINGS,
  formatThreshold,
  normalizeRecognitionSettings,
  settingsToDraft,
  SUPPORT_THRESHOLD_STEP,
  THRESHOLD_CONFIGS,
  type ThresholdConfig,
  type ThresholdKey,
} from "./recognitionSettingsModel";

interface RecognitionThresholdSettingsProps {
  active: boolean;
  helpText?: string;
  loadSettings: () => Promise<RecognitionSettings>;
  loadDefaultSettings?: () => Promise<RecognitionSettings>;
  saveThreshold: (
    config: Pick<ThresholdConfig, "key" | "command">,
    value: number,
    options?: { restoreDefault?: boolean }
  ) => Promise<RecognitionSettings>;
}

export function RecognitionThresholdSettings({
  active,
  helpText,
  loadSettings,
  loadDefaultSettings,
  saveThreshold,
}: RecognitionThresholdSettingsProps) {
  const [saved, setSaved] = useState<RecognitionSettings>(DEFAULT_RECOGNITION_SETTINGS);
  const [defaults, setDefaults] = useState<RecognitionSettings>(DEFAULT_RECOGNITION_SETTINGS);
  const [draft, setDraft] = useState<Record<ThresholdKey, string>>(
    settingsToDraft(DEFAULT_RECOGNITION_SETTINGS)
  );
  const [loading, setLoading] = useState(false);
  const [savingKey, setSavingKey] = useState<ThresholdKey | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedMessageKey, setSavedMessageKey] = useState<ThresholdKey | null>(null);

  const draftNumbers = useMemo(
    () =>
      Object.fromEntries(
        THRESHOLD_CONFIGS.map((config) => [
          config.key,
          Number.parseFloat(draft[config.key]),
        ])
      ) as Record<ThresholdKey, number>,
    [draft]
  );

  const loadCurrentSettings = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSavedMessageKey(null);
    try {
      const [settings, defaultSettings] = await Promise.all([
        loadSettings(),
        loadDefaultSettings?.() ?? Promise.resolve(DEFAULT_RECOGNITION_SETTINGS),
      ]);
      setDefaults(normalizeRecognitionSettings(defaultSettings));
      const normalizedSettings = normalizeRecognitionSettings(settings);
      setSaved(normalizedSettings);
      setDraft(settingsToDraft(normalizedSettings));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [loadDefaultSettings, loadSettings]);

  useEffect(() => {
    if (active) {
      void loadCurrentSettings();
    }
  }, [active, loadCurrentSettings]);

  const saveCurrentThreshold = useCallback(
    async (config: ThresholdConfig, value: number, options?: { restoreDefault?: boolean }) => {
      setSavingKey(config.key);
      setError(null);
      setSavedMessageKey(null);
      try {
        const settings = normalizeRecognitionSettings(
          await saveThreshold(config, clampThreshold(value, config), options)
        );
        setSaved(settings);
        setDraft(settingsToDraft(settings));
        setSavedMessageKey(config.key);
      } catch (err) {
        setError(String(err));
      } finally {
        setSavingKey(null);
      }
    },
    [saveThreshold]
  );

  return (
    <Flex direction="column" gap="4">
      <Box className="settings-section-panel">
        <Flex direction="column" gap="5" className="recognition-setting-block">
          {helpText && (
            <Text size="2" color="gray" className="recognition-setting-help">
              {helpText}
            </Text>
          )}
          {THRESHOLD_CONFIGS.map((config) => {
            const draftNumber = draftNumbers[config.key];
            const draftValid =
              Number.isFinite(draftNumber) &&
              draftNumber >= config.min &&
              draftNumber <= config.max;
            const dirty =
              draftValid &&
              formatThreshold(draftNumber, config) !== formatThreshold(saved[config.key], config);
            const saving = savingKey === config.key;
            const defaultValue = defaults[config.key];

            return (
              <Flex key={config.key} direction="column" gap="4">
                <Flex direction="column" gap="1">
                  <Text size="2" weight="bold">
                    {config.label}
                  </Text>
                  <Text size="1" color="gray">
                    {config.description(defaultValue)}
                  </Text>
                </Flex>

                <Flex align="center" gap="3" className="recognition-threshold-row">
                  <input
                    type="range"
                    min={config.min}
                    max={config.max}
                    step={SUPPORT_THRESHOLD_STEP}
                    value={draftValid ? draftNumber : saved[config.key]}
                    onChange={(event) => {
                      const value = Number.parseFloat(event.currentTarget.value);
                      setDraft((current) => ({
                        ...current,
                        [config.key]: formatThreshold(value, config),
                      }));
                      setSavedMessageKey(null);
                    }}
                    aria-label={config.label}
                    className="recognition-threshold-range"
                    disabled={loading || savingKey != null}
                  />
                  <TextField.Root
                    type="number"
                    min={config.min}
                    max={config.max}
                    step={SUPPORT_THRESHOLD_STEP}
                    value={draft[config.key]}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      setDraft((current) => ({
                        ...current,
                        [config.key]: value,
                      }));
                      setSavedMessageKey(null);
                    }}
                    aria-label={config.ariaLabel}
                    className="recognition-threshold-input"
                    disabled={loading || savingKey != null}
                  />
                </Flex>

                <Flex align="center" justify="between" gap="3" wrap="wrap">
                  <Text size="1" color={draftValid ? "gray" : "red"}>
                    允许范围 {config.min.toFixed(2)} 到 {config.max.toFixed(2)}
                  </Text>
                  <Flex align="center" gap="2">
                    {error && savingKey == null && (
                      <Text size="1" color="red">
                        {error}
                      </Text>
                    )}
                    {savedMessageKey === config.key && !error && (
                      <Text size="1" color="green">
                        已保存
                      </Text>
                    )}
                    <Button
                      type="button"
                      variant="soft"
                      color="gray"
                      disabled={
                        loading ||
                        savingKey != null ||
                        formatThreshold(saved[config.key], config) ===
                          formatThreshold(defaultValue, config)
                      }
                      onClick={() =>
                        void saveCurrentThreshold(config, defaultValue, { restoreDefault: true })
                      }
                    >
                      恢复默认
                    </Button>
                    <Button
                      type="button"
                      disabled={loading || savingKey != null || !dirty}
                      onClick={() => void saveCurrentThreshold(config, draftNumber)}
                    >
                      {saving ? "保存中…" : "保存"}
                    </Button>
                  </Flex>
                </Flex>
              </Flex>
            );
          })}
        </Flex>
      </Box>
    </Flex>
  );
}

export function SettingsRecognitionPage({ active }: { active: boolean }) {
  const loadSettings = useCallback(
    () => invoke<RecognitionSettings>("get_recognition_settings"),
    []
  );
  const saveThreshold = useCallback(
    (config: Pick<ThresholdConfig, "command">, value: number) =>
      invoke<RecognitionSettings>(config.command, { value }),
    []
  );

  return (
    <RecognitionThresholdSettings
      active={active}
      helpText="此处为全局设置；若只想针对某个队伍进行设置，请前往队伍设置页面。"
      loadSettings={loadSettings}
      saveThreshold={saveThreshold}
    />
  );
}
