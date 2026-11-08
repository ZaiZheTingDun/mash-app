import { useCallback, useEffect, useRef, useState } from "react";
import { Box, Button, Dialog, Flex, Select, Text, TextField } from "@radix-ui/themes";
import { DiamondSwitch } from "../../components/common/DiamondSwitch";
import { SettingsRow } from "./SettingsRow";
import preAttackNpGaugeDialogImage from "../../../src-tauri/resources/images/battle/pre_attack_np_gauge_dialog.png";
import { invoke } from "../../tauri";
import type { NoblePhantasmDetectionMode, RecognitionSettings } from "../../types/recognition";
import {
  normalizeRecognitionSettings,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED,
} from "./recognitionSettingsModel";

const TIMEOUT_COMMAND = "set_unknown_screen_timeout_count";

export function SettingsAdvancedPage({ active }: { active: boolean }) {
  const [mode, setMode] = useState<NoblePhantasmDetectionMode>("card");
  const [preAttackModeDialogOpen, setPreAttackModeDialogOpen] = useState(false);
  const [verifySkillActivation, setVerifySkillActivation] = useState(false);
  const [supportFullListOcrFallback, setSupportFullListOcrFallback] = useState(false);
  const [apRecoveryConfirmWaitMs, setApRecoveryConfirmWaitMs] = useState<number | null>(null);
  const [apRecoveryConfirmWaitDraft, setApRecoveryConfirmWaitDraft] = useState("");
  const apRecoveryWaitSaveSequence = useRef(0);
  const apRecoveryWaitSaveQueue = useRef<Promise<void>>(Promise.resolve());
  const [timeoutCount, setTimeoutCount] = useState(UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT);
  const [timeoutDraft, setTimeoutDraft] = useState(
    String(UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT)
  );
  const [timeoutEnabled, setTimeoutEnabled] = useState(true);
  const timeoutSaveSequence = useRef(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  const applySettings = useCallback((settings: RecognitionSettings) => {
    setMode(settings.noblePhantasmDetectionMode);
    setVerifySkillActivation(settings.verifySkillActivation);
    setSupportFullListOcrFallback(settings.supportFullListOcrFallback);
    setApRecoveryConfirmWaitMs(settings.apRecoveryConfirmWaitMs);
    setApRecoveryConfirmWaitDraft(String(settings.apRecoveryConfirmWaitMs));
    setTimeoutCount(settings.unknownScreenTimeoutCount);
    setTimeoutEnabled(
      settings.unknownScreenTimeoutCount !== UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED
    );
    if (settings.unknownScreenTimeoutCount !== UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED) {
      setTimeoutDraft(String(settings.unknownScreenTimeoutCount));
    }
  }, []);

  const updateApRecoveryConfirmWait = (draft: string, valid: boolean) => {
    setApRecoveryConfirmWaitDraft(draft);
    const sequence = ++apRecoveryWaitSaveSequence.current;
    setError(null);
    setSavedMessage(null);
    if (draft.trim() === "" || !valid) return;

    // Keep writes in input order without blocking typing or replacing a newer draft.
    apRecoveryWaitSaveQueue.current = apRecoveryWaitSaveQueue.current.then(async () => {
      try {
        const settings = await invoke<RecognitionSettings>("set_ap_recovery_confirm_wait_ms", {
          value: Number(draft),
        });
        if (apRecoveryWaitSaveSequence.current !== sequence) return;
        setApRecoveryConfirmWaitMs(settings.apRecoveryConfirmWaitMs);
        setSavedMessage("已自动保存");
      } catch (err) {
        if (apRecoveryWaitSaveSequence.current === sequence) setError(String(err));
      }
    });
  };

  const saveTimeoutCount = useCallback(async (value: number) => {
    const sequence = timeoutSaveSequence.current + 1;
    timeoutSaveSequence.current = sequence;
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>(TIMEOUT_COMMAND, { value })
      );
      if (timeoutSaveSequence.current !== sequence) return;

      const savedValue = settings.unknownScreenTimeoutCount;
      setTimeoutCount(savedValue);
      setTimeoutEnabled(savedValue !== UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED);
      if (savedValue !== UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED) {
        setTimeoutDraft(String(savedValue));
      }
      setSavedMessage("已自动保存");
    } catch (err) {
      if (timeoutSaveSequence.current !== sequence) return;
      setError(String(err));
    }
  }, []);

  const updateTimeoutDraft = useCallback(
    (rawValue: string) => {
      if (!timeoutEnabled) return;

      const parsed = Number(rawValue);
      if (
        rawValue !== "" &&
        Number.isFinite(parsed) &&
        parsed > UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX
      ) {
        setTimeoutDraft(String(UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX));
        void saveTimeoutCount(UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX);
        return;
      }

      setTimeoutDraft(rawValue);
      setSavedMessage(null);
      setError(null);
      if (
        Number.isInteger(parsed) &&
        parsed >= UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN &&
        parsed <= UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX
      ) {
        void saveTimeoutCount(parsed);
      }
    },
    [saveTimeoutCount, timeoutEnabled]
  );

  const normalizeTimeoutDraft = useCallback(() => {
    if (!timeoutEnabled) return;

    const parsed = Number(timeoutDraft);
    if (
      Number.isInteger(parsed) &&
      parsed >= UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN &&
      parsed <= UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX
    ) {
      return;
    }

    const fallback =
      timeoutCount === UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED
        ? UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
        : timeoutCount;
    const normalized = Number.isFinite(parsed)
      ? Math.min(
          UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX,
          Math.max(UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN, Math.round(parsed))
        )
      : fallback;
    setTimeoutDraft(String(normalized));
    void saveTimeoutCount(normalized);
  }, [saveTimeoutCount, timeoutCount, timeoutDraft, timeoutEnabled]);

  const toggleTimeoutLimit = useCallback(
    (enabled: boolean) => {
      setTimeoutEnabled(enabled);
      if (!enabled) {
        void saveTimeoutCount(UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED);
        return;
      }

      const parsed = Number(timeoutDraft);
      const value =
        Number.isInteger(parsed) &&
        parsed >= UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN &&
        parsed <= UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX
          ? parsed
          : UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT;
      setTimeoutDraft(String(value));
      void saveTimeoutCount(value);
    },
    [saveTimeoutCount, timeoutDraft]
  );

  const loadSettings = useCallback(async () => {
    setError(null);
    setSavedMessage(null);
    try {
      applySettings(normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("get_recognition_settings")
      ));
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
      if (settings.noblePhantasmDetectionMode === "gaugeBeforeAttack") {
        setPreAttackModeDialogOpen(true);
      }
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [applySettings]);

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

  const saveSupportFullListOcrFallback = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("set_support_full_list_ocr_fallback", { value })
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
    <Box className="settings-section-panel settings-basic">
      <Flex direction="column" className="recognition-setting-block">
        <SettingsRow label="回复道具等待时长" description="选中行动力恢复道具后，会等待此时长后点击决定">
          <TextField.Root type="number" min={500} max={60000} step={1}
            value={apRecoveryConfirmWaitDraft}
            onChange={(event) => updateApRecoveryConfirmWait(
              event.currentTarget.value, event.currentTarget.validity.valid
            )}
            aria-label="回复道具等待时长" className="recognition-threshold-input"
            disabled={saving || apRecoveryConfirmWaitMs == null} />
          <Text size="2" color="gray">毫秒</Text>
        </SettingsRow>
        <SettingsRow label="画面识别超时" description="连续无法识别达到此次数后停止">
          <TextField.Root type="number" min={UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN} max={UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX}
            step={1} value={timeoutDraft} onChange={(event) => updateTimeoutDraft(event.currentTarget.value)}
            onBlur={normalizeTimeoutDraft} aria-label="识别超时次数" className="recognition-threshold-input"
            disabled={saving || !timeoutEnabled} />
          <Text size="2" color="gray">次</Text>
          <DiamondSwitch checked={timeoutEnabled} onCheckedChange={toggleTimeoutLimit} disabled={saving} aria-label="画面识别超时" />
        </SettingsRow>
        <SettingsRow label="宝具识别" description="控制 Mash 使用的宝具状态识别方式"
          help={mode === "gaugeBeforeAttack" && <Text size="1" color="orange">当前模式宝具条可能被从者台词遮挡，请在设置中关闭台词</Text>}>
          <Select.Root value={mode} onValueChange={(value) => void saveMode(value as NoblePhantasmDetectionMode)} disabled={saving}>
            <Select.Trigger aria-label="宝具识别" className="recognition-mode-select" />
            <Select.Content position="popper" className="settings-select-menu">
              <Select.Item value="card">指令卡识别</Select.Item>
              <Select.Item value="gaugeBeforeAttack">宝具条识别（选卡前）</Select.Item>
              <Select.Item value="gauge">宝具条识别（选卡时）</Select.Item>
            </Select.Content>
          </Select.Root>
        </SettingsRow>
        <SettingsRow label="技能使用确认" description="开启后会确认技能使用成功，失败会进行重试">
          <DiamondSwitch checked={verifySkillActivation} onCheckedChange={(value) => void saveVerifySkillActivation(value)}
            disabled={saving} aria-label="技能使用确认" />
        </SettingsRow>
        <SettingsRow label="使用全列表 OCR" description="助战识别失败时从锚点识别回退到全列表 OCR 识别">
          <DiamondSwitch checked={supportFullListOcrFallback} onCheckedChange={(value) => void saveSupportFullListOcrFallback(value)}
            disabled={saving} aria-label="使用全列表 OCR" />
        </SettingsRow>
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

        <Dialog.Root open={preAttackModeDialogOpen} onOpenChange={setPreAttackModeDialogOpen}>
          <Dialog.Content maxWidth="860px" className="pre-attack-np-dialog">
            <Dialog.Title size="4">请关闭战斗中宝具语音字幕</Dialog.Title>
            <Dialog.Description size="2" color="gray">
              攻击前读取宝具条时，从者语音字幕可能遮挡识别区域。请按照下图关闭相关设置。
            </Dialog.Description>
            <img
              src={preAttackNpGaugeDialogImage}
              alt="关闭战斗中宝具语音字幕的设置示例"
              className="pre-attack-np-dialog-image"
            />
            <Flex justify="end" mt="4">
              <Dialog.Close>
                <Button type="button">知道了</Button>
              </Dialog.Close>
            </Flex>
          </Dialog.Content>
        </Dialog.Root>
      </Flex>
    </Box>
  );
}
