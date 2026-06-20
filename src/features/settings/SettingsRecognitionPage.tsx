import { useCallback, useEffect, useMemo, useState } from "react";
import { Box, Button, Flex, Text, TextField } from "@radix-ui/themes";
import { invoke } from "../../tauri";

const SUPPORT_CE_THRESHOLD_DEFAULT = 0.7;
const SUPPORT_CE_THRESHOLD_MIN = 0.6;
const SUPPORT_CE_THRESHOLD_MAX = 0.85;
const SUPPORT_CE_THRESHOLD_STEP = 0.01;

interface RecognitionSettings {
  supportCeThreshold: number;
}

function clampThreshold(value: number) {
  if (!Number.isFinite(value)) return SUPPORT_CE_THRESHOLD_DEFAULT;
  return Math.min(SUPPORT_CE_THRESHOLD_MAX, Math.max(SUPPORT_CE_THRESHOLD_MIN, value));
}

function formatThreshold(value: number) {
  return clampThreshold(value).toFixed(2);
}

export function SettingsRecognitionPage({ active }: { active: boolean }) {
  const [saved, setSaved] = useState(SUPPORT_CE_THRESHOLD_DEFAULT);
  const [draft, setDraft] = useState(formatThreshold(SUPPORT_CE_THRESHOLD_DEFAULT));
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  const draftNumber = useMemo(() => Number.parseFloat(draft), [draft]);
  const draftValid =
    Number.isFinite(draftNumber) &&
    draftNumber >= SUPPORT_CE_THRESHOLD_MIN &&
    draftNumber <= SUPPORT_CE_THRESHOLD_MAX;
  const dirty = draftValid && formatThreshold(draftNumber) !== formatThreshold(saved);

  const loadSettings = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSavedMessage(null);
    try {
      const settings = await invoke<RecognitionSettings>("get_recognition_settings");
      const threshold = clampThreshold(settings.supportCeThreshold);
      setSaved(threshold);
      setDraft(formatThreshold(threshold));
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
    async (value: number) => {
      setSaving(true);
      setError(null);
      setSavedMessage(null);
      try {
        const settings = await invoke<RecognitionSettings>("set_support_ce_threshold", {
          value: clampThreshold(value),
        });
        const threshold = clampThreshold(settings.supportCeThreshold);
        setSaved(threshold);
        setDraft(formatThreshold(threshold));
        setSavedMessage("已保存");
      } catch (err) {
        setError(String(err));
      } finally {
        setSaving(false);
      }
    },
    []
  );

  return (
    <Flex direction="column" gap="4">
      <Box>
        <Text size="2" color="gray" className="setup-subtitle">
          调整助战列表中礼装图片匹配的通过阈值
        </Text>
      </Box>

      <Box className="settings-section-panel">
        <Flex direction="column" gap="4" className="recognition-setting-block">
          <Flex direction="column" gap="1">
            <Text size="2" weight="bold">
              助战礼装匹配阈值
            </Text>
            <Text size="1" color="gray">
              较低更容易命中，较高更不容易误选；默认 0.70。
            </Text>
          </Flex>

          <Flex align="center" gap="3" className="recognition-threshold-row">
            <input
              type="range"
              min={SUPPORT_CE_THRESHOLD_MIN}
              max={SUPPORT_CE_THRESHOLD_MAX}
              step={SUPPORT_CE_THRESHOLD_STEP}
              value={draftValid ? draftNumber : saved}
              onChange={(event) => {
                setDraft(formatThreshold(Number.parseFloat(event.currentTarget.value)));
                setSavedMessage(null);
              }}
              aria-label="助战礼装匹配阈值"
              className="recognition-threshold-range"
              disabled={loading || saving}
            />
            <TextField.Root
              type="number"
              min={SUPPORT_CE_THRESHOLD_MIN}
              max={SUPPORT_CE_THRESHOLD_MAX}
              step={SUPPORT_CE_THRESHOLD_STEP}
              value={draft}
              onChange={(event) => {
                setDraft(event.currentTarget.value);
                setSavedMessage(null);
              }}
              aria-label="助战礼装匹配阈值数值"
              className="recognition-threshold-input"
              disabled={loading || saving}
            />
          </Flex>

          <Flex align="center" justify="between" gap="3" wrap="wrap">
            <Text size="1" color={draftValid ? "gray" : "red"}>
              允许范围 0.60 到 0.85
            </Text>
            <Flex align="center" gap="2">
              {error && (
                <Text size="1" color="red">
                  {error}
                </Text>
              )}
              {savedMessage && !error && (
                <Text size="1" color="green">
                  {savedMessage}
                </Text>
              )}
              <Button
                type="button"
                variant="soft"
                color="gray"
                disabled={loading || saving || formatThreshold(saved) === formatThreshold(SUPPORT_CE_THRESHOLD_DEFAULT)}
                onClick={() => void saveThreshold(SUPPORT_CE_THRESHOLD_DEFAULT)}
              >
                恢复默认
              </Button>
              <Button
                type="button"
                disabled={loading || saving || !dirty}
                onClick={() => void saveThreshold(draftNumber)}
              >
                保存
              </Button>
            </Flex>
          </Flex>
        </Flex>
      </Box>
    </Flex>
  );
}
