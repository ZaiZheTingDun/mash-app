import { useCallback, useEffect, useState } from "react";
import { Box, Flex, Switch, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { DebugSettings } from "../../types/debug";

const DEFAULT_DEBUG_SETTINGS: DebugSettings = {
  autoCaptureBattleBeforeAttack: false,
  autoCaptureBattleResultLoot: false,
  autoCaptureUnknownScreenTimeout: false,
  autoCaptureSkillUseProbe: false,
  autoCaptureUnrecognizedCriticalChance: false,
  simulateStuckAttackSelection: false,
};

function normalizeDebugSettings(settings: Partial<DebugSettings>): DebugSettings {
  return {
    autoCaptureBattleBeforeAttack: settings.autoCaptureBattleBeforeAttack === true,
    autoCaptureBattleResultLoot: settings.autoCaptureBattleResultLoot === true,
    autoCaptureUnknownScreenTimeout: settings.autoCaptureUnknownScreenTimeout === true,
    autoCaptureSkillUseProbe: settings.autoCaptureSkillUseProbe === true,
    autoCaptureUnrecognizedCriticalChance:
      settings.autoCaptureUnrecognizedCriticalChance === true,
    simulateStuckAttackSelection: settings.simulateStuckAttackSelection === true,
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

  const saveAutoCaptureBattleBeforeAttack = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(
        await invoke<DebugSettings>("set_auto_capture_battle_before_attack", { value })
      );
      setSettings(next);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, []);

  const saveAutoCaptureUnknownScreenTimeout = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(
        await invoke<DebugSettings>("set_auto_capture_unknown_screen_timeout", { value })
      );
      setSettings(next);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, []);

  const saveAutoCaptureSkillUseProbe = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(
        await invoke<DebugSettings>("set_auto_capture_skill_use_probe", { value })
      );
      setSettings(next);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, []);

  const saveAutoCaptureUnrecognizedCriticalChance = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(
        await invoke<DebugSettings>("set_auto_capture_unrecognized_critical_chance", { value })
      );
      setSettings(next);
      setSavedMessage("已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, []);

  const saveSimulateStuckAttackSelection = useCallback(async (value: boolean) => {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      const next = normalizeDebugSettings(
        await invoke<DebugSettings>("set_simulate_stuck_attack_selection", { value })
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
              点击攻击前自动截图
            </Text>
            <Text size="1" color="gray">
              开启后每次在战斗页面点击攻击按钮前都会保存一张无损 PNG，用于数字识别数据收集。
            </Text>
          </Flex>

          <Switch
            checked={settings.autoCaptureBattleBeforeAttack}
            onCheckedChange={(value) => void saveAutoCaptureBattleBeforeAttack(value)}
            disabled={loading || saving}
            aria-label="点击攻击前自动截图"
          />
        </Flex>

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

        <Flex align="start" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              无法识别画面超时时截图
            </Text>
            <Text size="1" color="gray">
              开启后自动化因无法识别当前画面超时停止时，会保存当前画面截图用于排查模板或弹窗问题。
            </Text>
          </Flex>

          <Switch
            checked={settings.autoCaptureUnknownScreenTimeout}
            onCheckedChange={(value) => void saveAutoCaptureUnknownScreenTimeout(value)}
            disabled={loading || saving}
            aria-label="无法识别画面超时时截图"
          />
        </Flex>

        <Flex align="start" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              保存技能确认 probe 截图
            </Text>
            <Text size="1" color="gray">
              开启后技能确认弹窗识别会保存实际送进 sidecar 的那一帧，用于排查误判；关闭后不保存。
            </Text>
          </Flex>

          <Switch
            checked={settings.autoCaptureSkillUseProbe}
            onCheckedChange={(value) => void saveAutoCaptureSkillUseProbe(value)}
            disabled={loading || saving}
            aria-label="保存技能确认 probe 截图"
          />
        </Flex>

        <Flex align="start" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              暴击率无法识别时截图
            </Text>
            <Text size="1" color="gray">
              开启后暴击模式中任意指令卡的暴击率无法识别时，会保存当前指令卡画面用于排查。
            </Text>
          </Flex>

          <Switch
            checked={settings.autoCaptureUnrecognizedCriticalChance}
            onCheckedChange={(value) => void saveAutoCaptureUnrecognizedCriticalChance(value)}
            disabled={loading || saving}
            aria-label="暴击率无法识别时截图"
          />
        </Flex>

        <Flex align="start" justify="between" gap="4" wrap="wrap" className="basic-setting-row">
          <Flex direction="column" gap="1" className="basic-setting-copy">
            <Text size="2" weight="bold">
              测试选卡卡住恢复
            </Text>
            <Text size="1" color="gray">
              开启后会在下一次自动选卡时故意跳过第 3 张卡的点击，用于验证自动返回并重新选卡；触发后自动关闭。
            </Text>
          </Flex>

          <Switch
            checked={settings.simulateStuckAttackSelection}
            onCheckedChange={(value) => void saveSimulateStuckAttackSelection(value)}
            disabled={loading || saving}
            aria-label="测试选卡卡住恢复"
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
