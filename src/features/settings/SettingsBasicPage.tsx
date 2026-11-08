import { useCallback, useEffect, useState } from "react";
import {
  Box,
  Button,
  Flex,
  Heading,
  Select,
  Text,
  Tooltip,
} from "@radix-ui/themes";
import { DiamondSwitch } from "../../components/common/DiamondSwitch";
import { SettingsRow } from "./SettingsRow";
import { TouchControlSetting } from "./TouchControlSetting";
import { invoke } from "../../tauri";
import {
  DEFAULT_BATTLE_START_PANEL,
  normalizeBattleStartPanel,
  type BattleStartPanel,
  type SkillEditorLayout,
  type MysticCodeGender,
} from "../../types/appUiSettings";
import type { RecognitionSettings } from "../../types/recognition";
import { featureToggles } from "../../featureToggles";
import { isAppColorTheme, type AppColorTheme } from "../../types/theme";
import { normalizeRecognitionSettings } from "./recognitionSettingsModel";

export function SettingsBasicPage({
  active,
  onBattleStartPanelChange,
  onSkillEditorLayoutChange,
  mysticCodeGender = "female",
  onMysticCodeGenderChange,
  colorTheme,
  colorThemeError,
  onColorThemeReload,
  onColorThemeChange,
}: {
  active: boolean;
  onSkillEditorLayoutChange?: (value: SkillEditorLayout) => void;
  onBattleStartPanelChange?: (value: BattleStartPanel) => void;
  mysticCodeGender?: MysticCodeGender;
  onMysticCodeGenderChange?: (value: MysticCodeGender) => void;
  colorTheme?: AppColorTheme;
  colorThemeError?: string;
  onColorThemeReload?: () => Promise<void>;
  onColorThemeChange?: (value: AppColorTheme) => Promise<void>;
}) {
  const [skillEditorLayout, setSkillEditorLayout] = useState<SkillEditorLayout>("guided");
  const [battleStartPanel, setBattleStartPanel] = useState<BattleStartPanel>(
    DEFAULT_BATTLE_START_PANEL
  );
  const [stopOnBondLevelUp, setStopOnBondLevelUp] = useState(false);
  const [stopOnBondMaxLevel, setStopOnBondMaxLevel] = useState(false);
  const [autoCaptureBondLevelUp, setAutoCaptureBondLevelUp] = useState(false);
  const [autoFriendRequest, setAutoFriendRequest] = useState(false);
  const [enableExtraClassFilter, setEnableExtraClassFilter] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  const [savingGender, setSavingGender] = useState(false);
  const [savingColorTheme, setSavingColorTheme] = useState(false);

  const saveColorTheme = async (value: string) => {
    if (!isAppColorTheme(value) || !onColorThemeChange) return;
    setSavingColorTheme(true);
    setError(null);
    setSavedMessage(null);
    try {
      await onColorThemeChange(value);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSavingColorTheme(false);
    }
  };

  const applySettings = useCallback((settings: RecognitionSettings) => {
    setStopOnBondLevelUp(settings.stopOnBondLevelUp);
    setStopOnBondMaxLevel(settings.stopOnBondMaxLevel);
    setAutoCaptureBondLevelUp(settings.autoCaptureBondLevelUp);
    setAutoFriendRequest(settings.autoFriendRequest);
    setEnableExtraClassFilter(settings.enableExtraClassFilter);
  }, []);

  const loadSettings = useCallback(async () => {
    setError(null);
    setSavedMessage(null);
    try {
      const [rawSettings, rawBattleStartPanel, rawGender, rawLayout] = await Promise.all([
        invoke<RecognitionSettings>("get_recognition_settings"),
        invoke<BattleStartPanel>("get_battle_start_panel"),
        invoke<MysticCodeGender>("get_mystic_code_gender"),
        invoke<SkillEditorLayout>("get_skill_editor_layout"),
      ]);
      const settings = normalizeRecognitionSettings(rawSettings);
      applySettings(settings);
      setBattleStartPanel(normalizeBattleStartPanel(rawBattleStartPanel));
      if (rawLayout === "guided" || rawLayout === "simple") setSkillEditorLayout(rawLayout);
      onMysticCodeGenderChange?.(rawGender === "male" ? "male" : "female");
    } catch (err) {
      setError(String(err));
    }
  }, [applySettings, onMysticCodeGenderChange]);

  const saveSkillEditorLayout = async (value: SkillEditorLayout) => {
    setSaving(true); setError(null); setSavedMessage(null);
    try {
      const saved = await invoke<SkillEditorLayout>("set_skill_editor_layout", { value });
      setSkillEditorLayout(saved);
      onSkillEditorLayoutChange?.(saved);
      setSavedMessage("已保存");
    } catch (reason) { setError(String(reason)); }
    finally { setSaving(false); }
  };

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

  const saveAutoFriendRequest = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    try {
      applySettings(normalizeRecognitionSettings(
        await invoke<RecognitionSettings>("set_auto_friend_request", { value })
      ));
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
        <section className="settings-basic-group" aria-label="外观">
          <Heading as="h3" size="3" className="settings-basic-group-title">外观</Heading>
          <SettingsRow label="主题" description="选择界面主题颜色"
            help={colorThemeError && <Text size="1" color="red" role="alert">{colorThemeError}</Text>}>
            <Select.Root value={colorTheme ?? ""}
              onValueChange={(value) => void saveColorTheme(value)}
              disabled={savingColorTheme || !colorTheme || !onColorThemeChange}>
              <Select.Trigger aria-label="主题" className="recognition-mode-select"
                placeholder={colorThemeError ? "读取失败" : "正在读取…"} />
              <Select.Content position="popper" className="settings-select-menu">
                <Select.Item value="blue">蓝色</Select.Item>
                <Select.Item value="violet">紫色</Select.Item>
                <Select.Item value="gold">香槟金</Select.Item>
              </Select.Content>
            </Select.Root>
            {colorThemeError && onColorThemeReload && (
              <Button size="1" variant="ghost" onClick={() => void onColorThemeReload()}>重试</Button>
            )}
          </SettingsRow>
          <SettingsRow label="技能设置" description="可切换经典模式或简易模式（Chaldea 模式）">
            <Select.Root value={skillEditorLayout} onValueChange={value => void saveSkillEditorLayout(value as SkillEditorLayout)} disabled={saving}>
              <Select.Trigger aria-label="技能设置" className="recognition-mode-select" />
              <Select.Content position="popper" className="settings-select-menu">
                <Select.Item value="guided">经典模式</Select.Item>
                <Select.Item value="simple">简易模式（Chaldea 模式）</Select.Item>
              </Select.Content>
            </Select.Root>
          </SettingsRow>
          <SettingsRow label="御主性别" description="影响御主显示以及御主礼装识别">
            <Select.Root value={mysticCodeGender ?? "female"}
              onValueChange={(value) => void saveMysticCodeGender(value as MysticCodeGender)} disabled={saving || savingGender}>
              <Select.Trigger aria-label="御主性别" className="recognition-mode-select" />
              <Select.Content position="popper" className="settings-select-menu">
                <Select.Item value="female">女</Select.Item>
                <Select.Item value="male">男</Select.Item>
              </Select.Content>
            </Select.Root>
          </SettingsRow>
        </section>
        <section className="settings-basic-group" aria-label="运行">
          <Heading as="h3" size="3" className="settings-basic-group-title">运行</Heading>
          <TouchControlSetting active={active} />
          <SettingsRow label="自动展开" description="控制开始战斗后自动展开面板">
            <Select.Root value={battleStartPanel}
              onValueChange={(value) => void saveBattleStartPanel(value as BattleStartPanel)} disabled={saving}>
              <Select.Trigger aria-label="自动展开" className="recognition-mode-select" />
              <Select.Content position="popper" className="settings-select-menu">
                <Select.Item value="operationLog">操作日志</Select.Item>
                <Select.Item value="runStatus">运行状态</Select.Item>
                <Select.Item value="none">不弹出</Select.Item>
              </Select.Content>
            </Select.Root>
          </SettingsRow>
          <SettingsRow label="额外职阶筛选" description={<><strong>此为国服特有功能</strong>，助战为额外职阶时，可长按并选择具体职阶</>}>
            <DiamondSwitch checked={enableExtraClassFilter} onCheckedChange={(value) => void saveEnableExtraClassFilter(value)}
              disabled={saving} aria-label="额外职阶筛选" />
          </SettingsRow>
          {featureToggles.autoFriendRequest && (
            <SettingsRow label="自动申请好友" description="结算页的申请好友按钮处于激活状态时自动申请好友">
              <DiamondSwitch checked={autoFriendRequest} onCheckedChange={(value) => void saveAutoFriendRequest(value)}
                disabled={saving} aria-label="自动申请好友" />
            </SettingsRow>
          )}
        </section>
        <section className="settings-basic-group" aria-label="牵绊">
          <Heading as="h3" size="3" className="settings-basic-group-title">牵绊</Heading>
          <SettingsRow label="自动截图" description="牵绊升级后自动截图并保存">
            <Button size="1" variant="ghost" className="settings-folder-action"
              onClick={() => void openBondLevelUpScreenshotFolder()} disabled={saving}>打开截图文件夹 <span aria-hidden="true">↗</span></Button>
            <DiamondSwitch checked={autoCaptureBondLevelUp} disabled={saving} aria-label="自动截图"
              onCheckedChange={(value) => void saveBondStopSetting("set_auto_capture_bond_level_up", value)} />
          </SettingsRow>
          <SettingsRow label="升级自动停止" description="任意牵绊升级后自动停止">
            {stopOnBondMaxLevel ? (
              <Tooltip content="牵绊满级自动停止已开启；关闭满级开关后可修改此项">
                <Box tabIndex={0}>
                  <DiamondSwitch checked={stopOnBondLevelUp} disabled aria-label="升级自动停止"
                    onCheckedChange={(value) => void saveBondStopSetting("set_stop_on_bond_level_up", value)} />
                </Box>
              </Tooltip>
            ) : <DiamondSwitch checked={stopOnBondLevelUp} disabled={saving} aria-label="升级自动停止"
              onCheckedChange={(value) => void saveBondStopSetting("set_stop_on_bond_level_up", value)} />}
          </SettingsRow>
          <SettingsRow label="满级自动停止" description="任意牵绊满级后自动停止">
            <DiamondSwitch checked={stopOnBondMaxLevel} disabled={saving} aria-label="满级自动停止"
              onCheckedChange={(value) => void saveBondStopSetting("set_stop_on_bond_max_level", value)} />
          </SettingsRow>
        </section>
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
