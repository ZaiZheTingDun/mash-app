import { useCallback, useEffect, useMemo, useState } from "react";
import { Box, Button, Flex, Text, TextField } from "@radix-ui/themes";
import { invoke } from "../../tauri";

const SUPPORT_THRESHOLD_DEFAULT = 0.7;
const SUPPORT_THRESHOLD_MIN = 0.6;
const SUPPORT_THRESHOLD_MAX = 0.85;
const SUPPORT_THRESHOLD_STEP = 0.01;

type ThresholdKey =
  | "supportCeThreshold"
  | "supportMlbIconThreshold"
  | "supportBondIconThreshold";

interface RecognitionSettings {
  supportCeThreshold: number;
  supportMlbIconThreshold: number;
  supportBondIconThreshold: number;
}

interface ThresholdConfig {
  key: ThresholdKey;
  label: string;
  description: string;
  ariaLabel: string;
  command: string;
}

const THRESHOLD_CONFIGS: ThresholdConfig[] = [
  {
    key: "supportCeThreshold",
    label: "助战礼装匹配阈值",
    description: "较低更容易命中，较高更不容易误选；默认 0.70。",
    ariaLabel: "助战礼装匹配阈值数值",
    command: "set_support_ce_threshold",
  },
  {
    key: "supportMlbIconThreshold",
    label: "满破图标匹配阈值",
    description: "用于校验助战礼装右侧满破图标；默认 0.70。",
    ariaLabel: "满破图标匹配阈值数值",
    command: "set_support_mlb_icon_threshold",
  },
  {
    key: "supportBondIconThreshold",
    label: "牵绊图标匹配阈值",
    description: "用于校验冠位礼装牵绊 / 冠位连接牵绊图标；默认 0.70。",
    ariaLabel: "牵绊图标匹配阈值数值",
    command: "set_support_bond_icon_threshold",
  },
];

const DEFAULT_SETTINGS: RecognitionSettings = {
  supportCeThreshold: SUPPORT_THRESHOLD_DEFAULT,
  supportMlbIconThreshold: SUPPORT_THRESHOLD_DEFAULT,
  supportBondIconThreshold: SUPPORT_THRESHOLD_DEFAULT,
};

function clampThreshold(value: number) {
  if (!Number.isFinite(value)) return SUPPORT_THRESHOLD_DEFAULT;
  return Math.min(SUPPORT_THRESHOLD_MAX, Math.max(SUPPORT_THRESHOLD_MIN, value));
}

function normalizeSettings(settings: RecognitionSettings): RecognitionSettings {
  return {
    supportCeThreshold: clampThreshold(settings.supportCeThreshold),
    supportMlbIconThreshold: clampThreshold(settings.supportMlbIconThreshold),
    supportBondIconThreshold: clampThreshold(settings.supportBondIconThreshold),
  };
}

function formatThreshold(value: number) {
  return clampThreshold(value).toFixed(2);
}

function settingsToDraft(settings: RecognitionSettings): Record<ThresholdKey, string> {
  return {
    supportCeThreshold: formatThreshold(settings.supportCeThreshold),
    supportMlbIconThreshold: formatThreshold(settings.supportMlbIconThreshold),
    supportBondIconThreshold: formatThreshold(settings.supportBondIconThreshold),
  };
}

export function SettingsRecognitionPage({ active }: { active: boolean }) {
  const [saved, setSaved] = useState<RecognitionSettings>(DEFAULT_SETTINGS);
  const [draft, setDraft] = useState<Record<ThresholdKey, string>>(
    settingsToDraft(DEFAULT_SETTINGS)
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

  const loadSettings = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSavedMessageKey(null);
    try {
      const settings = normalizeSettings(
        await invoke<RecognitionSettings>("get_recognition_settings")
      );
      setSaved(settings);
      setDraft(settingsToDraft(settings));
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

  const saveThreshold = useCallback(
    async (config: ThresholdConfig, value: number) => {
      setSavingKey(config.key);
      setError(null);
      setSavedMessageKey(null);
      try {
        const settings = normalizeSettings(
          await invoke<RecognitionSettings>(config.command, {
            value: clampThreshold(value),
          })
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
    []
  );

  return (
    <Flex direction="column" gap="4">
      <Box>
        <Text size="2" color="gray" className="setup-subtitle">
          调整助战列表中礼装与礼装图标匹配的通过阈值
        </Text>
      </Box>

      <Box className="settings-section-panel">
        <Flex direction="column" gap="5" className="recognition-setting-block">
          {THRESHOLD_CONFIGS.map((config) => {
            const draftNumber = draftNumbers[config.key];
            const draftValid =
              Number.isFinite(draftNumber) &&
              draftNumber >= SUPPORT_THRESHOLD_MIN &&
              draftNumber <= SUPPORT_THRESHOLD_MAX;
            const dirty =
              draftValid &&
              formatThreshold(draftNumber) !== formatThreshold(saved[config.key]);
            const saving = savingKey === config.key;

            return (
              <Flex key={config.key} direction="column" gap="4">
                <Flex direction="column" gap="1">
                  <Text size="2" weight="bold">
                    {config.label}
                  </Text>
                  <Text size="1" color="gray">
                    {config.description}
                  </Text>
                </Flex>

                <Flex align="center" gap="3" className="recognition-threshold-row">
                  <input
                    type="range"
                    min={SUPPORT_THRESHOLD_MIN}
                    max={SUPPORT_THRESHOLD_MAX}
                    step={SUPPORT_THRESHOLD_STEP}
                    value={draftValid ? draftNumber : saved[config.key]}
                    onChange={(event) => {
                      const value = Number.parseFloat(event.currentTarget.value);
                      setDraft((current) => ({
                        ...current,
                        [config.key]: formatThreshold(value),
                      }));
                      setSavedMessageKey(null);
                    }}
                    aria-label={config.label}
                    className="recognition-threshold-range"
                    disabled={loading || savingKey != null}
                  />
                  <TextField.Root
                    type="number"
                    min={SUPPORT_THRESHOLD_MIN}
                    max={SUPPORT_THRESHOLD_MAX}
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
                    允许范围 0.60 到 0.85
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
                        formatThreshold(saved[config.key]) ===
                          formatThreshold(SUPPORT_THRESHOLD_DEFAULT)
                      }
                      onClick={() => void saveThreshold(config, SUPPORT_THRESHOLD_DEFAULT)}
                    >
                      恢复默认
                    </Button>
                    <Button
                      type="button"
                      disabled={loading || savingKey != null || !dirty}
                      onClick={() => void saveThreshold(config, draftNumber)}
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
