import { useCallback, useEffect, useRef, useState } from "react";
import { Box, Button, Flex, Text, TextField } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import { ResetIcon } from "@radix-ui/react-icons";
import { FooterNote } from "../../components/common/FooterNote";
import type { RecognitionSettings } from "../../types/recognition";
import { SettingsRow } from "./SettingsRow";
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
  const [loading, setLoading] = useState(true);
  const [saveStatus, setSaveStatus] = useState<Partial<Record<ThresholdKey, "saving" | "saved">>>({});
  const [errors, setErrors] = useState<Partial<Record<ThresholdKey, string>>>({});
  const [loadError, setLoadError] = useState<string | null>(null);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());
  const editSequences = useRef<Partial<Record<ThresholdKey, number>>>({});
  const callbacks = useRef({ loadSettings, loadDefaultSettings, saveThreshold });

  useEffect(() => {
    callbacks.current = { loadSettings, loadDefaultSettings, saveThreshold };
  }, [loadSettings, loadDefaultSettings, saveThreshold]);

  const loadCurrentSettings = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    setSaveStatus({});
    setErrors({});
    try {
      const [settings, defaultSettings] = await Promise.all([
        callbacks.current.loadSettings(),
        callbacks.current.loadDefaultSettings?.() ?? Promise.resolve(DEFAULT_RECOGNITION_SETTINGS),
      ]);
      setDefaults(normalizeRecognitionSettings(defaultSettings));
      const normalizedSettings = normalizeRecognitionSettings(settings);
      setSaved(normalizedSettings);
      setDraft(settingsToDraft(normalizedSettings));
    } catch (err) {
      setLoadError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (active) {
      void loadCurrentSettings();
    }
  }, [active, loadCurrentSettings]);

  const saveCurrentThreshold = useCallback(
    (config: ThresholdConfig, value: number, sequence: number, options?: { restoreDefault?: boolean }) => {
      setSaveStatus((current) => ({ ...current, [config.key]: "saving" }));
      setErrors((current) => ({ ...current, [config.key]: undefined }));
      // Preserve write order across fields without blocking further edits.
      saveQueue.current = saveQueue.current.then(async () => {
        try {
          const settings = normalizeRecognitionSettings(
            await callbacks.current.saveThreshold(config, clampThreshold(value, config), options)
          );
          setSaved((current) => ({ ...current, [config.key]: settings[config.key] }));
          if (editSequences.current[config.key] !== sequence) return;
          if (options?.restoreDefault) {
            setDraft((current) => ({ ...current, [config.key]: formatThreshold(settings[config.key], config) }));
          }
          setSaveStatus((current) => ({ ...current, [config.key]: "saved" }));
        } catch (err) {
          if (editSequences.current[config.key] !== sequence) return;
          setSaveStatus((current) => ({ ...current, [config.key]: undefined }));
          setErrors((current) => ({ ...current, [config.key]: String(err) }));
        }
      });
    },
    []
  );

  const updateDraft = (config: ThresholdConfig, value: string) => {
    const sequence = (editSequences.current[config.key] ?? 0) + 1;
    editSequences.current[config.key] = sequence;
    setDraft((current) => ({ ...current, [config.key]: value }));
    setSaveStatus((current) => ({ ...current, [config.key]: undefined }));
    setErrors((current) => ({ ...current, [config.key]: undefined }));
    const parsed = Number(value);
    if (value.trim() !== "" && Number.isFinite(parsed) && parsed >= config.min && parsed <= config.max) {
      saveCurrentThreshold(config, parsed, sequence);
    }
  };

  const restoreDefault = (config: ThresholdConfig, value: number) => {
    const sequence = (editSequences.current[config.key] ?? 0) + 1;
    editSequences.current[config.key] = sequence;
    setDraft((current) => ({ ...current, [config.key]: formatThreshold(value, config) }));
    saveCurrentThreshold(config, value, sequence, { restoreDefault: true });
  };

  return (
    <Flex direction="column" gap="4" className="settings-with-footer">
      <Box className="settings-section-panel settings-basic">
        <Flex direction="column" gap="5" className="recognition-setting-block">
          {loadError && <Text size="1" color="red" role="alert">{loadError}</Text>}
          {THRESHOLD_CONFIGS.map((config) => {
            const draftNumber = Number(draft[config.key]);
            const draftValid =
              draft[config.key].trim() !== "" && Number.isFinite(draftNumber) &&
              draftNumber >= config.min &&
              draftNumber <= config.max;
            const defaultValue = defaults[config.key];

            return (
              <SettingsRow key={config.key} label={config.label}
                description={config.description(defaultValue)}
                help={<>
                  <Text size="1" color={draftValid ? "gray" : "red"}>
                    允许范围 {config.min.toFixed(2)} 到 {config.max.toFixed(2)}
                  </Text>
                  {errors[config.key] && <Text size="1" color="red" role="alert">{errors[config.key]}</Text>}
                  {saveStatus[config.key] === "saving" && <Text size="1" color="gray">保存中…</Text>}
                  {saveStatus[config.key] === "saved" && <Text size="1" color="green">已保存</Text>}
                </>}>
                <Flex align="center" gap="3">
                  <Button type="button" size="1" variant="ghost" className="settings-reset-action"
                    disabled={loading || (draftValid && draftNumber === defaultValue && saved[config.key] === defaultValue)}
                    onClick={() => restoreDefault(config, defaultValue)}>
                    <ResetIcon />恢复默认
                  </Button>
                  <TextField.Root
                    type="number"
                    min={config.min}
                    max={config.max}
                    step={SUPPORT_THRESHOLD_STEP}
                    value={draft[config.key]}
                    onChange={(event) => updateDraft(config, event.currentTarget.value)}
                    aria-label={config.ariaLabel}
                    className="recognition-threshold-input"
                    disabled={loading}
                  />
                </Flex>
              </SettingsRow>
            );
          })}
        </Flex>
      </Box>
      {helpText && <FooterNote className="settings-footer-note">{helpText}</FooterNote>}
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
