import { useCallback, useEffect, useRef, useState } from "react";
import {
  Box,
  Button,
  Dialog,
  Flex,
  Select,
  Switch,
  Text,
  TextField,
  Tooltip,
} from "@radix-ui/themes";
import preAttackNpGaugeDialogImage from "../../../src-tauri/resources/images/battle/pre_attack_np_gauge_dialog.png";
import { invoke } from "../../tauri";
import {
  DEFAULT_BATTLE_START_PANEL,
  normalizeBattleStartPanel,
  type BattleStartPanel,
  type MysticCodeGender,
} from "../../types/appUiSettings";
import type { NoblePhantasmDetectionMode, RecognitionSettings } from "../../types/recognition";
import {
  normalizeRecognitionSettings,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN,
  UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED,
} from "./recognitionSettingsModel";

const TIMEOUT_COMMAND = "set_unknown_screen_timeout_count";

export function SettingsBasicPage({
  active,
  onBattleStartPanelChange,
  mysticCodeGender = "female",
  onMysticCodeGenderChange,
}: {
  active: boolean;
  onBattleStartPanelChange?: (value: BattleStartPanel) => void;
  mysticCodeGender?: MysticCodeGender;
  onMysticCodeGenderChange?: (value: MysticCodeGender) => void;
}) {
  const [battleStartPanel, setBattleStartPanel] = useState<BattleStartPanel>(
    DEFAULT_BATTLE_START_PANEL
  );
  const [mode, setMode] = useState<NoblePhantasmDetectionMode>("card");
  const [preAttackModeDialogOpen, setPreAttackModeDialogOpen] = useState(false);
  const [stopOnBondLevelUp, setStopOnBondLevelUp] = useState(false);
  const [stopOnBondMaxLevel, setStopOnBondMaxLevel] = useState(false);
  const [autoCaptureBondLevelUp, setAutoCaptureBondLevelUp] = useState(false);
  const [verifySkillActivation, setVerifySkillActivation] = useState(false);
  const [enableExtraClassFilter, setEnableExtraClassFilter] = useState(true);
  const [supportFullListOcrFallback, setSupportFullListOcrFallback] = useState(false);
  const [timeoutCount, setTimeoutCount] = useState(UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT);
  const [timeoutDraft, setTimeoutDraft] = useState(
    String(UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT)
  );
  const [timeoutEnabled, setTimeoutEnabled] = useState(true);
  const timeoutSaveSequence = useRef(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  const [savingGender, setSavingGender] = useState(false);

  const applySettings = useCallback((settings: RecognitionSettings) => {
    setMode(settings.noblePhantasmDetectionMode);
    setStopOnBondLevelUp(settings.stopOnBondLevelUp);
    setStopOnBondMaxLevel(settings.stopOnBondMaxLevel);
    setAutoCaptureBondLevelUp(settings.autoCaptureBondLevelUp);
    setVerifySkillActivation(settings.verifySkillActivation);
    setEnableExtraClassFilter(settings.enableExtraClassFilter);
    setSupportFullListOcrFallback(settings.supportFullListOcrFallback);
    setTimeoutCount(settings.unknownScreenTimeoutCount);
    setTimeoutEnabled(
      settings.unknownScreenTimeoutCount !== UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED
    );
    if (settings.unknownScreenTimeoutCount !== UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED) {
      setTimeoutDraft(String(settings.unknownScreenTimeoutCount));
    }
  }, []);

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
      const [rawSettings, rawBattleStartPanel, rawGender] = await Promise.all([
        invoke<RecognitionSettings>("get_recognition_settings"),
        invoke<BattleStartPanel>("get_battle_start_panel"),
        invoke<MysticCodeGender>("get_mystic_code_gender"),
      ]);
      const settings = normalizeRecognitionSettings(rawSettings);
      applySettings(settings);
      setBattleStartPanel(normalizeBattleStartPanel(rawBattleStartPanel));
      onMysticCodeGenderChange?.(rawGender === "male" ? "male" : "female");
    } catch (err) {
      setError(String(err));
    }
  }, [applySettings, onMysticCodeGenderChange]);

  const saveMysticCodeGender = useCallback(async (value: MysticCodeGender) => {
    setSavingGender(true);
    setError(null);
    try {
      const saved = await invoke<MysticCodeGender>("set_mystic_code_gender", { value });
      onMysticCodeGenderChange?.(saved === "male" ? "male" : "female");
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSavingGender(false);
    }
  }, [onMysticCodeGenderChange]);

  const saveBattleStartPanel = useCallback(
    async (value: BattleStartPanel) => {
      setSaving(true);
      setError(null);
      setSavedMessage(null);
      try {
        const saved = normalizeBattleStartPanel(
          await invoke<BattleStartPanel>("set_battle_start_panel", { value })
        );
        setBattleStartPanel(saved);
        onBattleStartPanelChange?.(saved);
        setSavedMessage("已保存");
      } catch (err) {
        setError(String(err));
      } finally {
        setSaving(false);
      }
    },
    [onBattleStartPanelChange]
  );

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

  const openBondLevelUpScreenshotFolder = useCallback(async () => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      await invoke("open_bond_level_up_screenshot_folder");
      setSavedMessage("已打开截图文件夹");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, []);

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

  const saveEnableExtraClassFilter = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const settings = normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("set_enable_extra_class_filter", { value })
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
              御主礼装显示性别
            </Text>
            <Text size="1" color="gray">
              选择队伍配置中御主礼装图标使用的服装款式
            </Text>
          </Flex>
          <Select.Root
            value={mysticCodeGender ?? "female"}
            onValueChange={(value) => void saveMysticCodeGender(value as MysticCodeGender)}
            disabled={saving || savingGender}
          >
            <Select.Trigger aria-label="御主礼装显示性别" className="recognition-mode-select" />
            <Select.Content>
              <Select.Item value="female">女</Select.Item>
              <Select.Item value="male">男</Select.Item>
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
              开始后展开
            </Text>
            <Text size="1" color="gray">
              点击战斗页面的开始后，自动展开所选底栏面板
            </Text>
          </Flex>

          <Select.Root
            value={battleStartPanel}
            onValueChange={(value) => void saveBattleStartPanel(value as BattleStartPanel)}
            disabled={saving}
          >
            <Select.Trigger aria-label="开始后展开" className="recognition-mode-select" />
            <Select.Content>
              <Select.Item value="operationLog">操作日志</Select.Item>
              <Select.Item value="runStatus">运行状态</Select.Item>
              <Select.Item value="none">不弹出</Select.Item>
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
              宝具识别方式
            </Text>
            <Text size="1" color="gray">
              出现宝具识别问题可尝试切换，仍在实验中可能导致选卡速度变慢
            </Text>
            {mode === "gaugeBeforeAttack" && (
              <Text size="1" color="orange">
                攻击前识别可能被从者台词遮挡，请关闭台词
              </Text>
            )}
          </Flex>

          <Select.Root
            value={mode}
            onValueChange={(value) => void saveMode(value as NoblePhantasmDetectionMode)}
            disabled={saving}
          >
            <Select.Trigger aria-label="宝具识别方式" className="recognition-mode-select" />
            <Select.Content>
              <Select.Item value="card">指令卡识别</Select.Item>
              <Select.Item value="gaugeBeforeAttack">
                宝具条识别（选卡前）
              </Select.Item>
              <Select.Item value="gauge">宝具条识别（选卡时）</Select.Item>
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
              牵绊升级时自动截图
            </Text>
            <Text size="1" color="gray">
              出现牵绊等级提升页面时自动保存当前截图
            </Text>
          </Flex>

          <Flex align="center" gap="2">
            <Button
              size="1"
              variant="soft"
              onClick={() => void openBondLevelUpScreenshotFolder()}
              disabled={saving}
            >
              打开截图文件夹
            </Button>
            <Switch
              checked={autoCaptureBondLevelUp}
              onCheckedChange={(value) =>
                void saveBondStopSetting("set_auto_capture_bond_level_up", value)
              }
              disabled={saving}
              aria-label="牵绊升级时自动截图"
            />
          </Flex>
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
              Extra 职阶筛选
            </Text>
            <Text size="1" color="gray">
              国服助战目标为 Extra 职阶时，长按并选择具体职阶；队伍可以单独覆盖此设置
            </Text>
          </Flex>

          <Switch
            checked={enableExtraClassFilter}
            onCheckedChange={(value) => void saveEnableExtraClassFilter(value)}
            disabled={saving}
            aria-label="全局 Extra 职阶筛选"
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
              使用全列表 OCR
            </Text>
            <Text size="1" color="gray">
              默认使用锚点进行识别，开启后会在识别失败时回退到全列表 OCR 识别，可能会降低识别速度
            </Text>
          </Flex>

          <Switch
            checked={supportFullListOcrFallback}
            onCheckedChange={(value) => void saveSupportFullListOcrFallback(value)}
            disabled={saving}
            aria-label="使用全列表 OCR"
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

        <Flex
          align="start"
          justify="between"
          gap="4"
          wrap="wrap"
          className="basic-setting-row"
        >
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              识别超时限制
            </Text>
            <Text size="1" color="gray">
              连续无法识别达到此次数后停止
            </Text>
          </Flex>

          <Flex align="center" gap="2">
            <TextField.Root
              type="number"
              min={UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN}
              max={UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX}
              step={1}
              value={timeoutDraft}
              onChange={(event) => updateTimeoutDraft(event.currentTarget.value)}
              onBlur={normalizeTimeoutDraft}
              aria-label="识别超时次数"
              className="recognition-threshold-input"
              disabled={saving || !timeoutEnabled}
            />
            <Text size="2" color="gray">
              次
            </Text>
            <Switch
              checked={timeoutEnabled}
              onCheckedChange={toggleTimeoutLimit}
              disabled={saving}
              aria-label="识别超时限制"
            />
          </Flex>
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
